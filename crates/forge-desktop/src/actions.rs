use std::collections::HashMap;

use gpui::{App, KeyBinding, Keystroke, actions};

pub const SHELL_CONTEXT: &str = "ForgeShell";
pub const LIST_CONTEXT: &str = "ForgeList";

actions!(
    forge_shell,
    [GoHome, GoChat, GoActions, GoTriggers, GoTwitch, GoSettings]
);

actions!(forge_list, [ListSelectPrev, ListSelectNext, ListActivate]);

pub fn bind_list_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", ListSelectPrev, Some(LIST_CONTEXT)),
        KeyBinding::new("down", ListSelectNext, Some(LIST_CONTEXT)),
        KeyBinding::new("enter", ListActivate, Some(LIST_CONTEXT)),
    ]);
}

pub struct ShortcutEntry {
    pub id: &'static str,
    pub default_chord: &'static str,
    pub label_key: &'static str,
}

pub const SHORTCUTS: &[ShortcutEntry] = &[
    ShortcutEntry {
        id: "nav.home",
        default_chord: "ctrl-1",
        label_key: "settings_shortcuts_action_nav_home",
    },
    ShortcutEntry {
        id: "nav.chat",
        default_chord: "ctrl-2",
        label_key: "settings_shortcuts_action_nav_live_chat",
    },
    ShortcutEntry {
        id: "nav.actions",
        default_chord: "ctrl-3",
        label_key: "settings_shortcuts_action_nav_actions",
    },
    ShortcutEntry {
        id: "nav.triggers",
        default_chord: "ctrl-4",
        label_key: "settings_shortcuts_action_nav_triggers",
    },
    ShortcutEntry {
        id: "nav.twitch",
        default_chord: "ctrl-5",
        label_key: "settings_shortcuts_action_nav_twitch",
    },
    ShortcutEntry {
        id: "nav.settings",
        default_chord: "ctrl-6",
        label_key: "settings_shortcuts_action_nav_settings",
    },
];

fn is_modifier_token(token: &str) -> bool {
    matches!(
        token,
        "ctrl"
            | "alt"
            | "shift"
            | "cmd"
            | "super"
            | "win"
            | "fn"
            | "secondary"
            | "control"
            | "platform"
            | "function"
    )
}

fn has_strong_modifier(chord: &str) -> bool {
    let segments: Vec<&str> = chord.split('-').collect();
    if segments.len() < 2 {
        return false;
    }
    segments[..segments.len() - 1]
        .iter()
        .any(|token| matches!(*token, "ctrl" | "alt" | "cmd" | "super" | "win"))
}

pub fn chord_is_bindable(chord: &str) -> bool {
    let Some(key) = chord.rsplit('-').next() else {
        return false;
    };
    if key.is_empty() || is_modifier_token(key) {
        return false;
    }
    let is_f_key = key
        .strip_prefix('f')
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=12).contains(&n));
    has_strong_modifier(chord) || is_f_key
}

pub fn canonical_chord(keystroke: &Keystroke) -> Option<String> {
    let key = keystroke.key.as_str();
    if key.is_empty() || is_modifier_token(key) {
        return None;
    }
    let modifiers = &keystroke.modifiers;
    let ctrl = modifiers.control || modifiers.platform;
    let mut out = String::new();
    if modifiers.function {
        out.push_str("fn-");
    }
    if ctrl {
        out.push_str("ctrl-");
    }
    if modifiers.alt {
        out.push_str("alt-");
    }
    if modifiers.shift {
        out.push_str("shift-");
    }
    out.push_str(key);
    Some(out)
}

fn cap_token(token: &str) -> String {
    match token {
        "ctrl" | "control" => "Ctrl".to_owned(),
        "alt" => "Alt".to_owned(),
        "shift" => "Shift".to_owned(),
        "cmd" | "super" | "win" | "platform" => "Meta".to_owned(),
        "fn" | "function" | "secondary" => "Fn".to_owned(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect(),
                None => String::new(),
            }
        }
    }
}

/// Rewrites a gpui chord (`ctrl-shift-r`) into the `+`-separated form the keycap row splits on.
pub fn chord_caps(chord: &str) -> String {
    let mut caps: Vec<String> = Vec::new();
    let mut rest = chord;
    while let Some((head, tail)) = rest.split_once('-') {
        if !is_modifier_token(head) || tail.is_empty() {
            break;
        }
        caps.push(cap_token(head));
        rest = tail;
    }
    caps.push(cap_token(rest));
    caps.join("+")
}

pub fn shortcut_entry(id: &str) -> Option<&'static ShortcutEntry> {
    SHORTCUTS.iter().find(|entry| entry.id == id)
}

pub fn effective_chord<'a>(
    overrides: &'a HashMap<String, String>,
    entry: &'a ShortcutEntry,
) -> Option<&'a str> {
    match overrides.get(entry.id) {
        Some(stored) if chord_is_bindable(stored) => Some(stored.as_str()),
        Some(_) => None,
        None => Some(entry.default_chord),
    }
}

pub fn parse_stored_overrides(raw: &str) -> HashMap<String, String> {
    let parsed: HashMap<String, String> = match serde_json::from_str(raw) {
        Ok(map) => map,
        Err(e) => {
            tracing::warn!(error = %e, "stored keyboard shortcuts unreadable; using defaults");
            return HashMap::new();
        }
    };
    parsed
        .into_iter()
        .filter(|(id, _)| SHORTCUTS.iter().any(|entry| entry.id == id))
        .collect()
}

