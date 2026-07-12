use forge_components::{
    BORDER_THIN, ConfirmTone, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_SM, FONT_XS,
    ForgePalette, Icon, InputEvent, OverlayPosition, Radius, Spacing, TextInput, badge, card,
    confirm_modal, icon, modal, overlay, primary_button, primary_button_with_icon, radius,
    search_input, secondary_button, spacing, toggle, with_alpha,
};
use gpui::{
    AnyElement, App, ClickEvent, Context, Div, Entity, FontWeight, Pixels, Rgba, SharedString,
    Subscription, Window, div, prelude::*, px, relative,
};

use crate::presentation::ActivePresentation;

/// Toolbar search-field width — the parity source pins it at a fixed 240px, off the
/// `Spacing` scale, so it is carried as a named literal.
const SEARCH_W: Pixels = px(240.0);
/// Assign/edit modal width — the parity source caps its card at a fixed 440px, which
/// no [`forge_components::ModalSize`] step reproduces, so it is pinned exactly.
const MODAL_W: Pixels = px(440.0);
/// Trailing actions column width (the source's fixed 90px mono column).
const ACTIONS_W: Pixels = px(90.0);
/// Viewer avatar tile side (the source's fixed 22px square).
const AVATAR: Pixels = px(22.0);
/// Corner radius of the table's outer top/bottom rounding (the source's fixed 8px).
const TABLE_RADIUS: Pixels = px(8.0);
/// Role-badge caption size — the source pins it at a fixed 8.5px, below `FONT_XXS`.
const ROLE_BADGE_FS: Pixels = px(8.5);
/// Voice-column engine glyph size (the source's fixed 12px icon).
const ENGINE_GLYPH: Pixels = px(12.0);
/// Row action glyph size (preview / edit / delete), matching the source's 13-14px.
const ACTION_GLYPH: Pixels = px(14.0);
/// Total manual aliases the section reports; the seeded rows are a live-loaded slice
/// of this larger set, surfaced in the footer count until a real store reaches here.
const TOTAL_ALIASES: usize = 18;

/// Column grow weights reproducing the source's `1.4fr 1.6fr 0.8fr 0.8fr` table grid;
/// the trailing actions column is a fixed [`ACTIONS_W`].
const VIEWER_GROW: f32 = 1.4;
const VOICE_GROW: f32 = 1.6;
const PITCH_GROW: f32 = 0.8;
const SPEED_GROW: f32 = 0.8;

/// How a voice is chosen for viewers without a manual alias. Mirrors the domain's
/// assignment strategy; here it is a view-state choice, persisted and hot-reloaded
/// into the speak queue over the runtime handle once wired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StrategyChoice {
    DeterministicByName,
    Random,
    SingleVoice,
}

impl StrategyChoice {
    const ALL: [StrategyChoice; 3] = [
        StrategyChoice::DeterministicByName,
        StrategyChoice::Random,
        StrategyChoice::SingleVoice,
    ];

    fn label(self) -> &'static str {
        match self {
            StrategyChoice::DeterministicByName => "Deterministic by name",
            StrategyChoice::Random => "Random",
            StrategyChoice::SingleVoice => "Single voice",
        }
    }

    fn key(self) -> &'static str {
        match self {
            StrategyChoice::DeterministicByName => "deterministic",
            StrategyChoice::Random => "random",
            StrategyChoice::SingleVoice => "single",
        }
    }
}

/// Where an engine runs — drives the voice-column glyph and its hue. `Local` engines
/// run on-device (terminal glyph, ready hue); `Cloud` engines round-trip a service
/// (globe glyph, info hue).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EngineKind {
    Local,
    Cloud,
}

/// A moderator/VIP/subscriber marker shown as a coloured badge beside the viewer.
#[derive(Debug, Clone, Copy)]
enum Role {
    Mod,
    Vip,
    Sub,
}

impl Role {
    fn label(self) -> &'static str {
        match self {
            Role::Mod => "MOD",
            Role::Vip => "VIP",
            Role::Sub => "SUB",
        }
    }

    fn color(self, palette: &ForgePalette) -> Rgba {
        match self {
            Role::Mod => palette.warning,
            Role::Vip => palette.brand,
            Role::Sub => palette.success,
        }
    }
}

/// One manual voice alias. A cached view-model of a stored alias; the live set is
/// read from `forge-voice`'s alias store over the runtime→UI bridge, never owned
/// here. `blocked` viewers are never spoken, so their voice fields are inapplicable.
struct AliasRow {
    viewer_name: String,
    role: Option<Role>,
    kind: EngineKind,
    engine_id: &'static str,
    engine_label: &'static str,
    voice_label: &'static str,
    pitch_semitones: Option<f32>,
    rate_multiplier: Option<f32>,
    blocked: bool,
}

