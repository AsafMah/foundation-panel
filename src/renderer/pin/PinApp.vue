<script setup>
// The full-screen PIN entry (src-tauri/src/pin_window.rs): a dimmed, transparent window over the display with one card: who asks, the keypad, Cancel.
// Keyboard (digits, Backspace, Esc), touch and mouse press the keypad's own buttons; a controller moves over them (d-pad or left stick, A presses, B cancels).
// The host closes the window when the pairing is over (`pin-pairing-ended`); the page says so first.
import { computed, onBeforeUnmount, onMounted, ref, watchEffect } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import MPinPad from 'foundation-ui/vue/MPinPad.vue'
import MIcon from 'foundation-ui/vue/MIcon.vue'
import MButton from 'foundation-ui/vue/MButton.vue'
import { useI18n } from '../desktop/i18n/index.js'
import { documentLang } from '../desktop/i18n/merge.js'
import {
  CLOSE_AFTER_ENDED_MS, CLOSE_AFTER_SUCCESS_MS, PAD_CELLS, PIN_LENGTH, classifyAnswer, classifyFailure, endedKey, headingFor, keepWhileMounted, movePad, padEdges, stickDirection,
} from './pinFlow.js'

const DEFAULT_PROXY = 'http://localhost:48081'
const DEFAULT_NAME = 'Moonlight Client'

// every text of the card is in the language of the panel (desktop/i18n, the `pinEntry` section of each language file)
const { t, locale } = useI18n()
const text = computed(() => t.value.pinEntry || {})
const keyboardHint = computed(() => text.value.keyboardHint || '')
watchEffect(() => { document.documentElement.lang = documentLang(locale.value); document.title = text.value.title || '' })
// a screen with nothing but a finger to type with has no use for it
const coarsePointer = window.matchMedia?.('(any-pointer: coarse)')
const finePointer = window.matchMedia?.('(any-pointer: fine)')
const touchOnly = ref(false)
const onPointers = () => { touchOnly.value = !!coarsePointer?.matches && !finePointer?.matches }
onPointers()

const pin = ref('')
// preparing: the host's proxy is not answering yet | ready | submitting | done | ended
const phase = ref('preparing')
const notice = ref({ kind: '', key: '' })
const clientName = ref('')
// the address the host's proxy is bound to once it is ready ('' until then: nothing is sent to a guessed port)
const proxy = ref('')
const padCursor = ref(null)
const root = ref(null)

const heading = computed(() => headingFor(text.value, clientName.value))
const locked = computed(() => phase.value !== 'ready')
const noticeText = computed(() => (phase.value === 'preparing' ? text.value.preparing : phase.value === 'submitting' ? text.value.pairing : notice.value.key ? text.value[notice.value.key] : ''))

const timers = new Set()
const later = (fn, ms) => { const id = setTimeout(() => { timers.delete(id); fn() }, ms); timers.add(id) }
const closeWindow = async () => { try { await getCurrentWindow().close() } catch { /* a browser preview has no window to close */ } }
const cancel = () => { if (phase.value !== 'submitting') void closeWindow() }

const inPanel = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

// `get_proxy_url_command` answers with the port the proxy is meant to have, while the listener is bound a moment later and may take another one when that is in use:
// the address is asked of `wait_for_proxy_ready`, which answers when the listener is up. Outside the panel (a browser preview) the default address stands.
async function resolveProxy() {
  if (!inPanel()) return DEFAULT_PROXY
  for (let attempt = 0; attempt < 3 && phase.value !== 'ended' && !unmounted; attempt++) {
    try {
      const base = await invoke('wait_for_proxy_ready')
      if (base) return String(base).replace(/\/+$/, '')
    } catch { /* the proxy did not come up in time: ask again */ }
  }
  return ''
}

// the client that waits for its PIN, as the host reports it (the host may report it a moment after the window opened, so it is asked again until it is known)
async function refreshClientName() {
  if (!inPanel()) return
  try {
    const name = String((await invoke('pin_pairing_client')) || '').trim()
    if (name && name !== clientName.value) clientName.value = name
  } catch { /* the host is not reporting: the heading stays general */ }
}

