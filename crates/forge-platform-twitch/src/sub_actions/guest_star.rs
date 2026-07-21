use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, SubActionConfigExt};
use forge_types::Variant;

use super::identity::{SelfIdentity, resolve_user_id};
use crate::helix::{HelixError, HelixTransport};

/// Default value for the `session_id` config field. The active Guest Star
/// session id is not knowable at config time; it arrives on a Guest Star
/// EventSub event, so the field interpolates from the runtime arg stack.
pub(crate) const SESSION_ID_DEFAULT: &str = "%guest_star.session_id%";

/// Builds the `session_id` config field shared by every Guest Star runner.
pub(crate) fn session_id_field() -> FormField {
    FormField::Text {
        key: "session_id",
        label: "Session ID",
        placeholder: SESSION_ID_DEFAULT,
    }
}

/// Builds the `target_user_login` config field for runners that act on a guest.
pub(crate) fn target_login_field() -> FormField {
    FormField::Text {
        key: "target_user_login",
        label: "Guest Login",
        placeholder: "%user_login%",
    }
}

/// Seeds `session_id` with its runtime-interpolated default in a config map.
pub(crate) fn with_session_id(mut config: SubActionConfig) -> SubActionConfig {
    config.insert(
        "session_id".to_owned(),
        Variant::String(SESSION_ID_DEFAULT.to_owned()),
    );
    config
}

/// Seeds an empty `target_user_login` in a config map.
pub(crate) fn with_target_login(mut config: SubActionConfig) -> SubActionConfig {
    config.insert(
        "target_user_login".to_owned(),
        Variant::String(String::new()),
    );
    config
}

/// Rejects a config whose `session_id` is missing or empty.
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

/// Rejects a config whose `target_user_login` is missing or empty.
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

/// Reads a config key, interpolates it against the run-time arg stack, and
/// returns the resolved string (empty when the key is absent).
pub(crate) fn interpolate(
    config: &SubActionConfig,
    arg_stack: &forge_types::ArgStack,
    key: &str,
) -> String {
    let template = config.str(key).unwrap_or_default();
    arg_stack.interpolate(template)
}

/// The three identity params every Guest Star management call shares.
///
/// `broadcaster_id` and `moderator_id` are both the authenticated user: the
/// broadcaster manages their own session, and Twitch validates `moderator_id`
/// for mod privileges, which the broadcaster trivially holds in their own
/// channel. `guest_id` is the target login resolved to a numeric user id via
/// GET /helix/users (one extra Helix call, mirroring the shoutout flow).
pub(crate) struct GuestStarContext {
    pub self_id: String,
    pub guest_id: String,
}

impl GuestStarContext {
    /// Loads self id from credentials, then resolves `target_login` to a
    /// numeric guest id. Both ids are needed before any management call fires.
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
