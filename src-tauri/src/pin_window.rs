//! The full-screen PIN entry: a window of its own that covers the display, comes before a game and takes the keyboard, a finger or a controller, while a client waits for its PIN.
//!
//! It is the same entry the host's Web page `/pin` stood for before the panel pages took over: the panel's own "pairing" page (`open_main_panel_page(app, "pairing")`) stays, this
//! is the one that does not need the main window to be open or in front.
//!
//! How it is made follows windows/building.rs and windows/closing.rs:
//!   * `WebviewWindowBuilder::build` is never called from a handler of the main thread (the tray's menu, a toast click, a second launch all are): the window is built on a worker, once
//!     at a time. A window built there and shown before its page had loaded was the flat grey surface of building.rs; this one is made hidden and shown when its page has loaded
//!     (with a cap, so a page that never loads does not leave the person without an entry).
//!   * It is closed through `closing::close_window`: hidden first, destroyed after, and never shown again while it closes. A request that comes while it closes waits for the end.
//!
//! Its life follows the host's tray state (`TrayState.status == "pairing"`, tray.rs `apply_tray_state`): it ends when pairing succeeded, expired or was cancelled. `Lifecycle` is
//! that rule without any window, so it is tested on its own.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use log::{debug, error, info, warn};
use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewWindow, webview::PageLoadEvent};

use crate::windows::{building, closing};

pub const PIN_WINDOW_ID: &str = "pin_pairing";
/// The page tells itself that pairing is over with this event (its payload is why), so it can say so before it closes.
pub const PAIRING_ENDED_EVENT: &str = "pin-pairing-ended";

/// How long an entry that was asked for waits for the host to report a pairing. The host's state can come a moment after the address that asked for the entry (a second launch from the
/// web page, a toast of a request that is still being registered); after this long without a pairing the entry is not wanted.
pub(crate) const PAIRING_GRACE: Duration = Duration::from_secs(10);
/// After the page was told that pairing is over, the window is closed by the host if the page has not closed it.
const END_FALLBACK: Duration = Duration::from_millis(3000);
/// If the page never loads, the window still comes.
const SHOW_CAP: Duration = Duration::from_millis(4000);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    Keep,
    /// Pairing is over: say why to the page and close the window.
    End(EndReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EndReason {
    /// A pairing was pending and is not any more: it succeeded, expired or was cancelled.
    Finished,
    /// No pairing was ever reported in the grace period.
    NoRequest,
    /// The host is not reachable.
    HostLost,
}

impl EndReason {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Finished => "finished",
            Self::NoRequest => "no-request",
            Self::HostLost => "host-lost",
        }
    }
}

/// The rule of the entry's life, for one window at a time.
#[derive(Default)]
pub(crate) struct Lifecycle {
    opened: Option<Instant>,
    seen_pairing: bool,
}

impl Lifecycle {
    pub(crate) fn open(&mut self, now: Instant) {
        self.opened = Some(now);
        self.seen_pairing = false;
    }

    pub(crate) fn is_open(&self) -> bool {
        self.opened.is_some()
    }

    pub(crate) fn closed(&mut self) {
        *self = Self::default();
    }

    /// The host's status at `now`; `None` when the host is not reachable.
    pub(crate) fn observe(&mut self, status: Option<&str>, now: Instant) -> Verdict {
        let Some(opened) = self.opened else { return Verdict::Keep };
        match status {
            Some("pairing") => {
                self.seen_pairing = true;
                Verdict::Keep
            }
            None if self.seen_pairing || now.duration_since(opened) >= PAIRING_GRACE => Verdict::End(EndReason::HostLost),
            Some(_) if self.seen_pairing => Verdict::End(EndReason::Finished),
            Some(_) if now.duration_since(opened) >= PAIRING_GRACE => Verdict::End(EndReason::NoRequest),
            // a host that has not reported yet: the entry waits for it
            _ => Verdict::Keep,
        }
    }
}

