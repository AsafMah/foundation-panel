//! Where a pairing request asks for its PIN: the full-screen entry (`pin_window.rs`, the default) or the main panel's own "pairing" page.
//!
//! One value drives both user interfaces: the host's configuration key `pin_prompt` (`fullscreen` | `panel`, Foundation Sunshine's `config.cpp`). The Web UI edits it with the other host
//! settings and the panel's settings page writes it through the same `POST /api/config`; the panel reads it from the saved `sunshine.conf` at the moment a request needs it, like it reads
//! the host's language (`SunshineConfig`), so a change made in either place is in force at the next pairing request without a restart of either.
//! The host's configuration is the only copy: the panel keeps no setting of its own for it (`desktop-settings.json` holds the panel's own behaviour; this one is the host's).

/// The values the host accepts for `pin_prompt` (Foundation Sunshine validates the same two).
pub(crate) const FULLSCREEN: &str = "fullscreen";
pub(crate) const PANEL: &str = "panel";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum PinPrompt {
    #[default]
    Fullscreen,
    Panel,
}

impl PinPrompt {
    /// The setting as the configuration file holds it. A missing value, an empty one, and one that is not a value of the setting are the default: the host
    /// replaces such a value by the default as well (`string_restricted_f`).
    pub(crate) fn from_config(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some(PANEL) => Self::Panel,
            _ => Self::Fullscreen,
        }
    }

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Fullscreen => FULLSCREEN,
            Self::Panel => PANEL,
        }
    }

    /// What the host's saved configuration says now. An unreadable file is the default: a pairing request must always find somewhere to be answered.
    pub(crate) fn current() -> Self {
        match crate::sunshine::parse_sunshine_config_sync() {
            Ok(config) => Self::from_config(config.pin_prompt.as_deref()),
            Err(error) => {
                log::warn!("pin_prompt: the host configuration could not be read ({error}): the full-screen entry is used");
                Self::default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_values_of_the_host_and_the_default() {
        assert_eq!(PinPrompt::from_config(Some("fullscreen")), PinPrompt::Fullscreen);
        assert_eq!(PinPrompt::from_config(Some("panel")), PinPrompt::Panel);
        assert_eq!(PinPrompt::from_config(Some("  panel ")), PinPrompt::Panel);
        for fallback in [None, Some(""), Some("PANEL"), Some("window"), Some("true")] {
            assert_eq!(PinPrompt::from_config(fallback), PinPrompt::Fullscreen, "{fallback:?}");
        }
        assert_eq!(PinPrompt::default(), PinPrompt::Fullscreen);
    }

    #[test]
    fn every_value_is_written_back_as_the_host_reads_it() {
        for prompt in [PinPrompt::Fullscreen, PinPrompt::Panel] {
            assert_eq!(PinPrompt::from_config(Some(prompt.as_str())), prompt);
        }
        assert_eq!((FULLSCREEN, PANEL), ("fullscreen", "panel"));
    }
}
