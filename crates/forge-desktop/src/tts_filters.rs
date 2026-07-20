use std::sync::Arc;

use forge_components::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_SM, FONT_XS, FONT_XXS,
    ForgePalette, Icon, InputEvent, Radius, Spacing, TextArea, TextInput, card, icon,
    primary_button, radius, secondary_button, spacing, toggle, tr, with_alpha,
};
use forge_speak_queue::{
    PipelineConfigHandle, Priority, RequestId, SpeakCommand, SpeakQueueHandle, SpeakRequest,
    build_config_lenient, build_config_strict,
};
use forge_storage::{
    BlocklistMode, FilterRule, FilterRuleKind, TtsFiltersRepo, TtsPipelineSettings,
};
use forge_tts_pipeline::{PipelineResult, SkipReason, StageAction, StageName, StageOutcome};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, Pixels, Rgba, SharedString, Subscription, Window,
    div, prelude::*, px,
};

use crate::presentation::ActivePresentation;

const STAGE_CIRCLE: Pixels = px(22.0);
const PREVIEW_W: Pixels = px(320.0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DraftKind {
    Literal,
    Regex,
    Blocklist,
}

impl DraftKind {
    fn key(self) -> &'static str {
        match self {
            DraftKind::Literal => "text",
            DraftKind::Regex => "regex",
            DraftKind::Blocklist => "blocklist",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DraftScope {
    Replacement,
    Blocklist,
}

#[derive(Clone, Copy)]
enum SkipRule {
    ContainsUrl,
    StartsBang,
    BotAccounts,
    LongerThan,
    Repeat,
}

impl SkipRule {
    fn key(self) -> &'static str {
        match self {
            SkipRule::ContainsUrl => "url",
            SkipRule::StartsBang => "bang",
            SkipRule::BotAccounts => "bots",
            SkipRule::LongerThan => "length",
            SkipRule::Repeat => "repeat",
        }
    }
}

#[derive(Clone, Copy)]
enum OutputOpt {
    ReadName,
    EmoteWord,
}

impl OutputOpt {
    fn key(self) -> &'static str {
        match self {
            OutputOpt::ReadName => "name",
            OutputOpt::EmoteWord => "emote",
        }
    }
}

struct RuleDraft {
    editing: Option<usize>,
    kind: DraftKind,
    scope: DraftScope,
    name: Entity<TextInput>,
    pattern: Entity<TextInput>,
    replacement: Entity<TextInput>,
    words: Entity<TextInput>,
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
    draft: Option<RuleDraft>,
    save_error: Option<String>,
    blocklist_expanded: bool,
    preview_input: Entity<TextArea>,
    cached_preview: Option<CachedPreview>,
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

        let preview_input = cx.new(|cx| {
            let mut input = TextArea::new(tr!("tts_filters_preview_input_placeholder"), cx)
                .with_palette(palette)
                .on_surface()
                .with_height(px(56.0));
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
            settings: TtsPipelineSettings::default(),
            draft: None,
            save_error: None,
            blocklist_expanded: false,
            preview_input,
            cached_preview: None,
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
        renumber(&mut self.rules);
        self.settings = settings;
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
        let speaker_name = tr!("tts_filters_preview_speaker_name");
        let context = forge_tts_pipeline::PipelineContext {
            viewer_name: &speaker_name,
            recent_messages: &[],
        };
        let (result, stages) = forge_tts_pipeline::preview(&input, &config, &context);
        self.cached_preview = Some(CachedPreview { stages, result });
    }

    fn after_change(&mut self, cx: &mut Context<Self>) {
        self.refresh_preview(cx);
        self.persist(cx);
        cx.notify();
    }

    fn persist(&mut self, cx: &mut Context<Self>) {
        let config = match build_config_strict(&self.rules, &self.settings) {
            Ok(config) => config,
            Err(e) => {
                self.save_error = Some(e.to_string());
                return;
            }
        };
        self.save_error = None;

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
        cx.spawn(async move |this, cx| {
            if let Ok(Err(message)) = rx.await {
                let _ = this.update(cx, |this, cx| {
                    this.save_error = Some(message);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn skip_flag(&self, rule: SkipRule) -> bool {
        match rule {
            SkipRule::ContainsUrl => self.settings.skip_contains_url,
            SkipRule::StartsBang => self.settings.skip_starts_with_bang,
            SkipRule::BotAccounts => self.settings.skip_from_bot_accounts,
            SkipRule::LongerThan => self.settings.skip_longer_than,
            SkipRule::Repeat => self.settings.skip_repeat_of_recent,
        }
    }

    fn set_skip(&mut self, rule: SkipRule, value: bool, cx: &mut Context<Self>) {
        if self.skip_flag(rule) == value {
            return;
        }
        match rule {
            SkipRule::ContainsUrl => self.settings.skip_contains_url = value,
            SkipRule::StartsBang => self.settings.skip_starts_with_bang = value,
            SkipRule::BotAccounts => self.settings.skip_from_bot_accounts = value,
            SkipRule::LongerThan => self.settings.skip_longer_than = value,
            SkipRule::Repeat => self.settings.skip_repeat_of_recent = value,
        }
        self.after_change(cx);
    }

    fn toggle_skip(&mut self, rule: SkipRule, cx: &mut Context<Self>) {
        let value = !self.skip_flag(rule);
        self.set_skip(rule, value, cx);
    }

    fn output_flag(&self, opt: OutputOpt) -> bool {
        match opt {
            OutputOpt::ReadName => self.settings.output_read_display_name_first,
            OutputOpt::EmoteWord => self.settings.output_emote_to_word,
        }
    }

    fn set_output(&mut self, opt: OutputOpt, value: bool, cx: &mut Context<Self>) {
        if self.output_flag(opt) == value {
            return;
        }
        match opt {
            OutputOpt::ReadName => self.settings.output_read_display_name_first = value,
            OutputOpt::EmoteWord => self.settings.output_emote_to_word = value,
        }
        self.after_change(cx);
    }

    fn toggle_output(&mut self, opt: OutputOpt, cx: &mut Context<Self>) {
        let value = !self.output_flag(opt);
        self.set_output(opt, value, cx);
    }

    fn set_blocklist_mode(&mut self, mode: BlocklistMode, cx: &mut Context<Self>) {
        self.settings.blocklist_mode = mode;
        for rule in self.rules.iter_mut() {
            if let FilterRuleKind::Blocklist { mode: m, .. } = &mut rule.kind {
                *m = mode;
            }
        }
        self.after_change(cx);
    }

    fn remove_blocklist_word(
        &mut self,
        rule_index: usize,
        word_index: usize,
        cx: &mut Context<Self>,
    ) {
        let mut drop_rule = false;
        if let Some(rule) = self.rules.get_mut(rule_index)
            && let FilterRuleKind::Blocklist { words, .. } = &mut rule.kind
        {
            if word_index < words.len() {
                words.remove(word_index);
            }
            drop_rule = words.is_empty();
        }
        if drop_rule && rule_index < self.rules.len() {
            self.rules.remove(rule_index);
            renumber(&mut self.rules);
        }
        self.after_change(cx);
    }

    fn expand_blocklist(&mut self, cx: &mut Context<Self>) {
        self.blocklist_expanded = true;
        cx.notify();
    }

    fn toggle_rule(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(rule) = self.rules.get_mut(index) {
            rule.enabled = !rule.enabled;
        }
        self.after_change(cx);
    }

    fn delete_rule(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.rules.len() {
            self.rules.remove(index);
            renumber(&mut self.rules);
        }
        self.after_change(cx);
    }

    fn open_add_replacement(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let draft = self.build_draft(
            None,
            DraftKind::Literal,
            DraftScope::Replacement,
            "",
            "",
            "",
            "",
            cx,
        );
        draft.pattern.update(cx, |f, cx| f.focus(window, cx));
        self.draft = Some(draft);
        cx.notify();
    }

    fn open_add_blocklist(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let draft = self.build_draft(
            None,
            DraftKind::Blocklist,
            DraftScope::Blocklist,
            "",
            "",
            "",
            "",
            cx,
        );
        draft.words.update(cx, |f, cx| f.focus(window, cx));
        self.draft = Some(draft);
        cx.notify();
    }

    fn set_draft_kind(&mut self, kind: DraftKind, cx: &mut Context<Self>) {
        if let Some(draft) = self.draft.as_mut() {
            draft.kind = kind;
        }
        cx.notify();
    }

    fn cancel_draft(&mut self, cx: &mut Context<Self>) {
        self.draft = None;
        self.save_error = None;
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
                mode: self.settings.blocklist_mode,
            },
        };
        let editing = draft.editing;

        let mut prospective = self.rules.clone();
        match editing {
            Some(i) if i < prospective.len() => {
                prospective[i].name = name;
                prospective[i].kind = kind;
            }
            _ => {
                let position = prospective.len() as u32;
                prospective.push(FilterRule {
                    id: ulid::Ulid::generate().to_string(),
                    name,
                    enabled: true,
                    position,
                    kind,
                });
            }
        }
        renumber(&mut prospective);

        if let Err(e) = build_config_strict(&prospective, &self.settings) {
            self.save_error = Some(e.to_string());
            cx.notify();
            return;
        }

        self.rules = prospective;
        self.draft = None;
        self.save_error = None;
        self.after_change(cx);
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
        scope: DraftScope,
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
            scope,
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
        }
    }

    fn pipeline_column(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let intro = div()
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(FONT_XS)
            .text_color(palette.text_muted)
            .child(tr!("tts_filters_pipeline_intro"));

        let mut col = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .p(spacing(Spacing::Md, density))
            .child(intro);

        if let Some(err) = &self.save_error {
            col = col.child(
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
                    .child(err.clone()),
            );
        }

        col = col
            .child(self.skip_card(palette, density, cx))
            .child(self.blocklist_card(palette, density, cx))
            .child(self.replacements_card(palette, density, cx))
            .child(self.output_card(palette, density, cx));

        if self.draft.is_some() {
            col = col.child(self.draft_card(palette, density, cx));
        }

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
    fn stage_frame(
        &self,
        n: u8,
        glyph: Icon,
        color: Rgba,
        title: impl Into<SharedString>,
        add: Option<AnyElement>,
        body: AnyElement,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
        let circle = div()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .size(STAGE_CIRCLE)
            .rounded(radius(Radius::Pill))
            .bg(color)
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.shell)
                    .child(n.to_string()),
            );

        let mut header = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Md, density))
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(circle)
            .child(icon(glyph, FONT_SM, color))
            .child(
                div()
                    .flex_1()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(title.into()),
            );
        if let Some(add) = add {
            header = header.child(add);
        }

        let body_wrap = div()
            .px(spacing(Spacing::Md, density))
            .py(spacing(Spacing::Xs, density))
            .child(body);

        card(
            div().flex().flex_col().child(header).child(body_wrap),
            palette,
        )
        .padding(px(0.0))
        .radius(Radius::Md)
        .full_width()
        .into_any_element()
    }

    fn add_button(
        &self,
        id: &'static str,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
        div()
            .id(id)
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .cursor_pointer()
            .on_click(handler)
            .child(icon(Icon::Plus, FONT_XS, palette.brand))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.brand)
                    .child(tr!("tts_filters_stage_add")),
            )
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn stage_row(
        &self,
        label: SharedString,
        on: bool,
        meta: Option<SharedString>,
        toggle_id: SharedString,
        x_id: SharedString,
        divider: bool,
        deletable: bool,
        on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
        on_x: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
        let label_color = if on {
            palette.text_primary
        } else {
            palette.text_muted
        };
        let mut row = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .py(spacing(Spacing::Xs, density));
        if divider {
            row = row
                .border_b(BORDER_THIN)
                .border_color(palette.border_regular);
        }
        row = row
            .child(toggle(on, palette).on_click(toggle_id, on_toggle))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(label_color)
                    .child(label),
            );
        if let Some(meta) = meta {
            row = row.child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(meta),
            );
        }
        if deletable {
            row = row.child(
                div()
                    .id(x_id)
                    .flex()
                    .cursor_pointer()
                    .on_click(on_x)
                    .child(icon(Icon::X, FONT_XS, palette.text_faint)),
            );
        }
        row.into_any_element()
    }

    fn skip_card(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows: [(SkipRule, SharedString, bool); 5] = [
            (
                SkipRule::ContainsUrl,
                tr!("tts_filters_skip_contains_url").into(),
                self.settings.skip_contains_url,
            ),
            (
                SkipRule::StartsBang,
                tr!("tts_filters_skip_starts_bang").into(),
                self.settings.skip_starts_with_bang,
            ),
            (
                SkipRule::BotAccounts,
                tr!("tts_filters_skip_bot_accounts").into(),
                self.settings.skip_from_bot_accounts,
            ),
            (
                SkipRule::LongerThan,
                tr!(
                    "tts_filters_skip_longer_than",
                    chars = self.settings.longer_than_max_chars as i64
                )
                .into(),
                self.settings.skip_longer_than,
            ),
            (
                SkipRule::Repeat,
                tr!(
                    "tts_filters_skip_repeat",
                    window = self.settings.repeat_of_recent_window as i64
                )
                .into(),
                self.settings.skip_repeat_of_recent,
            ),
        ];

        let last = rows.len() - 1;
        let mut body = div().flex().flex_col();
        for (i, (rule, label, on)) in rows.into_iter().enumerate() {
            let toggle_id = SharedString::from(format!("filt-skip-t-{}", rule.key()));
            let x_id = SharedString::from(format!("filt-skip-x-{}", rule.key()));
            body = body.child(self.stage_row(
                label,
                on,
                None,
                toggle_id,
                x_id,
                i != last,
                true,
                cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_skip(rule, cx)),
                cx.listener(move |this, _: &ClickEvent, _, cx| this.set_skip(rule, false, cx)),
                palette,
                density,
            ));
        }

        self.stage_frame(
            1,
            Icon::FilterOff,
            palette.random,
            tr!("tts_filters_stage_skip_title"),
            None,
            body.into_any_element(),
            palette,
            density,
        )
    }

    fn blocklist_card(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let censor_on = self.settings.blocklist_mode == BlocklistMode::Censor;
        let skip_on = self.settings.blocklist_mode == BlocklistMode::Suppress;

        let mut body = div().flex().flex_col();
        body = body.child(self.stage_row(
            tr!("tts_filters_blocklist_censor").into(),
            censor_on,
            Some(tr!("tts_filters_blocklist_censor_meta").into()),
            "filt-bl-censor-t".into(),
            "filt-bl-censor-x".into(),
            true,
            false,
            cx.listener(|this, _: &ClickEvent, _, cx| {
                this.set_blocklist_mode(BlocklistMode::Censor, cx)
            }),
            |_, _, _| {},
            palette,
            density,
        ));
        body = body.child(self.stage_row(
            tr!("tts_filters_blocklist_skip").into(),
            skip_on,
            None,
            "filt-bl-skip-t".into(),
            "filt-bl-skip-x".into(),
            true,
            false,
            cx.listener(|this, _: &ClickEvent, _, cx| {
                this.set_blocklist_mode(BlocklistMode::Suppress, cx)
            }),
            |_, _, _| {},
            palette,
            density,
        ));

        body = body.child(self.blocklist_chips(palette, density, cx));

        let add = self.add_button(
            "filt-bl-add",
            cx.listener(|this, _: &ClickEvent, window, cx| this.open_add_blocklist(window, cx)),
            palette,
            density,
        );

        self.stage_frame(
            2,
            Icon::Ban,
            palette.warning,
            tr!("tts_filters_stage_blocklist_title"),
            Some(add),
            body.into_any_element(),
            palette,
            density,
        )
    }

    fn blocklist_chips(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut words: Vec<(usize, usize, String)> = Vec::new();
        for (ri, rule) in self.rules.iter().enumerate() {
            if let FilterRuleKind::Blocklist { words: ws, .. } = &rule.kind {
                for (wi, w) in ws.iter().enumerate() {
                    words.push((ri, wi, w.clone()));
                }
            }
        }

        if words.is_empty() {
            return div()
                .py(spacing(Spacing::Xs, density))
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_muted)
                .child(tr!("tts_filters_blocklist_empty"))
                .into_any_element();
        }

        let total = words.len();
        let visible = if self.blocklist_expanded {
            total
        } else {
            total.min(3)
        };

        let mut row = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .py(spacing(Spacing::Xs, density));

        for (ri, wi, word) in words.into_iter().take(visible) {
            let x_id = SharedString::from(format!("filt-bl-word-{ri}-{wi}"));
            row = row.child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xxs, density))
                    .py(spacing(Spacing::Xxs, density))
                    .px(spacing(Spacing::Xs, density))
                    .rounded(radius(Radius::Sm))
                    .bg(palette.surface_overlay)
                    .child(
                        div()
                            .font_family(DEFAULT_MONO_FAMILY)
                            .text_size(FONT_XXS)
                            .text_color(palette.text_primary)
                            .child(word),
                    )
                    .child(
                        div()
                            .id(x_id)
                            .flex()
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.remove_blocklist_word(ri, wi, cx)
                            }))
                            .child(icon(Icon::X, FONT_XXS, palette.text_faint)),
                    ),
            );
        }

        if !self.blocklist_expanded && total > 3 {
            row = row.child(
                div()
                    .id("filt-bl-more")
                    .cursor_pointer()
                    .py(spacing(Spacing::Xxs, density))
                    .px(spacing(Spacing::Xs, density))
                    .rounded(radius(Radius::Sm))
                    .bg(palette.surface_overlay)
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.expand_blocklist(cx)))
                    .child(
                        div()
                            .font_family(DEFAULT_MONO_FAMILY)
                            .text_size(FONT_XXS)
                            .text_color(palette.text_faint)
                            .child(tr!(
                                "tts_filters_blocklist_more",
                                count = (total - 3) as i64
                            )),
                    ),
            );
        }

        row.into_any_element()
    }

    fn replacements_card(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let reps: Vec<(usize, &FilterRule)> = self
            .rules
            .iter()
            .enumerate()
            .filter(|(_, r)| is_replacement_kind(&r.kind))
            .collect();

        let body: AnyElement = if reps.is_empty() {
            div()
                .py(spacing(Spacing::Xs, density))
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_muted)
                .child(tr!("tts_filters_replacements_empty"))
                .into_any_element()
        } else {
            let last = reps.len() - 1;
            let mut col = div().flex().flex_col();
            for (pos, (index, rule)) in reps.into_iter().enumerate() {
                let toggle_id = SharedString::from(format!("filt-rep-t-{index}"));
                let x_id = SharedString::from(format!("filt-rep-x-{index}"));
                col = col.child(self.stage_row(
                    rule_summary(rule).into(),
                    rule.enabled,
                    Some(replacement_meta(rule).into()),
                    toggle_id,
                    x_id,
                    pos != last,
                    true,
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_rule(index, cx)),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.delete_rule(index, cx)),
                    palette,
                    density,
                ));
            }
            col.into_any_element()
        };

        let add = self.add_button(
            "filt-rep-add",
            cx.listener(|this, _: &ClickEvent, window, cx| this.open_add_replacement(window, cx)),
            palette,
            density,
        );

        self.stage_frame(
            3,
            Icon::Replace,
            palette.info,
            tr!("tts_filters_stage_replacements_title"),
            Some(add),
            body,
            palette,
            density,
        )
    }

    fn output_card(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows: [(OutputOpt, SharedString, SharedString, bool); 2] = [
            (
                OutputOpt::ReadName,
                tr!("tts_filters_output_read_name").into(),
                tr!("tts_filters_output_read_name_meta").into(),
                self.settings.output_read_display_name_first,
            ),
            (
                OutputOpt::EmoteWord,
                tr!("tts_filters_output_emote").into(),
                tr!("tts_filters_output_emote_meta").into(),
                self.settings.output_emote_to_word,
            ),
        ];

        let last = rows.len() - 1;
        let mut body = div().flex().flex_col();
        for (i, (opt, label, meta, on)) in rows.into_iter().enumerate() {
            let toggle_id = SharedString::from(format!("filt-out-t-{}", opt.key()));
            let x_id = SharedString::from(format!("filt-out-x-{}", opt.key()));
            body = body.child(self.stage_row(
                label,
                on,
                Some(meta),
                toggle_id,
                x_id,
                i != last,
                true,
                cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_output(opt, cx)),
                cx.listener(move |this, _: &ClickEvent, _, cx| this.set_output(opt, false, cx)),
                palette,
                density,
            ));
        }

        self.stage_frame(
            4,
            Icon::Send,
            palette.success,
            tr!("tts_filters_stage_output_title"),
            None,
            body.into_any_element(),
            palette,
            density,
        )
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

        let params: AnyElement = match draft.scope {
            DraftScope::Replacement => {
                let mut kind_row = div().flex().flex_row().gap(spacing(Spacing::Xs, density));
                for (kind, label) in [
                    (DraftKind::Literal, tr!("tts_filters_badge_text")),
                    (DraftKind::Regex, tr!("tts_filters_badge_regex")),
                ] {
                    let active = draft.kind == kind;
                    kind_row = kind_row.child(seg_button(
                        SharedString::from(format!("filt-draft-kind-{}", kind.key())),
                        label,
                        active,
                        palette.info,
                        palette,
                        density,
                        cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.set_draft_kind(kind, cx)
                        }),
                    ));
                }
                div()
                    .flex()
                    .flex_col()
                    .gap(spacing(Spacing::Xs, density))
                    .child(kind_row)
                    .child(draft.pattern.clone())
                    .child(draft.replacement.clone())
                    .child(draft.name.clone())
                    .into_any_element()
            }
            DraftScope::Blocklist => div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xs, density))
                .child(draft.words.clone())
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
            .child(params)
            .child(actions);

        card(body, palette)
            .padding_xy(spacing(Spacing::Sm, density), spacing(Spacing::Sm, density))
            .full_width()
            .into_any_element()
    }

    fn preview_column(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let gap_md = spacing(Spacing::Sm, density);

        let header = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(icon(Icon::Eye, FONT_SM, palette.brand))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(tr!("tts_filters_preview_header")),
            );

        let input_block = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(mono_caption(
                tr!("tts_filters_preview_input_label"),
                palette,
            ))
            .child(self.preview_input.clone());

        let mut stages_section = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(mono_caption(
                tr!("tts_filters_preview_output_label"),
                palette,
            ));

        if let Some(preview) = &self.cached_preview {
            for (i, outcome) in preview.stages.iter().enumerate() {
                stages_section = stages_section.child(preview_stage_card(
                    (i + 1) as u32,
                    stage_name_label(outcome.stage),
                    outcome,
                    palette,
                    density,
                ));
            }
            stages_section = stages_section.child(final_output_card(
                (preview.stages.len() + 1) as u32,
                &preview.result,
                palette,
                density,
            ));
        } else {
            stages_section = stages_section.child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(tr!("tts_filters_preview_empty")),
            );
        }

        let speak_btn = div()
            .id("filt-speak-preview")
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Xs, density))
            .rounded(radius(Radius::Sm))
            .bg(palette.brand)
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.speak_preview(cx)))
            .child(icon(Icon::PlayerPlayFilled, FONT_XS, palette.shell))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.shell)
                    .child(tr!("tts_filters_speak_preview_btn")),
            );

        let inner = div()
            .flex()
            .flex_col()
            .gap(gap_md)
            .child(header)
            .child(input_block)
            .child(stages_section)
            .child(speak_btn);

        div()
            .w(PREVIEW_W)
            .flex_none()
            .h_full()
            .bg(palette.shell)
            .border_l(BORDER_THIN)
            .border_color(palette.border_regular)
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
}

