//! Startup hygiene for the WebView2 disk cache: after an upgrade the cache can hold responses of
//! the previous Web UI build, and the proxy answers hashed assets with "immutable", so the browser
//! reuses such an entry without revalidation even when the panel now serves correct bytes. A build
//! can also change the bytes under a URL that kept its name, and a broken build can have written
//! garbage outright (the 2026-11-22 incident: Brotli bodies served without their Content-Encoding
//! header, cached as JavaScript). Clearing the HTTP cache at startup after the panel executable
//! changed removes all of these at once.
//!
//! This runs before the first window is made, so no WebView2 process exists yet and no cache file
//! is locked. A marker beside EBWebView records the panel executable that produced the cache; a
//! different executable means "clear". The marker is only rewritten when every present cache
//! directory was removed, so a leftover browser process that locks a directory defers the marker
//! and the sweep retries on the next start. The previous marker is dropped as soon as the epoch
//! differs, so a rollback to the old executable cannot skip a cache that a failed sweep left.

use log::{info, warn};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tauri::Manager;

/// The marker sits beside EBWebView and records the panel executable that produced the cache.
const MARKER_FILE: &str = "cache-epoch.txt";

/// HTTP response cache and compiled-code caches only. Everything else in the profile (Local
/// Storage, IndexedDB, Session Storage, WebStorage) holds the user's settings and sign-in state
/// and stays.
const CACHE_SUBDIRS: [&str; 6] = ["Cache", "Code Cache", "GPUCache", "DawnWebGPUCache", "DawnGraphiteCache", "Shared Dictionary"];

/// What a sweep found, for the log line.
pub struct SweepResult {
    pub cleared: Vec<String>,
    pub skipped: bool,
    pub failures: Vec<String>,
}

/// The epoch of the running panel executable: size and last write time, so an installer that
/// replaces the panel changes it without any version plumbing (the panel builds carry no release
/// version in their own metadata).
pub fn epoch_of_current_panel() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let meta = fs::metadata(exe).ok()?;
    let modified = meta.modified().ok()?;
    let stamp = modified.duration_since(SystemTime::UNIX_EPOCH).map(|since| since.as_nanos().to_string()).unwrap_or_else(|_| String::from("pre-epoch"));
    Some(format!("{}:{}", meta.len(), stamp))
}

/// The WebView2 user data folder of the panel: the app local data dir, whose EBWebView
/// subdirectory the WebView2 runtime creates.
pub fn webview_user_data_root(app: &tauri::AppHandle) -> Option<PathBuf> {
    app.path().app_local_data_dir().ok()
}

