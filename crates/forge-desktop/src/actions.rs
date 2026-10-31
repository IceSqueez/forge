use std::collections::HashMap;

use forge_components::FocusNextField;
use gpui::{App, KeyBinding, Keystroke, actions};

pub const SHELL_CONTEXT: &str = "ForgeShell";
pub const LIST_CONTEXT: &str = "ForgeList";
const NEXT_FIELD_CHORD: &str = "tab";
const QUIT_CHORD: &str = "cmd-q";

actions!(
    forge_shell,
    [GoHome, GoChat, GoActions, GoTriggers, GoTwitch, GoSettings]
);

actions!(
    forge_commands,
    [
        SaveFocused,
        NewAction,
        ToggleLiveChat,
        ToggleEventFeed,
        RunFocused
    ]
);

actions!(forge_list, [ListSelectPrev, ListSelectNext, ListActivate]);

actions!(forge_app, [Quit]);

fn bind_list_keys(cx: &mut App) {
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
    ShortcutEntry {
        id: "app.save",
        default_chord: "ctrl-s",
        label_key: "settings_shortcuts_action_save",
    },
    ShortcutEntry {
        id: "app.new_action",
        default_chord: "ctrl-n",
        label_key: "settings_shortcuts_action_new_action",
    },
    ShortcutEntry {
        id: "app.toggle_live_chat",
        default_chord: "ctrl-shift-c",
        label_key: "settings_shortcuts_action_toggle_live_chat",
    },
    ShortcutEntry {
        id: "app.toggle_event_feed",
        default_chord: "ctrl-shift-e",
        label_key: "settings_shortcuts_action_toggle_event_feed",
    },
    ShortcutEntry {
        id: "app.run",
        default_chord: "f5",
        label_key: "settings_shortcuts_action_run_script",
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
    let mut out = String::new();
    if modifiers.function {
        out.push_str("fn-");
    }
    if modifiers.control {
        out.push_str("ctrl-");
    }
    if modifiers.platform {
        out.push_str("cmd-");
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
        "secondary" if cfg!(target_os = "macos") => "Meta".to_owned(),
        "secondary" => "Ctrl".to_owned(),
        "fn" | "function" => "Fn".to_owned(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect(),
                None => String::new(),
            }
        }
    }
}

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

pub fn platform_default_chord(entry: &ShortcutEntry) -> String {
    let primary = if cfg!(target_os = "macos") {
        "cmd-"
    } else {
        "ctrl-"
    };
    entry.default_chord.replacen("ctrl-", primary, 1)
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
        "app.save" => KeyBinding::new(chord, SaveFocused, context),
        "app.new_action" => KeyBinding::new(chord, NewAction, context),
        "app.toggle_live_chat" => KeyBinding::new(chord, ToggleLiveChat, context),
        "app.toggle_event_feed" => KeyBinding::new(chord, ToggleEventFeed, context),
        "app.run" => KeyBinding::new(chord, RunFocused, context),
        _ => return None,
    };
    Some(binding)
}

fn keymap_chord(overrides: &HashMap<String, String>, entry: &ShortcutEntry) -> Option<String> {
    if overrides.contains_key(entry.id) {
        effective_chord(overrides, entry).map(str::to_owned)
    } else {
        Some(entry.default_chord.replacen("ctrl-", "secondary-", 1))
    }
}

fn bind_shell(cx: &mut App, overrides: &HashMap<String, String>) {
    let mut bindings = vec![KeyBinding::new(
        NEXT_FIELD_CHORD,
        FocusNextField,
        Some(SHELL_CONTEXT),
    )];
    for entry in SHORTCUTS {
        if let Some(chord) = keymap_chord(overrides, entry) {
            bindings.extend(make_binding(entry.id, &chord));
        }
    }
    if cfg!(target_os = "macos") {
        bindings.push(KeyBinding::new(QUIT_CHORD, Quit, None));
    }
    cx.bind_keys(bindings);
}

pub fn register_key_bindings(cx: &mut App) {
    bind_all(cx, &HashMap::new());
}

pub fn reapply_key_bindings(cx: &mut App, overrides: &HashMap<String, String>) {
    cx.clear_key_bindings();
    bind_all(cx, overrides);
}

fn bind_all(cx: &mut App, overrides: &HashMap<String, String>) {
    forge_components::bind_text_input_keys(cx);
    forge_components::bind_confirm_keys(cx);
    forge_components::bind_text_area_keys(cx);
    forge_components::bind_code_editor_keys(cx);
    forge_components::bind_picker_keys(cx);
    bind_list_keys(cx);
    bind_shell(cx, overrides);
}

