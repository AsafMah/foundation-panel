//! Release repository of this build. Official builds follow AlkaidLab/foundation-sunshine (metadata mirrors, CNB, Mirror酱 and GitHub);
//! a fork build (FOUNDATION_RELEASE_REPOSITORY at build time, see build.rs) follows only its own repository's GitHub releases, so the
//! updater never offers an official build over the fork.

pub(crate) const OWNER: &str = env!("SUNSHINE_RELEASE_OWNER");
pub(crate) const REPOSITORY: &str = env!("SUNSHINE_RELEASE_REPO");
pub(crate) const FORK: bool = matches!(env!("SUNSHINE_FORK_RELEASES").as_bytes(), b"1");

pub(crate) const GITHUB_LATEST_URL: &str = concat!(
    "https://api.github.com/repos/",
    env!("SUNSHINE_RELEASE_OWNER"),
    "/",
    env!("SUNSHINE_RELEASE_REPO"),
    "/releases/latest"
);
pub(crate) const GITHUB_RELEASES_URL: &str = concat!(
    "https://api.github.com/repos/",
    env!("SUNSHINE_RELEASE_OWNER"),
    "/",
    env!("SUNSHINE_RELEASE_REPO"),
    "/releases?per_page=30"
);
pub(crate) const GITHUB_RELEASE_TAG_BASE: &str = concat!(
    "https://github.com/",
    env!("SUNSHINE_RELEASE_OWNER"),
    "/",
    env!("SUNSHINE_RELEASE_REPO"),
    "/releases/tag"
);
pub(crate) const GITHUB_RELEASE_DOWNLOAD_BASE: &str = concat!(
    "https://github.com/",
    env!("SUNSHINE_RELEASE_OWNER"),
    "/",
    env!("SUNSHINE_RELEASE_REPO"),
    "/releases/download"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_follow_the_configured_repository() {
        let path = format!("{OWNER}/{REPOSITORY}");
        for url in [GITHUB_LATEST_URL, GITHUB_RELEASES_URL, GITHUB_RELEASE_TAG_BASE, GITHUB_RELEASE_DOWNLOAD_BASE] {
            assert!(url.contains(&path), "{url}");
        }
        assert_eq!(FORK, path != "AlkaidLab/foundation-sunshine");
    }
}
