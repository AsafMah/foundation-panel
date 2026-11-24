use log::{debug, warn};
use std::sync::Mutex;
use tauri::{AppHandle, Runtime};
#[cfg(not(target_os = "windows"))]
use tauri_plugin_notification::NotificationExt;

#[cfg(target_os = "windows")]
use std::{path::PathBuf, sync::OnceLock};

#[cfg(any(target_os = "windows", test))]
mod worker;
mod pairing_texts;
mod toast;
use crate::notify_action::NotifyAction;

#[cfg(target_os = "windows")]
static WINDOWS_NOTIFICATION_WORKER: once_cell::sync::OnceCell<worker::Worker> =
    once_cell::sync::OnceCell::new();

#[cfg(target_os = "windows")]
static WINDOWS_AUMID_REGISTRATION: OnceLock<Result<(), String>> = OnceLock::new();
#[cfg(target_os = "windows")]
static WINDOWS_NOTIFICATION_ICON: OnceLock<Result<PathBuf, String>> = OnceLock::new();
type NotificationKey = (String, u64);
// One reservation covers both queued and delivered notifications. Failures
// release only their own key, so an older job cannot clear a newer reservation.
static LAST_NOTIFICATION: Mutex<Option<NotificationKey>> = Mutex::new(None);

#[cfg(target_os = "windows")]
const WINDOWS_NOTIFICATION_ICON_BYTES: &[u8] = include_bytes!("../../icons/tray/sunshine.ico");
#[cfg(target_os = "windows")]
const WINDOWS_NOTIFICATION_ICON_FILE: &str = "notification-icon-v2.ico";

#[derive(Clone, Debug, PartialEq, Eq)]
enum ConnectionChange {
    Connected {
        client_name: Option<String>,
        highly_suspected_unknown_client: bool,
    },
    Disconnected(Option<String>),
}

#[derive(Debug, PartialEq, Eq)]
struct NotificationContent {
    title: String,
    body: String,
}

#[cfg(target_os = "windows")]
pub(super) fn ensure_windows_notification_icon() -> Result<PathBuf, String> {
    WINDOWS_NOTIFICATION_ICON
        .get_or_init(|| {
            let directory = crate::test_isolation::data_local_dir()
                .ok_or_else(|| "Failed to resolve local application data directory".to_string())?
                .join("Sunshine GUI");
            std::fs::create_dir_all(&directory).map_err(|error| {
                format!("Failed to create notification asset directory: {error}")
            })?;

            let path = directory.join(WINDOWS_NOTIFICATION_ICON_FILE);
            let icon_is_current = std::fs::read(&path)
                .map(|bytes| bytes == WINDOWS_NOTIFICATION_ICON_BYTES)
                .unwrap_or(false);
            if !icon_is_current {
                std::fs::write(&path, WINDOWS_NOTIFICATION_ICON_BYTES)
                    .map_err(|error| format!("Failed to write notification icon: {error}"))?;
            }
            Ok(path)
        })
        .clone()
}

#[cfg(target_os = "windows")]
fn ensure_windows_aumid_registered<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    WINDOWS_AUMID_REGISTRATION
        .get_or_init(|| {
            use winreg::RegKey;
            use winreg::enums::HKEY_CURRENT_USER;

            let identifier = app.config().identifier.clone();
            let icon_uri = ensure_windows_notification_icon()?
                .to_string_lossy()
                .to_string();
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            let (key, _) = hkcu
                .create_subkey(format!(r"Software\Classes\AppUserModelId\{identifier}"))
                .map_err(|error| format!("Failed to register notification AUMID: {error}"))?;

            key.set_value("DisplayName", &"Sunshine")
                .and_then(|_| key.set_value("IconBackgroundColor", &"0"))
                .and_then(|_| key.set_value("IconUri", &icon_uri))
                .map_err(|error| format!("Failed to configure notification AUMID: {error}"))
        })
        .clone()
}

pub(super) fn initialize<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        ensure_windows_aumid_registered(app)?;
        // an isolated measurement instance must not point the person's toast clicks at itself
        if !crate::test_isolation::active() {
            toast::ensure_protocol_registered()?;
        }
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = app;
        Ok(())
    }
}

/// The panel's code for a locale name of Sunshine's settings, when the panel has that language.
pub(super) fn pairing_code_for(locale: &str) -> Option<&'static str> {
    pairing_texts::code_for(locale)
}

