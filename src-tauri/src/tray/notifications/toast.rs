//! The toast, as the XML Windows shows, and the address handler that answers its click.
//!
//! The click is answered by protocol activation: the toast's `launch` is an address of this application (notify_action.rs) with `activationType="protocol"`, and the scheme is
//! registered per user (`HKCU\Software\Classes\sunshine-panel`) to start this executable with the address. Windows keeps the toast in the notification centre, and the click there
//! starts the handler even when the panel that sent the toast has gone: the callback of an in-process listener (`Toast::on_activated`) was lost with the process.
//! A running panel receives the second start through the single-instance hand-off, a panel that is not running reads the address at its start.
//!
//! Sources: panel-ms-toast-activation-schema, panel-ms-toast-element-toast, panel-ms-uri-scheme; why not a COM activator: docs/notification-click.md (panel-ms-toast-quickstart).
//!
//! The text of a toast and the path of the icon come from the host and from this process; everything that enters the XML is escaped.

use std::path::Path;

pub(super) struct ToastXml<'a> {
    pub title: &'a str,
    pub body: &'a str,
    /// The address of the click (`NotifyAction::to_uri`).
    pub launch: &'a str,
    pub icon: Option<&'a Path>,
    pub alt_text: &'a str,
}

fn escape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // the characters XML 1.0 does not allow in a document at all
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

/// The XML of a `ToastGeneric` toast: a round icon, a title, a body, the default sound, and the click that launches `launch` through its protocol.
pub(super) fn toast_xml(toast: &ToastXml<'_>) -> String {
    let mut xml = format!(
        r#"<toast launch="{}" activationType="protocol"><visual><binding template="ToastGeneric">"#,
        escape(toast.launch)
    );
    if let Some(icon) = toast.icon {
        xml.push_str(&format!(
            r#"<image placement="appLogoOverride" hint-crop="circle" src="file:///{}" alt="{}"/>"#,
            escape(&icon.display().to_string()),
            escape(toast.alt_text)
        ));
    }
    xml.push_str(&format!("<text>{}</text>", escape(toast.title)));
    if !toast.body.trim().is_empty() {
        xml.push_str(&format!("<text>{}</text>", escape(toast.body)));
    }
    xml.push_str("</binding></visual></toast>");
    xml
}

/// The values of the scheme's registration for the executable `exe`: (subkey under the scheme's key, value name, value). The empty name is the key's default value.
pub(super) fn protocol_registry_values(scheme: &str, exe: &str) -> Vec<(String, &'static str, String)> {
    vec![
        (String::new(), "", format!("URL:{scheme} notification")),
        (String::new(), "URL Protocol", String::new()),
        (r"DefaultIcon".to_owned(), "", format!("\"{exe}\",0")),
        // the address goes in quoted: it is the one argument, whatever it contains (Registering an Application to a URI Scheme)
        (r"shell\open\command".to_owned(), "", format!("\"{exe}\" \"%1\"")),
    ]
}

#[cfg(target_os = "windows")]
pub(super) fn ensure_protocol_registered() -> Result<(), String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let exe = std::env::current_exe()
        .map_err(|error| format!("Failed to resolve the executable for the notification scheme: {error}"))?
        .to_string_lossy()
        .to_string();
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let scheme = crate::notify_action::SCHEME;
    for (subkey, name, value) in protocol_registry_values(scheme, &exe) {
        let path = if subkey.is_empty() {
            format!(r"Software\Classes\{scheme}")
        } else {
            format!(r"Software\Classes\{scheme}\{subkey}")
        };
        let (key, _) = hkcu
            .create_subkey(&path)
            .map_err(|error| format!("Failed to register the notification scheme: {error}"))?;
        // written only when it differs: the registration is checked at every start and a moved executable repairs it
        if key.get_value::<String, _>(name).ok().as_deref() != Some(value.as_str()) {
            key.set_value(name, &value)
                .map_err(|error| format!("Failed to register the notification scheme: {error}"))?;
        }
    }
    Ok(())
}

/// Shows a toast from its XML. Blocking COM calls: run it on the notification worker, in a COM apartment.
#[cfg(target_os = "windows")]
pub(super) fn show_xml(app_id: &str, xml: &str) -> Result<(), String> {
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
    use windows::core::HSTRING;

    let describe = |what: &str, error: windows::core::Error| format!("Failed to {what}: {error}");
    let document = XmlDocument::new().map_err(|e| describe("create the toast document", e))?;
    document
        .LoadXml(&HSTRING::from(xml))
        .map_err(|e| describe("read the toast XML", e))?;
    let toast = ToastNotification::CreateToastNotification(&document).map_err(|e| describe("create the toast", e))?;
    ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(app_id))
        .map_err(|e| describe("reach the toast notifier", e))?
        .Show(&toast)
        .map_err(|e| describe("show the toast", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn a_toast_carries_the_address_of_its_click_and_activates_by_protocol() {
        let icon = PathBuf::from("C:/Users/x/icon.ico");
        let xml = toast_xml(&ToastXml {
            title: "Pairing request",
            body: "Enter the PIN",
            launch: "sunshine-panel://notify/pin",
            icon: Some(&icon),
            alt_text: "Sunshine",
        });
        assert!(xml.starts_with(r#"<toast launch="sunshine-panel://notify/pin" activationType="protocol">"#), "{xml}");
        assert!(xml.contains("<text>Pairing request</text><text>Enter the PIN</text>"));
        assert!(xml.contains(r#"hint-crop="circle" src="file:///C:/Users/x/icon.ico""#));
        assert!(xml.ends_with("</binding></visual></toast>"));
    }

    #[test]
    fn what_comes_from_a_client_cannot_break_out_of_the_document() {
        let xml = toast_xml(&ToastXml {
            title: "\"><toast launch=\"evil\" & '<b>",
            body: "line\u{1}break",
            launch: "sunshine-panel://notify/pin\" activationType=\"foreground",
            icon: None,
            alt_text: "",
        });
        assert!(!xml.contains("<toast launch=\"evil\""), "{xml}");
        assert!(!xml.contains("<b>"), "{xml}");
        assert!(xml.contains("&quot;&gt;&lt;toast launch=&quot;evil&quot; &amp; &apos;&lt;b&gt;"));
        assert!(xml.contains("<text>linebreak</text>"), "a control character is dropped: {xml}");
        // the launch attribute stays one attribute whatever it holds
        assert_eq!(xml.matches("activationType=\"").count(), 1, "{xml}");
    }

    #[test]
    fn an_empty_body_makes_no_empty_line() {
        let xml = toast_xml(&ToastXml { title: "Client connected", body: "  ", launch: "sunshine-panel://notify/main", icon: None, alt_text: "" });
        assert_eq!(xml.matches("<text>").count(), 1, "{xml}");
        assert!(!xml.contains("<image"));
    }

    #[test]
    fn the_scheme_is_registered_with_the_quoted_address_argument() {
        let values = protocol_registry_values("sunshine-panel", r"C:\Program Files\Sunshine\sunshine-gui.exe");
        let command = values.iter().find(|(subkey, _, _)| subkey == r"shell\open\command").expect("the open command");
        assert_eq!(command.2, r#""C:\Program Files\Sunshine\sunshine-gui.exe" "%1""#);
        assert!(values.iter().any(|(subkey, name, value)| subkey.is_empty() && *name == "URL Protocol" && value.is_empty()), "without URL Protocol the handler does not launch");
        assert!(values.iter().any(|(subkey, name, _)| subkey == "DefaultIcon" && name.is_empty()));
    }
}