async function waitForProxy() {
  if (!proxy.value) return null
  for (let attempt = 0; attempt < 30; attempt++) {
    try {
      const controller = new AbortController()
      const timer = setTimeout(() => controller.abort(), 3000)
      const response = await fetch(`${proxy.value}/api/config`, { signal: controller.signal })
      clearTimeout(timer)
      if (response.ok) return await response.json().catch(() => ({}))
    } catch { /* the host is still starting */ }
    if (phase.value === 'ended' || unmounted) return null
    await new Promise((resolve) => setTimeout(resolve, 1000))
  }
  return null
}

async function submit(code) {
  if (phase.value !== 'ready' || code.length !== PIN_LENGTH) return
  phase.value = 'submitting'
  notice.value = { kind: '', key: '' }
  let answer
  try {
    // a proxy that was not up at the start is asked for again before a PIN is given up on
    if (!proxy.value) proxy.value = await resolveProxy()
    if (!proxy.value) throw new Error('no proxy')
    await refreshClientName()
    const controller = new AbortController()
    const timer = setTimeout(() => controller.abort(), 10000)
    const response = await fetch(`${proxy.value}/api/pin`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ pin: code, name: clientName.value || DEFAULT_NAME }),
      signal: controller.signal,
    })
    clearTimeout(timer)
    if (!response.ok) throw new Error(String(response.status))
    answer = classifyAnswer(await response.json())
  } catch { answer = classifyFailure() }
  if (phase.value === 'ended') return
  if (answer === 'ok') {
    phase.value = 'done'
    notice.value = { kind: 'ok', key: 'done' }
    later(closeWindow, CLOSE_AFTER_SUCCESS_MS)
    return
  }
  pin.value = ''
  notice.value = { kind: 'error', key: answer === 'wrong' ? 'wrong' : 'unreachable' }
  phase.value = 'ready'
}

// ---- a controller: the d-pad or the left stick moves a cursor over the keys, A presses, B cancels ----
const pads = () => (navigator.getGamepads ? Array.from(navigator.getGamepads()).filter(Boolean) : [])
let before = []
let stickBefore = null
let cursor = null
let poll = 0
let nameTimer = 0
// set when the component goes: what an `await` of the mount brings back after that is undone or never started
let unmounted = false
const keyButtons = () => Array.from(root.value?.querySelectorAll('.m-pin__key') || [])
function showCursor(cell) {
  const buttons = keyButtons()
  // the pad is the one this page was written against: a different layout leaves the controller without a cursor rather than with a wrong one
  if (buttons.length !== PAD_CELLS.length) return
  cursor = cell
  padCursor.value = PAD_CELLS.findIndex(([r, c]) => r === cell[0] && c === cell[1])
}
function onPad(action) {
  if (action === 'cancel') return cancel()
  if (phase.value === 'ended') return
  if (action === 'press') {
    if (cursor === null) return showCursor([1, 1])
    const button = keyButtons()[padCursor.value]
    if (button && !button.disabled) button.click()
    return
  }
  showCursor(cursor === null ? [1, 1] : movePad(cursor, action))
}
function readPad() {
  const pad = pads()[0]
  if (!pad) { before = []; stickBefore = null; return }
  const now = pad.buttons.map((button) => button.pressed)
  for (const action of padEdges(before, now)) onPad(action)
  before = now
  const stick = stickDirection(pad.axes[0] || 0, pad.axes[1] || 0)
  if (stick && stick !== stickBefore) onPad(stick)
  stickBefore = stick
}

// ---- keys that are not the keypad's ----
const onKey = (event) => { if (event.key === 'Escape') { event.preventDefault(); cancel() } }
const onScrim = (event) => { if (event.target === event.currentTarget) cancel() }

// the colour scheme follows the system while the window is open (index.html sets it for the first paint)
const systemDark = window.matchMedia?.('(prefers-color-scheme: dark)')
const onScheme = () => {
  const root = document.documentElement
  const theme = systemDark?.matches ? 'dark' : 'light'
  root.setAttribute('data-bs-theme', theme)
  root.setAttribute('data-theme', theme)
}

