use forge_platform_core::{
    BannerLevel, BuiltinContent, DetailSection, InfoField, QuickAction, QuickActions,
    test_donation_quick_action,
};

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
    let field = |label: &str, value: Option<String>| InfoField {
        label: label.to_owned(),
        value: value.unwrap_or_else(|| NOT_SET.to_owned()),
        monospace_value: false,
    };
    DetailSection::InfoCard {
        title: "monobank jar".to_owned(),
        live: status.phase == PollPhase::Polling,
        fields: vec![
            field("Jar", jar.and_then(|jar| jar.title.clone())),
            field(
                "Currency",
                jar.and_then(|jar| jar.currency.as_ref())
                    .map(ToString::to_string),
            ),
            field(
                "Goal",
                jar.and_then(|jar| jar.goal.as_ref())
                    .map(ToString::to_string),
            ),
            field("Status", Some(status.phase_label().to_owned())),
        ],
        health_bar: None,
    }
}

fn token_scope_notice() -> DetailSection {
    DetailSection::WarningBanner {
        level: BannerLevel::Info,
        title: "Token access".to_owned(),
        body: "A monobank personal token can read every account and jar you hold. \
               forge only reads the statement of the jar you select, and the token never \
               leaves this computer."
            .to_owned(),
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
        Some(PollFailure::TokenRejected) => {
            "monobank rejected the token. Create a new token at api.monobank.ua and paste it here."
        }
        Some(PollFailure::JarNotSelected) => "Pick the jar that receives donations.",
        Some(PollFailure::JarMissing) => {
            "The selected jar no longer exists for this token. Pick another jar."
        }
        _ => {
            "Paste a personal token from api.monobank.ua and pick a jar to start receiving donations."
        }
    };
    Some(DetailSection::WarningBanner {
        level: BannerLevel::Warning,
        title: status
            .last_error
            .map_or(PollFailure::MissingToken.label(), PollFailure::label)
            .to_owned(),
        body: body.to_owned(),
        cta: None,
    })
}
