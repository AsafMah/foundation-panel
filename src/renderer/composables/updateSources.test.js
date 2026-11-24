import assert from 'node:assert/strict'
import test from 'node:test'
import {
  belongsToUpdate,
  downloadFailureText,
  githubUrlOf,
  identityOf,
  isInstallerMismatch,
  skipNote,
  skipSentences,
} from './updateSources.js'

const NEW = 'v2026.1123.20745.杂鱼'
const skip = {
  not_synced: '{source} 还没同步到 {version}',
  size_mismatch: '{source} 的文件与 {version} 不符（大小不一致）',
  hash_mismatch: '{source} 的文件与 {version} 不符（校验不一致）',
  unreachable: '没有连上 {source}',
}
const info = { version: NEW, download_name: 'Sunshine.v2026.1123.WindowsInstaller.exe', download_sha256: 'AB'.repeat(32), download_size: 42534326 }

test('a download belongs to the update by its SHA-256, in any case', () => {
  assert.equal(belongsToUpdate({ sha256: 'ab'.repeat(32), filename: 'other.exe' }, info), true)
  assert.equal(belongsToUpdate({ sha256: 'cd'.repeat(32), filename: info.download_name }, info), false)
})

test('without a hash on the download the file name decides', () => {
  assert.equal(belongsToUpdate({ filename: info.download_name }, info), true)
  assert.equal(belongsToUpdate({ filename: 'Sunshine.v2026.1122.WindowsInstaller.exe' }, info), false)
  assert.equal(belongsToUpdate({}, info), false)
})

test('an update without an installer owns no download', () => {
  assert.equal(belongsToUpdate({ sha256: 'ab'.repeat(32), filename: 'a.exe' }, { version: NEW }), false)
  assert.equal(belongsToUpdate({ sha256: 'ab'.repeat(32) }, null), false)
})

test('the identity of an update is its tag, lower-case hash and file name', () => {
  assert.deepEqual(identityOf(info), { version: NEW, sha256: 'ab'.repeat(32), filename: info.download_name })
})

test('GitHub is offered only when the update has a GitHub address', () => {
  const github = 'https://github.com/AlkaidLab/foundation-sunshine/releases/download/v1/a.exe'
  const cnb = 'https://cnb.cool/AlkaidLab/foundation-sunshine-release/-/releases/download/v1/a.exe'
  assert.equal(githubUrlOf({ download_url: cnb, download_fallback_url: github }), github)
  assert.equal(githubUrlOf({ download_url: github }), github)
  assert.equal(githubUrlOf({ download_url: cnb }), '')
  assert.equal(githubUrlOf({ download_url: 'https://github.com.evil.example/a.exe' }), '')
  assert.equal(githubUrlOf(null), '')
})

test('every source that was left out gets one sentence naming the release', () => {
  const sentences = skipSentences(
    [
      { source: 'CNB', reason: 'not_synced', listed: 'v2026.1122.165529.杂鱼' },
      { source: 'Mirror酱', reason: 'hash_mismatch' },
      { source: 'X', reason: 'something_new' },
    ],
    NEW,
    skip,
  )
  assert.deepEqual(sentences, ['CNB 还没同步到 2026.1123.20745.杂鱼', 'Mirror酱 的文件与 2026.1123.20745.杂鱼 不符（校验不一致）'])
})

test('a download that went on after a source was left out says where it came from', () => {
  const texts = { skip, usedInstead: '{skipped}，已改用 {source} 下载' }
  assert.equal(skipNote([{ source: 'CNB', reason: 'not_synced' }], NEW, 'GitHub', texts), 'CNB 还没同步到 2026.1123.20745.杂鱼，已改用 GitHub 下载')
  assert.equal(skipNote([], NEW, 'GitHub', texts), '')
  assert.equal(skipNote([{ source: 'CNB', reason: 'not_synced' }], NEW, '', texts), '')
})

test('a failed download says why each source was left out, or only the generic sentence', () => {
  const codes = { no_verified_source: '暂时没有下载源提供 2026.1123.20745.杂鱼。', default: '下载失败' }
  assert.equal(
    downloadFailureText({ code: 'no_verified_source', skipped: [{ source: 'CNB', reason: 'not_synced' }, { source: 'Mirror酱', reason: 'unreachable' }], version: NEW }, { codes, skip }),
    '暂时没有下载源提供 2026.1123.20745.杂鱼。 CNB 还没同步到 2026.1123.20745.杂鱼；没有连上 Mirror酱',
  )
  assert.equal(downloadFailureText({ code: 'weird', skipped: [], version: NEW }, { codes, skip }), '下载失败')
})

test('the native "installer_mismatch" answer is recognised, other failures are not', () => {
  assert.equal(isInstallerMismatch('installer_mismatch: 文件大小不匹配: expected 42534326, got 42528017'), true)
  assert.equal(isInstallerMismatch(new Error('installer_mismatch: x')), true)
  assert.equal(isInstallerMismatch('无法定位安装包: 系统找不到指定的文件'), false)
  assert.equal(isInstallerMismatch(undefined), false)
})
