//! Actions screen — editor detail pane: header, the sub-action step chain and
//! step controls, the edit-sub-action side sheet, the placeholder triggers
//! section, and the unified add-sub-action grid picker.

use super::*;
use crate::presentation::ActivePresentation;
use forge_components::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_LG, FONT_SM, FONT_XS,
    FONT_XXS, ForgePalette, GridPicker, GridPickerConfig, GridPickerEvent, GridPickerGroup,
    GridPickerItem, GridPickerItemState, GridPickerSubtitle, Icon, MenuPlacement, OverlayPosition,
    Radius, SheetPosition, Spacing, TextInput, ghost_button_with_icon, icon, icon_inherit,
    menu_button, menu_divider, menu_item, overlay, primary_button, radius, row_card,
    secondary_button, side_sheet, spacing, status_dot, toggle,
};
use forge_registry::{
    FormField, SubActionCategory, SubActionRegistry, SubActionRunner, TriggerKindDescriptor,
    TriggerRegistry,
};
use forge_types::{SubActionConfig, SubActionStep, TriggerInstance, TriggerInstanceId, Variant};
use gpui::{
    AnyElement, App, ClickEvent, Context, ElementId, Entity, FontWeight, Rgba, SharedString,
    Window, div, px,
};
use std::collections::HashMap;

/// The step-card leading glyph name, its title, and the mono summary line for a
/// step whose `kind_id` the registry does not resolve — the runner's own
/// label/icon take precedence when present.
fn sub_action_summary(step: &SubActionStep) -> (&'static str, String, String) {
    fn as_str(v: &Variant) -> &str {
        if let Variant::String(s) = v {
            s.as_str()
        } else {
            ""
        }
    }
    fn as_i64(v: &Variant) -> i64 {
        if let Variant::Int(n) = v { *n } else { 0 }
    }
    match step.kind_id.as_str() {
        "twitch.chat.send_message" => {
            let target = step.config.get("target").map(as_str).unwrap_or("twitch");
            let message = step.config.get("message").map(as_str).unwrap_or("");
            (
                "send",
                "Send chat message".to_owned(),
                format!("\u{2192} {target}: \"{message}\""),
            )
        }
        "core.globals.set" => {
            let name = step.config.get("name").map(as_str).unwrap_or("");
            let value = step.config.get("value").map(as_str).unwrap_or("");
            (
                "variable",
                "Set global".to_owned(),
                format!("{name} = \"{value}\""),
            )
        }
        "core.logic.wait" => {
            let ms = step.config.get("ms").map(as_i64).unwrap_or(0);
            ("clock", "Delay".to_owned(), format!("{ms} ms"))
        }
        "core.log.write" => {
            let level = step.config.get("level").map(as_str).unwrap_or("info");
            let message = step.config.get("message").map(as_str).unwrap_or("");
            (
                "info-circle",
                "Write log".to_owned(),
                format!("[{level}] \"{message}\""),
            )
        }
        "soundboard.sound.play" => {
            let clip_id = step.config.get("clip_id").map(as_str).unwrap_or("");
            ("music", "Play sound".to_owned(), clip_id.to_owned())
        }
        "tts.speak.text" => {
            let text = step.config.get("text").map(as_str).unwrap_or("");
            ("volume", "Speak (TTS)".to_owned(), text.to_owned())
        }
        "core.file.read" => {
            let path = step.config.get("path").map(as_str).unwrap_or("");
            let var = step.config.get("target_var").map(as_str).unwrap_or("");
            (
                "file",
                "Read file".to_owned(),
                format!("{path} \u{2192} %{var}%"),
            )
        }
        "core.random.int" => {
            let min = step.config.get("min").map(as_i64).unwrap_or(0);
            let max = step.config.get("max").map(as_i64).unwrap_or(0);
            let var = step.config.get("target_var").map(as_str).unwrap_or("");
            (
                "dice",
                "Random number".to_owned(),
                format!("[{min}..{max}] \u{2192} %{var}%"),
            )
        }
        _ => ("bolt", "Run sub-action".to_owned(), step.kind_id.clone()),
    }
}

fn sub_category_label(cat: SubActionCategory) -> &'static str {
    match cat {
        SubActionCategory::Chat => "Chat",
        SubActionCategory::Moderation => "Moderation",
        SubActionCategory::ChannelPoints => "Channel Points",
        SubActionCategory::PollsPredictions => "Polls & Predictions",
        SubActionCategory::Globals => "Globals",
        SubActionCategory::Logic => "Logic",
        SubActionCategory::Delay => "Delay",
        SubActionCategory::Scripts => "Scripts",
        SubActionCategory::Files => "Files",
        SubActionCategory::Twitch => "Twitch",
        SubActionCategory::YouTube => "YouTube",
        SubActionCategory::Kick => "Kick",
        SubActionCategory::Obs => "OBS",
        SubActionCategory::VTube => "VTube Studio",
        SubActionCategory::Discord => "Discord",
        SubActionCategory::Midi => "MIDI",
        SubActionCategory::Hotkey => "Hotkey",
        SubActionCategory::Audio => "Audio",
        SubActionCategory::Tts => "Text-to-speech",
        SubActionCategory::Http => "HTTP",
        SubActionCategory::Server => "Server",
        SubActionCategory::Util => "Utilities",
    }
}

fn sub_category_slug(cat: SubActionCategory) -> &'static str {
    match cat {
        SubActionCategory::Chat => "chat",
        SubActionCategory::Moderation => "moderation",
        SubActionCategory::ChannelPoints => "channel-points",
        SubActionCategory::PollsPredictions => "polls",
        SubActionCategory::Globals => "globals",
        SubActionCategory::Logic => "logic",
        SubActionCategory::Delay => "delay",
        SubActionCategory::Scripts => "scripts",
        SubActionCategory::Files => "files",
        SubActionCategory::Twitch => "twitch",
        SubActionCategory::YouTube => "youtube",
        SubActionCategory::Kick => "kick",
        SubActionCategory::Obs => "obs",
        SubActionCategory::VTube => "vtube",
        SubActionCategory::Discord => "discord",
        SubActionCategory::Midi => "midi",
        SubActionCategory::Hotkey => "hotkey",
        SubActionCategory::Audio => "audio",
        SubActionCategory::Tts => "tts",
        SubActionCategory::Http => "http",
        SubActionCategory::Server => "server",
        SubActionCategory::Util => "util",
    }
}

