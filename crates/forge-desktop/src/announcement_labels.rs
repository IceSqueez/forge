use forge_components::tr;

const ANNOUNCEMENT_COLOR_KEY: &str = "color";

pub(crate) fn announcement_color_label(key: &str, option: &str) -> Option<String> {
    if key != ANNOUNCEMENT_COLOR_KEY {
        return None;
    }
    let label = match option {
        "primary" => tr!("announcement_color_primary"),
        "blue" => tr!("announcement_color_blue"),
        "green" => tr!("announcement_color_green"),
        "orange" => tr!("announcement_color_orange"),
        "purple" => tr!("announcement_color_purple"),
        _ => return None,
    };
    Some(label)
}

#[cfg(test)]
mod tests {
    use forge_storage::Language;

    use super::announcement_color_label;
    use crate::config_field_label::localized_label;
    use crate::i18n::install_language;

    const COLORS: [&str; 5] = ["primary", "blue", "green", "orange", "purple"];

    #[test]
    fn every_announcement_color_gets_its_own_translated_label_in_each_language() {
        for language in [Language::En, Language::Uk] {
            install_language(language);

            let labels: Vec<String> = COLORS
                .iter()
                .filter_map(|color| announcement_color_label("color", color))
                .collect();

            assert_eq!(labels.len(), COLORS.len(), "{language:?}");
            for label in &labels {
                assert!(
                    !label.starts_with("announcement_color_"),
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
    fn color_labels_apply_only_to_known_colors_of_the_color_field() {
        for (key, option) in [
            ("color", "red"),
            ("color", ""),
            ("color", "Blue"),
            ("sender", "blue"),
        ] {
            assert_eq!(
                announcement_color_label(key, option),
                None,
                "{key}={option}"
            );
        }
    }

    #[test]
    fn the_color_and_send_as_field_labels_are_translated() {
        install_language(Language::Uk);
        for english in ["Color", "Send As"] {
            let localized = localized_label(english);

            assert!(
                localized != english && !localized.starts_with("config_field_label_"),
                "{english}: {localized}"
            );
        }
    }
}
