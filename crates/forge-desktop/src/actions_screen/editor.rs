//! Actions screen — editor detail pane: header, the sub-action step chain and
//! step controls, the edit-sub-action side sheet, the triggers section, the
//! unified add grid picker and the trigger-unlink confirm.

use super::*;
use crate::presentation::ActivePresentation;
use forge_components::{
    BORDER_THIN, ConfirmTone, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_LG, FONT_SM,
    FONT_XS, FONT_XXS, ForgePalette, GridPicker, GridPickerConfig, GridPickerEvent,
    GridPickerGroup, GridPickerItem, GridPickerItemState, GridPickerSubtitle, Icon, MenuPlacement,
    OverlayPosition, Radius, SheetPosition, Spacing, TextInput, confirm_modal,
    ghost_button_with_icon, icon, menu_button, menu_divider, menu_item, overlay, primary_button,
    radius, row_card, secondary_button, side_sheet, spacing, status_dot,
};
use gpui::{
    AnyElement, App, ClickEvent, Context, ElementId, Entity, FontWeight, Rgba, SharedString,
    Window, div, px,
};
use std::collections::{BTreeMap, HashMap};

fn platform_group_for(kind_id: &str) -> PlatformGroup {
    if kind_id.starts_with("twitch.") {
        PlatformGroup::Twitch
    } else if kind_id.starts_with("youtube.") {
        PlatformGroup::YouTube
    } else if kind_id.starts_with("kick.") {
        PlatformGroup::Kick
    } else if kind_id.starts_with("obs.") {
        PlatformGroup::Obs
    } else if kind_id.starts_with("vtube.") {
        PlatformGroup::VTube
    } else if kind_id.starts_with("midi.") {
        PlatformGroup::Midi
    } else if kind_id.starts_with("hotkey.") {
        PlatformGroup::Hotkey
    } else if kind_id.starts_with("discord.") {
        PlatformGroup::Discord
    } else if kind_id.starts_with("script.") {
        PlatformGroup::Script
    } else {
        PlatformGroup::Core
    }
}