/// One selectable engine in the assign/edit form's engine picker. Seeded here; the
/// real list is the registered TTS engine roster reaching this view over the bridge.
struct EngineOption {
    id: &'static str,
    label: &'static str,
    kind: EngineKind,
}

const ENGINE_OPTIONS: [EngineOption; 4] = [
    EngineOption {
        id: "piper",
        label: "Piper",
        kind: EngineKind::Local,
    },
    EngineOption {
        id: "espeak-ng",
        label: "eSpeak-NG",
        kind: EngineKind::Local,
    },
    EngineOption {
        id: "polly",
        label: "Amazon Polly",
        kind: EngineKind::Cloud,
    },
    EngineOption {
        id: "elevenlabs",
        label: "ElevenLabs",
        kind: EngineKind::Cloud,
    },
];

/// The open assign/edit dialog. `editing` is the index of the row being edited (or
/// `None` for a fresh assign). The text fields are child [`TextInput`] entities so
/// they own their own edit state; `engine` is the selected engine id.
struct AliasForm {
    editing: Option<usize>,
    viewer: Entity<TextInput>,
    voice: Entity<TextInput>,
    pitch: Entity<TextInput>,
    rate: Entity<TextInput>,
    engine: Option<String>,
    blocked: bool,
    _subs: Vec<Subscription>,
}

/// The TTS Voice Aliases section view-entity: a default-strategy banner, a search +
/// assign toolbar, and a viewer→voice alias table with per-row preview / edit /
/// delete, plus the assign/edit modal and a delete-confirm overlay.
///
/// Owns its alias roster and strategy as seeded stub state — `forge-desktop` wires no
/// alias store yet, so the rows and the chosen strategy are seeded representative and
/// the CRUD handlers mutate this cached state. The real screen loads the roster and
/// strategy from `forge-voice`'s alias store over the runtime→UI bridge; assign/edit
/// upserts and delete removes through that store's handle (and hot-reloads the live
/// speak queue via `SpeakCommand::{SetAlias, RemoveAlias, SetStrategy}`); per-row
/// preview enqueues a `SpeakRequest` through the speak-queue dispatch handle.
pub struct VoiceAliasesView {
    strategy: StrategyChoice,
    aliases: Vec<AliasRow>,
    total_count: usize,
    search: Entity<TextInput>,
    form: Option<AliasForm>,
    /// Two-phase delete gate: the index armed by a row's delete button, resolved by
    /// the confirm overlay. `None` = no confirm showing.
    pending_delete: Option<usize>,
    _search_sub: Subscription,
}

