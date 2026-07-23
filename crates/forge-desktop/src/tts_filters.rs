use std::sync::Arc;

use forge_components::{
    BORDER_THIN, Density, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon, InputEvent, ModalSize,
    OverlayPosition, Radius, Spacing, TextArea, TextInput, body_family, card, field_label,
    ghost_button, icon, modal, mono_family, overlay, primary_button_with_icon, radio_row,
    radio_row_label, radius, spacing, toggle, tr, with_alpha,
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
    AnyElement, App, ClickEvent, Context, Entity, EventEmitter, Pixels, Rgba, SharedString,
    Subscription, Window, div, prelude::*, px,
};

use crate::async_bridge;
use crate::presentation::ActivePresentation;

const STAGE_CIRCLE: Pixels = px(22.0);
const PREVIEW_W: Pixels = px(320.0);
const MODAL_W: Pixels = px(480.0);

#[derive(Clone, Copy)]
enum SkipRule {
    ContainsUrl,
    BotAccounts,
    LongerThan,
    Repeat,
    EmoteOnly,
    MostlyNonLatin,
}

impl SkipRule {
    fn key(self) -> &'static str {
        match self {
            SkipRule::ContainsUrl => "url",
            SkipRule::BotAccounts => "bots",
            SkipRule::LongerThan => "length",
            SkipRule::Repeat => "repeat",
            SkipRule::EmoteOnly => "emote-only",
            SkipRule::MostlyNonLatin => "non-latin",
        }
    }
}

#[derive(Clone, Copy)]
enum OutputOpt {
    ReadName,
    EmoteWord,
    Sanitize,
}