/// Where a click on a "pairing request" toast goes, by what the host reports now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Entry {
    FullScreen,
    /// The request is over (a toast from the notification centre that was answered or expired): the full-screen entry would be a dead end, the panel's pairing page is not.
    PanelPage,
}

/// Where a pairing request is answered, from the setting (`pin_prompt.rs`) and what the host reports: the panel's own pairing page when the setting says so, otherwise the full-screen
/// entry for as long as a request can still be answered (`entry_for_click`).
pub(crate) fn route(prompt: crate::pin_prompt::PinPrompt, status: Option<&str>) -> Entry {
    match prompt {
        crate::pin_prompt::PinPrompt::Panel => Entry::PanelPage,
        crate::pin_prompt::PinPrompt::Fullscreen => entry_for_click(status),
    }
}

pub(crate) fn entry_for_click(status: Option<&str>) -> Entry {
    match status {
        // `None`: the host has not reported since this panel started (a click that started the panel), its request is likely still pending
        Some("pairing") | None => Entry::FullScreen,
        Some(_) => Entry::PanelPage,
    }
}

static LIFECYCLE: Mutex<Option<Lifecycle>> = Mutex::new(None);
/// Counts the windows made: the host's late close of a window that was told its pairing is over must not close the one made after it.
static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn lifecycle<T>(f: impl FnOnce(&mut Lifecycle) -> T) -> T {
    let mut guard = LIFECYCLE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    f(guard.get_or_insert_with(Lifecycle::default))
}

/// Opens the full-screen entry, or brings the one that is open to the front. Safe to call from any thread and from any handler.
pub fn open<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    if closing::is_closing(PIN_WINDOW_ID) {
        closing::after_closed(app, PIN_WINDOW_ID, |app| {
            if let Err(error) = open(&app) {
                error!("[pin] it could not be opened after the close: {error}");
            }
        });
        return Ok(());
    }
    if let Some(window) = app.get_webview_window(PIN_WINDOW_ID) {
        crate::windows::show_and_activate_window(&window);
        debug!("[pin] the entry is open already: brought forward");
        return Ok(());
    }
    if building::on_worker() {
        building::build_once(PIN_WINDOW_ID, || create(app))?;
    } else {
        building::run_off_main(app, PIN_WINDOW_ID, |app| {
            if let Err(error) = open(&app) {
                error!("[pin] it could not be made: {error}");
            }
        });
    }
    Ok(())
}

fn create<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    info!("[pin] making the full-screen PIN entry");
    let shown = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let page_shown = shown.clone();
    let builder = crate::webview_env::adopt(tauri::WebviewWindowBuilder::new(
        app,
        PIN_WINDOW_ID,
        tauri::WebviewUrl::App("pin/index.html".into()),
    ))
    .title("PIN")
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(false)
    .disable_drag_drop_handler();
    // A measurement instance (SUNSHINE_GUI_TEST_OFFSCREEN, windows.rs `test_offscreen`) opens its windows outside every monitor: nothing of it shows on the desk of whoever uses
    // the machine. The entry is then a window of the size of a display, out of sight, instead of the display-covering one.
    let builder = if crate::windows::test_offscreen() { builder.inner_size(1920.0, 1080.0).position(6000.0, 100.0) } else { builder.fullscreen(true) };
    let window = builder
        .on_page_load(move |window, payload| {
            if payload.event() == PageLoadEvent::Finished {
                reveal(&window, &page_shown, "page loaded");
            }
        })
        .build()
        .map_err(|e| format!("the PIN window could not be made: {e}"))?;

    #[cfg(target_os = "windows")]
    crate::windows::configure_webview_security(&window);
    crate::windows::disable_context_menu(&window);

    GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    lifecycle(|l| l.open(Instant::now()));
    // a pairing the host reported before the window existed counts as seen
    let status = crate::tray::current_status();
    on_tray_state(app, status.as_deref());

    let safety = window.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SHOW_CAP).await;
        reveal(&safety, &shown, "cap");
    });
    let app_for_grace = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(PAIRING_GRACE + Duration::from_millis(250)).await;
        let status = crate::tray::current_status();
        on_tray_state(&app_for_grace, status.as_deref());
    });

    crate::power::refresh_ecoqos_state(app);
    info!("[pin] the PIN entry is made, it is shown when its page has loaded");
    Ok(())
}

