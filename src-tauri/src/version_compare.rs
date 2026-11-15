//! The date part of a Foundation Sunshine version, the same rules as `src/version_compare.h` (host) and `sunshine_version.js` (Web UI); the three
//! share `tests/fixtures/version_vectors.json` (this crate keeps an identical copy in `src-tauri/tests/fixtures/`).
//!
//! A version is `YYYY.MDD.HMMSS`, written unpadded (CI tag `v2026.1103.21122`) or zero-padded (local build `2026.1112.025657`). What follows the
//! third part (`.杂鱼`, `.<commit>`, emoji, any text) never takes part in a comparison. `manual-YYYYMMDD-HHMMSS[-dev]` compares by its embedded
//! date; a string with no date version is unknown, which is never newer, older or equal.

use std::cmp::Ordering;

pub(crate) type VersionKey = [u64; 3];

/// A digit run of 1..=9 digits at `pos` (exactly `want` digits when `want` is not 0): the value and the position after it.
fn read_number(text: &[u8], pos: usize, want: usize) -> Option<(u64, usize)> {
    let digits = text[pos.min(text.len())..]
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if digits == 0 || digits > 9 || (want != 0 && digits != want) {
        return None;
    }
    let value = text[pos..pos + digits]
        .iter()
        .fold(0u64, |acc, byte| acc * 10 + u64::from(byte - b'0'));
    Some((value, pos + digits))
}

/// The first three parts of `version`, or `None` when it carries no date version.
pub(crate) fn version_key(version: &str) -> Option<VersionKey> {
    let mut text = version.trim_start_matches([' ', '\t', '\r', '\n']);
    text = text.strip_prefix(['v', 'V']).unwrap_or(text);
    let bytes = text.as_bytes();

    if bytes.len() >= 7 && bytes[..7].eq_ignore_ascii_case(b"manual-") {
        // manual-YYYYMMDD-HHMMSS[-dev]: the date is the version
        let (date, pos) = read_number(bytes, 7, 8)?;
        if bytes.get(pos) != Some(&b'-') {
            return None;
        }
        let (time, _) = read_number(bytes, pos + 1, 6)?;
        let (month, day) = (date / 100 % 100, date % 100);
        if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
            return None;
        }
        return Some([date / 10_000, date % 10_000, time]);
    }

    let mut key = [0u64; 3];
    let mut pos = 0;
    for (index, part) in key.iter_mut().enumerate() {
        let (value, next) = read_number(bytes, pos, 0)?;
        *part = value;
        pos = next;
        if index < 2 {
            if bytes.get(pos) != Some(&b'.') {
                return None;
            }
            pos += 1;
        }
    }
    Some(key)
}

/// Fork extension: the CI run of a fork release (`v<upstream base>.<run>.杂鱼`, AsafMah/foundation-sunshine), else 0. The fourth part counts
/// only when it is 1..=6 digits ending the string or followed by `.` (or white space), so a short commit (7+ characters) never does. Same
/// rule as `version_compare::fork_run` (host) and `forkRun` (Web UI); it orders only versions whose three-part keys are equal.
pub(crate) fn fork_run(version: &str) -> u64 {
    let mut text = version.trim_start_matches([' ', '\t', '\r', '\n']);
    text = text.strip_prefix(['v', 'V']).unwrap_or(text);
    let bytes = text.as_bytes();
    if bytes.len() >= 7 && bytes[..7].eq_ignore_ascii_case(b"manual-") {
        return 0;
    }
    let mut pos = 0;
    for index in 0..3 {
        let Some((_, next)) = read_number(bytes, pos, 0) else {
            return 0;
        };
        pos = next;
        if index < 2 {
            if bytes.get(pos) != Some(&b'.') {
                return 0;
            }
            pos += 1;
        }
    }
    if bytes.get(pos) != Some(&b'.') {
        return 0;
    }
    let start = pos + 1;
    let Some((run, end)) = read_number(bytes, start, 0) else {
        return 0;
    };
    if end - start > 6 {
        return 0;
    }
    match bytes.get(end) {
        None | Some(b'.' | b' ' | b'\t' | b'\r' | b'\n') => run,
        _ => 0,
    }
}

