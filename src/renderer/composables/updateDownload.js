import { computed, reactive } from 'vue'
import { downloadView } from 'foundation-ui/update'
import { belongsToUpdate, identityOf } from './updateSources.js'

// The update transfer belongs to the Rust command, not to the sheet: closing
// the sheet (or its window) must not stop it, and reopening the window must
// show where it is. The module-scoped store survives the sheet's v-if and is
// seeded from `download_update_status` after a webview reload.
const idle = () => ({
  active: false,
  downloadPhase: '',
  progress: null,
  downloaded: 0,
  total: 0,
  source: '',
  // why the download is not from Mirror酱 although it is the chosen source ('' when it is or was not chosen)
  fallbackReason: '',
  filename: '',
  path: '',
  // the release the file (or the failure) belongs to: its tag and the SHA-256 of its installer
  version: '',
  sha256: '',
  // sources that were left out for that release, and why (the native `skipped` list), and the sources that were tried and failed
  skipped: [],
  failed: [],
  errorCode: '',
  errorDetail: '',
  opened: true,
})

const codeOf = (error) => (error && typeof error === 'object' ? String(error.code || '') : '')
const detailOf = (error) => (error && typeof error === 'object' ? String(error.detail || error.message || '') : String(error || ''))

export function createUpdateDownload({ invoke, listen }) {
  const state = reactive(idle())
  let unlisten = null
  let running = null

  const percent = computed(() => {
    if (state.progress == null) return null
    return Math.round(Math.min(1, Math.max(0, state.progress)) * 100)
  })
  const visible = computed(() => state.active || !!state.path)
  const ready = computed(() => !state.active && !!state.path)

  const apply = (payload) => {
    Object.assign(state, downloadView(payload))
    if (typeof payload?.fallbackReason === 'string') state.fallbackReason = payload.fallbackReason
  }

  /**
   * Read the transfer that may already be running (or finished) after a webview reload or while the sheet was closed.
   * `expected` is the update the sheet is offering now: the native task keeps the LAST download, which can be the installer of an earlier release,
   * and a finished file of another release is not a download of this one (installing it fails, and repeating that never ends).
   */
  const refresh = async (expected) => {
    if (running) return
    try {
      const status = await invoke('download_update_status')
      if (!status || typeof status !== 'object') return
      if (expected && !belongsToUpdate(status, expected)) {
        // the status is about another release: nothing of it is shown for this one
        reconcile(expected)
        return
      }
      const view = downloadView(status)
      const active = status.active === true
      if (!active && !view.downloaded && !state.path) return
      Object.assign(state, view, {
        active,
        filename: status.filename || state.filename,
        path: status.path || state.path,
        version: status.version || state.version,
        sha256: status.sha256 || state.sha256,
        skipped: Array.isArray(status.skipped) ? status.skipped : state.skipped,
        errorCode: active ? '' : state.errorCode,
        errorDetail: active ? '' : (status.error || state.errorDetail),
        fallbackReason: status.fallback_reason || '',
      })
    } catch {
      // no backend (browser preview): the sheet still works for its own start
    }
  }

  /**
   * Drops what the store holds when it belongs to another release than `expected` (a finished file, a "not from Mirror酱" reason, a failure).
   * A transfer that is running is never touched. Returns true when something was dropped.
   */
  const reconcile = (expected) => {
    if (state.active || running) return false
    const holds = !!(state.path || state.fallbackReason || state.errorCode || state.skipped.length || state.sha256 || state.filename)
    if (!holds) return false
    const mine = state.sha256 || state.filename
      ? belongsToUpdate({ sha256: state.sha256, filename: state.filename }, expected)
      // a store that knows no identity (a file the cache check found has none yet) is kept only with its own check: see adoptCached
      : false
    if (mine) return false
    Object.assign(state, idle(), { opened: state.opened })
    return true
  }

  /** A verified installer that was already on disk (the native cache check matched its size and SHA-256 against `expected`). */
  const adoptCached = (path, expected) => {
    const identity = identityOf(expected)
    Object.assign(state, { path, version: identity.version, sha256: identity.sha256, filename: identity.filename || state.filename })
  }

  const start = async ({ url, fallbackUrl, filename, expectedSize, expectedSha256, assetType, targetVersion, onlySource }) => {
    if (running) return { busy: true, ok: false }
    Object.assign(state, idle(), { active: true, opened: false, filename, version: targetVersion || '', sha256: String(expectedSha256 || '').toLowerCase() })
    const task = (async () => {
      try {
        if (unlisten) unlisten()
        unlisten = await listen('download-progress', (event) => apply(event.payload))
        const result = await invoke('download_update', {
          url,
          fallbackUrl: fallbackUrl || null,
          filename,
          expectedSize: expectedSize || null,
          expectedSha256: expectedSha256 || null,
          assetType: assetType || null,
          targetVersion: targetVersion || null,
          onlySource: onlySource || null,
        })
        if (result?.success) {
          state.path = result.file_path || ''
          state.active = false
          state.downloadPhase = 'complete'
          state.progress = 1
          state.fallbackReason = result.fallback_reason || ''
          state.skipped = Array.isArray(result.skipped) ? result.skipped : []
          state.failed = []
          if (result.source) state.source = String(result.source)
          state.errorCode = ''
          state.errorDetail = ''
          return { ok: true, path: state.path, cached: !!result.cached }
        }
        state.active = false
        state.errorCode = 'download_failed'
        return { ok: false, errorCode: state.errorCode }
      } catch (error) {
        state.active = false
        state.errorCode = codeOf(error) || 'download_failed'
        state.errorDetail = detailOf(error)
        state.skipped = Array.isArray(error?.skipped) ? error.skipped : []
        state.failed = Array.isArray(error?.failed) ? error.failed : []
        return { ok: false, errorCode: state.errorCode, error }
      } finally {
        if (unlisten) {
          try { await unlisten() } catch { /* the listener is gone with the view */ }
          unlisten = null
        }
        running = null
      }
    })()
    running = task
    return task
  }

  const discard = () => {
    if (running) return
    Object.assign(state, idle())
  }

  const markOpened = () => { state.opened = true }

  return { state, percent, visible, ready, start, refresh, reconcile, adoptCached, discard, markOpened }
}

let singleton = null

export function useUpdateDownload() {
  if (!singleton) {
    singleton = createUpdateDownload({
      invoke: (...args) => import('@tauri-apps/api/core').then((module) => module.invoke(...args)),
      listen: (...args) => import('@tauri-apps/api/event').then((module) => module.listen(...args)),
    })
  }
  return singleton
}
