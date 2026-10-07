use forge_components::tr;

pub(super) const DEFAULT_VOICE: &str = "";

pub(super) fn speech_voice_choices(
    voices: &[(String, String)],
    selected: &str,
) -> Vec<(String, String)> {
    let mut options = vec![(DEFAULT_VOICE.to_owned(), tr!("overlays_voice_default"))];
    options.extend(voices.iter().cloned());
    let listed = selected == DEFAULT_VOICE || voices.iter().any(|(value, _)| value == selected);
    if !listed {
        options.push((
            selected.to_owned(),
            tr!("overlays_voice_not_installed", voice = selected),
        ));
    }
    options
}

#[cfg(test)]
mod tests {
    use super::*;

    fn installed() -> Vec<(String, String)> {
        vec![
            ("piper/amy".to_owned(), "Piper - amy (en_US)".to_owned()),
            ("sapi/zira".to_owned(), "Microsoft SAPI 5 - Zira".to_owned()),
        ]
    }

    fn values(options: &[(String, String)]) -> Vec<&str> {
        options.iter().map(|(value, _)| value.as_str()).collect()
    }

    #[test]
    fn the_default_voice_leads_the_installed_voices() {
        crate::i18n::install_language(forge_storage::Language::En);
        for (voices, selected, expected) in [
            (installed(), "", vec!["", "piper/amy", "sapi/zira"]),
            (installed(), "sapi/zira", vec!["", "piper/amy", "sapi/zira"]),
            (Vec::new(), "", vec![""]),
        ] {
            let options = speech_voice_choices(&voices, selected);

            assert_eq!(values(&options), expected, "selected {selected:?}");
            assert_eq!(options[0].1, "Default voice");
        }
    }

    #[test]
    fn a_stored_voice_that_is_not_installed_stays_listed_and_marked() {
        crate::i18n::install_language(forge_storage::Language::En);
        for (voices, selected) in [(installed(), "piper/ghost"), (Vec::new(), "piper/amy")] {
            let options = speech_voice_choices(&voices, selected);

            let (value, label) = options.last().cloned().unwrap_or_default();
            assert_eq!(
                (
                    options.len(),
                    value.as_str(),
                    label.replace(['\u{2068}', '\u{2069}'], "")
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
