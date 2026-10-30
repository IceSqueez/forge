use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    ChoiceField, ConfigChoice, ConfigProblem, ConfigReason, ConfigSpec, DurationBounds,
    DurationField, Interpolation, RunContext, StepTimer, SubActionCategory, SubActionRunner,
    TextEditor, TextField, TypedField, ValueBounds,
};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};

use super::os_ports::{DesktopNotice, NotifyPort, NotifyUrgency};
use crate::core_messages;

const TIMEOUT_MIN_MS: i64 = 1000;
const TIMEOUT_MAX_MS: i64 = 60000;
const DEFAULT_TIMEOUT_MS: i64 = 5000;
const MIN_TITLE_CHARS: usize = 1;
const MAX_TITLE_CHARS: usize = 100;

impl ConfigChoice for NotifyUrgency {
    const ALL: &'static [Self] = &[Self::Low, Self::Normal, Self::Critical];
    const WIRE_NAMES: &'static [&'static str] = &["low", "normal", "critical"];

    fn wire_name(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::Critical => "critical",
        }
    }
}

const TITLE: TextField = TextField {
    key: "title",
    label: "Title",
    editor: TextEditor::Line {
        placeholder: "Stream starting",
    },
    default: "",
    required: false,
};

const BODY: TextField = TextField {
    key: "body",
    label: "Body",
    editor: TextEditor::Area,
    default: "",
    required: false,
};

const URGENCY: ChoiceField<NotifyUrgency> = ChoiceField {
    key: "urgency",
    label: "Urgency (Linux only)",
    default: NotifyUrgency::Normal,
};

const ICON_PATH: TextField = TextField {
    key: "icon_path",
    label: "Icon Path",
    editor: TextEditor::Line {
        placeholder: "/path/to/icon.png",
    },
    default: "",
    required: false,
};

const TIMEOUT: DurationField = DurationField {
    key: "timeout_ms",
    label: "Timeout",
    default: DEFAULT_TIMEOUT_MS,
    bounds: DurationBounds::in_millis(TIMEOUT_MIN_MS, TIMEOUT_MAX_MS),
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&TITLE, &BODY, &URGENCY, &ICON_PATH, &TIMEOUT]);

fn title_length(config: &SubActionConfig) -> Result<(), ConfigProblem> {
    let title_chars = config
        .get(TITLE.key)
        .and_then(Variant::as_str)
        .map_or(0, |title| title.chars().count());
    if (MIN_TITLE_CHARS..=MAX_TITLE_CHARS).contains(&title_chars) {
        Ok(())
    } else {
        Err(ConfigProblem::field(
            TITLE.key,
            ConfigReason::LengthOutOfRange(ValueBounds::between(MIN_TITLE_CHARS, MAX_TITLE_CHARS)),
        ))
    }
}

fn read_notice(config: &SubActionConfig, args: &ArgStack) -> Result<DesktopNotice, ConfigProblem> {
    let timeout = TIMEOUT.read(config, args)?;
    Ok(DesktopNotice {
        title: TITLE.read(config, args)?,
        body: BODY.read(config, args)?,
        urgency: URGENCY.read(config, args)?,
        icon_path: Some(ICON_PATH.read(config, args)?).filter(|path| !path.is_empty()),
        timeout_ms: u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX),
    })
}

pub struct CoreNotifyShowRunner {
    notify: Arc<dyn NotifyPort>,
}

impl CoreNotifyShowRunner {
    pub fn new(notify: Arc<dyn NotifyPort>) -> Self {
        Self { notify }
    }
}

#[async_trait]
impl SubActionRunner for CoreNotifyShowRunner {
    fn id(&self) -> &str {
        "core.notify.show"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }

    fn label(&self) -> &str {
        "Show Desktop Notification"
    }

    fn summary(&self) -> &str {
        "Show an OS-level notification"
    }

    fn search_text(&self) -> &str {
        "notification notify desktop toast popup os alert"
    }