impl VoiceAliasesView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let palette = cx.palette();
        let search = cx.new(|cx| search_input("Search viewers…", palette, cx));
        let search_sub = cx.subscribe(&search, |_this, _input, event: &InputEvent, cx| {
            // The filter reads the field's live content at render; a keystroke just
            // needs a repaint. Submit/cancel carry no extra behaviour here.
            if let InputEvent::Changed(_) = event {
                cx.notify();
            }
        });

        Self {
            strategy: StrategyChoice::DeterministicByName,
            aliases: seed_aliases(),
            total_count: TOTAL_ALIASES,
            search,
            form: None,
            pending_delete: None,
            _search_sub: search_sub,
        }
    }

    // --- handlers (view-state stubs) --------------------------------------

    /// Sets the default assignment strategy. Real path: persist through the alias
    /// store and hot-reload the speak queue with `SpeakCommand::SetStrategy`.
    fn set_strategy(&mut self, choice: StrategyChoice, cx: &mut Context<Self>) {
        self.strategy = choice;
        cx.notify();
    }

    /// Opens an empty assign form and focuses the viewer field.
    fn open_assign(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let form = self.build_form(None, "", None, "", "", "", false, cx);
        form.viewer.read(cx).focus(window);
        self.form = Some(form);
        cx.notify();
    }

    /// Opens an edit form prefilled from the row at `index` and focuses the viewer
    /// field. A stale index simply opens nothing.
    fn open_edit(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.aliases.get(index) else {
            return;
        };
        let viewer = row.viewer_name.clone();
        let engine = (!row.blocked).then(|| row.engine_id.to_owned());
        let voice = row.voice_label.to_owned();
        let pitch = fmt_field(row.pitch_semitones);
        let rate = fmt_field(row.rate_multiplier);
        let blocked = row.blocked;
        let form = self.build_form(
            Some(index),
            &viewer,
            engine,
            &voice,
            &pitch,
            &rate,
            blocked,
            cx,
        );
        form.viewer.read(cx).focus(window);
        self.form = Some(form);
        cx.notify();
    }

    fn close_form(&mut self, cx: &mut Context<Self>) {
        self.form = None;
        cx.notify();
    }

    fn set_form_engine(&mut self, id: &'static str, cx: &mut Context<Self>) {
        if let Some(form) = self.form.as_mut() {
            form.engine = Some(id.to_owned());
        }
        cx.notify();
    }

    fn toggle_form_blocked(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = self.form.as_mut() {
            form.blocked = !form.blocked;
        }
        cx.notify();
    }

    /// Commits the open form into the cached roster: edit replaces the target row,
    /// assign appends a new one. A blank viewer keeps the form open. Real path: upsert
    /// through the alias store and hot-reload the speak queue with
    /// `SpeakCommand::SetAlias`.
    fn save_form(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self.form.as_ref() else {
            return;
        };
        let viewer = form.viewer.read(cx).content().trim().to_owned();
        if viewer.is_empty() {
            return;
        }
        let blocked = form.blocked;
        let (engine_id, engine_label, kind) = match form.engine.as_deref() {
            Some(id) => engine_meta(id),
            None => ("", "", EngineKind::Cloud),
        };
        let voice_label = leak(form.voice.read(cx).content().trim());
        let pitch = form.pitch.read(cx).content().trim().parse::<f32>().ok();
        let rate = form.rate.read(cx).content().trim().parse::<f32>().ok();
        let editing = form.editing;

        let row = AliasRow {
            viewer_name: viewer,
            role: editing
                .and_then(|i| self.aliases.get(i))
                .and_then(|r| r.role),
            kind,
            engine_id: engine_label_id(engine_id),
            engine_label: leak(engine_label),
            voice_label,
            pitch_semitones: pitch,
            rate_multiplier: rate,
            blocked,
        };

        match editing {
            Some(i) if i < self.aliases.len() => self.aliases[i] = row,
            _ => {
                self.aliases.push(row);
                self.total_count = self.total_count.saturating_add(1);
            }
        }
        self.form = None;
        cx.notify();
    }

    /// Enqueues a one-off preview utterance for the alias at `index`. Blocked aliases
    /// never speak, so preview is a no-op for them. Real path: enqueue a `SpeakRequest`
    /// through the speak-queue dispatch handle. Here it is a view-state stub.
    fn preview(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.aliases.get(index).is_some_and(|r| r.blocked) {
            return;
        }
        cx.notify();
    }

    fn request_delete(&mut self, index: usize, cx: &mut Context<Self>) {
        self.pending_delete = Some(index);
        cx.notify();
    }

    fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        self.pending_delete = None;
        cx.notify();
    }

    /// Removes the armed alias from the cached roster. Real path: delete through the
    /// alias store and hot-reload the speak queue with `SpeakCommand::RemoveAlias`.
    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        if let Some(index) = self.pending_delete.take()
            && index < self.aliases.len()
        {
            self.aliases.remove(index);
            self.total_count = self.total_count.saturating_sub(1);
        }
        cx.notify();
    }

    /// True while the open form has a non-blank viewer — the save gate.
    fn saveable(&self, cx: &Context<Self>) -> bool {
        self.form
            .as_ref()
            .is_some_and(|f| !f.viewer.read(cx).content().trim().is_empty())
    }

    /// Builds an [`AliasForm`], creating and prefilling its field entities and
    /// subscribing to their edits (viewer submit saves; any change repaints so the
    /// save gate re-evaluates; Escape closes).
    #[allow(clippy::too_many_arguments)]
    fn build_form(
        &self,
        editing: Option<usize>,
        viewer: &str,
        engine: Option<String>,
        voice: &str,
        pitch: &str,
        rate: &str,
        blocked: bool,
        cx: &mut Context<Self>,
    ) -> AliasForm {
        let palette = cx.palette();
        let viewer = text_field("Viewer name", viewer, palette, cx);
        let voice = text_field("Voice id", voice, palette, cx);
        let pitch = text_field("0", pitch, palette, cx);
        let rate = text_field("1.0", rate, palette, cx);

        let mut subs = Vec::new();
        subs.push(cx.subscribe(
            &viewer,
            |this, _input, event: &InputEvent, cx| match event {
                InputEvent::Submitted(_) => this.save_form(cx),
                InputEvent::Changed(_) => cx.notify(),
                InputEvent::Cancelled => this.close_form(cx),
            },
        ));
        for field in [&voice, &pitch, &rate] {
            subs.push(
                cx.subscribe(field, |this, _input, event: &InputEvent, cx| match event {
                    InputEvent::Changed(_) => cx.notify(),
                    InputEvent::Cancelled => this.close_form(cx),
                    InputEvent::Submitted(_) => this.save_form(cx),
                }),
            );
        }

        AliasForm {
            editing,
            viewer,
            voice,
            pitch,
            rate,
            engine,
            blocked,
            _subs: subs,
        }
    }

    // --- strategy banner --------------------------------------------------

    fn strategy_banner(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut segmented = div()
            .flex()
            .flex_row()
            .gap(spacing(Spacing::Xxs, density))
            .p(spacing(Spacing::Xxs, density))
            .rounded(radius(Radius::Sm))
            .bg(palette.shell);
        for choice in StrategyChoice::ALL {
            let active = self.strategy == choice;
            segmented = segmented.child(seg_button(
                SharedString::from(format!("va-strat-{}", choice.key())),
                choice.label(),
                active,
                palette,
                cx.listener(move |this, _: &ClickEvent, _, cx| this.set_strategy(choice, cx)),
            ));
        }

        let row = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(
                div()
                    .flex_1()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child("Default assignment strategy"),
            )
            .child(segmented);

        div()
            .w_full()
            .px(spacing(Spacing::Md, density))
            .pt(spacing(Spacing::Sm, density))
            .pb(spacing(Spacing::Sm, density))
            .child(
                card(row, palette)
                    .padding_xy(spacing(Spacing::Sm, density), spacing(Spacing::Sm, density))
                    .full_width(),
            )
            .into_any_element()
    }

    // --- toolbar ----------------------------------------------------------

    fn toolbar(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let count = div()
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(FONT_SM)
            .text_color(palette.text_muted)
            .child(format!("{} manual aliases", self.total_count));

        let assign = primary_button_with_icon(Icon::Plus, "Assign voice", palette).on_click(
            "va-assign",
            cx.listener(|this, _: &ClickEvent, window, cx| this.open_assign(window, cx)),
        );

        let right = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(count)
            .child(assign);

        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .px(spacing(Spacing::Md, density))
            .pb(spacing(Spacing::Sm, density))
            .child(div().w(SEARCH_W).child(self.search.clone()))
            .child(right)
            .into_any_element()
    }

    // --- table ------------------------------------------------------------

    fn table(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let header = card(header_row(palette), palette)
            .background(palette.shell)
            .split_radius(TABLE_RADIUS, px(0.0))
            .padding_xy(spacing(Spacing::Xs, density), spacing(Spacing::Sm, density))
            .full_width();

        let needle = self.search.read(cx).content().to_ascii_lowercase();
        let visible: Vec<(usize, &AliasRow)> = self
            .aliases
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                needle.is_empty() || a.viewer_name.to_ascii_lowercase().contains(&needle)
            })
            .collect();

        let body: AnyElement = if visible.is_empty() {
            div()
                .w_full()
                .py(spacing(Spacing::Lg, density))
                .px(spacing(Spacing::Sm, density))
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child("No voice aliases configured")
                .into_any_element()
        } else {
            let total = visible.len();
            let mut col = div().w_full().flex().flex_col();
            for (pos, (index, row)) in visible.iter().enumerate() {
                let last = pos + 1 == total;
                col = col.child(self.alias_row(pos, *index, row, last, palette, density, cx));
            }
            col.into_any_element()
        };

        let body_frame = div()
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .rounded_b(TABLE_RADIUS)
            .overflow_hidden()
            .child(
                div()
                    .id("va-table-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .child(body),
            );

        let footer = div()
            .w_full()
            .py(spacing(Spacing::Xs, density))
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XS)
            .text_color(palette.text_faint)
            .child(format!(
                "Showing {} of {} manual aliases",
                visible.len(),
                self.total_count
            ));

        div()
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .px(spacing(Spacing::Md, density))
            .pb(spacing(Spacing::Md, density))
            .child(header)
            .child(body_frame)
            .child(footer)
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn alias_row(
        &self,
        pos: usize,
        index: usize,
        row: &AliasRow,
        last: bool,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = row.blocked;
        let name_color = if muted {
            palette.text_muted
        } else {
            palette.text_primary
        };

        // Viewer column: avatar tile + name + role/blocked badge.
        let initial = row
            .viewer_name
            .chars()
            .next()
            .unwrap_or('?')
            .to_uppercase()
            .next()
            .unwrap_or('?');
        let (avatar_bg, avatar_fg) = if muted {
            (palette.surface_overlay, palette.text_muted)
        } else {
            (avatar_color_for(&row.viewer_name, palette), palette.shell)
        };
        let avatar = div()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .size(AVATAR)
            .rounded(radius(Radius::Sm))
            .bg(avatar_bg)
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(FONT_XS)
                    .text_color(avatar_fg)
                    .child(initial.to_string()),
            );
        let mut viewer_inner = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(avatar)
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_SM)
                    .text_color(name_color)
                    .child(row.viewer_name.clone()),
            );
        if let Some(role) = row.role {
            viewer_inner =
                viewer_inner.child(role_badge(role.label(), role.color(palette), palette));
        } else if muted {
            viewer_inner = viewer_inner.child(role_badge("BLOCKED", palette.random, palette));
        }

        // Voice column: blocked → "Never speak"; else engine glyph + "engine · voice".
        let voice_inner: AnyElement = if muted {
            div()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xxs, density))
                .child(icon(Icon::Volume, ENGINE_GLYPH, palette.random))
                .child(
                    div()
                        .font_family(DEFAULT_MONO_FAMILY)
                        .text_size(FONT_SM)
                        .text_color(palette.random)
                        .child("Never speak"),
                )
                .into_any_element()
        } else {
            div()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xxs, density))
                .child(icon(
                    engine_glyph(row.kind),
                    ENGINE_GLYPH,
                    engine_color(row.kind, palette),
                ))
                .child(
                    div()
                        .font_family(DEFAULT_MONO_FAMILY)
                        .text_size(FONT_SM)
                        .text_color(palette.text_primary)
                        .child(format!("{} · {}", row.engine_label, row.voice_label)),
                )
                .into_any_element()
        };

        let (pitch_color, speed_color) = if muted {
            (palette.surface_overlay, palette.surface_overlay)
        } else {
            (palette.text_muted, palette.text_muted)
        };
        let pitch_cell = mono_cell(fmt_pitch(row.pitch_semitones, muted), pitch_color);
        let speed_cell = mono_cell(fmt_rate(row.rate_multiplier, muted), speed_color);

        // Actions: preview (dim + inert when blocked) · edit · delete.
        let preview_color = if muted {
            palette.surface_overlay
        } else {
            palette.success
        };
        let mut preview = div().id(("va-preview", index)).flex().child(icon(
            Icon::PlayerPlay,
            ACTION_GLYPH,
            preview_color,
        ));
        if !muted {
            preview = preview
                .cursor_pointer()
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.preview(index, cx)));
        }
        let edit = div()
            .id(("va-edit", index))
            .flex()
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.open_edit(index, window, cx)
            }))
            .child(icon(Icon::Pencil, ACTION_GLYPH, palette.text_muted));
        let delete = div()
            .id(("va-delete", index))
            .flex()
            .cursor_pointer()
            .on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| this.request_delete(index, cx)),
            )
            .child(icon(Icon::X, ACTION_GLYPH, palette.text_muted));
        let actions = div()
            .w(ACTIONS_W)
            .flex_none()
            .flex()
            .items_center()
            .justify_end()
            .gap(spacing(Spacing::Sm, density))
            .child(preview)
            .child(edit)
            .child(delete);

        let bg = if pos.is_multiple_of(2) {
            palette.elevated
        } else {
            palette.shell
        };
        let mut root = div()
            .w_full()
            .flex()
            .items_center()
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .bg(bg)
            .child(weighted(VIEWER_GROW, viewer_inner))
            .child(weighted(VOICE_GROW, voice_inner))
            .child(weighted(PITCH_GROW, pitch_cell))
            .child(weighted(SPEED_GROW, speed_cell))
            .child(actions);
        if !last {
            root = root
                .border_b(BORDER_THIN)
                .border_color(palette.border_regular);
        }
        root.into_any_element()
    }

    // --- overlays ---------------------------------------------------------

    /// The active overlay for this frame: the assign/edit modal takes precedence over
    /// the delete confirm, mirroring the source's stack order.
    fn active_overlay(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if let Some(form) = self.form.as_ref() {
            Some(self.form_modal(form, palette, density, cx))
        } else {
            self.pending_delete
                .map(|index| self.delete_confirm(index, palette, cx))
        }
    }

    fn form_modal(
        &self,
        form: &AliasForm,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let title = if form.editing.is_some() {
            "Edit voice alias"
        } else {
            "Assign a voice"
        };

        let viewer_field = form_field("VIEWER", form.viewer.clone(), palette, density);

        let block_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(spacing(Spacing::Xxs, density))
                    .child(
                        div()
                            .font_family(DEFAULT_BODY_FAMILY)
                            .text_size(FONT_SM)
                            .text_color(palette.text_primary)
                            .child("Block from TTS"),
                    )
                    .child(
                        div()
                            .font_family(DEFAULT_BODY_FAMILY)
                            .text_size(FONT_XS)
                            .text_color(palette.text_muted)
                            .child("This viewer's messages are never spoken."),
                    ),
            )
            .child(toggle(form.blocked, palette).on_click(
                "va-form-block",
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_form_blocked(cx)),
            ));

        // A blocked viewer is never spoken, so voice configuration is inapplicable —
        // mirror the row's "Never speak" state instead of dead engine/voice inputs.
        let config: AnyElement = if form.blocked {
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_faint)
                .child("Never speak — voice settings do not apply.")
                .into_any_element()
        } else {
            let mut chips = div().flex().flex_wrap().gap(spacing(Spacing::Xxs, density));
            for opt in &ENGINE_OPTIONS {
                let active = form.engine.as_deref() == Some(opt.id);
                let id = opt.id;
                chips = chips.child(seg_button(
                    SharedString::from(format!("va-form-eng-{id}")),
                    opt.label,
                    active,
                    palette,
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.set_form_engine(id, cx)),
                ));
            }
            let engine_block = labelled("ENGINE", chips, palette, density);
            let voice_block = form_field("VOICE", form.voice.clone(), palette, density);
            let pitch_block = form_field("PITCH (st)", form.pitch.clone(), palette, density);
            let rate_block = form_field("RATE (x)", form.rate.clone(), palette, density);
            div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Sm, density))
                .child(engine_block)
                .child(voice_block)
                .child(
                    div()
                        .flex()
                        .gap(spacing(Spacing::Sm, density))
                        .child(div().flex_1().child(pitch_block))
                        .child(div().flex_1().child(rate_block)),
                )
                .into_any_element()
        };

        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(viewer_field)
            .child(block_row)
            .child(config);

        let save_label = if form.editing.is_some() {
            "Save"
        } else {
            "Create"
        };
        let footer = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(secondary_button("Cancel", palette).on_click(
                "va-form-cancel",
                cx.listener(|this, _: &ClickEvent, _, cx| this.close_form(cx)),
            ))
            .child(
                primary_button(save_label, palette)
                    .disabled(!self.saveable(cx))
                    .on_click(
                        "va-form-save",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.save_form(cx)),
                    ),
            );

        let card = modal(title, body, palette)
            .width(MODAL_W)
            .footer(footer)
            .on_close(
                "va-form-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.close_form(cx)),
            );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("va-form-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.close_form(cx));
            })
            .into_any_element()
    }

    fn delete_confirm(
        &self,
        index: usize,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let viewer = self
            .aliases
            .get(index)
            .map(|a| a.viewer_name.clone())
            .unwrap_or_default();
        let message = format!("{viewer} will fall back to the default voice assignment strategy.");

        let card = confirm_modal(
            "Delete voice alias?",
            message,
            ConfirmTone::Destructive,
            palette,
        )
        .esc_hint("to cancel")
        .on_cancel(
            "va-delete-cancel",
            "Cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_delete(cx)),
        )
        .on_confirm(
            "va-delete-confirm",
            "Delete",
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_delete(cx)),
        );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("va-delete-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_delete(cx));
            })
            .into_any_element()
    }
}