/// Show a native desktop notification using the same user preference as the
/// rest of the GUI notification surface. Tray menu state remains available
/// even when native notifications are disabled.
fn is_enabled() -> bool {
    if crate::test_isolation::active() {
        return false;
    }
    crate::desktop_settings::load_desktop_settings_from_disk().notifications
}

fn connection_is_enabled() -> bool {
    if crate::test_isolation::active() {
        return false;
    }
    let settings = crate::desktop_settings::load_desktop_settings_from_disk();
    settings.notifications && settings.connection_notify
}

const fn supports_actions() -> bool {
    cfg!(target_os = "windows")
}

fn supports_session_snapshots(state: &crate::sunshine::TrayState) -> bool {
    state
        .capabilities
        .iter()
        .any(|capability| capability == "sessions-v1")
}

fn connection_changes(
    previous: Option<&crate::sunshine::TrayState>,
    current: &crate::sunshine::TrayState,
) -> Vec<ConnectionChange> {
    let Some(previous) = previous else {
        return Vec::new();
    };
    if previous.instance_id != current.instance_id {
        return Vec::new();
    }

    if supports_session_snapshots(previous) && supports_session_snapshots(current) {
        let mut changes = Vec::new();
        for session in &current.sessions {
            if !previous
                .sessions
                .iter()
                .any(|previous_session| previous_session.id == session.id)
            {
                changes.push(ConnectionChange::Connected {
                    client_name: (!session.client_name.trim().is_empty())
                        .then(|| session.client_name.trim().to_string()),
                    highly_suspected_unknown_client: session.highly_suspected_unknown_client,
                });
            }
        }
        for session in &previous.sessions {
            if !current
                .sessions
                .iter()
                .any(|current_session| current_session.id == session.id)
            {
                changes.push(ConnectionChange::Disconnected(
                    (!session.client_name.trim().is_empty())
                        .then(|| session.client_name.trim().to_string()),
                ));
            }
        }
        changes
    } else if matches!(previous.status.as_str(), "idle" | "paused") && current.status == "streaming"
    {
        vec![ConnectionChange::Connected {
            client_name: None,
            highly_suspected_unknown_client: false,
        }]
    } else if previous.status == "streaming" && matches!(current.status.as_str(), "idle" | "paused")
    {
        vec![ConnectionChange::Disconnected(None)]
    } else {
        Vec::new()
    }
}

/// A click on "client connected" shows the running session; on "disconnected" there is none, so the main panel opens.
fn connection_action(change: &ConnectionChange) -> NotifyAction {
    match change {
        ConnectionChange::Connected { .. } => NotifyAction::OpenPage("live-session".into()),
        ConnectionChange::Disconnected(_) => NotifyAction::OpenMain,
    }
}

fn connection_content(
    strings: &super::TrayStrings,
    change: ConnectionChange,
) -> NotificationContent {
    match change {
        ConnectionChange::Connected {
            client_name,
            highly_suspected_unknown_client,
        } => {
            let mut body = client_name
                .map(|name| {
                    strings
                        .client_connected_named
                        .replace("{name}", &super::menu::compact_menu_text(&name, 64))
                })
                .unwrap_or_else(|| strings.client_connected_detail.to_string());
            if highly_suspected_unknown_client {
                body.push('\n');
                body.push_str(strings.suspicious_client_warning);
            }
            NotificationContent {
                title: strings.client_connected.to_string(),
                body,
            }
        }
        ConnectionChange::Disconnected(client_name) => NotificationContent {
            title: strings.client_disconnected.to_string(),
            body: client_name
                .map(|name| {
                    strings
                        .client_disconnected_named
                        .replace("{name}", &super::menu::compact_menu_text(&name, 64))
                })
                .unwrap_or_else(|| strings.client_disconnected_detail.to_string()),
        },
    }
}

fn core_notification_content(
    strings: &super::TrayStrings,
    locale: &str,
    state: &crate::sunshine::TrayState,
    supports_actions: bool,
) -> NotificationContent {
    let notification = &state.notification;
    if notification.action == "nr_active" {
        return NotificationContent { title: "Sunshine · DLSS NR".into(), body: strings.nr_active.into() };
    }
    if notification.action == "open_pin" {
        // the pairing request is in the language of the host, one of the 20 of the panel (pairing_texts.rs)
        let texts = pairing_texts::texts(locale);
        let client_name = state.pairing_client_name.trim();
        let title = if client_name.is_empty() {
            texts.incoming_pairing.to_string()
        } else {
            format!(
                "{} · {}",
                texts.incoming_pairing,
                super::menu::compact_menu_text(client_name, 64)
            )
        };
        return NotificationContent {
            title,
            body: if supports_actions {
                texts.pairing_instruction
            } else {
                texts.pairing_menu_instruction
            }
            .to_string(),
        };
    }

    NotificationContent {
        title: if notification.title.trim().is_empty() {
            strings.notification.to_string()
        } else {
            notification.title.trim().to_string()
        },
        body: notification.message.trim().to_string(),
    }
}

