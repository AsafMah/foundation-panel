//! Starting the Sparxie window from the Control Panel: which Windows verb to use, and what a failed start means.
//!
//! Sparxie keeps `config.json`, its plugins and its logs in the folder that holds `Sparxie.exe`. The component
//! lives under the host's own install folder (`...\Program Files\...`), where a standard user may read but not
//! write. A Sparxie started without administrator rights then stops at once with a message that it needs them,
//! because it has no folder it may write to. Neither `Sparxie.exe` nor `Sparxie.Desktop.exe` asks for elevation in
//! its manifest, so Windows does not raise a UAC prompt by itself either.
//!
//! The Control Panel therefore checks the same fact Sparxie checks (can this process create a file in that folder?)
//! and uses the `runas` verb only when it cannot. A folder the person can write to is started with `open`, so
//! the window does not run elevated without a reason.
//!
//! The decisions live here, free of Windows calls, so they are tested on every platform. The call to
//! `ShellExecuteExW` is in `utils::open_sparxie`.
//!
//! Sources (Microsoft Learn, read 2026-11-23):
//! - `runas` verb ("Launches an application as Administrator. User Account Control (UAC) will prompt the user for consent"):
//!   <https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shellexecutew>
//! - `ShellExecuteExW` returns `ERROR_CANCELLED` when the person cancels the prompt it showed:
//!   <https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shellexecuteexw>
//! - `requestedExecutionLevel` `asInvoker` runs at the level of the starting process; only `requireAdministrator` makes the system ask:
//!   <https://learn.microsoft.com/en-us/windows/win32/sbscs/application-manifests>
//! - `ERROR_ELEVATION_REQUIRED` (740), "The requested operation requires elevation":
//!   <https://learn.microsoft.com/en-us/windows/win32/debug/system-error-codes--500-999->

#![cfg_attr(not(target_os = "windows"), allow(dead_code))]

use std::path::{Path, PathBuf};

/// `ERROR_ELEVATION_REQUIRED`: the program's manifest asks for rights the caller does not have.
pub(crate) const ERROR_ELEVATION_REQUIRED: u32 = 740;
/// `ERROR_CANCELLED`: the person closed the UAC prompt or refused it.
pub(crate) const ERROR_CANCELLED: u32 = 1223;
/// `SE_ERR_ACCESSDENIED` in the legacy `hInstApp` status of `ShellExecute`.
pub(crate) const SE_ERR_ACCESSDENIED: isize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StartVerb {
    /// Start it as the signed-in person, like double-clicking it in Explorer.
    Open,
    /// Ask Windows for the person's consent (UAC) and start it elevated.
    Runas,
}

impl StartVerb {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            StartVerb::Open => "open",
            StartVerb::Runas => "runas",
        }
    }
}

/// How a failed start is to be treated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StartFailure {
    /// The person refused the UAC prompt. Not an error: nothing is shown.
    Declined,
    /// Windows says the program needs administrator rights and `open` does not give them: ask again with `runas`.
    NeedsElevation,
    /// Anything else; carries the Windows code for the message.
    Other(u32),
}

/// The folder Sparxie writes to. A packaged build runs from `app-<version>` inside the data folder, so a path that
/// points into it maps to its parent.
pub(crate) fn data_directory(exe: &Path) -> PathBuf {
    let parent = exe.parent().unwrap_or(exe);
    let in_app_folder = parent
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.to_ascii_lowercase().starts_with("app-"));
    match (in_app_folder, parent.parent()) {
        (true, Some(data)) => data.to_path_buf(),
        _ => parent.to_path_buf(),
    }
}

/// Can this process create (and remove) a file in `dir`? This is Sparxie's own test of its data folder, so the
/// answer matches what Sparxie will find once it runs under the same token.
pub(crate) fn directory_is_writable(dir: &Path) -> bool {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    let probe = dir.join(format!(".panel-write-test-{}-{unique}", std::process::id()));
    let created = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .is_ok();
    if created {
        let _ = std::fs::remove_file(&probe);
    }
    created
}

/// `runas` when Sparxie would find its folder read-only, `open` otherwise. A Control Panel that already runs
/// elevated passes its token on, whatever the folder says.
pub(crate) fn choose_verb(panel_is_elevated: bool, data_dir_writable: bool) -> StartVerb {
    if panel_is_elevated || data_dir_writable {
        StartVerb::Open
    } else {
        StartVerb::Runas
    }
}

/// The Win32 error behind an `HRESULT_FROM_WIN32` value (`0x8007xxxx`), which is how the Windows bindings report
/// the last error of a failed call. Anything else has no Win32 code.
pub(crate) fn win32_error_from_hresult(hresult: i32) -> Option<u32> {
    let value = hresult as u32;
    (value >> 16 == 0x8007).then_some(value & 0xFFFF)
}