impl Render for VoiceAliasesView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let banner = self.strategy_banner(&palette, density, cx);
        let toolbar = self.toolbar(&palette, density, cx);
        let table = self.table(&palette, density, cx);
        let overlay = self.active_overlay(&palette, density, cx);

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(banner)
            .child(toolbar)
            .child(table)
            .children(overlay)
    }
}

// ── view-specific fragments ───────────────────────────────────────────────

/// A flex table cell that grows proportionally to `grow`, matching the source's
/// `fr`-unit column grid. `flex_basis: 0` makes the grow weights the sole size driver.
fn weighted(grow: f32, child: impl IntoElement) -> Div {
    let mut cell = div().min_w(px(0.0)).child(child);
    let style = cell.style();
    style.flex_grow = Some(grow);
    style.flex_basis = Some(relative(0.0).into());
    cell
}

/// The table header row: four grow-weighted mono captions plus a fixed, right-aligned
/// actions caption.
fn header_row(palette: &ForgePalette) -> impl IntoElement {
    let caption = |text: &'static str| {
        div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XS)
            .text_color(palette.text_muted)
            .child(text)
    };
    div()
        .w_full()
        .flex()
        .items_center()
        .child(weighted(VIEWER_GROW, caption("VIEWER")))
        .child(weighted(VOICE_GROW, caption("VOICE")))
        .child(weighted(PITCH_GROW, caption("PITCH")))
        .child(weighted(SPEED_GROW, caption("SPEED")))
        .child(
            div()
                .w(ACTIONS_W)
                .flex_none()
                .flex()
                .justify_end()
                .child(caption("ACTIONS")),
        )
}