/// The second `kind_id` segment title-cased into a subgroup label, mirroring the
/// registry's grouping (`obs.scenes.current_changed` → "Scenes").
fn sub_group_label_for(kind_id: &str) -> String {
    let Some(segment) = kind_id.split('.').nth(1) else {
        return "Other".to_owned();
    };
    segment
        .split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The representative catalog seeded before a trigger registry is wired: a few
/// platforms, each carrying one or two kinds with a default and an occasional custom
/// instance, so every grid group and both the default / saved cards render populated.
fn seed_picker_entries() -> Vec<PickerEntry> {
    let counter = std::cell::Cell::new(0u64);
    let id = || {
        let v = counter.get();
        counter.set(v + 1);
        v
    };
    let entry = |kind_id: &'static str,
                 label: &'static str,
                 desc: &'static str,
                 customs: Vec<PickerCustom>| PickerEntry {
        kind_id,
        label,
        desc,
        sub_group: sub_group_label_for(kind_id),
        default_id: id(),
        customs,
    };
    vec![
        entry(
            "twitch.chat.command",
            "Chat command",
            "Fires on a !command in chat",
            vec![PickerCustom {
                id: id(),
                name: "!hello",
                override_summary: "command=!hello",
                enabled: true,
            }],
        ),
        entry(
            "twitch.support.subscriber",
            "New subscriber",
            "A new paid subscription",
            vec![
                PickerCustom {
                    id: id(),
                    name: "VIP sub alert",
                    override_summary: "tier=3000",
                    enabled: true,
                },
                PickerCustom {
                    id: id(),
                    name: "Gift-bomb alert",
                    override_summary: "min gifts=5",
                    enabled: false,
                },
            ],
        ),
        entry(
            "twitch.points.reward",
            "Channel point reward",
            "A channel-point reward redeemed",
            Vec::new(),
        ),
        entry(
            "youtube.chat.message",
            "Chat message",
            "Every message posted in chat",
            Vec::new(),
        ),
        entry(
            "youtube.support.member",
            "New member",
            "A new channel membership",
            Vec::new(),
        ),
        entry(
            "kick.chat.command",
            "Chat command",
            "Fires on a !command in chat",
            Vec::new(),
        ),
        entry(
            "obs.scenes.current_changed",
            "Scene changed",
            "Active scene switched",
            Vec::new(),
        ),
        entry(
            "obs.stream.started",
            "Stream started",
            "OBS started streaming",
            Vec::new(),
        ),
        entry(
            "core.timer.tick",
            "Timer tick",
            "Every N minutes while live",
            Vec::new(),
        ),
    ]
}

/// The seeded sub-action catalog as grid groups (one per [`SubCategory`] in first-seen
/// order) paired with the pick each card id applies.
fn build_step_groups(
    palette: &ForgePalette,
) -> (Vec<GridPickerGroup>, HashMap<SharedString, GridPick>) {
    let mut groups: Vec<GridPickerGroup> = Vec::new();
    let mut picks: HashMap<SharedString, GridPick> = HashMap::new();
    for kind in SUB_KINDS {
        let cat = kind.category();
        let scope = SharedString::from(cat.slug());
        let color = cat.color(palette);
        let id = SharedString::from(format!("step-{}", kind.slug()));
        picks.insert(id.clone(), GridPick::Step(kind));
        let item = GridPickerItem {
            id,
            icon: kind.glyph(),
            icon_color: color,
            name: kind.label().into(),
            desc: kind.summary_hint().into(),
            state: GridPickerItemState::Normal,
        };
        match groups.iter_mut().find(|g| g.scope == scope) {
            Some(g) => g.items.push(item),
            None => groups.push(GridPickerGroup {
                label: cat.label().into(),
                dot_color: color,
                scope,
                items: vec![item],
            }),
        }
    }
    (groups, picks)
}

/// The seeded trigger catalog as grid groups: a leading "Your saved triggers" group from
/// the custom instances (cards flagged `Added` when already linked, `Disabled` when the
/// custom is off), then one group per platform · subgroup of default kinds — paired with
/// the pick each card id applies.
fn build_trigger_groups(
    entries: &[PickerEntry],
    detail: Option<&ActionDetail>,
    palette: &ForgePalette,
) -> (Vec<GridPickerGroup>, HashMap<SharedString, GridPick>) {
    let linked: Vec<&str> = detail
        .map(|d| d.triggers.iter().map(|t| t.name.as_str()).collect())
        .unwrap_or_default();

    let mut groups: Vec<GridPickerGroup> = Vec::new();
    let mut picks: HashMap<SharedString, GridPick> = HashMap::new();

    let mut saved: Vec<GridPickerItem> = Vec::new();
    for entry in entries {
        let group = platform_group_for(entry.kind_id);
        for custom in &entry.customs {
            let added = linked.contains(&custom.name);
            let state = if !custom.enabled {
                GridPickerItemState::Disabled
            } else if added {
                GridPickerItemState::Added
            } else {
                GridPickerItemState::Normal
            };
            let id = SharedString::from(format!("trig-custom-{}", custom.id));
            picks.insert(
                id.clone(),
                GridPick::Trigger(TriggerSeed {
                    name: custom.name.to_owned(),
                    kind_label: entry.label.to_owned(),
                    condition: custom.override_summary.to_owned(),
                    glyph: group.glyph(),
                    enabled: true,
                }),
            );
            saved.push(GridPickerItem {
                id,
                icon: group.glyph(),
                icon_color: group.color(palette),
                name: custom.name.into(),
                desc: custom.override_summary.into(),
                state,
            });
        }
    }
    if !saved.is_empty() {
        groups.push(GridPickerGroup {
            label: "Your saved triggers".into(),
            dot_color: palette.bits,
            scope: SharedString::from("all"),
            items: saved,
        });
    }

    for entry in entries {
        let group = platform_group_for(entry.kind_id);
        let scope = SharedString::from(group.key());
        let label = format!("{} \u{b7} {}", group.label(), entry.sub_group);
        let id = SharedString::from(format!("trig-default-{}", entry.default_id));
        picks.insert(
            id.clone(),
            GridPick::Trigger(TriggerSeed {
                name: entry.label.to_owned(),
                kind_label: group.label().to_owned(),
                condition: String::new(),
                glyph: group.glyph(),
                enabled: true,
            }),
        );
        let item = GridPickerItem {
            id,
            icon: group.glyph(),
            icon_color: group.color(palette),
            name: entry.label.into(),
            desc: entry.desc.into(),
            state: GridPickerItemState::Normal,
        };
        match groups
            .iter_mut()
            .find(|g| g.label.as_ref() == label.as_str())
        {
            Some(g) => g.items.push(item),
            None => groups.push(GridPickerGroup {
                label: label.into(),
                dot_color: group.color(palette),
                scope,
                items: vec![item],
            }),
        }
    }

    (groups, picks)
}

/// Builds the config inputs for `kind`, each seeded from `seed` (an existing step's
/// config when editing, or the kind's defaults when adding).
fn build_sub_fields(
    kind: SubKind,
    seed: &BTreeMap<String, String>,
    palette: ForgePalette,
    cx: &mut Context<ScreenActionsView>,
) -> Vec<(&'static SubField, Entity<TextInput>)> {
    kind.fields()
        .iter()
        .map(|spec| {
            let value = seed.get(spec.key).cloned().unwrap_or_default();
            let placeholder = spec.placeholder;
            let input = cx.new(|cx| {
                let mut input = TextInput::new(placeholder, cx).with_palette(palette);
                if !value.is_empty() {
                    input.set_content(value, cx);
                }
                input
            });
            (spec, input)
        })
        .collect()
}

/// Splits `s` into `(chunk, is_variable)` runs, marking `%name%` interpolation tokens
/// (leading letter/underscore, then alphanumerics/`_`/`.`) so the caller can two-tone
/// them.
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

/// Renders a summary line with `%variable%` tokens tinted `warning` and plain text
/// tinted `text_muted`, wrapping like the source's flowed mono row.
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

/// Full-width, centered "Add …" button closing a section (triggers / sub-actions):
/// the deep-panel fill, an accent icon + label and a thin hairline, washing
/// `surface_overlay` on hover.
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

impl ScreenActionsView {
    // --- editor: step interaction handlers --------------------------------

    /// Copies `id`/`n` out of the loaded detail before touching the tree so the
    /// summary borrow ends before the mutable group iteration begins. The tree badge
    /// tracks the action's *top-level* chain length, so a nested edit leaves it
    /// unchanged.
    fn sync_selected_count(&mut self) {
        let Some((id, n)) = self.detail.as_ref().map(|d| (d.action_id, d.steps.len())) else {
            return;
        };
        for group in &mut self.groups {
            if let Some(action) = group.actions.iter_mut().find(|a| a.id == id) {
                action.sub_action_count = n;
            }
        }
    }

    /// The chain the step list currently renders — the action's top-level steps at
    /// root, or the nested sub-chain [`Self::nav_path`] descends into. Falls back to
    /// an empty slice when the path no longer resolves (never panics).
    pub(super) fn current_chain(&self) -> &[EditorStep] {
        match &self.detail {
            Some(detail) => resolve_chain(&detail.steps, &self.nav_path).unwrap_or(&[]),
            None => &[],
        }
    }

    /// Mutable handle to the current chain. Clones the (small, `Copy`-framed) nav path
    /// so the detail can be borrowed mutably alongside it.
    pub(super) fn current_chain_mut(&mut self) -> Option<&mut Vec<EditorStep>> {
        let path = self.nav_path.clone();
        resolve_chain_mut(&mut self.detail.as_mut()?.steps, &path)
    }

    fn move_step(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        if let Some(chain) = self.current_chain_mut()
            && from < chain.len()
            && to < chain.len()
            && from != to
        {
            let step = chain.remove(from);
            chain.insert(to, step);
        }
        self.step_menu_open = None;
        self.sync_case_fields(cx);
        cx.notify();
    }

    fn move_step_up(&mut self, i: usize, cx: &mut Context<Self>) {
        if i > 0 {
            self.move_step(i, i - 1, cx);
        }
    }

    fn move_step_down(&mut self, i: usize, cx: &mut Context<Self>) {
        self.move_step(i, i + 1, cx);
    }

    fn move_step_top(&mut self, i: usize, cx: &mut Context<Self>) {
        self.move_step(i, 0, cx);
    }

    fn move_step_bottom(&mut self, i: usize, cx: &mut Context<Self>) {
        let last = self.current_chain().len();
        if last > 0 {
            self.move_step(i, last - 1, cx);
        }
    }

    fn duplicate_step(&mut self, i: usize, cx: &mut Context<Self>) {
        if let Some(chain) = self.current_chain_mut()
            && let Some(src) = chain.get(i)
        {
            let clone = src.clone();
            chain.insert(i + 1, clone);
        }
        self.sync_selected_count();
        self.step_menu_open = None;
        self.sync_case_fields(cx);
        cx.notify();
    }

    fn remove_step(&mut self, i: usize, cx: &mut Context<Self>) {
        if let Some(chain) = self.current_chain_mut()
            && i < chain.len()
        {
            chain.remove(i);
        }
        self.sync_selected_count();
        self.step_menu_open = None;
        self.sync_case_fields(cx);
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

    /// Local, persistence-free re-run affordance: the runtime engine is not yet wired
    /// into `forge-desktop`, so Test-run only repaints.
    fn test_run(&mut self, cx: &mut Context<Self>) {
        cx.notify();
    }

    // --- editor: edit-sub-action side sheet -------------------------------

    fn open_edit_sub_action(&mut self, i: usize, cx: &mut Context<Self>) {
        let palette = cx.palette();
        let Some((kind, seed)) = self
            .current_chain()
            .get(i)
            .map(|step| (step.kind, step.config.clone()))
        else {
            return;
        };
        let fields = build_sub_fields(kind, &seed, palette, cx);
        self.step_menu_open = None;
        self.sub_form = Some(EditSubActionForm {
            kind,
            fields,
            index: i,
        });
        cx.notify();
    }

    fn cancel_sub_action(&mut self, cx: &mut Context<Self>) {
        self.sub_form = None;
        cx.notify();
    }

    fn submit_sub_action(&mut self, cx: &mut Context<Self>) {
        let (kind, index, fields) = {
            let Some(form) = self.sub_form.as_ref() else {
                return;
            };
            (form.kind, form.index, form.fields.clone())
        };

        let mut config = BTreeMap::new();
        for (spec, input) in &fields {
            config.insert(spec.key.to_owned(), input.read(cx).content().to_owned());
        }

        // Editing keeps the step's nested branches / cases intact — only its kind +
        // scalar config are re-authored from the form.
        if let Some(chain) = self.current_chain_mut()
            && let Some(step) = chain.get_mut(index)
        {
            step.kind = kind;
            step.config = config;
        }
        self.sync_selected_count();
        self.sync_case_fields(cx);
        self.sub_form = None;
        cx.notify();
    }

    // --- editor: unified "Add" grid picker --------------------------------

    fn open_grid_picker(&mut self, kind: PickerKind, window: &mut Window, cx: &mut Context<Self>) {
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
            .map(|d| d.name.clone())
            .unwrap_or_else(|| "this action".to_owned());
        let (groups, picks, config) = match kind {
            PickerKind::Step => {
                let (groups, picks) = build_step_groups(&palette);
                let count = SUB_KINDS.len();
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
                (groups, picks, config)
            }
            PickerKind::Trigger => {
                let entries = seed_picker_entries();
                let (groups, picks) =
                    build_trigger_groups(&entries, self.detail.as_ref(), &palette);
                let count = entries.len();
                let config = GridPickerConfig {
                    accent: palette.warning,
                    header_icon: Icon::Bolt,
                    title: "Add trigger".into(),
                    subtitle: GridPickerSubtitle::Context {
                        lead: "Fires".into(),
                        name: ctx_name.into(),
                        note: format!("\u{b7} {count} trigger types").into(),
                    },
                    footer_hint: "Pick a trigger \u{2014} configure it in the Triggers registry"
                        .into(),
                    search_placeholder: "Search triggers\u{2026}".into(),
                    scope_cap: Some(6),
                };
                (groups, picks, config)
            }
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

    /// Routes a [`GridPickerEvent`] from the grid picker: a pick resolves the card id back
    /// to its stored [`GridPick`] and applies it; a dismiss closes the picker.
    fn on_grid_picker_event(
        &mut self,
        _picker: Entity<GridPicker>,
        event: &GridPickerEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            GridPickerEvent::Picked(id) => {
                if let Some(pick) = self
                    .grid_picker
                    .as_ref()
                    .and_then(|f| f.picks.get(id).cloned())
                {
                    self.grid_apply_pick(pick, cx);
                }
            }
            GridPickerEvent::Dismissed => self.cancel_grid_picker(cx),
        }
    }

    pub(super) fn cancel_grid_picker(&mut self, cx: &mut Context<Self>) {
        self.grid_picker = None;
        cx.notify();
    }

    fn grid_pick_step(&mut self, kind: SubKind, cx: &mut Context<Self>) {
        if let Some(chain) = self.current_chain_mut() {
            chain.push(EditorStep::new(kind, kind.seed_config()));
        }
        self.sync_selected_count();
        self.sync_case_fields(cx);
        self.grid_picker = None;
        cx.notify();
    }

    /// Links a picked trigger to the open action, guarding on the picker still
    /// targeting the selected action, then closes the picker.
    fn grid_pick_trigger(&mut self, trigger: SeededTrigger, cx: &mut Context<Self>) {
        let same = self
            .grid_picker
            .as_ref()
            .zip(self.detail.as_ref())
            .is_some_and(|(f, d)| f.action_id == d.action_id);
        if same && let Some(detail) = self.detail.as_mut() {
            detail.triggers.push(trigger);
        }
        self.grid_picker = None;
        cx.notify();
    }

    // --- trigger links: unlink --------------------------------------------

    /// Navigate-to-registry intent for a trigger card. The triggers registry screen is
    /// not yet built in `forge-desktop`, so the click is inert.
    fn trigger_card_clicked(&mut self, cx: &mut Context<Self>) {
        cx.notify();
    }

    fn request_trigger_unlink(&mut self, index: usize, cx: &mut Context<Self>) {
        self.pending_trigger_unlink = Some(index);
        cx.notify();
    }

    fn cancel_trigger_unlink(&mut self, cx: &mut Context<Self>) {
        self.pending_trigger_unlink = None;
        cx.notify();
    }

    fn confirm_trigger_unlink(&mut self, cx: &mut Context<Self>) {
        if let Some(i) = self.pending_trigger_unlink.take()
            && let Some(detail) = self.detail.as_mut()
            && i < detail.triggers.len()
        {
            detail.triggers.remove(i);
        }
        cx.notify();
    }

    // --- render: right editor pane ----------------------------------------

    pub(super) fn render_editor_pane(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match (self.selected, self.detail.as_ref()) {
            (Some(sel), Some(detail)) if detail.action_id == sel => {
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
        let (pill_color, pill_label) = if detail.enabled {
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
                    .child(detail.name.clone()),
            )
            .child(pill);

        let desc = detail
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
            .child(format!("TRIGGERS · {}", detail.triggers.len()));

        let hint = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(icon(Icon::InfoCircle, HINT_GLYPH, palette.text_faint))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child("Click a trigger to edit it in the registry"),
            );

        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .child(label)
            .child(hint);

        let mut col = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, Density::Cozy));
        if detail.triggers.is_empty() {
            col = col.child(empty_placeholder_card(
                Icon::Bolt,
                palette.warning,
                "No triggers — this action will never fire on its own",
                palette,
            ));
        } else {
            for (index, trigger) in detail.triggers.iter().enumerate() {
                col = col.child(self.render_trigger_card(index, trigger, palette, cx));
            }
        }
        col = col.child(add_row_button(
            "actions-add-trigger",
            Icon::Plus,
            "Add trigger",
            palette.warning,
            palette,
            cx.listener(|this, _: &ClickEvent, window, cx| {
                this.open_grid_picker(PickerKind::Trigger, window, cx)
            }),
        ));

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(header)
            .child(col)
            .into_any_element()
    }

    fn render_sub_actions_section(
        &self,
        detail: &ActionDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let current = resolve_chain(&detail.steps, &self.nav_path).unwrap_or(&[]);
        let total = current.len();
        let at_root = self.nav_path.is_empty();
        let depth = self.nav_path.len();

        // At root: the mono sub-action count. Drilled in: a breadcrumb of the nav
        // path with the current chain's length pinned to the right edge.
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
                        this.open_grid_picker(PickerKind::Step, window, cx)
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
        step: &EditorStep,
        i: usize,
        total: usize,
        depth: usize,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_last = i + 1 == total;

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
            .child(step.kind.label());

        let card = row_card(title_el, palette)
            .leading(icon(step.kind.glyph(), CARD_GLYPH, palette.text_secondary))
            .meta(variable_text(&step.detail(), palette))
            .trailing(self.render_step_controls(i, total, palette, cx))
            .idle_background(palette.elevated)
            .bordered(palette.border_regular, BORDER_THIN, radius(Radius::Md));

        let step_row = div()
            .flex()
            .items_start()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(left_col)
            .child(div().flex_1().min_w(px(0.0)).child(card));

        // Composite / switch steps carry their branch drill-ins indented under the
        // card body, aligned past the step-circle column.
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
        let mut fields_col = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(form.kind.label()),
            )
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child("CONFIGURATION"),
            );
        if form.fields.is_empty() {
            fields_col = fields_col.child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_muted)
                    .child("This sub-action has no configuration."),
            );
        }
        for (spec, input) in &form.fields {
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
                            .child(spec.label),
                    )
                    .child(input.clone()),
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

    // --- render: trigger-link card + picker + unlink confirm --------------

    /// A trigger-link card: a leading dot + kind glyph, the name / kind / condition
    /// title cluster, and a trailing unlink `X` that arms the two-phase confirm. The
    /// card body carries the navigate-to-registry click (inert until that screen
    /// exists); the `X`'s own handler runs first, so a click on it unlinks without the
    /// inert navigate interfering.
    fn render_trigger_card(
        &self,
        index: usize,
        trigger: &SeededTrigger,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let accent = if trigger.enabled {
            palette.brand
        } else {
            palette.disabled
        };
        let name_color = if trigger.enabled {
            palette.text_primary
        } else {
            palette.text_faint
        };

        let leading = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(status_dot(accent, TRIGGER_DOT))
            .child(icon(trigger.glyph, CARD_GLYPH, accent));

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
                    .child(trigger.name.clone()),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(trigger.kind_label.clone()),
            )
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.bits)
                    .child(trigger.condition.clone()),
            );

        let hover = palette.surface_overlay;
        let unlink = div()
            .id(SharedString::from(format!(
                "actions-trigger-unlink-{index}"
            )))
            .flex()
            .items_center()
            .justify_center()
            .size(STEP_BTN)
            .rounded(STEP_BTN_RADIUS)
            .cursor_pointer()
            .hover(move |s| s.bg(hover))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.request_trigger_unlink(index, cx)
            }))
            .child(icon(Icon::X, CARD_GLYPH, palette.random));

        row_card(title, palette)
            .leading(leading)
            .trailing(unlink)
            .idle_background(palette.elevated)
            .bordered(palette.border_regular, BORDER_THIN, radius(Radius::Md))
            .on_click(
                SharedString::from(format!("actions-trigger-card-{index}")),
                cx.listener(|this, _: &ClickEvent, _, cx| this.trigger_card_clicked(cx)),
            )
            .into_any_element()
    }

    fn grid_apply_pick(&mut self, pick: GridPick, cx: &mut Context<Self>) {
        match pick {
            GridPick::Step(kind) => self.grid_pick_step(kind, cx),
            GridPick::Trigger(seed) => self.grid_pick_trigger(
                SeededTrigger {
                    name: seed.name,
                    kind_label: seed.kind_label,
                    condition: seed.condition,
                    glyph: seed.glyph,
                    enabled: seed.enabled,
                },
                cx,
            ),
        }
    }

    pub(super) fn render_trigger_unlink_confirm(
        &self,
        index: usize,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = self
            .detail
            .as_ref()
            .and_then(|d| d.triggers.get(index))
            .map(|t| t.name.clone())
            .unwrap_or_default();
        let card = confirm_modal(
            "Delete trigger link?",
            "This item will be permanently removed. This action cannot be undone.",
            ConfirmTone::Destructive,
            palette,
        )
        .item_name(name)
        .esc_hint("to cancel")
        .on_cancel(
            "actions-trigger-unlink-cancel",
            "Cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_trigger_unlink(cx)),
        )
        .on_confirm(
            "actions-trigger-unlink-confirm",
            "Delete",
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_trigger_unlink(cx)),
        );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("actions-trigger-unlink-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_trigger_unlink(cx));
            })
            .into_any_element()
    }
}
