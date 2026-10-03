use std::sync::Arc;

use async_trait::async_trait;
use forge_platform_core::{
    TEST_DONATION_AMOUNT, TEST_DONATION_AMOUNT_KEY, TEST_DONATION_CURRENCY,
    TEST_DONATION_CURRENCY_KEY, TEST_DONATION_DONOR, TEST_DONATION_DONOR_KEY,
    TEST_DONATION_MESSAGE, TEST_DONATION_MESSAGE_KEY, TEST_DONATION_PROVIDER_KEY,
    TEST_DONATION_SUB_ACTION,
};
use forge_registry::{
    FormField, RegistryError, RunContext, StepTimer, SubActionCategory, SubActionConfigExt,
    SubActionRegistry, SubActionRunner,
};
use forge_types::{
    ArgStack, CurrencyCode, Donation, DonationOrigin, Donor, IntegrationId, MoneyAmount,
    SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant,
};
use time::OffsetDateTime;

use crate::donations::DonationIngest;

pub struct TestDonationRunner {
    ingest: Arc<DonationIngest>,
}

impl TestDonationRunner {
    pub fn new(ingest: Arc<DonationIngest>) -> Self {
        Self { ingest }
    }

    fn run(&self, config: &SubActionConfig, ctx: &RunContext<'_>) -> SubActionOutcome {
        let text = |key: &str| {
            ctx.arg_stack
                .interpolate(config.str(key).unwrap_or_default())
        };
        let provider = text(TEST_DONATION_PROVIDER_KEY);
        if provider.trim().is_empty() {
            return SubActionOutcome::Failed("test donation needs a donation service".to_owned());
        }
        let currency = match CurrencyCode::parse(&text(TEST_DONATION_CURRENCY_KEY)) {
            Ok(currency) => currency,
            Err(error) => return SubActionOutcome::Failed(format!("test donation: {error}")),
        };
        let amount =
            match MoneyAmount::parse_decimal(text(TEST_DONATION_AMOUNT_KEY).trim(), currency) {
                Ok(amount) => amount,
                Err(error) => return SubActionOutcome::Failed(format!("test donation: {error}")),
            };
        let message = text(TEST_DONATION_MESSAGE_KEY);
        let donation = Donation {
            provider: IntegrationId::new(provider.trim()),
            donation_id: format!("test-{}", ctx.parent_event_id),
            donor: Donor::named(&text(TEST_DONATION_DONOR_KEY)),
            message: (!message.trim().is_empty()).then(|| message.trim().to_owned()),
            amount,
            occurred_at: OffsetDateTime::now_utc(),
            origin: DonationOrigin::Test,
        };
        if self
            .ingest
            .announce_test(&donation, Some(ctx.parent_event_id))
        {
            SubActionOutcome::Success
        } else {
            SubActionOutcome::Failed("test donation could not be announced".to_owned())
        }
    }
}

#[async_trait]
impl SubActionRunner for TestDonationRunner {
    fn id(&self) -> &str {
        TEST_DONATION_SUB_ACTION
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "Send test donation"
    }

    fn summary(&self) -> &str {
        "Announce a fake donation that fires donation triggers marked as a test"
    }

    fn search_text(&self) -> &str {
        "test donation donate tip fake simulate money alert"
    }

    fn icon_name(&self) -> &str {
        "coin"
    }

    fn default_config(&self) -> SubActionConfig {
        let text = |value: &str| Variant::String(value.to_owned());
        SubActionConfig::from([
            (TEST_DONATION_PROVIDER_KEY.to_owned(), text("")),
            (
                TEST_DONATION_DONOR_KEY.to_owned(),
                text(TEST_DONATION_DONOR),
            ),
            (
                TEST_DONATION_AMOUNT_KEY.to_owned(),
                text(TEST_DONATION_AMOUNT),
            ),
            (
                TEST_DONATION_CURRENCY_KEY.to_owned(),
                text(TEST_DONATION_CURRENCY),
            ),
            (
                TEST_DONATION_MESSAGE_KEY.to_owned(),
                text(TEST_DONATION_MESSAGE),
            ),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::DynamicSelect {
                key: TEST_DONATION_PROVIDER_KEY,
                label: "Donation service",
                options_key: crate::triggers::DONATION_PROVIDER_OPTIONS_KEY,
            },
            FormField::Text {
                key: TEST_DONATION_DONOR_KEY,
                label: "Donor name",
                placeholder: "",
            },
            FormField::Text {
                key: TEST_DONATION_AMOUNT_KEY,
                label: "Amount",
                placeholder: "",
            },
            FormField::Text {
                key: TEST_DONATION_CURRENCY_KEY,
                label: "Currency",
                placeholder: "",
            },
            FormField::TextArea {
                key: TEST_DONATION_MESSAGE_KEY,
                label: "Message",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        config.require_str(TEST_DONATION_PROVIDER_KEY).map(|_| ())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, TEST_DONATION_SUB_ACTION);
        let outcome = self.run(config, ctx);
        (timer.finish(outcome), None)
    }
}

pub fn register_donation_sub_actions(
    registry: &mut SubActionRegistry,
    ingest: Arc<DonationIngest>,
) -> Result<(), RegistryError> {
    registry.register(Box::new(TestDonationRunner::new(ingest)))
}