/// One mono value cell (pitch / speed), inking `color`.
fn mono_cell(value: String, color: Rgba) -> impl IntoElement {
    div()
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(FONT_SM)
        .text_color(color)
        .child(value)
}

/// A small pill-shaped role/blocked badge: an uppercase mono caption on a
/// `surface_overlay` tile inking `color`.
fn role_badge(label: &str, color: Rgba, palette: &ForgePalette) -> impl IntoElement {
    badge(
        palette.surface_overlay,
        color,
        label.to_owned(),
        true,
        ROLE_BADGE_FS,
    )
}

/// A selectable segment/chip: brand-filled with shell ink when active, otherwise a
/// transparent, secondary-inked pill. Shared by the strategy banner and the form's
/// engine picker.
fn seg_button(
    id: SharedString,
    label: impl Into<SharedString>,
    active: bool,
    palette: &ForgePalette,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let fg = if active {
        palette.shell
    } else {
        palette.text_secondary
    };
    let weight = if active {
        FontWeight::MEDIUM
    } else {
        FontWeight::NORMAL
    };
    let mut chip = div()
        .id(id)
        .py(spacing(Spacing::Xxs, Density::Cozy))
        .px(spacing(Spacing::Sm, Density::Cozy))
        .rounded(radius(Radius::Sm))
        .cursor_pointer()
        .font_family(DEFAULT_BODY_FAMILY)
        .font_weight(weight)
        .text_size(FONT_XS)
        .text_color(fg)
        .on_click(handler)
        .child(label.into());
    if active {
        chip = chip.bg(palette.brand);
    } else {
        let hover = with_alpha(palette.border_regular, 0.06);
        chip = chip.hover(move |s| s.bg(hover));
    }
    chip
}

