use std::collections::HashMap;
use std::sync::Arc;

use forge_storage::SettingsRepo;
use forge_storage::settings::reserved_keys::KEYBOARD_SHORTCUTS;
use forge_widgets::{
    ForgePalette,
    icons::{Icon, tabler_icon},
    key_capture,
    sections::section_header,
    tokens::{
        BORDER_THIN, FONT_LG, FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius, sp, spf,
    },
};
use iced::{
    Background, Border, Color, Element, Length, Task,
    widget::{Space, button, column, container, row, scrollable, text},
};

use crate::Message;
use crate::message::SettingsMsg;
use crate::runtime_view::RuntimeView;
use crate::screen::{Screen, SettingsSection};

pub struct ShortcutEntry {
    pub id: &'static str,
    pub label_key: &'static str,
    pub default_chord: &'static str,
}

/// Every entry must map to behavior reachable from the sidebar today; chords
/// avoid the script-editor autocomplete keys and the soundboard F-key context.
pub const CATALOG: &[ShortcutEntry] = &[
    ShortcutEntry {
        id: "nav.home",
        label_key: "settings_shortcuts_action_nav_home",
        default_chord: "Ctrl+Shift+H",
    },
    ShortcutEntry {
        id: "nav.live_chat",
        label_key: "settings_shortcuts_action_nav_live_chat",
        default_chord: "Ctrl+Shift+C",
    },
    ShortcutEntry {
        id: "nav.event_feed",
        label_key: "settings_shortcuts_action_nav_event_feed",
        default_chord: "Ctrl+Shift+E",
    },
    ShortcutEntry {
        id: "nav.actions",
        label_key: "settings_shortcuts_action_nav_actions",
        default_chord: "Ctrl+Shift+A",
    },
    ShortcutEntry {
        id: "nav.globals",
        label_key: "settings_shortcuts_action_nav_globals",
        default_chord: "Ctrl+Shift+G",
    },
    ShortcutEntry {
        id: "nav.script_editor",
        label_key: "settings_shortcuts_action_nav_script_editor",
        default_chord: "Ctrl+Shift+R",
    },
    ShortcutEntry {
        id: "nav.settings",
        label_key: "settings_shortcuts_action_nav_settings",
        default_chord: "Ctrl+Shift+S",
    },
];

#[derive(Debug, Clone)]
pub struct ShortcutConflict {
    pub target_id: &'static str,
    pub owner_id: &'static str,
    pub chord: String,
}

#[derive(Default)]
pub struct ShortcutsState {
    /// Empty-string value marks an entry the user explicitly unbound.
    pub overrides: HashMap<String, String>,
    pub rebinding: Option<&'static str>,
    pub rebind_error: Option<String>,
    pub conflict: Option<ShortcutConflict>,
}

#[derive(Debug, Clone)]
pub enum ShortcutsMsg {
    ChordPressed(String),
    RebindStarted(&'static str),
    ChordCaptured(String),
    CaptureCancelled,
    ConflictSteal,
    ConflictCancel,
    ResetEntry(&'static str),
    ResetAll,
    Persisted(Result<(), String>),
}

/// Garbled documents yield an empty map; entries for unknown action ids are dropped.
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
        .filter(|(id, _)| CATALOG.iter().any(|entry| entry.id == id))
        .collect()
}

fn chord_is_bindable(chord: &str) -> bool {
    let Some(key) = chord.split('+').next_back() else {
        return false;
    };
    if key.is_empty() || matches!(key, "Ctrl" | "Shift" | "Alt" | "Meta") {
        return false;
    }
    let mods: Vec<&str> = chord.split('+').collect();
    let has_strong_modifier = mods[..mods.len() - 1]
        .iter()
        .any(|m| matches!(*m, "Ctrl" | "Alt" | "Meta"));
    let is_f_key = key
        .strip_prefix('F')
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=12).contains(&n));
    has_strong_modifier || is_f_key
}

pub fn effective_chord<'a>(state: &'a ShortcutsState, entry: &'a ShortcutEntry) -> Option<&'a str> {
    match state.overrides.get(entry.id) {
        Some(stored) if chord_is_bindable(stored) => Some(stored.as_str()),
        Some(_) => None,
        None => Some(entry.default_chord),
    }
}

fn resolve_action(state: &ShortcutsState, chord: &str) -> Option<&'static str> {
    CATALOG
        .iter()
        .find(|entry| effective_chord(state, entry) == Some(chord))
        .map(|entry| entry.id)
}

