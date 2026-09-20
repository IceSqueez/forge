use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::AudioError;

const ROUTE_LOCAL: &str = "local";
const ROUTE_OVERLAY: &str = "overlay";
const ROUTE_BOTH: &str = "both";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioRoute {
    #[default]
    Local,
    Overlay,
    Both,
}

impl AudioRoute {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => ROUTE_LOCAL,
            Self::Overlay => ROUTE_OVERLAY,
            Self::Both => ROUTE_BOTH,
        }
    }

    pub fn plays_local(self) -> bool {
        matches!(self, Self::Local | Self::Both)
    }

    pub fn plays_overlay(self) -> bool {
        matches!(self, Self::Overlay | Self::Both)
    }
}

impl fmt::Display for AudioRoute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for AudioRoute {
    type Err = AudioError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let candidate = s.trim();
        for route in [Self::Local, Self::Overlay, Self::Both] {
            if candidate.eq_ignore_ascii_case(route.as_str()) {
                return Ok(route);
            }
        }
        Err(AudioError::UnknownRoute(s.to_owned()))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const EVERY_ROUTE: [AudioRoute; 3] = [AudioRoute::Local, AudioRoute::Overlay, AudioRoute::Both];

    #[test]
    fn each_route_composes_exactly_the_legs_it_names() {
        for (route, local, overlay) in [
            (AudioRoute::Local, true, false),
            (AudioRoute::Overlay, false, true),
            (AudioRoute::Both, true, true),
        ] {
            assert_eq!(route.plays_local(), local, "{route} local leg");
            assert_eq!(route.plays_overlay(), overlay, "{route} overlay leg");
        }

        assert_eq!(AudioRoute::default(), AudioRoute::Local);
    }

    #[test]
    fn route_parses_every_canonical_spelling_regardless_of_case_or_padding() {
        for (text, expected) in [
            ("local", AudioRoute::Local),
            ("overlay", AudioRoute::Overlay),
            ("both", AudioRoute::Both),
            ("LOCAL", AudioRoute::Local),
            ("Overlay", AudioRoute::Overlay),
            ("  both\n", AudioRoute::Both),
        ] {
            assert_eq!(
                text.parse::<AudioRoute>().ok(),
                Some(expected),
                "input {text:?}"
            );
        }
    }

    #[test]
    fn route_rejects_anything_outside_the_vocabulary_echoing_the_raw_input() {
        for bad in [
            "",
            "   ",
            "loca",
            "local overlay",
            "localoverlay",
            "none",
            "remote",
        ] {
            let err = bad.parse::<AudioRoute>().unwrap_err();
            assert!(
                matches!(&err, AudioError::UnknownRoute(raw) if raw == bad),
                "input {bad:?} produced {err:?}"
            );
        }
    }

    #[test]
    fn route_serializes_under_the_same_spelling_it_parses() {
        for route in EVERY_ROUTE {
            let json = serde_json::to_string(&route).unwrap();
            assert_eq!(json, format!("\"{route}\""));
            assert_eq!(serde_json::from_str::<AudioRoute>(&json).unwrap(), route);
            assert_eq!(route.to_string().parse::<AudioRoute>().unwrap(), route);
        }
    }
}