fn sub_category_color(cat: SubActionCategory, palette: &ForgePalette) -> Rgba {
    match cat {
        SubActionCategory::Chat | SubActionCategory::Twitch => palette.brand,
        SubActionCategory::Tts | SubActionCategory::Audio => palette.success,
        SubActionCategory::Globals => palette.warning,
        SubActionCategory::Files => palette.random,
        SubActionCategory::YouTube => palette.platform_youtube,
        SubActionCategory::Kick => palette.platform_kick,
        SubActionCategory::Obs => palette.text_secondary,
        SubActionCategory::VTube => palette.accent_teal,
        SubActionCategory::Discord | SubActionCategory::Http | SubActionCategory::Server => {
            palette.info
        }
        SubActionCategory::Midi | SubActionCategory::Moderation => palette.random,
        SubActionCategory::ChannelPoints => palette.accent_pink_light,
        SubActionCategory::Hotkey
        | SubActionCategory::PollsPredictions
        | SubActionCategory::Scripts => palette.warning,
        SubActionCategory::Logic | SubActionCategory::Delay | SubActionCategory::Util => {
            palette.text_muted
        }
    }
}

/// The registry's runners as grid groups (one per [`SubActionCategory`], runners
/// ordered by category slug then label) paired with the `kind_id` each card id
/// appends.
fn build_step_groups(
    registry: &SubActionRegistry,
    palette: &ForgePalette,
) -> (Vec<GridPickerGroup>, HashMap<SharedString, String>) {
    let mut runners: Vec<&dyn SubActionRunner> = registry.all().collect();
    runners.sort_by(|a, b| {
        sub_category_slug(a.category())
            .cmp(sub_category_slug(b.category()))
            .then_with(|| a.label().cmp(b.label()))
    });

    let mut groups: Vec<GridPickerGroup> = Vec::new();
    let mut picks: HashMap<SharedString, String> = HashMap::new();
    for runner in runners {
        let cat = runner.category();
        let scope = SharedString::from(sub_category_slug(cat));
        let color = sub_category_color(cat, palette);
        let id = SharedString::from(format!("step-{}", runner.id()));
        picks.insert(id.clone(), runner.id().to_owned());
        let item = GridPickerItem {
            id,
            icon: Icon::from_name(runner.icon_name()),
            icon_color: color,
            name: runner.label().to_string().into(),
            desc: runner.summary().to_string().into(),
            state: GridPickerItemState::Normal,
        };
        match groups.iter_mut().find(|g| g.scope == scope) {
            Some(g) => g.items.push(item),
            None => groups.push(GridPickerGroup {
                label: sub_category_label(cat).into(),
                dot_color: color,
                scope,
                items: vec![item],
            }),
        }
    }
    (groups, picks)
}

/// Builds one edit-form input row, seeding the field from the step's stored
/// config value.
#[allow(clippy::too_many_arguments)]
fn build_input_field(
    key: &str,
    label: &str,
    placeholder: &'static str,
    integer: bool,
    gate: Option<String>,
    config: &SubActionConfig,
    palette: ForgePalette,
    cx: &mut Context<ScreenActionsView>,
) -> SubFormField {
    let seed = config
        .get(key)
        .map(nav::variant_to_display_str)
        .unwrap_or_default();
    let input = cx.new(|cx| {
        let mut input = TextInput::new(placeholder, cx).with_palette(palette);
        if !seed.is_empty() {
            input.set_content(seed, cx);
        }
        input
    });
    SubFormField::Input {
        key: key.to_owned(),
        label: label.to_owned(),
        integer,
        gate,
        input,
    }
}

/// Folds one `FormField` (recursing through `Optional`) into the flat edit-form
/// field list. Select / DynamicSelect degrade to a free-text input — the kit
/// ships no value-picker primitive yet.
fn push_form_field(
    spec: &FormField,
    gate: Option<String>,
    config: &SubActionConfig,
    palette: ForgePalette,
    out: &mut Vec<SubFormField>,
    cx: &mut Context<ScreenActionsView>,
) {
    match spec {
        FormField::Text {
            key,
            label,
            placeholder,
        } => out.push(build_input_field(
            key,
            label,
            placeholder,
            false,
            gate,
            config,
            palette,
            cx,
        )),
        FormField::TextArea { key, label } => out.push(build_input_field(
            key, label, "", false, gate, config, palette, cx,
        )),
        FormField::Integer { key, label, .. } => out.push(build_input_field(
            key, label, "0", true, gate, config, palette, cx,
        )),
        FormField::Select { key, label, .. } | FormField::DynamicSelect { key, label, .. } => out
            .push(build_input_field(
                key, label, "", false, gate, config, palette, cx,
            )),
        FormField::Toggle { key, label } => {
            let value = matches!(config.get(*key), Some(Variant::Bool(true)));
            out.push(SubFormField::Bool {
                key: (*key).to_owned(),
                label: (*label).to_owned(),
                gate,
                value,
            });
        }
        FormField::SubChain { label, .. } | FormField::CaseList { label, .. } => {
            out.push(SubFormField::Hint {
                label: (*label).to_owned(),
            });
        }
        FormField::Optional { key, label, inner } => {
            let value = matches!(config.get(*key), Some(Variant::Bool(true)));
            out.push(SubFormField::Bool {
                key: (*key).to_owned(),
                label: (*label).to_owned(),
                gate: gate.clone(),
                value,
            });
            push_form_field(inner, Some((*key).to_owned()), config, palette, out, cx);
        }
    }
}

fn parse_variable_segments(s: &str) -> Vec<(&str, bool)> {
    let bytes = s.as_bytes();
    let mut segs: Vec<(&str, bool)> = Vec::new();
    let mut plain_start = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let var_start = i + 1;
            let mut j = var_start;
            if j < bytes.len() && (bytes[j].is_ascii_alphabetic() || bytes[j] == b'_') {
                j += 1;
                while j < bytes.len()
                    && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_' || bytes[j] == b'.')
                {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b'%' && j > var_start {
                    if plain_start < i {
                        segs.push((&s[plain_start..i], false));
                    }
                    segs.push((&s[i..j + 1], true));
                    i = j + 1;
                    plain_start = i;
                    continue;
                }
            }
        }
        i += 1;
    }
    if plain_start < s.len() {
        segs.push((&s[plain_start..], false));
    }
    segs
}