fn action_message(id: &str) -> Option<Message> {
    let screen = match id {
        "nav.home" => Screen::Home,
        "nav.live_chat" => Screen::LiveChat,
        "nav.event_feed" => Screen::EventFeed,
        "nav.actions" => Screen::Actions,
        "nav.globals" => Screen::Globals,
        "nav.script_editor" => Screen::ScriptEditor,
        "nav.settings" => Screen::Settings(SettingsSection::Appearance),
        _ => return None,
    };
    Some(Message::Navigate(screen))
}

pub fn shortcut_filter(event: iced::keyboard::Event) -> Option<Message> {
    let iced::keyboard::Event::KeyPressed { key, modifiers, .. } = event else {
        return None;
    };
    let chord = forge_widgets::chord_from_key(&key, modifiers)?;
    if !chord_is_bindable(&chord) {
        return None;
    }
    Some(Message::Settings(SettingsMsg::Shortcuts(
        ShortcutsMsg::ChordPressed(chord),
    )))
}

fn registered_global_combo(rt: &RuntimeView, chord: &str) -> bool {
    rt.hotkey_client
        .as_ref()
        .map(|client| {
            client
                .registered_combos()
                .iter()
                .any(|(_, combo)| combo.as_str() == chord)
        })
        .unwrap_or(false)
}

fn apply_chord(state: &mut ShortcutsState, id: &'static str, chord: String) {
    let default = CATALOG
        .iter()
        .find(|entry| entry.id == id)
        .map(|entry| entry.default_chord);
    if default == Some(chord.as_str()) {
        state.overrides.remove(id);
    } else {
        state.overrides.insert(id.to_owned(), chord);
    }
}

fn persist(state: &ShortcutsState, rt: &RuntimeView) -> Task<Message> {
    let map = state.overrides.clone();
    let settings: Arc<dyn SettingsRepo> = Arc::clone(&rt.backend) as Arc<dyn SettingsRepo>;
    Task::perform(
        async move {
            let json = serde_json::to_string(&map).map_err(|e| e.to_string())?;
            settings
                .set_string(KEYBOARD_SHORTCUTS, &json)
                .await
                .map_err(|e| e.to_string())
        },
        |r| Message::Settings(SettingsMsg::Shortcuts(ShortcutsMsg::Persisted(r))),
    )
}

pub fn update(state: &mut ShortcutsState, rt: &RuntimeView, msg: ShortcutsMsg) -> Task<Message> {
    match msg {
        ShortcutsMsg::ChordPressed(chord) => {
            if state.rebinding.is_some() || state.conflict.is_some() {
                return Task::none();
            }
            match resolve_action(state, &chord).and_then(action_message) {
                Some(message) => Task::done(message),
                None => Task::none(),
            }
        }
        ShortcutsMsg::RebindStarted(id) => {
            state.rebinding = Some(id);
            state.rebind_error = None;
            Task::none()
        }
        ShortcutsMsg::CaptureCancelled => {
            state.rebinding = None;
            Task::none()
        }
        ShortcutsMsg::ChordCaptured(chord) => {
            let Some(id) = state.rebinding.take() else {
                return Task::none();
            };
            if !chord_is_bindable(&chord) {
                state.rebind_error = Some(forge_widgets::tr!(
                    "settings_shortcuts_error_needs_modifier"
                ));
                return Task::none();
            }
            if registered_global_combo(rt, &chord) {
                state.rebind_error = Some(forge_widgets::tr!(
                    "settings_shortcuts_error_global_hotkey",
                    chord = chord.as_str()
                ));
                return Task::none();
            }
            if let Some(owner_id) = resolve_action(state, &chord).filter(|owner| *owner != id) {
                state.conflict = Some(ShortcutConflict {
                    target_id: id,
                    owner_id,
                    chord,
                });
                return Task::none();
            }
            state.rebind_error = None;
            apply_chord(state, id, chord);
            persist(state, rt)
        }
        ShortcutsMsg::ConflictSteal => {
            let Some(conflict) = state.conflict.take() else {
                return Task::none();
            };
            state
                .overrides
                .insert(conflict.owner_id.to_owned(), String::new());
            apply_chord(state, conflict.target_id, conflict.chord);
            state.rebind_error = None;
            persist(state, rt)
        }
        ShortcutsMsg::ConflictCancel => {
            state.conflict = None;
            Task::none()
        }
        ShortcutsMsg::ResetEntry(id) => {
            state.overrides.remove(id);
            state.rebind_error = None;
            persist(state, rt)
        }
        ShortcutsMsg::ResetAll => {
            state.overrides.clear();
            state.rebind_error = None;
            state.rebinding = None;
            persist(state, rt)
        }
        ShortcutsMsg::Persisted(result) => {
            if let Err(e) = result {
                tracing::warn!(error = %e, "failed to persist keyboard shortcuts");
            }
            Task::none()
        }
    }
}