impl Render for TtsFiltersView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let pipeline = self.pipeline_column(&palette, density, cx);
        let preview = self.preview_column(&palette, density, cx);

        div()
            .size_full()
            .flex()
            .flex_row()
            .bg(palette.base)
            .child(pipeline)
            .child(preview)
    }
}

fn mono_caption(label: impl Into<SharedString>, palette: &ForgePalette) -> impl IntoElement {
    div()
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(FONT_XXS)
        .text_color(palette.text_muted)
        .child(label.into())
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
    n: u32,
    name: String,
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
                    .text_size(FONT_XS)
                    .text_color(palette.success)
                    .child("\u{2713}"),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(tr!("tts_filters_stage_pass")),
            )
            .into_any_element(),
        StageAction::Transformed => {
            highlighted_output(&outcome.input, &outcome.output, palette, density)
        }
        StageAction::Skipped { reason } => div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.random)
                    .child("\u{d7}"),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.random)
                    .child(format!(
                        "{}: {}",
                        tr!("tts_filters_stage_skipped"),
                        skip_reason_label(reason)
                    )),
            )
            .into_any_element(),
    };

    stage_card_frame(format!("{n} \u{b7} {name}"), body, palette, density)
}

fn final_output_card(
    n: u32,
    result: &PipelineResult,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let (text, color) = match result {
        PipelineResult::Speak(spoken) => (format!("\"{spoken}\""), palette.text_primary),
        PipelineResult::Skip { reason } => (
            format!(
                "{}: {}",
                tr!("tts_filters_stage_skipped"),
                skip_reason_label(reason)
            ),
            palette.random,
        ),
    };

    stage_card_frame(
        format!("{} \u{b7} {}", n, tr!("tts_filters_preview_final_label")),
        div()
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(FONT_XS)
            .text_color(color)
            .child(text)
            .into_any_element(),
        palette,
        density,
    )
}

