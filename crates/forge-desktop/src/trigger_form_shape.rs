use forge_registry::{
    FormField, IGNORE_BOT_MESSAGES_KEY, KindPlatformContract, TriggerKindDescriptor,
};
use forge_types::{PlatformId, TriggerConfig, Variant};

const IGNORE_BOT_MESSAGES_LABEL: &str = "Ignore the bot's messages";

fn offers_bot_filter(descriptor: Option<&dyn TriggerKindDescriptor>) -> bool {
    descriptor.is_some_and(|descriptor| {
        descriptor.chat_trigger_family().is_some()
            && matches!(
                descriptor.platform_contract(),
                KindPlatformContract::Universal
                    | KindPlatformContract::PlatformSpecific(PlatformId::Twitch)
            )
    })
}

pub(crate) fn trigger_form_defaults(
    descriptor: Option<&dyn TriggerKindDescriptor>,
) -> TriggerConfig {
    let mut defaults = descriptor
        .map(TriggerKindDescriptor::default_config)
        .unwrap_or_default();
    if offers_bot_filter(descriptor) {
        defaults
            .entry(IGNORE_BOT_MESSAGES_KEY.to_owned())
            .or_insert(Variant::Bool(true));
    }
    defaults
}

pub(crate) fn trigger_form_specs(descriptor: Option<&dyn TriggerKindDescriptor>) -> Vec<FormField> {
    let mut specs = descriptor
        .map(TriggerKindDescriptor::config_fields)
        .unwrap_or_default();
    if offers_bot_filter(descriptor)
        && !specs
            .iter()
            .any(|spec| spec.key() == IGNORE_BOT_MESSAGES_KEY)
    {
        specs.push(FormField::Toggle {
            key: IGNORE_BOT_MESSAGES_KEY,
            label: IGNORE_BOT_MESSAGES_LABEL,
        });
    }
    specs
}

#[cfg(test)]
mod tests {
    use forge_registry::{
        ChatTriggerFamily, FormField, IGNORE_BOT_MESSAGES_KEY, KindPlatformContract,
        TriggerCategory, TriggerKindDescriptor,
    };
    use forge_types::{PlatformId, TriggerConfig, Variant};

    use super::{trigger_form_defaults, trigger_form_specs};
    use crate::config_form::sparse_overrides;
    use crate::test_support::{Declares, StubTrigger};

    const KEYWORD: FormField = FormField::Text {
        key: "keyword",
        label: "Keyword",
        placeholder: "",
    };

    fn chat_trigger(family: ChatTriggerFamily) -> StubTrigger {
        StubTrigger::new(
            "twitch.chat.message",
            "Chat message",
            TriggerCategory::Chat,
            Declares::Nothing,
        )
        .with_fields(vec![KEYWORD])
        .in_chat_family(family)
    }

    fn follow_trigger() -> StubTrigger {
        StubTrigger::new(
            "twitch.follow",
            "Follow",
            TriggerCategory::Chat,
            Declares::Nothing,
        )
        .with_fields(vec![KEYWORD])
    }

    fn bot_toggles(specs: &[FormField]) -> usize {
        specs
            .iter()
            .filter(|spec| spec.key() == IGNORE_BOT_MESSAGES_KEY)
            .count()
    }

    #[test]
    fn every_chat_family_form_gains_one_ignore_bot_toggle_after_its_own_fields() {
        for family in [ChatTriggerFamily::Message, ChatTriggerFamily::Command] {
            let descriptor = chat_trigger(family);

            let specs = trigger_form_specs(Some(&descriptor as &dyn TriggerKindDescriptor));

            assert_eq!(
                specs.first().map(FormField::key),
                Some("keyword"),
                "{family:?}"
            );
            assert!(
                matches!(specs.last(), Some(FormField::Toggle { key, .. }) if *key == IGNORE_BOT_MESSAGES_KEY),
                "{family:?}"
            );
            assert_eq!(bot_toggles(&specs), 1, "{family:?}");
        }
    }