pub fn view<'a>(state: &'a ShortcutsState, palette: &'a ForgePalette) -> Element<'a, Message> {
    let p = *palette;

    let header = row![
        tabler_icon(Icon::Keyboard, 18.0, p.brand),
        text(forge_widgets::tr!("settings_shortcuts_title"))
            .size(FONT_LG)
            .color(p.text_primary),
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(iced::Alignment::Center);

    let subtitle = text(forge_widgets::tr!("settings_shortcuts_subtitle"))
        .size(FONT_SM)
        .color(p.text_muted);

    let mut list = column![].spacing(spf(Spacing::Xxs));
    for entry in CATALOG {
        list = list.push(entry_row(state, entry, palette));
    }

    let fixed_section = column![
        section_header(
            forge_widgets::tr!("settings_shortcuts_fixed_section"),
            None,
            palette
        ),
        fixed_row(
            forge_widgets::tr!("settings_shortcuts_fixed_enter"),
            "Enter",
            palette
        ),
        fixed_row(
            forge_widgets::tr!("settings_shortcuts_fixed_escape"),
            "Esc",
            palette
        ),
        text(forge_widgets::tr!("settings_shortcuts_fixed_note"))
            .size(FONT_XS)
            .color(p.text_faint),
    ]
    .spacing(spf(Spacing::Xs));

    let reset_all = forge_widgets::ghost_button(
        forge_widgets::tr!("settings_shortcuts_reset_all"),
        Message::Settings(SettingsMsg::Shortcuts(ShortcutsMsg::ResetAll)),
        palette,
    );

    let body = column![
        header,
        subtitle,
        rebind_error_el(state, palette),
        list,
        Space::new().height(spf(Spacing::Sm)),
        fixed_section,
        Space::new().height(spf(Spacing::Sm)),
        row![reset_all],
    ]
    .spacing(spf(Spacing::Sm));

    let page: Element<'a, Message> = container(scrollable(body.padding(sp(Spacing::Lg))))
        .width(Length::Fill)
        .height(Length::Fill)
        .into();

    if let Some(conflict) = &state.conflict {
        iced::widget::stack![page, conflict_overlay(conflict, palette)].into()
    } else {
        page
    }
}

fn entry_label(entry: &ShortcutEntry) -> String {
    forge_widgets::tr!(entry.label_key)
}

fn chord_chip<'a>(label: &'a str, palette: &'a ForgePalette) -> Element<'a, Message> {
    let p = *palette;
    container(
        text(label)
            .size(FONT_XS)
            .color(p.text_primary)
            .font(font(FontRole::Monospace)),
    )
    .padding([3_u16, 8_u16])
    .style(move |_: &iced::Theme| container::Style {
        background: Some(Background::Color(p.surface_overlay)),
        border: Border {
            radius: radius(Radius::Sm).into(),
            ..Default::default()
        },
        ..container::Style::default()
    })
    .into()
}

fn entry_row<'a>(
    state: &'a ShortcutsState,
    entry: &'a ShortcutEntry,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let p = *palette;
    let label = text(entry_label(entry)).size(FONT_SM).color(p.text_primary);

    let mut controls = row![]
        .spacing(spf(Spacing::Xs))
        .align_y(iced::Alignment::Center);

    if state.rebinding == Some(entry.id) {
        let capture: Element<'a, Message> = key_capture(palette)
            .on_captured(|chord| {
                Message::Settings(SettingsMsg::Shortcuts(ShortcutsMsg::ChordCaptured(chord)))
            })
            .on_reset(|| Message::Settings(SettingsMsg::Shortcuts(ShortcutsMsg::CaptureCancelled)))
            .into();
        controls = controls.push(container(capture).width(Length::Fixed(220.0)));
    } else {
        match effective_chord(state, entry) {
            Some(chord) => controls = controls.push(chord_chip(chord, palette)),
            None => {
                controls = controls.push(
                    text(forge_widgets::tr!("settings_shortcuts_unbound"))
                        .size(FONT_XS)
                        .color(p.text_faint),
                );
            }
        }
        controls = controls.push(forge_widgets::ghost_button(
            forge_widgets::tr!("settings_shortcuts_rebind"),
            Message::Settings(SettingsMsg::Shortcuts(ShortcutsMsg::RebindStarted(
                entry.id,
            ))),
            palette,
        ));
        if state.overrides.contains_key(entry.id) {
            controls = controls.push(forge_widgets::ghost_button(
                forge_widgets::tr!("settings_shortcuts_reset"),
                Message::Settings(SettingsMsg::Shortcuts(ShortcutsMsg::ResetEntry(entry.id))),
                palette,
            ));
        }
    }

    container(
        row![label, Space::new().width(Length::Fill), controls].align_y(iced::Alignment::Center),
    )
    .padding([6_u16, 0_u16])
    .width(Length::Fill)
    .style(move |_: &iced::Theme| container::Style {
        border: Border {
            color: p.border_regular,
            width: 0.5,
            radius: 0.0.into(),
        },
        ..container::Style::default()
    })
    .into()
}

