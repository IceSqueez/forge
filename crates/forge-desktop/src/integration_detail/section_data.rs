use super::header::hero_badge_elem;
use super::{IntegrationDetail, VIEWER_DELTA_WINDOW_MINUTES};
use crate::user_text::user_text_string;
use forge_components::{ForgePalette, tr};
use forge_platform_core::{DetailSection, HealthMetric, HealthMetricId, HealthText, HealthValue};
use forge_types::{UserText, VerbatimText};
use gpui::{AnyElement, prelude::*};
use std::collections::HashMap;

impl IntegrationDetail {
    pub(super) fn augmented_sections(&self) -> Vec<DetailSection> {
        let mut sections = self.sections.clone();
        for section in &mut sections {
            if let DetailSection::TwoColumn { right, .. } = section {
                Self::fill_subscription_counts(right, &self.eventsub_tally);
            } else {
                Self::fill_subscription_counts(section, &self.eventsub_tally);
            }
        }
        sections
    }

    pub(super) fn fill_subscription_counts(
        section: &mut DetailSection,
        tally: &HashMap<String, u64>,
    ) {
        if let DetailSection::SubscriptionList { items, .. } = section {
            for row in items.iter_mut() {
                if row.error_label.is_none() {
                    row.event_count = Some(tally.get(&row.kind).copied().unwrap_or(0));
                }
            }
        }
    }

    pub(super) fn augmented_health(&self) -> [HealthMetric; 4] {
        let mut metrics = self.health_metrics.clone();
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
        self.status
            .name_badges()
            .into_iter()
            .map(|b| hero_badge_elem(&b, palette).into_any_element())
            .collect()
    }

    pub(super) fn token_countdown(&self) -> Option<String> {
        let expiry = self.status.token_expiry()?;
        let remaining = expiry.duration_since(std::time::SystemTime::now()).ok()?;
        let total = remaining.as_secs();
        if total == 0 {
            return None;
        }
        let hours = total / 3600;
        let minutes = (total % 3600) / 60;
        Some(tr!(
            "integration_token_expires_in",
            time = format!("{hours}h {minutes}m")
        ))
    }

    pub(super) fn is_oauth_platform(&self) -> bool {
        matches!(self.status.id().as_str(), "twitch" | "youtube" | "kick")
    }

    pub(super) fn hero_subtitle(&self) -> String {
        self.status
            .endpoint()
            .map(|endpoint| user_text_string(&endpoint))
            .unwrap_or_default()
    }
}
