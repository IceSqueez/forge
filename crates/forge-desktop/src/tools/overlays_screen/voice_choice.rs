use crate::shared::option_choices::ChoiceOption;
use forge_components::tr;

pub(super) const DEFAULT_VOICE: &str = "";

pub(super) fn speech_voice_choices(voices: &[ChoiceOption], selected: &str) -> Vec<ChoiceOption> {
    let mut options = vec![ChoiceOption::new(
        DEFAULT_VOICE,
        tr!("overlays_voice_default"),
    )];
    options.extend(voices.iter().cloned());
    let listed = selected == DEFAULT_VOICE || voices.iter().any(|option| option.value == selected);
    if !listed {
        options.push(ChoiceOption::new(
            selected,
            tr!("overlays_voice_not_installed", voice = selected),
        ));
    }
    options
}

#[cfg(test)]
mod tests {
    use super::*;

    fn installed() -> Vec<ChoiceOption> {
        vec![
            ChoiceOption::new("piper/amy", "Piper - amy (en_US)"),
            ChoiceOption::new("sapi/zira", "Microsoft SAPI 5 - Zira"),
        ]
    }

    fn values(options: &[ChoiceOption]) -> Vec<&str> {
        options.iter().map(|option| option.value.as_str()).collect()
    }

    #[test]
    fn the_default_voice_leads_the_installed_voices() {
        crate::app::i18n::install_language(forge_storage::Language::En);
        for (voices, selected, expected) in [
            (installed(), "", vec!["", "piper/amy", "sapi/zira"]),
            (installed(), "sapi/zira", vec!["", "piper/amy", "sapi/zira"]),
            (Vec::new(), "", vec![""]),
        ] {
            let options = speech_voice_choices(&voices, selected);

            assert_eq!(values(&options), expected, "selected {selected:?}");
            assert_eq!(options[0].label, "Default voice");
        }
    }

    #[test]
    fn a_stored_voice_that_is_not_installed_stays_listed_and_marked() {
        crate::app::i18n::install_language(forge_storage::Language::En);
        for (voices, selected) in [(installed(), "piper/ghost"), (Vec::new(), "piper/amy")] {
            let options = speech_voice_choices(&voices, selected);

            let last = options.last().expect("the list is never empty");
            assert_eq!(
                (
                    options.len(),
                    last.value.as_str(),
                    last.label.replace(crate::test_support::FORMAT_ISOLATES, "")
                ),
                (
                    voices.len() + 2,
                    selected,
                    format!("{selected} - not installed")
                ),
                "{selected:?}"
            );
        }
    }
}
