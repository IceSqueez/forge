use super::*;
use crate::presentation::ActivePresentation;
use forge_components::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_LG, FONT_SM, FONT_XS,
    FONT_XXS, ForgePalette, GridPicker, GridPickerConfig, GridPickerEvent, GridPickerGroup,
    GridPickerItem, GridPickerItemState, GridPickerSubtitle, Icon, InputEvent, MenuPlacement,
    ModalSize, OverlayPosition, Radius, SheetPosition, Spacing, TextInput, ghost_button_with_icon,
    icon, icon_inherit, menu_button, menu_divider, menu_item, modal, overlay, primary_button,
    radius, row_card, secondary_button, side_sheet, spacing, status_dot, toggle, tr,
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

fn sub_action_summary(step: &SubActionStep) -> (&'static str, String, Option<String>) {
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
    fn wrap_var(name: &str) -> String {
        format!("%{}%", name.trim_matches('%'))
    }
    match step.kind_id.as_str() {
        "twitch.chat.send_message" => {
            let target = step.config.get("target").map(as_str).unwrap_or("twitch");
            let message = step.config.get("message").map(as_str).unwrap_or("");
            (
                "send",
                tr!("action_editor_kind_send_chat"),
                Some(format!("\u{2192} {target}: \"{message}\"")),
            )
        }
        "core.globals.set" => {
            let name = step.config.get("name").map(as_str).unwrap_or("");
            let value = step.config.get("value").map(as_str).unwrap_or("");
            (
                "variable",
                tr!("action_editor_kind_set_global"),
                Some(format!("{name} = \"{value}\"")),
            )
        }
        "core.globals.increment" => {
            let name = step.config.get("name").map(as_str).unwrap_or("");
            let amount = step.config.get("amount").map(as_i64).unwrap_or(1);
            (
                "variable",
                tr!("action_editor_kind_incr_global"),
                Some(format!(
                    "{name} += {amount} {note}",
                    note = tr!("action_editor_persisted_note")
                )),
            )
        }
        "core.logic.wait" => {
            let ms = step.config.get("ms").map(as_i64).unwrap_or(0);
            (
                "clock",
                tr!("action_editor_kind_delay"),
                Some(format!("{ms} ms")),
            )
        }
        "core.log.write" => {
            let level = step.config.get("level").map(as_str).unwrap_or("info");
            let message = step.config.get("message").map(as_str).unwrap_or("");
            (
                "info-circle",
                tr!("action_editor_kind_log"),
                Some(format!("[{level}] \"{message}\"")),
            )
        }
        "script.run.named" => {
            let script_name = step.config.get("script_name").map(as_str).unwrap_or("");
            (
                "script",
                tr!("action_editor_kind_run_script"),
                Some(script_name.to_owned()),
            )
        }
        "soundboard.sound.play" => {
            let clip_id = step.config.get("clip_id").map(as_str).unwrap_or("");
            (
                "music",
                tr!("action_editor_kind_play_sound"),
                Some(clip_id.to_owned()),
            )
        }
        "tts.speak.text" => {
            let text = step.config.get("text").map(as_str).unwrap_or("");
            (
                "volume",
                tr!("action_editor_kind_speak"),
                Some(text.to_owned()),
            )
        }
        "core.file.read" => {
            let path = step.config.get("path").map(as_str).unwrap_or("");
            let var = step.config.get("target_var").map(as_str).unwrap_or("");
            (
                "file",
                tr!("action_editor_kind_read_file"),
                Some(format!("{path} \u{2192} {}", wrap_var(var))),
            )
        }
        "core.random.int" => {
            let min = step.config.get("min").map(as_i64).unwrap_or(0);
            let max = step.config.get("max").map(as_i64).unwrap_or(0);
            let var = step.config.get("target_var").map(as_str).unwrap_or("");
            (
                "dice",
                tr!("action_editor_kind_random_int"),
                Some(format!("[{min}..{max}] \u{2192} {}", wrap_var(var))),
            )
        }
        _ => ("bolt", tr!("action_editor_kind_sub_action"), None),
    }
}

