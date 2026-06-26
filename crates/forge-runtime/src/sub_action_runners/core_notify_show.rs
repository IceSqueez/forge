use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

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
        let title_len = config
            .get("title")
            .and_then(|v| v.as_str())
            .map(|s| s.chars().count())
            .unwrap_or(0);
        if !(1..=100).contains(&title_len) {
            return Err(RegistryError::UnknownKindId(
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
        let started_at = OffsetDateTime::now_utc();

        let title = ctx
            .arg_stack
            .interpolate(config.get("title").and_then(|v| v.as_str()).unwrap_or(""));
        let body = ctx
            .arg_stack
            .interpolate(config.get("body").and_then(|v| v.as_str()).unwrap_or(""));
        let urgency = parse_urgency(config.get("urgency").and_then(|v| v.as_str()).unwrap_or(""));
        let icon_path = config
            .get("icon_path")
            .and_then(|v| v.as_str())
            .map(|s| ctx.arg_stack.interpolate(s))
            .filter(|s| !s.is_empty());
        let timeout_ms = config
            .get("timeout_ms")
            .and_then(|v| v.as_int())
            .unwrap_or(5000)
            .clamp(1000, 60000) as u32;

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

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.notify.show".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}

fn parse_urgency(s: &str) -> NotifyUrgency {
    match s {
        "low" => NotifyUrgency::Low,
        "critical" => NotifyUrgency::Critical,
        _ => NotifyUrgency::Normal,
    }
}
