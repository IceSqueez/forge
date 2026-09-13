mod surface;
mod validate;

use std::collections::BTreeMap;
use std::ffi::OsString;

pub use surface::EndpointSurface;
pub use validate::EndpointRefusal;

use crate::error::PlatformError;

/// `Default` carries no overrides: every surface resolves to its production endpoint.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlatformEndpoints {
    overrides: BTreeMap<EndpointSurface, String>,
}

impl PlatformEndpoints {
    pub fn from_env() -> Result<Self, PlatformError> {
        Self::resolve(std::env::var_os)
    }

    /// Empty values count as unset. Warns once per call when any surface is overridden.
    pub fn resolve(
        lookup: impl Fn(&'static str) -> Option<OsString>,
    ) -> Result<Self, PlatformError> {
        let mut overrides = BTreeMap::new();
        for surface in EndpointSurface::ALL {
            let variable = surface.env_var();
            let Some(raw) = lookup(variable).filter(|value| !value.is_empty()) else {
                continue;
            };
            let url = validate::validate_override(surface.protocol(), &raw)
                .map_err(|reason| PlatformError::EndpointOverrideRefused { variable, reason })?;
            overrides.insert(surface, url);
        }
        let endpoints = Self { overrides };
        endpoints.warn_if_overridden();
        Ok(endpoints)
    }

    /// No trailing slash, override or not; append paths beginning with `/`.
    pub fn base_url(&self, surface: EndpointSurface) -> &str {
        self.overrides
            .get(&surface)
            .map_or(surface.default_base_url(), String::as_str)
    }

    pub fn overridden(&self) -> impl Iterator<Item = (EndpointSurface, &str)> {
        self.overrides
            .iter()
            .map(|(surface, url)| (*surface, url.as_str()))
    }

    fn warn_if_overridden(&self) {
        if self.overrides.is_empty() {
            return;
        }
        let surfaces = self
            .overrides
            .iter()
            .map(|(surface, url)| format!("{}={url}", surface.env_var()))
            .collect::<Vec<_>>()
            .join(", ");
        tracing::warn!(
            count = self.overrides.len(),
            surfaces = %surfaces,
            "PLATFORM ENDPOINTS OVERRIDDEN: platform traffic for these surfaces goes to loopback, not to the real services"
        );
    }
}
