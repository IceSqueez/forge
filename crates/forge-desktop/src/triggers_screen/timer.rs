use forge_components::{Density, ForgePalette, Spacing, body_family, spacing, tr};
use forge_registry::{TriggerCategory, TriggerKindDescriptor};
use forge_runtime::triggers::{TIMER_TICK_KIND, TimerSchedule};
use forge_types::TriggerConfig;
use gpui::{AnyElement, div, prelude::*};

use crate::config_form::FILL_VAL_FS;

const SUMMARY_SEPARATOR: &str = " \u{b7} ";

pub(crate) fn is_timer_kind(kind_id: &str) -> bool {
    kind_id == TIMER_TICK_KIND
}

pub(crate) fn cooldown_applies(descriptor: Option<&dyn TriggerKindDescriptor>) -> bool {
    descriptor.is_none_or(|d| d.category() != TriggerCategory::Timer)
}

pub(crate) fn timer_condition_summary(config: &TriggerConfig) -> String {
    let schedule = TimerSchedule::from_config(config);
    let mut parts = vec![tr!(
        "triggers_timer_every",
        minutes = i64::from(schedule.interval_minutes)
    )];
    if schedule.only_while_live {
        parts.push(tr!("triggers_timer_live_only"));
    }
    if schedule.min_chat_messages > 0 {
        parts.push(tr!(
            "triggers_timer_min_messages",
            count = i64::from(schedule.min_chat_messages)
        ));
    }
    parts.join(SUMMARY_SEPARATOR)
}

pub(crate) fn condition_display(
    descriptor: Option<&dyn TriggerKindDescriptor>,
    kind_id: &str,
    config: &TriggerConfig,
) -> String {
    if is_timer_kind(kind_id) {
        timer_condition_summary(config)
    } else {
        descriptor
            .map(|d| d.condition_display(config))
            .unwrap_or_default()
    }
}

pub(crate) fn timer_field_hints(kind_id: &str, palette: &ForgePalette) -> Option<AnyElement> {
    if !is_timer_kind(kind_id) {
        return None;
    }
    let hints = [
        tr!("triggers_timer_hint_interval"),
        tr!("triggers_timer_hint_live"),
        tr!("triggers_timer_hint_messages"),
    ];
    Some(
        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .px(spacing(Spacing::Xxs, Density::Cozy))
            .font_family(body_family())
            .text_size(FILL_VAL_FS)
            .text_color(palette.text_faint)
            .children(hints.into_iter().map(|hint| div().child(hint)))
            .into_any_element(),
    )
}
