use forge_components::{Icon, tr};
use forge_runtime::{IntegrationStartError, StartFailureCategory};

use crate::user_text::{ResolvedText, resolve_message};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartRemedy {
    SignIn,
    OpenSettings,
    Retry,
}

impl StartRemedy {
    pub fn of(error: &IntegrationStartError) -> Self {
        match error.category() {
            StartFailureCategory::Credentials => Self::SignIn,
            StartFailureCategory::NotConfigured => Self::OpenSettings,
            StartFailureCategory::Unreachable
            | StartFailureCategory::RateLimited
            | StartFailureCategory::Refused
            | StartFailureCategory::Internal => Self::Retry,
        }
    }

    pub fn opens_detail(self) -> bool {
        !matches!(self, Self::Retry)
    }

    pub fn label(self) -> String {
        match self {
            Self::SignIn => tr!("integration_failed_sign_in"),
            Self::OpenSettings => tr!("integration_failed_open_settings"),
            Self::Retry => tr!("integration_retry"),
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            Self::SignIn => Icon::Plug,
            Self::OpenSettings => Icon::Settings,
            Self::Retry => Icon::Refresh,
        }
    }
}

pub fn start_failure_text(error: &IntegrationStartError) -> ResolvedText {
    resolve_message(error.message())
}

pub fn start_failure_tooltip(error: &IntegrationStartError) -> String {
    let text = start_failure_text(error);
    match text.secondary {
        Some(secondary) => format!("{}\n{secondary}", text.primary),
        None => text.primary,
    }
}

#[cfg(test)]
mod tests {
    use forge_types::{MessageRef, UPSTREAM_FAILURE};

    use super::*;

    #[test]
    fn credential_and_configuration_failures_open_the_detail_while_the_rest_offer_a_retry() {
        for (category, remedy, opens_detail) in [
            (StartFailureCategory::Credentials, StartRemedy::SignIn, true),
            (
                StartFailureCategory::NotConfigured,
                StartRemedy::OpenSettings,
                true,
            ),
            (StartFailureCategory::Unreachable, StartRemedy::Retry, false),
            (StartFailureCategory::RateLimited, StartRemedy::Retry, false),
            (StartFailureCategory::Refused, StartRemedy::Retry, false),
            (StartFailureCategory::Internal, StartRemedy::Retry, false),
        ] {
            let error = IntegrationStartError::new(category, MessageRef::new(UPSTREAM_FAILURE));
            let chosen = StartRemedy::of(&error);
            assert_eq!(
                (chosen, chosen.opens_detail()),
                (remedy, opens_detail),
                "{category:?}"
            );
        }
    }
}