/// Renders a summary line with `%variable%` tokens tinted `warning` and plain
/// text tinted `text_muted`, wrapping like the source's flowed mono row.
fn variable_text(s: &str, palette: &ForgePalette) -> AnyElement {
    if s.is_empty() {
        return div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XS)
            .text_color(palette.text_muted)
            .child(String::new())
            .into_any_element();
    }
    let mut row = div()
        .flex()
        .flex_wrap()
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(FONT_XS);
    for (chunk, is_var) in parse_variable_segments(s) {
        let color = if is_var {
            palette.warning
        } else {
            palette.text_muted
        };
        row = row.child(div().text_color(color).child(chunk.to_owned()));
    }
    row.into_any_element()
}

/// Full-width, centered "Add …" button closing a section: the deep-panel fill,
/// an accent icon + label and a thin hairline, washing `surface_overlay` on
/// hover.
fn add_row_button(
    id: impl Into<ElementId>,
    glyph: Icon,
    label: &'static str,
    accent: Rgba,
    palette: &ForgePalette,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let hover = palette.surface_overlay;
    div()
        .id(id.into())
        .w_full()
        .flex()
        .items_center()
        .justify_center()
        .gap(spacing(Spacing::Xxs, Density::Cozy))
        .py(CARD_PAD_V)
        .px(CARD_PAD_H)
        .rounded(radius(Radius::Md))
        .border(BORDER_THIN)
        .border_color(palette.border_input)
        .bg(palette.shell)
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .on_click(handler)
        .child(icon(glyph, CARD_GLYPH, accent))
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XS)
                .text_color(accent)
                .child(label),
        )
        .into_any_element()
}

/// A centered, hairline-framed empty-state card for a section with no rows.
fn empty_placeholder_card(
    glyph: Icon,
    glyph_color: Rgba,
    label: &'static str,
    palette: &ForgePalette,
) -> AnyElement {
    div()
        .w_full()
        .flex()
        .flex_col()
        .items_center()
        .gap(spacing(Spacing::Xs, Density::Cozy))
        .py(EMPTY_CARD_PAD_V)
        .px(EMPTY_CARD_PAD_H)
        .rounded(radius(Radius::Md))
        .border(HALF_BORDER)
        .border_color(palette.border_input)
        .child(icon(glyph, EMPTY_CARD_GLYPH, glyph_color))
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_muted)
                .child(label),
        )
        .into_any_element()
}

/// The platform accent a trigger instance's card glyph inks, keyed off the
/// `kind_id` prefix (mirroring the trigger picker's platform grouping).
fn trigger_kind_color(kind_id: &str, palette: &ForgePalette) -> Rgba {
    if kind_id.starts_with("twitch.") {
        palette.brand
    } else if kind_id.starts_with("youtube.") {
        palette.platform_youtube
    } else if kind_id.starts_with("kick.") {
        palette.platform_kick
    } else if kind_id.starts_with("obs.") {
        palette.text_secondary
    } else if kind_id.starts_with("vtube.") {
        palette.accent_teal
    } else if kind_id.starts_with("midi.") {
        palette.random
    } else if kind_id.starts_with("hotkey.") || kind_id.starts_with("script.") {
        palette.warning
    } else {
        palette.info
    }
}

/// The linkable user-defined instances as one "saved triggers" grid band, paired
/// with the [`TriggerInstanceId`] each card id links. A disabled instance renders in
/// the [`GridPickerItemState::Disabled`] look.
fn build_trigger_groups(
    instances: &[TriggerInstance],
    registry: &TriggerRegistry,
    palette: &ForgePalette,
) -> (
    Vec<GridPickerGroup>,
    HashMap<SharedString, TriggerInstanceId>,
) {
    let mut items: Vec<GridPickerItem> = Vec::with_capacity(instances.len());
    let mut picks: HashMap<SharedString, TriggerInstanceId> = HashMap::new();
    for instance in instances {
        let descriptor = registry.get(&instance.kind_id);
        let color = trigger_kind_color(&instance.kind_id, palette);
        let id = SharedString::from(format!("trigger-{}", instance.id));
        picks.insert(id.clone(), instance.id);
        let glyph = Icon::from_name(
            descriptor
                .map(TriggerKindDescriptor::icon_name)
                .unwrap_or("bolt"),
        );
        let condition = descriptor
            .map(|d| d.condition_display(&instance.overrides))
            .unwrap_or_default();
        let desc = if condition.is_empty() {
            descriptor
                .map(|d| d.label().to_owned())
                .unwrap_or_else(|| instance.kind_id.clone())
        } else {
            condition
        };
        let state = if instance.enabled {
            GridPickerItemState::Normal
        } else {
            GridPickerItemState::Disabled
        };
        items.push(GridPickerItem {
            id,
            icon: glyph,
            icon_color: color,
            name: instance.name.clone().into(),
            desc: desc.into(),
            state,
        });
    }
    let groups = vec![GridPickerGroup {
        label: "Your saved triggers".into(),
        dot_color: palette.warning,
        scope: SharedString::from("all"),
        items,
    }];
    (groups, picks)
}

/// Borderless trailing unlink affordance on a linked trigger card: a faint `X` that,
/// on hover, washes its frame solid `random` and inverts its glyph to `shell`.
fn trigger_unlink_btn(
    id: impl Into<ElementId>,
    palette: &ForgePalette,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let solid = palette.random;
    let on_solid = palette.shell;
    div()
        .id(id.into())
        .flex()
        .items_center()
        .justify_center()
        .p(spacing(Spacing::Xxs, Density::Cozy))
        .rounded(radius(Radius::Sm))
        .text_color(palette.text_faint)
        .cursor_pointer()
        .hover(move |s| s.bg(solid).text_color(on_solid))
        .on_click(handler)
        .child(icon_inherit(Icon::X, UNLINK_GLYPH))
        .into_any_element()
}

impl ScreenActionsView {
    // --- editor: current chain --------------------------------------------

    /// The chain the step list currently renders — the action's top-level steps
    /// at root, or the nested sub-chain [`Self::nav_path`] descends into.
    pub(super) fn current_chain(&self) -> Vec<SubActionStep> {
        match &self.detail {
            Some(detail) => nav::resolve_chain(&detail.action.sub_actions, &self.nav_path),
            None => Vec::new(),
        }
    }

    // --- editor: step interaction handlers --------------------------------

