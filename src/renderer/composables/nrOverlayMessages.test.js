import assert from 'node:assert/strict'
import test from 'node:test'
import { nrDegradedReason, nrOverlayText } from './nrOverlayMessages.js'

const zh = nrOverlayText('zh')
const en = nrOverlayText('en')

test('component-unavailable degradation explains the permission fix', () => {
  const pipeline = { nr_reason: 'component_unavailable' }
  assert.deepEqual(nrDegradedReason(pipeline, zh), { main: zh.componentUnavailable, detail: '' })
  assert.deepEqual(nrDegradedReason(pipeline, en), { main: en.componentUnavailable, detail: '' })
})

test('other degradation reasons keep the fallback and show their code', () => {
  assert.deepEqual(nrDegradedReason({ nr_reason: 'runtime_untrusted' }, en), {
    main: en.fallback,
    detail: 'runtime_untrusted'
  })
  assert.deepEqual(nrDegradedReason({}, en), { main: en.fallback, detail: '' })
  assert.deepEqual(nrDegradedReason(null, zh), { main: zh.fallback, detail: '' })
})