impl OutputOpt {
    fn key(self) -> &'static str {
        match self {
            OutputOpt::ReadName => "name",
            OutputOpt::EmoteWord => "emote",
            OutputOpt::Sanitize => "sanitize",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModalStage {
    Skip,
    Blocklist,
    Replace,
    Output,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkipPreset {
    Url,
    Prefix,
    Bots,
    Length,
    Repeat,
    EmoteOnly,
    NonLatin,
    Regex,
}

const SKIP_PRESETS: [SkipPreset; 8] = [
    SkipPreset::Url,
    SkipPreset::Prefix,
    SkipPreset::Bots,
    SkipPreset::Length,
    SkipPreset::Repeat,
    SkipPreset::EmoteOnly,
    SkipPreset::NonLatin,
    SkipPreset::Regex,
];

impl SkipPreset {
    fn key(self) -> &'static str {
        match self {
            SkipPreset::Url => "url",
            SkipPreset::Prefix => "prefix",
            SkipPreset::Bots => "bots",
            SkipPreset::Length => "length",
            SkipPreset::Repeat => "repeat",
            SkipPreset::EmoteOnly => "emote-only",
            SkipPreset::NonLatin => "non-latin",
            SkipPreset::Regex => "regex",
        }
    }

    fn label(self) -> SharedString {
        match self {
            SkipPreset::Url => tr!("tts_filters_preset_skip_url"),
            SkipPreset::Prefix => tr!("tts_filters_preset_skip_prefix"),
            SkipPreset::Bots => tr!("tts_filters_preset_skip_bots"),
            SkipPreset::Length => tr!("tts_filters_preset_skip_length"),
            SkipPreset::Repeat => tr!("tts_filters_preset_skip_repeat"),
            SkipPreset::EmoteOnly => tr!("tts_filters_preset_skip_emote_only"),
            SkipPreset::NonLatin => tr!("tts_filters_preset_skip_non_latin"),
            SkipPreset::Regex => tr!("tts_filters_preset_skip_regex"),
        }
        .into()
    }

    fn param_label(self) -> Option<SharedString> {
        match self {
            SkipPreset::Prefix => Some(tr!("tts_filters_preset_skip_prefix_label").into()),
            SkipPreset::Length => Some(tr!("tts_filters_preset_skip_length_label").into()),
            SkipPreset::Regex => Some(tr!("tts_filters_preset_skip_regex_label").into()),
            _ => None,
        }
    }

    fn placeholder(self) -> SharedString {
        match self {
            SkipPreset::Prefix => tr!("tts_filters_preset_skip_prefix_placeholder").into(),
            SkipPreset::Length => tr!("tts_filters_preset_skip_length_placeholder").into(),
            SkipPreset::Regex => tr!("tts_filters_preset_skip_regex_placeholder").into(),
            _ => SharedString::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputPreset {
    Name,
    Emote,
    Lang,
    MaxDur,
    Sanitize,
}

const OUTPUT_PRESETS: [OutputPreset; 5] = [
    OutputPreset::Name,
    OutputPreset::Emote,
    OutputPreset::Lang,
    OutputPreset::MaxDur,
    OutputPreset::Sanitize,
];

impl OutputPreset {
    fn key(self) -> &'static str {
        match self {
            OutputPreset::Name => "name",
            OutputPreset::Emote => "emote",
            OutputPreset::Lang => "lang",
            OutputPreset::MaxDur => "maxdur",
            OutputPreset::Sanitize => "sanitize",
        }
    }

    fn label(self) -> SharedString {
        match self {
            OutputPreset::Name => tr!("tts_filters_output_read_name"),
            OutputPreset::Emote => tr!("tts_filters_output_emote"),
            OutputPreset::Lang => tr!("tts_filters_preset_output_lang"),
            OutputPreset::MaxDur => tr!("tts_filters_preset_output_maxdur"),
            OutputPreset::Sanitize => tr!("tts_filters_output_sanitize"),
        }
        .into()
    }

    fn hint(self) -> SharedString {
        match self {
            OutputPreset::Name => tr!("tts_filters_preset_output_name_hint"),
            OutputPreset::Emote => tr!("tts_filters_preset_output_emote_hint"),
            OutputPreset::Lang => tr!("tts_filters_preset_output_lang_hint"),
            OutputPreset::MaxDur => tr!("tts_filters_preset_output_maxdur_hint"),
            OutputPreset::Sanitize => tr!("tts_filters_preset_output_sanitize_hint"),
        }
        .into()
    }

    fn disabled(self) -> bool {
        matches!(self, OutputPreset::Lang | OutputPreset::MaxDur)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReplaceKind {
    Text,
    Regex,
}

enum FilterDraft {
    Skip {
        preset: SkipPreset,
        param: String,
    },
    Output {
        preset: OutputPreset,
    },
    Blocklist {
        words: Vec<String>,
        mode: BlocklistMode,
    },
    Replace {
        kind: ReplaceKind,
        from: String,
        to: String,
    },
}

enum AddFilterEvent {
    Submit(FilterDraft),
    Cancel,
}

struct AddFilterModal {
    stage: ModalStage,
    skip_preset: SkipPreset,
    output_preset: OutputPreset,
    param: Entity<TextInput>,
    blocklist_words: Entity<TextArea>,
    blocklist_mode: BlocklistMode,
    replace_kind: ReplaceKind,
    replace_from: Entity<TextInput>,
    replace_to: Entity<TextInput>,
}

impl EventEmitter<AddFilterEvent> for AddFilterModal {}

impl AddFilterModal {
    fn new(stage: ModalStage, blocklist_mode: BlocklistMode, cx: &mut Context<Self>) -> Self {
        let palette = cx.palette();
        let param = cx.new(|cx| TextInput::new(SharedString::default(), cx).with_palette(palette));
        let blocklist_words = cx.new(|cx| {
            TextArea::new(tr!("tts_filters_modal_blocklist_words_placeholder"), cx)
                .with_palette(palette)
                .on_surface()
                .with_height(px(62.0))
        });
        let replace_from = cx.new(|cx| {
            TextInput::new(tr!("tts_filters_modal_replace_find_placeholder"), cx)
                .with_palette(palette)
        });
        let replace_to = cx.new(|cx| {
            TextInput::new(
                tr!("tts_filters_modal_replace_replace_text_placeholder"),
                cx,
            )
            .with_palette(palette)
        });
        AddFilterModal {
            stage,
            skip_preset: SkipPreset::Url,
            output_preset: OutputPreset::Name,
            param,
            blocklist_words,
            blocklist_mode,
            replace_kind: ReplaceKind::Text,
            replace_from,
            replace_to,
        }
    }

    fn set_skip_preset(&mut self, preset: SkipPreset, cx: &mut Context<Self>) {
        self.skip_preset = preset;
        let placeholder = preset.placeholder();
        self.param.update(cx, |field, cx| {
            field.set_placeholder(placeholder, cx);
            field.set_content("", cx);
        });
        cx.notify();
    }

    fn set_output_preset(&mut self, preset: OutputPreset, cx: &mut Context<Self>) {
        if preset.disabled() {
            return;
        }
        self.output_preset = preset;
        cx.notify();
    }

    fn set_blocklist_mode(&mut self, mode: BlocklistMode, cx: &mut Context<Self>) {
        self.blocklist_mode = mode;
        cx.notify();
    }

    fn set_replace_kind(&mut self, kind: ReplaceKind, cx: &mut Context<Self>) {
        self.replace_kind = kind;
        cx.notify();
    }

    fn is_valid(&self, cx: &App) -> bool {
        match self.stage {
            ModalStage::Skip => match self.skip_preset {
                SkipPreset::Prefix | SkipPreset::Regex => {
                    !self.param.read(cx).content().trim().is_empty()
                }
                SkipPreset::Length => self.param.read(cx).content().trim().parse::<u32>().is_ok(),
                _ => true,
            },
            ModalStage::Output => !self.output_preset.disabled(),
            ModalStage::Blocklist => !self.blocklist_words.read(cx).content().trim().is_empty(),
            ModalStage::Replace => !self.replace_from.read(cx).content().trim().is_empty(),
        }
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if !self.is_valid(cx) {
            return;
        }
        let draft = match self.stage {
            ModalStage::Skip => FilterDraft::Skip {
                preset: self.skip_preset,
                param: self.param.read(cx).content().trim().to_owned(),
            },
            ModalStage::Output => FilterDraft::Output {
                preset: self.output_preset,
            },
            ModalStage::Blocklist => FilterDraft::Blocklist {
                words: self
                    .blocklist_words
                    .read(cx)
                    .content()
                    .split([',', '\n'])
                    .map(str::trim)
                    .filter(|w| !w.is_empty())
                    .map(str::to_owned)
                    .collect(),
                mode: self.blocklist_mode,
            },
            ModalStage::Replace => FilterDraft::Replace {
                kind: self.replace_kind,
                from: self.replace_from.read(cx).content().trim().to_owned(),
                to: self.replace_to.read(cx).content().trim().to_owned(),
            },
        };
        cx.emit(AddFilterEvent::Submit(draft));
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(AddFilterEvent::Cancel);
    }
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
    add_modal: Option<Entity<AddFilterModal>>,
    _add_sub: Option<Subscription>,
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
            add_modal: None,
            _add_sub: None,
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
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let rules = repo.list_rules().await.map_err(|e| e.to_string())?;
                let settings = repo
                    .get_pipeline_settings()
                    .await
                    .map_err(|e| e.to_string())?;
                Ok::<_, String>((rules, settings))
            },
            |this, result, cx| match result {
                Ok((rules, settings)) => this.apply_loaded(rules, settings, cx),
                Err(message) => this.on_repo_error(&message, cx),
            },
            cx,
        );
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
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                repo.replace_rules(&rules)
                    .await
                    .map_err(|e| e.to_string())?;
                repo.set_pipeline_settings(&settings)
                    .await
                    .map_err(|e| e.to_string())?;
                if let Some(handle) = pipeline_config {
                    handle.swap(config);
                }
                Ok::<(), String>(())
            },
            |this, result, cx| {
                if let Err(message) = result {
                    this.save_error = Some(message);
                    cx.notify();
                }
            },
            cx,
        );
    }

    fn try_apply(&mut self, cx: &mut Context<Self>, mutate: impl FnOnce(&mut Self)) -> bool {
        let backup_settings = self.settings.clone();
        let backup_rules = self.rules.clone();
        mutate(self);
        renumber(&mut self.rules);
        match build_config_strict(&self.rules, &self.settings) {
            Ok(_) => {
                self.after_change(cx);
                true
            }
            Err(e) => {
                self.settings = backup_settings;
                self.rules = backup_rules;
                self.save_error = Some(e.to_string());
                cx.notify();
                false
            }
        }
    }

    fn skip_flag(&self, rule: SkipRule) -> bool {
        match rule {
            SkipRule::ContainsUrl => self.settings.skip_contains_url,
            SkipRule::BotAccounts => self.settings.skip_from_bot_accounts,
            SkipRule::LongerThan => self.settings.skip_longer_than,
            SkipRule::Repeat => self.settings.skip_repeat_of_recent,
            SkipRule::EmoteOnly => self.settings.skip_emote_only,
            SkipRule::MostlyNonLatin => self.settings.skip_mostly_non_latin,
        }
    }

    fn set_skip(&mut self, rule: SkipRule, value: bool, cx: &mut Context<Self>) {
        if self.skip_flag(rule) == value {
            return;
        }
        match rule {
            SkipRule::ContainsUrl => self.settings.skip_contains_url = value,
            SkipRule::BotAccounts => self.settings.skip_from_bot_accounts = value,
            SkipRule::LongerThan => self.settings.skip_longer_than = value,
            SkipRule::Repeat => self.settings.skip_repeat_of_recent = value,
            SkipRule::EmoteOnly => self.settings.skip_emote_only = value,
            SkipRule::MostlyNonLatin => self.settings.skip_mostly_non_latin = value,
        }
        self.after_change(cx);
    }

    fn toggle_skip(&mut self, rule: SkipRule, cx: &mut Context<Self>) {
        let value = !self.skip_flag(rule);
        self.set_skip(rule, value, cx);
    }

    fn clear_skip_prefix(&mut self, cx: &mut Context<Self>) {
        if self.settings.skip_prefix.is_none() && !self.settings.skip_starts_with_bang {
            return;
        }
        self.settings.skip_prefix = None;
        self.settings.skip_starts_with_bang = false;
        self.after_change(cx);
    }

    fn remove_skip_regex(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.settings.skip_custom_regexes.len() {
            self.settings.skip_custom_regexes.remove(index);
        }
        self.after_change(cx);
    }

    fn output_flag(&self, opt: OutputOpt) -> bool {
        match opt {
            OutputOpt::ReadName => self.settings.output_read_display_name_first,
            OutputOpt::EmoteWord => self.settings.output_emote_to_word,
            OutputOpt::Sanitize => self.settings.output_sanitize_punctuation,
        }
    }

    fn set_output(&mut self, opt: OutputOpt, value: bool, cx: &mut Context<Self>) {
        if self.output_flag(opt) == value {
            return;
        }
        match opt {
            OutputOpt::ReadName => self.settings.output_read_display_name_first = value,
            OutputOpt::EmoteWord => self.settings.output_emote_to_word = value,
            OutputOpt::Sanitize => self.settings.output_sanitize_punctuation = value,
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

    fn open_add_modal(&mut self, stage: ModalStage, cx: &mut Context<Self>) {
        let mode = self.settings.blocklist_mode;
        let modal = cx.new(|cx| AddFilterModal::new(stage, mode, cx));
        self._add_sub = Some(cx.subscribe(&modal, Self::on_add_event));
        self.add_modal = Some(modal);
        self.save_error = None;
        cx.notify();
    }

    fn close_add_modal(&mut self, cx: &mut Context<Self>) {
        self.add_modal = None;
        self._add_sub = None;
        self.save_error = None;
        cx.notify();
    }

    fn on_add_event(
        &mut self,
        _modal: Entity<AddFilterModal>,
        event: &AddFilterEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            AddFilterEvent::Submit(draft) => self.apply_draft(draft, cx),
            AddFilterEvent::Cancel => self.close_add_modal(cx),
        }
    }

    fn apply_draft(&mut self, draft: &FilterDraft, cx: &mut Context<Self>) {
        let applied = match draft {
            FilterDraft::Skip { preset, param } => {
                self.apply_skip_preset(*preset, param.clone(), cx)
            }
            FilterDraft::Output { preset } => self.apply_output_preset(*preset, cx),
            FilterDraft::Blocklist { words, mode } => {
                self.apply_blocklist_words(words.clone(), *mode, cx)
            }
            FilterDraft::Replace { kind, from, to } => {
                self.apply_replacement(*kind, from.clone(), to.clone(), cx)
            }
        };
        if applied {
            self.close_add_modal(cx);
        } else {
            cx.notify();
        }
    }

    fn apply_skip_preset(
        &mut self,
        preset: SkipPreset,
        param: String,
        cx: &mut Context<Self>,
    ) -> bool {
        self.try_apply(cx, |this| match preset {
            SkipPreset::Url => this.settings.skip_contains_url = true,
            SkipPreset::Prefix => {
                this.settings.skip_prefix = Some(param);
                this.settings.skip_starts_with_bang = false;
            }
            SkipPreset::Bots => this.settings.skip_from_bot_accounts = true,
            SkipPreset::Length => {
                this.settings.skip_longer_than = true;
                if let Ok(n) = param.parse::<u32>() {
                    this.settings.longer_than_max_chars = n;
                }
            }
            SkipPreset::Repeat => this.settings.skip_repeat_of_recent = true,
            SkipPreset::EmoteOnly => this.settings.skip_emote_only = true,
            SkipPreset::NonLatin => this.settings.skip_mostly_non_latin = true,
            SkipPreset::Regex => this.settings.skip_custom_regexes.push(param),
        })
    }

    fn apply_output_preset(&mut self, preset: OutputPreset, cx: &mut Context<Self>) -> bool {
        self.try_apply(cx, |this| match preset {
            OutputPreset::Name => this.settings.output_read_display_name_first = true,
            OutputPreset::Emote => this.settings.output_emote_to_word = true,
            OutputPreset::Sanitize => this.settings.output_sanitize_punctuation = true,
            OutputPreset::Lang | OutputPreset::MaxDur => {}
        })
    }

    fn apply_blocklist_words(
        &mut self,
        words: Vec<String>,
        mode: BlocklistMode,
        cx: &mut Context<Self>,
    ) -> bool {
        if words.is_empty() {
            return false;
        }
        self.try_apply(cx, |this| {
            this.settings.blocklist_mode = mode;
            if let Some(rule) = this
                .rules
                .iter_mut()
                .find(|r| matches!(r.kind, FilterRuleKind::Blocklist { .. }))
            {
                if let FilterRuleKind::Blocklist {
                    words: existing,
                    mode: m,
                } = &mut rule.kind
                {
                    for word in &words {
                        if !existing.iter().any(|e| e.eq_ignore_ascii_case(word)) {
                            existing.push(word.clone());
                        }
                    }
                    *m = mode;
                }
            } else {
                let position = this.rules.len() as u32;
                this.rules.push(FilterRule {
                    id: ulid::Ulid::generate().to_string(),
                    name: String::new(),
                    enabled: true,
                    position,
                    kind: FilterRuleKind::Blocklist { words, mode },
                });
            }
        })
    }

    fn apply_replacement(
        &mut self,
        kind: ReplaceKind,
        from: String,
        to: String,
        cx: &mut Context<Self>,
    ) -> bool {
        self.try_apply(cx, |this| {
            let filter_kind = match kind {
                ReplaceKind::Text => FilterRuleKind::Literal {
                    pattern: from,
                    replacement: to,
                },
                ReplaceKind::Regex => FilterRuleKind::Regex {
                    pattern: from,
                    replacement: to,
                },
            };
            let position = this.rules.len() as u32;
            this.rules.push(FilterRule {
                id: ulid::Ulid::generate().to_string(),
                name: String::new(),
                enabled: true,
                position,
                kind: filter_kind,
            });
        })
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
                source_event_id: None,
                is_reward: false,
            };
            if let Err(e) = handle.send(SpeakCommand::Enqueue(request)).await {
                eprintln!("forge-desktop: filter preview speak failed: {e}");
            }
        });
    }

    fn pipeline_column(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let intro = div()
            .font_family(body_family())
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
                    .font_family(mono_family())
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
                    .font_family(mono_family())
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
                    .font_family(body_family())
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
                    .font_family(body_family())
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
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(label_color)
                    .child(label),
            );
        if let Some(meta) = meta {
            row = row.child(
                div()
                    .font_family(mono_family())
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
        let fixed: [(SkipRule, SharedString); 6] = [
            (
                SkipRule::ContainsUrl,
                tr!("tts_filters_skip_contains_url").into(),
            ),
            (
                SkipRule::BotAccounts,
                tr!("tts_filters_skip_bot_accounts").into(),
            ),
            (
                SkipRule::LongerThan,
                tr!(
                    "tts_filters_skip_longer_than",
                    chars = self.settings.longer_than_max_chars as i64
                )
                .into(),
            ),
            (
                SkipRule::Repeat,
                tr!(
                    "tts_filters_skip_repeat",
                    window = self.settings.repeat_of_recent_window as i64
                )
                .into(),
            ),
            (
                SkipRule::EmoteOnly,
                tr!("tts_filters_skip_emote_only").into(),
            ),
            (
                SkipRule::MostlyNonLatin,
                tr!("tts_filters_skip_mostly_non_latin").into(),
            ),
        ];

        let prefix_display = self
            .settings
            .skip_prefix
            .clone()
            .filter(|p| !p.is_empty())
            .or_else(|| self.settings.skip_starts_with_bang.then(|| "!".to_owned()));
        let regex_count = self.settings.skip_custom_regexes.len();
        let total = fixed.len() + usize::from(prefix_display.is_some()) + regex_count;

        let mut body = div().flex().flex_col();
        let mut i = 0usize;
        for (rule, label) in fixed {
            let on = self.skip_flag(rule);
            let toggle_id = SharedString::from(format!("filt-skip-t-{}", rule.key()));
            let x_id = SharedString::from(format!("filt-skip-x-{}", rule.key()));
            i += 1;
            body = body.child(self.stage_row(
                label,
                on,
                None,
                toggle_id,
                x_id,
                i != total,
                true,
                cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_skip(rule, cx)),
                cx.listener(move |this, _: &ClickEvent, _, cx| this.set_skip(rule, false, cx)),
                palette,
                density,
            ));
        }
        if let Some(prefix) = prefix_display {
            i += 1;
            body = body.child(self.stage_row(
                tr!("tts_filters_skip_prefix", prefix = prefix).into(),
                true,
                None,
                "filt-skip-t-prefix".into(),
                "filt-skip-x-prefix".into(),
                i != total,
                true,
                cx.listener(|this, _: &ClickEvent, _, cx| this.clear_skip_prefix(cx)),
                cx.listener(|this, _: &ClickEvent, _, cx| this.clear_skip_prefix(cx)),
                palette,
                density,
            ));
        }
        for (ri, pattern) in self.settings.skip_custom_regexes.iter().enumerate() {
            i += 1;
            let x_id = SharedString::from(format!("filt-skip-regex-x-{ri}"));
            let toggle_id = SharedString::from(format!("filt-skip-regex-t-{ri}"));
            body = body.child(self.stage_row(
                tr!("tts_filters_skip_regex_row", pattern = pattern.clone()).into(),
                true,
                Some(tr!("tts_filters_badge_regex").into()),
                toggle_id,
                x_id,
                i != total,
                true,
                cx.listener(move |this, _: &ClickEvent, _, cx| this.remove_skip_regex(ri, cx)),
                cx.listener(move |this, _: &ClickEvent, _, cx| this.remove_skip_regex(ri, cx)),
                palette,
                density,
            ));
        }

        let add = self.add_button(
            "filt-skip-add",
            cx.listener(|this, _: &ClickEvent, _, cx| this.open_add_modal(ModalStage::Skip, cx)),
            palette,
            density,
        );

        self.stage_frame(
            1,
            Icon::FilterOff,
            palette.random,
            tr!("tts_filters_stage_skip_title"),
            Some(add),
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
            cx.listener(|this, _: &ClickEvent, _, cx| {
                this.open_add_modal(ModalStage::Blocklist, cx)
            }),
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
                .font_family(body_family())
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
                            .font_family(mono_family())
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
                            .font_family(mono_family())
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
                .font_family(body_family())
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
            cx.listener(|this, _: &ClickEvent, _, cx| this.open_add_modal(ModalStage::Replace, cx)),
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
        let rows: [(OutputOpt, SharedString, SharedString, bool); 3] = [
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
            (
                OutputOpt::Sanitize,
                tr!("tts_filters_output_sanitize").into(),
                tr!("tts_filters_output_sanitize_meta").into(),
                self.settings.output_sanitize_punctuation,
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

        let add = self.add_button(
            "filt-out-add",
            cx.listener(|this, _: &ClickEvent, _, cx| this.open_add_modal(ModalStage::Output, cx)),
            palette,
            density,
        );

        self.stage_frame(
            4,
            Icon::Send,
            palette.success,
            tr!("tts_filters_stage_output_title"),
            Some(add),
            body.into_any_element(),
            palette,
            density,
        )
    }
}

impl AddFilterModal {
    fn modal_meta(
        stage: ModalStage,
    ) -> (Icon, fn(&ForgePalette) -> Rgba, SharedString, SharedString) {
        match stage {
            ModalStage::Skip => (
                Icon::FilterOff,
                (|p: &ForgePalette| p.random) as fn(&ForgePalette) -> Rgba,
                tr!("tts_filters_modal_skip_title").into(),
                tr!("tts_filters_modal_skip_subtitle").into(),
            ),
            ModalStage::Blocklist => (
                Icon::Ban,
                (|p: &ForgePalette| p.warning) as fn(&ForgePalette) -> Rgba,
                tr!("tts_filters_modal_blocklist_title").into(),
                tr!("tts_filters_modal_blocklist_subtitle").into(),
            ),
            ModalStage::Replace => (
                Icon::Replace,
                (|p: &ForgePalette| p.info) as fn(&ForgePalette) -> Rgba,
                tr!("tts_filters_modal_replace_title").into(),
                tr!("tts_filters_modal_replace_subtitle").into(),
            ),
            ModalStage::Output => (
                Icon::Send,
                (|p: &ForgePalette| p.success) as fn(&ForgePalette) -> Rgba,
                tr!("tts_filters_modal_output_title").into(),
                tr!("tts_filters_modal_output_subtitle").into(),
            ),
        }
    }

    fn render_skip_body(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let color = palette.random;
        let mut list = div().flex().flex_col().gap(px(5.0));
        for preset in SKIP_PRESETS {
            let selected = self.skip_preset == preset;
            let id = SharedString::from(format!("filt-modal-skip-{}", preset.key()));
            list =
                list.child(
                    radio_row(
                        id,
                        selected,
                        color,
                        radio_row_label(preset.label(), None, selected, palette),
                        palette,
                    )
                    .on_click(cx.listener(
                        move |this, _: &ClickEvent, _, cx| this.set_skip_preset(preset, cx),
                    )),
                );
        }

        let mut body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(field_label(
                palette,
                tr!("tts_filters_modal_condition_label"),
                list,
            ));

        if let Some(param_label) = self.skip_preset.param_label() {
            body = body.child(field_label(palette, param_label, self.param.clone()));
        }

        body.into_any_element()
    }

    fn render_output_body(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let color = palette.success;
        let mut list = div().flex().flex_col().gap(px(5.0));
        for preset in OUTPUT_PRESETS {
            let selected = self.output_preset == preset;
            let disabled = preset.disabled();
            let id = SharedString::from(format!("filt-modal-output-{}", preset.key()));
            list = list.child(
                radio_row(
                    id,
                    selected,
                    color,
                    radio_row_label(preset.label(), Some(preset.hint()), selected, palette),
                    palette,
                )
                .disabled(disabled)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.set_output_preset(preset, cx)
                })),
            );
        }

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(field_label(
                palette,
                tr!("tts_filters_modal_condition_label"),
                list,
            ))
            .into_any_element()
    }

    fn render_blocklist_body(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let note = div()
            .font_family(body_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(tr!("tts_filters_modal_blocklist_note"));

        let censor_selected = self.blocklist_mode == BlocklistMode::Censor;
        let skip_selected = self.blocklist_mode == BlocklistMode::Suppress;
        let mode_list = div()
            .flex()
            .flex_col()
            .gap(px(5.0))
            .child(
                radio_row(
                    "filt-modal-bl-censor",
                    censor_selected,
                    palette.warning,
                    radio_row_label(
                        tr!("tts_filters_modal_blocklist_censor_row"),
                        Some(tr!("tts_filters_modal_blocklist_censor_row_hint").into()),
                        censor_selected,
                        palette,
                    ),
                    palette,
                )
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.set_blocklist_mode(BlocklistMode::Censor, cx)
                })),
            )
            .child(
                radio_row(
                    "filt-modal-bl-skip",
                    skip_selected,
                    palette.warning,
                    radio_row_label(
                        tr!("tts_filters_modal_blocklist_skip_row"),
                        Some(tr!("tts_filters_modal_blocklist_skip_row_hint").into()),
                        skip_selected,
                        palette,
                    ),
                    palette,
                )
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.set_blocklist_mode(BlocklistMode::Suppress, cx)
                })),
            );

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(field_label(
                palette,
                tr!("tts_filters_modal_blocklist_words_label"),
                self.blocklist_words.clone(),
            ))
            .child(note)
            .child(field_label(
                palette,
                tr!("tts_filters_modal_blocklist_when_matched_label"),
                mode_list,
            ))
            .into_any_element()
    }

    fn render_replace_body(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_regex = self.replace_kind == ReplaceKind::Regex;
        let mut tabs = div()
            .flex()
            .p(px(2.0))
            .gap(px(2.0))
            .rounded(radius(Radius::Sm))
            .bg(palette.shell)
            .border(BORDER_THIN)
            .border_color(palette.border_regular);
        for (kind, label) in [
            (ReplaceKind::Text, tr!("tts_filters_modal_replace_text_tab")),
            (
                ReplaceKind::Regex,
                tr!("tts_filters_modal_replace_regex_tab"),
            ),
        ] {
            let active = self.replace_kind == kind;
            let id = SharedString::from(format!("filt-modal-replace-tab-{}", tab_key(kind)));
            let fg = if active {
                palette.shell
            } else {
                palette.text_secondary
            };
            let mut chip = div()
                .id(id)
                .py(px(5.0))
                .px(px(14.0))
                .rounded(radius(Radius::Sm))
                .cursor_pointer()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(fg)
                .on_click(
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.set_replace_kind(kind, cx)),
                )
                .child(label);
            chip = if active { chip.bg(palette.info) } else { chip };
            tabs = tabs.child(chip);
        }

        let find_label = if is_regex {
            tr!("tts_filters_modal_replace_match_label")
        } else {
            tr!("tts_filters_modal_replace_find_label")
        };
        let note = div()
            .font_family(body_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(tr!("tts_filters_modal_replace_note"));

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(tabs)
            .child(field_label(palette, find_label, self.replace_from.clone()))
            .child(field_label(
                palette,
                tr!("tts_filters_modal_replace_replace_label"),
                self.replace_to.clone(),
            ))
            .child(note)
            .into_any_element()
    }
}

