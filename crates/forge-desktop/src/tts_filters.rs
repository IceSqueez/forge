use std::sync::Arc;

use forge_components::{
    BORDER_THIN, ConfirmTone, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_SM, FONT_XS,
    ForgePalette, Icon, InputEvent, OverlayPosition, Radius, Spacing, TextArea, TextInput, badge,
    card, confirm_modal, icon, overlay, primary_button, radius, secondary_button, spacing, toggle,
    with_alpha,
};
use forge_speak_queue::{
    PipelineConfigHandle, Priority, RequestId, SpeakCommand, SpeakQueueHandle, SpeakRequest,
    build_config_lenient, build_config_strict,
};
use forge_storage::{
    BlocklistMode, FilterRule, FilterRuleKind, TtsFiltersRepo, TtsPipelineSettings, UrlMode,
};
use forge_tts_pipeline::{PipelineResult, StageAction, StageOutcome};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, Pixels, Rgba, SharedString, Subscription, Window,
    div, prelude::*, px,
};

use crate::presentation::ActivePresentation;

/// Speaker name attached to a one-off preview utterance enqueued from this screen.
const PREVIEW_SPEAKER: &str = "Preview";

/// The three URL-handling modes in banner order.
const URL_MODES: [UrlMode; 3] = [UrlMode::Speak, UrlMode::Replace, UrlMode::Suppress];

/// Numbered stage-badge side — the parity source pins the pill at a fixed 20px square,
/// off the `Spacing` scale, so it is carried as a named literal.
const BADGE_SIZE: Pixels = px(20.0);
/// Rule-row caption size (kind badge + summary line) — the source pins both at a fixed
/// 8.5px mono, below `FONT_XXS`.
const MICRO_FS: Pixels = px(8.5);
/// Preview column width — the source pins the right column at a fixed 300px.
const PREVIEW_W: Pixels = px(300.0);

/// Selectable rule kind in the draft editor, decoupled from the parameter-carrying
/// [`FilterRuleKind`] so the picker can be chosen before the parameters exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DraftKind {
    Literal,
    Regex,
    Blocklist,
}

impl DraftKind {
    fn of(kind: &FilterRuleKind) -> Self {
        match kind {
            FilterRuleKind::Literal { .. } => DraftKind::Literal,
            FilterRuleKind::Regex { .. } => DraftKind::Regex,
            FilterRuleKind::Blocklist { .. } => DraftKind::Blocklist,
        }
    }

    fn label(self) -> &'static str {
        match self {
            DraftKind::Literal => "Text",
            DraftKind::Regex => "Regex",
            DraftKind::Blocklist => "Blocklist",
        }
    }

    fn key(self) -> &'static str {
        match self {
            DraftKind::Literal => "text",
            DraftKind::Regex => "regex",
            DraftKind::Blocklist => "blocklist",
        }
    }
}

/// The open add/edit form. `editing` is the index into the working rule list when
/// editing an existing rule, `None` when adding. The text fields are child
/// [`TextInput`] entities so they own their own edit state; only the rendered subset
/// (per [`DraftKind`]) is read on submit.
struct RuleDraft {
    editing: Option<usize>,
    kind: DraftKind,
    name: Entity<TextInput>,
    pattern: Entity<TextInput>,
    replacement: Entity<TextInput>,
    words: Entity<TextInput>,
    blocklist_mode: BlocklistMode,
}

/// The computed live preview: the final pipeline result plus each stage's outcome,
/// recomputed on every edit via `forge-tts-pipeline::preview`.
struct CachedPreview {
    stages: Vec<StageOutcome>,
    result: PipelineResult,
}

/// The TTS Filters section view-entity: a two-column layout — a scrollable message-
/// preprocessing pipeline (numbered stage cards, an inline rule editor and a save
/// bar) on the left, and a fixed-width live-preview column on the right.
///
/// The rule roster and settings are pulled from the TTS-filters store on mount and
/// after Save (write-through then dirty-clear, never a local patch). Save validates
/// the whole set with `build_config_strict`, persists rules + settings through the
/// repo, then hot-swaps the live speak-queue pipeline config. The preview pane is
/// computed on every edit via `forge-tts-pipeline::preview`; the speak-preview button
/// enqueues a `SpeakRequest` through the speak-queue handle.
pub struct TtsFiltersView {
    repo: Arc<dyn TtsFiltersRepo>,
    /// The live pipeline config, hot-swapped on Save. `None` only when the speak
    /// subsystem didn't build, in which case persistence still happens and only the
    /// hot-swap is skipped.
    pipeline_config: Option<PipelineConfigHandle>,
    /// The live speak-queue handle, driving the speak-preview button. `None` skips
    /// only the preview enqueue.
    speak: Option<SpeakQueueHandle>,
    rt_handle: tokio::runtime::Handle,
    rules: Vec<FilterRule>,
    settings: TtsPipelineSettings,
    max_length: Entity<TextInput>,
    draft: Option<RuleDraft>,
    /// The strict-validation error surfaced by the last Save attempt; blocks persist.
    save_error: Option<String>,
    dirty: bool,
    /// Two-phase delete gate: the index armed by a row's delete button, resolved by
    /// the confirm overlay. `None` = no confirm showing.
    pending_delete: Option<usize>,
    preview_input: Entity<TextArea>,
    cached_preview: Option<CachedPreview>,
    _max_length_sub: Subscription,
    _preview_sub: Subscription,
}

