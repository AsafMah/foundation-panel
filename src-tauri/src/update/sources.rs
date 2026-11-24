//! Which download sources may carry a given release.
//!
//! The version the person is offered and the file that gets downloaded must be the same release. Every source (CNB, GitHub,
//! Mirror酱) syncs on its own clock, so a source that still lists the previous build is skipped for this release, with the reason,
//! and the next source that lists exactly this release is used. Nothing here touches the network: the callers hand in what each
//! source lists, which keeps the rules testable.

use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// What the person is installing: the release tag and the size and SHA-256 its own metadata gives for the installer.
#[derive(Clone, Debug)]
pub(crate) struct Target {
    /// `None` when the caller did not say which release this is: addresses are then not compared by tag.
    pub(crate) tag: Option<String>,
    pub(crate) size: Option<u64>,
    pub(crate) sha256: String,
}

/// One address a download could come from, with what the source itself lists for it. `None` means the source does not say
/// (Mirror酱 addresses carry a token and no tag): the downloaded bytes are then the only proof, and they are checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Offer {
    pub(crate) name: &'static str,
    pub(crate) url: String,
    pub(crate) listed_tag: Option<String>,
    pub(crate) listed_size: Option<u64>,
    pub(crate) listed_sha256: Option<String>,
}

impl Offer {
    /// An address of a release asset: the tag is read from the path (`.../download/<tag>/<file>`), so an address that belongs to
    /// another release can never be taken for this one.
    pub(crate) fn from_release_url(name: &'static str, url: &str) -> Self {
        Self {
            name,
            url: url.to_string(),
            listed_tag: tag_of_release_url(url),
            listed_size: None,
            listed_sha256: None,
        }
    }
}

/// The tag segment of a release asset address (`.../releases/download/<tag>/<file>`), decoded.
pub(crate) fn tag_of_release_url(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    let segments = parsed.path_segments()?.collect::<Vec<_>>();
    let download = segments.iter().position(|segment| *segment == "download")?;
    if segments.len() != download + 3 {
        return None;
    }
    super::metadata::decode_segment(segments[download + 1]).ok()
}

/// Why a source is not used for this release. The words are language neutral: the renderer writes the sentences.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SkipReason {
    /// The source lists another release (it has not synced this one yet).
    NotSynced,
    /// The source lists this release with another size.
    SizeMismatch,
    /// The source lists this release with another SHA-256.
    HashMismatch,
    /// The source could not be asked (no answer, or an answer that could not be read).
    Unreachable,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Skipped {
    pub(crate) source: &'static str,
    pub(crate) reason: SkipReason,
    /// The release the source lists instead, when it says one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) listed: Option<String>,
}

#[derive(Debug, Default)]
pub(crate) struct Plan {
    /// In the order they are tried.
    pub(crate) usable: Vec<Offer>,
    pub(crate) skipped: Vec<Skipped>,
}