/// Shows the window once, when its page has loaded or the cap has passed. A window that closes in the meantime is not shown (`show_and_activate_window` refuses it).
fn reveal<R: Runtime>(window: &WebviewWindow<R>, shown: &std::sync::atomic::AtomicBool, why: &str) {
    if shown.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    if matches!(window.is_visible(), Ok(true)) {
        return;
    }
    if why == "cap" {
        warn!("[pin] no first view after {} ms: showing the window", SHOW_CAP.as_millis());
    }
    crate::windows::show_and_activate_window(window);
    let _ = window.set_focus();
}

/// The host's status changed (tray.rs `apply_tray_state`), or the host is gone (`None`).
pub(crate) fn on_tray_state<R: Runtime>(app: &AppHandle<R>, status: Option<&str>) {
    let verdict = lifecycle(|l| l.observe(status, Instant::now()));
    if let Verdict::End(reason) = verdict {
        end(app, reason);
    }
}

fn end<R: Runtime>(app: &AppHandle<R>, reason: EndReason) {
    // one end per window: the lifecycle is closed before anything is told
    lifecycle(Lifecycle::closed);
    let Some(window) = app.get_webview_window(PIN_WINDOW_ID) else { return };
    info!("[pin] pairing is over ({}): the entry closes", reason.as_str());
    let _ = window.emit(PAIRING_ENDED_EVENT, reason.as_str());
    let app = app.clone();
    let generation = GENERATION.load(std::sync::atomic::Ordering::SeqCst);
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(END_FALLBACK).await;
        // closes it unless the page has closed it already, or a newer window has taken its place
        if GENERATION.load(std::sync::atomic::Ordering::SeqCst) == generation {
            closing::close_window(&app, PIN_WINDOW_ID, "pairing is over");
        }
    });
}

/// The page asks who is waiting for its PIN.
#[tauri::command]
pub fn pin_pairing_client() -> String {
    crate::tray::current_pairing_client()
}

/// What a click on a pairing notification does, for the isolated test instance (test_isolation.rs): the isolation has no notifications and no second launch, so it cannot receive the click
/// that opens the entry. Goes through the same door as the click, the tray's pairing item and `--url=…/pin` (`windows::open_pin_window`): the saved setting decides between the
/// full-screen entry and the panel's pairing page. Answers with the setting it followed. Refused outside a test profile.
#[tauri::command]
pub fn pin_window_selftest(app: AppHandle) -> Result<String, String> {
    if !crate::test_isolation::active() {
        return Err("pin_window_selftest runs only in a test profile".into());
    }
    crate::windows::open_pin_window(&app).map_err(|error| error.to_string())?;
    Ok(crate::pin_prompt::PinPrompt::current().as_str().to_owned())
}