    #[test]
    fn chat_family_form_defaults_ignore_the_bot() {
        let descriptor = chat_trigger(ChatTriggerFamily::Message);

        let defaults = trigger_form_defaults(Some(&descriptor as &dyn TriggerKindDescriptor));

        assert_eq!(
            defaults.get(IGNORE_BOT_MESSAGES_KEY),
            Some(&Variant::Bool(true))
        );
    }

    #[test]
    fn non_chat_and_unknown_kinds_get_no_bot_toggle_or_default() {
        let follow = follow_trigger();
        for descriptor in [Some(&follow as &dyn TriggerKindDescriptor), None] {
            let specs = trigger_form_specs(descriptor);
            let defaults = trigger_form_defaults(descriptor);

            assert_eq!(bot_toggles(&specs), 0);
            assert!(!defaults.contains_key(IGNORE_BOT_MESSAGES_KEY));
        }
    }

    #[test]
    fn a_descriptor_declaring_the_toggle_itself_keeps_its_field_and_default() {
        let declared = FormField::Toggle {
            key: IGNORE_BOT_MESSAGES_KEY,
            label: "Skip bot",
        };
        let descriptor = chat_trigger(ChatTriggerFamily::Command)
            .with_fields(vec![declared])
            .with_defaults(TriggerConfig::from([(
                IGNORE_BOT_MESSAGES_KEY.to_owned(),
                Variant::Bool(false),
            )]));
        let descriptor = &descriptor as &dyn TriggerKindDescriptor;

        let specs = trigger_form_specs(Some(descriptor));
        let defaults = trigger_form_defaults(Some(descriptor));

        assert!(
            matches!(
                specs.as_slice(),
                [FormField::Toggle {
                    label: "Skip bot",
                    ..
                }]
            ),
            "the descriptor's own toggle must not be duplicated"
        );
        assert_eq!(
            defaults.get(IGNORE_BOT_MESSAGES_KEY),
            Some(&Variant::Bool(false))
        );
    }

    #[test]
    fn saving_a_chat_trigger_stores_the_bot_option_only_when_switched_off() {
        let descriptor = chat_trigger(ChatTriggerFamily::Message);
        let defaults = trigger_form_defaults(Some(&descriptor as &dyn TriggerKindDescriptor));
        let mut switched_off = defaults.clone();
        switched_off.insert(IGNORE_BOT_MESSAGES_KEY.to_owned(), Variant::Bool(false));

        let untouched = sparse_overrides(&defaults, &defaults);
        let off = sparse_overrides(&defaults, &switched_off);

        assert!(untouched.is_empty());
        assert_eq!(
            off,
            TriggerConfig::from([(IGNORE_BOT_MESSAGES_KEY.to_owned(), Variant::Bool(false))])
        );
    }

    #[test]
    fn the_bot_toggle_is_offered_only_where_a_twitch_bot_message_can_arrive() {
        for (contract, offered) in [
            (KindPlatformContract::Universal, true),
            (
                KindPlatformContract::PlatformSpecific(PlatformId::Twitch),
                true,
            ),
            (
                KindPlatformContract::PlatformSpecific(PlatformId::Kick),
                false,
            ),
            (
                KindPlatformContract::PlatformSpecific(PlatformId::YouTube),
                false,
            ),
        ] {
            let descriptor = chat_trigger(ChatTriggerFamily::Message).with_contract(contract);
            let descriptor = &descriptor as &dyn TriggerKindDescriptor;

            let specs = trigger_form_specs(Some(descriptor));
            let defaults = trigger_form_defaults(Some(descriptor));

            assert_eq!(
                (
                    bot_toggles(&specs) == 1,
                    defaults.contains_key(IGNORE_BOT_MESSAGES_KEY)
                ),
                (offered, offered),
                "{contract:?}"
            );
        }
    }
}
