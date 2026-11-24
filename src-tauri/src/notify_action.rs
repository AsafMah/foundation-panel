//! What a click on a notification does, as a small address that survives the process.
//!
//! A toast is kept by Windows after it leaves the screen, and the notification centre can show it long after the Panel that sent it was restarted. A callback of the sending
//! process is gone by then, so the toast carries its own answer: a `sunshine-panel://notify/...` address (`launch` with `activationType="protocol"`, see
//! tray/notifications/toast.rs). Windows starts the registered handler of that scheme with the address as its argument; a running Panel receives it through the single-instance
//! hand-off (`app::handle_single_instance`), a Panel that is not running reads it at its start (`app::setup_application`). Both come to `NotifyAction::from_args`.
//!
//! Sources: panel-ms-uri-scheme, panel-tauri-single-instance. Design: docs/notification-click.md.
//!
//! The address is input from outside the process: any program or web page can start the handler of a scheme. Only the three shapes below are accepted, the page of
//! `page/<name>` must be one of `commands::PANEL_PAGES`, and nothing in an address is ever a path, a command or a URL to load.
//!
//!   sunshine-panel://notify/pin          the full-screen PIN entry
//!   sunshine-panel://notify/main         the main panel
//!   sunshine-panel://notify/page/<name>  a page of the main panel

/// The scheme this application registers for toast clicks (Windows: `HKCU\Software\Classes\sunshine-panel`).
pub const SCHEME: &str = "sunshine-panel";
const PREFIX: &str = "sunshine-panel://notify/";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotifyAction {
    /// The full-screen PIN entry (a pairing request is waiting).
    OpenPin,
    /// The main panel, when nothing more specific is known.
    OpenMain,
    /// One page of the main panel (a name of `commands::PANEL_PAGES`).
    OpenPage(String),
}

impl NotifyAction {
    /// The action of a notification that the host's tray state carries (`notification.action`).
    pub fn for_core(action: &str) -> Self {
        match action {
            "open_pin" => Self::OpenPin,
            "open_vdd_settings" => Self::OpenPage("vdd".into()),
            _ => Self::OpenMain,
        }
    }

    /// The address a toast carries for this action.
    pub fn to_uri(&self) -> String {
        match self {
            Self::OpenPin => format!("{PREFIX}pin"),
            Self::OpenMain => format!("{PREFIX}main"),
            Self::OpenPage(page) => format!("{PREFIX}page/{page}"),
        }
    }

    /// Reads one address. Anything that is not exactly one of the accepted shapes is `None`.
    pub fn parse(arg: &str) -> Option<Self> {
        let arg = arg.trim();
        // the scheme is case-insensitive (RFC 3986 section 3.1); the path is ours and is not
        let head = arg.get(..PREFIX.len())?;
        if !head.eq_ignore_ascii_case(PREFIX) {
            return None;
        }
        // Windows may add a closing slash when it hands over a URI
        let rest = arg[PREFIX.len()..].trim_end_matches('/');
        match rest {
            "pin" => Some(Self::OpenPin),
            "main" => Some(Self::OpenMain),
            _ => {
                let page = rest.strip_prefix("page/")?;
                crate::commands::PANEL_PAGES
                    .contains(&page)
                    .then(|| Self::OpenPage(page.to_owned()))
            }
        }
    }

    /// The action in a process's arguments: the first argument that is an address of ours.
    pub fn from_args<S: AsRef<str>>(args: &[S]) -> Option<Self> {
        args.iter().find_map(|arg| Self::parse(arg.as_ref()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_action_survives_its_own_address() {
        let mut all = vec![NotifyAction::OpenPin, NotifyAction::OpenMain];
        all.extend(crate::commands::PANEL_PAGES.iter().map(|page| NotifyAction::OpenPage((*page).to_owned())));
        for action in all {
            assert_eq!(NotifyAction::parse(&action.to_uri()), Some(action.clone()), "{action:?}");
        }
    }

    #[test]
    fn the_core_notifications_map_to_their_pages() {
        assert_eq!(NotifyAction::for_core("open_pin"), NotifyAction::OpenPin);
        assert_eq!(NotifyAction::for_core("open_vdd_settings"), NotifyAction::OpenPage("vdd".into()));
        // a notification without a more specific page, and one this build does not know, open the main panel
        assert_eq!(NotifyAction::for_core(""), NotifyAction::OpenMain);
        assert_eq!(NotifyAction::for_core("nr_active"), NotifyAction::OpenMain);
        assert_eq!(NotifyAction::for_core("a_newer_action"), NotifyAction::OpenMain);
    }

    #[test]
    fn the_page_a_core_notification_names_is_a_real_panel_page() {
        let NotifyAction::OpenPage(page) = NotifyAction::for_core("open_vdd_settings") else { panic!("a page") };
        assert!(crate::commands::PANEL_PAGES.contains(&page.as_str()));
    }

    #[test]
    fn windows_may_change_the_scheme_case_and_add_a_closing_slash() {
        assert_eq!(NotifyAction::parse("SUNSHINE-PANEL://notify/pin/"), Some(NotifyAction::OpenPin));
        assert_eq!(NotifyAction::parse(" sunshine-panel://notify/main "), Some(NotifyAction::OpenMain));
        assert_eq!(NotifyAction::parse("sunshine-panel://notify/page/vdd/"), Some(NotifyAction::OpenPage("vdd".into())));
    }

    #[test]
    fn only_the_accepted_shapes_are_actions() {
        for refused in [
            "",
            "sunshine-panel://",
            "sunshine-panel://notify/",
            "sunshine-panel://notify/PIN",
            "sunshine-panel://notify/pin/extra",
            "sunshine-panel://notify/pin?x=1",
            "sunshine-panel://notify/page/",
            "sunshine-panel://notify/page/not-a-page",
            "sunshine-panel://notify/page/../pin",
            "sunshine-panel://notify/page/vdd/extra",
            "sunshine-panel://other/pin",
            "sunshine-panel:notify/pin",
            "https://example.test/sunshine-panel://notify/pin",
            "--url=sunshine-panel://notify/pin",
            "é",
        ] {
            assert_eq!(NotifyAction::parse(refused), None, "{refused:?}");
        }
    }

    #[test]
    fn the_action_is_found_among_the_arguments_of_a_launch() {
        let args = ["C:\\Panel\\sunshine-gui.exe", "--minimized", "sunshine-panel://notify/pin/"];
        assert_eq!(NotifyAction::from_args(&args), Some(NotifyAction::OpenPin));
        assert_eq!(NotifyAction::from_args(&["sunshine-gui.exe", "--desktop"]), None);
        assert_eq!(NotifyAction::from_args::<&str>(&[]), None);
    }
}
