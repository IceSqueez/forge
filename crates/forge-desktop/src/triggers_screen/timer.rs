use forge_components::{Density, ForgePalette, Spacing, body_family, spacing, tr};
use forge_registry::{TriggerCategory, TriggerKindDescriptor};
use forge_runtime::triggers::TIMER_TICK_KIND;
use forge_types::TriggerConfig;
use gpui::{AnyElement, div, prelude::*};

use crate::config_form::FILL_VAL_FS;
use crate::user_text::resolve_user_text;

pub(crate) fn is_timer_kind(kind_id: &str) -> bool {
    kind_id == TIMER_TICK_KIND
}

pub(crate) fn cooldown_applies(descriptor: Option<&dyn TriggerKindDescriptor>) -> bool {
    descriptor.is_none_or(|d| d.category() != TriggerCategory::Timer)
}

pub(crate) fn condition_display(
    descriptor: Option<&dyn TriggerKindDescriptor>,
    config: &TriggerConfig,
) -> String {
    descriptor
        .map(|d| resolve_user_text(&d.condition_display(config)).primary)
        .unwrap_or_default()
}

pub(crate) fn timer_field_hints(
    kind_id: &str,
    palette: &ForgePalette,
    density: Density,
) -> Option<AnyElement> {
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
            .gap(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Xxs, density))
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
            let summary = plain(condition_display(
                Some(&TimerTickDescriptor),
                &timer_config(interval, live, messages),
            ));
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
    fn condition_display_resolves_the_descriptor_text_and_is_empty_without_a_descriptor() {
        install_language(Language::En);
        let config = timer_config(15, true, 0);
        let chat = StubTrigger::new(
            "twitch.chat",
            "Chat",
            TriggerCategory::Chat,
            Declares::Nothing,
        );
        for (descriptor, expected) in [
            (Some(&chat as &dyn TriggerKindDescriptor), "any"),
            (None, ""),
        ] {
            assert_eq!(condition_display(descriptor, &config), expected);
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
