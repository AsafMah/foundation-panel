// Where a pairing request asks for its PIN: the full-screen entry or the control panel's own pairing page.
//
// One value, the host's configuration key `pin_prompt` (Foundation Sunshine's config.cpp), is shared by the two user interfaces: the Web UI edits it with the other host settings, this panel
// writes it through the same `POST /api/config`, and the panel's native side reads the saved file when a request comes (src-tauri/src/pin_prompt.rs). Nothing is kept in the panel itself.

export const PIN_PROMPT_KEY = 'pin_prompt'
export const PIN_PROMPTS = Object.freeze(['fullscreen', 'panel'])
export const DEFAULT_PIN_PROMPT = 'fullscreen'

/** The setting as the host holds it; a missing value or one that is not a value of the setting is the default (the host does the same). */
export function normalizePinPrompt(value) {
  const text = String(value ?? '').trim()
  return PIN_PROMPTS.includes(text) ? text : DEFAULT_PIN_PROMPT
}

/** The options of the row, in the order they are offered: the default first. `texts` is the `settings` section of the language (desktop/i18n). */
export function pinPromptOptions(texts) {
  return [
    { value: 'fullscreen', label: texts.pinPromptFullscreen },
    { value: 'panel', label: texts.pinPromptPanel },
  ]
}

/** A host configuration with the setting written to it; the rest of the configuration is untouched (the host takes the whole document). */
export const withPinPrompt = (config, value) => ({ ...(config && typeof config === 'object' ? config : {}), [PIN_PROMPT_KEY]: normalizePinPrompt(value) })

/**
 * Reads and writes the setting through the host's HTTP API. `proxyUrl()` gives the address of the host through the panel's proxy (it waits until the proxy is up), `fetchImpl` is the
 * browser's fetch. A failed read leaves the default on show and a failed write is thrown, so the row can say it was not saved.
 */
export function createPinPromptStore({ proxyUrl, fetchImpl = (...args) => globalThis.fetch(...args) }) {
  const url = async (path) => `${String(await proxyUrl()).replace(/\/+$/, '')}${path}`
  const getConfig = async () => {
    const response = await fetchImpl(await url('/api/config'))
    if (!response.ok) throw new Error(`HTTP ${response.status}`)
    const config = await response.json()
    if (!config || typeof config !== 'object') throw new Error('the host sent no configuration')
    return config
  }
  return {
    async read() {
      try { return normalizePinPrompt((await getConfig())[PIN_PROMPT_KEY]) } catch { return DEFAULT_PIN_PROMPT }
    },
    async write(value) {
      const next = normalizePinPrompt(value)
      const response = await fetchImpl(await url('/api/config'), {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(withPinPrompt(await getConfig(), next)),
      })
      if (!response.ok) throw new Error(`HTTP ${response.status}`)
      return next
    },
  }
}