fn sub_category_label(cat: SubActionCategory) -> String {
    match cat {
        SubActionCategory::Chat => tr!("sub_cat_chat"),
        SubActionCategory::Moderation => tr!("sub_cat_moderation"),
        SubActionCategory::ChannelPoints => tr!("sub_cat_channel_points"),
        SubActionCategory::PollsPredictions => tr!("sub_cat_polls_predictions"),
        SubActionCategory::Globals => tr!("sub_cat_globals"),
        SubActionCategory::Logic => tr!("sub_cat_logic"),
        SubActionCategory::Delay => tr!("sub_cat_delay"),
        SubActionCategory::Scripts => tr!("sub_cat_scripts"),
        SubActionCategory::Files => tr!("sub_cat_files"),
        SubActionCategory::Twitch => "Twitch".to_owned(),
        SubActionCategory::YouTube => "YouTube".to_owned(),
        SubActionCategory::Kick => "Kick".to_owned(),
        SubActionCategory::Obs => "OBS".to_owned(),
        SubActionCategory::VTube => "VTube Studio".to_owned(),
        SubActionCategory::Discord => "Discord".to_owned(),
        SubActionCategory::Midi => "MIDI".to_owned(),
        SubActionCategory::Hotkey => tr!("sub_cat_hotkey"),
        SubActionCategory::Audio => tr!("sub_cat_audio"),
        SubActionCategory::Tts => tr!("sub_cat_tts"),
        SubActionCategory::Http => tr!("sub_cat_http"),
        SubActionCategory::Server => tr!("sub_cat_server"),
        SubActionCategory::Util => tr!("sub_cat_util"),
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

fn step_avg_badge(avg_ms: u64, palette: &ForgePalette) -> AnyElement {
    div()
        .flex_shrink_0()
        .py(px(1.0))
        .px(px(6.0))
        .rounded(CHIP_RADIUS)
        .bg(palette.surface_overlay)
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(FONT_XXS)
        .text_color(palette.success)
        .child(tr!("action_step_avg_badge", count = avg_ms as i64))
        .into_any_element()
}

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

fn add_row_button(
    id: impl Into<ElementId>,
    glyph: Icon,
    label: impl Into<SharedString>,
    accent: Rgba,
    palette: &ForgePalette,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let label = label.into();
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

fn empty_placeholder_card(
    glyph: Icon,
    glyph_color: Rgba,
    label: impl Into<SharedString>,
    palette: &ForgePalette,
) -> AnyElement {
    let label = label.into();
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
        label: tr!("action_editor_saved_triggers").into(),
        dot_color: palette.warning,
        scope: SharedString::from("all"),
        items,
    }];
    (groups, picks)
}

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
    pub(super) fn current_chain(&self) -> Vec<SubActionStep> {
        match &self.detail {
            Some(detail) => nav::resolve_chain(&detail.action.sub_actions, &self.nav_path),
            None => Vec::new(),
        }
    }

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
                    cx.push_toast(ToastKind::Success, tr!("action_editor_test_fired"));
                });
            }
            Ok(Ok(false)) => {
                let _ = this.update(cx, |_, cx| {
                    cx.push_toast(ToastKind::Warn, tr!("action_editor_test_no_match"));
                });
            }
            Ok(Err(message)) => {
                let _ = this.update(cx, |_, cx| {
                    cx.push_toast(
                        ToastKind::Error,
                        tr!("action_editor_test_failed", error = message.as_str()),
                    );
                });
            }
            Err(_) => {}
        })
        .detach();
        cx.notify();
    }

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
            .unwrap_or_else(|| tr!("action_editor_this_action"));
        let (groups, picks) = build_step_groups(&self.sub_action_registry, &palette);
        let count = self.sub_action_registry.all().count();
        let config = GridPickerConfig {
            accent: palette.brand,
            header_icon: Icon::LayoutGrid,
            title: tr!("action_editor_picker_add_sub_title").into(),
            subtitle: GridPickerSubtitle::Context {
                lead: tr!("action_editor_picker_inserting_into").into(),
                name: ctx_name.into(),
                note: tr!("action_editor_picker_sub_count", count = count as i64).into(),
            },
            footer_hint: tr!("action_editor_picker_footer_hint").into(),
            search_placeholder: tr!("action_editor_picker_search", count = count as i64).into(),
            scope_cap: Some(7),
        };
        let picker = cx.new(|cx| GridPicker::new(config, groups, palette, cx));
        let sub = cx.subscribe(&picker, Self::on_grid_picker_event);
        picker.update(cx, |f, cx| f.focus(window, cx));
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
            cx.push_toast(ToastKind::Info, tr!("action_editor_no_unlinked_triggers"));
            cx.notify();
            return;
        }
        let palette = cx.palette();
        let action_name = self
            .detail
            .as_ref()
            .map(|d| d.action.name.clone())
            .unwrap_or_else(|| tr!("action_editor_this_action"));
        let count = instances.len();
        let (groups, picks) = build_trigger_groups(&instances, &self.trigger_registry, &palette);
        let config = GridPickerConfig {
            accent: palette.warning,
            header_icon: Icon::Bolt,
            title: tr!("action_editor_add_trigger").into(),
            subtitle: GridPickerSubtitle::Context {
                lead: tr!("action_editor_picker_fires").into(),
                name: action_name.into(),
                note: tr!("action_editor_picker_available_count", count = count as i64).into(),
            },
            footer_hint: tr!("action_editor_trigger_picker_footer_hint").into(),
            search_placeholder: tr!("triggers_search_placeholder").into(),
            scope_cap: Some(6),
        };
        let picker = cx.new(|cx| GridPicker::new(config, groups, palette, cx));
        let sub = cx.subscribe(&picker, Self::on_trigger_picker_event);
        picker.update(cx, |f, cx| f.focus(window, cx));
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
                    .child(tr!("actions_detail_empty_title")),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(tr!("actions_detail_empty_hint")),
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
            .child(tr!("action_editor_loading"))
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
            (palette.success, tr!("action_editor_enabled"))
        } else {
            (palette.text_faint, tr!("action_editor_disabled"))
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
            .unwrap_or_else(|| tr!("action_editor_no_description"));
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

        let id = action.id;
        let menu_open = self.header_menu_open;
        let view = cx.entity();
        let header_menu = menu_button(Icon::DotsVertical, menu_open, palette)
            .placement(MenuPlacement::BottomRight)
            .items(vec![
                menu_item(
                    SharedString::from("actions-header-menu-dup"),
                    tr!("action_editor_duplicate"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.duplicate(id, cx)),
                )
                .icon(Icon::Copy)
                .into(),
                menu_divider(),
                menu_item(
                    SharedString::from("actions-header-menu-del"),
                    tr!("action_editor_menu_delete"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.request_delete(id, cx)),
                )
                .icon(Icon::Eraser)
                .color(palette.random)
                .into(),
            ])
            .on_toggle(
                SharedString::from("actions-header-menu-trigger"),
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_header_menu(cx)),
            )
            .on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_header_menu(cx));
            });

        let btn_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(
                ghost_button_with_icon(Icon::PlayerPlay, tr!("action_editor_test_run"), palette)
                    .on_click(
                        "actions-editor-test",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.test_run(cx)),
                    ),
            )
            .child(
                ghost_button_with_icon(Icon::Pencil, tr!("action_editor_edit"), palette).on_click(
                    "actions-editor-edit",
                    cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.open_edit_modal(window, cx)
                    }),
                ),
            )
            .child(header_menu);

        let header_row = div()
            .flex()
            .items_start()
            .justify_between()
            .child(header_left)
            .child(btn_row);

        let mut col = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, Density::Cozy))
            .child(header_row);
        if let Some(telemetry) = &self.telemetry {
            col = col.child(self.render_stats_row(telemetry, palette));
        }
        col.into_any_element()
    }

    fn toggle_header_menu(&mut self, cx: &mut Context<Self>) {
        self.header_menu_open = !self.header_menu_open;
        cx.notify();
    }

    fn close_header_menu(&mut self, cx: &mut Context<Self>) {
        self.header_menu_open = false;
        cx.notify();
    }

    fn open_edit_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(detail) = self.detail.as_ref() else {
            return;
        };
        let action = &detail.action;
        let id = action.id;
        let seed_name = action.name.clone();
        let seed_desc = action.description.clone().unwrap_or_default();
        let palette = cx.palette();
        let name = cx.new(|cx| {
            let mut input =
                TextInput::new(tr!("actions_name_placeholder"), cx).with_palette(palette);
            input.set_content(seed_name, cx);
            input
        });
        let description = cx.new(|cx| {
            let mut area =
                TextArea::new(tr!("actions_description_placeholder"), cx).with_palette(palette);
            area.set_content(seed_desc, cx);
            area
        });
        name.update(cx, |f, cx| f.focus(window, cx));
        let name_sub = cx.subscribe(&name, |_this, _f, _e: &InputEvent, cx| cx.notify());
        self.header_menu_open = false;
        self.edit_modal = Some(EditActionForm {
            id,
            name,
            description,
            _name_sub: name_sub,
        });
        cx.notify();
    }

    fn cancel_edit_modal(&mut self, cx: &mut Context<Self>) {
        self.edit_modal = None;
        cx.notify();
    }

    fn submit_edit_modal(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self.edit_modal.as_ref() else {
            return;
        };
        let name = form.name.read(cx).content().trim().to_owned();
        if name.is_empty() {
            return;
        }
        let description = form.description.read(cx).content().trim().to_owned();
        let id = form.id;
        let Some(detail) = self.detail.as_ref() else {
            self.edit_modal = None;
            cx.notify();
            return;
        };
        if detail.action.id != id {
            return;
        }
        let mut action = detail.action.clone();
        action.name = name;
        action.description = (!description.is_empty()).then_some(description);
        self.edit_modal = None;
        cx.notify();
        self.persist_action(action, cx);
    }

    pub(super) fn render_edit_modal(
        &self,
        form: &EditActionForm,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let valid = !form.name.read(cx).content().trim().is_empty();

        let name_section = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(tr!("actions_modal_section_name")),
            )
            .child(div().child(form.name.clone()));

        let desc_section = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(tr!("actions_modal_section_description")),
            )
            .child(div().child(form.description.clone()));

        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .child(name_section)
            .child(desc_section);

        let cancel = secondary_button(tr!("actions_modal_cancel_btn"), palette).on_click(
            "actions-edit-modal-cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_edit_modal(cx)),
        );
        let save = primary_button(tr!("action_editor_edit_save_btn"), palette)
            .disabled(!valid)
            .on_click(
                "actions-edit-modal-save",
                cx.listener(|this, _: &ClickEvent, _, cx| this.submit_edit_modal(cx)),
            );
        let footer = div()
            .w_full()
            .flex()
            .items_center()
            .justify_end()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(cancel)
            .child(save);

        let card = modal(tr!("action_editor_edit_modal_title"), body, palette)
            .size(ModalSize::Md)
            .footer(footer)
            .kbd_hint(tr!("actions_esc_hint"))
            .on_close(
                "actions-edit-modal-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_edit_modal(cx)),
            );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("actions-edit-modal-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_edit_modal(cx));
            })
            .into_any_element()
    }

    fn render_stats_row(&self, telemetry: &ActionTelemetry, palette: &ForgePalette) -> AnyElement {
        let last_fired = fmt_relative_time(telemetry.last_fired_at);
        let runs = fmt_number(telemetry.runs_today as f64, 0);
        let avg = match telemetry.avg_duration_ms {
            Some(ms) => tr!("action_stat_avg_ms", count = ms as i64),
            None => tr!("action_stat_avg_none"),
        };
        let errors = telemetry.errors_7d;
        let error_color = if errors > 0 {
            palette.random
        } else {
            palette.text_primary
        };
        let error_hint: Option<SharedString> = if errors == 0 {
            Some(tr!("action_stat_no_errors").into())
        } else {
            None
        };

        div()
            .flex()
            .gap(STAT_GAP)
            .py(STAT_PAD_V)
            .px(CARD_PAD_H)
            .rounded(radius(Radius::Md))
            .bg(palette.base)
            .border(HALF_BORDER)
            .border_color(palette.border_regular)
            .child(self.render_stat_cell(
                tr!("action_stat_last_fired"),
                last_fired,
                palette.text_primary,
                None,
                palette,
            ))
            .child(self.render_stat_cell(
                tr!("action_stat_runs_today"),
                runs,
                palette.text_primary,
                None,
                palette,
            ))
            .child(self.render_stat_cell(
                tr!("action_stat_avg_time"),
                avg,
                palette.success,
                None,
                palette,
            ))
            .child(self.render_stat_cell(
                tr!("action_stat_errors_7d"),
                errors.to_string(),
                error_color,
                error_hint,
                palette,
            ))
            .into_any_element()
    }

    fn render_stat_cell(
        &self,
        label: impl Into<SharedString>,
        value: impl Into<SharedString>,
        value_color: Rgba,
        hint: Option<SharedString>,
        palette: &ForgePalette,
    ) -> AnyElement {
        let mut cell = div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(STAT_VALUE_GAP)
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(label.into()),
            )
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(value_color)
                    .child(value.into()),
            );
        if let Some(hint) = hint {
            cell = cell.child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(hint),
            );
        }
        cell.into_any_element()
    }

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
            .child(tr!(
                "action_editor_section_triggers_count",
                count = detail.trigger_instances.len() as i64
            ));

        let mut col = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, Density::Cozy));
        if detail.trigger_instances.is_empty() {
            col = col.child(empty_placeholder_card(
                Icon::Bolt,
                palette.warning,
                tr!("action_editor_no_triggers"),
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
            tr!("action_editor_add_trigger"),
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
                    .child(tr!("action_editor_sub_actions_count", count = total as i64)),
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
                tr!("action_editor_no_steps")
            } else {
                tr!("action_editor_branch_empty")
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
                    tr!("action_editor_add_step"),
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
        let (fallback_icon, fallback_title, detail_opt) = sub_action_summary(step);
        let runner = self.sub_action_registry.get(&step.kind_id);
        let title = runner
            .map(|r| r.label().to_owned())
            .unwrap_or(fallback_title);
        let glyph = Icon::from_name(runner.map(|r| r.icon_name()).unwrap_or(fallback_icon));
        let detail_str = detail_opt
            .or_else(|| runner.map(|r| r.summary().to_owned()))
            .unwrap_or_else(|| step.kind_id.clone());

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

        let title_text = div()
            .flex_1()
            .font_family(DEFAULT_BODY_FAMILY)
            .font_weight(FontWeight::SEMIBOLD)
            .text_size(FONT_XS)
            .text_color(palette.text_primary)
            .child(title);
        let avg_ms = if depth == 0 {
            self.detail
                .as_ref()
                .and_then(|d| d.sub_action_avg_ms.get(i).copied().flatten())
        } else {
            None
        };
        let mut title_el = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(title_text);
        if let Some(avg) = avg_ms {
            title_el = title_el.child(step_avg_badge(avg, palette));
        }

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
                    tr!("action_editor_step_menu_edit"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.open_edit_sub_action(i, cx)
                    }),
                )
                .icon(Icon::InfoCircle)
                .into(),
                menu_item(
                    SharedString::from(format!("actions-step-dup-{i}")),
                    tr!("action_editor_step_menu_duplicate"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.duplicate_step(i, cx)),
                )
                .icon(Icon::Copy)
                .into(),
                menu_divider(),
                menu_item(
                    SharedString::from(format!("actions-step-top-{i}")),
                    tr!("action_editor_step_menu_move_top"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.move_step_top(i, cx)),
                )
                .icon(Icon::ArrowBarUp)
                .disabled(i == 0)
                .into(),
                menu_item(
                    SharedString::from(format!("actions-step-bottom-{i}")),
                    tr!("action_editor_step_menu_move_bottom"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.move_step_bottom(i, cx)),
                )
                .icon(Icon::ArrowBarDown)
                .disabled(i + 1 >= total)
                .into(),
                menu_divider(),
                menu_item(
                    SharedString::from(format!("actions-step-del-{i}")),
                    tr!("action_editor_step_menu_delete"),
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
                    .child(tr!("action_editor_config_label")),
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
                                    .child(tr!("action_editor_branch_modal_hint")),
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
                    .child(tr!("actions_sub_no_config")),
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

        let cancel = secondary_button(tr!("common_cancel"), palette).on_click(
            "actions-sub-cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_sub_action(cx)),
        );
        let save = primary_button(tr!("common_save"), palette).on_click(
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
                    .child(tr!("actions_esc_hint")),
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
            .header(tr!("actions_sub_modal_edit_title"))
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