    fn move_step_up(&mut self, i: usize, cx: &mut Context<Self>) {
        self.step_menu_open = None;
        self.persist_chain_mutation(
            move |chain| {
                if i > 0 && i < chain.len() {
                    let step = chain.remove(i);
                    chain.insert(i - 1, step);
                }
            },
            cx,
        );
        cx.notify();
    }

    fn move_step_down(&mut self, i: usize, cx: &mut Context<Self>) {
        self.step_menu_open = None;
        self.persist_chain_mutation(
            move |chain| {
                if i + 1 < chain.len() {
                    let step = chain.remove(i);
                    chain.insert(i + 1, step);
                }
            },
            cx,
        );
        cx.notify();
    }

    fn move_step_top(&mut self, i: usize, cx: &mut Context<Self>) {
        self.step_menu_open = None;
        self.persist_chain_mutation(
            move |chain| {
                if i != 0 && i < chain.len() {
                    let step = chain.remove(i);
                    chain.insert(0, step);
                }
            },
            cx,
        );
        cx.notify();
    }

    fn move_step_bottom(&mut self, i: usize, cx: &mut Context<Self>) {
        self.step_menu_open = None;
        self.persist_chain_mutation(
            move |chain| {
                let len = chain.len();
                if len > 0 && i < len - 1 {
                    let step = chain.remove(i);
                    chain.insert(len - 1, step);
                }
            },
            cx,
        );
        cx.notify();
    }

    fn duplicate_step(&mut self, i: usize, cx: &mut Context<Self>) {
        self.step_menu_open = None;
        self.persist_chain_mutation(
            move |chain| {
                if i < chain.len() {
                    let clone = chain[i].clone();
                    chain.insert(i + 1, clone);
                }
            },
            cx,
        );
        cx.notify();
    }

    fn remove_step(&mut self, i: usize, cx: &mut Context<Self>) {
        self.step_menu_open = None;
        self.persist_chain_mutation(
            move |chain| {
                if i < chain.len() {
                    chain.remove(i);
                }
            },
            cx,
        );
        cx.notify();
    }

    fn toggle_step_menu(&mut self, i: usize, cx: &mut Context<Self>) {
        self.step_menu_open = if self.step_menu_open == Some(i) {
            None
        } else {
            Some(i)
        };
        cx.notify();
    }

    fn close_step_menu(&mut self, cx: &mut Context<Self>) {
        self.step_menu_open = None;
        cx.notify();
    }