/// The ordering key of a version: its date key, then the fork run; `None` when it has no date version.
pub(crate) fn version_order(version: &str) -> Option<(VersionKey, u64)> {
    Some((version_key(version)?, fork_run(version)))
}

/// How `left` orders against `right`; `None` when either has no date version (unknown is never newer).
#[cfg(test)]
pub(crate) fn compare_versions(left: &str, right: &str) -> Option<Ordering> {
    Some(version_order(left)?.cmp(&version_order(right)?))
}

/// Whether `candidate` is a newer version than `installed`. An unreadable version on either side is no update.
pub(crate) fn is_newer(installed: &str, candidate: &str) -> bool {
    matches!(
        (version_order(installed), version_order(candidate)),
        (Some(installed), Some(candidate)) if candidate.cmp(&installed) == Ordering::Greater
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn vectors() -> Value {
        serde_json::from_str(include_str!("../tests/fixtures/version_vectors.json"))
            .expect("version_vectors.json")
    }

    #[test]
    fn the_shared_table_is_loaded() {
        let table = vectors();
        assert_eq!(table["schema"], 1);
        assert!(table["parse"].as_array().unwrap().len() >= 25);
        assert!(table["compare"].as_array().unwrap().len() >= 15);
    }

    #[test]
    fn version_key_matches_the_shared_parse_vectors() {
        for row in vectors()["parse"].as_array().unwrap() {
            let input = row["input"].as_str().unwrap();
            let want: Option<VersionKey> = row["key"].as_array().map(|parts| {
                [
                    parts[0].as_u64().unwrap(),
                    parts[1].as_u64().unwrap(),
                    parts[2].as_u64().unwrap(),
                ]
            });
            assert_eq!(version_key(input), want, "{input:?}");
        }
    }

    #[test]
    fn compare_matches_the_shared_compare_vectors() {
        for row in vectors()["compare"].as_array().unwrap() {
            let (left, right) = (
                row["left"].as_str().unwrap(),
                row["right"].as_str().unwrap(),
            );
            let want = row["order"].as_i64().map(|order| order.cmp(&0));
            assert_eq!(compare_versions(left, right), want, "{left:?} vs {right:?}");
            assert_eq!(
                compare_versions(right, left),
                want.map(Ordering::reverse),
                "{right:?} vs {left:?}"
            );
            // `right` is an update for an installed `left` only when it is strictly newer, never when either is unknown
            assert_eq!(is_newer(left, right), want == Some(Ordering::Less));
        }
    }

    #[test]
    fn fork_run_matches_the_fork_vectors() {
        let table = vectors();
        let rows = table["fork_run"].as_array().expect("fork_run vectors");
        assert!(rows.len() >= 5);
        for row in rows {
            let input = row["input"].as_str().unwrap();
            assert_eq!(fork_run(input), row["run"].as_u64().unwrap(), "{input:?}");
        }
        assert!(is_newer("v2026.1112.92926.20.杂鱼", "v2026.1112.92926.21.杂鱼"));
        assert!(!is_newer("v2026.1112.92926.21.杂鱼", "v2026.1112.92926.杂鱼"));
    }

    #[test]
    fn a_suffix_never_offers_an_update_and_an_unknown_installed_version_offers_none() {
        assert!(!is_newer("2026.1112.025657", "v2026.1112.025657.杂鱼"));
        assert!(!is_newer(
            "2026.1112.025657.970e0298b",
            "v2026.1112.25657.杂鱼"
        ));
        assert!(!is_newer("Unknown", "v2026.1112.25657.杂鱼"));
        assert!(is_newer("2026.1031.121458.杂鱼", "v2026.1111.173208.杂鱼"));
    }
}