fn make_binding(entry_id: &str, chord: &str) -> Option<KeyBinding> {
    if Keystroke::parse(chord).is_err() {
        return None;
    }
    let context = Some(SHELL_CONTEXT);
    let binding = match entry_id {
        "nav.home" => KeyBinding::new(chord, GoHome, context),
        "nav.chat" => KeyBinding::new(chord, GoChat, context),
        "nav.actions" => KeyBinding::new(chord, GoActions, context),
        "nav.triggers" => KeyBinding::new(chord, GoTriggers, context),
        "nav.twitch" => KeyBinding::new(chord, GoTwitch, context),
        "nav.settings" => KeyBinding::new(chord, GoSettings, context),
        _ => return None,
    };
    Some(binding)
}

fn key_bindings_for(entry_id: &str, chord: &str) -> Vec<KeyBinding> {
    let mut chords = vec![chord.to_owned()];
    let platform_variant = chord.replacen("ctrl-", "cmd-", 1);
    if platform_variant != chord {
        chords.push(platform_variant);
    }
    chords
        .iter()
        .filter_map(|chord| make_binding(entry_id, chord))
        .collect()
}

fn bind_shell(cx: &mut App, overrides: &HashMap<String, String>) {
    let mut bindings = Vec::new();
    for entry in SHORTCUTS {
        if let Some(chord) = effective_chord(overrides, entry) {
            bindings.extend(key_bindings_for(entry.id, chord));
        }
    }
    cx.bind_keys(bindings);
}

pub fn register_shell_key_bindings(cx: &mut App) {
    bind_shell(cx, &HashMap::new());
}

pub fn reapply_key_bindings(cx: &mut App, overrides: &HashMap<String, String>) {
    cx.clear_key_bindings();
    forge_components::bind_text_input_keys(cx);
    forge_components::bind_text_area_keys(cx);
    forge_components::bind_picker_keys(cx);
    bind_list_keys(cx);
    bind_shell(cx, overrides);
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn en_catalog() -> String {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("locales")
            .join("en")
            .join("main.ftl");
        std::fs::read_to_string(&path).expect("en catalog is readable")
    }

    #[test]
    fn chord_caps_rewrites_a_gpui_chord_into_plus_separated_keycaps() {
        let cases = [
            ("ctrl-1", "Ctrl+1"),
            ("ctrl-shift-r", "Ctrl+Shift+R"),
            ("alt-shift-f12", "Alt+Shift+F12"),
            ("cmd-k", "Meta+K"),
            ("super-k", "Meta+K"),
            ("fn-f1", "Fn+F1"),
            ("f5", "F5"),
            ("", ""),
        ];

        for (chord, expected) in cases {
            assert_eq!(chord_caps(chord), expected, "wrong caps for {chord:?}");
        }
    }

    #[test]
    fn chord_is_bindable_requires_a_strong_modifier_or_a_function_key() {
        let bindable = ["ctrl-1", "alt-r", "cmd-k", "super-k", "win-k", "f1", "f12"];
        let rejected = [
            "shift-r",
            "r",
            "f13",
            "f0",
            "",
            "ctrl",
            "ctrl-shift",
            "fn-r",
        ];

        for chord in bindable {
            assert!(chord_is_bindable(chord), "expected bindable: {chord:?}");
        }
        for chord in rejected {
            assert!(!chord_is_bindable(chord), "expected rejected: {chord:?}");
        }
    }

    #[test]
    fn every_shortcut_in_the_roster_is_bindable_resolvable_and_labelled() {
        let catalog = en_catalog();
        let mut seen: Vec<&str> = Vec::new();

        for entry in SHORTCUTS {
            assert!(
                !seen.contains(&entry.id),
                "duplicate shortcut id {:?} - owner_of and shortcut_entry would pick the first",
                entry.id
            );
            seen.push(entry.id);

            assert!(
                chord_is_bindable(entry.default_chord),
                "{}: default chord {:?} is not bindable, so effective_chord reports the shortcut as unbound",
                entry.id,
                entry.default_chord
            );
            assert!(
                make_binding(entry.id, entry.default_chord).is_some(),
                "{}: no make_binding arm, so the shortcut never reaches cx.bind_keys",
                entry.id
            );
            assert!(
                catalog.contains(&format!("\n{} = ", entry.label_key)),
                "{}: label key {:?} is missing from the en catalog",
                entry.id,
                entry.label_key
            );
        }
    }

    #[test]
    fn parse_stored_overrides_drops_ids_the_roster_no_longer_defines() {
        let raw = r#"{"nav.home":"ctrl-9","nav.gone":"ctrl-8"}"#;

        let parsed = parse_stored_overrides(raw);

        assert_eq!(parsed.get("nav.home").map(String::as_str), Some("ctrl-9"));
        assert!(!parsed.contains_key("nav.gone"));
    }

    #[test]
    fn parse_stored_overrides_falls_back_to_defaults_when_the_setting_is_unreadable() {
        for raw in ["", "not json", "[]", r#"{"nav.home":7}"#] {
            assert!(
                parse_stored_overrides(raw).is_empty(),
                "expected defaults for {raw:?}"
            );
        }
    }

    #[test]
    fn effective_chord_separates_an_unbound_shortcut_from_an_untouched_one() {
        let entry = shortcut_entry("nav.home").expect("nav.home is in the roster");
        let mut overrides = HashMap::new();

        assert_eq!(effective_chord(&overrides, entry), Some("ctrl-1"));

        overrides.insert("nav.home".to_owned(), String::new());
        assert_eq!(
            effective_chord(&overrides, entry),
            None,
            "an unbindable stored chord means unbound, not back to the default"
        );

        overrides.insert("nav.home".to_owned(), "ctrl-9".to_owned());
        assert_eq!(effective_chord(&overrides, entry), Some("ctrl-9"));
    }
}
