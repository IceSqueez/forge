use super::sub_action_modal::{
    EditSubActionForm, SubFormCommit, SubFormEvent, SubFormLaunch, SubFormTarget,
};
use super::*;
use crate::async_bridge;
use crate::config_form::{
    ChoiceSupport, ConfigField, ConfigFieldHandlers, FILL_VAL_FS, FoldContext,
    collect_field_values, fold_config_field, render_config_row, sparse_overrides,
};
use crate::presentation::ActivePresentation;
use crate::triggers_screen::platform_dot_color;
use forge_components::{
    BORDER_THIN, Density, FONT_LG, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, GridPicker,
    GridPickerConfig, GridPickerEvent, GridPickerGroup, GridPickerItem, GridPickerItemState,
    GridPickerSubtitle, Icon, InputEvent, MenuPlacement, ModalSize, OverlayPosition, PlatformKind,
    Radius, Spacing, TextInput, body_family, ghost_button_with_icon, icon, menu_button,
    menu_divider, menu_item, modal, mono_family, overlay, platform_color, primary_button, radius,
    row_card, secondary_button, spacing, status_dot, tooltip_lines_builder, tr, with_alpha,
};
use forge_registry::{
    FormSchemaSource, SubActionCategory, SubActionRegistry, SubActionRunner, TriggerKindDescriptor,
    TriggerRegistry,
};
use forge_types::{
    ExecutionOutcome, PermissionRung, PlatformScope, SubActionStep, TriggerInstance,
    TriggerInstanceId, Variant,
};
use gpui::{
    AnyElement, App, ClickEvent, Context, ElementId, Entity, FontWeight, Rgba, SharedString,
    Window, div, px,
};
use std::collections::HashMap;

struct SelectOptionsFetch {
    options: HashMap<String, Vec<(String, String)>>,
    overlay_kind_by_identity: HashMap<String, String>,
    concurrent_queue_ids: HashSet<String>,
}

fn analyzer_finding_message(finding: &analyzer::Finding) -> SharedString {
    let text = match finding {
        analyzer::Finding::UnknownVariable(name) => {
            tr!("action_editor_health_unknown_var", name = name.clone())
        }
        analyzer::Finding::ProducedLater(name) => {
            tr!("action_editor_health_produced_later", name = name.clone())
        }
        analyzer::Finding::IsolatedSibling(name) => {
            tr!("action_editor_health_isolated_sibling", name = name.clone())
        }
        analyzer::Finding::SomeTriggersOnly(name) => {
            tr!("action_editor_health_some_triggers", name = name.clone())
        }
        analyzer::Finding::LastRunFailed(message) => {
            tr!(
                "action_editor_health_last_run_failed",
                message = message.clone()
            )
        }
    };
    SharedString::from(text)
}

fn sanitize_action_stem(name: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in name.chars() {
        if ch.is_alphanumeric() {
            out.push(ch);
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "action".to_owned()
    } else {
        trimmed.to_owned()
    }
}

async fn export_action_to_chosen_file(action: Action) -> Result<std::path::PathBuf, String> {
    let json = serde_json::to_string_pretty(&action).map_err(|e| e.to_string())?;
    let default_name = format!("{}.forge.json", sanitize_action_stem(&action.name));
    let filter = async_bridge::DialogFilter {
        name: "JSON".to_owned(),
        extensions: &["json"],
    };
    let path = async_bridge::save_file(Some(filter), Some(default_name)).await?;
    tokio::fs::write(&path, json)
        .await
        .map_err(|e| e.to_string())?;
    Ok(path)
}

pub(super) fn sub_action_summary(step: &SubActionStep) -> (&'static str, String, Option<String>) {
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
            let target = step.config.get("target_var").map(as_str).unwrap_or("");
            let detail = if target.is_empty() {
                script_name.to_owned()
            } else {
                format!("{script_name} \u{2192} {}", wrap_var(target))
            };
            ("script", tr!("action_editor_kind_run_script"), Some(detail))
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
        SubActionCategory::Overlay => tr!("sub_cat_overlay"),
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
        SubActionCategory::Overlay => "overlay",
        SubActionCategory::Util => "util",
    }
}