pub(super) fn show_connection_change_if_any<R: Runtime>(
    app: &AppHandle<R>,
    previous: Option<&crate::sunshine::TrayState>,
    current: &crate::sunshine::TrayState,
) {
    if !connection_is_enabled() {
        return;
    }

    let strings = super::get_tray_strings();
    for change in connection_changes(previous, current) {
        let action = connection_action(&change);
        let content = connection_content(strings, change);
        if let Err(error) = show(app, content, action, connection_is_enabled, None) {
            warn!("{}", error);
        }
    }
}

pub(super) fn show_core_notification_if_new<R: Runtime>(
    app: &AppHandle<R>,
    state: &crate::sunshine::TrayState,
) {
    let notification = &state.notification;
    if !notification.active || notification.id == 0 || !is_enabled() {
        return;
    }

    let key = (state.instance_id.clone(), notification.id);
    {
        let mut last = LAST_NOTIFICATION.lock().unwrap();
        if last.as_ref() == Some(&key) {
            return;
        }
        // Reserve before enqueueing: a fast worker failure must be the last
        // writer, rather than being overwritten by the enqueueing thread.
        *last = Some(key.clone());
    }

    let supports_actions = supports_actions();
    let content = core_notification_content(super::get_tray_strings(), super::notification_locale(), state, supports_actions);
    // every notification answers its click (a toast that left the screen is in the notification centre, and its click is answered from there too)
    let action = NotifyAction::for_core(&notification.action);
    match show(app, content, action, is_enabled, Some(key)) {
        Ok(()) => {
            debug!("Queued native tray notification {}", notification.id);
        }
        Err(error) => warn!("{}", error),
    }
}

fn forget_notification(key: &Option<NotificationKey>) {
    if let Some(key) = key {
        let mut last = LAST_NOTIFICATION.lock().unwrap();
        if last.as_ref() == Some(key) {
            *last = None;
        }
    }
}

fn deliver_notification(
    key: &Option<NotificationKey>,
    enabled: impl FnOnce() -> bool,
    send: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    if !enabled() {
        forget_notification(key);
        return Ok(());
    }
    let result = send();
    if result.is_err() {
        forget_notification(key);
    }
    result
}

fn show<R: Runtime>(
    app: &AppHandle<R>,
    content: NotificationContent,
    action: NotifyAction,
    enabled: fn() -> bool,
    key: Option<NotificationKey>,
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let result = (|| {
        // Build COM objects on the serial worker. A blocked Windows service
        // must not hold up native window or tray event delivery.
        // Retry transient thread-creation failures on the next notification.
        let worker = WINDOWS_NOTIFICATION_WORKER
            .get_or_try_init(|| worker::Worker::new("sunshine-notifications", 32))?;
        let app = app.clone();
        let key = key.clone();
        worker.submit(move || {
            let started = std::time::Instant::now();
            if let Err(error) = deliver_notification(&key, enabled, || {
                show_windows(&app, &content.title, &content.body, &action)
            }) {
                warn!("{error}");
            }
            if started.elapsed() >= std::time::Duration::from_secs(2) {
                warn!(
                    "Windows notification submission took {:?}",
                    started.elapsed()
                );
            }
        })
    })();

    #[cfg(not(target_os = "windows"))]
    let result = deliver_notification(&key, enabled, || {
        let mut builder = app
            .notification()
            .builder()
            .title(&content.title)
            .sound("Default");
        if !content.body.trim().is_empty() {
            builder = builder.body(&content.body);
        }
        let _ = action;
        builder
            .show()
            .map_err(|error| format!("Failed to show native tray notification: {error}"))
    });

    #[cfg(target_os = "windows")]
    if result.is_err() {
        forget_notification(&key); // Worker initialization/queue rejection can fail too.
    }
    result
}

