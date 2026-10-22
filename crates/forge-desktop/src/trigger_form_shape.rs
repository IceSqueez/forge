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
