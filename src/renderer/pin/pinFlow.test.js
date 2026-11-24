import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { test } from 'node:test'
import { fileURLToPath } from 'node:url'
import {
  ENDED_REASONS, PAD_CELLS, PIN_TEXT_KEYS, PIN_LENGTH, PIN_ROWS, classifyAnswer, classifyFailure, endedKey, headingFor, keepWhileMounted, keyAt, movePad, padEdges, stickDirection,
} from './pinFlow.js'
import { en } from '../desktop/i18n/en.js'
import { zh } from '../desktop/i18n/zh.js'

const MESSAGES = { en: en.pinEntry, zh: zh.pinEntry }

const here = dirname(fileURLToPath(import.meta.url))
const source = (...parts) => readFileSync(join(here, ...parts), 'utf8')

test('the heading names the client the host reports, and does not when it reports none', () => {
  assert.equal(headingFor(MESSAGES.en, 'Living Room TV'), '“Living Room TV” wants to pair')
  assert.equal(headingFor(MESSAGES.zh, '客厅电视'), '“客厅电视”请求配对')
  assert.equal(headingFor(MESSAGES.en, ''), 'A device wants to pair')
  assert.equal(headingFor(MESSAGES.en, '   '), 'A device wants to pair')
  assert.equal(headingFor(MESSAGES.en, undefined), 'A device wants to pair')
  assert.equal(headingFor(MESSAGES.en, 'x'.repeat(200)).length, '“” wants to pair'.length + 64, 'a long name is cut')
})

test('the host answers {"status":"true"} when it paired, anything else is a wrong PIN', () => {
  assert.equal(classifyAnswer({ status: 'true' }), 'ok')
  assert.equal(classifyAnswer({ status: true }), 'ok')
  assert.equal(classifyAnswer({ status: 'True' }), 'ok')
  assert.equal(classifyAnswer({ status: 'false' }), 'wrong')
  assert.equal(classifyAnswer({ status: false }), 'wrong')
  assert.equal(classifyAnswer({}), 'wrong')
  assert.equal(classifyAnswer(null), 'wrong')
  assert.equal(classifyFailure(), 'unreachable')
})

test('an ended pairing says why, and an unknown reason reads as the general one', () => {
  for (const reason of ENDED_REASONS) {
    assert.ok(MESSAGES.en[endedKey(reason)], reason)
    assert.ok(MESSAGES.zh[endedKey(reason)], reason)
    assert.ok(PIN_TEXT_KEYS.includes(endedKey(reason)), reason)
  }
  assert.equal(endedKey('something-newer'), 'endedFinished')
  assert.equal(endedKey(undefined), 'endedFinished')
})

test('the reasons are the words the host sends', () => {
  const rust = source('..', '..', '..', 'src-tauri', 'src', 'pin_window.rs')
  const sent = [...rust.matchAll(/Self::\w+ => "([a-z-]+)"/g)].map((match) => match[1])
  assert.deepEqual([...sent].sort(), [...ENDED_REASONS].sort())
  const event = /PAIRING_ENDED_EVENT: &str = "([^"]+)"/.exec(rust)?.[1]
  assert.ok(event && source('PinApp.vue').includes(`'${event}'`), 'the page listens for the event the host emits')
  const label = /PIN_WINDOW_ID: &str = "([^"]+)"/.exec(rust)?.[1]
  assert.ok(label)
})

test('a controller moves over the keys that can be pressed and stops at the edges', () => {
  assert.equal(PIN_LENGTH, 4)
  assert.equal(PAD_CELLS.length, 11, 'ten digits and the backspace')
  assert.deepEqual(movePad([1, 1], 'up'), [0, 1])
  assert.equal(keyAt(movePad([1, 1], 'down')), '8')
  assert.equal(keyAt(movePad([2, 1], 'down')), '0')
  assert.deepEqual(movePad([0, 0], 'up'), [0, 0])
  assert.deepEqual(movePad([0, 0], 'left'), [0, 0])
  assert.deepEqual(movePad([0, 2], 'right'), [0, 2])
  // the empty cell next to the zero is no key: the cursor stays on the zero
  assert.deepEqual(movePad([3, 1], 'right'), [3, 1])
  assert.deepEqual(movePad([2, 2], 'down'), [2, 2], 'below the nine there is no key')
  assert.equal(keyAt(movePad([3, 1], 'left')), 'backspace')
  assert.deepEqual(movePad([1, 1], 'sideways'), [1, 1])
})