impl TtsFiltersView {
    pub fn new(
        repo: Arc<dyn TtsFiltersRepo>,
        pipeline_config: Option<PipelineConfigHandle>,
        speak: Option<SpeakQueueHandle>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let settings = TtsPipelineSettings::default();

        let seed_len = settings
            .max_length
            .map(|n| n.to_string())
            .unwrap_or_default();
        let max_length = cx.new(|cx| {
            let mut input = TextInput::new("e.g. 300", cx).with_palette(palette);
            if !seed_len.is_empty() {
                input.set_content(seed_len, cx);
            }
            input
        });
        let max_length_sub = cx.subscribe(&max_length, |this, _input, event: &InputEvent, cx| {
            if let InputEvent::Changed(s) = event {
                this.settings.max_length = s.trim().parse::<u32>().ok();
                this.dirty = true;
                this.refresh_preview(cx);
                cx.notify();
            }
        });

        let preview_input = cx.new(|cx| {
            let mut input = TextArea::new("Type a test message…", cx).with_palette(palette);
            input.set_content("hey @koval check this out https://example.com POGGERS", cx);
            input
        });
        let preview_sub = cx.subscribe(&preview_input, |this, _input, event: &InputEvent, cx| {
            if let InputEvent::Changed(_) = event {
                this.refresh_preview(cx);
                cx.notify();
            }
        });

        let mut view = Self {
            repo,
            pipeline_config,
            speak,
            rt_handle,
            rules: Vec::new(),
            settings,
            max_length,
            draft: None,
            save_error: None,
            dirty: false,
            pending_delete: None,
            preview_input,
            cached_preview: None,
            _max_length_sub: max_length_sub,
            _preview_sub: preview_sub,
        };
        view.refresh_preview(cx);
        view.reload(cx);
        view
    }

    // --- async pull + reconcile -------------------------------------------