fn same_sha256(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

/// Splits the offers into the ones that may carry `target` and the ones that may not. The order of `offers` is kept, except that a
/// source whose file already failed the size or SHA-256 check for this release (`bad`) goes last: it is tried again only after the others.
pub(crate) fn plan(target: &Target, offers: Vec<Offer>, bad: &[&str]) -> Plan {
    let mut plan = Plan::default();
    for offer in offers {
        if let (Some(listed), Some(tag)) = (offer.listed_tag.as_deref(), target.tag.as_deref())
            && listed != tag
        {
            plan.skipped.push(Skipped {
                source: offer.name,
                reason: SkipReason::NotSynced,
                listed: Some(listed.to_string()),
            });
            continue;
        }
        if let (Some(listed), Some(expected)) = (offer.listed_size, target.size)
            && listed != expected
        {
            plan.skipped.push(Skipped {
                source: offer.name,
                reason: SkipReason::SizeMismatch,
                listed: offer.listed_tag.clone(),
            });
            continue;
        }
        if let Some(listed) = offer.listed_sha256.as_deref()
            && !same_sha256(listed, &target.sha256)
        {
            plan.skipped.push(Skipped {
                source: offer.name,
                reason: SkipReason::HashMismatch,
                listed: offer.listed_tag.clone(),
            });
            continue;
        }
        plan.usable.push(offer);
    }
    // stable: sources that are not known to hold another file keep their order
    plan.usable.sort_by_key(|offer| bad.contains(&offer.name));
    plan
}

/// Sources whose file failed the size or SHA-256 check, per release (keyed by the release's installer SHA-256). A retry puts them behind
/// the sources that have not failed; a release that downloaded fine forgets its entry.
static BAD_SOURCES: OnceLock<Mutex<HashMap<String, Vec<&'static str>>>> = OnceLock::new();

fn bad_sources() -> &'static Mutex<HashMap<String, Vec<&'static str>>> {
    BAD_SOURCES.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn bad_for(sha256: &str) -> Vec<&'static str> {
    bad_sources()
        .lock()
        .ok()
        .and_then(|map| map.get(&sha256.to_ascii_lowercase()).cloned())
        .unwrap_or_default()
}

pub(crate) fn mark_bad(sha256: &str, source: &'static str) {
    if let Ok(mut map) = bad_sources().lock() {
        let entry = map.entry(sha256.to_ascii_lowercase()).or_default();
        if !entry.contains(&source) {
            entry.push(source);
        }
    }
}

pub(crate) fn forget(sha256: &str) {
    if let Ok(mut map) = bad_sources().lock() {
        map.remove(&sha256.to_ascii_lowercase());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OLD: &str = "v2026.1122.165529.杂鱼";
    const NEW: &str = "v2026.1123.20745.杂鱼";
    const SHA_NEW: &str = "ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12";
    const SHA_OLD: &str = "cd34cd34cd34cd34cd34cd34cd34cd34cd34cd34cd34cd34cd34cd34cd34cd34";

    fn url(host: &str, tag: &str) -> String {
        let tag = tag.replace('杂', "%E6%9D%82").replace('鱼', "%E9%B1%BC");
        match host {
            "cnb.cool" => format!(
                "https://cnb.cool/AlkaidLab/foundation-sunshine-release/-/releases/download/{tag}/Sunshine.WindowsInstaller.exe"
            ),
            _ => format!(
                "https://github.com/AlkaidLab/foundation-sunshine/releases/download/{tag}/Sunshine.WindowsInstaller.exe"
            ),
        }
    }

    fn target() -> Target {
        Target {
            tag: Some(NEW.to_string()),
            size: Some(42_534_326),
            sha256: SHA_NEW.to_string(),
        }
    }

    fn names(plan: &Plan) -> Vec<&'static str> {
        plan.usable.iter().map(|offer| offer.name).collect()
    }

    #[test]
    fn the_tag_is_read_from_the_address_and_decoded() {
        assert_eq!(tag_of_release_url(&url("cnb.cool", NEW)).as_deref(), Some(NEW));
        assert_eq!(tag_of_release_url(&url("github.com", OLD)).as_deref(), Some(OLD));
        assert_eq!(tag_of_release_url("https://mirrorchyan.com/resources/download/token"), None);
    }

    /// The failure of 2026-11-23: the person was offered the new release while CNB still listed the previous build. An address of the
    /// previous release is never used for the new one, however the two were combined.
    #[test]
    fn a_cnb_address_of_the_previous_release_is_not_used_for_the_new_one() {
        let offers = vec![
            Offer::from_release_url("CNB", &url("cnb.cool", OLD)),
            Offer::from_release_url("GitHub", &url("github.com", NEW)),
        ];
        let plan = plan(&target(), offers, &[]);
        assert_eq!(names(&plan), ["GitHub"]);
        assert_eq!(
            plan.skipped,
            [Skipped { source: "CNB", reason: SkipReason::NotSynced, listed: Some(OLD.to_string()) }]
        );
    }

    #[test]
    fn a_cnb_that_has_synced_keeps_its_place_in_front() {
        let offers = vec![
            Offer::from_release_url("CNB", &url("cnb.cool", NEW)),
            Offer::from_release_url("GitHub", &url("github.com", NEW)),
        ];
        let plan = plan(&target(), offers, &[]);
        assert_eq!(names(&plan), ["CNB", "GitHub"]);
        assert!(plan.skipped.is_empty());
    }

    #[test]
    fn a_source_that_lists_the_release_with_another_size_or_hash_is_skipped() {
        let mut wrong_size = Offer::from_release_url("CNB", &url("cnb.cool", NEW));
        wrong_size.listed_size = Some(42_528_017);
        let mut wrong_hash = Offer::from_release_url("Mirror酱", &url("github.com", NEW));
        wrong_hash.listed_sha256 = Some(SHA_OLD.to_string());
        let mut right = Offer::from_release_url("GitHub", &url("github.com", NEW));
        right.listed_size = Some(42_534_326);
        right.listed_sha256 = Some(SHA_NEW.to_uppercase());
        let plan = plan(&target(), vec![wrong_size, wrong_hash, right], &[]);
        assert_eq!(names(&plan), ["GitHub"]);
        let reasons: Vec<_> = plan.skipped.iter().map(|skipped| (skipped.source, skipped.reason)).collect();
        assert_eq!(reasons, [("CNB", SkipReason::SizeMismatch), ("Mirror酱", SkipReason::HashMismatch)]);
    }

    #[test]
    fn every_source_lagging_leaves_nothing_to_download_from() {
        let offers = vec![
            Offer::from_release_url("CNB", &url("cnb.cool", OLD)),
            Offer::from_release_url("GitHub", &url("github.com", OLD)),
        ];
        let plan = plan(&target(), offers, &[]);
        assert!(plan.usable.is_empty());
        assert_eq!(plan.skipped.len(), 2);
    }

    #[test]
    fn a_source_whose_file_failed_before_goes_behind_the_others() {
        let offers = vec![
            Offer::from_release_url("CNB", &url("cnb.cool", NEW)),
            Offer::from_release_url("GitHub", &url("github.com", NEW)),
        ];
        let plan = plan(&target(), offers, &["CNB"]);
        assert_eq!(names(&plan), ["GitHub", "CNB"]);
    }

    #[test]
    fn a_failed_source_is_remembered_per_release_until_the_release_downloads() {
        let sha = "ef56ef56ef56ef56ef56ef56ef56ef56ef56ef56ef56ef56ef56ef56ef56ef56";
        assert!(bad_for(sha).is_empty());
        mark_bad(sha, "CNB");
        mark_bad(&sha.to_uppercase(), "CNB");
        assert_eq!(bad_for(sha), ["CNB"]);
        assert!(bad_for(SHA_OLD).is_empty());
        forget(sha);
        assert!(bad_for(sha).is_empty());
    }
}
