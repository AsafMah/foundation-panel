//! Explicit, user-confirmed installation of the pinned usbip-win2 transport.
//!
//! This runs only inside the elevated helper, after the Device Hub asked the
//! person to confirm the installation. The installer is staged in a directory
//! that only administrators can write, hashed against the pinned digest and
//! only then executed.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) enum InstallResult {
    Ready,
    RebootRecommended,
}

/// Inno Setup exits with the code requested by `/RESTARTEXITCODE`.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn classify_installer_exit_code(
    exit_code: Option<i32>,
) -> Result<InstallResult, String> {
    match exit_code {
        Some(0) => Ok(InstallResult::Ready),
        Some(3010) => Ok(InstallResult::RebootRecommended),
        code => Err(format!(
            "USBIP-SETUP-010: the USB/IP installer failed with exit code {}",
            code.unwrap_or(-1)
        )),
    }
}

/// What `usbip-package.ps1 -Action replace` answers. 107 is the host refusing before it touched anything; 102 and 103
/// come from the root-hub guard's rollback. Everything else is a failure whose real state the panel reads afterwards.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn classify_replace_exit_code(exit_code: Option<i32>) -> Result<InstallResult, String> {
    match exit_code {
        Some(0) => Ok(InstallResult::Ready),
        Some(3010) => Ok(InstallResult::RebootRecommended),
        Some(107) => Err("USBIP-REPLACE-001: this USB/IP installation cannot be replaced from here; nothing was changed".to_string()),
        Some(102) => Err("USBIP-REPLACE-002: the replacement was rolled back and the USB root hubs are healthy".to_string()),
        Some(103) => Err("USBIP-REPLACE-003: the USB root hubs were not fully recovered after the replacement".to_string()),
        code => Err(format!(
            "USBIP-REPLACE-004: the replacement failed with exit code {}",
            code.unwrap_or(-1)
        )),
    }
}

/// True when the host's `status` answer says this is an external installation the person may ask to replace.
/// Anything unexpected is "no": the button only appears on a positive, well-formed answer.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn parse_host_replaceable(output: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(output.trim().trim_start_matches('\u{feff}'))
        .map(|value| {
            value["ownership"].as_str() == Some("external")
                && value["replaceable"].as_bool() == Some(true)
        })
        .unwrap_or(false)
}

/// Directory next to the Sunshine installation where the installer is staged.
#[cfg(target_os = "windows")]
pub(crate) fn staging_root() -> std::path::PathBuf {
    crate::sunshine::install_dir()
        .join("tools")
        .join("usbip-staging")
}

/// Refuses to touch the transport while a stream may be using it.
#[cfg(target_os = "windows")]
pub(crate) async fn ensure_no_active_session() -> Result<(), String> {
    let sessions = crate::sunshine::get_active_sessions()
        .await
        .map_err(|error| format!("USBIP-SETUP-006: unable to verify stream ownership: {error}"))?;
    if sessions.is_empty() {
        Ok(())
    } else {
        Err(
            "USBIP-SETUP-006: finish the active Sunshine stream before changing the transport"
                .to_string(),
        )
    }
}

/// Reuses a healthy pinned transport or installs the pinned release.
/// An existing incompatible installation needs the shared cleanup first.
#[cfg(target_os = "windows")]
pub(crate) async fn ensure_pinned_usbip(
    _staging_root: &std::path::Path,
) -> Result<InstallResult, String> {
    use super::InstallDisposition;

    match super::install_disposition(super::installed_usbip_version()?.as_deref())? {
        InstallDisposition::Ready => {
            let status = super::usbip_get_status().await?;
            if !status.ready {
                return Err(format!(
                    "USBIP-SETUP-008: existing USB/IP transport needs repair before reuse: {}",
                    status.detail
                ));
            }
            return Ok(InstallResult::Ready);
        }
        InstallDisposition::Install => {}
    }

    run_host_package("install").await
}

/// Whether the host's lifecycle helper reports the installed USB/IP as one the person may replace. The ownership
/// rules live only in that helper, so the panel asks it instead of keeping a third copy.
#[cfg(target_os = "windows")]
pub(super) async fn host_replaceable() -> bool {
    let script = crate::sunshine::install_dir()
        .join("scripts")
        .join("usbip-package.ps1");
    let Some(root) = std::env::var_os("SystemRoot") else {
        return false;
    };
    if !script.is_file() {
        return false;
    }
    let powershell =
        std::path::PathBuf::from(root).join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    let mut command = tokio::process::Command::new(powershell);
    command
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .args(["-Action", "status"])
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .creation_flags(windows::Win32::System::Threading::CREATE_NO_WINDOW.0);
    let Ok(child) = command.spawn() else {
        return false;
    };
    match tokio::time::timeout(std::time::Duration::from_secs(30), child.wait_with_output()).await {
        Ok(Ok(output)) if output.status.success() => {
            parse_host_replaceable(&String::from_utf8_lossy(&output.stdout))
        }
        _ => false,
    }
}

