use std::sync::Arc;

use async_trait::async_trait;
use forge_platform_core::{
    TEST_DONATION_AMOUNT, TEST_DONATION_AMOUNT_KEY, TEST_DONATION_CURRENCY,
    TEST_DONATION_CURRENCY_KEY, TEST_DONATION_DONOR, TEST_DONATION_DONOR_KEY,
    TEST_DONATION_MESSAGE, TEST_DONATION_MESSAGE_KEY, TEST_DONATION_PROVIDER_KEY,
    TEST_DONATION_SUB_ACTION,
};
use forge_registry::{
    ConfigProblem, ConfigReason, ConfigSpec, DynamicSelectField, Interpolation, RegistryError,
    RunContext, StepTimer, SubActionCategory, SubActionRegistry, SubActionRunner, TextEditor,
    TextField, TypedField,
};
use forge_types::{
    ArgStack, CurrencyCode, Donation, DonationOrigin, Donor, IntegrationId, MoneyAmount,
    SubActionConfig, SubActionOutcome, SubActionTelemetry,
};
use time::OffsetDateTime;

use crate::core_messages;
use crate::donations::DonationIngest;
use crate::triggers::DONATION_PROVIDER_OPTIONS_KEY;

const TEST_DONATION_ID_PREFIX: &str = "test-";

const PROVIDER: DynamicSelectField<String> = DynamicSelectField::new(
    TEST_DONATION_PROVIDER_KEY,
    "Donation service",
    DONATION_PROVIDER_OPTIONS_KEY,
);

const DONOR: TextField = TextField {
    key: TEST_DONATION_DONOR_KEY,
    label: "Donor name",
    editor: TextEditor::Line { placeholder: "" },
    default: TEST_DONATION_DONOR,
    required: false,
};

const AMOUNT: TextField = TextField {
    key: TEST_DONATION_AMOUNT_KEY,
    label: "Amount",
    editor: TextEditor::Line { placeholder: "" },
    default: TEST_DONATION_AMOUNT,
    required: true,
};

const CURRENCY: TextField = TextField {
    key: TEST_DONATION_CURRENCY_KEY,
    label: "Currency",
    editor: TextEditor::Line { placeholder: "" },
    default: TEST_DONATION_CURRENCY,
    required: true,
};

const MESSAGE: TextField = TextField {
    key: TEST_DONATION_MESSAGE_KEY,
    label: "Message",
    editor: TextEditor::Area,
    default: TEST_DONATION_MESSAGE,
    required: false,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&PROVIDER, &DONOR, &AMOUNT, &CURRENCY, &MESSAGE]);

fn money(amount: &str, currency: &str) -> Result<MoneyAmount, ConfigProblem> {
    let currency = CurrencyCode::parse(currency).map_err(|error| {
        ConfigProblem::field(
            TEST_DONATION_CURRENCY_KEY,
            ConfigReason::InvalidFormat(error.to_string()),
        )
    })?;
    MoneyAmount::parse_decimal(amount.trim(), currency).map_err(|error| {
        ConfigProblem::field(
            TEST_DONATION_AMOUNT_KEY,
            ConfigReason::InvalidFormat(error.to_string()),
        )
    })
}

struct TestDonation {
    provider: String,
    donor: String,
    amount: MoneyAmount,
    message: String,
}

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<TestDonation, ConfigProblem> {
    Ok(TestDonation {
        provider: PROVIDER.read(config, args)?,
        donor: DONOR.read(config, args)?,
        amount: money(&AMOUNT.read(config, args)?, &CURRENCY.read(config, args)?)?,
        message: MESSAGE.read(config, args)?,
    })
}

pub struct TestDonationRunner {
    ingest: Arc<DonationIngest>,
}

impl TestDonationRunner {
    pub fn new(ingest: Arc<DonationIngest>) -> Self {
        Self { ingest }
    }

    fn announce(&self, read: TestDonation, ctx: &RunContext<'_>) -> SubActionOutcome {
        let message = read.message.trim();
        let donation = Donation {
            provider: IntegrationId::new(&read.provider),
            donation_id: format!("{TEST_DONATION_ID_PREFIX}{}", ctx.parent_event_id),
            donor: Donor::named(&read.donor),
            message: (!message.is_empty()).then(|| message.to_owned()),
            amount: read.amount,
            occurred_at: OffsetDateTime::now_utc(),
            origin: DonationOrigin::Test,
        };
        if self
            .ingest
            .announce_test(&donation, Some(ctx.parent_event_id))
        {
            SubActionOutcome::Success
        } else {
            SubActionOutcome::Failed(core_messages::test_donation_not_announced().into())
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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn check_config(
        &self,
        config: &SubActionConfig,
        interpolation: Interpolation<'_>,
    ) -> Result<(), ConfigProblem> {
        match (
            AMOUNT.peek(config, interpolation)?,
            CURRENCY.peek(config, interpolation)?,
        ) {
            (Some(amount), Some(currency)) => money(&amount, &currency).map(|_| ()),
            _ => Ok(()),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, TEST_DONATION_SUB_ACTION);
        let outcome = match read_config(config, ctx.arg_stack) {
            Ok(read) => self.announce(read, ctx),
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
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
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Mutex;

    use forge_events::{DonationReceived, Event, EventPublisher, TypedEvent};
    use forge_platform_core::{test_donation_quick_action, test_donation_step};
    use forge_storage::MockDonationRepo;
    use forge_types::{EventId, Shared};

    use super::*;
    use forge_types::Variant;

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
