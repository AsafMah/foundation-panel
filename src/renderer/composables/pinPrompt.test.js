import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { DESKTOP_LOCALES } from '../desktop/i18n/merge.js'
import { en } from '../desktop/i18n/en.js'
import { zh } from '../desktop/i18n/zh.js'
import { DEFAULT_PIN_PROMPT, PIN_PROMPTS, createPinPromptStore, normalizePinPrompt, pinPromptOptions, withPinPrompt } from './pinPrompt.js'

const source = (path) => readFileSync(new URL(`../../../${path}`, import.meta.url), 'utf8')
const tables = { en, zh }
for (const code of DESKTOP_LOCALES) if (!tables[code]) tables[code] = (await import(`../desktop/i18n/${code}.js`))[code]

test('the setting is one of the two values the host takes, and anything else is the default', () => {
  assert.deepEqual([...PIN_PROMPTS], ['fullscreen', 'panel'])
  assert.equal(DEFAULT_PIN_PROMPT, 'fullscreen')
  assert.equal(normalizePinPrompt('panel'), 'panel')
  assert.equal(normalizePinPrompt(' panel '), 'panel')
  assert.equal(normalizePinPrompt('fullscreen'), 'fullscreen')
  for (const other of [undefined, null, '', 'PANEL', 'window', true, 0]) assert.equal(normalizePinPrompt(other), 'fullscreen', String(other))
})

test('writing the setting keeps the rest of the host configuration', () => {
  assert.deepEqual(withPinPrompt({ port: 47989, locale: 'de' }, 'panel'), { port: 47989, locale: 'de', pin_prompt: 'panel' })
  assert.deepEqual(withPinPrompt(null, 'panel'), { pin_prompt: 'panel' })
  assert.equal(withPinPrompt({ pin_prompt: 'panel' }, 'nonsense').pin_prompt, 'fullscreen')
})

function fakeHost(initial, { writeStatus = 200, readStatus = 200 } = {}) {
  const seen = []
  let config = { ...initial }
  const fetchImpl = async (url, init) => {
    seen.push({ url, method: init?.method || 'GET', body: init?.body ? JSON.parse(init.body) : null })
    if (init?.method === 'POST') {
      if (writeStatus === 200) config = JSON.parse(init.body)
      return { ok: writeStatus === 200, status: writeStatus, json: async () => ({}) }
    }
    return { ok: readStatus === 200, status: readStatus, json: async () => config }
  }
  return { seen, fetchImpl, config: () => config }
}

test('the row reads the host and saves the whole configuration with the new value', async () => {
  const host = fakeHost({ port: 47989, pin_prompt: 'panel' })
  const store = createPinPromptStore({ proxyUrl: async () => 'http://localhost:48081/', fetchImpl: host.fetchImpl })
  assert.equal(await store.read(), 'panel')
  assert.equal(await store.write('fullscreen'), 'fullscreen')
  assert.deepEqual(host.config(), { port: 47989, pin_prompt: 'fullscreen' })
  assert.equal(host.seen[0].url, 'http://localhost:48081/api/config')
  assert.equal(host.seen.at(-1).method, 'POST')
})

test('a host without the key, an unreadable host and a refused save', async () => {
  const bare = createPinPromptStore({ proxyUrl: async () => 'http://h', fetchImpl: fakeHost({ port: 1 }).fetchImpl })
  assert.equal(await bare.read(), 'fullscreen', 'a host that has never set it')
  const down = createPinPromptStore({ proxyUrl: async () => 'http://h', fetchImpl: fakeHost({}, { readStatus: 500 }).fetchImpl })
  assert.equal(await down.read(), 'fullscreen', 'an unreadable host shows the default')
  const refused = fakeHost({ port: 1 }, { writeStatus: 403 })
  const store = createPinPromptStore({ proxyUrl: async () => 'http://h', fetchImpl: refused.fetchImpl })
  await assert.rejects(store.write('panel'), /403/)
  assert.deepEqual(refused.config(), { port: 1 }, 'nothing was changed')
})

test('the row offers both ways in the language of the panel, in all 20 languages', () => {
  for (const code of DESKTOP_LOCALES) {
    const texts = tables[code].settings
    for (const key of ['pinPrompt', 'pinPromptDesc', 'pinPromptFullscreen', 'pinPromptPanel']) assert.ok(typeof texts[key] === 'string' && texts[key].trim(), `${code}.${key}`)
    const options = pinPromptOptions(texts)
    assert.deepEqual(options.map((o) => o.value), [...PIN_PROMPTS])
    assert.notEqual(options[0].label, options[1].label, code)
    if (!['en', 'en_GB', 'en_US'].includes(code)) assert.notEqual(texts.pinPromptDesc, en.settings.pinPromptDesc, `${code} is translated`)
  }
  assert.match(zh.settings.pinPromptFullscreen, /全屏输入 PIN（默认）/)
  assert.match(zh.settings.pinPromptPanel, /在控制面板中输入/)
})

test('the setting row is in the notification sheet of the panel settings and writes through the host', () => {
  const page = source('src/renderer/components/m3e/PanelOptions.vue')
  assert.match(page, /<MChoiceItem :model-value="pinPrompt"[^>]*data-panel-option="pinPrompt"/)
  assert.match(page, /createPinPromptStore\(/)
  assert.match(page, /pinPromptStore\.write\(value\)/)
})
