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

#[cfg(test)]
mod tests {
    use forge_runtime::triggers::TimerTickDescriptor;
    use forge_storage::Language;
    use forge_types::Variant;

    use super::*;
    use crate::i18n::install_language;
    use crate::test_support::{Declares, StubTrigger};

    fn timer_config(interval: i64, live: bool, messages: i64) -> TriggerConfig {
        let mut config = TriggerConfig::new();
        config.insert("interval_minutes".to_owned(), Variant::Int(interval));
        config.insert("only_while_live".to_owned(), Variant::Bool(live));
        config.insert("min_chat_messages".to_owned(), Variant::Int(messages));
        config
    }

    fn plain(text: String) -> String {
        text.replace(['\u{2068}', '\u{2069}'], "")
    }

    fn assert_summaries(language: Language, cases: &[(i64, bool, i64, &str)]) {
        install_language(language);
        for &(interval, live, messages, expected) in cases {
            let summary = plain(timer_condition_summary(&timer_config(
                interval, live, messages,
            )));
            assert_eq!(
                summary, expected,
                "{language:?} interval={interval} live={live} messages={messages}"
            );
        }
    }

    #[test]
    fn english_summary_lists_only_the_active_conditions() {
        assert_summaries(
            Language::En,
            &[
                (1, false, 0, "every 1 min"),
                (60, true, 0, "every 60 min \u{b7} only while live"),
                (1440, false, 1, "every 1440 min \u{b7} \u{2265}1 message"),
                (
                    60,
                    true,
                    1,
                    "every 60 min \u{b7} only while live \u{b7} \u{2265}1 message",
                ),
                (
                    30,
                    true,
                    5,
                    "every 30 min \u{b7} only while live \u{b7} \u{2265}5 messages",
                ),
                (
                    1440,
                    false,
                    1000,
                    "every 1440 min \u{b7} \u{2265}1000 messages",
                ),
            ],
        );
    }

    #[test]
    fn ukrainian_summary_lists_only_the_active_conditions_with_plural_forms() {
        assert_summaries(
            Language::Uk,
            &[
                (1, false, 0, "кожні 1 хв"),
                (60, true, 0, "кожні 60 хв \u{b7} лише під час ефіру"),
                (
                    1440,
                    false,
                    1,
                    "кожні 1440 хв \u{b7} \u{2265}1 повідомлення",
                ),
                (
                    60,
                    true,
                    2,
                    "кожні 60 хв \u{b7} лише під час ефіру \u{b7} \u{2265}2 повідомлення",
                ),
                (
                    30,
                    true,
                    5,
                    "кожні 30 хв \u{b7} лише під час ефіру \u{b7} \u{2265}5 повідомлень",
                ),
                (1, false, 11, "кожні 1 хв \u{b7} \u{2265}11 повідомлень"),
                (1, false, 21, "кожні 1 хв \u{b7} \u{2265}21 повідомлення"),
            ],
        );
    }

    #[test]
    fn condition_display_uses_the_localized_summary_for_the_timer_kind() {
        install_language(Language::En);
        let config = timer_config(15, true, 0);
        let shown = plain(condition_display(
            Some(&TimerTickDescriptor),
            TIMER_TICK_KIND,
            &config,
        ));
        assert_eq!(shown, "every 15 min \u{b7} only while live");
    }

    #[test]
    fn condition_display_defers_to_the_descriptor_for_other_kinds() {
        install_language(Language::En);
        let config = timer_config(15, true, 0);
        let chat = StubTrigger::new(
            "twitch.chat",
            "Chat",
            TriggerCategory::Chat,
            Declares::Nothing,
        );
        for descriptor in [Some(&chat as &dyn TriggerKindDescriptor), None] {
            assert_eq!(condition_display(descriptor, "twitch.chat", &config), "");
        }
    }

    #[test]
    fn cooldown_applies_to_every_trigger_except_timers() {
        let chat = StubTrigger::new(
            "twitch.chat",
            "Chat",
            TriggerCategory::Chat,
            Declares::Nothing,
        );
        for (case, descriptor, expected) in [
            (
                "timer",
                Some(&TimerTickDescriptor as &dyn TriggerKindDescriptor),
                false,
            ),
            ("chat", Some(&chat as &dyn TriggerKindDescriptor), true),
            ("unknown kind", None, true),
        ] {
            assert_eq!(cooldown_applies(descriptor), expected, "{case}");
        }
    }
}
