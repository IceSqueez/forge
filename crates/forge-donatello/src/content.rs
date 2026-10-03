use forge_platform_core::{
    BannerLevel, BuiltinContent, DetailSection, InfoField, QuickAction, QuickActions,
};

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
        Vec::new()
    }
}

fn account_card(status: &PollStatus) -> DetailSection {
    let account = status.account.as_ref();
    let field = |label: &str, value: Option<&str>| InfoField {
        label: label.to_owned(),
        value: value.unwrap_or(NOT_SET).to_owned(),
        monospace_value: false,
    };
    DetailSection::InfoCard {
        title: "Donatello account".to_owned(),
        live: status.phase == PollPhase::Polling,
        fields: vec![
            field(
                "Nickname",
                account.and_then(|account| account.nickname.as_deref()),
            ),
            field("Page", account.and_then(|account| account.page.as_deref())),
            field("Status", Some(status.phase_label())),
            InfoField {
                label: "Poll interval".to_owned(),
                value: format!("{}s", status.poll_interval.as_secs()),
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
        Some(PollFailure::ProfileIncomplete) => {
            "Finish the profile setup in the Donatello panel, then reconnect."
        }
        Some(PollFailure::TokenRejected) => {
            "Donatello rejected the token. Paste a new token from the Donatello panel."
        }
        _ => "Paste the API token from the Donatello panel to start receiving donations.",
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