/// Host-supplied lifecycle helper owns the global mutex through hub verification.
#[cfg(target_os = "windows")]
pub(super) async fn run_host_package(action: &str) -> Result<InstallResult, String> {
    // Use the host's immutable signed payload and its root-hub transaction.
    // No runtime installer or vendor setup executable is downloaded by the panel.
    let script = crate::sunshine::install_dir()
        .join("scripts")
        .join("usbip-package.ps1");
    if !script.is_file() {
        return Err(
            "USBIP-SETUP-002: update the host to install the bundled USB/IP component".into(),
        );
    }
    // `replace` takes over an installation the person confirmed; it needs the same verified payload as an install.
    if action == "install" || action == "replace" {
        super::manager::verify_slim_payload(
            &crate::sunshine::install_dir().join("scripts").join("usbip"),
        )?;
    }
    let powershell = std::path::PathBuf::from(
        std::env::var_os("SystemRoot")
            .ok_or("USBIP-SETUP-002: Windows directory is unavailable")?,
    )
    .join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    let mut command = tokio::process::Command::new(powershell);
    command
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .args(["-Action", action])
        // panel-usbip-tokio-20261117, kill_on_drop: closing the UI must not
        // interrupt the child guard between driver mutation and hub recovery.
        .kill_on_drop(false)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(windows::Win32::System::Threading::CREATE_NO_WINDOW.0);
    let mut child = command
        .spawn()
        .map_err(|error| format!("USBIP-SETUP-010: USB/IP setup did not start: {error}"))?;
    let status = tokio::time::timeout(std::time::Duration::from_secs(600), child.wait())
        .await
        .map_err(|_| "USBIP-SETUP-010: USB/IP setup timed out".to_string())?
        .map_err(|error| format!("USBIP-SETUP-010: USB/IP setup failed: {error}"))?;
    let result = if action == "replace" {
        classify_replace_exit_code(status.code())?
    } else {
        classify_installer_exit_code(status.code())?
    };
    if action == "install" || action == "replace" {
        super::manager::find_installation()?;
        super::manager::verify_driver_compatibility()?;
        if action == "replace"
            && super::usbip_uninstall_entries()?
                .iter()
                .any(|entry| !entry.slim)
        {
            return Err(
                "USBIP-REPLACE-004: the old USB/IP registration remains after the replacement"
                    .into(),
            );
        }
    } else if !super::usbip_uninstall_entries()?.is_empty() {
        return Err("USBIP-CLEAN-001: USB/IP registration remains after guarded cleanup".into());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installer_exit_codes_map_to_ready_or_reboot() {
        assert_eq!(
            classify_installer_exit_code(Some(0)),
            Ok(InstallResult::Ready)
        );
        assert_eq!(
            classify_installer_exit_code(Some(3010)),
            Ok(InstallResult::RebootRecommended)
        );
    }

    #[test]
    fn replace_exit_codes_map_to_results_the_person_can_act_on() {
        assert_eq!(
            classify_replace_exit_code(Some(0)),
            Ok(InstallResult::Ready)
        );
        assert_eq!(
            classify_replace_exit_code(Some(3010)),
            Ok(InstallResult::RebootRecommended)
        );
        for (code, expected) in [
            (Some(107), "USBIP-REPLACE-001:"),
            (Some(102), "USBIP-REPLACE-002:"),
            (Some(103), "USBIP-REPLACE-003:"),
            (Some(1), "USBIP-REPLACE-004:"),
            (Some(106), "USBIP-REPLACE-004:"),
            (None, "USBIP-REPLACE-004:"),
        ] {
            let error = classify_replace_exit_code(code).unwrap_err();
            assert!(error.starts_with(expected), "{code:?}: {error}");
        }
    }

    #[test]
    fn host_status_offers_the_replacement_only_on_a_positive_well_formed_answer() {
        let external = r#"{"ownership":"external","compatible":true,"reason":"gui-or-sdk","ready":true,"replaceable":true,"version":"0.9.8.1"}"#;
        assert!(parse_host_replaceable(external));
        // Windows PowerShell may prefix a byte order mark and end with a newline.
        assert!(parse_host_replaceable(&format!("\u{feff}{external}\r\n")));
        for refused in [
            r#"{"ownership":"external","replaceable":false}"#,
            r#"{"ownership":"managed","replaceable":true}"#,
            r#"{"ownership":"legacy-owned","replaceable":true}"#,
            r#"{"ownership":"external"}"#,
            r#"{"ownership":"external","replaceable":"true"}"#,
            "not json",
            "",
        ] {
            assert!(!parse_host_replaceable(refused), "{refused}");
        }
    }

    #[test]
    fn installer_failure_carries_a_usbip_code() {
        for code in [Some(1), Some(-1), None] {
            let error = classify_installer_exit_code(code).unwrap_err();
            assert!(error.starts_with("USBIP-SETUP-010:"), "{error}");
        }
    }
}