    /// Pulls the full rule set and pipeline settings off the store and reconciles the
    /// cached state. Runs on mount; Save re-pulls nothing (it already holds the set).
    fn reload(&self, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.repo);
        let (tx, rx) = tokio::sync::oneshot::channel::<
            Result<(Vec<FilterRule>, TtsPipelineSettings), String>,
        >();
        self.rt_handle.spawn(async move {
            let outcome = async {
                let rules = repo.list_rules().await.map_err(|e| e.to_string())?;
                let settings = repo
                    .get_pipeline_settings()
                    .await
                    .map_err(|e| e.to_string())?;
                Ok((rules, settings))
            }
            .await;
            let _ = tx.send(outcome);
        });
        cx.spawn(async move |this, cx| match rx.await {
            Ok(Ok((rules, settings))) => {
                let _ = this.update(cx, |this, cx| this.apply_loaded(rules, settings, cx));
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
        mut rules: Vec<FilterRule>,
        settings: TtsPipelineSettings,
        cx: &mut Context<Self>,
    ) {
        rules.sort_by_key(|r| r.position);
        self.rules = rules;
        self.renumber();
        let seed = settings
            .max_length
            .map(|n| n.to_string())
            .unwrap_or_default();
        self.max_length
            .update(cx, |input, cx| input.set_content(seed, cx));
        self.settings = settings;
        self.dirty = false;
        self.save_error = None;
        self.refresh_preview(cx);
        cx.notify();
    }

    fn on_repo_error(&mut self, message: &str, cx: &mut Context<Self>) {
        eprintln!("forge-desktop: tts filters operation failed: {message}");
        cx.notify();
    }

    /// Recomputes the live preview from the current rules + settings and the preview
    /// input. Empty input clears the preview. Lenient config build drops invalid regex
    /// rules rather than failing, mirroring the boot posture.
    fn refresh_preview(&mut self, cx: &mut Context<Self>) {
        let input = self.preview_input.read(cx).content().to_owned();
        if input.is_empty() {
            self.cached_preview = None;
            return;
        }
        let config = build_config_lenient(&self.rules, &self.settings);
        let (result, stages) = forge_tts_pipeline::preview(&input, &config);
        self.cached_preview = Some(CachedPreview { stages, result });
    }

    // --- pure in-memory logic (rule mutations) ----------------------------

    fn renumber(&mut self) {
        for (i, rule) in self.rules.iter_mut().enumerate() {
            rule.position = i as u32;
        }
    }

    fn open_add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let draft = self.build_draft(None, DraftKind::Literal, "", "", "", "", cx);
        draft.name.read(cx).focus(window);
        self.draft = Some(draft);
        cx.notify();
    }

    fn open_edit(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(rule) = self.rules.get(index) else {
            return;
        };
        let kind = DraftKind::of(&rule.kind);
        let name = rule.name.clone();
        let (pattern, replacement, words) = match &rule.kind {
            FilterRuleKind::Literal {
                pattern,
                replacement,
            }
            | FilterRuleKind::Regex {
                pattern,
                replacement,
            } => (pattern.clone(), replacement.clone(), String::new()),
            FilterRuleKind::Blocklist { words, .. } => {
                (String::new(), String::new(), words.join(", "))
            }
        };
        let mode = match &rule.kind {
            FilterRuleKind::Blocklist { mode, .. } => *mode,
            _ => BlocklistMode::Censor,
        };
        let mut draft =
            self.build_draft(Some(index), kind, &name, &pattern, &replacement, &words, cx);
        draft.blocklist_mode = mode;
        draft.name.read(cx).focus(window);
        self.draft = Some(draft);
        cx.notify();
    }

    fn set_draft_kind(&mut self, kind: DraftKind, cx: &mut Context<Self>) {
        if let Some(draft) = self.draft.as_mut() {
            draft.kind = kind;
        }
        cx.notify();
    }

    fn set_draft_blocklist_mode(&mut self, mode: BlocklistMode, cx: &mut Context<Self>) {
        if let Some(draft) = self.draft.as_mut() {
            draft.blocklist_mode = mode;
        }
        cx.notify();
    }

    fn cancel_draft(&mut self, cx: &mut Context<Self>) {
        self.draft = None;
        cx.notify();
    }

    /// Commits the open draft into the cached roster: editing replaces the target row,
    /// adding appends a new one. Reads the field entities for the current kind only.
    fn submit_draft(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = self.draft.as_ref() else {
            return;
        };
        let name = draft.name.read(cx).content().trim().to_owned();
        let kind = match draft.kind {
            DraftKind::Literal => FilterRuleKind::Literal {
                pattern: draft.pattern.read(cx).content().trim().to_owned(),
                replacement: draft.replacement.read(cx).content().trim().to_owned(),
            },
            DraftKind::Regex => FilterRuleKind::Regex {
                pattern: draft.pattern.read(cx).content().trim().to_owned(),
                replacement: draft.replacement.read(cx).content().trim().to_owned(),
            },
            DraftKind::Blocklist => FilterRuleKind::Blocklist {
                words: draft
                    .words
                    .read(cx)
                    .content()
                    .split(',')
                    .map(str::trim)
                    .filter(|w| !w.is_empty())
                    .map(str::to_owned)
                    .collect(),
                mode: draft.blocklist_mode,
            },
        };
        let editing = draft.editing;

        match editing {
            Some(i) if i < self.rules.len() => {
                self.rules[i].name = name;
                self.rules[i].kind = kind;
            }
            _ => {
                let position = self.rules.len() as u32;
                self.rules.push(FilterRule {
                    id: ulid::Ulid::r#gen().to_string(),
                    name,
                    enabled: true,
                    position,
                    kind,
                });
            }
        }
        self.renumber();
        self.draft = None;
        self.dirty = true;
        self.refresh_preview(cx);
        cx.notify();
    }

    fn toggle_rule(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(rule) = self.rules.get_mut(index) {
            rule.enabled = !rule.enabled;
            self.dirty = true;
            self.refresh_preview(cx);
            cx.notify();
        }
    }

    fn move_up(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(j) = same_kind_prev_index(&self.rules, index) {
            self.rules.swap(index, j);
            self.renumber();
            self.dirty = true;
            self.refresh_preview(cx);
            cx.notify();
        }
    }

    fn move_down(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(j) = same_kind_next_index(&self.rules, index) {
            self.rules.swap(index, j);
            self.renumber();
            self.dirty = true;
            self.refresh_preview(cx);
            cx.notify();
        }
    }

    fn request_delete(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.rules.len() {
            self.pending_delete = Some(index);
            cx.notify();
        }
    }

    fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        self.pending_delete = None;
        cx.notify();
    }

    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        if let Some(index) = self.pending_delete.take()
            && index < self.rules.len()
        {
            self.rules.remove(index);
            self.renumber();
            self.dirty = true;
            self.refresh_preview(cx);
        }
        cx.notify();
    }

    fn set_url_mode(&mut self, mode: UrlMode, cx: &mut Context<Self>) {
        self.settings.url_mode = mode;
        self.dirty = true;
        self.refresh_preview(cx);
        cx.notify();
    }

    fn toggle_strip_twitch(&mut self, cx: &mut Context<Self>) {
        self.settings.strip_twitch_emotes = !self.settings.strip_twitch_emotes;
        self.dirty = true;
        self.refresh_preview(cx);
        cx.notify();
    }

    fn toggle_strip_reward(&mut self, cx: &mut Context<Self>) {
        self.settings.strip_reward_emotes = !self.settings.strip_reward_emotes;
        self.dirty = true;
        self.refresh_preview(cx);
        cx.notify();
    }

    fn set_settings_blocklist_mode(&mut self, mode: BlocklistMode, cx: &mut Context<Self>) {
        self.settings.blocklist_mode = mode;
        self.dirty = true;
        self.refresh_preview(cx);
        cx.notify();
    }

    /// Validates the whole set with `build_config_strict` (surfacing the offending
    /// pattern on error without touching storage), then persists rules + settings
    /// through the repo and hot-swaps the live speak-queue pipeline config.
    fn save(&mut self, cx: &mut Context<Self>) {
        let config = match build_config_strict(&self.rules, &self.settings) {
            Ok(config) => config,
            Err(e) => {
                self.save_error = Some(e.to_string());
                cx.notify();
                return;
            }
        };
        self.save_error = None;
        cx.notify();

        let repo = Arc::clone(&self.repo);
        let pipeline_config = self.pipeline_config.clone();
        let rules = self.rules.clone();
        let settings = self.settings.clone();
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<(), String>>();
        self.rt_handle.spawn(async move {
            let outcome = async {
                repo.replace_rules(&rules)
                    .await
                    .map_err(|e| e.to_string())?;
                repo.set_pipeline_settings(&settings)
                    .await
                    .map_err(|e| e.to_string())?;
                if let Some(handle) = pipeline_config {
                    handle.swap(config);
                }
                Ok(())
            }
            .await;
            let _ = tx.send(outcome);
        });
        cx.spawn(async move |this, cx| match rx.await {
            Ok(Ok(())) => {
                let _ = this.update(cx, |this, cx| {
                    this.dirty = false;
                    cx.notify();
                });
            }
            Ok(Err(message)) => {
                let _ = this.update(cx, |this, cx| {
                    this.save_error = Some(message);
                    cx.notify();
                });
            }
            Err(_) => {}
        })
        .detach();
    }

    /// Enqueues a one-off preview utterance for the current preview input through the
    /// speak queue. Empty input or a missing queue handle drops the request.
    fn speak_preview(&self, cx: &mut Context<Self>) {
        let text = self.preview_input.read(cx).content().trim().to_owned();
        if text.is_empty() {
            return;
        }
        let Some(handle) = self.speak.clone() else {
            return;
        };
        self.rt_handle.spawn(async move {
            let request = SpeakRequest {
                request_id: RequestId::new(),
                viewer_id: String::new(),
                viewer_name: PREVIEW_SPEAKER.to_owned(),
                text,
                priority: Priority::Normal,
                alias_override: None,
                engine_override: None,
                voice_override: None,
                source_event_id: forge_types::EventId::new(),
                is_reward: false,
            };
            if let Err(e) = handle.send(SpeakCommand::Enqueue(request)).await {
                eprintln!("forge-desktop: filter preview speak failed: {e}");
            }
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn build_draft(
        &self,
        editing: Option<usize>,
        kind: DraftKind,
        name: &str,
        pattern: &str,
        replacement: &str,
        words: &str,
        cx: &mut Context<Self>,
    ) -> RuleDraft {
        let palette = cx.palette();
        RuleDraft {
            editing,
            kind,
            name: draft_field("Rule name (optional)", name, palette, cx),
            pattern: draft_field("Pattern", pattern, palette, cx),
            replacement: draft_field("Replacement", replacement, palette, cx),
            words: draft_field("word1, word2, …", words, palette, cx),
            blocklist_mode: BlocklistMode::Censor,
        }
    }

    // --- pipeline column --------------------------------------------------

    fn pipeline_column(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let gap_md = spacing(Spacing::Sm, density);

        let header = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(mono_caption("PIPELINE", palette))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_muted)
                    .child("Message preprocessing runs top to bottom."),
            );

        let mut col = div()
            .flex()
            .flex_col()
            .gap(gap_md)
            .p(spacing(Spacing::Md, density))
            .child(header)
            .child(self.emote_url_card(palette, density, cx))
            .child(self.replacements_card(palette, density, cx))
            .child(self.blocklist_card(palette, density, cx));
        if self.draft.is_some() {
            col = col.child(self.draft_card(palette, density, cx));
        }
        col = col
            .child(self.output_card(palette, density))
            .child(self.save_bar(palette, density, cx));

        div()
            .id("filt-pipeline")
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .overflow_y_scroll()
            .child(col)
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn stage_card(
        &self,
        n: u8,
        glyph: Icon,
        color: Rgba,
        title: &'static str,
        add: bool,
        body: AnyElement,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let badge = div()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .size(BADGE_SIZE)
            .rounded(radius(Radius::Pill))
            .bg(color)
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.shell)
                    .child(n.to_string()),
            );

        let mut header = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(badge)
            .child(icon(glyph, FONT_SM, color))
            .child(
                div()
                    .flex_1()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(title),
            );
        if add {
            header = header.child(
                div()
                    .id(SharedString::from(format!("filt-add-{n}")))
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xxs, density))
                    .py(spacing(Spacing::Xxs, density))
                    .px(spacing(Spacing::Xs, density))
                    .cursor_pointer()
                    .on_click(
                        cx.listener(|this, _: &ClickEvent, window, cx| this.open_add(window, cx)),
                    )
                    .child(icon(Icon::Plus, FONT_XS, palette.brand))
                    .child(
                        div()
                            .font_family(DEFAULT_BODY_FAMILY)
                            .text_size(FONT_XS)
                            .text_color(palette.brand)
                            .child("Add"),
                    ),
            );
        }

        card(
            div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xs, density))
                .child(header)
                .child(body),
            palette,
        )
        .padding_xy(spacing(Spacing::Sm, density), spacing(Spacing::Sm, density))
        .full_width()
        .into_any_element()
    }

    fn emote_url_card(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut url_seg = div().flex().flex_row().gap(spacing(Spacing::Xxs, density));
        for mode in URL_MODES {
            let active = self.settings.url_mode == mode;
            url_seg = url_seg.child(seg_button(
                SharedString::from(format!("filt-url-{}", url_key(mode))),
                url_label(mode),
                active,
                palette.brand,
                palette,
                density,
                cx.listener(move |this, _: &ClickEvent, _, cx| this.set_url_mode(mode, cx)),
            ));
        }

        let url_block = labeled("URLS", url_seg.into_any_element(), palette, density);
        let twitch = self.toggle_row(
            "Strip Twitch emotes",
            self.settings.strip_twitch_emotes,
            "filt-strip-twitch",
            palette,
            density,
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_strip_twitch(cx)),
        );
        let reward = self.toggle_row(
            "Strip reward emotes",
            self.settings.strip_reward_emotes,
            "filt-strip-reward",
            palette,
            density,
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_strip_reward(cx)),
        );

        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(url_block)
            .child(twitch)
            .child(reward)
            .into_any_element();

        self.stage_card(
            1,
            Icon::Globe,
            palette.accent_teal,
            "Emotes & URLs",
            false,
            body,
            palette,
            density,
            cx,
        )
    }

    fn replacements_card(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let body = self.rules_body(is_replacement_kind, palette, density, cx);
        self.stage_card(
            2,
            Icon::Repeat,
            palette.info,
            "Text replacements",
            true,
            body,
            palette,
            density,
            cx,
        )
    }

    fn blocklist_card(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rules_body = self.rules_body(is_blocklist_kind, palette, density, cx);
        let mode_row = labeled(
            "DEFAULT ACTION",
            self.blocklist_mode_toggle(
                self.settings.blocklist_mode,
                ModeTarget::Settings,
                "filt-settings-mode",
                palette,
                density,
                cx,
            ),
            palette,
            density,
        );
        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(rules_body)
            .child(mode_row)
            .into_any_element();
        self.stage_card(
            3,
            Icon::AlertTriangle,
            palette.warning,
            "Word blocklist",
            true,
            body,
            palette,
            density,
            cx,
        )
    }

    fn output_card(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let body = labeled(
            "MAX LENGTH",
            self.max_length.clone().into_any_element(),
            palette,
            density,
        );
        // The output stage carries no user-extensible list and no add affordance, so
        // it is built without the shared listener path.
        let badge = div()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .size(BADGE_SIZE)
            .rounded(radius(Radius::Pill))
            .bg(palette.success)
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.shell)
                    .child("4"),
            );
        let header = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(badge)
            .child(icon(Icon::Send, FONT_SM, palette.success))
            .child(
                div()
                    .flex_1()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child("Output length"),
            );
        card(
            div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xs, density))
                .child(header)
                .child(body),
            palette,
        )
        .padding_xy(spacing(Spacing::Sm, density), spacing(Spacing::Sm, density))
        .full_width()
        .into_any_element()
    }

    fn rules_body(
        &self,
        keep: fn(&FilterRuleKind) -> bool,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let filtered: Vec<(usize, &FilterRule)> = self
            .rules
            .iter()
            .enumerate()
            .filter(|(_, r)| keep(&r.kind))
            .collect();

        if filtered.is_empty() {
            return div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child("No rules")
                .into_any_element();
        }

        let last = filtered.len() - 1;
        let mut col = div().flex().flex_col().gap(spacing(Spacing::Xs, density));
        for (pos, (index, rule)) in filtered.into_iter().enumerate() {
            col =
                col.child(self.rule_row(index, rule, pos == 0, pos == last, palette, density, cx));
        }
        col.into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn rule_row(
        &self,
        index: usize,
        rule: &FilterRule,
        is_first: bool,
        is_last: bool,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (badge_label, badge_color) = match &rule.kind {
            FilterRuleKind::Literal { .. } => ("TEXT", palette.info),
            FilterRuleKind::Regex { .. } => ("REGEX", palette.brand),
            FilterRuleKind::Blocklist { .. } => ("BLOCK", palette.warning),
        };
        let name_color = if rule.enabled {
            palette.text_primary
        } else {
            palette.text_faint
        };

        let text = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(name_color)
                    .child(display_name(rule)),
            )
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(MICRO_FS)
                    .text_color(palette.text_muted)
                    .child(rule_summary(rule)),
            );

        let controls = div()
            .flex()
            .items_center()
            .child(toggle(rule.enabled, palette).on_click(
                SharedString::from(format!("filt-toggle-{index}")),
                cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_rule(index, cx)),
            ))
            .child(self.row_icon(
                Icon::ArrowUp,
                ("filt-up", index),
                !is_first,
                palette,
                density,
                cx.listener(move |this, _: &ClickEvent, _, cx| this.move_up(index, cx)),
            ))
            .child(self.row_icon(
                Icon::ArrowDown,
                ("filt-down", index),
                !is_last,
                palette,
                density,
                cx.listener(move |this, _: &ClickEvent, _, cx| this.move_down(index, cx)),
            ))
            .child(self.row_icon(
                Icon::Settings,
                ("filt-edit", index),
                true,
                palette,
                density,
                cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.open_edit(index, window, cx)
                }),
            ))
            .child(self.row_icon(
                Icon::X,
                ("filt-del", index),
                true,
                palette,
                density,
                cx.listener(move |this, _: &ClickEvent, _, cx| this.request_delete(index, cx)),
            ));

        card(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xs, density))
                .child(badge(
                    palette.surface_overlay,
                    badge_color,
                    badge_label,
                    true,
                    MICRO_FS,
                ))
                .child(text)
                .child(controls),
            palette,
        )
        .background(palette.shell)
        .radius(Radius::Sm)
        .padding_xy(spacing(Spacing::Xs, density), spacing(Spacing::Xs, density))
        .full_width()
        .into_any_element()
    }

    fn row_icon(
        &self,
        glyph: Icon,
        id: (&'static str, usize),
        enabled: bool,
        palette: &ForgePalette,
        density: Density,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let color = if enabled {
            palette.text_muted
        } else {
            palette.text_faint
        };
        let mut btn = div()
            .id(id)
            .flex()
            .p(spacing(Spacing::Xxs, density))
            .child(icon(glyph, FONT_XS, color));
        if enabled {
            btn = btn.cursor_pointer().on_click(handler);
        }
        btn.into_any_element()
    }

    fn toggle_row(
        &self,
        label: &'static str,
        on: bool,
        id: &'static str,
        palette: &ForgePalette,
        density: Density,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        div()
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
                    .child(label),
            )
            .child(toggle(on, palette).on_click(id, handler))
            .into_any_element()
    }

    fn blocklist_mode_toggle(
        &self,
        current: BlocklistMode,
        target: ModeTarget,
        id_prefix: &'static str,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut seg = div().flex().flex_row();
        for (mode, label) in [
            (BlocklistMode::Censor, "Censor"),
            (BlocklistMode::Suppress, "Suppress"),
        ] {
            let active = current == mode;
            seg = seg.child(seg_button(
                SharedString::from(format!("{id_prefix}-{label}")),
                label,
                active,
                palette.warning,
                palette,
                density,
                cx.listener(move |this, _: &ClickEvent, _, cx| match target {
                    ModeTarget::Settings => this.set_settings_blocklist_mode(mode, cx),
                    ModeTarget::Draft => this.set_draft_blocklist_mode(mode, cx),
                }),
            ));
        }
        card(seg, palette)
            .background(palette.shell)
            .radius(Radius::Sm)
            .padding(spacing(Spacing::Xxs, density))
            .into_any_element()
    }

    fn draft_card(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(draft) = self.draft.as_ref() else {
            return div().into_any_element();
        };

        let mut kind_row = div().flex().flex_row().gap(spacing(Spacing::Xs, density));
        for kind in [DraftKind::Literal, DraftKind::Regex, DraftKind::Blocklist] {
            let active = draft.kind == kind;
            kind_row = kind_row.child(seg_button(
                SharedString::from(format!("filt-draft-kind-{}", kind.key())),
                kind.label(),
                active,
                palette.brand,
                palette,
                density,
                cx.listener(move |this, _: &ClickEvent, _, cx| this.set_draft_kind(kind, cx)),
            ));
        }

        let params: AnyElement = match draft.kind {
            DraftKind::Literal | DraftKind::Regex => div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xs, density))
                .child(draft.pattern.clone())
                .child(draft.replacement.clone())
                .into_any_element(),
            DraftKind::Blocklist => div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xs, density))
                .child(draft.words.clone())
                .child(self.blocklist_mode_toggle(
                    draft.blocklist_mode,
                    ModeTarget::Draft,
                    "filt-draft-mode",
                    palette,
                    density,
                    cx,
                ))
                .into_any_element(),
        };

        let submit_label = if draft.editing.is_some() {
            "Save"
        } else {
            "Add rule"
        };
        let actions = div()
            .flex()
            .flex_row()
            .gap(spacing(Spacing::Xs, density))
            .child(primary_button(submit_label, palette).on_click(
                "filt-draft-submit",
                cx.listener(|this, _: &ClickEvent, _, cx| this.submit_draft(cx)),
            ))
            .child(secondary_button("Cancel", palette).on_click(
                "filt-draft-cancel",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_draft(cx)),
            ));

        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(mono_caption("RULE EDITOR", palette))
            .child(kind_row)
            .child(draft.name.clone())
            .child(params)
            .child(actions);

        card(body, palette)
            .padding_xy(spacing(Spacing::Sm, density), spacing(Spacing::Sm, density))
            .full_width()
            .into_any_element()
    }

    fn save_bar(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (dirty_label, dirty_color) = if self.dirty {
            ("Unsaved changes", palette.warning)
        } else {
            ("Saved", palette.text_muted)
        };

        let save_btn: AnyElement = if self.dirty {
            primary_button("Save", palette)
                .on_click(
                    "filt-save",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.save(cx)),
                )
                .into_any_element()
        } else {
            div()
                .py(spacing(Spacing::Sm, density))
                .px(spacing(Spacing::Md, density))
                .rounded(radius(Radius::Md))
                .border(BORDER_THIN)
                .border_color(palette.border_regular)
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_faint)
                .child("Save")
                .into_any_element()
        };

        let error_box = self.save_error.as_ref().map(|err| {
            div()
                .w_full()
                .py(spacing(Spacing::Xs, density))
                .px(spacing(Spacing::Sm, density))
                .rounded(radius(Radius::Sm))
                .border(BORDER_THIN)
                .border_color(palette.random)
                .bg(with_alpha(palette.random, 0.1))
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.random)
                .child(err.clone())
        });

        let dirty_row = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(
                div()
                    .flex_1()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(dirty_color)
                    .child(dirty_label),
            )
            .child(save_btn);

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .children(error_box)
            .child(dirty_row)
            .into_any_element()
    }

    // --- preview column ---------------------------------------------------

    fn preview_column(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let gap_sm = spacing(Spacing::Xs, density);
        let gap_md = spacing(Spacing::Sm, density);

        let input_block = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(mono_caption("INPUT", palette))
            .child(self.preview_input.clone());

        let stages: AnyElement = if let Some(preview) = &self.cached_preview {
            let mut col = div().flex().flex_col().gap(gap_sm);
            for (i, outcome) in preview.stages.iter().enumerate() {
                col = col.child(preview_stage_card(
                    format!("Stage {}", i + 1),
                    outcome,
                    palette,
                    density,
                ));
            }
            col.into_any_element()
        } else {
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child("Preview will appear here.")
                .into_any_element()
        };

        let spoken = self
            .cached_preview
            .as_ref()
            .map(|p| matches!(p.result, PipelineResult::Speak(_)))
            .unwrap_or(false);
        let output_text: String = match self.cached_preview.as_ref().map(|p| &p.result) {
            Some(PipelineResult::Speak(s)) => s.clone(),
            Some(PipelineResult::Skip { .. }) => "[message would be skipped]".to_owned(),
            None => "\u{2014}".to_owned(),
        };
        let output_border = if spoken {
            palette.success
        } else {
            palette.border_regular
        };
        let output_block = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(mono_caption("OUTPUT", palette))
            .child(
                div()
                    .w_full()
                    .py(spacing(Spacing::Xs, density))
                    .px(spacing(Spacing::Sm, density))
                    .rounded(radius(Radius::Sm))
                    .border(BORDER_THIN)
                    .border_color(output_border)
                    .bg(palette.elevated)
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(output_text),
            );

        let speak_btn = div()
            .id("filt-speak-preview")
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .gap(spacing(Spacing::Xxs, density))
            .py(spacing(Spacing::Xs, density))
            .rounded(radius(Radius::Sm))
            .bg(palette.brand)
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.speak_preview(cx)))
            .child(icon(Icon::PlayerPlay, FONT_SM, palette.shell))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.shell)
                    .child("Speak preview"),
            );

        let tip = card(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_muted)
                .child("Preview shows each stage's effect on a sample message."),
            palette,
        )
        .radius(Radius::Sm)
        .padding_xy(spacing(Spacing::Xs, density), spacing(Spacing::Xs, density))
        .full_width();

        let inner = div()
            .flex()
            .flex_col()
            .gap(gap_md)
            .child(mono_caption("PREVIEW", palette))
            .child(input_block)
            .child(stages)
            .child(output_block)
            .child(speak_btn)
            .child(tip);

        div()
            .w(PREVIEW_W)
            .flex_none()
            .h_full()
            .bg(palette.shell)
            .flex()
            .flex_col()
            .child(
                div()
                    .id("filt-preview")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .p(spacing(Spacing::Md, density))
                    .child(inner),
            )
            .into_any_element()
    }

    // --- overlay ----------------------------------------------------------

    fn delete_confirm(
        &self,
        index: usize,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = self.rules.get(index).map(display_name).unwrap_or_default();

        let card = confirm_modal(
            "Delete rule?",
            "This rule will be removed from the preprocessing pipeline.",
            ConfirmTone::Destructive,
            palette,
        )
        .item_name(name)
        .esc_hint("to cancel")
        .on_cancel(
            "filt-delete-cancel",
            "Cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_delete(cx)),
        )
        .on_confirm(
            "filt-delete-confirm",
            "Delete",
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_delete(cx)),
        );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("filt-delete-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_delete(cx));
            })
            .into_any_element()
    }
}