    fn icon_name(&self) -> &str {
        "bell"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn check_config(
        &self,
        config: &SubActionConfig,
        _interpolation: Interpolation<'_>,
    ) -> Result<(), ConfigProblem> {
        title_length(config)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        let notice = match read_notice(config, ctx.arg_stack) {
            Ok(notice) => notice,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let notify = Arc::clone(&self.notify);
        let outcome = match tokio::task::spawn_blocking(move || notify.show(notice)).await {
            Ok(Ok(())) => SubActionOutcome::Success,
            Ok(Err(e)) => SubActionOutcome::Failed(core_messages::system_service_failed(e).into()),
            Err(e) => SubActionOutcome::Failed(core_messages::task_failed(e).into()),
        };

        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sub_action_runners::os_ports::test_ports::{
        MockErr, NullPublisher, RecordingNotifyPort,
    };
    use forge_types::EventId;

    async fn run(
        notify: Arc<RecordingNotifyPort>,
        stack: ArgStack,
        cfg: SubActionConfig,
    ) -> SubActionOutcome {
        let publisher = NullPublisher;
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &publisher);
        CoreNotifyShowRunner::new(notify)
            .execute(&cfg, &ctx)
            .await
            .0
            .outcome
    }

    fn cfg_with(title: &str, body: &str, urgency: &str, timeout_ms: i64) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("title".to_owned(), Variant::String(title.to_owned()));
        cfg.insert("body".to_owned(), Variant::String(body.to_owned()));
        cfg.insert("urgency".to_owned(), Variant::String(urgency.to_owned()));
        cfg.insert("timeout_ms".to_owned(), Variant::Int(timeout_ms));
        cfg
    }

    #[tokio::test]
    async fn forwards_resolved_fields_to_port() {
        let stack = ArgStack::new().set("who".to_owned(), Variant::String("Alice".to_owned()));
        let port = Arc::new(RecordingNotifyPort::new());
        let cfg = cfg_with("Hi %who%", "Welcome %who%", "critical", 3000);
        let outcome = run(Arc::clone(&port), stack, cfg).await;
        assert!(matches!(outcome, SubActionOutcome::Success));
        let shown = port.shown();
        let notice = &shown[0];
        assert_eq!(notice.title, "Hi Alice");
        assert_eq!(notice.body, "Welcome Alice");
        assert_eq!(notice.urgency, NotifyUrgency::Critical);
        assert_eq!(notice.timeout_ms, 3000);
    }

    #[tokio::test]
    async fn maps_port_error_to_failed_without_panicking() {
        let port = Arc::new(RecordingNotifyPort::failing(MockErr::Unavailable));
        let cfg = cfg_with("Title", "Body", "normal", 5000);
        let outcome = run(Arc::clone(&port), ArgStack::new(), cfg).await;
        assert!(matches!(outcome, SubActionOutcome::Failed(_)));
        assert_eq!(
            port.call_count(),
            1,
            "the notice was dispatched before failing"
        );
    }

    #[tokio::test]
    async fn a_timeout_inside_its_bounds_reaches_the_notice() {
        for timeout in [TIMEOUT_MIN_MS, DEFAULT_TIMEOUT_MS, TIMEOUT_MAX_MS] {
            let port = Arc::new(RecordingNotifyPort::new());
            let cfg = cfg_with("x", "", "normal", timeout);
            let outcome = run(Arc::clone(&port), ArgStack::new(), cfg).await;
            assert!(matches!(outcome, SubActionOutcome::Success), "{timeout}");
            assert_eq!(i64::from(port.shown()[0].timeout_ms), timeout, "{timeout}");
        }
    }

    #[tokio::test]
    async fn a_timeout_outside_its_bounds_fails_without_showing_a_notice() {
        for timeout in [TIMEOUT_MIN_MS - 1, TIMEOUT_MAX_MS + 1] {
            let port = Arc::new(RecordingNotifyPort::new());
            let cfg = cfg_with("x", "", "normal", timeout);
            let outcome = run(Arc::clone(&port), ArgStack::new(), cfg).await;
            assert!(
                matches!(&outcome, SubActionOutcome::Failed(m) if m.code_or_text().contains("'timeout_ms'")),
                "{timeout}: {outcome:?}"
            );
            assert!(port.shown().is_empty(), "{timeout}");
        }
    }

    #[test]
    fn validate_rejects_title_length_outside_one_to_hundred() {
        let runner = CoreNotifyShowRunner::new(Arc::new(RecordingNotifyPort::new()));
        let cases: [(String, bool); 4] = [
            (String::new(), false),   // empty rejected
            ("x".to_owned(), true),   // lower boundary
            ("a".repeat(100), true),  // upper boundary
            ("a".repeat(101), false), // one over upper boundary
        ];
        for (title, expect_ok) in cases {
            let mut cfg = SubActionConfig::new();
            cfg.insert("title".to_owned(), Variant::String(title.clone()));
            assert_eq!(
                runner.validate_config(&cfg).is_ok(),
                expect_ok,
                "title char count {}",
                title.chars().count()
            );
        }
    }

    #[test]
    fn validate_rejects_missing_title_key() {
        let runner = CoreNotifyShowRunner::new(Arc::new(RecordingNotifyPort::new()));
        let cfg = SubActionConfig::new();
        assert!(runner.validate_config(&cfg).is_err());
    }

    #[test]
    fn validate_counts_unicode_scalar_values_not_bytes() {
        let runner = CoreNotifyShowRunner::new(Arc::new(RecordingNotifyPort::new()));
        let mut ok = SubActionConfig::new();
        ok.insert("title".to_owned(), Variant::String("😀".repeat(100)));
        assert!(runner.validate_config(&ok).is_ok());
        let mut too_long = SubActionConfig::new();
        too_long.insert("title".to_owned(), Variant::String("😀".repeat(101)));
        assert!(runner.validate_config(&too_long).is_err());
    }
}
