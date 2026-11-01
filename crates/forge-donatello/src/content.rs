use forge_platform_core::{
    BannerLevel, BuiltinContent, DetailSection, InfoField, QuickAction, QuickActions,
    test_donation_quick_action,
};

use forge_types::{MessageCode, MessageRef, UserText, VerbatimText};

use crate::messages;
use crate::provider::DonatelloProvider;
use crate::status::{PollFailure, PollStatus, PollerPhase};

const NOT_SET: &str = "-";

impl BuiltinContent for DonatelloProvider {
    fn sections(&self) -> Vec<DetailSection> {
        let status = self.poll_status();
        let mut sections: Vec<DetailSection> = action_banner(&status).into_iter().collect();
        sections.push(account_card(&status));
        sections
    }

    fn content_revisions(&self) -> Option<forge_platform_core::CollectionRevisions> {
        None
    }
}

impl QuickActions for DonatelloProvider {
    fn actions(&self) -> Vec<QuickAction> {
        vec![test_donation_quick_action(self.poll.id())]
    }
}

fn account_card(status: &PollStatus) -> DetailSection {
    let account = status.subject.as_ref();
    let field = |label: MessageCode, value: Option<UserText>| InfoField {
        label: label.into(),
        value: value.unwrap_or_else(|| VerbatimText::new(NOT_SET).into()),
        value_is_identifier: false,
    };
    let known = |value: Option<&String>| value.map(|text| VerbatimText::new(text.as_str()).into());
    DetailSection::InfoCard {
        title: messages::LABEL_ACCOUNT_TITLE.into(),
        live: status.phase == PollerPhase::Polling,
        fields: vec![
            field(
                messages::LABEL_NICKNAME,
                known(account.and_then(|account| account.nickname.as_ref())),
            ),
            field(
                messages::LABEL_PAGE,
                known(account.and_then(|account| account.page.as_ref())),
            ),
            field(
                messages::LABEL_STATUS,
                Some(
                    status
                        .phase
                        .message(&messages::POLL_VOCABULARY.phases)
                        .into(),
                ),
            ),
            InfoField {
                label: messages::LABEL_POLL_INTERVAL.into(),
                value: MessageRef::new(messages::LABEL_POLL_INTERVAL_VALUE)
                    .with_arg("seconds", status.poll_interval.as_secs())
                    .into(),
                value_is_identifier: true,
            },
        ],
        health_bar: None,
    }
}

fn action_banner(status: &PollStatus) -> Option<DetailSection> {
    if !matches!(
        status.phase,
        PollerPhase::AwaitingToken | PollerPhase::ActionRequired
    ) {
        return None;
    }
    let body = status
        .last_error
        .and_then(PollFailure::action_banner)
        .unwrap_or(messages::LABEL_BANNER_TOKEN_MISSING);
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
            PollerPhase::AwaitingToken,
            PollerPhase::ActionRequired,
            PollerPhase::Polling,
        ] {
            for failure in [
                None,
                Some(PollFailure::MissingToken),
                Some(PollFailure::TokenRejected),
                Some(PollFailure::TokenUnreadable),
                Some(PollFailure::ProfileIncomplete),
            ] {
                let mut status = PollStatus::initial(std::time::Duration::ZERO);
                status.phase = phase;
                status.last_error = failure;
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

    #[test]
    fn a_token_that_cannot_be_read_gets_its_own_banner_not_the_rejected_one() {
        for (failure, body) in [
            (
                PollFailure::TokenUnreadable,
                messages::LABEL_BANNER_TOKEN_UNREADABLE,
            ),
            (
                PollFailure::TokenRejected,
                messages::LABEL_BANNER_TOKEN_REJECTED,
            ),
        ] {
            let mut status = PollStatus::initial(std::time::Duration::ZERO);
            status.phase = PollerPhase::ActionRequired;
            status.last_error = Some(failure);

            let Some(DetailSection::WarningBanner { body: shown, .. }) = action_banner(&status)
            else {
                panic!("{failure:?} must raise a banner");
            };

            assert_eq!(shown, body.into(), "{failure:?}");
        }
    }
}
