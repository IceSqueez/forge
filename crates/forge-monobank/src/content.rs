use forge_platform_core::{
    BannerLevel, BuiltinContent, DetailSection, InfoField, QuickAction, QuickActions,
    test_donation_quick_action,
};

use forge_types::{MessageCode, UserText, VerbatimText};

use crate::messages;
use crate::provider::MonobankProvider;
use crate::status::{PollFailure, PollPhase, PollStatus};

const NOT_SET: &str = "-";

impl BuiltinContent for MonobankProvider {
    fn sections(&self) -> Vec<DetailSection> {
        let status = self.poll_status();
        let mut sections: Vec<DetailSection> = action_banner(&status).into_iter().collect();
        sections.push(jar_card(&status));
        sections.push(token_scope_notice());
        sections
    }
}

impl QuickActions for MonobankProvider {
    fn actions(&self) -> Vec<QuickAction> {
        vec![test_donation_quick_action(&self.id)]
    }
}

fn jar_card(status: &PollStatus) -> DetailSection {
    let jar = status.jar.as_ref();
    let field = |label: MessageCode, value: Option<UserText>| InfoField {
        label: label.into(),
        value: value.unwrap_or_else(|| VerbatimText::new(NOT_SET).into()),
        monospace_value: false,
    };
    DetailSection::InfoCard {
        title: messages::LABEL_JAR_TITLE.into(),
        live: status.phase == PollPhase::Polling,
        fields: vec![
            field(
                messages::LABEL_JAR,
                jar.and_then(|jar| jar.title.clone())
                    .map(|title| VerbatimText::new(title).into()),
            ),
            field(
                messages::LABEL_CURRENCY,
                jar.and_then(|jar| jar.currency.as_ref())
                    .map(|currency| VerbatimText::new(currency.to_string()).into()),
            ),
            field(
                messages::LABEL_GOAL,
                jar.and_then(|jar| jar.goal.as_ref())
                    .map(|goal| VerbatimText::new(goal.to_string()).into()),
            ),
            field(messages::LABEL_STATUS, Some(status.phase.message().into())),
        ],
        health_bar: None,
    }
}

fn token_scope_notice() -> DetailSection {
    DetailSection::WarningBanner {
        level: BannerLevel::Info,
        title: messages::LABEL_TOKEN_ACCESS_TITLE.into(),
        body: messages::LABEL_TOKEN_ACCESS_BODY.into(),
        cta: None,
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
        Some(PollFailure::TokenRejected) => messages::LABEL_BANNER_TOKEN_REJECTED,
        Some(PollFailure::TokenUnreadable) => messages::LABEL_BANNER_TOKEN_UNREADABLE,
        Some(PollFailure::JarNotSelected) => messages::LABEL_BANNER_JAR_NOT_SELECTED,
        Some(PollFailure::JarMissing) => messages::LABEL_BANNER_JAR_MISSING,
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
    use crate::messages::MONOBANK_MESSAGES;

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
                Some(PollFailure::TokenUnreadable),
                Some(PollFailure::JarNotSelected),
                Some(PollFailure::JarMissing),
            ] {
                let status = PollStatus {
                    phase,
                    jar: None,
                    last_poll_at: None,
                    last_donation_at: None,
                    last_error: failure,
                    last_error_at: None,
                    polling_since: None,
                };
                let emitted = emitted_codes(&serde_json::json!((
                    action_banner(&status),
                    jar_card(&status)
                )));

                assert!(!emitted.is_empty(), "{phase:?} {failure:?}");
                assert_eq!(
                    uncatalogued(&emitted, &[MONOBANK_MESSAGES, COMMON_MESSAGES]),
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
            let status = PollStatus {
                phase: PollPhase::ActionRequired,
                jar: None,
                last_poll_at: None,
                last_donation_at: None,
                last_error: Some(failure),
                last_error_at: None,
                polling_since: None,
            };

            let Some(DetailSection::WarningBanner { body: shown, .. }) = action_banner(&status)
            else {
                panic!("{failure:?} must raise a banner");
            };

            assert_eq!(shown, body.into(), "{failure:?}");
        }
    }
}
