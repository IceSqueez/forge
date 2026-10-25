use std::time::Duration;

use super::call_class::CallClass;
use crate::rate_limit::DEFAULT_RETRY_AFTER_SECS;
use crate::throttle_hint::ThrottleHintFamily;

const USER_DIRECTED_NO_HINT_WAIT: Duration = Duration::from_secs(DEFAULT_RETRY_AFTER_SECS as u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EgressKind {
    PlatformApi,
    UserDirected { allow_local: bool },
    TrustedAbsolute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GatewayProfile {
    pub default_class: CallClass,
    pub hint_family: ThrottleHintFamily,
    pub no_hint_default: Duration,
    pub egress: EgressKind,
}

impl GatewayProfile {
    pub const fn platform_api(
        default_class: CallClass,
        hint_family: ThrottleHintFamily,
        no_hint_default: Duration,
    ) -> Self {
        Self {
            default_class,
            hint_family,
            no_hint_default,
            egress: EgressKind::PlatformApi,
        }
    }

    pub const fn user_directed(allow_local: bool) -> Self {
        Self {
            default_class: CallClass::Interactive,
            hint_family: ThrottleHintFamily::RetryAfter,
            no_hint_default: USER_DIRECTED_NO_HINT_WAIT,
            egress: EgressKind::UserDirected { allow_local },
        }
    }

    pub const fn absolute_https(
        default_class: CallClass,
        hint_family: ThrottleHintFamily,
        no_hint_default: Duration,
    ) -> Self {
        Self {
            default_class,
            hint_family,
            no_hint_default,
            egress: EgressKind::TrustedAbsolute,
        }
    }

    pub const fn accepts_absolute_urls(&self) -> bool {
        matches!(
            self.egress,
            EgressKind::UserDirected { .. } | EgressKind::TrustedAbsolute
        )
    }

    pub(crate) const fn attaches_credentials(&self) -> bool {
        !matches!(self.egress, EgressKind::TrustedAbsolute)
    }

    pub(crate) const fn allows_plain_http_to_any_host(&self) -> bool {
        matches!(self.egress, EgressKind::UserDirected { .. })
    }

    pub(crate) const fn guards_addresses(&self) -> bool {
        matches!(self.egress, EgressKind::UserDirected { allow_local: false })
    }
}