const off = []
onMounted(async () => {
  systemDark?.addEventListener?.('change', onScheme)
  coarsePointer?.addEventListener?.('change', onPointers)
  finePointer?.addEventListener?.('change', onPointers)
  window.addEventListener('keydown', onKey)
  poll = setInterval(readPad, 33)
  // for the harness and for a person at the devtools: the calls the host and the keypad make, and a way to put the page in a state to look at
  window.__pinEntry = {
    ended: (reason) => onEnded(reason),
    client: (name) => { clientName.value = String(name || '') },
    state: (next, key = '', kind = '') => { phase.value = next; notice.value = { kind, key } },
    cursor: (cell) => showCursor(cell),
    pin: (digits) => { pin.value = String(digits || '').slice(0, PIN_LENGTH) },
  }
  // outside the panel there is no host to listen to: the harness drives window.__pinEntry
  const mounted = await keepWhileMounted(() => listen('pin-pairing-ended', (event) => onEnded(event.payload)), () => !unmounted, (undo) => off.push(undo))
  // the window can close while the listener is being registered: then the listener is undone already and no timer is started
  if (!mounted) return
  void refreshClientName()
  nameTimer = setInterval(() => { if (phase.value !== 'done' && phase.value !== 'ended') void refreshClientName() }, 1000)
  proxy.value = await resolveProxy()
  if (unmounted) return
  const config = await waitForProxy()
  if (unmounted) return
  if (phase.value === 'preparing') {
    if (config) phase.value = 'ready'
    else { notice.value = { kind: 'error', key: 'unreachable' }; phase.value = 'ready' }
  }
})

function onEnded(reason) {
  // a pairing that just succeeded is closing on its own timer
  if (phase.value === 'done' || phase.value === 'ended') return
  phase.value = 'ended'
  notice.value = { kind: 'info', key: endedKey(reason) }
  later(closeWindow, CLOSE_AFTER_ENDED_MS)
}

onBeforeUnmount(() => {
  unmounted = true
  systemDark?.removeEventListener?.('change', onScheme)
  coarsePointer?.removeEventListener?.('change', onPointers)
  finePointer?.removeEventListener?.('change', onPointers)
  window.removeEventListener('keydown', onKey)
  clearInterval(poll)
  clearInterval(nameTimer)
  timers.forEach(clearTimeout)
  off.forEach((fn) => fn())
})
</script>

<template>
  <div ref="root" class="pin-scrim" role="presentation" @click="onScrim" @contextmenu.prevent>
    <main class="pin-card" role="dialog" aria-modal="true" :aria-label="heading">
      <header class="pin-head">
        <span class="pin-ico" aria-hidden="true"><MIcon :name="phase === 'done' ? 'check_circle' : 'lock'" fill /></span>
        <h1 class="pin-title" data-redact="client">{{ heading }}</h1>
        <p class="pin-hint">{{ text.hint }}</p>
      </header>
      <MPinPad
        v-model="pin"
        class="pin-pad"
        :length="PIN_LENGTH"
        :disabled="locked"
        :label="text.title"
        :backspace-label="text.backspace"
        :data-cursor="padCursor"
        @complete="submit"
      />
      <p class="pin-notice" :class="`is-${notice.kind || 'idle'}`" role="status" aria-live="polite">{{ noticeText }}</p>
      <footer class="pin-foot" :class="{ 'is-quiet': phase === 'done' }">
        <MButton variant="text" :disabled="phase === 'submitting'" @click="cancel">{{ text.cancel }}</MButton>
      </footer>
      <p v-if="keyboardHint && !touchOnly" class="pin-keys" :class="{ 'is-quiet': phase === 'done' }">{{ keyboardHint }}</p>
    </main>
  </div>
</template>

