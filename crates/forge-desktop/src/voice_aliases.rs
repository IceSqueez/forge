use std::future::Future;
use std::sync::Arc;

use forge_components::{
    BORDER_THIN, ConfirmTone, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_SM, FONT_XS,
    ForgePalette, Icon, InputEvent, OverlayPosition, Radius, Spacing, TextInput, badge, card,
    confirm_modal, icon, modal, overlay, primary_button, primary_button_with_icon, radius,
    search_input, secondary_button, spacing, toggle, with_alpha,
};
use forge_speak_queue::{Priority, RequestId, SpeakCommand, SpeakQueueHandle, SpeakRequest};
use forge_storage::{AliasId, AssignmentStrategy, VoiceAlias, VoiceAliasRepo};
use forge_voice::{AliasState, EngineId, VoiceId};
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
/// Utterance a row's preview button enqueues to demonstrate the resolved voice.
const PREVIEW_TEXT: &str = "This is a voice preview.";

/// Column grow weights reproducing the source's `1.4fr 1.6fr 0.8fr 0.8fr` table grid;
/// the trailing actions column is a fixed [`ACTIONS_W`].
const VIEWER_GROW: f32 = 1.4;
const VOICE_GROW: f32 = 1.6;
const PITCH_GROW: f32 = 0.8;
const SPEED_GROW: f32 = 0.8;

/// How a voice is chosen for viewers without a manual alias, as the segmented banner's
/// selection. Persisted to the alias store and hot-reloaded into the live speak queue
/// over the queue handle when the banner changes.
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

/// One manual voice alias, a presentation row folded from a stored [`VoiceAlias`].
/// `blocked` viewers are never spoken, so their voice fields are inapplicable.
struct AliasRow {
    id: AliasId,
    viewer_id: String,
    viewer_name: String,
    kind: EngineKind,
    engine_id: String,
    engine_label: String,
    voice_id: String,
    voice_label: String,
    pitch_semitones: Option<f32>,
    rate_multiplier: Option<f32>,
    blocked: bool,
}

/// One selectable engine in the assign/edit form's engine picker. Seeded here; the
/// real list is the registered TTS engine roster reaching this view over the bridge.
struct EngineOption {
    id: &'static str,
    label: &'static str,
}

const ENGINE_OPTIONS: [EngineOption; 4] = [
    EngineOption {
        id: "piper",
        label: "Piper",
    },
    EngineOption {
        id: "espeak-ng",
        label: "eSpeak-NG",
    },
    EngineOption {
        id: "polly",
        label: "Amazon Polly",
    },
    EngineOption {
        id: "elevenlabs",
        label: "ElevenLabs",
    },
];

/// The open assign/edit dialog. `editing` is the id of the alias being edited (or
/// `None` for a fresh assign). The text fields are child [`TextInput`] entities so
/// they own their own edit state; `engine` is the selected engine id.
struct AliasForm {
    editing: Option<AliasId>,
    viewer: Entity<TextInput>,
    voice: Entity<TextInput>,
    pitch: Entity<TextInput>,
    rate: Entity<TextInput>,
    engine: Option<String>,
    blocked: bool,
    /// True while an upsert write is in flight; the modal stays open with Save
    /// disabled until the write resolves and either closes it or clears the flag.
    saving: bool,
    _subs: Vec<Subscription>,
}

/// The TTS Voice Aliases section view-entity: a default-strategy banner, a search +
/// assign toolbar, and a viewer→voice alias table with per-row preview / edit /
/// delete, plus the assign/edit modal and a delete-confirm overlay.
///
/// The roster and the chosen strategy are pulled from the alias store on mount and
/// after every write (write-through then full re-pull, never a local row patch).
/// Assign/edit upserts and delete removes through the store's repo, hot-reloading the
/// live speak queue via `SpeakCommand::{SetAlias, RemoveAlias, SetStrategy}`; per-row
/// preview enqueues a `SpeakRequest` through the same queue handle.
pub struct VoiceAliasesView {
    repo: Arc<dyn VoiceAliasRepo>,
    /// The live speak-queue handle; `None` only if queue construction failed, in which
    /// case hot-reload and preview are skipped (persistence still happens).
    speak: Option<SpeakQueueHandle>,
    rt_handle: tokio::runtime::Handle,
    /// True until the first pull lands, so the table shows a loading caption rather
    /// than the empty caption before any row arrives.
    loading: bool,
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
    pub fn new(
        repo: Arc<dyn VoiceAliasRepo>,
        speak: Option<SpeakQueueHandle>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let search = cx.new(|cx| search_input("Search viewers…", palette, cx));
        let search_sub = cx.subscribe(&search, |_this, _input, event: &InputEvent, cx| {
            // The filter reads the field's live content at render; a keystroke just
            // needs a repaint. Submit/cancel carry no extra behaviour here.
            if let InputEvent::Changed(_) = event {
                cx.notify();
            }
        });

        let view = Self {
            repo,
            speak,
            rt_handle,
            loading: true,
            strategy: StrategyChoice::DeterministicByName,
            aliases: Vec::new(),
            total_count: 0,
            search,
            form: None,
            pending_delete: None,
            _search_sub: search_sub,
        };
        view.reload(cx);
        view
    }

