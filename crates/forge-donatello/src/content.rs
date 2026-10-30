use forge_platform_core::{
    BannerLevel, BuiltinContent, DetailSection, InfoField, QuickAction, QuickActions,
    test_donation_quick_action,
};

use forge_types::{MessageCode, MessageRef, UserText};

use crate::messages;
use crate::provider::DonatelloProvider;
use crate::status::{PollFailure, PollPhase, PollStatus};

const NOT_SET: &str = "-";

impl BuiltinContent for DonatelloProvider {
    fn sections(&self) -> Vec<DetailSection> {
        let status = self.poll_status();
        let mut sections: Vec<DetailSection> = action_banner(&status).into_iter().collect();
        sections.push(account_card(&status));
        sections
    }
}

impl QuickActions for DonatelloProvider {
    fn actions(&self) -> Vec<QuickAction> {
        vec![test_donation_quick_action(&self.id)]
    }
}

fn account_card(status: &PollStatus) -> DetailSection {
    let account = status.account.as_ref();
    let field = |label: MessageCode, value: Option<UserText>| InfoField {
        label: label.into(),
        value: value.unwrap_or_else(|| UserText::unlocalized(NOT_SET)),
        monospace_value: false,
    };
    let known = |value: Option<&String>| value.map(UserText::unlocalized);
    DetailSection::InfoCard {
        title: messages::LABEL_ACCOUNT_TITLE.into(),
        live: status.phase == PollPhase::Polling,
        fields: vec![
            field(
                messages::LABEL_NICKNAME,
                known(account.and_then(|account| account.nickname.as_ref())),
            ),
            field(
                messages::LABEL_PAGE,
                known(account.and_then(|account| account.page.as_ref())),
            ),
            field(messages::LABEL_STATUS, Some(status.phase.message().into())),
            InfoField {
                label: messages::LABEL_POLL_INTERVAL.into(),
                value: MessageRef::new(messages::LABEL_POLL_INTERVAL_VALUE)
                    .with_arg("seconds", status.poll_interval.as_secs())
                    .into(),
                monospace_value: true,
            },
        ],
        health_bar: None,
    }
}

fn action_banner(status: &PollStatus) -> Option<DetailSection> {
    if !matches!(
        status.phase,
        PollPhase::AwaitingToken | PollPhase::ActionRequired
    ) {
        return None;
    }
    let body = match status.last_error {
        Some(PollFailure::ProfileIncomplete) => messages::LABEL_BANNER_PROFILE_INCOMPLETE,
        Some(PollFailure::TokenRejected) => messages::LABEL_BANNER_TOKEN_REJECTED,
        _ => messages::LABEL_BANNER_TOKEN_MISSING,
    };
    Some(DetailSection::WarningBanner {
        level: BannerLevel::Warning,
        title: status
            .last_error
            .map_or(PollFailure::MissingToken.message(), PollFailure::message)
            .into(),
        body: body.into(),
        cta: None,
    })
}

#[cfg(test)]
mod tests {
    use forge_types::COMMON_MESSAGES;

    use super::*;
    use crate::emitted_codes::{emitted_codes, uncatalogued};
    use crate::messages::DONATELLO_MESSAGES;

    #[test]
    fn every_banner_and_card_code_is_catalogued_in_both_languages() {
        for phase in [
            PollPhase::AwaitingToken,
            PollPhase::ActionRequired,
            PollPhase::Polling,
        ] {
            for failure in [
                None,
                Some(PollFailure::MissingToken),
                Some(PollFailure::TokenRejected),
                Some(PollFailure::ProfileIncomplete),
            ] {
                let status = PollStatus {
                    phase,
                    account: None,
                    poll_interval: std::time::Duration::ZERO,
                    last_poll_at: None,
                    last_donation_at: None,
                    last_error: failure,
                    last_error_at: None,
                    polling_since: None,
                };
                let emitted = emitted_codes(&serde_json::json!((
                    action_banner(&status),
                    account_card(&status)
                )));

                assert!(!emitted.is_empty(), "{phase:?} {failure:?}");
                assert_eq!(
                    uncatalogued(&emitted, &[DONATELLO_MESSAGES, COMMON_MESSAGES]),
                    Vec::<String>::new(),
                    "{phase:?} {failure:?}"
                );
            }
        }
    }
}