#[cfg(target_os = "windows")]
fn show_windows<R: Runtime>(
    app: &AppHandle<R>,
    title: &str,
    body: &str,
    action: &NotifyAction,
) -> Result<(), String> {
    use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};

    struct ComApartment;
    impl Drop for ComApartment {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
        .ok()
        .map_err(|error| format!("Failed to initialize notification COM apartment: {error}"))?;
    let _apartment = ComApartment;

    // Unpackaged apps must register the same AUMID used by the toast backend.
    ensure_windows_aumid_registered(app)?;

    let icon_path = ensure_windows_notification_icon()?;
    let launch = action.to_uri();
    let xml = toast::toast_xml(&toast::ToastXml {
        title,
        body,
        launch: &launch,
        icon: Some(&icon_path),
        alt_text: "Sunshine",
    });
    toast::show_xml(&app.config().identifier, &xml)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queued_notifications_recheck_preferences_and_release_only_failed_reservations() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
            mpsc,
        };
        use std::time::Duration;

        let worker = worker::Worker::new("notification-preferences-test", 2).unwrap();
        let (started, ready) = mpsc::channel();
        let (release, blocked) = mpsc::channel();
        worker
            .submit(move || {
                started.send(()).unwrap();
                blocked.recv().unwrap();
            })
            .unwrap();
        ready.recv_timeout(Duration::from_secs(2)).unwrap();

        let key = Some(("core".into(), 1));
        *LAST_NOTIFICATION.lock().unwrap() = key.clone();
        let enabled = Arc::new(AtomicBool::new(true));
        let preference = enabled.clone();
        let queued_key = key.clone();
        let (done, completed) = mpsc::channel();
        worker
            .submit(move || {
                let result = deliver_notification(
                    &queued_key,
                    || preference.load(Ordering::Acquire),
                    || panic!("a notification disabled while queued must not be shown"),
                );
                done.send(result).unwrap();
            })
            .unwrap();
        enabled.store(false, Ordering::Release);
        release.send(()).unwrap();
        completed
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap();
        assert!(LAST_NOTIFICATION.lock().unwrap().is_none());

        // Reserving before enqueueing lets even a fast worker failure release
        // the key; no caller assignment may overwrite that failure afterward.
        *LAST_NOTIFICATION.lock().unwrap() = key.clone();
        let queued_key = key.clone();
        let (done, completed) = mpsc::channel();
        worker
            .submit(move || {
                done.send(deliver_notification(
                    &queued_key,
                    || true,
                    || Err("service unavailable".into()),
                ))
                .unwrap();
            })
            .unwrap();
        assert!(
            completed
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .is_err()
        );
        assert!(LAST_NOTIFICATION.lock().unwrap().is_none());

        let newer = Some(("core".into(), 2));
        *LAST_NOTIFICATION.lock().unwrap() = newer.clone();
        assert!(deliver_notification(&key, || true, || Err("older job failed".into())).is_err());
        assert_eq!(*LAST_NOTIFICATION.lock().unwrap(), newer);
        deliver_notification(&newer, || true, || Ok(())).unwrap();
        assert_eq!(*LAST_NOTIFICATION.lock().unwrap(), newer);
        *LAST_NOTIFICATION.lock().unwrap() = None;
    }

    fn tray_state(status: &str, instance_id: &str) -> crate::sunshine::TrayState {
        crate::sunshine::TrayState {
            status: status.to_string(),
            instance_id: instance_id.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn connection_changes_use_session_snapshot_names() {
        let mut idle = tray_state("idle", "core-instance");
        idle.capabilities.push("sessions-v1".to_string());
        let mut connected = tray_state("streaming", "core-instance");
        connected.capabilities.push("sessions-v1".to_string());
        connected.sessions.push(crate::sunshine::TrayClientSession {
            id: 7,
            client_name: "Living Room TV".to_string(),
            highly_suspected_unknown_client: false,
        });

        assert_eq!(
            connection_changes(Some(&idle), &connected),
            vec![ConnectionChange::Connected {
                client_name: Some("Living Room TV".to_string()),
                highly_suspected_unknown_client: false,
            }]
        );
        assert_eq!(
            connection_changes(Some(&connected), &idle),
            vec![ConnectionChange::Disconnected(Some(
                "Living Room TV".to_string()
            ))]
        );
    }

    #[test]
    fn connection_changes_fall_back_to_streaming_transitions() {
        let idle = tray_state("idle", "core-instance");
        let streaming = tray_state("streaming", "core-instance");
        let paused = tray_state("paused", "core-instance");
        let notification = tray_state("notification", "core-instance");

        assert_eq!(
            connection_changes(Some(&idle), &streaming),
            vec![ConnectionChange::Connected {
                client_name: None,
                highly_suspected_unknown_client: false,
            }]
        );
        assert_eq!(
            connection_changes(Some(&streaming), &paused),
            vec![ConnectionChange::Disconnected(None)]
        );
        assert!(connection_changes(Some(&streaming), &notification).is_empty());
    }

    #[test]
    fn connection_changes_ignore_initial_state_and_core_restart() {
        let streaming = tray_state("streaming", "core-instance");
        let restarted = tray_state("streaming", "new-core-instance");

        assert!(connection_changes(None, &streaming).is_empty());
        assert!(connection_changes(Some(&streaming), &restarted).is_empty());
    }

    #[test]
    fn connection_copy_has_a_clear_title_and_named_detail() {
        assert_eq!(
            connection_content(
                &super::super::ZH_STRINGS,
                ConnectionChange::Connected {
                    client_name: Some("客厅电视".to_string()),
                    highly_suspected_unknown_client: false,
                }
            ),
            NotificationContent {
                title: "客户端已连接".to_string(),
                body: "「客厅电视」已连接到这台电脑。".to_string(),
            }
        );
        assert_eq!(
            connection_content(
                &super::super::EN_STRINGS,
                ConnectionChange::Disconnected(None)
            ),
            NotificationContent {
                title: "Client disconnected".to_string(),
                body: "The streaming connection has ended.".to_string(),
            }
        );
    }

    #[test]
    fn suspicious_client_warning_is_appended_to_connection_notification() {
        assert_eq!(
            connection_content(
                &super::super::ZH_STRINGS,
                ConnectionChange::Connected {
                    client_name: Some("可疑客户端".to_string()),
                    highly_suspected_unknown_client: true,
                }
            ),
            NotificationContent {
                title: "客户端已连接".to_string(),
                body: concat!(
                    "「可疑客户端」已连接到这台电脑。\n",
                    "警告：您很可能正在受到未知侵权客户端的侵害。",
                    "该判断基于客户端网络特征，可能存在误判。"
                )
                .to_string(),
            }
        );
    }

    #[test]
    fn the_pairing_request_speaks_the_language_of_the_host() {
        let mut state = tray_state("pairing", "core-instance");
        state.pairing_client_name = "客厅电视".to_string();
        state.notification.action = "open_pin".to_string();
        let de = core_notification_content(&super::super::EN_STRINGS, "de", &state, true);
        assert_eq!(de.title, "Moonlight-Kopplungsanfrage · 客厅电视");
        assert!(de.body.starts_with("Auf diese Benachrichtigung klicken"), "{}", de.body);
        // without a click that opens the entry the text points to the tray menu instead
        let no_click = core_notification_content(&super::super::EN_STRINGS, "de", &state, false);
        assert!(no_click.body.contains("Complete Pairing"), "{}", no_click.body);
        // a language the panel does not have, and one that was never set, are English
        assert_eq!(core_notification_content(&super::super::EN_STRINGS, "he", &state, true).title, "Moonlight pairing request · 客厅电视");
    }

    #[test]
    fn a_click_on_a_connection_toast_opens_the_session_or_the_panel() {
        let connected = ConnectionChange::Connected { client_name: None, highly_suspected_unknown_client: false };
        assert_eq!(connection_action(&connected), NotifyAction::OpenPage("live-session".into()));
        assert_eq!(connection_action(&ConnectionChange::Disconnected(None)), NotifyAction::OpenMain);
    }

    #[test]
    fn every_core_notification_has_a_click_that_a_launch_can_read_back() {
        for action in ["open_pin", "open_vdd_settings", "nr_active", "", "something_newer"] {
            let click = NotifyAction::for_core(action);
            assert_eq!(NotifyAction::parse(&click.to_uri()), Some(click), "{action:?}");
        }
        assert_eq!(NotifyAction::for_core("open_pin"), NotifyAction::OpenPin);
    }

    #[test]
    fn pairing_copy_names_the_client_and_explains_the_click_action() {
        let mut state = tray_state("pairing", "core-instance");
        state.pairing_client_name = "Living Room TV".to_string();
        state.notification.action = "open_pin".to_string();

        assert_eq!(
            core_notification_content(&super::super::EN_STRINGS, "en", &state, true),
            NotificationContent {
                title: "Moonlight pairing request · Living Room TV".to_string(),
                body: "Click this notification to enter the 4-digit PIN shown in Moonlight."
                    .to_string(),
            }
        );
    }
}
