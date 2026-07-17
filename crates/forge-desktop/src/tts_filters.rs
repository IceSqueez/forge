use std::sync::Arc;

use forge_components::{
    BORDER_THIN, ConfirmTone, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_SM, FONT_XS,
    ForgePalette, Icon, InputEvent, OverlayPosition, Radius, Spacing, TextArea, TextInput, badge,
    card, confirm_modal, icon, overlay, primary_button, radius, secondary_button, spacing, toggle,
    tr, with_alpha,
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

const URL_MODES: [UrlMode; 3] = [UrlMode::Speak, UrlMode::Replace, UrlMode::Suppress];

const BADGE_SIZE: Pixels = px(20.0);
const MICRO_FS: Pixels = px(8.5);
const PREVIEW_W: Pixels = px(300.0);

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

    fn label(self) -> String {
        match self {
            DraftKind::Literal => tr!("tts_filters_kind_literal"),
            DraftKind::Regex => tr!("tts_filters_kind_regex"),
            DraftKind::Blocklist => tr!("tts_filters_kind_blocklist"),
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

struct RuleDraft {
    editing: Option<usize>,
    kind: DraftKind,
    name: Entity<TextInput>,
    pattern: Entity<TextInput>,
    replacement: Entity<TextInput>,
    words: Entity<TextInput>,
    blocklist_mode: BlocklistMode,
}

struct CachedPreview {
    stages: Vec<StageOutcome>,
    result: PipelineResult,
}

pub struct TtsFiltersView {
    repo: Arc<dyn TtsFiltersRepo>,
    pipeline_config: Option<PipelineConfigHandle>,
    speak: Option<SpeakQueueHandle>,
    rt_handle: tokio::runtime::Handle,
    rules: Vec<FilterRule>,
    settings: TtsPipelineSettings,
    max_length: Entity<TextInput>,
    draft: Option<RuleDraft>,
    save_error: Option<String>,
    dirty: bool,
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
            let mut input =
                TextInput::new(tr!("tts_filters_length_placeholder"), cx).with_palette(palette);
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
            let mut input = TextArea::new(tr!("tts_filters_preview_input_placeholder"), cx)
                .with_palette(palette);
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

    fn renumber(&mut self) {
        for (i, rule) in self.rules.iter_mut().enumerate() {
            rule.position = i as u32;
        }
    }

    fn open_add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let draft = self.build_draft(None, DraftKind::Literal, "", "", "", "", cx);
        draft.name.update(cx, |f, cx| f.focus(window, cx));
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
        draft.name.update(cx, |f, cx| f.focus(window, cx));
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
                    id: ulid::Ulid::generate().to_string(),
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

    fn speak_preview(&self, cx: &mut Context<Self>) {
        let text = self.preview_input.read(cx).content().trim().to_owned();
        if text.is_empty() {
            return;
        }
        let Some(handle) = self.speak.clone() else {
            return;
        };
        let speaker_name = tr!("tts_filters_preview_speaker_name");
        self.rt_handle.spawn(async move {
            let request = SpeakRequest {
                request_id: RequestId::new(),
                viewer_id: String::new(),
                viewer_name: speaker_name,
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
            name: draft_field(tr!("tts_filters_draft_name_placeholder"), name, palette, cx),
            pattern: draft_field(
                tr!("tts_filters_draft_pattern_placeholder"),
                pattern,
                palette,
                cx,
            ),
            replacement: draft_field(
                tr!("tts_filters_draft_replacement_placeholder"),
                replacement,
                palette,
                cx,
            ),
            words: draft_field(
                tr!("tts_filters_draft_words_placeholder"),
                words,
                palette,
                cx,
            ),
            blocklist_mode: BlocklistMode::Censor,
        }
    }

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
            .child(mono_caption(tr!("tts_filters_pipeline_header"), palette))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_muted)
                    .child(tr!("tts_filters_pipeline_hint")),
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
        title: impl Into<SharedString>,
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
                    .child(title.into()),
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
                            .child(tr!("tts_filters_stage_add")),
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

        let url_block = labeled(
            tr!("tts_filters_url_label"),
            url_seg.into_any_element(),
            palette,
            density,
        );
        let twitch = self.toggle_row(
            tr!("tts_filters_strip_twitch"),
            self.settings.strip_twitch_emotes,
            "filt-strip-twitch",
            palette,
            density,
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_strip_twitch(cx)),
        );
        let reward = self.toggle_row(
            tr!("tts_filters_strip_reward"),
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
            tr!("tts_filters_stage_emote_url_title"),
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
            tr!("tts_filters_stage_replacements_title"),
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
            tr!("tts_filters_blocklist_default_label"),
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
            tr!("tts_filters_stage_blocklist_title"),
            true,
            body,
            palette,
            density,
            cx,
        )
    }

    fn output_card(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let body = labeled(
            tr!("tts_filters_length_label"),
            self.max_length.clone().into_any_element(),
            palette,
            density,
        );
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
                    .child(tr!("tts_filters_stage_output_title")),
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
                .child(tr!("tts_filters_no_rules"))
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
            FilterRuleKind::Literal { .. } => (tr!("tts_filters_badge_text"), palette.info),
            FilterRuleKind::Regex { .. } => (tr!("tts_filters_badge_regex"), palette.brand),
            FilterRuleKind::Blocklist { .. } => (tr!("tts_filters_badge_block"), palette.warning),
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
        label: impl Into<SharedString>,
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
                    .child(label.into()),
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
        for (mode, id_part, label) in [
            (
                BlocklistMode::Censor,
                "censor",
                tr!("tts_filters_mode_censor"),
            ),
            (
                BlocklistMode::Suppress,
                "suppress",
                tr!("tts_filters_mode_skip"),
            ),
        ] {
            let active = current == mode;
            seg = seg.child(seg_button(
                SharedString::from(format!("{id_prefix}-{id_part}")),
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
            tr!("common_save")
        } else {
            tr!("tts_filters_draft_add")
        };
        let actions = div()
            .flex()
            .flex_row()
            .gap(spacing(Spacing::Xs, density))
            .child(primary_button(submit_label, palette).on_click(
                "filt-draft-submit",
                cx.listener(|this, _: &ClickEvent, _, cx| this.submit_draft(cx)),
            ))
            .child(secondary_button(tr!("common_cancel"), palette).on_click(
                "filt-draft-cancel",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_draft(cx)),
            ));

        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(mono_caption(tr!("tts_filters_draft_header"), palette))
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
            (tr!("tts_filters_unsaved"), palette.warning)
        } else {
            (tr!("tts_filters_saved"), palette.text_muted)
        };

        let save_btn: AnyElement = if self.dirty {
            primary_button(tr!("common_save"), palette)
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
                .child(tr!("common_save"))
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
            .child(mono_caption(
                tr!("tts_filters_preview_input_label"),
                palette,
            ))
            .child(self.preview_input.clone());

        let stages: AnyElement = if let Some(preview) = &self.cached_preview {
            let mut col = div().flex().flex_col().gap(gap_sm);
            for (i, outcome) in preview.stages.iter().enumerate() {
                col = col.child(preview_stage_card(
                    tr!("tts_filters_stage_n", n = (i + 1) as i64),
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
                .child(tr!("tts_filters_preview_empty"))
                .into_any_element()
        };

        let spoken = self
            .cached_preview
            .as_ref()
            .map(|p| matches!(p.result, PipelineResult::Speak(_)))
            .unwrap_or(false);
        let output_text: String = match self.cached_preview.as_ref().map(|p| &p.result) {
            Some(PipelineResult::Speak(s)) => s.clone(),
            Some(PipelineResult::Skip { .. }) => tr!("tts_filters_preview_skipped"),
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
            .child(mono_caption(
                tr!("tts_filters_preview_output_label"),
                palette,
            ))
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
                    .child(tr!("tts_filters_speak_preview_btn")),
            );

        let tip = card(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_muted)
                .child(tr!("tts_filters_preview_tip")),
            palette,
        )
        .radius(Radius::Sm)
        .padding_xy(spacing(Spacing::Xs, density), spacing(Spacing::Xs, density))
        .full_width();

        let inner = div()
            .flex()
            .flex_col()
            .gap(gap_md)
            .child(mono_caption(tr!("tts_filters_preview_header"), palette))
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

    fn delete_confirm(
        &self,
        index: usize,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = self.rules.get(index).map(display_name).unwrap_or_default();

        let card = confirm_modal(
            tr!("tts_filters_delete_title"),
            tr!("tts_filters_delete_body"),
            ConfirmTone::Destructive,
            palette,
        )
        .item_name(name)
        .esc_hint(tr!("widget_confirm_esc_to_cancel"))
        .on_cancel(
            "filt-delete-cancel",
            tr!("common_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_delete(cx)),
        )
        .on_confirm(
            "filt-delete-confirm",
            tr!("common_delete"),
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

#[derive(Clone, Copy)]
enum ModeTarget {
    Settings,
    Draft,
}

fn mono_caption(label: impl Into<SharedString>, palette: &ForgePalette) -> impl IntoElement {
    div()
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(FONT_XS)
        .text_color(palette.text_muted)
        .child(label.into())
}

fn labeled(
    label: impl Into<SharedString>,
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

#[allow(clippy::too_many_arguments)]
fn seg_button(
    id: SharedString,
    label: impl Into<SharedString>,
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
        .child(label.into());
    if active {
        chip = chip.bg(active_bg);
    } else {
        let hover = with_alpha(palette.border_regular, 0.06);
        chip = chip.hover(move |s| s.bg(hover));
    }
    chip
}

fn draft_field(
    placeholder: impl Into<SharedString>,
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
                    .child(tr!("tts_filters_stage_pass")),
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
                    .child(format!("{} - {reason:?}", tr!("tts_filters_stage_skipped"))),
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

fn url_label(mode: UrlMode) -> String {
    match mode {
        UrlMode::Speak => tr!("tts_filters_url_speak"),
        UrlMode::Replace => tr!("tts_filters_url_replace"),
        UrlMode::Suppress => tr!("tts_filters_url_suppress"),
    }
}

fn url_key(mode: UrlMode) -> &'static str {
    match mode {
        UrlMode::Speak => "speak",
        UrlMode::Replace => "replace",
        UrlMode::Suppress => "suppress",
    }
}

fn display_name(rule: &FilterRule) -> String {
    if rule.name.trim().is_empty() {
        DraftKind::of(&rule.kind).label()
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

fn same_kind_prev_index(rules: &[FilterRule], i: usize) -> Option<usize> {
    let kind = DraftKind::of(&rules.get(i)?.kind);
    rules[..i]
        .iter()
        .enumerate()
        .rev()
        .find(|(_, r)| DraftKind::of(&r.kind) == kind)
        .map(|(j, _)| j)
}

fn same_kind_next_index(rules: &[FilterRule], i: usize) -> Option<usize> {
    let kind = DraftKind::of(&rules.get(i)?.kind);
    rules
        .get(i + 1..)?
        .iter()
        .enumerate()
        .find(|(_, r)| DraftKind::of(&r.kind) == kind)
        .map(|(j, _)| i + 1 + j)
}