/// A form control block: an uppercase mono caption over `control`.
fn labelled(
    label: &'static str,
    control: impl IntoElement,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xxs, density))
        .child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_muted)
                .child(label),
        )
        .child(control)
}

/// A labelled text-input field for the assign/edit form.
fn form_field(
    label: &'static str,
    input: Entity<TextInput>,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement {
    labelled(label, input, palette, density)
}

/// Builds a form text field entity seeded with `initial` and adopting `palette`.
fn text_field(
    placeholder: &'static str,
    initial: &str,
    palette: ForgePalette,
    cx: &mut Context<VoiceAliasesView>,
) -> Entity<TextInput> {
    let initial = initial.to_owned();
    cx.new(|cx| {
        let mut input = TextInput::new(placeholder, cx).with_palette(palette);
        if !initial.is_empty() {
            input.set_content(initial, cx);
        }
        input
    })
}

// ── formatting + resolution helpers ───────────────────────────────────────

/// Formats a pitch value the way the source does: blocked → em dash, else a signed
/// semitone reading (`+2 st` / `-1 st` / `0 st`).
fn fmt_pitch(value: Option<f32>, blocked: bool) -> String {
    if blocked {
        return "—".to_owned();
    }
    match value {
        Some(p) if p >= 0.0 => format!("+{p:.0} st"),
        Some(p) => format!("{p:.0} st"),
        None => "0 st".to_owned(),
    }
}

/// Formats a rate multiplier: blocked → em dash, else a one-decimal `x` reading.
fn fmt_rate(value: Option<f32>, blocked: bool) -> String {
    if blocked {
        return "—".to_owned();
    }
    value
        .map(|r| format!("{r:.1}x"))
        .unwrap_or_else(|| "1.0x".to_owned())
}

/// Renders an optional numeric field back into the plain text a form input prefills
/// with (empty when unset).
fn fmt_field(value: Option<f32>) -> String {
    value.map(|v| format!("{v}")).unwrap_or_default()
}

/// The voice-column glyph for an engine's locality.
fn engine_glyph(kind: EngineKind) -> Icon {
    match kind {
        EngineKind::Local => Icon::Terminal,
        EngineKind::Cloud => Icon::Globe,
    }
}

/// The voice-column glyph hue: local engines the ready hue, cloud engines the info hue.
fn engine_color(kind: EngineKind, palette: &ForgePalette) -> Rgba {
    match kind {
        EngineKind::Local => palette.success,
        EngineKind::Cloud => palette.info,
    }
}

/// Resolves an engine id to its display label and locality, for a row saved from the
/// form's engine picker. Unknown ids fall back to the raw id as a cloud engine.
fn engine_meta(id: &str) -> (&'static str, &'static str, EngineKind) {
    ENGINE_OPTIONS
        .iter()
        .find(|o| o.id == id)
        .map(|o| (o.id, o.label, o.kind))
        .unwrap_or(("", "", EngineKind::Cloud))
}

/// Interns a known engine id to its `'static` form (for the cached row); an unknown id
/// interns as the empty string.
fn engine_label_id(id: &str) -> &'static str {
    ENGINE_OPTIONS
        .iter()
        .find(|o| o.id == id)
        .map(|o| o.id)
        .unwrap_or("")
}

