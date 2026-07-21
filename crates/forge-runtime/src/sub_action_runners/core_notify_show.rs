use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    FormField, RegistryError, RunContext, StepTimer, SubActionCategory, SubActionConfigExt,
    SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};

use super::os_ports::{DesktopNotice, NotifyPort, NotifyUrgency};

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

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("title".to_owned(), Variant::String(String::new()));
        cfg.insert("body".to_owned(), Variant::String(String::new()));
        cfg.insert("urgency".to_owned(), Variant::String("normal".to_owned()));
        cfg.insert("icon_path".to_owned(), Variant::String(String::new()));
        cfg.insert("timeout_ms".to_owned(), Variant::Int(5000));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "title",
                label: "Title",
                placeholder: "Stream starting",
            },
            FormField::TextArea {
                key: "body",
                label: "Body",
            },
            FormField::Select {
                key: "urgency",
                label: "Urgency (Linux only)",
                options: &["low", "normal", "critical"],
            },
            FormField::Text {
                key: "icon_path",
                label: "Icon Path",
                placeholder: "/path/to/icon.png",
            },
            FormField::Integer {
                key: "timeout_ms",
                label: "Timeout (ms)",
                min: 1000,
                max: 60000,
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        let title_len = config.str("title").map(|s| s.chars().count()).unwrap_or(0);
        if !(1..=100).contains(&title_len) {
            return Err(RegistryError::InvalidConfig(
                "core.notify.show: title length must be 1..=100".to_owned(),
            ));
        }
        Ok(())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "core.notify.show");

        let title = ctx.arg_stack.interpolate(config.str("title").unwrap_or(""));
        let body = ctx.arg_stack.interpolate(config.str("body").unwrap_or(""));
        let urgency = parse_urgency(config.str("urgency").unwrap_or(""));
        let icon_path = config
            .str("icon_path")
            .map(|s| ctx.arg_stack.interpolate(s))
            .filter(|s| !s.is_empty());
        let timeout_ms = config.int("timeout_ms").unwrap_or(5000).clamp(1000, 60000) as u32;

        let notice = DesktopNotice {
            title,
            body,
            urgency,
            icon_path,
            timeout_ms,
        };

        let notify = Arc::clone(&self.notify);
        let outcome = match tokio::task::spawn_blocking(move || notify.show(notice)).await {
            Ok(Ok(())) => SubActionOutcome::Success,
            Ok(Err(e)) => SubActionOutcome::Failed(e.to_string()),
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        (timer.finish(outcome), None)
    }
}

fn parse_urgency(s: &str) -> NotifyUrgency {
    match s {
        "low" => NotifyUrgency::Low,
        "critical" => NotifyUrgency::Critical,
        _ => NotifyUrgency::Normal,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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
    async fn clamps_out_of_range_timeout_instead_of_rejecting() {
        // Out-of-range timeouts are clamped into [1000, 60000], NOT rejected.
        for (input, expected) in [
            (10_i64, 1000_u32),
            (999, 1000),
            (1000, 1000),
            (5000, 5000),
            (60000, 60000),
            (999_999, 60000),
        ] {
            let port = Arc::new(RecordingNotifyPort::new());
            let cfg = cfg_with("x", "", "normal", input);
            let outcome = run(Arc::clone(&port), ArgStack::new(), cfg).await;
            assert!(
                matches!(outcome, SubActionOutcome::Success),
                "input {input}"
            );
            assert_eq!(port.shown()[0].timeout_ms, expected, "input {input}");
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
        // Why: 100 emoji = 100 chars but 400 bytes; a byte-length check would
        // wrongly reject. The boundary is char-counted.
        let runner = CoreNotifyShowRunner::new(Arc::new(RecordingNotifyPort::new()));
        let mut ok = SubActionConfig::new();
        ok.insert("title".to_owned(), Variant::String("😀".repeat(100)));
        assert!(runner.validate_config(&ok).is_ok());
        let mut too_long = SubActionConfig::new();
        too_long.insert("title".to_owned(), Variant::String("😀".repeat(101)));
        assert!(runner.validate_config(&too_long).is_err());
    }
}
