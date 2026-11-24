// Which release a downloaded file belongs to, and the sentences about sources that did not carry it.
//
// The update download is owned by the native side and outlives the dialog, so what the dialog finds when it opens (a finished file, a failure,
// a "not from Mirror酱" reason) can belong to an EARLIER release than the one it is now offering. Everything here compares by the release's own
// identity (installer SHA-256, file name), never by "there is a path".
import { displayVersion } from 'foundation-ui/update'

export const SKIP_REASONS = Object.freeze(['not_synced', 'size_mismatch', 'hash_mismatch', 'unreachable'])

/** The words of src-tauri/src/update/sources.rs (SkipReason) that this window has a sentence for; anything else is dropped, never shown raw. */
const knownSkips = (skipped) =>
  (Array.isArray(skipped) ? skipped : []).filter((entry) => entry && typeof entry === 'object' && typeof entry.source === 'string' && SKIP_REASONS.includes(entry.reason))

const lower = (value) => String(value || '').trim().toLowerCase()

/** The identity of the installer an update offers (what `UpdateInfo` of the native side carries). */
export const identityOf = (info) => ({
  version: String(info?.version || ''),
  sha256: lower(info?.download_sha256),
  filename: String(info?.download_name || ''),
})

/**
 * Whether a download (a status read from the native task, or the store after a start) is the installer of `expected`.
 * No expectation (an update without an installer) is not a match: nothing may be installed from it.
 * The SHA-256 decides when both sides have one; otherwise the file name does (a status written before the hash was recorded).
 */
export function belongsToUpdate(download, expected) {
  const wanted = identityOf(expected)
  if (!wanted.sha256 && !wanted.filename) return false
  const have = lower(download?.sha256)
  if (have && wanted.sha256) return have === wanted.sha256
  return !!download?.filename && download.filename === wanted.filename
}

/** The GitHub address of the installer of `info` (its main address or its fallback), or '' when GitHub does not carry it. */
export function githubUrlOf(info) {
  for (const candidate of [info?.download_url, info?.download_fallback_url]) {
    try {
      if (candidate && new URL(candidate).hostname === 'github.com') return candidate
    } catch { /* not an address */ }
  }
  return ''
}

const fill = (line, values) => Object.entries(values).reduce((text, [key, value]) => text.replaceAll(`{${key}}`, value), String(line || ''))

/** One sentence per source that was left out: "CNB 还没同步到 2026.1123.20745.杂鱼". */
export function skipSentences(skipped, version, texts) {
  return knownSkips(skipped)
    .map((entry) => fill(texts?.[entry.reason], { source: entry.source, version: displayVersion(version), listed: displayVersion(entry.listed) }))
    .filter(Boolean)
}

/** The sentence for a download that succeeded after some sources were left out: what was skipped and where the file came from. */
export function skipNote(skipped, version, source, texts) {
  const sentences = skipSentences(skipped, version, texts?.skip)
  if (!sentences.length || !source) return ''
  return fill(texts?.usedInstead, { skipped: sentences.join('；'), source })
}

/**
 * The text of a failed download: the sentence of its code, then why each source was left out. `codes` maps native error codes to sentences.
 * Failures that are no sentence of ours stay the generic one; the native detail is for the log.
 */
export function downloadFailureText({ code, skipped, version }, texts) {
  const base = texts?.codes?.[code] || texts?.codes?.default || ''
  const reasons = skipSentences(skipped, version, texts?.skip).join('；')
  return [base, reasons].filter(Boolean).join(' ')
}

/** `installer_mismatch: ...` is what `install_update` answers when the file on disk is not the installer of the update (and has deleted it). */
export const isInstallerMismatch = (error) => /^\s*installer_mismatch\b/.test(String(error?.message ?? error ?? ''))
