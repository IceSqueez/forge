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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Mutex;

    use forge_events::{DonationReceived, Event, EventPublisher};
    use forge_platform_core::{test_donation_quick_action, test_donation_step};
    use forge_storage::MockDonationRepo;
    use forge_types::{EventId, Shared};

    use super::*;

    const PLACEHOLDER: &str = "Anonymous donor";

    #[derive(Default)]
    struct Recorder(Mutex<Vec<Event>>);

    impl EventPublisher for Recorder {
        fn publish(&self, event: Event) {
            self.0.lock().unwrap().push(event);
        }
    }

    impl Recorder {
        fn events(&self) -> Vec<Event> {
            self.0.lock().unwrap().clone()
        }

        fn announced(&self) -> Vec<DonationReceived> {
            self.events()
                .iter()
                .map(|event| DonationReceived::from_event(event).unwrap())
                .collect()
        }
    }

    fn rig() -> (TestDonationRunner, Arc<Recorder>) {
        let recorder = Arc::new(Recorder::default());
        let ingest = DonationIngest::new(
            Arc::new(MockDonationRepo::new()),
            Arc::clone(&recorder) as Arc<dyn EventPublisher>,
            Shared::new(PLACEHOLDER.to_owned()),
        );
        (TestDonationRunner::new(Arc::new(ingest)), recorder)
    }

    fn config(entries: &[(&str, &str)]) -> SubActionConfig {
        let mut config = test_donation_step(&IntegrationId::new("donatello")).config;
        for (key, value) in entries {
            config.insert((*key).to_owned(), Variant::String((*value).to_owned()));
        }
        config
    }

    async fn run(
        runner: &TestDonationRunner,
        config: &SubActionConfig,
        parent: EventId,
    ) -> SubActionOutcome {
        let stack = ArgStack::new();
        let publisher = Recorder::default();
        let ctx = RunContext::leaf(&stack, 0, parent, &publisher);
        runner.execute(config, &ctx).await.0.outcome
    }

    #[tokio::test]
    async fn a_test_donation_is_announced_flagged_as_a_test_with_the_configured_amount() {
        let (runner, recorder) = rig();

        let outcome = run(
            &runner,
            &config(&[(TEST_DONATION_AMOUNT_KEY, "12.50")]),
            EventId::new(),
        )
        .await;

        let announced: Vec<(bool, u64)> = recorder
            .announced()
            .iter()
            .map(|received| (received.test, received.amount_micros))
            .collect();
        assert!(matches!(outcome, SubActionOutcome::Success));
        assert_eq!(announced, [(true, 12_500_000)]);
    }

    #[tokio::test]
    async fn a_test_donation_is_caused_by_the_run_that_sent_it() {
        let (runner, recorder) = rig();
        let parent = EventId::new();

        run(&runner, &config(&[]), parent).await;

        let causes: Vec<Option<EventId>> = recorder
            .events()
            .iter()
            .map(|event| event.caused_by)
            .collect();
        assert_eq!(causes, [Some(parent)]);
    }

    #[tokio::test]
    async fn a_malformed_amount_or_currency_fails_without_announcing() {
        for (amount, currency) in [("abc", "UAH"), ("-5", "UAH"), ("", "UAH"), ("10", "UA")] {
            let (runner, recorder) = rig();

            let outcome = run(
                &runner,
                &config(&[
                    (TEST_DONATION_AMOUNT_KEY, amount),
                    (TEST_DONATION_CURRENCY_KEY, currency),
                ]),
                EventId::new(),
            )
            .await;

            assert!(
                matches!(outcome, SubActionOutcome::Failed(_)) && recorder.events().is_empty(),
                "amount {amount:?} currency {currency:?}"
            );
        }
    }

    #[tokio::test]
    async fn a_test_donation_without_a_donation_service_fails_without_announcing() {
        for provider in ["", "   "] {
            let (runner, recorder) = rig();

            let outcome = run(
                &runner,
                &config(&[(TEST_DONATION_PROVIDER_KEY, provider)]),
                EventId::new(),
            )
            .await;

            assert!(
                matches!(outcome, SubActionOutcome::Failed(_)) && recorder.events().is_empty(),
                "provider {provider:?}"
            );
        }
    }

    #[tokio::test]
    async fn a_blank_donor_name_is_announced_under_the_anonymous_placeholder() {
        let (runner, recorder) = rig();

        run(
            &runner,
            &config(&[(TEST_DONATION_DONOR_KEY, "  ")]),
            EventId::new(),
        )
        .await;

        let names: Vec<String> = recorder
            .announced()
            .into_iter()
            .map(|received| received.donor_name)
            .collect();
        assert_eq!(names, [PLACEHOLDER]);
    }

    #[test]
    fn the_quick_action_template_is_the_default_config_aimed_at_its_provider() {
        let (runner, _) = rig();
        let provider = IntegrationId::new("monobank");
        let mut expected = runner.default_config();
        expected.insert(
            TEST_DONATION_PROVIDER_KEY.to_owned(),
            Variant::String(provider.as_str().to_owned()),
        );

        assert_eq!(test_donation_step(&provider).config, expected);
    }

    #[test]
    fn every_form_and_quick_action_field_edits_a_key_the_step_reads() {
        let (runner, _) = rig();
        let config_keys: BTreeSet<String> = runner.default_config().into_keys().collect();
        let form_keys: BTreeSet<String> = runner
            .config_fields()
            .iter()
            .map(|field| field.key().to_owned())
            .collect();
        let quick_keys: BTreeSet<String> =
            test_donation_quick_action(&IntegrationId::new("donatello"))
                .fields
                .into_iter()
                .map(|field| field.key)
                .collect();

        assert_eq!(form_keys, config_keys);
        assert!(quick_keys.is_subset(&config_keys), "{quick_keys:?}");
    }
}
