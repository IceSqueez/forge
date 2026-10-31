mod surface;
mod validate;

use std::ffi::OsString;

pub use surface::EndpointSurface;
pub use validate::EndpointRefusal;

use crate::error::PlatformError;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlatformEndpoints {
    overrides: Vec<(EndpointSurface, String)>,
}

impl PlatformEndpoints {
    pub fn from_env(surfaces: &[EndpointSurface]) -> Result<Self, PlatformError> {
        Self::resolve(surfaces, std::env::var_os)
    }

    pub fn resolve(
        surfaces: &[EndpointSurface],
        lookup: impl Fn(&'static str) -> Option<OsString>,
    ) -> Result<Self, PlatformError> {
        let mut overrides = Vec::new();
        for &surface in surfaces {
            let variable = surface.env_var();
            let Some(raw) = lookup(variable).filter(|value| !value.is_empty()) else {
                continue;
            };
            let url = validate::validate_override(surface.protocol(), &raw)
                .map_err(|reason| PlatformError::EndpointOverrideRefused { variable, reason })?;
            overrides.push((surface, url));
        }
        let endpoints = Self { overrides };
        endpoints.warn_if_overridden();
        Ok(endpoints)
    }

    pub fn base_url(&self, surface: EndpointSurface) -> &str {
        self.overrides
            .iter()
            .find(|(overridden, _)| *overridden == surface)
            .map_or(surface.default_base_url(), |(_, url)| url.as_str())
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
