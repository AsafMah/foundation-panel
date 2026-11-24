// The full-screen PIN entry (src-tauri/src/pin_window.rs): what it says, what the host's answer means, and how a controller moves over the keypad. No DOM and no
// framework, so the tests run it as it is.
export const PIN_LENGTH = 4
/** After a pairing the window stays this long, so the person sees that it worked. */
export const CLOSE_AFTER_SUCCESS_MS = 1600
/** After the host says that pairing is over the page says so, then closes itself (the host closes it at 3 s if the page has not). */
export const CLOSE_AFTER_ENDED_MS = 1800
/** The reasons pin_window.rs sends with `pin-pairing-ended` (EndReason::as_str). */
export const ENDED_REASONS = Object.freeze(['finished', 'no-request', 'host-lost'])

// The texts of the card are the `pinEntry` section of every desktop language file (desktop/i18n/<code>.js); a test keeps them whole in all 20.
/** The keys of that section, in the order of the files. */
export const PIN_TEXT_KEYS = Object.freeze(['title', 'hint', 'named', 'unnamed', 'keyboardHint', 'cancel', 'preparing', 'pairing', 'done', 'wrong', 'unreachable', 'failed', 'endedFinished', 'endedNoRequest', 'endedHostLost', 'backspace'])

/** The heading: the client's name when the host reports it. */
export function headingFor(text, clientName) {
  const name = String(clientName || '').trim().slice(0, 64)
  return name ? text.named.replace('{name}', name) : text.unnamed
}

/** What the host's answer to `POST /api/pin` is: 'ok' when it paired, 'wrong' when it did not take the PIN. */
export function classifyAnswer(data) {
  return String(data && data.status).toLowerCase() === 'true' ? 'ok' : 'wrong'
}

/** A failed request is the host not being there (the page cannot say more than that). */
export const classifyFailure = () => 'unreachable'

/** The message key of an ended pairing; an unknown reason reads as the general one. */
const ENDED_KEYS = { finished: 'endedFinished', 'no-request': 'endedNoRequest', 'host-lost': 'endedHostLost' }
export function endedKey(reason) {
  return ENDED_KEYS[ENDED_REASONS.includes(reason) ? reason : 'finished']
}

/** The rows of MPinPad's keypad (foundation-ui vue/pinpad.js PIN_ROWS), kept here so that this file runs in node without the UI library; PinApp checks the pad it mounted against it. */
export const PIN_ROWS = Object.freeze([['1', '2', '3'], ['4', '5', '6'], ['7', '8', '9'], ['backspace', '0', '']])

/** The cells of the keypad that can be pressed, as [row, col]: the empty cell of the last row is not one. */
export const PAD_CELLS = Object.freeze(PIN_ROWS.flatMap((row, r) => row.map((key, c) => (key ? [r, c] : null)).filter(Boolean)))

/** The cell a direction of the pad leads to from [row, col]: the next pressable cell that way, the same one at the edge. */
export function movePad([row, col], direction) {
  const step = { up: [-1, 0], down: [1, 0], left: [0, -1], right: [0, 1] }[direction]
  if (!step) return [row, col]
  let [r, c] = [row + step[0], col + step[1]]
  // the empty cell is skipped along the row; up and down stay in their column
  while (r >= 0 && r < PIN_ROWS.length && c >= 0 && c < PIN_ROWS[0].length) {
    if (PIN_ROWS[r][c]) return [r, c]
    r += step[0]; c += step[1]
  }
  return [row, col]
}

/** The key a cell of the pad is: a digit, or 'backspace'. */
export const keyAt = ([row, col]) => PIN_ROWS[row][col]

// the standard layout of a controller (Gamepad API, "Standard Gamepad"): A, B and the d-pad
const BUTTON_A = 0, BUTTON_B = 1, DPAD = { 12: 'up', 13: 'down', 14: 'left', 15: 'right' }

/**
 * What a controller did between two readings: the pressed buttons of the standard layout, as a boolean array each. Only a press (not held) counts, so a held d-pad
 * moves one cell. Returns a list of 'up' | 'down' | 'left' | 'right' | 'press' | 'cancel'.
 */
export function padEdges(before, now) {
  const out = []
  const edge = (i) => !!now[i] && !before[i]
  for (const [index, direction] of Object.entries(DPAD)) if (edge(Number(index))) out.push(direction)
  if (edge(BUTTON_A)) out.push('press')
  if (edge(BUTTON_B)) out.push('cancel')
  return out
}

/** The left stick as a d-pad: past the dead zone, the stronger axis. null inside it. */
export function stickDirection(x, y, deadZone = 0.55) {
  if (Math.hypot(x, y) < deadZone) return null
  return Math.abs(x) > Math.abs(y) ? (x > 0 ? 'right' : 'left') : (y > 0 ? 'down' : 'up')
}

/**
 * Registers something that is handed back asynchronously (the host's `listen` answers with the function that undoes it) for a component that may be gone by then.
 * `register()` resolves to the undo function; `keep(undo)` takes it while the component is mounted. When the component was unmounted while `register()` was waiting, nothing
 * is kept: the undo runs at once. Returns whether the component is still mounted, so the caller does not start anything (a timer) for a component that is gone.
 * A `register()` that fails (outside the panel there is no host to listen to) leaves nothing to undo and answers by the mounted state alone.
 */
export async function keepWhileMounted(register, isMounted, keep) {
  let undo
  try { undo = await register() } catch { return isMounted() }
  if (!isMounted()) {
    try { if (typeof undo === 'function') undo() } catch { /* it was undone already */ }
    return false
  }
  if (typeof undo === 'function') keep(undo)
  return true
}