impl Render for TtsFiltersView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let pipeline = self.pipeline_column(&palette, density, cx);
        let preview = self.preview_column(&palette, density, cx);
        let overlay = self
            .pending_delete
            .map(|index| self.delete_confirm(index, &palette, cx));

        div()
            .size_full()
            .flex()
            .flex_row()
            .bg(palette.base)
            .child(pipeline)
            .child(preview)
            .children(overlay)
    }
}

// ── view-specific fragments ───────────────────────────────────────────────

/// Which blocklist-mode field a segment click targets, letting one segmented-toggle
/// helper serve both the settings default and the draft form.
#[derive(Clone, Copy)]
enum ModeTarget {
    Settings,
    Draft,
}

/// An uppercase mono section caption inking `text_muted`.
fn mono_caption(label: &'static str, palette: &ForgePalette) -> impl IntoElement {
    div()
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(FONT_XS)
        .text_color(palette.text_muted)
        .child(label)
}

/// A form control block: an uppercase mono caption over `control`.
fn labeled(
    label: &'static str,
    control: AnyElement,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xxs, density))
        .child(mono_caption(label, palette))
        .child(control)
        .into_any_element()
}

/// A selectable segment: `active_bg`-filled with shell ink when active, otherwise a
/// transparent, secondary-inked pill. Shared by the URL mode, the kind picker and the
/// blocklist-mode toggles (the active fill hue varies per call).
#[allow(clippy::too_many_arguments)]
fn seg_button(
    id: SharedString,
    label: &'static str,
    active: bool,
    active_bg: Rgba,
    palette: &ForgePalette,
    density: Density,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let fg = if active {
        palette.shell
    } else {
        palette.text_secondary
    };
    let mut chip = div()
        .id(id)
        .py(spacing(Spacing::Xxs, density))
        .px(spacing(Spacing::Xs, density))
        .rounded(radius(Radius::Sm))
        .cursor_pointer()
        .font_family(DEFAULT_BODY_FAMILY)
        .text_size(FONT_XS)
        .text_color(fg)
        .on_click(handler)
        .child(label);
    if active {
        chip = chip.bg(active_bg);
    } else {
        let hover = with_alpha(palette.border_regular, 0.06);
        chip = chip.hover(move |s| s.bg(hover));
    }
    chip
}

