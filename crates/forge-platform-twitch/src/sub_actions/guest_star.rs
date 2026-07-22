use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, SubActionConfigExt};
use forge_types::Variant;

use super::identity::{SelfIdentity, resolve_user_id};
use crate::helix::{HelixError, HelixTransport};

/// The active session id is not knowable at config time; it arrives on a Guest Star EventSub event.
pub(crate) const SESSION_ID_DEFAULT: &str = "%guest_star.session_id%";

pub(crate) fn session_id_field() -> FormField {
    FormField::Text {
        key: "session_id",
        label: "Session ID",
        placeholder: SESSION_ID_DEFAULT,
    }
}

pub(crate) fn target_login_field() -> FormField {
    FormField::Text {
        key: "target_user_login",
        label: "Guest Login",
        placeholder: "%user_login%",
    }
}

pub(crate) fn with_session_id(mut config: SubActionConfig) -> SubActionConfig {
    config.insert(
        "session_id".to_owned(),
        Variant::String(SESSION_ID_DEFAULT.to_owned()),
    );
    config
}

pub(crate) fn with_target_login(mut config: SubActionConfig) -> SubActionConfig {
    config.insert(
        "target_user_login".to_owned(),
        Variant::String(String::new()),
    );
    config
}

pub(crate) fn validate_session_id(
    kind_id: &str,
    config: &SubActionConfig,
) -> Result<(), RegistryError> {
    match config.get("session_id") {
        Some(Variant::String(s)) if !s.is_empty() => Ok(()),
        _ => Err(RegistryError::InvalidConfig(format!(
            "{kind_id}: 'session_id' is required"
        ))),
    }
}

pub(crate) fn validate_target_login(
    kind_id: &str,
    config: &SubActionConfig,
) -> Result<(), RegistryError> {
    match config.get("target_user_login") {
        Some(Variant::String(s)) if !s.is_empty() => Ok(()),
        _ => Err(RegistryError::InvalidConfig(format!(
            "{kind_id}: 'target_user_login' is required"
        ))),
    }
}

/// Returns the empty string when the key is absent.
pub(crate) fn interpolate(
    config: &SubActionConfig,
    arg_stack: &forge_types::ArgStack,
    key: &str,
) -> String {
    let template = config.str(key).unwrap_or_default();
    arg_stack.interpolate(template)
}

/// `broadcaster_id`/`moderator_id` are both self; `guest_id` is `target_login` resolved via `/helix/users`.
pub(crate) struct GuestStarContext {
    pub self_id: String,
    pub guest_id: String,
}

impl GuestStarContext {
    pub(crate) async fn resolve(
        transport: &dyn HelixTransport,
        identity: &SelfIdentity,
        target_login: &str,
    ) -> Result<Self, HelixError> {
        let self_id = identity.user_id().await?;
        let guest_id = resolve_user_id(transport, target_login).await?;
        Ok(Self { self_id, guest_id })
    }
}