#[cfg(test)]
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

    struct ShellSaveHost {
        focus: gpui::FocusHandle,
        saves: usize,
    }

    impl gpui::Render for ShellSaveHost {
        fn render(
            &mut self,
            _: &mut gpui::Window,
            cx: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            use gpui::prelude::*;
            gpui::div()
                .key_context(SHELL_CONTEXT)
                .track_focus(&self.focus)
                .on_action(cx.listener(|this, _: &SaveFocused, _, _| this.saves += 1))
                .size_full()
        }
    }

    fn saves_after(cx: &mut gpui::TestAppContext, keystroke: &str) -> usize {
        let (host, vcx) = cx.add_window_view(|_, cx| ShellSaveHost {
            focus: cx.focus_handle(),
            saves: 0,
        });
        vcx.update(|window, cx| window.focus(&host.read(cx).focus.clone(), cx));
        vcx.run_until_parked();
        vcx.simulate_keystrokes(keystroke);
        vcx.update(|_, cx| host.read(cx).saves)
    }

    fn other_platforms_save_key() -> &'static str {
        if cfg!(target_os = "macos") {
            "ctrl-s"
        } else {
            "cmd-s"
        }
    }

    #[test]
    fn default_chords_follow_the_platform_and_recorded_chords_stay_literal() {
        let save = shortcut_entry("app.save").expect("the save shortcut exists");

        assert_eq!(
            keymap_chord(&HashMap::new(), save).as_deref(),
            Some("secondary-s")
        );
        for chord in ["ctrl-s", "ctrl-shift-s", "cmd-s", "alt-s"] {
            let recorded = HashMap::from([(save.id.to_owned(), chord.to_owned())]);
            assert_eq!(
                keymap_chord(&recorded, save).as_deref(),
                Some(chord),
                "{chord}"
            );
        }
    }

    #[test]
    fn a_cleared_override_binds_nothing() {
        let save = shortcut_entry("app.save").expect("the save shortcut exists");
        let cleared = HashMap::from([(save.id.to_owned(), String::new())]);

        assert_eq!(keymap_chord(&cleared, save), None);
    }

    #[test]
    fn a_recording_keeps_control_and_the_platform_key_apart() {
        for (typed, recorded) in [
            ("ctrl-s", "ctrl-s"),
            ("cmd-s", "cmd-s"),
            ("ctrl-cmd-s", "ctrl-cmd-s"),
        ] {
            let keystroke = Keystroke::parse(typed).expect("a parsable keystroke");
            assert_eq!(
                canonical_chord(&keystroke).as_deref(),
                Some(recorded),
                "{typed}"
            );
        }
    }

    #[test]
    fn every_ctrl_default_binds_through_the_secondary_modifier() {
        for entry in SHORTCUTS {
            let bound = keymap_chord(&HashMap::new(), entry).expect("a default binds");
            assert_eq!(
                bound,
                entry.default_chord.replacen("ctrl-", "secondary-", 1),
                "{}",
                entry.id
            );
            assert!(make_binding(entry.id, &bound).is_some(), "{}", entry.id);
        }
    }

    #[test]
    fn a_default_chord_reads_as_the_key_the_platform_really_binds() {
        let save = shortcut_entry("app.save").expect("the save shortcut exists");
        let modifier = if cfg!(target_os = "macos") {
            "Meta"
        } else {
            "Ctrl"
        };

        assert_eq!(
            chord_caps(&platform_default_chord(save)),
            format!("{modifier}+S")
        );
    }

    #[gpui::test]
    fn the_default_save_fires_on_the_platform_secondary_key_only(cx: &mut gpui::TestAppContext) {
        cx.update(register_key_bindings);

        let on_this_platform = saves_after(cx, "secondary-s");
        let on_the_other_platform = saves_after(cx, other_platforms_save_key());

        assert_eq!((on_this_platform, on_the_other_platform), (1, 0));
    }

    #[gpui::test]
    fn a_recorded_save_chord_fires_as_recorded_on_every_platform(cx: &mut gpui::TestAppContext) {
        let overrides = HashMap::from([("app.save".to_owned(), "ctrl-shift-s".to_owned())]);
        cx.update(|cx| reapply_key_bindings(cx, &overrides));

        let recorded = saves_after(cx, "ctrl-shift-s");
        let replaced_default = saves_after(cx, "secondary-s");

        assert_eq!((recorded, replaced_default), (1, 0));
    }
}