impl Render for AddFilterModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();
        let (glyph, color_fn, title, subtitle) = Self::modal_meta(self.stage);
        let color = color_fn(&palette);

        let body: AnyElement = match self.stage {
            ModalStage::Skip => self.render_skip_body(&palette, density, cx),
            ModalStage::Output => self.render_output_body(&palette, density, cx),
            ModalStage::Blocklist => self.render_blocklist_body(&palette, density, cx),
            ModalStage::Replace => self.render_replace_body(&palette, density, cx),
        };

        let valid = self.is_valid(cx);
        let status_text = if valid {
            tr!("tts_filters_modal_footer_valid")
        } else {
            tr!("tts_filters_modal_footer_invalid")
        };
        let submit_label = if self.stage == ModalStage::Blocklist {
            tr!("tts_filters_modal_add_words")
        } else {
            tr!("tts_filters_modal_add_rule")
        };

        let footer = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(status_text),
            )
            .child(
                div()
                    .flex()
                    .gap(spacing(Spacing::Xs, density))
                    .child(
                        ghost_button(tr!("tts_filters_modal_cancel"), &palette).on_click(
                            "filt-modal-cancel",
                            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
                        ),
                    )
                    .child(
                        primary_button_with_icon(Icon::Plus, submit_label, &palette)
                            .disabled(!valid)
                            .on_click(
                                "filt-modal-submit",
                                cx.listener(|this, _: &ClickEvent, _, cx| this.submit(cx)),
                            ),
                    ),
            );

        let card = modal(title, body, &palette)
            .subtitle(subtitle)
            .header_icon(glyph, color)
            .size(ModalSize::Md)
            .width(MODAL_W)
            .footer(footer)
            .on_close(
                "filt-modal-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
            );

        let view = cx.entity();
        overlay(card, &palette)
            .position(OverlayPosition::Center)
            .on_dismiss("filt-modal-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel(cx));
            })
            .into_any_element()
    }
}

impl TtsFiltersView {
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
                    .font_family(body_family())
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
                    .font_family(body_family())
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
                    .font_family(body_family())
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
        let modal_overlay = self.add_modal.clone();

        div()
            .size_full()
            .flex()
            .flex_row()
            .bg(palette.base)
            .child(pipeline)
            .child(preview)
            .children(modal_overlay)
    }
}

fn mono_caption(label: impl Into<SharedString>, palette: &ForgePalette) -> impl IntoElement {
    div()
        .font_family(mono_family())
        .text_size(FONT_XXS)
        .text_color(palette.text_muted)
        .child(label.into())
}

fn tab_key(kind: ReplaceKind) -> &'static str {
    match kind {
        ReplaceKind::Text => "text",
        ReplaceKind::Regex => "regex",
    }
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
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.success)
                    .child("\u{2713}"),
            )
            .child(
                div()
                    .font_family(body_family())
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
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.random)
                    .child("\u{d7}"),
            )
            .child(
                div()
                    .font_family(body_family())
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
            .font_family(body_family())
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
                    .font_family(mono_family())
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
                    .font_family(body_family())
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
                    .font_family(body_family())
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
