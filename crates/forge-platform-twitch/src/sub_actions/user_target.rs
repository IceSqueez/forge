use forge_registry::runner::SubActionConfig;
use forge_registry::{RegistryError, RunContext, SubActionConfigExt};

use super::identity::resolve_user_id;
use crate::helix::{HelixError, HelixTransport};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UserTarget<'a> {
    Id(&'a str),
    Login(&'a str),
}

impl<'a> UserTarget<'a> {
    pub(crate) fn pick(id: &'a str, login: &'a str) -> Option<Self> {
        if !id.is_empty() {
            Some(Self::Id(id))
        } else if !login.is_empty() {
            Some(Self::Login(login))
        } else {
            None
        }
    }

    pub(crate) async fn user_id(
        self,
        transport: &dyn HelixTransport,
    ) -> Result<String, HelixError> {
        match self {
            Self::Id(id) => Ok(id.to_owned()),
            Self::Login(login) => resolve_user_id(transport, login).await,
        }
    }
}

pub(crate) struct TargetKeys {
    pub(crate) id: &'static str,
    pub(crate) login: &'static str,
}

impl TargetKeys {
    pub(crate) fn validate(
        &self,
        kind_id: &str,
        config: &SubActionConfig,
    ) -> Result<(), RegistryError> {
        if config.str_nonempty(self.id).is_some() || config.str_nonempty(self.login).is_some() {
            return Ok(());
        }
        Err(RegistryError::InvalidConfig(format!(
            "{kind_id}: '{}' or '{}' must be a non-empty string",
            self.login, self.id
        )))
    }

    pub(crate) fn interpolate(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> InterpolatedTarget {
        let interpolate = |key: &str| {
            config
                .str(key)
                .map(|template| ctx.arg_stack.interpolate(template))
                .unwrap_or_default()
        };
        InterpolatedTarget {
            id: interpolate(self.id),
            login: interpolate(self.login),
        }
    }

    pub(crate) fn empty_after_interpolation(&self) -> String {
        format!(
            "{} and {} are empty after interpolation",
            self.login, self.id
        )
    }
}

pub(crate) struct InterpolatedTarget {
    id: String,
    login: String,
}

impl InterpolatedTarget {
    pub(crate) fn target(&self) -> Option<UserTarget<'_>> {
        UserTarget::pick(&self.id, &self.login)
    }
}
