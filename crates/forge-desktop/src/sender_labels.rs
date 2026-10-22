use forge_components::tr;
use forge_platform_core::{SENDER_FIELD, SenderPreference};

const PLATFORM_NEUTRAL_SENDER_STEPS: [&str; 1] = ["twitch.chat.send_message"];

pub(crate) fn sender_option_label(key: &str, option: &str) -> Option<String> {
    if key != SENDER_FIELD {
        return None;
    }
    let label = match SenderPreference::parse(option)? {
        SenderPreference::Default => tr!("sender_option_default"),
        SenderPreference::Streamer => tr!("sender_option_streamer"),
        SenderPreference::BotOnly => tr!("sender_option_bot"),
    };
    Some(label)
}

pub(crate) fn sender_field_helper(kind_id: &str, key: &str) -> Option<String> {
    (key == SENDER_FIELD && PLATFORM_NEUTRAL_SENDER_STEPS.contains(&kind_id))
        .then(|| tr!("sender_field_twitch_only_helper"))
}