pub(super) fn sub_category_color(cat: SubActionCategory, palette: &ForgePalette) -> Rgba {
    match cat {
        SubActionCategory::Chat | SubActionCategory::Twitch => palette.brand,
        SubActionCategory::Tts | SubActionCategory::Audio => palette.success,
        SubActionCategory::Globals => palette.warning,
        SubActionCategory::Files => palette.random,
        SubActionCategory::YouTube => platform_color(PlatformKind::YouTube, palette),
        SubActionCategory::Kick => platform_color(PlatformKind::Kick, palette),
        SubActionCategory::Obs => palette.text_secondary,
        SubActionCategory::VTube => palette.accent_teal,
        SubActionCategory::Discord
        | SubActionCategory::Http
        | SubActionCategory::Server
        | SubActionCategory::Overlay => palette.info,
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

pub(super) fn step_glyph(
    kind_id: &str,
    fallback_icon: &str,
    fallback_color: Option<Rgba>,
    palette: &ForgePalette,
) -> (Icon, Rgba) {
    let (name, color) = match kind_id {
        "core.file.read" => ("file-text", palette.info),
        "core.random.int" => ("dice", palette.random),
        "script.run.named" => ("code", palette.success),
        "core.globals.increment" | "core.globals.set" => ("variable", palette.warning),
        "twitch.chat.send_message" => ("send", palette.brand),
        _ => {
            return (
                Icon::from_name(fallback_icon),
                fallback_color.unwrap_or(palette.text_secondary),
            );
        }
    };
    (Icon::from_name(name), color)
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

fn build_recent_group(
    instances: &[TriggerInstance],
    registry: &TriggerRegistry,
    palette: &ForgePalette,
) -> (
    Option<GridPickerGroup>,
    HashMap<SharedString, TriggerInstanceId>,
) {
    let mut picks: HashMap<SharedString, TriggerInstanceId> = HashMap::new();
    if instances.is_empty() {
        return (None, picks);
    }
    let mut items: Vec<GridPickerItem> = Vec::with_capacity(instances.len());
    for instance in instances {
        let descriptor = registry.get(&instance.kind_id);
        let id = SharedString::from(format!("recent-{}", instance.id));
        picks.insert(id.clone(), instance.id);
        let glyph = Icon::from_name(
            descriptor
                .map(TriggerKindDescriptor::icon_name)
                .unwrap_or("bolt"),
        );
        let kind_label = descriptor
            .map(|d| d.label().to_owned())
            .unwrap_or_else(|| instance.kind_id.clone());
        let condition = descriptor
            .map(|d| d.condition_display(&instance.overrides))
            .unwrap_or_default();
        let desc = if condition.is_empty() {
            kind_label
        } else {
            format!("{kind_label} \u{b7} {condition}")
        };
        let state = if instance.enabled {
            GridPickerItemState::Normal
        } else {
            GridPickerItemState::Disabled
        };
        items.push(GridPickerItem {
            id,
            icon: glyph,
            icon_color: platform_dot_color(&instance.kind_id, palette),
            name: instance.name.clone().into(),
            desc: desc.into(),
            state,
        });
    }
    let group = GridPickerGroup {
        label: tr!("action_editor_recent_triggers").into(),
        dot_color: palette.warning,
        scope: SharedString::from("all"),
        items,
    };
    (Some(group), picks)
}

pub(crate) fn parse_variable_segments(s: &str) -> Vec<(&str, bool)> {
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

fn variable_text(s: &str, palette: &ForgePalette) -> AnyElement {
    if s.is_empty() {
        return div()
            .font_family(mono_family())
            .text_size(FONT_XS)
            .text_color(palette.text_muted)
            .child(String::new())
            .into_any_element();
    }
    let mut row = div()
        .flex()
        .flex_wrap()
        .font_family(mono_family())
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
        .hover(move |s| s.bg(hover).border_color(accent))
        .on_click(handler)
        .child(icon(glyph, CARD_GLYPH, accent))
        .child(
            div()
                .font_family(body_family())
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
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(palette.text_muted)
                .child(label),
        )
        .into_any_element()
}

fn inline_warning_card(
    title: impl Into<SharedString>,
    hint: impl Into<SharedString>,
    palette: &ForgePalette,
) -> AnyElement {
    let title = title.into();
    let hint = hint.into();
    div()
        .w_full()
        .flex()
        .items_start()
        .gap(spacing(Spacing::Xs, Density::Cozy))
        .py(CARD_PAD_V)
        .px(CARD_PAD_H)
        .rounded(radius(Radius::Md))
        .bg(with_alpha(palette.warning, STEP_HEALTH_TILE_ALPHA))
        .child(icon(Icon::AlertTriangle, CARD_GLYPH, palette.warning))
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xxs, Density::Cozy))
                .child(
                    div()
                        .font_family(body_family())
                        .text_size(FONT_XS)
                        .text_color(palette.text_primary)
                        .child(title),
                )
                .child(
                    div()
                        .font_family(body_family())
                        .text_size(FONT_XXS)
                        .text_color(palette.text_faint)
                        .child(hint),
                ),
        )
        .into_any_element()
}

fn trigger_unlink_btn(
    id: impl Into<ElementId>,
    palette: &ForgePalette,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let solid = palette.random;
    div()
        .id(id.into())
        .flex()
        .items_center()
        .justify_center()
        .p(spacing(Spacing::Xxs, Density::Cozy))
        .rounded(radius(Radius::Sm))
        .cursor_pointer()
        .hover(move |s| s.bg(solid))
        .on_click(handler)
        .child(icon(Icon::X, UNLINK_GLYPH, palette.text_faint))
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

    fn toggle_step_menu(&mut self, i: usize, position: Point<Pixels>, cx: &mut Context<Self>) {
        if self.step_menu_open == Some(i) {
            self.step_menu_open = None;
        } else {
            self.step_menu_open = Some(i);
            self.menu_click_pos = Some(position);
        }
        cx.notify();
    }

    fn close_step_menu(&mut self, cx: &mut Context<Self>) {
        self.step_menu_open = None;
        cx.notify();
    }

    fn kind_label(&self, kind_id: &str) -> String {
        self.sub_action_registry
            .get(kind_id)
            .map(|r| r.label().to_owned())
            .unwrap_or_else(|| kind_id.to_owned())
    }

    fn open_sub_form(&mut self, launch: SubFormLaunch, cx: &mut Context<Self>) {
        let form = cx.new(|cx| EditSubActionForm::new(launch, self.rt_handle.clone(), cx));
        self._sub_form_sub = Some(cx.subscribe(&form, Self::on_sub_form_event));
        self.sub_form = Some(form);
        self.fetch_select_options(cx);
        cx.notify();
    }

    fn on_sub_form_event(
        &mut self,
        _form: Entity<EditSubActionForm>,
        event: &SubFormEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            SubFormEvent::Commit(commit) => {
                let commit = commit.clone();
                self.sub_form = None;
                self._sub_form_sub = None;
                self.step_menu_open = None;
                self.apply_sub_form_commit(commit, cx);
                cx.notify();
            }
            SubFormEvent::Cancel => {
                self.sub_form = None;
                self._sub_form_sub = None;
                cx.notify();
            }
        }
    }

    fn apply_sub_form_commit(&mut self, commit: SubFormCommit, cx: &mut Context<Self>) {
        let SubFormCommit {
            target,
            kind_id,
            overrides,
            continue_on_error,
            condition,
            label,
        } = commit;
        match target {
            SubFormTarget::Edit(index) => {
                self.persist_chain_mutation(
                    move |chain| {
                        if let Some(step) = chain.get_mut(index) {
                            for (key, value) in overrides {
                                step.config.insert(key, value);
                            }
                            step.continue_on_error = continue_on_error;
                            step.condition = condition;
                            step.label = label;
                        }
                    },
                    cx,
                );
            }
            SubFormTarget::Add => {
                let mut config = self
                    .sub_action_registry
                    .get(&kind_id)
                    .map(|r| r.default_config())
                    .unwrap_or_default();
                for (key, value) in overrides {
                    config.insert(key, value);
                }
                self.persist_chain_mutation(
                    move |chain| {
                        chain.push(SubActionStep {
                            kind_id,
                            config,
                            enabled: true,
                            continue_on_error,
                            condition,
                            label,
                        });
                    },
                    cx,
                );
            }
        }
    }

    fn open_edit_sub_action(&mut self, i: usize, cx: &mut Context<Self>) {
        let chain = self.current_chain();
        let Some(step) = chain.get(i) else {
            return;
        };
        let kind_id = step.kind_id.clone();
        let config = step.config.clone();
        let continue_on_error = step.continue_on_error;
        let Some((specs, icon_name, category, refinement)) =
            self.sub_action_registry.get(&kind_id).map(|r| {
                (
                    r.config_fields(),
                    r.icon_name().to_owned(),
                    r.category(),
                    r.config_refinement(),
                )
            })
        else {
            return;
        };
        let kind_label = self.kind_label(&kind_id);
        let name_value = step.label.clone().unwrap_or_else(|| kind_label.clone());
        let condition_value = step.condition.clone().unwrap_or_default();
        let chain_len = chain.len();
        let launch = SubFormLaunch {
            kind_id,
            target: SubFormTarget::Edit(i),
            specs,
            config,
            name_value,
            condition_value,
            continue_on_error,
            kind_label,
            icon_name,
            category: Some(category),
            chain_len,
            options_seed: self.select_options.clone(),
            refinement,
            schema: Arc::clone(&self.overlay_schema) as Arc<dyn FormSchemaSource>,
        };
        self.step_menu_open = None;
        self.open_sub_form(launch, cx);
    }

    fn set_step_enabled(&mut self, i: usize, enabled: bool, cx: &mut Context<Self>) {
        self.step_menu_open = None;
        self.persist_chain_mutation(
            move |chain| {
                if let Some(step) = chain.get_mut(i) {
                    step.enabled = enabled;
                }
            },
            cx,
        );
        cx.notify();
    }

    fn fetch_select_options(&self, cx: &mut Context<Self>) {
        let action_repo = Arc::clone(&self.action_repo);
        let queue_repo = Arc::clone(&self.queue_repo);
        let ti_repo = Arc::clone(&self.trigger_instance_repo);
        let script_repo = Arc::clone(&self.script_repo);
        let soundboard_repo = Arc::clone(&self.soundboard_repo);
        let globals_repo = Arc::clone(&self.globals_repo);
        let overlay_repo = Arc::clone(&self.overlay_repo);
        let tts_registry = self.tts_registry.clone();
        let speak = self.speak.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let mut map: HashMap<String, Vec<(String, String)>> = HashMap::new();
                let mut overlay_kind_by_identity: HashMap<String, String> = HashMap::new();
                let mut concurrent_queue_ids: HashSet<String> = HashSet::new();
                if let Ok(actions) = action_repo.list().await {
                    map.insert(
                        "action.ids".to_owned(),
                        actions
                            .into_iter()
                            .map(|a| (a.id.to_string(), a.name))
                            .collect(),
                    );
                }
                if let Ok(queues) = queue_repo.list().await {
                    concurrent_queue_ids = queues
                        .iter()
                        .filter(|q| !q.is_serial())
                        .map(|q| q.id.to_string())
                        .collect();
                    map.insert(
                        "queue.ids".to_owned(),
                        queues
                            .into_iter()
                            .map(|q| (q.id.to_string(), q.name))
                            .collect(),
                    );
                }
                if let Ok(instances) = ti_repo.list_all().await {
                    map.insert(
                        "trigger_instance.ids".to_owned(),
                        instances
                            .into_iter()
                            .map(|ti| (ti.id.to_string(), ti.name))
                            .collect(),
                    );
                }
                if let Ok(scripts) = script_repo.list().await {
                    map.insert(
                        "script.names".to_owned(),
                        scripts
                            .into_iter()
                            .map(|s| (s.name.clone(), s.name))
                            .collect(),
                    );
                }
                if let Ok(clips) = soundboard_repo.list().await {
                    map.insert(
                        "soundboard.clip_ids".to_owned(),
                        clips
                            .into_iter()
                            .map(|c| (c.id.to_string(), c.name))
                            .collect(),
                    );
                }
                if let Ok(globals) = globals_repo.list().await {
                    map.insert(
                        "global.names".to_owned(),
                        globals
                            .into_iter()
                            .map(|g| (g.name.clone(), g.name))
                            .collect(),
                    );
                }
                if let Ok(overlays) = overlay_repo.list().await {
                    map.insert(
                        "overlay.ids".to_owned(),
                        overlays
                            .iter()
                            .map(|o| (o.id.to_string(), o.display_name.clone()))
                            .collect(),
                    );
                    overlay_kind_by_identity = overlays
                        .into_iter()
                        .map(|o| (o.id.to_string(), o.kind_id))
                        .collect();
                }
                if let Some(registry) = tts_registry {
                    let ids = registry
                        .read()
                        .unwrap_or_else(|e| e.into_inner())
                        .engine_ids();
                    map.insert(
                        "tts.engine_ids".to_owned(),
                        ids.into_iter().map(|id| (id.0.clone(), id.0)).collect(),
                    );
                }
                if let Some(speak) = speak {
                    for voice in speak.available_voices().iter() {
                        map.entry(format!("tts.voices.{}", voice.engine_id.0))
                            .or_default()
                            .push((voice.id.0.clone(), voice.name.clone()));
                    }
                }
                SelectOptionsFetch {
                    options: map,
                    overlay_kind_by_identity,
                    concurrent_queue_ids,
                }
            },
            |this, fetch, cx| this.on_select_options_fetched(fetch, cx),
            cx,
        );
    }

    fn on_select_options_fetched(&mut self, fetch: SelectOptionsFetch, cx: &mut Context<Self>) {
        let SelectOptionsFetch {
            options,
            overlay_kind_by_identity,
            concurrent_queue_ids,
        } = fetch;
        self.overlay_schema = Arc::new(
            self.overlay_schema
                .with_identities(overlay_kind_by_identity),
        );
        self.concurrent_queue_ids = concurrent_queue_ids;
        if let Some(form) = self.sub_form.clone() {
            let schema = Arc::clone(&self.overlay_schema) as Arc<dyn FormSchemaSource>;
            form.update(cx, |form, cx| {
                form.apply_options(&options, cx);
                form.set_schema(schema, cx);
            });
        }
        self.select_options = options;
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
            favorites_label: tr!("picker_favorites").into(),
            favorites_empty: tr!("picker_favorites_empty").into(),
        };
        let favorites = self.sub_action_favorites.clone();
        let picker = cx.new(|cx| GridPicker::new(config, groups, favorites, palette, cx));
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
            GridPickerEvent::FavoriteToggled(id) => {
                if self.sub_action_favorites.contains(id) {
                    self.sub_action_favorites.remove(id);
                } else {
                    self.sub_action_favorites.insert(id.clone());
                }
                let favorites = self.sub_action_favorites.clone();
                self.persist_favorites(reserved_keys::PICKER_FAVORITES_SUB_ACTIONS, favorites, cx);
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
        let prepared = self
            .sub_action_registry
            .get(&kind_id)
            .filter(|_| same)
            .map(|runner| {
                (
                    runner.default_config(),
                    runner.config_fields(),
                    runner.icon_name().to_owned(),
                    runner.category(),
                    runner.config_refinement(),
                )
            });
        self.grid_picker = None;
        let Some((config, specs, icon_name, category, refinement)) = prepared else {
            cx.notify();
            return;
        };
        let kind_label = self.kind_label(&kind_id);
        let chain_len = self.current_chain().len();
        let launch = SubFormLaunch {
            kind_id,
            target: SubFormTarget::Add,
            specs,
            config,
            name_value: kind_label.clone(),
            condition_value: String::new(),
            continue_on_error: false,
            kind_label,
            icon_name,
            category: Some(category),
            chain_len,
            options_seed: self.select_options.clone(),
            refinement,
            schema: Arc::clone(&self.overlay_schema) as Arc<dyn FormSchemaSource>,
        };
        self.step_menu_open = None;
        self.open_sub_form(launch, cx);
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
                    this.open_trigger_picker_with(action_id, instances, window, cx)
                });
            }
            Ok(Err(message)) => {
                let _ = this.update(cx, |this, cx| this.on_repo_error(&message, cx));
            }
            Err(_) => {}
        })
        .detach();
    }

    fn open_trigger_picker_with(
        &mut self,
        action_id: ActionId,
        instances: Vec<TriggerInstance>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected != Some(action_id) || self.detail.is_none() {
            return;
        }
        let palette = cx.palette();
        let action_name = self
            .detail
            .as_ref()
            .map(|d| d.action.name.clone())
            .unwrap_or_else(|| tr!("action_editor_this_action"));
        let (mut kind_groups, picks_kind) =
            crate::triggers_screen::build_kind_groups(&self.trigger_registry, &palette);
        let (recent_group, picks_instance) =
            build_recent_group(&instances, &self.trigger_registry, &palette);
        let mut groups = Vec::with_capacity(kind_groups.len() + 1);
        groups.extend(recent_group);
        groups.append(&mut kind_groups);
        let count = self.trigger_registry.all().count();
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
            favorites_label: tr!("picker_favorites").into(),
            favorites_empty: tr!("picker_favorites_empty").into(),
        };
        let favorites = self.trigger_favorites.clone();
        let picker = cx.new(|cx| GridPicker::new(config, groups, favorites, palette, cx));
        let sub = cx.subscribe(&picker, Self::on_trigger_picker_event);
        picker.update(cx, |f, cx| f.focus(window, cx));
        self.add_trigger = Some(AddTriggerStage::Pick(AddTriggerPicker {
            picker,
            picks_kind,
            picks_instance,
            action_id,
            _sub: sub,
        }));
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
                let Some(AddTriggerStage::Pick(picker)) = self.add_trigger.as_ref() else {
                    return;
                };
                let action_id = picker.action_id;
                if let Some(instance_id) = picker.picks_instance.get(id).copied() {
                    self.link_existing_trigger(action_id, instance_id, cx);
                } else if let Some(kind_id) = picker.picks_kind.get(id).cloned() {
                    self.enter_trigger_fill(action_id, kind_id, cx);
                }
            }
            GridPickerEvent::FavoriteToggled(id) => {
                if self.trigger_favorites.contains(id) {
                    self.trigger_favorites.remove(id);
                } else {
                    self.trigger_favorites.insert(id.clone());
                }
                let favorites = self.trigger_favorites.clone();
                self.persist_favorites(reserved_keys::PICKER_FAVORITES_TRIGGERS, favorites, cx);
            }
            GridPickerEvent::Dismissed => self.cancel_trigger_picker(cx),
        }
    }

    pub(super) fn cancel_trigger_picker(&mut self, cx: &mut Context<Self>) {
        self.add_trigger = None;
        cx.notify();
    }

    fn link_existing_trigger(
        &mut self,
        action_id: ActionId,
        instance_id: TriggerInstanceId,
        cx: &mut Context<Self>,
    ) {
        self.add_trigger = None;
        if self.selected != Some(action_id) {
            cx.notify();
            return;
        }
        let service = Arc::clone(&self.actions_service);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                service
                    .link_trigger_instance(action_id, instance_id)
                    .await
                    .map_err(|e| e.to_string())
            },
            |this, result, cx| match result {
                Ok(()) => this.reload_detail(cx),
                Err(message) => this.on_repo_error(&message, cx),
            },
            cx,
        );
        cx.notify();
    }

    fn enter_trigger_fill(&mut self, action_id: ActionId, kind_id: String, cx: &mut Context<Self>) {
        let palette = cx.palette();
        let descriptor = self.trigger_registry.get(&kind_id);
        let kind_label = descriptor
            .map(|d| d.label().to_owned())
            .unwrap_or_else(|| kind_id.clone());
        let default = descriptor.map(|d| d.default_config()).unwrap_or_default();
        let specs = descriptor.map(|d| d.config_fields()).unwrap_or_default();

        let fold = FoldContext {
            config: &default,
            palette: &palette,
            choices: ChoiceSupport::Text,
            on_committed: Self::on_trigger_config_committed,
        };
        let mut fields: Vec<ConfigField> = Vec::new();
        for spec in &specs {
            fold_config_field(spec, None, &fold, &mut fields, cx);
        }

        let name_field = cx.new(|cx| {
            TextInput::new(tr!("triggers_create_name_placeholder"), cx)
                .with_palette(palette)
                .static_chrome(palette.brand, Radius::Sm)
        });
        let name_sub = cx.subscribe(&name_field, Self::on_trigger_name_event);

        self.add_trigger = Some(AddTriggerStage::Fill(AddTriggerFill {
            action_id,
            kind_id,
            kind_label,
            name_field,
            fields,
            saving: false,
            _name_sub: name_sub,
        }));
        cx.notify();
    }

    fn on_trigger_name_event(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Submitted(_) => self.submit_trigger_fill(cx),
            InputEvent::Cancelled => self.cancel_trigger_picker(cx),
            InputEvent::Changed(_) => cx.notify(),
        }
    }

    fn on_trigger_config_committed(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if let InputEvent::Submitted(_) = event {
            self.submit_trigger_fill(cx);
        }
    }

    fn toggle_trigger_config_field(&mut self, key: String, cx: &mut Context<Self>) {
        if let Some(AddTriggerStage::Fill(form)) = self.add_trigger.as_mut() {
            for field in &mut form.fields {
                if let ConfigField::Bool { key: k, value, .. } = field
                    && *k == key
                {
                    *value = !*value;
                }
            }
        }
        cx.notify();
    }

    fn slide_trigger_config_field(&mut self, key: String, next: i64, cx: &mut Context<Self>) {
        if let Some(AddTriggerStage::Fill(form)) = self.add_trigger.as_mut() {
            for field in &mut form.fields {
                if let ConfigField::Slide { key: k, value, .. } = field
                    && *k == key
                {
                    *value = next;
                }
            }
        }
        cx.notify();
    }

    fn pick_trigger_config_field(&mut self, key: String, choice: String, cx: &mut Context<Self>) {
        if let Some(AddTriggerStage::Fill(form)) = self.add_trigger.as_mut() {
            for field in &mut form.fields {
                if let ConfigField::Swatch {
                    key: k, selected, ..
                } = field
                    && *k == key
                {
                    selected.clone_from(&choice);
                }
            }
        }
        cx.notify();
    }

    fn trigger_config_handlers() -> ConfigFieldHandlers<Self> {
        ConfigFieldHandlers {
            toggle: Self::toggle_trigger_config_field,
            slide: Self::slide_trigger_config_field,
            pick: Self::pick_trigger_config_field,
            open_choice: None,
        }
    }

    fn back_to_trigger_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_trigger_picker(window, cx);
    }

    fn submit_trigger_fill(&mut self, cx: &mut Context<Self>) {
        let Some(AddTriggerStage::Fill(form)) = self.add_trigger.as_ref() else {
            return;
        };
        if form.saving {
            return;
        }
        let name = form.name_field.read(cx).content().trim().to_owned();
        if name.is_empty() {
            return;
        }
        let action_id = form.action_id;
        if self.selected != Some(action_id) {
            self.add_trigger = None;
            cx.notify();
            return;
        }
        let kind_id = form.kind_id.clone();
        let default = self
            .trigger_registry
            .get(&kind_id)
            .map(|d| d.default_config())
            .unwrap_or_default();
        let mut buffer = default.clone();
        collect_field_values(&form.fields, &mut buffer, cx);
        let overrides = sparse_overrides(&default, &buffer);

        let new_id = TriggerInstanceId::new();
        let instance = TriggerInstance {
            id: new_id,
            kind_id,
            name,
            overrides,
            enabled: true,
            user_defined: true,
            platform_scope: PlatformScope::Any,
            cooldown_secs: 0,
            cooldown_global: true,
            permission_rung: PermissionRung::Everyone,
        };

        if let Some(AddTriggerStage::Fill(form)) = self.add_trigger.as_mut() {
            form.saving = true;
        }
        cx.notify();

        let repo = Arc::clone(&self.trigger_instance_repo);
        let service = Arc::clone(&self.actions_service);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                repo.save(&instance).await.map_err(|e| e.to_string())?;
                service
                    .link_trigger_instance(action_id, new_id)
                    .await
                    .map_err(|e| e.to_string())
            },
            |this, result: Result<(), String>, cx| match result {
                Ok(()) => {
                    this.add_trigger = None;
                    this.reload_detail(cx);
                    cx.notify();
                }
                Err(message) => {
                    if let Some(AddTriggerStage::Fill(form)) = this.add_trigger.as_mut() {
                        form.saving = false;
                    }
                    this.on_repo_error(&message, cx);
                }
            },
            cx,
        );
    }

    pub(super) fn render_add_trigger(
        &self,
        stage: &AddTriggerStage,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match stage {
            AddTriggerStage::Pick(picker) => {
                let view = cx.entity();
                overlay(picker.picker.clone(), palette)
                    .position(OverlayPosition::Center)
                    .on_dismiss("actions-trigger-grid-scrim", move |_window, cx| {
                        view.update(cx, |this, cx| this.cancel_trigger_picker(cx));
                    })
                    .into_any_element()
            }
            AddTriggerStage::Fill(form) => self.render_trigger_fill(form, palette, cx),
        }
    }

    fn render_trigger_fill(
        &self,
        form: &AddTriggerFill,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let dot_color = platform_dot_color(&form.kind_id, palette);
        let glyph = self
            .trigger_registry
            .get(&form.kind_id)
            .map(|d| Icon::from_name(d.icon_name()))
            .unwrap_or(Icon::Bolt);

        let name_section = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(self.fill_section_label(tr!("triggers_create_section_name"), palette))
            .child(div().child(form.name_field.clone()));

        let config_card: AnyElement = if form.fields.is_empty() {
            div()
                .py(spacing(Spacing::Sm, Density::Cozy))
                .px(spacing(Spacing::Sm, Density::Cozy))
                .italic()
                .font_family(body_family())
                .text_size(FILL_VAL_FS)
                .text_color(palette.text_faint)
                .child(tr!("triggers_sheet_no_config"))
                .into_any_element()
        } else {
            let last = form.fields.len().saturating_sub(1);
            let view = cx.entity();
            let mut col = div().flex().flex_col();
            for (i, field) in form.fields.iter().enumerate() {
                col = col.child(render_config_row(
                    field,
                    i == last,
                    palette,
                    "actions-trigger-field",
                    &view,
                    &Self::trigger_config_handlers(),
                ));
            }
            div()
                .w_full()
                .rounded(radius(Radius::Md))
                .border(BORDER_THIN)
                .border_color(palette.border_regular)
                .bg(palette.shell)
                .child(col)
                .into_any_element()
        };

        let config_section = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(self.fill_section_label(tr!("triggers_create_section_config"), palette))
            .child(config_card);

        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, Density::Cozy))
            .child(name_section)
            .child(config_section);

        let can_create = !form.name_field.read(cx).content().trim().is_empty() && !form.saving;

        let back = ghost_button_with_icon(Icon::ArrowBackUp, tr!("triggers_create_back"), palette)
            .on_click(
                "actions-trigger-fill-back",
                cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.back_to_trigger_picker(window, cx)
                }),
            );
        let cancel = secondary_button(tr!("triggers_create_cancel"), palette).on_click(
            "actions-trigger-fill-cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_trigger_picker(cx)),
        );
        let create = primary_button(tr!("triggers_create_btn"), palette)
            .disabled(!can_create)
            .on_click(
                "actions-trigger-fill-submit",
                cx.listener(|this, _: &ClickEvent, _, cx| this.submit_trigger_fill(cx)),
            );
        let footer = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(back)
            .child(div().flex_1())
            .child(cancel)
            .child(create);

        let card = modal(
            tr!(
                "triggers_create_new_instance",
                kind = form.kind_label.as_str()
            ),
            body,
            palette,
        )
        .header_icon(glyph, dot_color)
        .subtitle(form.kind_id.clone())
        .size(ModalSize::Md)
        .footer(footer)
        .kbd_hint(tr!("triggers_create_kbd_hint"))
        .on_close(
            "actions-trigger-fill-close",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_trigger_picker(cx)),
        );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("actions-trigger-fill-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_trigger_picker(cx));
            })
            .into_any_element()
    }

    fn fill_section_label(
        &self,
        label: impl Into<SharedString>,
        palette: &ForgePalette,
    ) -> AnyElement {
        div()
            .font_family(mono_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_muted)
            .child(label.into())
            .into_any_element()
    }

    fn unlink_trigger(&mut self, instance_id: TriggerInstanceId, cx: &mut Context<Self>) {
        let Some(action_id) = self.selected else {
            return;
        };
        let service = Arc::clone(&self.actions_service);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                service
                    .unlink_trigger_instance(action_id, instance_id)
                    .await
                    .map_err(|e| e.to_string())
            },
            |this, result, cx| match result {
                Ok(()) => this.reload_detail(cx),
                Err(message) => this.on_repo_error(&message, cx),
            },
            cx,
        );
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
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.text_secondary)
                    .child(tr!("actions_detail_empty_title")),
            )
            .child(
                div()
                    .font_family(body_family())
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
            .font_family(body_family())
            .text_size(FONT_SM)
            .text_color(palette.text_muted)
            .child(tr!("action_editor_loading"))
            .into_any_element()
    }

    pub(super) fn overlay_order_at_risk(&self, action: &Action) -> bool {
        self.concurrent_queue_ids
            .contains(&action.queue_id.to_string())
            && analyzer::sends_order_sensitive_overlay(
                &action.sub_actions,
                &self.sub_action_registry,
                &|identity| self.overlay_schema.is_order_sensitive(identity),
            )
    }

    fn render_order_warning(&self, action: &Action, palette: &ForgePalette) -> Option<AnyElement> {
        self.overlay_order_at_risk(action).then(|| {
            inline_warning_card(
                tr!("action_editor_overlay_order_warning"),
                tr!("action_editor_overlay_order_warning_hint"),
                palette,
            )
        })
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
            .children(self.render_order_warning(&detail.action, palette))
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
        let pill_id = action.id;
        let pill_enabled = action.enabled;
        let pill = div()
            .id("actions-editor-status-pill")
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .py(px(1.0))
            .px(px(6.0))
            .rounded(PILL_RADIUS)
            .bg(palette.surface_overlay)
            .cursor_pointer()
            .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                if event.click_count() >= 2 {
                    this.set_enabled(pill_id, !pill_enabled, cx);
                }
            }))
            .child(status_dot(pill_color, PILL_DOT))
            .child(
                div()
                    .font_family(body_family())
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
                    .font_family(body_family())
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
            .font_family(body_family())
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
        let header_menu = menu_button(Icon::DotsVertical, menu_open.is_some(), palette)
            .placement(MenuPlacement::BottomRight)
            .open_at(menu_open)
            .items(vec![
                menu_item(
                    SharedString::from("actions-header-menu-dup"),
                    tr!("action_editor_duplicate"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.duplicate(id, cx)),
                )
                .icon(Icon::Copy)
                .into(),
                menu_item(
                    SharedString::from("actions-header-menu-history"),
                    tr!("action_editor_run_history"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.open_history_modal(cx)),
                )
                .icon(Icon::History)
                .into(),
                menu_item(
                    SharedString::from("actions-header-menu-export"),
                    tr!("action_editor_export"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.export_json(cx)),
                )
                .icon(Icon::Download)
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
                cx.listener(|this, ev: &ClickEvent, _, cx| {
                    this.toggle_header_menu(ev.position(), cx)
                }),
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
                    .height(HEADER_ACTION_H)
                    .on_click(
                        "actions-editor-test",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.start_test_run(cx)),
                    ),
            )
            .child(
                ghost_button_with_icon(Icon::Edit, tr!("action_editor_edit"), palette)
                    .height(HEADER_ACTION_H)
                    .on_click(
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
            col = col.child(self.render_stats_row(telemetry, palette, cx));
        }
        col.into_any_element()
    }

    fn toggle_header_menu(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        self.header_menu_open = if self.header_menu_open.is_some() {
            None
        } else {
            Some(position)
        };
        cx.notify();
    }

    fn close_header_menu(&mut self, cx: &mut Context<Self>) {
        self.header_menu_open = None;
        cx.notify();
    }

    fn export_json(&mut self, cx: &mut Context<Self>) {
        let Some(detail) = self.detail.as_ref() else {
            return;
        };
        let action = detail.action.clone();
        self.header_menu_open = None;
        async_bridge::spawn_dialog(
            &self.rt_handle,
            export_action_to_chosen_file(action),
            |_this, result, cx| match result {
                Ok(path) => {
                    let shown = path.display().to_string();
                    cx.push_toast(
                        ToastKind::Success,
                        tr!("action_editor_export_done", path = shown.as_str()),
                    );
                }
                Err(reason) if reason == async_bridge::DIALOG_CANCELLED => {}
                Err(reason) => {
                    cx.push_toast(
                        ToastKind::Error,
                        tr!("action_editor_export_failed", error = reason.as_str()),
                    );
                }
            },
            cx,
        );
        cx.notify();
    }

    fn open_edit_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(detail) = self.detail.as_ref() else {
            return;
        };
        let action = detail.action.clone();
        self.header_menu_open = None;
        self.open_action_modal(Some(action), window, cx);
    }

    fn render_stats_row(
        &self,
        telemetry: &ActionTelemetry,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
            palette.success
        };

        let (exec_value, exec_color): (SharedString, Rgba) = match &self.last_outcome {
            Some(ExecutionOutcome::Failed(_)) => (
                tr!("action_editor_run_history_outcome_failed").into(),
                palette.random,
            ),
            Some(ExecutionOutcome::Cancelled) => ("-".into(), palette.text_muted),
            Some(ExecutionOutcome::Success) | None => ("0".into(), palette.text_primary),
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
                palette,
            ))
            .child(self.render_history_link_cell(
                "actions-runs-history-link",
                tr!("action_stat_runs_today"),
                runs,
                palette.brand,
                palette,
                cx,
            ))
            .child(self.render_stat_cell(
                tr!("action_stat_avg_time"),
                avg,
                palette.success,
                palette,
            ))
            .child(self.render_history_link_cell(
                "actions-execution-history-link",
                tr!("action_stat_execution"),
                exec_value,
                exec_color,
                palette,
                cx,
            ))
            .child(self.render_history_link_cell(
                "actions-errors-history-link",
                tr!("action_stat_errors_7d"),
                errors.to_string(),
                error_color,
                palette,
                cx,
            ))
            .into_any_element()
    }

    fn render_stat_cell(
        &self,
        label: impl Into<SharedString>,
        value: impl Into<SharedString>,
        value_color: Rgba,
        palette: &ForgePalette,
    ) -> AnyElement {
        div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(STAT_VALUE_GAP)
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(label.into()),
            )
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_SM)
                    .text_color(value_color)
                    .child(value.into()),
            )
            .into_any_element()
    }

    fn render_history_link_cell(
        &self,
        id: &'static str,
        label: impl Into<SharedString>,
        value: impl Into<SharedString>,
        value_color: Rgba,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_start()
            .gap(STAT_VALUE_GAP)
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(label.into()),
            )
            .child(
                div()
                    .id(id)
                    .cursor_pointer()
                    .font_family(mono_family())
                    .text_size(FONT_SM)
                    .text_color(value_color)
                    .underline()
                    .text_decoration_1()
                    .text_decoration_color(palette.border_input)
                    .child(value.into())
                    .on_click(
                        cx.listener(|this, _: &ClickEvent, _, cx| this.open_history_modal(cx)),
                    ),
            )
            .into_any_element()
    }

    fn render_triggers_section(
        &self,
        detail: &ActionDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = div()
            .font_family(mono_family())
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
        let mut condition = descriptor
            .map(|d| d.condition_display(&instance.overrides))
            .unwrap_or_default();
        condition.push_str(&crate::triggers_screen::cooldown_suffix(
            instance.cooldown_secs,
            instance.cooldown_global,
        ));
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
                    .font_family(body_family())
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(FONT_XS)
                    .text_color(name_color)
                    .child(instance.name.clone()),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(kind_label),
            )
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.bits)
                    .child(condition),
            );

        let instance_id = instance.id;
        let unlink = trigger_unlink_btn(
            SharedString::from(format!("actions-trigger-unlink-{instance_id}")),
            palette,
            cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.unlink_trigger(instance_id, cx)
            }),
        );

        row_card(title, palette)
            .leading(leading)
            .trailing(unlink)
            .trailing_reveal(SharedString::from(format!(
                "actions-trigger-row-{instance_id}"
            )))
            .idle_background(palette.elevated)
            .bordered(palette.border_regular, BORDER_THIN, radius(Radius::Md))
            .on_click(
                SharedString::from(format!("actions-trigger-open-{instance_id}")),
                cx.listener(move |_this, _: &ClickEvent, _, cx| {
                    cx.emit(NavRequested(Screen::Triggers(Some(instance_id))))
                }),
            )
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
                    .font_family(mono_family())
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
                        .font_family(mono_family())
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

    fn render_health_dot(
        &self,
        health: &analyzer::StepHealth,
        i: usize,
        palette: &ForgePalette,
    ) -> AnyElement {
        let severity = health.severity();
        let color = match severity {
            analyzer::HealthSeverity::Green => palette.success,
            analyzer::HealthSeverity::Yellow => palette.warning,
            analyzer::HealthSeverity::Red => palette.random,
        };
        let tile = div()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .size(STEP_HEALTH_TILE)
            .rounded(px(5.0))
            .bg(with_alpha(color, STEP_HEALTH_TILE_ALPHA))
            .child(icon(Icon::Heartbeat, STEP_HEALTH_GLYPH, color));
        let mut lines: Vec<SharedString> = vec![match severity {
            analyzer::HealthSeverity::Green => tr!("action_editor_health_ok").into(),
            analyzer::HealthSeverity::Yellow => tr!("action_editor_health_warn").into(),
            analyzer::HealthSeverity::Red => tr!("action_editor_health_error").into(),
        }];
        lines.extend(health.findings.iter().map(analyzer_finding_message));
        div()
            .id(SharedString::from(format!("actions-step-health-{i}")))
            .flex_none()
            .child(tile)
            .tooltip(tooltip_lines_builder(lines, palette))
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
        let (glyph, glyph_color) = step_glyph(
            &step.kind_id,
            runner.map(|r| r.icon_name()).unwrap_or(fallback_icon),
            runner.map(|r| sub_category_color(r.category(), palette)),
            palette,
        );
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
                    .font_family(mono_family())
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
            .font_family(body_family())
            .font_weight(FontWeight::SEMIBOLD)
            .text_size(FONT_XS)
            .text_color(palette.text_primary)
            .child(title);
        let health_dot = if depth == 0 {
            self.step_health
                .get(i)
                .map(|health| self.render_health_dot(health, i, palette))
        } else {
            None
        };
        let title_el = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(title_text)
            .children(health_dot);

        let enabled = step.enabled;
        let mut card = row_card(title_el, palette)
            .leading(icon(glyph, CARD_GLYPH, glyph_color))
            .meta(variable_text(&detail_str, palette))
            .trailing(self.render_step_controls(i, total, enabled, palette, cx))
            .idle_background(palette.elevated)
            .bordered(palette.border_regular, BORDER_THIN, radius(Radius::Md))
            .padding_xy(STEP_CARD_PAD_V, STEP_CARD_PAD_H)
            .on_click(
                SharedString::from(format!("actions-step-card-{i}")),
                cx.listener(move |this, _: &ClickEvent, _, cx| this.open_edit_sub_action(i, cx)),
            );
        if self.step_menu_open != Some(i) {
            card = card.trailing_reveal(SharedString::from(format!("actions-step-row-{i}")));
        }

        let step_row = div()
            .flex()
            .items_start()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(left_col)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .when(!enabled, |el| el.opacity(STEP_DISABLED_OPACITY))
                    .child(card),
            );

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
        enabled: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let menu_open = self.step_menu_open == Some(i);
        let menu_pos = if menu_open { self.menu_click_pos } else { None };
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
            .open_at(menu_pos)
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
                    SharedString::from(format!("actions-step-enabled-{i}")),
                    if enabled {
                        tr!("actions_step_disable")
                    } else {
                        tr!("actions_step_enable")
                    },
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.set_step_enabled(i, !enabled, cx)
                    }),
                )
                .icon(if enabled { Icon::EyeOff } else { Icon::Eye })
                .into(),
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
                cx.listener(move |this, ev: &ClickEvent, _, cx| {
                    this.toggle_step_menu(i, ev.position(), cx)
                }),
            )
            .on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_step_menu(cx));
            });

        div()
            .id(SharedString::from(format!("actions-step-controls-{i}")))
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(move_up)
            .child(move_down)
            .child(menu)
            .into_any_element()
    }
}
