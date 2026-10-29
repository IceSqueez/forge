use forge_components::tr;
use forge_platform_core::{SENDER_FIELD, SenderPreference};
use forge_runtime::sub_action_runners::CHAT_SEND_KIND_ID;

const PLATFORM_NEUTRAL_SENDER_STEPS: [&str; 1] = [CHAT_SEND_KIND_ID];

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

#[cfg(test)]
mod tests {
    use forge_platform_core::{SENDER_FIELD, SenderPreference};

    use forge_storage::Language;

    use super::{sender_field_helper, sender_option_label};
    use crate::i18n::install_language;

    #[test]
    fn every_sender_option_gets_its_own_translated_label_in_each_language() {
        for language in [Language::En, Language::Uk] {
            install_language(language);

            let labels: Vec<String> = SenderPreference::ALL
                .into_iter()
                .filter_map(|sender| sender_option_label(SENDER_FIELD, sender.as_str()))
                .collect();

            assert_eq!(labels.len(), SenderPreference::ALL.len(), "{language:?}");
            for label in &labels {
                assert!(
                    !label.starts_with("sender_option_"),
                    "{language:?}: {label}"
                );
                assert_eq!(
                    labels.iter().filter(|other| *other == label).count(),
                    1,
                    "{language:?}: {label}"
                );
            }
        }
    }

    #[test]
    fn sender_labels_apply_only_to_known_options_of_the_sender_field() {
        for (key, option) in [
            (SENDER_FIELD, "moderator"),
            (SENDER_FIELD, ""),
            (SENDER_FIELD, "Bot"),
            ("color", "bot"),
        ] {
            assert_eq!(sender_option_label(key, option), None, "{key}={option}");
        }
    }

    #[test]
    fn twitch_only_helper_shows_only_under_the_platform_neutral_send_step_sender() {
        for (kind, key, shown) in [
            ("twitch.chat.send_message", SENDER_FIELD, true),
            ("twitch.chat.send_message", "message", false),
            ("twitch.chat.reply", SENDER_FIELD, false),
            ("twitch.chat.send_whisper", SENDER_FIELD, false),
        ] {
            assert_eq!(
                sender_field_helper(kind, key).is_some(),
                shown,
                "{kind}/{key}"
            );
        }
    }
}
