use super::header::hero_badge_elem;
use super::{IntegrationDetail, VIEWER_DELTA_WINDOW_MINUTES};
use crate::integration_catalog::chat_platform_of;
use crate::user_text::user_text_string;
use forge_components::{ForgePalette, tr};
use forge_platform_core::{HealthMetric, HealthMetricId, HealthText, HealthValue};
use forge_types::{UserText, VerbatimText};
use gpui::{AnyElement, prelude::*};
use std::time::{Duration, SystemTime};

impl IntegrationDetail {
    pub(super) fn augmented_health(&self) -> [HealthMetric; 4] {
        let mut metrics = self.shown.health_metrics.clone();
        if let Some(metric) = metrics.iter_mut().find(|m| m.id == HealthMetricId::VIEWERS)
            && let HealthValue::Text { secondary, .. } = &mut metric.value
        {
            let delta = self.viewer_delta().unwrap_or(0);
            let sign = if delta >= 0 { "+" } else { "" };
            *secondary = Some(HealthText::Text(UserText::Verbatim(VerbatimText::new(
                tr!(
                    "integration_viewers_delta",
                    delta = format!("{sign}{delta}"),
                    window = format!("{VIEWER_DELTA_WINDOW_MINUTES}m")
                ),
            ))));
        }
        metrics
    }

    pub(super) fn hero_badges(&self, palette: &ForgePalette) -> Vec<AnyElement> {
        self.ports
            .status
            .name_badges()
            .into_iter()
            .map(|b| hero_badge_elem(&b, palette).into_any_element())
            .collect()
    }

    pub(super) fn token_remaining(&self) -> Option<Duration> {
        let expiry = self.ports.status.token_expiry()?;
        let remaining = expiry.duration_since(SystemTime::now()).ok()?;
        (remaining.as_secs() > 0).then_some(remaining)
    }

    pub(super) fn token_countdown(&self) -> Option<String> {
        let total = self.token_remaining()?.as_secs();
        let hours = total / 3600;
        let minutes = (total % 3600) / 60;
        Some(tr!(
            "integration_token_expires_in",
            time = format!("{hours}h {minutes}m")
        ))
    }

    pub(super) fn is_oauth_platform(&self) -> bool {
        chat_platform_of(self.ports.status.id()).is_some()
    }

    pub(super) fn hero_subtitle(&self) -> String {
        self.ports
            .status
            .endpoint()
            .map(|endpoint| user_text_string(&endpoint))
            .unwrap_or_default()
    }
}