    /// Fires a synthetic event for the selected action's first linked trigger
    /// through the runtime bus (store-then-replay, a single evaluation pass), then
    /// toasts whether it matched. The injection runs on the tokio runtime; the
    /// outcome hops back to the foreground executor as a toast.
    fn test_run(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.selected else {
            return;
        };
        let service = Arc::clone(&self.actions_service);
        let registry = Arc::clone(&self.trigger_registry);
        let bus = Arc::clone(&self.bus);
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let _ =
                tx.send(super::test_trigger::run_test_trigger(&service, &registry, &bus, id).await);
        });
        cx.spawn(async move |this, cx| match rx.await {
            Ok(Ok(true)) => {
                let _ = this.update(cx, |_, cx| {
                    cx.push_toast(ToastKind::Success, "Test trigger fired");
                });
            }
            Ok(Ok(false)) => {
                let _ = this.update(cx, |_, cx| {
                    cx.push_toast(ToastKind::Warn, "Test event did not match this trigger");
                });
            }
            Ok(Err(message)) => {
                let _ = this.update(cx, |_, cx| {
                    cx.push_toast(ToastKind::Error, format!("Test trigger failed: {message}"));
                });
            }
            Err(_) => {}
        })
        .detach();
        cx.notify();
    }

    // --- editor: edit-sub-action side sheet -------------------------------

    fn open_edit_sub_action(&mut self, i: usize, cx: &mut Context<Self>) {
        let chain = self.current_chain();
        let Some(step) = chain.get(i) else {
            return;
        };
        let kind_id = step.kind_id.clone();
        let Some(specs) = self
            .sub_action_registry
            .get(&kind_id)
            .map(|r| r.config_fields())
        else {
            return;
        };
        let palette = cx.palette();
        let config = step.config.clone();
        let mut fields: Vec<SubFormField> = Vec::new();
        for spec in &specs {
            push_form_field(spec, None, &config, palette, &mut fields, cx);
        }
        self.step_menu_open = None;
        self.sub_form = Some(EditSubActionForm {
            kind_id,
            index: i,
            fields,
        });
        cx.notify();
    }

    fn toggle_sub_field(&mut self, key: String, cx: &mut Context<Self>) {
        if let Some(form) = self.sub_form.as_mut() {
            for field in &mut form.fields {
                if let SubFormField::Bool { key: k, value, .. } = field
                    && *k == key
                {
                    *value = !*value;
                }
            }
        }
        cx.notify();
    }

    fn cancel_sub_action(&mut self, cx: &mut Context<Self>) {
        self.sub_form = None;
        cx.notify();
    }

    /// Overlays the form's scalar fields onto the edited step's existing config,
    /// leaving the step's kind, enabled flag, label and nested sub-chain keys
    /// intact, then persists.
    fn submit_sub_action(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self.sub_form.as_ref() else {
            return;
        };
        let index = form.index;
        let bool_vals: HashMap<String, bool> = form
            .fields
            .iter()
            .filter_map(|f| match f {
                SubFormField::Bool { key, value, .. } => Some((key.clone(), *value)),
                _ => None,
            })
            .collect();
        let gate_on = |gate: &Option<String>| {
            gate.as_ref()
                .map(|g| bool_vals.get(g).copied().unwrap_or(false))
                .unwrap_or(true)
        };
        let mut overrides: Vec<(String, Variant)> = Vec::new();
        for field in &form.fields {
            match field {
                SubFormField::Bool {
                    key, value, gate, ..
                } => {
                    if gate_on(gate) {
                        overrides.push((key.clone(), Variant::Bool(*value)));
                    }
                }
                SubFormField::Input {
                    key,
                    integer,
                    gate,
                    input,
                    ..
                } => {
                    if !gate_on(gate) {
                        continue;
                    }
                    let text = input.read(cx).content().to_owned();
                    if *integer {
                        if let Ok(n) = text.trim().parse::<i64>() {
                            overrides.push((key.clone(), Variant::Int(n)));
                        }
                    } else {
                        overrides.push((key.clone(), Variant::String(text)));
                    }
                }
                SubFormField::Hint { .. } => {}
            }
        }

        self.sub_form = None;
        self.step_menu_open = None;
        self.persist_chain_mutation(
            move |chain| {
                if let Some(step) = chain.get_mut(index) {
                    for (key, value) in overrides {
                        step.config.insert(key, value);
                    }
                }
            },
            cx,
        );
        cx.notify();
    }

    // --- editor: unified "Add sub-action" grid picker ---------------------

    fn open_grid_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(action_id) = self.selected else {
            return;
        };
        if self.detail.is_none() {
            return;
        }
        let palette = cx.palette();
        let ctx_name = self
            .detail
            .as_ref()
            .map(|d| d.action.name.clone())
            .unwrap_or_else(|| "this action".to_owned());
        let (groups, picks) = build_step_groups(&self.sub_action_registry, &palette);
        let count = self.sub_action_registry.all().count();
        let config = GridPickerConfig {
            accent: palette.brand,
            header_icon: Icon::LayoutGrid,
            title: "Add sub-action".into(),
            subtitle: GridPickerSubtitle::Context {
                lead: "Inserting into".into(),
                name: ctx_name.into(),
                note: format!("\u{b7} {count} sub-actions").into(),
            },
            footer_hint: "Added with smart defaults \u{2014} edit inline after".into(),
            search_placeholder: format!("Search {count} sub-actions\u{2026}").into(),
            scope_cap: Some(7),
        };
        let picker = cx.new(|cx| GridPicker::new(config, groups, palette, cx));
        let sub = cx.subscribe(&picker, Self::on_grid_picker_event);
        picker.read(cx).focus(window, cx);
        self.step_menu_open = None;
        self.grid_picker = Some(GridPickerForm {
            picker,
            picks,
            action_id,
            _sub: sub,
        });
        cx.notify();
    }

    fn on_grid_picker_event(
        &mut self,
        _picker: Entity<GridPicker>,
        event: &GridPickerEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            GridPickerEvent::Picked(id) => {
                if let Some(kind_id) = self
                    .grid_picker
                    .as_ref()
                    .and_then(|f| f.picks.get(id).cloned())
                {
                    self.grid_pick_step(kind_id, cx);
                }
            }
            GridPickerEvent::Dismissed => self.cancel_grid_picker(cx),
        }
    }

    pub(super) fn cancel_grid_picker(&mut self, cx: &mut Context<Self>) {
        self.grid_picker = None;
        cx.notify();
    }

    fn grid_pick_step(&mut self, kind_id: String, cx: &mut Context<Self>) {
        let same = self
            .grid_picker
            .as_ref()
            .zip(self.detail.as_ref())
            .is_some_and(|(f, d)| f.action_id == d.action.id);
        let config = self
            .sub_action_registry
            .get(&kind_id)
            .map(|r| r.default_config());
        self.grid_picker = None;
        cx.notify();
        let Some(config) = config else {
            return;
        };
        if !same {
            return;
        }
        self.persist_chain_mutation(
            move |chain| {
                chain.push(SubActionStep {
                    kind_id,
                    config,
                    enabled: true,
                    label: None,
                });
            },
            cx,
        );
    }

    // --- editor: link / unlink triggers -----------------------------------

    /// Opens the unified centred grid picker over the user-defined trigger instances
    /// not yet linked to the selected action. The linkable set is pulled off the
    /// runtime service first; the picker opens (and focuses) once it lands. An empty
    /// set toasts rather than opening an empty grid.
    fn open_trigger_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(action_id) = self.selected else {
            return;
        };
        if self.detail.is_none() {
            return;
        }
        self.step_menu_open = None;
        let service = Arc::clone(&self.actions_service);
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let _ = tx.send(
                service
                    .list_linkable_triggers(action_id)
                    .await
                    .map_err(|e| e.to_string()),
            );
        });
        cx.spawn_in(window, async move |this, cx| match rx.await {
            Ok(Ok(instances)) => {
                let _ = this.update_in(cx, |this, window, cx| {
                    this.apply_trigger_picker(action_id, instances, window, cx)
                });
            }
            Ok(Err(message)) => {
                let _ = this.update(cx, |this, cx| this.on_repo_error(&message, cx));
            }
            Err(_) => {}
        })
        .detach();
    }

    /// Builds and opens the trigger grid picker from the pulled linkable set, guarding
    /// against the selection having moved on while the pull was in flight.
    fn apply_trigger_picker(
        &mut self,
        action_id: ActionId,
        instances: Vec<TriggerInstance>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected != Some(action_id) {
            return;
        }
        if instances.is_empty() {
            cx.push_toast(
                ToastKind::Info,
                "No unlinked triggers available \u{2014} create one on the Triggers screen",
            );
            cx.notify();
            return;
        }
        let palette = cx.palette();
        let action_name = self
            .detail
            .as_ref()
            .map(|d| d.action.name.clone())
            .unwrap_or_else(|| "this action".to_owned());
        let count = instances.len();
        let (groups, picks) = build_trigger_groups(&instances, &self.trigger_registry, &palette);
        let config = GridPickerConfig {
            accent: palette.warning,
            header_icon: Icon::Bolt,
            title: "Add trigger".into(),
            subtitle: GridPickerSubtitle::Context {
                lead: "Fires".into(),
                name: action_name.into(),
                note: format!("\u{b7} {count} available").into(),
            },
            footer_hint: "Links a saved trigger \u{2014} create new ones on the Triggers screen"
                .into(),
            search_placeholder: "Search triggers\u{2026}".into(),
            scope_cap: Some(6),
        };
        let picker = cx.new(|cx| GridPicker::new(config, groups, palette, cx));
        let sub = cx.subscribe(&picker, Self::on_trigger_picker_event);
        picker.read(cx).focus(window, cx);
        self.add_trigger = Some(AddTriggerForm {
            picker,
            picks,
            action_id,
            _sub: sub,
        });
        cx.notify();
    }

    fn on_trigger_picker_event(
        &mut self,
        _picker: Entity<GridPicker>,
        event: &GridPickerEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            GridPickerEvent::Picked(id) => {
                if let Some((action_id, instance_id)) = self
                    .add_trigger
                    .as_ref()
                    .and_then(|f| f.picks.get(id).copied().map(|inst| (f.action_id, inst)))
                {
                    self.link_trigger(action_id, instance_id, cx);
                }
            }
            GridPickerEvent::Dismissed => self.cancel_trigger_picker(cx),
        }
    }

    pub(super) fn cancel_trigger_picker(&mut self, cx: &mut Context<Self>) {
        self.add_trigger = None;
        cx.notify();
    }

    /// Links `instance_id` to the selected action through the runtime service, then
    /// re-pulls the editor detail in full so the triggers section mirrors the persisted
    /// links rather than a locally-patched list.
    fn link_trigger(
        &mut self,
        action_id: ActionId,
        instance_id: TriggerInstanceId,
        cx: &mut Context<Self>,
    ) {
        if self.selected != Some(action_id) {
            self.add_trigger = None;
            cx.notify();
            return;
        }
        self.add_trigger = None;
        let service = Arc::clone(&self.actions_service);
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let _ = tx.send(
                service
                    .link_trigger_instance(action_id, instance_id)
                    .await
                    .map_err(|e| e.to_string()),
            );
        });
        cx.spawn(async move |this, cx| match rx.await {
            Ok(Ok(())) => {
                let _ = this.update(cx, |this, cx| this.reload_detail(cx));
            }
            Ok(Err(message)) => {
                let _ = this.update(cx, |this, cx| this.on_repo_error(&message, cx));
            }
            Err(_) => {}
        })
        .detach();
        cx.notify();
    }

    /// Unlinks `instance_id` from the selected action through the runtime service, then
    /// re-pulls the editor detail in full.
    fn unlink_trigger(&mut self, instance_id: TriggerInstanceId, cx: &mut Context<Self>) {
        let Some(action_id) = self.selected else {
            return;
        };
        let service = Arc::clone(&self.actions_service);
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let _ = tx.send(
                service
                    .unlink_trigger_instance(action_id, instance_id)
                    .await
                    .map_err(|e| e.to_string()),
            );
        });
        cx.spawn(async move |this, cx| match rx.await {
            Ok(Ok(())) => {
                let _ = this.update(cx, |this, cx| this.reload_detail(cx));
            }
            Ok(Err(message)) => {
                let _ = this.update(cx, |this, cx| this.on_repo_error(&message, cx));
            }
            Err(_) => {}
        })
        .detach();
        cx.notify();
    }

    // --- render: right editor pane ----------------------------------------

    pub(super) fn render_editor_pane(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match (self.selected, self.detail.as_ref()) {
            (Some(sel), Some(detail)) if detail.action.id == sel => {
                self.render_editor(detail, palette, cx)
            }
            (Some(_), _) => self.render_loading(palette),
            (None, _) => self.render_empty(palette),
        }
    }

    fn render_empty(&self, palette: &ForgePalette) -> AnyElement {
        let placeholder = div()
            .flex()
            .flex_col()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(icon(Icon::Bolt, EMPTY_GLYPH, palette.text_faint))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_secondary)
                    .child("No action selected"),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child("Select an action from the list to view its details."),
            );

        div()
            .flex_1()
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .overflow_hidden()
            .child(placeholder)
            .into_any_element()
    }

    fn render_loading(&self, palette: &ForgePalette) -> AnyElement {
        div()
            .flex_1()
            .h_full()
            .py(spacing(Spacing::Md, Density::Cozy))
            .px(spacing(Spacing::Lg, Density::Cozy))
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(FONT_SM)
            .text_color(palette.text_muted)
            .child("Loading action…")
            .into_any_element()
    }

    fn render_editor(
        &self,
        detail: &ActionDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, Density::Cozy))
            .child(self.render_editor_header(detail, palette, cx))
            .child(self.render_triggers_section(detail, palette, cx))
            .child(self.render_sub_actions_section(detail, palette, cx));

        div()
            .id("actions-editor-scroll")
            .flex_1()
            .h_full()
            .overflow_y_scroll()
            .py(PANE_PAD_V)
            .px(PANE_PAD_H)
            .child(body)
            .into_any_element()
    }

    fn render_editor_header(
        &self,
        detail: &ActionDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let action = &detail.action;
        let (pill_color, pill_label) = if action.enabled {
            (palette.success, "Enabled")
        } else {
            (palette.text_faint, "Disabled")
        };
        let pill = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .py(px(1.0))
            .px(px(6.0))
            .rounded(PILL_RADIUS)
            .bg(palette.surface_overlay)
            .child(status_dot(pill_color, PILL_DOT))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(pill_color)
                    .child(pill_label),
            );

        let title_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(FONT_LG)
                    .text_color(palette.text_primary)
                    .child(action.name.clone()),
            )
            .child(pill);

        let desc = action
            .description
            .clone()
            .unwrap_or_else(|| "No description".to_owned());
        let desc_line = div()
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(FONT_XS)
            .text_color(palette.text_muted)
            .child(desc);

        let header_left = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(title_row)
            .child(desc_line);

        let btn_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(
                ghost_button_with_icon(Icon::PlayerPlay, "Test run", palette).on_click(
                    "actions-editor-test",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.test_run(cx)),
                ),
            )
            .child(
                ghost_button_with_icon(Icon::Copy, "Duplicate", palette).on_click(
                    "actions-editor-dup",
                    cx.listener(|this, _: &ClickEvent, _, cx| {
                        if let Some(id) = this.selected {
                            this.duplicate(id, cx);
                        }
                    }),
                ),
            );

        div()
            .flex()
            .items_start()
            .justify_between()
            .child(header_left)
            .child(btn_row)
            .into_any_element()
    }

    /// Triggers section: the mono count header over one card per linked trigger
    /// instance (each with a trailing unlink affordance), an empty-state card when
    /// none are linked, and a closing "Add trigger" button opening the grid picker.
    fn render_triggers_section(
        &self,
        detail: &ActionDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XXS)
            .text_color(palette.text_muted)
            .child(format!(
                "TRIGGERS \u{b7} {}",
                detail.trigger_instances.len()
            ));

        let mut col = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, Density::Cozy));
        if detail.trigger_instances.is_empty() {
            col = col.child(empty_placeholder_card(
                Icon::Bolt,
                palette.warning,
                "No triggers linked",
                palette,
            ));
        } else {
            for instance in &detail.trigger_instances {
                col = col.child(self.render_trigger_card(instance, palette, cx));
            }
        }
        col = col.child(add_row_button(
            "actions-add-trigger",
            Icon::Plus,
            "Add trigger",
            palette.warning,
            palette,
            cx.listener(|this, _: &ClickEvent, window, cx| this.open_trigger_picker(window, cx)),
        ));

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(label)
            .child(col)
            .into_any_element()
    }

    fn render_trigger_card(
        &self,
        instance: &TriggerInstance,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let descriptor = self.trigger_registry.get(&instance.kind_id);
        let accent = if instance.enabled {
            palette.brand
        } else {
            palette.disabled
        };
        let name_color = if instance.enabled {
            palette.text_primary
        } else {
            palette.text_faint
        };
        let kind_label = descriptor
            .map(|d| d.label().to_owned())
            .unwrap_or_else(|| instance.kind_id.clone());
        let condition = descriptor
            .map(|d| d.condition_display(&instance.overrides))
            .unwrap_or_default();
        let glyph = Icon::from_name(
            descriptor
                .map(TriggerKindDescriptor::icon_name)
                .unwrap_or("bolt"),
        );

        let leading = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(status_dot(accent, TRIGGER_DOT))
            .child(icon(glyph, TRIGGER_GLYPH, accent));

        let title = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(FONT_XS)
                    .text_color(name_color)
                    .child(instance.name.clone()),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(kind_label),
            )
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.bits)
                    .child(condition),
            );

        let instance_id = instance.id;
        let unlink = trigger_unlink_btn(
            SharedString::from(format!("actions-trigger-unlink-{instance_id}")),
            palette,
            cx.listener(move |this, _: &ClickEvent, _, cx| this.unlink_trigger(instance_id, cx)),
        );

        row_card(title, palette)
            .leading(leading)
            .trailing(unlink)
            .idle_background(palette.elevated)
            .bordered(palette.border_regular, BORDER_THIN, radius(Radius::Md))
            .into_any_element()
    }

    fn render_sub_actions_section(
        &self,
        detail: &ActionDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let current = nav::resolve_chain(&detail.action.sub_actions, &self.nav_path);
        let total = current.len();
        let at_root = self.nav_path.is_empty();
        let depth = self.nav_path.len();

        let header = if at_root {
            div().flex().items_center().child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(format!("{total} sub-actions")),
            )
        } else {
            div()
                .flex()
                .items_center()
                .child(self.render_breadcrumb(detail, palette, cx))
                .child(div().flex_1())
                .child(
                    div()
                        .font_family(DEFAULT_MONO_FAMILY)
                        .text_size(FONT_XXS)
                        .text_color(palette.text_faint)
                        .child(total.to_string()),
                )
        };

        let mut steps_col = div().flex().flex_col();
        if current.is_empty() {
            let empty_label = if at_root {
                "This action has no steps yet"
            } else {
                "No steps yet · click Add step to start"
            };
            steps_col = steps_col.child(empty_placeholder_card(
                Icon::Plus,
                palette.brand,
                empty_label,
                palette,
            ));
        }
        for (i, step) in current.iter().enumerate() {
            steps_col = steps_col.child(self.render_step_block(step, i, total, depth, palette, cx));
        }
        steps_col = steps_col.child(
            div()
                .pl(STEP_COL_W + spacing(Spacing::Xs, Density::Cozy))
                .pt(spacing(Spacing::Xs, Density::Cozy))
                .child(add_row_button(
                    "actions-add-step",
                    Icon::Plus,
                    "Add step",
                    palette.brand,
                    palette,
                    cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.open_grid_picker(window, cx)
                    }),
                )),
        );

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(header)
            .child(steps_col)
            .into_any_element()
    }

    fn render_step_block(
        &self,
        step: &SubActionStep,
        i: usize,
        total: usize,
        depth: usize,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_last = i + 1 == total;
        let (fallback_icon, fallback_title, detail_str) = sub_action_summary(step);
        let runner = self.sub_action_registry.get(&step.kind_id);
        let title = runner
            .map(|r| r.label().to_owned())
            .unwrap_or(fallback_title);
        let glyph = Icon::from_name(runner.map(|r| r.icon_name()).unwrap_or(fallback_icon));

        let circle = div()
            .flex()
            .items_center()
            .justify_center()
            .size(STEP_CIRCLE)
            .rounded(STEP_CIRCLE_RADIUS)
            .bg(palette.brand)
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.shell)
                    .child((i + 1).to_string()),
            );
        let connector = div()
            .w(STEP_CONNECTOR_W)
            .h(if is_last { px(0.0) } else { STEP_CONNECTOR_H })
            .bg(palette.border_regular);
        let left_col = div()
            .flex()
            .flex_col()
            .items_center()
            .w(STEP_COL_W)
            .child(circle)
            .child(connector);

        let title_el = div()
            .font_family(DEFAULT_BODY_FAMILY)
            .font_weight(FontWeight::SEMIBOLD)
            .text_size(FONT_XS)
            .text_color(palette.text_primary)
            .child(title);

        let card = row_card(title_el, palette)
            .leading(icon(glyph, CARD_GLYPH, palette.text_secondary))
            .meta(variable_text(&detail_str, palette))
            .trailing(self.render_step_controls(i, total, palette, cx))
            .idle_background(palette.elevated)
            .bordered(palette.border_regular, BORDER_THIN, radius(Radius::Md));

        let step_row = div()
            .flex()
            .items_start()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(left_col)
            .child(div().flex_1().min_w(px(0.0)).child(card));

        let block: AnyElement = match self.render_branch_affordances(step, i, depth, palette, cx) {
            Some(branches) => {
                let indented = div()
                    .pl(STEP_COL_W + spacing(Spacing::Xs, Density::Cozy))
                    .pt(spacing(Spacing::Xxs, Density::Cozy))
                    .child(branches);
                div()
                    .flex()
                    .flex_col()
                    .child(step_row)
                    .child(indented)
                    .into_any_element()
            }
            None => step_row.into_any_element(),
        };

        div()
            .w_full()
            .pb(if is_last { px(0.0) } else { STEP_GAP })
            .child(block)
            .into_any_element()
    }

    fn render_step_controls(
        &self,
        i: usize,
        total: usize,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let menu_open = self.step_menu_open == Some(i);
        let view = cx.entity();

        let move_up = step_icon_btn(
            SharedString::from(format!("actions-step-up-{i}")),
            Icon::ArrowUp,
            i == 0,
            palette,
            cx.listener(move |this, _: &ClickEvent, _, cx| this.move_step_up(i, cx)),
        );
        let move_down = step_icon_btn(
            SharedString::from(format!("actions-step-down-{i}")),
            Icon::ArrowDown,
            i + 1 >= total,
            palette,
            cx.listener(move |this, _: &ClickEvent, _, cx| this.move_step_down(i, cx)),
        );

        let menu = menu_button(Icon::DotsVertical, menu_open, palette)
            .placement(MenuPlacement::BottomRight)
            .items(vec![
                menu_item(
                    SharedString::from(format!("actions-step-edit-{i}")),
                    "Edit…",
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.open_edit_sub_action(i, cx)
                    }),
                )
                .icon(Icon::InfoCircle)
                .into(),
                menu_item(
                    SharedString::from(format!("actions-step-dup-{i}")),
                    "Duplicate",
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.duplicate_step(i, cx)),
                )
                .icon(Icon::Copy)
                .into(),
                menu_divider(),
                menu_item(
                    SharedString::from(format!("actions-step-top-{i}")),
                    "Move to top",
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.move_step_top(i, cx)),
                )
                .icon(Icon::ArrowBarUp)
                .disabled(i == 0)
                .into(),
                menu_item(
                    SharedString::from(format!("actions-step-bottom-{i}")),
                    "Move to bottom",
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.move_step_bottom(i, cx)),
                )
                .icon(Icon::ArrowBarDown)
                .disabled(i + 1 >= total)
                .into(),
                menu_divider(),
                menu_item(
                    SharedString::from(format!("actions-step-del-{i}")),
                    "Delete",
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.remove_step(i, cx)),
                )
                .icon(Icon::Eraser)
                .color(palette.random)
                .into(),
            ])
            .on_toggle(
                SharedString::from(format!("actions-step-menu-{i}")),
                cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_step_menu(i, cx)),
            )
            .on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_step_menu(cx));
            });

        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(move_up)
            .child(move_down)
            .child(menu)
            .into_any_element()
    }

    // --- render: edit-sub-action side sheet -------------------------------

    pub(super) fn render_sub_action_modal(
        &self,
        form: &EditSubActionForm,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let title = self
            .sub_action_registry
            .get(&form.kind_id)
            .map(|r| r.label().to_owned())
            .unwrap_or_else(|| form.kind_id.clone());
        let bool_vals: HashMap<&str, bool> = form
            .fields
            .iter()
            .filter_map(|f| match f {
                SubFormField::Bool { key, value, .. } => Some((key.as_str(), *value)),
                _ => None,
            })
            .collect();
        let gate_on = |gate: &Option<String>| {
            gate.as_ref()
                .map(|g| bool_vals.get(g.as_str()).copied().unwrap_or(false))
                .unwrap_or(true)
        };

        let mut fields_col = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(title),
            )
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child("CONFIGURATION"),
            );

        let mut rendered_any = false;
        for field in &form.fields {
            match field {
                SubFormField::Input {
                    label, gate, input, ..
                } => {
                    if !gate_on(gate) {
                        continue;
                    }
                    rendered_any = true;
                    fields_col = fields_col.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(spacing(Spacing::Xxs, Density::Cozy))
                            .child(
                                div()
                                    .font_family(DEFAULT_MONO_FAMILY)
                                    .text_size(FONT_XXS)
                                    .text_color(palette.text_faint)
                                    .child(label.clone()),
                            )
                            .child(input.clone()),
                    );
                }
                SubFormField::Bool {
                    key,
                    label,
                    gate,
                    value,
                } => {
                    if !gate_on(gate) {
                        continue;
                    }
                    rendered_any = true;
                    let toggle_key = key.clone();
                    fields_col = fields_col.child(
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(spacing(Spacing::Sm, Density::Cozy))
                            .child(
                                div()
                                    .font_family(DEFAULT_BODY_FAMILY)
                                    .text_size(FONT_XS)
                                    .text_color(palette.text_primary)
                                    .child(label.clone()),
                            )
                            .child(toggle(*value, palette).on_click(
                                SharedString::from(format!("actions-sub-toggle-{key}")),
                                cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    this.toggle_sub_field(toggle_key.clone(), cx)
                                }),
                            )),
                    );
                }
                SubFormField::Hint { label } => {
                    rendered_any = true;
                    fields_col = fields_col.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(spacing(Spacing::Xxs, Density::Cozy))
                            .child(
                                div()
                                    .font_family(DEFAULT_MONO_FAMILY)
                                    .text_size(FONT_XXS)
                                    .text_color(palette.text_faint)
                                    .child(label.clone()),
                            )
                            .child(
                                div()
                                    .font_family(DEFAULT_BODY_FAMILY)
                                    .text_size(FONT_XS)
                                    .text_color(palette.text_faint)
                                    .child("Authored via drill-in on the step."),
                            ),
                    );
                }
            }
        }
        if !rendered_any {
            fields_col = fields_col.child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_muted)
                    .child("This sub-action has no configuration."),
            );
        }

        let body = div()
            .id("actions-sub-scroll")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .py(spacing(Spacing::Md, Density::Cozy))
            .px(spacing(Spacing::Md, Density::Cozy))
            .child(fields_col);

        let cancel = secondary_button("Cancel", palette).on_click(
            "actions-sub-cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_sub_action(cx)),
        );
        let save = primary_button("Save", palette).on_click(
            "actions-sub-submit",
            cx.listener(|this, _: &ClickEvent, _, cx| this.submit_sub_action(cx)),
        );
        let buttons = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(cancel)
            .child(save);

        let footer = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .py(px(12.0))
            .px(px(16.0))
            .border_t(HALF_BORDER)
            .border_color(palette.border_regular)
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child("ESC to cancel"),
            )
            .child(buttons);

        let content = div()
            .size_full()
            .flex()
            .flex_col()
            .child(body)
            .child(footer);

        let sheet = side_sheet(SUB_SHEET_W, content, palette)
            .position(SheetPosition::Right)
            .header("Edit sub-action")
            .on_close(
                "actions-sub-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_sub_action(cx)),
            );

        let view = cx.entity();
        overlay(sheet, palette)
            .position(OverlayPosition::Right(SUB_SHEET_W))
            .on_dismiss("actions-sub-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_sub_action(cx));
            })
            .into_any_element()
    }
}