/// Builds a draft text-field entity seeded with `initial` and adopting `palette`.
fn draft_field(
    placeholder: &'static str,
    initial: &str,
    palette: ForgePalette,
    cx: &mut Context<TtsFiltersView>,
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

fn preview_stage_card(
    label: String,
    outcome: &StageOutcome,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let body: AnyElement = match &outcome.action {
        StageAction::PassedThrough => div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.success)
                    .child("\u{2713}"),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child("pass"),
            )
            .into_any_element(),
        StageAction::Transformed => div()
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(FONT_SM)
            .text_color(palette.text_primary)
            .child(outcome.output.clone())
            .into_any_element(),
        StageAction::Skipped { reason } => div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.random)
                    .child("\u{d7}"),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(format!("skipped — {reason:?}")),
            )
            .into_any_element(),
    };

    card(
        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(label),
            )
            .child(body),
        palette,
    )
    .radius(Radius::Sm)
    .padding_xy(spacing(Spacing::Xs, density), spacing(Spacing::Sm, density))
    .full_width()
    .into_any_element()
}

/// Label for a URL-handling mode in the segmented picker.
fn url_label(mode: UrlMode) -> &'static str {
    match mode {
        UrlMode::Speak => "Speak",
        UrlMode::Replace => "Replace",
        UrlMode::Suppress => "Suppress",
    }
}