/// Clears the HTTP/code caches under "<root>/EBWebView/Default" when the epoch differs from the
/// marker beside EBWebView. The old marker is dropped up front, so it can only survive this call
/// (and with it the previous epoch) when a failure was recorded.
pub fn sweep_stale_http_cache(root: &Path, epoch: &str) -> SweepResult {
    let marker = root.join(MARKER_FILE);
    if fs::read_to_string(&marker).is_ok_and(|written| written == epoch) {
        return SweepResult { cleared: Vec::new(), skipped: true, failures: Vec::new() };
    }
    let profile = root.join("EBWebView").join("Default");
    let mut cleared = Vec::new();
    let mut failures = Vec::new();
    if let Err(error) = fs::remove_file(&marker) {
        if error.kind() != std::io::ErrorKind::NotFound {
            failures.push(format!("{}: {error}", marker.display()));
        }
    }
    for name in CACHE_SUBDIRS {
        let dir = profile.join(name);
        match fs::remove_dir_all(&dir) {
            Ok(()) => cleared.push(name.to_string()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => failures.push(format!("{name}: {error}")),
        }
    }
    if failures.is_empty() {
        if let Some(parent) = marker.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Err(error) = fs::write(&marker, epoch) {
            failures.push(format!("{}: {error}", marker.display()));
        }
    }
    SweepResult { cleared, skipped: false, failures }
}

/// Called in the setup hook before any window exists.
pub fn clear_stale_cache_at_startup(app: &tauri::AppHandle) {
    let Some(root) = webview_user_data_root(app) else { return };
    let Some(epoch) = epoch_of_current_panel() else { return };
    let result = sweep_stale_http_cache(&root, &epoch);
    if result.skipped {
        return;
    }
    if !result.failures.is_empty() {
        warn!("[webview-cache] could not clear some cache directories (retried on the next start): {}", result.failures.join("; "));
    } else if result.cleared.is_empty() {
        info!("[webview-cache] new panel executable: nothing cached yet");
    } else {
        info!("[webview-cache] cleared the cache of the previous panel: {}", result.cleared.join(", "));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded_profile(root: &Path, name: &str) {
        let dir = root.join("EBWebView").join("Default").join(name);
        fs::create_dir_all(&dir).expect("cache dir");
        fs::write(dir.join("entry"), b"cached").expect("cache file");
    }

    fn marker_of(root: &Path) -> Option<String> {
        fs::read_to_string(root.join(MARKER_FILE)).ok()
    }

    #[test]
    fn first_run_clears_the_cache_and_writes_the_marker() {
        let root = tempfile::tempdir().expect("tempdir").keep();
        seeded_profile(&root, "Cache");
        let result = sweep_stale_http_cache(&root, "epoch-a");
        assert_eq!(result.cleared, vec![String::from("Cache")]);
        assert!(!result.skipped);
        assert!(result.failures.is_empty());
        assert!(!root.join("EBWebView").join("Default").join("Cache").exists());
        assert_eq!(marker_of(&root).as_deref(), Some("epoch-a"));
        fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn same_epoch_leaves_the_cache_alone() {
        let root = tempfile::tempdir().expect("tempdir").keep();
        fs::write(root.join(MARKER_FILE), "epoch-a").expect("marker");
        seeded_profile(&root, "Code Cache");
        let result = sweep_stale_http_cache(&root, "epoch-a");
        assert!(result.skipped);
        assert!(result.cleared.is_empty());
        assert!(root.join("EBWebView").join("Default").join("Code Cache").exists());
        fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn a_new_epoch_clears_again() {
        let root = tempfile::tempdir().expect("tempdir").keep();
        fs::write(root.join(MARKER_FILE), "epoch-a").expect("marker");
        seeded_profile(&root, "GPUCache");
        let result = sweep_stale_http_cache(&root, "epoch-b");
        assert_eq!(result.cleared, vec![String::from("GPUCache")]);
        assert_eq!(marker_of(&root).as_deref(), Some("epoch-b"));
        fs::remove_dir_all(&root).expect("cleanup");
    }

    #[cfg(windows)]
    #[test]
    fn a_locked_directory_defers_the_marker_to_the_next_start() {
        use std::os::windows::fs::OpenOptionsExt;
        let root = tempfile::tempdir().expect("tempdir").keep();
        let dir = root.join("EBWebView").join("Default").join("Cache");
        fs::create_dir_all(&dir).expect("cache dir");
        let stuck = dir.join("locked-file");
        // A handle opened with no sharing flags is what a running WebView2 process holds: the
        // directory cannot be removed while it exists.
        let holder = fs::OpenOptions::new().create(true).write(true).share_mode(0).open(&stuck).expect("open without sharing");
        let result = sweep_stale_http_cache(&root, "epoch-a");
        assert!(result.failures.iter().any(|failure| failure.starts_with("Cache:")));
        assert_eq!(marker_of(&root), None);
        drop(holder);
        fs::remove_dir_all(&root).expect("cleanup");
    }

    #[cfg(windows)]
    #[test]
    fn a_failed_sweep_deletes_the_previous_marker_so_a_rollback_retries() {
        use std::os::windows::fs::OpenOptionsExt;
        let root = tempfile::tempdir().expect("tempdir").keep();
        fs::write(root.join(MARKER_FILE), "epoch-a").expect("marker");
        let dir = root.join("EBWebView").join("Default").join("Cache");
        fs::create_dir_all(&dir).expect("cache dir");
        let stuck = dir.join("locked-file");
        let holder = fs::OpenOptions::new().create(true).write(true).share_mode(0).open(&stuck).expect("open without sharing");
        let result = sweep_stale_http_cache(&root, "epoch-b");
        assert!(result.failures.iter().any(|failure| failure.starts_with("Cache:")));
        // the previous epoch died with its marker: sweeping "epoch-a" again must not skip
        assert_eq!(marker_of(&root), None);
        drop(holder);
        fs::remove_dir_all(&root).expect("cleanup");
    }
}
