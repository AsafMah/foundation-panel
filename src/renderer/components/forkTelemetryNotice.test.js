import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

// Fork (AsafMah/foundation-sunshine): the host sends no telemetry until the person turns it on (the host's src/telemetry/fork_defaults.h,
// reported by GET /api/config). The welcome's one-time notice says data is sent, so it is skipped while both choices are off.
const welcome = readFileSync(new URL('./welcome.vue', import.meta.url), 'utf8')

test('fork: the welcome does not say data is sent while both telemetry choices are off', () => {
  const mounted = welcome.slice(welcome.indexOf('onMounted(async () => {'))
  const refresh = mounted.indexOf('telemetry.refresh()')
  const mark = mounted.indexOf("if (telemetry.consent.value === 'off' && telemetry.errors.value === 'off') telemetry.markErrorsNoticeSeen()")
  const entry = mounted.indexOf('entry.value =')
  assert.ok(refresh >= 0 && mark > refresh && entry > mark, 'the notice is settled after the host answered and before the guide is drawn')
})