/// Stable element-id fragment for a URL-handling mode.
fn url_key(mode: UrlMode) -> &'static str {
    match mode {
        UrlMode::Speak => "speak",
        UrlMode::Replace => "replace",
        UrlMode::Suppress => "suppress",
    }
}

// ── formatting + reorder helpers ──────────────────────────────────────────

fn display_name(rule: &FilterRule) -> String {
    if rule.name.trim().is_empty() {
        DraftKind::of(&rule.kind).label().to_owned()
    } else {
        rule.name.clone()
    }
}

fn rule_summary(rule: &FilterRule) -> String {
    match &rule.kind {
        FilterRuleKind::Literal {
            pattern,
            replacement,
        }
        | FilterRuleKind::Regex {
            pattern,
            replacement,
        } => format!("{pattern} → {replacement}"),
        FilterRuleKind::Blocklist { words, .. } => words.join(", "),
    }
}

fn is_replacement_kind(kind: &FilterRuleKind) -> bool {
    matches!(
        kind,
        FilterRuleKind::Literal { .. } | FilterRuleKind::Regex { .. }
    )
}

fn is_blocklist_kind(kind: &FilterRuleKind) -> bool {
    matches!(kind, FilterRuleKind::Blocklist { .. })
}

/// Nearest preceding rule of the same [`DraftKind`] as `rules[i]`. Reorder arrows
/// operate within a rule's own kind group (the stage cards split the flat list per
/// kind), not the raw array — an adjacent rule of a different kind is not the target.
fn same_kind_prev_index(rules: &[FilterRule], i: usize) -> Option<usize> {
    let kind = DraftKind::of(&rules.get(i)?.kind);
    rules[..i]
        .iter()
        .enumerate()
        .rev()
        .find(|(_, r)| DraftKind::of(&r.kind) == kind)
        .map(|(j, _)| j)
}

/// Symmetric counterpart to [`same_kind_prev_index`] for "move down".
fn same_kind_next_index(rules: &[FilterRule], i: usize) -> Option<usize> {
    let kind = DraftKind::of(&rules.get(i)?.kind);
    rules
        .get(i + 1..)?
        .iter()
        .enumerate()
        .find(|(_, r)| DraftKind::of(&r.kind) == kind)
        .map(|(j, _)| i + 1 + j)
}
