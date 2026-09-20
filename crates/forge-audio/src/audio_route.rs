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