<style>
.pin-scrim {
  position: fixed; inset: 0; display: grid; place-items: center; padding: 16px; box-sizing: border-box;
  background: color-mix(in srgb, var(--md-sys-color-scrim, #000) 62%, transparent);
  -webkit-backdrop-filter: blur(14px); backdrop-filter: blur(14px);
  font-family: var(--md-ref-typeface-plain, system-ui, sans-serif);
  animation: pin-fade 200ms ease-out both;
}
.pin-card {
  width: min(100%, 400px); max-height: 100%; overflow: auto; box-sizing: border-box;
  display: grid; justify-items: center; gap: 20px; padding: 32px 24px 16px;
  grid-template-areas: "head" "pad" "note" "foot" "keys";
  border-radius: 36px;
  background: var(--md-sys-color-surface-container-low);
  color: var(--md-sys-color-on-surface);
  box-shadow: var(--md-sys-elevation-level3, 0 12px 40px rgba(0, 0, 0, 0.35));
  animation: pin-rise 320ms var(--m-spring-default-spatial, cubic-bezier(0.2, 0.9, 0.3, 1)) both;
}
.pin-head { grid-area: head; display: flex; flex-direction: column; align-items: center; gap: 8px; text-align: center; }
.pin-pad { grid-area: pad; }
.pin-notice { grid-area: note; }
.pin-foot { grid-area: foot; }
.pin-keys { grid-area: keys; margin: 0; font: 400 12px/16px var(--md-ref-typeface-plain, inherit); text-align: center; color: var(--md-sys-color-on-surface-variant); opacity: 0.78; }
.pin-keys.is-quiet { visibility: hidden; }
.pin-ico {
  display: grid; place-items: center; width: 56px; height: 56px; border-radius: 50%;
  background: var(--md-sys-color-primary-container); color: var(--md-sys-color-on-primary-container);
}
.pin-ico .m-icon, .pin-ico svg { font-size: 28px; width: 28px; height: 28px; }
.pin-title { margin: 8px 0 0; font: 500 24px/32px var(--md-ref-typeface-brand, inherit); overflow-wrap: anywhere; text-wrap: balance; word-break: auto-phrase; }
.pin-hint { margin: 0; font: 400 14px/20px var(--md-ref-typeface-plain, inherit); color: var(--md-sys-color-on-surface-variant); text-wrap: balance; word-break: auto-phrase; }
.pin-notice { min-height: 20px; margin: 0; font: 500 14px/20px var(--md-ref-typeface-plain, inherit); text-align: center; color: var(--md-sys-color-on-surface-variant); }
.pin-notice.is-error { color: var(--md-sys-color-error); }
.pin-notice.is-ok { color: var(--md-sys-color-primary); }
.pin-foot { display: flex; justify-content: center; }
/* a pairing that worked closes by itself: Cancel keeps its place and goes */
.pin-foot.is-quiet { visibility: hidden; }
/* the controller's cursor: the key it is on (the nth key of the pad) */
.pin-pad[data-cursor="0"] .m-pin__pad > :nth-child(1),
.pin-pad[data-cursor="1"] .m-pin__pad > :nth-child(2),
.pin-pad[data-cursor="2"] .m-pin__pad > :nth-child(3),
.pin-pad[data-cursor="3"] .m-pin__pad > :nth-child(4),
.pin-pad[data-cursor="4"] .m-pin__pad > :nth-child(5),
.pin-pad[data-cursor="5"] .m-pin__pad > :nth-child(6),
.pin-pad[data-cursor="6"] .m-pin__pad > :nth-child(7),
.pin-pad[data-cursor="7"] .m-pin__pad > :nth-child(8),
.pin-pad[data-cursor="8"] .m-pin__pad > :nth-child(9),
.pin-pad[data-cursor="9"] .m-pin__pad > :nth-child(10),
.pin-pad[data-cursor="10"] .m-pin__pad > :nth-child(11) { outline: 3px solid var(--md-sys-color-primary); outline-offset: 3px; }
/* a short window (a phone held sideways, a small display): the pad moves beside the words, and is a little tighter */
@media (max-height: 560px) {
  .pin-card {
    width: min(100%, 640px); grid-template-columns: 1fr auto; column-gap: 28px; row-gap: 8px; padding: 16px 24px; align-items: center;
    grid-template-areas: "head pad" "note pad" "foot pad" "keys pad";
  }
  .pin-pad { zoom: 0.8; }
  .pin-ico { display: none; }
  .pin-title { margin: 0; font-size: 20px; line-height: 28px; }
}
@keyframes pin-fade { from { opacity: 0; } to { opacity: 1; } }
@keyframes pin-rise { from { opacity: 0; transform: translateY(24px) scale(0.96); } to { opacity: 1; transform: none; } }
@media (prefers-reduced-motion: reduce) { .pin-scrim, .pin-card { animation: none; } }
</style>