    // --- async pull + reconcile -------------------------------------------

    /// Pulls the full alias set and the assignment strategy off the store and
    /// reconciles the cached roster. Runs on mount; writes re-pull the roster alone.
    fn reload(&self, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.repo);
        let (tx, rx) = tokio::sync::oneshot::channel::<
            Result<(Vec<VoiceAlias>, AssignmentStrategy), String>,
        >();
        self.rt_handle.spawn(async move {
            let outcome = async {
                let aliases = repo.list().await.map_err(|e| e.to_string())?;
                let strategy = repo.get_strategy().await.map_err(|e| e.to_string())?;
                Ok((aliases, strategy))
            }
            .await;
            let _ = tx.send(outcome);
        });
        cx.spawn(async move |this, cx| match rx.await {
            Ok(Ok((aliases, strategy))) => {
                let _ = this.update(cx, |this, cx| this.apply_loaded(aliases, strategy, cx));
            }
            Ok(Err(message)) => {
                let _ = this.update(cx, |this, cx| this.on_repo_error(&message, cx));
            }
            Err(_) => {}
        })
        .detach();
    }

    /// Spawns `work` (a repo verb that ends by returning the fresh `list`) on the
    /// tokio runtime, then folds the resulting roster back on the foreground executor.
    fn spawn_write(
        &self,
        work: impl Future<Output = Result<Vec<VoiceAlias>, String>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let _ = tx.send(work.await);
        });
        cx.spawn(async move |this, cx| match rx.await {
            Ok(Ok(aliases)) => {
                let _ = this.update(cx, |this, cx| this.apply_aliases(aliases, cx));
            }
            Ok(Err(message)) => {
                let _ = this.update(cx, |this, cx| this.on_repo_error(&message, cx));
            }
            Err(_) => {}
        })
        .detach();
    }

    fn apply_loaded(
        &mut self,
        aliases: Vec<VoiceAlias>,
        strategy: AssignmentStrategy,
        cx: &mut Context<Self>,
    ) {
        self.strategy = choice_from_strategy(&strategy);
        self.set_roster(aliases);
        cx.notify();
    }

    fn apply_aliases(&mut self, aliases: Vec<VoiceAlias>, cx: &mut Context<Self>) {
        self.set_roster(aliases);
        cx.notify();
    }

    fn set_roster(&mut self, aliases: Vec<VoiceAlias>) {
        self.total_count = aliases.len();
        self.aliases = aliases.into_iter().map(row_from_alias).collect();
        self.loading = false;
    }

    fn on_repo_error(&mut self, message: &str, cx: &mut Context<Self>) {
        eprintln!("forge-desktop: voice aliases operation failed: {message}");
        self.loading = false;
        cx.notify();
    }

    // --- handlers ---------------------------------------------------------

    /// Persists the assignment strategy and hot-reloads the live speak queue. A
    /// `SingleVoice` pick binds to the first catalog voice; with no engine running the
    /// catalog is empty, so the change is skipped. Persist and hot-reload both run even
    /// if the other errors; a missing queue handle skips only the hot-reload.
    fn set_strategy(&mut self, choice: StrategyChoice, cx: &mut Context<Self>) {
        self.strategy = choice;
        cx.notify();
        let Some(strategy) = self.strategy_to_assignment(choice) else {
            return;
        };
        let repo = Arc::clone(&self.repo);
        let speak = self.speak.clone();
        self.rt_handle.spawn(async move {
            if let Err(e) = repo.set_strategy(&strategy).await {
                eprintln!("forge-desktop: voice strategy persist failed: {e}");
            }
            if let Some(handle) = speak
                && let Err(e) = handle.send(SpeakCommand::SetStrategy(strategy)).await
            {
                eprintln!("forge-desktop: voice strategy hot-reload failed: {e}");
            }
        });
    }

    /// Resolves the banner choice to a domain strategy. `SingleVoice` needs a concrete
    /// voice; absent a dedicated picker it binds to the first live catalog voice, so it
    /// yields `None` when no engine is running.
    fn strategy_to_assignment(&self, choice: StrategyChoice) -> Option<AssignmentStrategy> {
        match choice {
            StrategyChoice::DeterministicByName => Some(AssignmentStrategy::DeterministicByName),
            StrategyChoice::Random => Some(AssignmentStrategy::Random),
            StrategyChoice::SingleVoice => {
                let voices = self.speak.as_ref()?.available_voices();
                let first = voices.first()?;
                Some(AssignmentStrategy::Single {
                    voice_id: first.id.clone(),
                    engine_id: first.engine_id.clone(),
                })
            }
        }
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
        let id = row.id.clone();
        let viewer = row.viewer_name.clone();
        let engine = (!row.blocked).then(|| row.engine_id.clone());
        let voice = row.voice_id.clone();
        let pitch = fmt_field(row.pitch_semitones);
        let rate = fmt_field(row.rate_multiplier);
        let blocked = row.blocked;
        let form = self.build_form(
            Some(id),
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

    /// Upserts the open form through the alias store, hot-reloads the live speak queue
    /// with `SpeakCommand::SetAlias`, then re-pulls the roster and closes the modal. A
    /// blank viewer keeps the form open; a write error clears the saving flag to retry.
    fn save_form(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self.form.as_ref() else {
            return;
        };
        if form.saving {
            return;
        }
        if form.viewer.read(cx).content().trim().is_empty() {
            return;
        }
        let alias = form_to_alias(form, cx);
        if let Some(form) = self.form.as_mut() {
            form.saving = true;
        }
        cx.notify();

        let repo = Arc::clone(&self.repo);
        let speak = self.speak.clone();
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<Vec<VoiceAlias>, String>>();
        self.rt_handle.spawn(async move {
            let outcome = async {
                repo.upsert(&alias).await.map_err(|e| e.to_string())?;
                if let Some(handle) = speak
                    && let Err(e) = handle.send(SpeakCommand::SetAlias(alias)).await
                {
                    eprintln!("forge-desktop: voice alias hot-reload failed: {e}");
                }
                repo.list().await.map_err(|e| e.to_string())
            }
            .await;
            let _ = tx.send(outcome);
        });
        cx.spawn(async move |this, cx| match rx.await {
            Ok(Ok(aliases)) => {
                let _ = this.update(cx, |this, cx| {
                    this.apply_aliases(aliases, cx);
                    this.form = None;
                    cx.notify();
                });
            }
            Ok(Err(message)) => {
                let _ = this.update(cx, |this, cx| {
                    if let Some(form) = this.form.as_mut() {
                        form.saving = false;
                    }
                    this.on_repo_error(&message, cx);
                });
            }
            Err(_) => {}
        })
        .detach();
    }

    /// Enqueues a one-off preview utterance for the alias at `index` through the speak
    /// queue. Blocked aliases never speak, and a missing queue handle drops the request.
    fn preview(&self, index: usize) {
        let Some(row) = self.aliases.get(index) else {
            return;
        };
        if row.blocked {
            return;
        }
        let Some(handle) = self.speak.clone() else {
            return;
        };
        let viewer_id = row.viewer_id.clone();
        let viewer_name = row.viewer_name.clone();
        self.rt_handle.spawn(async move {
            let request = SpeakRequest {
                request_id: RequestId::new(),
                viewer_id,
                viewer_name,
                text: PREVIEW_TEXT.to_owned(),
                priority: Priority::Normal,
                alias_override: None,
                engine_override: None,
                voice_override: None,
                source_event_id: forge_types::EventId::new(),
                is_reward: false,
            };
            if let Err(e) = handle.send(SpeakCommand::Enqueue(request)).await {
                eprintln!("forge-desktop: voice alias preview failed: {e}");
            }
        });
    }

    fn request_delete(&mut self, index: usize, cx: &mut Context<Self>) {
        self.pending_delete = Some(index);
        cx.notify();
    }

    fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        self.pending_delete = None;
        cx.notify();
    }

    /// Deletes the armed alias through the store, hot-reloads the live speak queue with
    /// `SpeakCommand::RemoveAlias`, then re-pulls the roster.
    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.pending_delete.take() else {
            return;
        };
        let Some(row) = self.aliases.get(index) else {
            cx.notify();
            return;
        };
        let id = row.id.clone();
        cx.notify();

        let repo = Arc::clone(&self.repo);
        let speak = self.speak.clone();
        self.spawn_write(
            async move {
                repo.delete(&id).await.map_err(|e| e.to_string())?;
                if let Some(handle) = speak
                    && let Err(e) = handle.send(SpeakCommand::RemoveAlias(id)).await
                {
                    eprintln!("forge-desktop: voice alias hot-reload (remove) failed: {e}");
                }
                repo.list().await.map_err(|e| e.to_string())
            },
            cx,
        );
    }

    /// True while the open form has a non-blank viewer and no write is in flight — the
    /// save gate.
    fn saveable(&self, cx: &Context<Self>) -> bool {
        self.form
            .as_ref()
            .is_some_and(|f| !f.saving && !f.viewer.read(cx).content().trim().is_empty())
    }

    /// Builds an [`AliasForm`], creating and prefilling its field entities and
    /// subscribing to their edits (viewer submit saves; any change repaints so the
    /// save gate re-evaluates; Escape closes).
    #[allow(clippy::too_many_arguments)]
    fn build_form(
        &self,
        editing: Option<AliasId>,
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
            saving: false,
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
            let caption = if self.loading {
                "Loading voice aliases…"
            } else {
                "No voice aliases configured"
            };
            div()
                .w_full()
                .py(spacing(Spacing::Lg, density))
                .px(spacing(Spacing::Sm, density))
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child(caption)
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
        if muted {
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
                .on_click(cx.listener(move |this, _: &ClickEvent, _, _| this.preview(index)));
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

/// Folds a stored [`VoiceAlias`] into a presentation row, deriving the engine's
/// display label and locality from its id.
fn row_from_alias(a: VoiceAlias) -> AliasRow {
    let engine = a.engine_id.0;
    let kind = if is_local_engine(&engine) {
        EngineKind::Local
    } else {
        EngineKind::Cloud
    };
    let engine_label = engine_display_label(&engine);
    let voice = a.voice_id.0;
    AliasRow {
        id: a.id,
        viewer_id: a.viewer_id,
        viewer_name: a.viewer_name,
        kind,
        engine_id: engine,
        engine_label,
        voice_id: voice.clone(),
        voice_label: voice,
        pitch_semitones: a.pitch_semitones,
        rate_multiplier: a.rate_multiplier,
        blocked: matches!(a.state, AliasState::Blocked),
    }
}

/// Builds a [`VoiceAlias`] from the open form. Editing carries the row's id so the
/// upsert targets it; a fresh assign mints a new id. An unparsable pitch/rate is left
/// unset (engine default).
fn form_to_alias(form: &AliasForm, cx: &App) -> VoiceAlias {
    let viewer = form.viewer.read(cx).content().trim().to_owned();
    let engine = form.engine.clone().unwrap_or_default();
    let voice = form.voice.read(cx).content().trim().to_owned();
    let pitch = form.pitch.read(cx).content().trim().parse::<f32>().ok();
    let rate = form.rate.read(cx).content().trim().parse::<f32>().ok();
    VoiceAlias {
        id: form.editing.clone().unwrap_or_default(),
        viewer_id: viewer.clone(),
        viewer_name: viewer,
        engine_id: EngineId(engine.trim().to_owned()),
        voice_id: VoiceId(voice),
        pitch_semitones: pitch,
        rate_multiplier: rate,
        state: if form.blocked {
            AliasState::Blocked
        } else {
            AliasState::Active
        },
    }
}

/// Maps a stored strategy to the banner's selection.
fn choice_from_strategy(strategy: &AssignmentStrategy) -> StrategyChoice {
    match strategy {
        AssignmentStrategy::DeterministicByName => StrategyChoice::DeterministicByName,
        AssignmentStrategy::Random => StrategyChoice::Random,
        AssignmentStrategy::Single { .. } => StrategyChoice::SingleVoice,
    }
}

/// Local engines run on-device with no network round-trip; everything else is a cloud
/// engine.
fn is_local_engine(engine_id: &str) -> bool {
    matches!(
        engine_id,
        "piper" | "espeak" | "espeak-ng" | "sapi" | "avfoundation"
    )
}

/// Maps an engine id to its display label; an unknown id is shown verbatim.
fn engine_display_label(engine_id: &str) -> String {
    match engine_id {
        "piper" => "Piper".to_owned(),
        "espeak" | "espeak-ng" => "eSpeak-NG".to_owned(),
        "sapi" => "SAPI 5".to_owned(),
        "avfoundation" => "AVFoundation".to_owned(),
        other => other.to_owned(),
    }
}

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
