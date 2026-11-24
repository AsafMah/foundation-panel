import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { DESKTOP_LOCALES } from '../desktop/i18n/merge.js'
import { en } from '../desktop/i18n/en.js'
import { zh } from '../desktop/i18n/zh.js'
import { ENDED_REASONS, PIN_TEXT_KEYS, endedKey } from './pinFlow.js'

const tables = { en, zh }
for (const code of DESKTOP_LOCALES) {
  if (!tables[code]) tables[code] = (await import(`../desktop/i18n/${code}.js`))[code]
}
const source = (name) => readFileSync(new URL(name, import.meta.url), 'utf8')

test('every text of the PIN card is in all 20 languages of the panel, and nothing else is in the section', () => {
  assert.equal(Object.keys(tables).length, DESKTOP_LOCALES.length)
  for (const code of DESKTOP_LOCALES) {
    const section = tables[code].pinEntry
    assert.ok(section && typeof section === 'object', `${code} has the section`)
    assert.deepEqual(Object.keys(section).sort(), [...PIN_TEXT_KEYS].sort(), `${code}: the same keys as the card reads`)
    for (const key of PIN_TEXT_KEYS) assert.ok(typeof section[key] === 'string' && section[key].trim(), `${code}.${key} is a text`)
  }
})

test('a language does not show another one beside the hint: only the languages that share a text may repeat the English one', () => {
  // the English-speaking variants are English; every other language has its own words for the long sentences
  const shared = new Set(['en', 'en_GB', 'en_US'])
  for (const code of DESKTOP_LOCALES) {
    if (shared.has(code)) continue
    for (const key of ['hint', 'named', 'unnamed', 'preparing', 'wrong', 'unreachable', 'failed', 'endedFinished', 'endedNoRequest', 'endedHostLost']) {
      assert.notEqual(tables[code].pinEntry[key], en.pinEntry[key], `${code}.${key} is translated`)
    }
  }
})

test('the heading keeps its {name} and the keyboard hint names the key that cancels, in every language', () => {
  for (const code of DESKTOP_LOCALES) {
    const section = tables[code].pinEntry
    assert.equal(section.named.split('{name}').length, 2, `${code}: {name} once in the heading`)
    assert.ok(!section.unnamed.includes('{name}'), code)
    const hint = section.keyboardHint
    assert.ok(hint.trim().length > 8 && hint.length <= 60, `${code}: one short line`)
    // the PIN is confirmed by the fourth digit, so no Enter step is promised; the French keyboard names the key Échap
    assert.match(hint, /Esc|Échap/, code)
    assert.doesNotMatch(hint, /Enter/, code)
  }
})

test('every reason the host gives has its sentence in every language', () => {
  for (const code of DESKTOP_LOCALES) for (const reason of ENDED_REASONS) assert.ok(tables[code].pinEntry[endedKey(reason)], `${code}:${reason}`)
})

test('the card reads its texts from the panel language and the hint is shown unless the screen has nothing but touch', () => {
  const page = source('PinApp.vue')
  assert.match(page, /const text = computed\(\(\) => t\.value\.pinEntry \|\| \{\}\)/)
  assert.doesNotMatch(page, /simple-i18n/)
  assert.match(page, /touchOnly = ref\(false\)/)
  assert.match(page, /coarsePointer\?\.matches && !finePointer\?\.matches/)
  assert.match(page, /v-if="keyboardHint && !touchOnly"/)
})
