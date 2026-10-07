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