/// Reads a failed `ShellExecuteExW`: `win32_error` is its last error (0 when unknown), `legacy_status` the
/// `hInstApp` field it also fills.
pub(crate) fn classify_failure(verb: StartVerb, win32_error: u32, legacy_status: isize) -> StartFailure {
    if win32_error == ERROR_CANCELLED {
        return StartFailure::Declined;
    }
    let refused_for_rights = win32_error == ERROR_ELEVATION_REQUIRED || legacy_status == SE_ERR_ACCESSDENIED;
    if verb == StartVerb::Open && refused_for_rights {
        return StartFailure::NeedsElevation;
    }
    let code = if win32_error != 0 {
        win32_error
    } else {
        u32::try_from(legacy_status).unwrap_or_default()
    };
    StartFailure::Other(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_read_only_folder_is_started_with_the_uac_prompt() {
        assert_eq!(choose_verb(false, false), StartVerb::Runas);
    }

    #[test]
    fn a_writable_folder_is_not_started_elevated() {
        assert_eq!(choose_verb(false, true), StartVerb::Open);
    }

    #[test]
    fn an_elevated_panel_passes_its_token_on_without_a_second_prompt() {
        assert_eq!(choose_verb(true, false), StartVerb::Open);
        assert_eq!(choose_verb(true, true), StartVerb::Open);
    }

    #[test]
    fn a_refused_prompt_is_not_an_error() {
        assert_eq!(classify_failure(StartVerb::Runas, ERROR_CANCELLED, 0), StartFailure::Declined);
        assert_eq!(classify_failure(StartVerb::Open, ERROR_CANCELLED, 0), StartFailure::Declined);
    }

    #[test]
    fn a_program_that_demands_rights_is_asked_for_again_with_runas() {
        assert_eq!(
            classify_failure(StartVerb::Open, ERROR_ELEVATION_REQUIRED, 0),
            StartFailure::NeedsElevation
        );
        assert_eq!(
            classify_failure(StartVerb::Open, 0, SE_ERR_ACCESSDENIED),
            StartFailure::NeedsElevation
        );
    }

    #[test]
    fn runas_never_asks_again() {
        assert_eq!(
            classify_failure(StartVerb::Runas, ERROR_ELEVATION_REQUIRED, 0),
            StartFailure::Other(ERROR_ELEVATION_REQUIRED)
        );
        assert_eq!(
            classify_failure(StartVerb::Runas, 0, SE_ERR_ACCESSDENIED),
            StartFailure::Other(5)
        );
    }

    #[test]
    fn other_failures_keep_their_code() {
        assert_eq!(classify_failure(StartVerb::Open, 2, 0), StartFailure::Other(2));
        assert_eq!(classify_failure(StartVerb::Open, 0, 31), StartFailure::Other(31));
    }

    #[test]
    fn hresult_from_win32_gives_the_win32_code_back() {
        assert_eq!(win32_error_from_hresult(0x800704C7_u32 as i32), Some(ERROR_CANCELLED));
        assert_eq!(win32_error_from_hresult(0x800702E4_u32 as i32), Some(ERROR_ELEVATION_REQUIRED));
        assert_eq!(win32_error_from_hresult(0x80004005_u32 as i32), None);
        assert_eq!(win32_error_from_hresult(0), None);
    }

    #[test]
    fn the_data_folder_is_the_one_holding_the_launcher() {
        assert_eq!(
            data_directory(Path::new("C:/host/components/sparxie/Sparxie.exe")),
            PathBuf::from("C:/host/components/sparxie")
        );
    }

    #[test]
    fn a_program_inside_the_app_folder_maps_to_its_parent() {
        assert_eq!(
            data_directory(Path::new("C:/host/components/sparxie/app-0.1.0/Sparxie.Desktop.exe")),
            PathBuf::from("C:/host/components/sparxie")
        );
    }

    #[test]
    fn a_folder_that_only_starts_like_app_is_not_one() {
        assert_eq!(
            data_directory(Path::new("C:/host/apps/Sparxie.exe")),
            PathBuf::from("C:/host/apps")
        );
    }

    #[test]
    fn a_writable_folder_is_detected_and_left_clean() {
        let dir = std::env::temp_dir().join(format!("sparxie-open-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(directory_is_writable(&dir));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0, "the probe file stays behind");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_missing_folder_is_not_writable() {
        let dir = std::env::temp_dir().join(format!("sparxie-open-missing-{}", std::process::id()));
        assert!(!directory_is_writable(&dir));
    }

    #[cfg(unix)]
    #[test]
    fn a_folder_without_write_permission_is_not_writable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("sparxie-open-ro-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        // root ignores the mode bits: only assert for an ordinary user
        let writable = directory_is_writable(&dir);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        if std::env::var("USER").map_or(true, |user| user != "root") {
            assert!(!writable);
        }
    }
}