/// The row view-model holds `&'static str` labels (seeded); a value typed into the
/// form must be promoted to `'static` to land in a saved row. The alias set is bounded
/// by the viewer roster, so the one-time leak per saved edit is negligible and never
/// grows unbounded in practice. The real store owns `String`s, so this vanishes once
/// the roster is a bridge-loaded `Vec<String>`-backed model.
fn leak(text: &str) -> &'static str {
    Box::leak(text.to_owned().into_boxed_str())
}

/// Hashes a viewer name to one of the palette's accent hues, so each avatar tile keeps
/// a stable colour across renders (the source's deterministic avatar tint).
fn avatar_color_for(name: &str, palette: &ForgePalette) -> Rgba {
    let hash = name.bytes().fold(0u32, |acc, b| {
        acc.wrapping_mul(31).wrapping_add(u32::from(b))
    });
    let colors = [
        palette.brand,
        palette.success,
        palette.warning,
        palette.info,
        palette.random,
        palette.bits,
    ];
    colors[(hash as usize) % colors.len()]
}

// ── seeded stub state ─────────────────────────────────────────────────────

/// The representative alias roster the section seeds before an alias store is wired,
/// mirroring the design's sample so every role badge, engine locality and the blocked
/// state render populated.
fn seed_aliases() -> Vec<AliasRow> {
    vec![
        AliasRow {
            viewer_name: "haash_".to_owned(),
            role: Some(Role::Mod),
            kind: EngineKind::Local,
            engine_id: "piper",
            engine_label: "Piper",
            voice_label: "UA-1",
            pitch_semitones: Some(2.0),
            rate_multiplier: Some(1.0),
            blocked: false,
        },
        AliasRow {
            viewer_name: "koval_dev".to_owned(),
            role: Some(Role::Vip),
            kind: EngineKind::Cloud,
            engine_id: "elevenlabs",
            engine_label: "ElevenLabs",
            voice_label: "Antoni",
            pitch_semitones: Some(0.0),
            rate_multiplier: Some(1.1),
            blocked: false,
        },
        AliasRow {
            viewer_name: "olena_lv".to_owned(),
            role: None,
            kind: EngineKind::Cloud,
            engine_id: "polly",
            engine_label: "Polly",
            voice_label: "Olena",
            pitch_semitones: Some(0.0),
            rate_multiplier: Some(1.0),
            blocked: false,
        },
        AliasRow {
            viewer_name: "danylo_ua".to_owned(),
            role: Some(Role::Sub),
            kind: EngineKind::Cloud,
            engine_id: "elevenlabs",
            engine_label: "ElevenLabs",
            voice_label: "Rachel",
            pitch_semitones: Some(-1.0),
            rate_multiplier: Some(0.9),
            blocked: false,
        },
        AliasRow {
            viewer_name: "spammer_xyz".to_owned(),
            role: None,
            kind: EngineKind::Cloud,
            engine_id: "",
            engine_label: "",
            voice_label: "",
            pitch_semitones: None,
            rate_multiplier: None,
            blocked: true,
        },
        AliasRow {
            viewer_name: "ostap_pl".to_owned(),
            role: None,
            kind: EngineKind::Cloud,
            engine_id: "polly",
            engine_label: "Polly",
            voice_label: "Maksym",
            pitch_semitones: Some(0.0),
            rate_multiplier: Some(1.2),
            blocked: false,
        },
    ]
}