fn stage_card_frame(
    label: String,
    body: AnyElement,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    card(
        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(label),
            )
            .child(body),
        palette,
    )
    .radius(Radius::Md)
    .padding_xy(spacing(Spacing::Xs, density), spacing(Spacing::Sm, density))
    .full_width()
    .into_any_element()
}

fn highlighted_output(
    input: &str,
    output: &str,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let input_tokens: std::collections::HashSet<&str> = input.split_whitespace().collect();
    let mut row = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(spacing(Spacing::Xxs, density));
    for token in output.split_whitespace() {
        if input_tokens.contains(token) {
            row = row.child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(token.to_owned()),
            );
        } else {
            let color = if token.contains('*') {
                palette.warning
            } else {
                palette.brand
            };
            row = row.child(
                div()
                    .px(px(3.0))
                    .rounded(px(2.0))
                    .bg(palette.surface_overlay)
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(color)
                    .child(token.to_owned()),
            );
        }
    }
    row.into_any_element()
}

fn stage_name_label(stage: StageName) -> String {
    match stage {
        StageName::SkipRules => tr!("tts_filters_stage_name_skip_rules"),
        StageName::WordBlocklist => tr!("tts_filters_stage_name_blocklist"),
        StageName::TextReplacements => tr!("tts_filters_stage_name_replacements"),
        StageName::Output => tr!("tts_filters_stage_name_output"),
    }
}

fn skip_reason_label(reason: &SkipReason) -> String {
    match reason {
        SkipReason::MatchedSkipRule(_) => tr!("tts_filters_skip_reason_rule"),
        SkipReason::BlockedByWordFilter => tr!("tts_filters_skip_reason_blocked"),
        SkipReason::EmptyAfterProcessing => tr!("tts_filters_skip_reason_empty"),
    }
}

fn renumber(rules: &mut [FilterRule]) {
    for (i, rule) in rules.iter_mut().enumerate() {
        rule.position = i as u32;
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

fn replacement_meta(rule: &FilterRule) -> String {
    let base = match &rule.kind {
        FilterRuleKind::Regex { .. } => tr!("tts_filters_badge_regex"),
        _ => tr!("tts_filters_badge_text"),
    };
    if rule.name.trim().is_empty() {
        base
    } else {
        format!("{base} ({})", rule.name.trim())
    }
}

fn is_replacement_kind(kind: &FilterRuleKind) -> bool {
    matches!(
        kind,
        FilterRuleKind::Literal { .. } | FilterRuleKind::Regex { .. }
    )
}
