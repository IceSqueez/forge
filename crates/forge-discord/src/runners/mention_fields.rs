use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, SubActionConfigExt};
use forge_types::Variant;

use crate::mention::MentionPolicy;

pub(super) const ALLOW_ROLE_PINGS_KEY: &str = "allow_role_pings";
pub(super) const ALLOW_EVERYONE_PINGS_KEY: &str = "allow_everyone_pings";

pub(super) fn mention_default_entries() -> [(String, Variant); 2] {
    let defaults = MentionPolicy::default();
    [
        (
            ALLOW_ROLE_PINGS_KEY.to_owned(),
            Variant::Bool(defaults.allow_roles),
        ),
        (
            ALLOW_EVERYONE_PINGS_KEY.to_owned(),
            Variant::Bool(defaults.allow_everyone),
        ),
    ]
}

pub(super) fn mention_form_fields() -> [FormField; 2] {
    [
        FormField::Toggle {
            key: ALLOW_ROLE_PINGS_KEY,
            label: "Allow role pings",
        },
        FormField::Toggle {
            key: ALLOW_EVERYONE_PINGS_KEY,
            label: "Allow @everyone / @here",
        },
    ]
}

pub(super) fn mention_policy(config: &SubActionConfig) -> MentionPolicy {
    let defaults = MentionPolicy::default();
    MentionPolicy {
        allow_roles: config
            .bool(ALLOW_ROLE_PINGS_KEY)
            .unwrap_or(defaults.allow_roles),
        allow_everyone: config
            .bool(ALLOW_EVERYONE_PINGS_KEY)
            .unwrap_or(defaults.allow_everyone),
    }
}
