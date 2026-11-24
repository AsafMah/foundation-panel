import assert from 'node:assert/strict'
import test from 'node:test'
import { createUpdateDownload } from './updateDownload.js'

const deferred = () => {
  let resolve
  let reject
  const promise = new Promise((next, fail) => { resolve = next; reject = fail })
  return { promise, resolve, reject }
}

const harness = ({ status = { active: false } } = {}) => {
  const api = {
    events: [],
    listener: null,
    status,
    download: deferred(),
    async invoke(command, args) {
      api.events.push({ command, args })
      if (command === 'download_update_status') return api.status
      return api.download.promise
    },
    async listen(name, callback) {
      api.listener = { name, callback }
      return () => { api.listener = null }
    },
    progress(payload) { api.listener?.callback({ payload }) },
  }
  return { api, download: createUpdateDownload({ invoke: api.invoke, listen: api.listen }) }
}

const tick = () => new Promise((resolve) => setImmediate(resolve))

test('progress stays in the store when the sheet is not there', async () => {
  const { api, download } = harness()
  const task = download.start({ url: 'https://cnb.example/a.exe', filename: 'a.exe' })
  await tick()
  api.progress({ progress: 42, downloaded: 420, total: 1000, phase: 'downloading', source: 'CNB' })
  assert.equal(download.state.active, true)
  assert.equal(download.percent.value, 42)
  assert.equal(download.state.source, 'CNB')

  api.download.resolve({ success: true, file_path: 'C:/tmp/a.exe' })
  assert.equal((await task).ok, true)
  assert.equal(download.state.active, false)
  assert.equal(download.state.path, 'C:/tmp/a.exe')
  assert.equal(download.ready.value, true)
  assert.equal(download.visible.value, true)
})

test('a second start shares the transfer that is already running', async () => {
  const { api, download } = harness()
  const first = download.start({ url: 'https://cnb.example/a.exe', filename: 'a.exe' })
  await tick()
  const second = await download.start({ url: 'https://cnb.example/a.exe', filename: 'a.exe' })
  assert.equal(second.busy, true)
  const downloads = api.events.filter((event) => event.command === 'download_update')
  assert.equal(downloads.length, 1)
  api.download.resolve({ success: true, file_path: 'C:/tmp/a.exe' })
  await first
})

test('a transfer started before a webview reload is read back from the native task', async () => {
  const { download } = harness({
    status: {
      active: true,
      phase: 'downloading',
      progress: 17,
      downloaded: 170,
      total: 1000,
      source: 'CNB',
      filename: 'a.exe',
      error: null,
    },
  })
  await download.refresh()
  assert.equal(download.state.active, true)
  assert.equal(download.percent.value, 17)
  assert.equal(download.state.filename, 'a.exe')
})

test('a failed transfer keeps its error code for the localized message', async () => {
  const { api, download } = harness()
  const task = download.start({ url: 'https://cnb.example/a.exe', filename: 'a.exe' })
  await tick()
  api.download.reject({ code: 'sources_exhausted' })
  const result = await task
  assert.equal(result.ok, false)
  assert.equal(download.state.errorCode, 'sources_exhausted')
})

// 2026-11-23: the dialog opened for the new release found the finished download of the PREVIOUS release in the native status, showed it as ready, and
// installing it failed with "文件大小不匹配" every time the install was repeated.
const previous = {
  active: false,
  phase: 'complete',
  progress: 100,
  downloaded: 42528017,
  total: 42528017,
  source: 'CNB',
  filename: 'Sunshine.v2026.1122.WindowsInstaller.exe',
  path: 'C:/tmp/Sunshine.v2026.1122.WindowsInstaller.exe',
  error: null,
  fallback_reason: 'network',
  version: 'v2026.1122.165529.杂鱼',
  sha256: 'cd'.repeat(32),
  size: 42528017,
}
const offered = {
  version: 'v2026.1123.20745.杂鱼',
  download_name: 'Sunshine.v2026.1123.WindowsInstaller.exe',
  download_sha256: 'AB'.repeat(32),
  download_size: 42534326,
}

test('the finished download of an earlier release is not shown for the release on offer', async () => {
  const { download } = harness({ status: previous })
  await download.refresh(offered)
  assert.equal(download.state.path, '')
  assert.equal(download.ready.value, false)
  assert.equal(download.state.fallbackReason, '')
})

test('the same status is what the sheet showed before the identity was compared', async () => {
  const { download } = harness({ status: previous })
  await download.refresh()
  assert.equal(download.state.path, previous.path)
})

test('the finished download of the release on offer is read back, whatever the case of its hash', async () => {
  const { download } = harness({
    status: { ...previous, filename: offered.download_name, path: 'C:/tmp/Sunshine.v2026.1123.WindowsInstaller.exe', sha256: offered.download_sha256.toLowerCase(), version: offered.version },
  })
  await download.refresh(offered)
  assert.equal(download.state.path, 'C:/tmp/Sunshine.v2026.1123.WindowsInstaller.exe')
  assert.equal(download.ready.value, true)
})

test('a newer release found while the sheet is open drops the file, the reason and the failure of the earlier one', async () => {
  const { download } = harness({ status: { ...previous, error: 'x' } })
  await download.refresh()
  download.state.errorCode = 'sources_exhausted'
  assert.equal(download.reconcile(offered), true)
  assert.equal(download.state.path, '')
  assert.equal(download.state.errorCode, '')
  assert.equal(download.state.fallbackReason, '')
  // nothing left to drop
  assert.equal(download.reconcile(offered), false)
})

test('a transfer that is running is never dropped by a newer check', async () => {
  const { api, download } = harness()
  const task = download.start({ url: 'https://cnb.example/a.exe', filename: 'a.exe', expectedSha256: 'ab'.repeat(32), targetVersion: 'v1' })
  await tick()
  assert.equal(download.reconcile(offered), false)
  assert.equal(download.state.active, true)
  api.download.resolve({ success: true, file_path: 'C:/tmp/a.exe' })
  await task
})

test('a download keeps the release it was started for and the sources that were left out', async () => {
  const { api, download } = harness()
  const skipped = [{ source: 'CNB', reason: 'not_synced', listed: 'v2026.1122.165529.杂鱼' }]
  const task = download.start({
    url: 'https://github.com/a.exe',
    filename: offered.download_name,
    expectedSha256: offered.download_sha256,
    targetVersion: offered.version,
    onlySource: 'github',
  })
  await tick()
  const call = api.events.find((event) => event.command === 'download_update')
  assert.equal(call.args.targetVersion, offered.version)
  assert.equal(call.args.onlySource, 'github')
  api.download.resolve({ success: true, file_path: 'C:/tmp/x.exe', source: 'GitHub', skipped })
  await task
  assert.equal(download.state.version, offered.version)
  assert.equal(download.state.source, 'GitHub')
  assert.deepEqual(download.state.skipped, skipped)
  assert.equal(download.reconcile(offered), false)
})

test('a failure keeps the sources that were left out and the ones that failed', async () => {
  const { api, download } = harness()
  const task = download.start({ url: 'https://cnb.example/a.exe', filename: 'a.exe', expectedSha256: 'ab'.repeat(32), targetVersion: 'v1' })
  await tick()
  api.download.reject({ code: 'no_verified_source', skipped: [{ source: 'CNB', reason: 'not_synced' }], failed: [{ source: 'GitHub', identity: false }] })
  await task
  assert.equal(download.state.errorCode, 'no_verified_source')
  assert.equal(download.state.skipped.length, 1)
  assert.equal(download.state.failed[0].source, 'GitHub')
})