/// The window event `Destroyed` of the entry.
pub(crate) fn destroyed() {
    lifecycle(Lifecycle::closed);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    #[test]
    fn a_closed_entry_ends_nothing() {
        let mut life = Lifecycle::default();
        let base = Instant::now();
        assert!(!life.is_open());
        assert_eq!(life.observe(Some("idle"), at(base, 60_000)), Verdict::Keep);
        assert_eq!(life.observe(None, at(base, 60_000)), Verdict::Keep);
    }

    #[test]
    fn it_ends_when_a_pending_pairing_is_over() {
        let mut life = Lifecycle::default();
        let base = Instant::now();
        life.open(base);
        assert_eq!(life.observe(Some("pairing"), at(base, 100)), Verdict::Keep);
        assert_eq!(life.observe(Some("pairing"), at(base, 5_000)), Verdict::Keep, "no limit while the request is pending");
        // succeeded, expired and cancelled all come back as another status
        assert_eq!(life.observe(Some("idle"), at(base, 6_000)), Verdict::End(EndReason::Finished));
        life.closed();
        life.open(base);
        life.observe(Some("pairing"), at(base, 1));
        assert_eq!(life.observe(Some("streaming"), at(base, 2)), Verdict::End(EndReason::Finished));
    }

    #[test]
    fn it_waits_for_a_host_that_has_not_reported_a_pairing_yet() {
        let mut life = Lifecycle::default();
        let base = Instant::now();
        life.open(base);
        assert_eq!(life.observe(Some("idle"), at(base, 100)), Verdict::Keep, "the request the address asked for may be registered a moment later");
        assert_eq!(life.observe(Some("idle"), at(base, 9_999)), Verdict::Keep);
        assert_eq!(life.observe(Some("idle"), at(base, 10_000)), Verdict::End(EndReason::NoRequest));
    }

    #[test]
    fn a_lost_host_ends_it_after_the_grace_or_a_pairing() {
        let mut life = Lifecycle::default();
        let base = Instant::now();
        life.open(base);
        assert_eq!(life.observe(None, at(base, 500)), Verdict::Keep, "the host may be starting");
        assert_eq!(life.observe(None, at(base, 10_000)), Verdict::End(EndReason::HostLost));
        life.open(base);
        life.observe(Some("pairing"), at(base, 1));
        assert_eq!(life.observe(None, at(base, 2)), Verdict::End(EndReason::HostLost), "a pending request is gone with the host");
    }

    #[test]
    fn a_window_that_is_opened_again_starts_over() {
        let mut life = Lifecycle::default();
        let base = Instant::now();
        life.open(base);
        life.observe(Some("pairing"), at(base, 1));
        life.closed();
        assert!(!life.is_open());
        life.open(at(base, 100));
        assert_eq!(life.observe(Some("idle"), at(base, 200)), Verdict::Keep, "the pairing of the window before does not count");
    }

    #[test]
    fn a_click_goes_to_the_full_screen_entry_only_while_a_pairing_can_still_be_answered() {
        assert_eq!(entry_for_click(Some("pairing")), Entry::FullScreen);
        assert_eq!(entry_for_click(None), Entry::FullScreen, "a click that started the panel: the host has not reported yet");
        assert_eq!(entry_for_click(Some("idle")), Entry::PanelPage);
        assert_eq!(entry_for_click(Some("streaming")), Entry::PanelPage);
    }

    #[test]
    fn the_setting_decides_where_a_pairing_request_is_answered() {
        use crate::pin_prompt::PinPrompt::{Fullscreen, Panel};
        // `panel`: the panel's pairing page, whatever the host reports
        for status in [Some("pairing"), None, Some("idle"), Some("streaming")] {
            assert_eq!(route(Panel, status), Entry::PanelPage, "{status:?}");
        }
        // `fullscreen` (the default): the full-screen entry while a request can be answered, the panel page for one that is over
        assert_eq!(route(Fullscreen, Some("pairing")), Entry::FullScreen);
        assert_eq!(route(Fullscreen, None), Entry::FullScreen);
        assert_eq!(route(Fullscreen, Some("idle")), Entry::PanelPage);
        // an unset or unreadable setting is `fullscreen`
        assert_eq!(route(crate::pin_prompt::PinPrompt::from_config(None), Some("pairing")), Entry::FullScreen);
    }

    #[test]
    fn the_selftest_is_for_a_test_profile_only() {
        // no test profile is active in the unit tests: the selftest source refuses, and the isolation lets exactly this command and the client name through
        assert!(!crate::test_isolation::active());
        let source = include_str!("pin_window.rs");
        assert!(source.contains("if !crate::test_isolation::active()"));
        assert!(crate::test_isolation::allows_command("pin_window_selftest"));
    }

    #[test]
    fn the_reasons_are_the_words_the_page_reads() {
        // renderer/pin/pinFlow.js ENDED_REASONS lists the same three
        assert_eq!(EndReason::Finished.as_str(), "finished");
        assert_eq!(EndReason::NoRequest.as_str(), "no-request");
        assert_eq!(EndReason::HostLost.as_str(), "host-lost");
    }
}