fn fixed_row<'a>(label: String, chord: &'a str, palette: &'a ForgePalette) -> Element<'a, Message> {
    let p = *palette;
    row![
        text(label).size(FONT_SM).color(p.text_secondary),
        Space::new().width(Length::Fill),
        chord_chip(chord, palette),
    ]
    .align_y(iced::Alignment::Center)
    .into()
}

fn rebind_error_el<'a>(
    state: &'a ShortcutsState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let Some(err) = &state.rebind_error else {
        return Space::new().height(0).into();
    };
    let p = *palette;
    container(
        row![
            tabler_icon(Icon::AlertTriangle, 13.0, p.random),
            text(err.as_str())
                .size(FONT_SM)
                .font(font(FontRole::Body))
                .color(p.random),
        ]
        .spacing(spf(Spacing::Xs))
        .align_y(iced::Alignment::Center),
    )
    .padding([6_u16, 10_u16])
    .style(move |_: &iced::Theme| container::Style {
        background: Some(Background::Color(Color { a: 0.1, ..p.random })),
        border: Border {
            color: p.random,
            width: BORDER_THIN,
            radius: radius(Radius::Sm).into(),
        },
        ..container::Style::default()
    })
    .into()
}

fn conflict_overlay<'a>(
    conflict: &'a ShortcutConflict,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let p = *palette;
    let backdrop = button(Space::new().width(Length::Fill).height(Length::Fill))
        .on_press(Message::Settings(SettingsMsg::Shortcuts(
            ShortcutsMsg::ConflictCancel,
        )))
        .style(|_, _| button::Style {
            background: Some(Background::Color(Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.5,
            })),
            border: Border::default(),
            text_color: Color::TRANSPARENT,
            ..button::Style::default()
        })
        .width(Length::Fill)
        .height(Length::Fill);

    let owner_label = CATALOG
        .iter()
        .find(|entry| entry.id == conflict.owner_id)
        .map(entry_label)
        .unwrap_or_default();

    let body_text = text(forge_widgets::tr!(
        "settings_shortcuts_conflict_body",
        chord = conflict.chord.as_str(),
        owner = owner_label
    ))
    .size(FONT_SM)
    .font(font(FontRole::Body))
    .color(p.text_secondary);

    let cancel_btn = button(
        text(forge_widgets::tr!("common_cancel"))
            .size(FONT_SM)
            .font(font(FontRole::Body))
            .color(p.text_secondary),
    )
    .on_press(Message::Settings(SettingsMsg::Shortcuts(
        ShortcutsMsg::ConflictCancel,
    )))
    .padding([8_u16, 14_u16])
    .style(move |_, _| button::Style {
        background: Some(Background::Color(p.surface_overlay)),
        border: Border {
            color: p.border_regular,
            width: BORDER_THIN,
            radius: radius(Radius::Sm).into(),
        },
        text_color: p.text_secondary,
        ..button::Style::default()
    });

    let steal_btn = button(
        text(forge_widgets::tr!("settings_shortcuts_conflict_steal"))
            .size(FONT_SM)
            .font(font(FontRole::Body))
            .color(p.shell),
    )
    .on_press(Message::Settings(SettingsMsg::Shortcuts(
        ShortcutsMsg::ConflictSteal,
    )))
    .padding([8_u16, 14_u16])
    .style(move |_, _| button::Style {
        background: Some(Background::Color(p.warning)),
        border: Border::default(),
        text_color: p.shell,
        ..button::Style::default()
    });

    let card = container(
        column![
            body_text,
            Space::new().height(spf(Spacing::Md)),
            row![cancel_btn, steal_btn]
                .spacing(spf(Spacing::Sm))
                .align_y(iced::Alignment::Center),
        ]
        .spacing(spf(Spacing::Xs)),
    )
    .padding([20_u16, 24_u16])
    .max_width(460.0)
    .style(move |_: &iced::Theme| container::Style {
        background: Some(Background::Color(p.elevated)),
        border: Border {
            color: p.border_regular,
            width: BORDER_THIN,
            radius: radius(Radius::Lg).into(),
        },
        ..container::Style::default()
    });

    let centered = container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::Alignment::Center)
        .align_y(iced::Alignment::Center);

    iced::widget::stack![backdrop, centered].into()
}