test('every pressable cell is reachable from the middle of the pad', () => {
  const seen = new Set(['1,1'])
  const queue = [[1, 1]]
  while (queue.length) {
    const cell = queue.shift()
    for (const direction of ['up', 'down', 'left', 'right']) {
      const next = movePad(cell, direction)
      if (!seen.has(String(next))) { seen.add(String(next)); queue.push(next) }
    }
  }
  assert.equal(seen.size, PAD_CELLS.length)
  assert.equal(PIN_ROWS.flat().filter(Boolean).length, PAD_CELLS.length)
})

test('only a new press of a button counts, so a held d-pad moves one key', () => {
  const idle = Array(17).fill(false)
  const press = (...indexes) => idle.map((_, i) => indexes.includes(i))
  assert.deepEqual(padEdges(idle, press(13)), ['down'])
  assert.deepEqual(padEdges(press(13), press(13)), [])
  assert.deepEqual(padEdges(press(13), idle), [])
  assert.deepEqual(padEdges(idle, press(0)), ['press'])
  assert.deepEqual(padEdges(idle, press(1)), ['cancel'])
  assert.deepEqual(padEdges([], press(12, 15)), ['up', 'right'])
  assert.deepEqual(padEdges(idle, press(3)), [], 'the face buttons that do nothing here stay quiet')
})

test('the left stick is a d-pad past its dead zone', () => {
  assert.equal(stickDirection(0, 0), null)
  assert.equal(stickDirection(0.3, 0.3), null)
  assert.equal(stickDirection(0.9, 0.1), 'right')
  assert.equal(stickDirection(-0.9, 0.2), 'left')
  assert.equal(stickDirection(0.1, 0.9), 'down')
  assert.equal(stickDirection(0.1, -0.9), 'up')
})

test('the window page is built with the panel: an entry of the build and a page the host opens', () => {
  assert.ok(source('..', '..', '..', 'vite.config.js').includes("pin: resolve(rendererSrcPath, 'pin/index.html')"))
  assert.ok(source('..', '..', '..', 'src-tauri', 'src', 'pin_window.rs').includes('"pin/index.html"'))
})

test('a listener registered for a component that is gone is undone at once and starts nothing', async () => {
  // still mounted: the undo is kept
  let undone = 0
  const kept = []
  assert.equal(await keepWhileMounted(async () => () => { undone += 1 }, () => true, (undo) => kept.push(undo)), true)
  assert.equal(kept.length, 1)
  assert.equal(undone, 0)

  // unmounted while the registration was waiting: undone at once, never kept, and the caller is told not to go on
  let mounted = true
  let release
  const registration = () => new Promise((resolve) => { release = () => resolve(() => { undone += 1 }) })
  const keptLate = []
  const pending = keepWhileMounted(registration, () => mounted, (undo) => keptLate.push(undo))
  mounted = false
  release()
  assert.equal(await pending, false)
  assert.equal(undone, 1, 'the listener is undone as soon as it arrives')
  assert.equal(keptLate.length, 0)

  // no host to listen to (a browser preview): nothing to undo, and the answer is the mounted state
  const noHost = async () => { throw new Error('no host') }
  assert.equal(await keepWhileMounted(noHost, () => true, () => assert.fail('nothing to keep')), true)
  assert.equal(await keepWhileMounted(noHost, () => false, () => assert.fail('nothing to keep')), false)
})

test('the page starts its client-name timer only after the listener is registered for a mounted page', () => {
  const page = source('PinApp.vue')
  const registered = page.indexOf('keepWhileMounted(')
  const timer = page.indexOf('nameTimer = setInterval')
  const guard = page.indexOf('if (!mounted) return')
  assert.ok(registered > 0 && guard > registered && timer > guard, 'register, then check, then start the timer')
  assert.match(page, /onBeforeUnmount\(\(\) => \{\s*unmounted = true/)
  assert.match(page, /clearInterval\(nameTimer\)/)
})
