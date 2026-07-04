use forge_speak_queue::{
    Priority, RequestId, SpeakCommand, SpeakRequest, build_config_lenient, build_config_strict,
};
use forge_storage::{
    BlocklistMode, FilterRule, FilterRuleKind, TtsFiltersRepo, TtsPipelineSettings, UrlMode,
};
use forge_tts_pipeline::{PipelineResult, StageAction, StageOutcome};
use forge_widgets::tokens::{
    BORDER_THIN, FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius, sp, spf,
};
use forge_widgets::{ForgePalette, Icon, tabler_icon};
use iced::widget::{Space, button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Background, Border, Color, Element, Length, Task};

use crate::Message;
use crate::message::{TtsFiltersMsg, TtsMsg};
use crate::runtime_view::RuntimeView;

/// Selectable rule kind in the draft editor, decoupled from the parameter-carrying
/// `FilterRuleKind` so the picker can be chosen before the parameters exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftKind {
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
}

/// In-progress add/edit form. `editing` is the index into the working rule list when
/// editing an existing rule, `None` when adding a new one.
struct RuleDraft {
    editing: Option<usize>,
    kind: DraftKind,
    name: String,
    pattern: String,
    replacement: String,
    words: String,
    blocklist_mode: BlocklistMode,
}

impl RuleDraft {
    fn blank() -> Self {
        Self {
            editing: None,
            kind: DraftKind::Literal,
            name: String::new(),
            pattern: String::new(),
            replacement: String::new(),
            words: String::new(),
            blocklist_mode: BlocklistMode::Censor,
        }
    }

    fn from_rule(index: usize, rule: &FilterRule) -> Self {
        let mut draft = Self::blank();
        draft.editing = Some(index);
        draft.kind = DraftKind::of(&rule.kind);
        draft.name = rule.name.clone();
        match &rule.kind {
            FilterRuleKind::Literal {
                pattern,
                replacement,
            }
            | FilterRuleKind::Regex {
                pattern,
                replacement,
            } => {
                draft.pattern = pattern.clone();
                draft.replacement = replacement.clone();
            }
            FilterRuleKind::Blocklist { words, mode } => {
                draft.words = words.join(", ");
                draft.blocklist_mode = *mode;
            }
        }
        draft
    }

    fn to_kind(&self) -> FilterRuleKind {
        match self.kind {
            DraftKind::Literal => FilterRuleKind::Literal {
                pattern: self.pattern.clone(),
                replacement: self.replacement.clone(),
            },
            DraftKind::Regex => FilterRuleKind::Regex {
                pattern: self.pattern.clone(),
                replacement: self.replacement.clone(),
            },
            DraftKind::Blocklist => FilterRuleKind::Blocklist {
                words: self
                    .words
                    .split(',')
                    .map(str::trim)
                    .filter(|w| !w.is_empty())
                    .map(str::to_owned)
                    .collect(),
                mode: self.blocklist_mode,
            },
        }
    }
}

pub struct TtsFiltersState {
    pub preview_input: String,
    rules: Vec<FilterRule>,
    settings: TtsPipelineSettings,
    max_length_input: String,
    draft: Option<RuleDraft>,
    save_error: Option<String>,
    dirty: bool,
    cached_preview: Option<CachedPreview>,
}

pub struct CachedPreview {
    pub stages: Vec<StageOutcome>,
    pub result: PipelineResult,
}

impl TtsFiltersState {
    pub fn new() -> Self {
        let settings = TtsPipelineSettings::default();
        Self {
            preview_input: String::new(),
            rules: Vec::new(),
            max_length_input: settings
                .max_length
                .map(|n| n.to_string())
                .unwrap_or_default(),
            settings,
            draft: None,
            save_error: None,
            dirty: false,
            cached_preview: None,
        }
    }

    fn renumber(&mut self) {
        for (i, rule) in self.rules.iter_mut().enumerate() {
            rule.position = i as u32;
        }
    }

    fn refresh_preview(&mut self) {
        if self.preview_input.is_empty() {
            self.cached_preview = None;
            return;
        }
        let config = build_config_lenient(&self.rules, &self.settings);
        let (result, stages) = forge_tts_pipeline::preview(&self.preview_input, &config);
        self.cached_preview = Some(CachedPreview { stages, result });
    }
}

impl Default for TtsFiltersState {
    fn default() -> Self {
        Self::new()
    }
}

async fn load_filters(
    repo: std::sync::Arc<dyn TtsFiltersRepo>,
) -> Result<(Vec<FilterRule>, TtsPipelineSettings), String> {
    let rules = repo.list_rules().await.map_err(|e| e.to_string())?;
    let settings = repo
        .get_pipeline_settings()
        .await
        .map_err(|e| e.to_string())?;
    Ok((rules, settings))
}

pub fn update(state: &mut TtsFiltersState, rt: &RuntimeView, msg: TtsFiltersMsg) -> Task<Message> {
    match msg {
        TtsFiltersMsg::LoadRequested => {
            let repo = rt.backend.tts_filters_repo();
            Task::perform(load_filters(repo), |r| {
                Message::Tts(TtsMsg::Filters(TtsFiltersMsg::Loaded(r)))
            })
        }
        TtsFiltersMsg::Loaded(Ok((mut rules, settings))) => {
            rules.sort_by_key(|r| r.position);
            state.rules = rules;
            state.renumber();
            state.max_length_input = settings
                .max_length
                .map(|n| n.to_string())
                .unwrap_or_default();
            state.settings = settings;
            state.dirty = false;
            state.save_error = None;
            state.refresh_preview();
            Task::none()
        }
        TtsFiltersMsg::Loaded(Err(e)) => {
            tracing::warn!(error = %e, "failed to load tts filters");
            Task::none()
        }
        TtsFiltersMsg::PreviewInputChanged(s) => {
            state.preview_input = s;
            state.refresh_preview();
            Task::none()
        }
        TtsFiltersMsg::AddRuleClicked => {
            state.draft = Some(RuleDraft::blank());
            Task::none()
        }
        TtsFiltersMsg::EditRule(i) => {
            if let Some(rule) = state.rules.get(i) {
                state.draft = Some(RuleDraft::from_rule(i, rule));
            }
            Task::none()
        }
        TtsFiltersMsg::DeleteRule(i) => {
            if i < state.rules.len() {
                state.rules.remove(i);
                state.renumber();
                state.dirty = true;
                state.refresh_preview();
            }
            Task::none()
        }
        TtsFiltersMsg::ToggleRule(i) => {
            if let Some(rule) = state.rules.get_mut(i) {
                rule.enabled = !rule.enabled;
                state.dirty = true;
                state.refresh_preview();
            }
            Task::none()
        }
        TtsFiltersMsg::MoveRuleUp(i) => {
            if i > 0 && i < state.rules.len() {
                state.rules.swap(i, i - 1);
                state.renumber();
                state.dirty = true;
                state.refresh_preview();
            }
            Task::none()
        }
        TtsFiltersMsg::MoveRuleDown(i) => {
            if i + 1 < state.rules.len() {
                state.rules.swap(i, i + 1);
                state.renumber();
                state.dirty = true;
                state.refresh_preview();
            }
            Task::none()
        }
        TtsFiltersMsg::DraftKindChanged(kind) => {
            if let Some(draft) = state.draft.as_mut() {
                draft.kind = kind;
            }
            Task::none()
        }
        TtsFiltersMsg::DraftNameChanged(s) => {
            if let Some(draft) = state.draft.as_mut() {
                draft.name = s;
            }
            Task::none()
        }
        TtsFiltersMsg::DraftPatternChanged(s) => {
            if let Some(draft) = state.draft.as_mut() {
                draft.pattern = s;
            }
            Task::none()
        }
        TtsFiltersMsg::DraftReplacementChanged(s) => {
            if let Some(draft) = state.draft.as_mut() {
                draft.replacement = s;
            }
            Task::none()
        }
        TtsFiltersMsg::DraftWordsChanged(s) => {
            if let Some(draft) = state.draft.as_mut() {
                draft.words = s;
            }
            Task::none()
        }
        TtsFiltersMsg::DraftBlocklistModeChanged(mode) => {
            if let Some(draft) = state.draft.as_mut() {
                draft.blocklist_mode = mode;
            }
            Task::none()
        }
        TtsFiltersMsg::DraftSubmit => {
            let Some(draft) = state.draft.take() else {
                return Task::none();
            };
            let kind = draft.to_kind();
            match draft.editing {
                Some(i) if i < state.rules.len() => {
                    state.rules[i].name = draft.name;
                    state.rules[i].kind = kind;
                }
                _ => {
                    let position = state.rules.len() as u32;
                    state.rules.push(FilterRule {
                        id: ulid::Ulid::new().to_string(),
                        name: draft.name,
                        enabled: true,
                        position,
                        kind,
                    });
                }
            }
            state.renumber();
            state.dirty = true;
            state.refresh_preview();
            Task::none()
        }
        TtsFiltersMsg::DraftCancel => {
            state.draft = None;
            Task::none()
        }
        TtsFiltersMsg::UrlModeChanged(mode) => {
            state.settings.url_mode = mode;
            state.dirty = true;
            state.refresh_preview();
            Task::none()
        }
        TtsFiltersMsg::MaxLengthChanged(raw) => {
            state.max_length_input = raw;
            state.settings.max_length = state.max_length_input.trim().parse::<u32>().ok();
            state.dirty = true;
            state.refresh_preview();
            Task::none()
        }
        TtsFiltersMsg::StripTwitchEmotesToggled(v) => {
            state.settings.strip_twitch_emotes = v;
            state.dirty = true;
            state.refresh_preview();
            Task::none()
        }
        TtsFiltersMsg::StripRewardEmotesToggled(v) => {
            state.settings.strip_reward_emotes = v;
            state.dirty = true;
            state.refresh_preview();
            Task::none()
        }
        TtsFiltersMsg::SettingsBlocklistModeChanged(mode) => {
            state.settings.blocklist_mode = mode;
            state.dirty = true;
            state.refresh_preview();
            Task::none()
        }
        TtsFiltersMsg::Save => {
            let config = match build_config_strict(&state.rules, &state.settings) {
                Ok(config) => config,
                Err(e) => {
                    state.save_error = Some(e.to_string());
                    return Task::none();
                }
            };
            state.save_error = None;
            let repo = rt.backend.tts_filters_repo();
            let handle = rt.pipeline_config.clone();
            let rules = state.rules.clone();
            let settings = state.settings.clone();
            Task::perform(
                async move {
                    repo.replace_rules(&rules)
                        .await
                        .map_err(|e| e.to_string())?;
                    repo.set_pipeline_settings(&settings)
                        .await
                        .map_err(|e| e.to_string())?;
                    if let Some(handle) = handle {
                        handle.swap(config);
                    }
                    Ok(())
                },
                |r| Message::Tts(TtsMsg::Filters(TtsFiltersMsg::SaveResult(r))),
            )
        }
        TtsFiltersMsg::SaveResult(Ok(())) => {
            state.dirty = false;
            Task::none()
        }
        TtsFiltersMsg::SaveResult(Err(e)) => {
            state.save_error = Some(e);
            Task::none()
        }
        TtsFiltersMsg::SpeakPreview => {
            let text = state.preview_input.trim();
            if text.is_empty() {
                return Task::none();
            }
            let Some(handle) = rt.speak_queue.clone() else {
                return Task::none();
            };
            let text = text.to_owned();
            Task::perform(
                async move {
                    let request = SpeakRequest {
                        request_id: RequestId::new(),
                        viewer_id: String::new(),
                        viewer_name: forge_widgets::tr!("tts_filters_preview_speaker_name"),
                        text,
                        priority: Priority::Normal,
                        alias_override: None,
                        engine_override: None,
                        voice_override: None,
                        source_event_id: forge_types::EventId::new(),
                    };
                    handle
                        .send(SpeakCommand::Enqueue(request))
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| Message::Tts(TtsMsg::Filters(TtsFiltersMsg::SpeakPreviewResult(r))),
            )
        }
        TtsFiltersMsg::SpeakPreviewResult(r) => {
            if let Err(e) = r {
                tracing::warn!(error = %e, "filter preview speak failed");
            }
            Task::none()
        }
    }
}

pub fn tts_filters_view<'a>(
    state: &'a TtsFiltersState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let gap_sm = spf(Spacing::Xs);
    let gap_md = spf(Spacing::Sm);

    let pipeline_col = pipeline_column_view(state, palette, gap_sm, gap_md);
    let preview_col = preview_column_view(state, palette, gap_sm, gap_md);

    row![pipeline_col, preview_col].height(Length::Fill).into()
}

fn pipeline_column_view<'a>(
    state: &'a TtsFiltersState,
    palette: &'a ForgePalette,
    gap_sm: f32,
    gap_md: f32,
) -> Element<'a, Message> {
    let header = column![
        text(forge_widgets::tr!("tts_filters_pipeline_header"))
            .size(FONT_XS)
            .color(palette.text_muted)
            .font(font(FontRole::Monospace)),
        text(forge_widgets::tr!("tts_filters_pipeline_hint"))
            .size(FONT_SM)
            .color(palette.text_muted),
    ]
    .spacing(spf(Spacing::Xxs));

    let rules_section = rules_section_view(state, palette, gap_sm);
    let settings_section = settings_section_view(state, palette, gap_sm);
    let draft_section: Element<'a, Message> = match &state.draft {
        Some(draft) => draft_form_view(draft, palette, gap_sm),
        None => Space::new().into(),
    };
    let save_bar = save_bar_view(state, palette, gap_sm);

    scrollable(
        container(
            column![
                header,
                rules_section,
                draft_section,
                settings_section,
                save_bar,
            ]
            .spacing(gap_md),
        )
        .padding([sp(Spacing::Md), sp(Spacing::Md)])
        .width(Length::Fill),
    )
    .height(Length::Fill)
    .width(Length::Fill)
    .into()
}

fn section_card<'a>(
    title: String,
    body: Element<'a, Message>,
    palette: &'a ForgePalette,
    gap_sm: f32,
) -> Element<'a, Message> {
    let header = text(title)
        .size(FONT_XS)
        .color(palette.text_muted)
        .font(font(FontRole::Monospace));
    container(column![header, body].spacing(gap_sm))
        .style(move |_| container::Style {
            background: Some(Background::Color(palette.elevated)),
            border: Border {
                color: palette.border_regular,
                width: BORDER_THIN,
                radius: radius(Radius::Md).into(),
            },
            ..container::Style::default()
        })
        .padding([sp(Spacing::Sm), sp(Spacing::Sm)])
        .width(Length::Fill)
        .into()
}

fn rules_section_view<'a>(
    state: &'a TtsFiltersState,
    palette: &'a ForgePalette,
    gap_sm: f32,
) -> Element<'a, Message> {
    let body: Element<'a, Message> = if state.rules.is_empty() {
        text(forge_widgets::tr!("tts_filters_no_rules"))
            .size(FONT_SM)
            .color(palette.text_muted)
            .into()
    } else {
        let last = state.rules.len() - 1;
        let rows: Vec<Element<'a, Message>> = state
            .rules
            .iter()
            .enumerate()
            .map(|(i, rule)| rule_row(i, rule, i == 0, i == last, palette, gap_sm))
            .collect();
        column(rows).spacing(gap_sm).into()
    };

    let add_btn = button(
        row![
            tabler_icon(Icon::Plus, FONT_XS, palette.text_muted),
            text(forge_widgets::tr!("tts_filters_add_rule_btn"))
                .size(FONT_XS)
                .color(palette.text_muted),
        ]
        .spacing(spf(Spacing::Xxs))
        .align_y(Alignment::Center),
    )
    .on_press(Message::Tts(TtsMsg::Filters(TtsFiltersMsg::AddRuleClicked)))
    .style(move |_, _| button::Style {
        background: None,
        border: Border {
            color: palette.border_regular,
            width: BORDER_THIN,
            radius: radius(Radius::Sm).into(),
        },
        text_color: palette.text_muted,
        ..button::Style::default()
    })
    .padding([sp(Spacing::Xxs), sp(Spacing::Xs)]);

    section_card(
        forge_widgets::tr!("tts_filters_rules_header"),
        column![body, add_btn].spacing(gap_sm).into(),
        palette,
        gap_sm,
    )
}

fn rule_row<'a>(
    index: usize,
    rule: &'a FilterRule,
    is_first: bool,
    is_last: bool,
    palette: &'a ForgePalette,
    gap_sm: f32,
) -> Element<'a, Message> {
    let (badge_label, badge_color) = match &rule.kind {
        FilterRuleKind::Literal { .. } => ("TEXT", palette.info),
        FilterRuleKind::Regex { .. } => ("REGEX", palette.brand),
        FilterRuleKind::Blocklist { .. } => ("BLOCK", palette.warning),
    };

    let badge = container(
        text(badge_label)
            .size(8.5)
            .color(badge_color)
            .font(font(FontRole::Monospace)),
    )
    .style(move |_| container::Style {
        background: Some(Background::Color(palette.surface_overlay)),
        border: Border {
            radius: radius(Radius::Sm).into(),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .padding([sp(Spacing::Xxs), sp(Spacing::Xxs)]);

    let summary = rule_summary(rule);
    let name_color = if rule.enabled {
        palette.text_primary
    } else {
        palette.text_faint
    };

    let toggle_label = if rule.enabled {
        forge_widgets::tr!("tts_filters_rule_on")
    } else {
        forge_widgets::tr!("tts_filters_rule_off")
    };
    let toggle_color = if rule.enabled {
        palette.success
    } else {
        palette.text_faint
    };

    let icon_btn = |icon: Icon, msg: TtsFiltersMsg, enabled: bool| {
        let color = if enabled {
            palette.text_muted
        } else {
            palette.text_faint
        };
        let mut b = button(tabler_icon(icon, FONT_XS, color))
            .style(move |_, _| button::Style {
                background: None,
                text_color: color,
                ..button::Style::default()
            })
            .padding(sp(Spacing::Xxs));
        if enabled {
            b = b.on_press(Message::Tts(TtsMsg::Filters(msg)));
        }
        b
    };

    let controls = row![
        button(
            text(toggle_label)
                .size(8.5)
                .color(toggle_color)
                .font(font(FontRole::Monospace)),
        )
        .on_press(Message::Tts(TtsMsg::Filters(TtsFiltersMsg::ToggleRule(
            index
        ))))
        .style(move |_, _| button::Style {
            background: None,
            text_color: toggle_color,
            ..button::Style::default()
        })
        .padding(sp(Spacing::Xxs)),
        icon_btn(Icon::ArrowUp, TtsFiltersMsg::MoveRuleUp(index), !is_first),
        icon_btn(
            Icon::ArrowDown,
            TtsFiltersMsg::MoveRuleDown(index),
            !is_last
        ),
        icon_btn(Icon::Settings, TtsFiltersMsg::EditRule(index), true),
        icon_btn(Icon::X, TtsFiltersMsg::DeleteRule(index), true),
    ]
    .spacing(0)
    .align_y(Alignment::Center);

    container(
        row![
            badge,
            column![
                text(display_name(rule)).size(FONT_XS).color(name_color),
                text(summary)
                    .size(8.5)
                    .color(palette.text_muted)
                    .font(font(FontRole::Monospace)),
            ]
            .spacing(spf(Spacing::Xxs))
            .width(Length::Fill),
            controls,
        ]
        .align_y(Alignment::Center)
        .spacing(gap_sm),
    )
    .style(move |_| container::Style {
        background: Some(Background::Color(palette.shell)),
        border: Border {
            color: palette.border_regular,
            width: BORDER_THIN,
            radius: radius(Radius::Sm).into(),
        },
        ..container::Style::default()
    })
    .padding([sp(Spacing::Xs), sp(Spacing::Xs)])
    .width(Length::Fill)
    .into()
}

fn display_name(rule: &FilterRule) -> String {
    if rule.name.trim().is_empty() {
        match &rule.kind {
            FilterRuleKind::Literal { .. } => forge_widgets::tr!("tts_filters_kind_literal"),
            FilterRuleKind::Regex { .. } => forge_widgets::tr!("tts_filters_kind_regex"),
            FilterRuleKind::Blocklist { .. } => forge_widgets::tr!("tts_filters_kind_blocklist"),
        }
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

fn draft_form_view<'a>(
    draft: &'a RuleDraft,
    palette: &'a ForgePalette,
    gap_sm: f32,
) -> Element<'a, Message> {
    let kind_btn = |label: String, kind: DraftKind| {
        let active = draft.kind == kind;
        button(text(label).size(FONT_XS))
            .on_press(Message::Tts(TtsMsg::Filters(
                TtsFiltersMsg::DraftKindChanged(kind),
            )))
            .style(move |_, _| {
                if active {
                    button::Style {
                        background: Some(Background::Color(palette.brand)),
                        border: Border {
                            radius: radius(Radius::Sm).into(),
                            ..Border::default()
                        },
                        text_color: palette.shell,
                        ..button::Style::default()
                    }
                } else {
                    button::Style {
                        background: None,
                        text_color: palette.text_secondary,
                        ..button::Style::default()
                    }
                }
            })
            .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
    };

    let kind_row = row![
        kind_btn(
            forge_widgets::tr!("tts_filters_kind_literal"),
            DraftKind::Literal
        ),
        kind_btn(
            forge_widgets::tr!("tts_filters_kind_regex"),
            DraftKind::Regex
        ),
        kind_btn(
            forge_widgets::tr!("tts_filters_kind_blocklist"),
            DraftKind::Blocklist
        ),
    ]
    .spacing(gap_sm);

    let name_input = forge_widgets::text_input_field(
        forge_widgets::tr!("tts_filters_draft_name_placeholder"),
        &draft.name,
        |s| Message::Tts(TtsMsg::Filters(TtsFiltersMsg::DraftNameChanged(s))),
        palette,
    );

    let params: Element<'a, Message> = match draft.kind {
        DraftKind::Literal | DraftKind::Regex => column![
            forge_widgets::text_input_field(
                forge_widgets::tr!("tts_filters_draft_pattern_placeholder"),
                &draft.pattern,
                |s| Message::Tts(TtsMsg::Filters(TtsFiltersMsg::DraftPatternChanged(s))),
                palette,
            ),
            forge_widgets::text_input_field(
                forge_widgets::tr!("tts_filters_draft_replacement_placeholder"),
                &draft.replacement,
                |s| Message::Tts(TtsMsg::Filters(TtsFiltersMsg::DraftReplacementChanged(s))),
                palette,
            ),
        ]
        .spacing(gap_sm)
        .into(),
        DraftKind::Blocklist => column![
            forge_widgets::text_input_field(
                forge_widgets::tr!("tts_filters_draft_words_placeholder"),
                &draft.words,
                |s| Message::Tts(TtsMsg::Filters(TtsFiltersMsg::DraftWordsChanged(s))),
                palette,
            ),
            blocklist_mode_toggle(
                draft.blocklist_mode,
                |m| Message::Tts(TtsMsg::Filters(TtsFiltersMsg::DraftBlocklistModeChanged(m))),
                palette,
            ),
        ]
        .spacing(gap_sm)
        .into(),
    };

    let submit_label = if draft.editing.is_some() {
        forge_widgets::tr!("common_save")
    } else {
        forge_widgets::tr!("tts_filters_draft_add")
    };
    let actions = row![
        forge_widgets::primary_button(
            submit_label,
            Message::Tts(TtsMsg::Filters(TtsFiltersMsg::DraftSubmit)),
            palette,
        ),
        forge_widgets::secondary_button(
            forge_widgets::tr!("common_cancel"),
            Message::Tts(TtsMsg::Filters(TtsFiltersMsg::DraftCancel)),
            palette,
        ),
    ]
    .spacing(gap_sm);

    section_card(
        forge_widgets::tr!("tts_filters_draft_header"),
        column![kind_row, name_input, params, actions]
            .spacing(gap_sm)
            .into(),
        palette,
        gap_sm,
    )
}

fn blocklist_mode_toggle<'a>(
    current: BlocklistMode,
    on_change: impl Fn(BlocklistMode) -> Message + 'a + Copy,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let mode_btn = move |label: String, mode: BlocklistMode| {
        let active = current == mode;
        button(text(label).size(FONT_XS))
            .on_press(on_change(mode))
            .style(move |_, _| {
                if active {
                    button::Style {
                        background: Some(Background::Color(palette.warning)),
                        border: Border {
                            radius: radius(Radius::Sm).into(),
                            ..Border::default()
                        },
                        text_color: palette.shell,
                        ..button::Style::default()
                    }
                } else {
                    button::Style {
                        background: None,
                        text_color: palette.text_secondary,
                        ..button::Style::default()
                    }
                }
            })
            .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
    };

    container(
        row![
            mode_btn(
                forge_widgets::tr!("tts_filters_mode_censor"),
                BlocklistMode::Censor
            ),
            mode_btn(
                forge_widgets::tr!("tts_filters_mode_skip"),
                BlocklistMode::Suppress
            ),
        ]
        .spacing(0),
    )
    .style(move |_| container::Style {
        background: Some(Background::Color(palette.shell)),
        border: Border {
            color: palette.border_regular,
            width: BORDER_THIN,
            radius: radius(Radius::Sm).into(),
        },
        ..container::Style::default()
    })
    .padding(sp(Spacing::Xxs))
    .into()
}

fn settings_section_view<'a>(
    state: &'a TtsFiltersState,
    palette: &'a ForgePalette,
    gap_sm: f32,
) -> Element<'a, Message> {
    let url_options: Vec<String> = vec![
        forge_widgets::tr!("tts_filters_url_speak"),
        forge_widgets::tr!("tts_filters_url_replace"),
        forge_widgets::tr!("tts_filters_url_suppress"),
    ];
    let url_selected = Some(match state.settings.url_mode {
        UrlMode::Speak => url_options[0].clone(),
        UrlMode::Replace => url_options[1].clone(),
        UrlMode::Suppress => url_options[2].clone(),
    });
    let url_o0 = url_options[0].clone();
    let url_o1 = url_options[1].clone();
    let url_picker = forge_widgets::select_owned(
        url_options,
        url_selected,
        String::new(),
        move |chosen| {
            let mode = if chosen == url_o0 {
                UrlMode::Speak
            } else if chosen == url_o1 {
                UrlMode::Replace
            } else {
                UrlMode::Suppress
            };
            Message::Tts(TtsMsg::Filters(TtsFiltersMsg::UrlModeChanged(mode)))
        },
        palette,
    );

    let length_input = forge_widgets::text_input_field(
        forge_widgets::tr!("tts_filters_length_placeholder"),
        &state.max_length_input,
        |s| Message::Tts(TtsMsg::Filters(TtsFiltersMsg::MaxLengthChanged(s))),
        palette,
    );

    let twitch_toggle = forge_widgets::toggle(
        palette,
        forge_widgets::ToggleProps {
            value: state.settings.strip_twitch_emotes,
            label: forge_widgets::tr!("tts_filters_strip_twitch"),
            description: String::new(),
            on_toggle: Message::Tts(TtsMsg::Filters(TtsFiltersMsg::StripTwitchEmotesToggled(
                !state.settings.strip_twitch_emotes,
            ))),
        },
    );
    let reward_toggle = forge_widgets::toggle(
        palette,
        forge_widgets::ToggleProps {
            value: state.settings.strip_reward_emotes,
            label: forge_widgets::tr!("tts_filters_strip_reward"),
            description: String::new(),
            on_toggle: Message::Tts(TtsMsg::Filters(TtsFiltersMsg::StripRewardEmotesToggled(
                !state.settings.strip_reward_emotes,
            ))),
        },
    );

    let blocklist_default = blocklist_mode_toggle(
        state.settings.blocklist_mode,
        |m| {
            Message::Tts(TtsMsg::Filters(
                TtsFiltersMsg::SettingsBlocklistModeChanged(m),
            ))
        },
        palette,
    );

    let body = column![
        labeled(
            forge_widgets::tr!("tts_filters_url_label"),
            url_picker,
            palette,
            gap_sm
        ),
        labeled(
            forge_widgets::tr!("tts_filters_length_label"),
            length_input,
            palette,
            gap_sm
        ),
        labeled(
            forge_widgets::tr!("tts_filters_blocklist_default_label"),
            blocklist_default,
            palette,
            gap_sm
        ),
        twitch_toggle,
        reward_toggle,
    ]
    .spacing(gap_sm);

    section_card(
        forge_widgets::tr!("tts_filters_settings_header"),
        body.into(),
        palette,
        gap_sm,
    )
}

fn labeled<'a>(
    label: String,
    field: Element<'a, Message>,
    palette: &'a ForgePalette,
    gap_sm: f32,
) -> Element<'a, Message> {
    let _ = gap_sm;
    column![
        text(label)
            .size(FONT_XS)
            .color(palette.text_muted)
            .font(font(FontRole::Monospace)),
        field,
    ]
    .spacing(spf(Spacing::Xxs))
    .into()
}

fn save_bar_view<'a>(
    state: &'a TtsFiltersState,
    palette: &'a ForgePalette,
    gap_sm: f32,
) -> Element<'a, Message> {
    let mut items: Vec<Element<'a, Message>> = Vec::new();
    if let Some(err) = &state.save_error {
        items.push(
            container(
                text(err.clone())
                    .size(FONT_XS)
                    .color(palette.random)
                    .font(font(FontRole::Monospace)),
            )
            .style(move |_| container::Style {
                background: Some(Background::Color(Color {
                    a: 0.1,
                    ..palette.random
                })),
                border: Border {
                    color: palette.random,
                    width: BORDER_THIN,
                    radius: radius(Radius::Sm).into(),
                },
                ..container::Style::default()
            })
            .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
            .width(Length::Fill)
            .into(),
        );
    }

    let dirty_label = if state.dirty {
        forge_widgets::tr!("tts_filters_unsaved")
    } else {
        forge_widgets::tr!("tts_filters_saved")
    };
    let dirty_color = if state.dirty {
        palette.warning
    } else {
        palette.text_muted
    };

    let save_btn: Element<'a, Message> = if state.dirty {
        forge_widgets::primary_button(
            forge_widgets::tr!("common_save"),
            Message::Tts(TtsMsg::Filters(TtsFiltersMsg::Save)),
            palette,
        )
    } else {
        button(
            text(forge_widgets::tr!("common_save"))
                .size(FONT_SM)
                .color(palette.text_faint),
        )
        .style(move |_, _| button::Style {
            background: None,
            border: Border {
                color: palette.border_regular,
                width: BORDER_THIN,
                radius: radius(Radius::Md).into(),
            },
            text_color: palette.text_faint,
            ..button::Style::default()
        })
        .padding([sp(Spacing::Sm), sp(Spacing::Md)])
        .into()
    };

    items.push(
        row![
            text(dirty_label).size(FONT_XS).color(dirty_color),
            Space::new().width(Length::Fill),
            save_btn,
        ]
        .align_y(Alignment::Center)
        .spacing(gap_sm)
        .into(),
    );

    column(items).spacing(gap_sm).into()
}

fn preview_column_view<'a>(
    state: &'a TtsFiltersState,
    palette: &'a ForgePalette,
    gap_sm: f32,
    gap_md: f32,
) -> Element<'a, Message> {
    let header = text(forge_widgets::tr!("tts_filters_preview_header"))
        .size(FONT_XS)
        .color(palette.text_muted)
        .font(font(FontRole::Monospace));

    let input_label = text(forge_widgets::tr!("tts_filters_preview_input_label"))
        .size(FONT_XS)
        .color(palette.text_muted)
        .font(font(FontRole::Monospace));

    let input_box = text_input(
        &forge_widgets::tr!("tts_filters_preview_input_placeholder"),
        &state.preview_input,
    )
    .on_input(|s| Message::Tts(TtsMsg::Filters(TtsFiltersMsg::PreviewInputChanged(s))))
    .size(FONT_SM)
    .width(Length::Fill)
    .style(move |_, _| text_input::Style {
        background: Background::Color(palette.elevated),
        border: Border {
            color: palette.border_regular,
            width: BORDER_THIN,
            radius: radius(Radius::Sm).into(),
        },
        icon: palette.text_muted,
        placeholder: palette.text_muted,
        value: palette.text_primary,
        selection: palette.brand,
    });

    let stage_results = if let Some(preview) = &state.cached_preview {
        preview_stage_rows(preview, palette, gap_sm)
    } else {
        text(forge_widgets::tr!("tts_filters_preview_empty"))
            .size(FONT_SM)
            .color(palette.text_muted)
            .into()
    };

    let output_label = text(forge_widgets::tr!("tts_filters_preview_output_label"))
        .size(FONT_XS)
        .color(palette.text_muted)
        .font(font(FontRole::Monospace));

    let output_text = if let Some(preview) = &state.cached_preview {
        match &preview.result {
            PipelineResult::Speak(s) => s.as_str(),
            PipelineResult::Skip { .. } => "[message would be skipped]",
        }
    } else {
        "\u{2014}"
    };

    let output_border_color = if state
        .cached_preview
        .as_ref()
        .map(|p| matches!(p.result, PipelineResult::Speak(_)))
        .unwrap_or(false)
    {
        palette.success
    } else {
        palette.border_regular
    };

    let output_box = container(
        text(output_text)
            .size(FONT_SM)
            .color(palette.text_primary)
            .font(font(FontRole::Monospace))
            .width(Length::Fill),
    )
    .style(move |_| container::Style {
        background: Some(Background::Color(palette.elevated)),
        border: Border {
            color: output_border_color,
            width: BORDER_THIN,
            radius: radius(Radius::Sm).into(),
        },
        ..container::Style::default()
    })
    .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
    .width(Length::Fill);

    let speak_btn = button(text(forge_widgets::tr!("tts_filters_speak_preview_btn")).size(FONT_SM))
        .on_press(Message::Tts(TtsMsg::Filters(TtsFiltersMsg::SpeakPreview)))
        .style(move |_, _| button::Style {
            background: Some(Background::Color(palette.brand)),
            border: Border {
                radius: radius(Radius::Sm).into(),
                ..Border::default()
            },
            text_color: palette.shell,
            ..button::Style::default()
        })
        .padding([sp(Spacing::Xs), 0])
        .width(Length::Fill);

    let tip = container(
        text(forge_widgets::tr!("tts_filters_preview_tip"))
            .size(FONT_XS)
            .color(palette.text_muted),
    )
    .style(move |_| container::Style {
        background: Some(Background::Color(palette.elevated)),
        border: Border {
            color: palette.border_regular,
            width: BORDER_THIN,
            radius: radius(Radius::Sm).into(),
        },
        ..container::Style::default()
    })
    .padding([sp(Spacing::Xs), sp(Spacing::Xs)])
    .width(Length::Fill);

    container(
        scrollable(
            column![
                header,
                column![input_label, input_box].spacing(spf(Spacing::Xxs)),
                stage_results,
                column![output_label, output_box].spacing(spf(Spacing::Xxs)),
                speak_btn,
                tip,
            ]
            .spacing(gap_md)
            .width(Length::Fill),
        )
        .height(Length::Fill),
    )
    .style(move |_| container::Style {
        background: Some(Background::Color(palette.shell)),
        border: Border {
            color: palette.border_regular,
            width: BORDER_THIN,
            radius: iced::border::Radius::default(),
        },
        ..container::Style::default()
    })
    .padding([sp(Spacing::Md), sp(Spacing::Md)])
    .width(300)
    .into()
}

fn preview_stage_rows<'a>(
    preview: &'a CachedPreview,
    palette: &'a ForgePalette,
    gap_sm: f32,
) -> Element<'a, Message> {
    let _ = gap_sm;
    let cards: Vec<Element<'a, Message>> = preview
        .stages
        .iter()
        .enumerate()
        .map(|(i, outcome)| {
            let label = forge_widgets::tr!("tts_filters_stage_n", n = (i + 1) as i64);
            preview_stage_card(label, outcome, palette)
        })
        .collect();

    column(cards).spacing(spf(Spacing::Xs)).into()
}

fn preview_stage_card<'a>(
    label: String,
    outcome: &'a StageOutcome,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let p = *palette;

    let label_el = text(label)
        .size(FONT_XS)
        .color(p.text_faint)
        .font(font(FontRole::Monospace));

    let body_el: Element<'a, Message> = match &outcome.action {
        StageAction::PassedThrough => row![
            text("\u{2713}").size(FONT_SM).color(p.success),
            text(" pass").size(FONT_SM).color(p.text_primary),
        ]
        .spacing(spf(Spacing::Xxs))
        .align_y(Alignment::Center)
        .into(),
        StageAction::Transformed => text(outcome.output.clone())
            .size(FONT_SM)
            .color(p.text_primary)
            .into(),
        StageAction::Skipped { reason } => row![
            text("\u{d7}").size(FONT_SM).color(p.random),
            text(format!(" skipped — {:?}", reason))
                .size(FONT_SM)
                .color(p.text_primary),
        ]
        .spacing(spf(Spacing::Xxs))
        .align_y(Alignment::Center)
        .into(),
    };

    container(column![label_el, body_el].spacing(spf(Spacing::Xxs)))
        .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
        .width(Length::Fill)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(p.elevated)),
            border: Border {
                color: p.border_regular,
                width: BORDER_THIN,
                radius: radius(Radius::Sm).into(),
            },
            ..container::Style::default()
        })
        .into()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::sync::Arc;

    use forge_storage::{
        CredentialsRepo, DataProvider, FilterRule, FilterRuleKind, TtsPipelineSettings,
    };
    use forge_storage_sqlite::SqliteBackend;

    use crate::message::TtsFiltersMsg;
    use crate::runtime_view::RuntimeView;
    use crate::tts_filters::{DraftKind, TtsFiltersState, update};

    // -------------------------------------------------------------------------
    // Helpers
    // -------------------------------------------------------------------------

    fn make_literal(name: &str, pattern: &str, replacement: &str, pos: u32) -> FilterRule {
        FilterRule {
            id: format!("rule-{pos}"),
            name: name.to_owned(),
            enabled: true,
            position: pos,
            kind: FilterRuleKind::Literal {
                pattern: pattern.to_owned(),
                replacement: replacement.to_owned(),
            },
        }
    }

    fn make_regex(name: &str, pattern: &str, replacement: &str, pos: u32) -> FilterRule {
        FilterRule {
            id: format!("rule-regex-{pos}"),
            name: name.to_owned(),
            enabled: true,
            position: pos,
            kind: FilterRuleKind::Regex {
                pattern: pattern.to_owned(),
                replacement: replacement.to_owned(),
            },
        }
    }

    /// Minimal RuntimeView backed by an in-memory SQLite database.
    fn test_rt() -> RuntimeView {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let backend: Arc<dyn DataProvider> = Arc::new(
            tokio_rt
                .block_on(SqliteBackend::open_with_key("sqlite::memory:", [0xab; 32]))
                .unwrap(),
        );
        let server_subsystem = Arc::new(crate::server_subsystem::ServerSubsystem::new(Arc::clone(
            &backend,
        )
            as Arc<dyn CredentialsRepo>));
        RuntimeView {
            actions: Arc::new(forge_runtime::actions::ActionsService::new(
                backend.action_repo(),
                backend.queue_repo(),
                backend.history_repo(),
                backend.trigger_instance_repo(),
                backend.soundboard_clips_repo(),
            )),
            backend,
            bus: forge_runtime::EventBus::new(Arc::new(forge_runtime::NullEventLogRepo)),
            script_registry: Arc::new(forge_runtime::ScriptRegistry::new()),
            server_subsystem,
            action_engine: None,
            scheduler: None,
            obs_client: None,
            vtube_client: None,
            vtube_sink: forge_vtube::SwitchableVTubeSink::new(),
            obs_sink: forge_obs::SwitchableObsSink::new(),
            discord_client: None,
            midi_client: None,
            hotkey_client: None,
            speak_queue: None,
            pipeline_config: None,
            tts_trigger_settings: None,
            sound_player: None,
            twitch_builtin: None,
            kick_builtin: None,
            platform_connection: std::collections::BTreeMap::new(),
            twitch_flow: None,
            youtube_flow: None,
            kick_flow: None,
            tts_engine_ids: Vec::new(),
            tts_registry: None,
            twitch_login: None,
            twitch_token_expires: None,
            twitch_reauth_required: false,
            sub_action_registry: Arc::new(forge_registry::SubActionRegistry::new()),
            trigger_registry: Arc::new(forge_registry::TriggerRegistry::new()),
        }
    }

    fn state_with(rules: Vec<FilterRule>) -> TtsFiltersState {
        let mut s = TtsFiltersState::new();
        s.rules = rules;
        s.rules
            .iter_mut()
            .enumerate()
            .for_each(|(i, r)| r.position = i as u32);
        s
    }

    // -------------------------------------------------------------------------
    // Reorder bounds
    // -------------------------------------------------------------------------

    #[test]
    fn move_up_at_index_zero_is_a_noop() {
        let rt = test_rt();
        let mut state = state_with(vec![
            make_literal("A", "a", "", 0),
            make_literal("B", "b", "", 1),
        ]);
        let _ = update(&mut state, &rt, TtsFiltersMsg::MoveRuleUp(0));
        assert_eq!(state.rules[0].name, "A");
        assert_eq!(state.rules[1].name, "B");
        assert!(!state.dirty, "no-op move must not set dirty");
    }

    #[test]
    fn move_down_at_last_index_is_a_noop() {
        let rt = test_rt();
        let mut state = state_with(vec![
            make_literal("A", "a", "", 0),
            make_literal("B", "b", "", 1),
        ]);
        let _ = update(&mut state, &rt, TtsFiltersMsg::MoveRuleDown(1));
        assert_eq!(state.rules[0].name, "A");
        assert_eq!(state.rules[1].name, "B");
        assert!(!state.dirty, "no-op move must not set dirty");
    }

    #[test]
    fn move_up_swaps_rule_with_predecessor_and_renumbers() {
        let rt = test_rt();
        let mut state = state_with(vec![
            make_literal("A", "a", "", 0),
            make_literal("B", "b", "", 1),
            make_literal("C", "c", "", 2),
        ]);
        let _ = update(&mut state, &rt, TtsFiltersMsg::MoveRuleUp(2));
        assert_eq!(state.rules[0].name, "A");
        assert_eq!(state.rules[1].name, "C");
        assert_eq!(state.rules[2].name, "B");
        assert_eq!(state.rules[0].position, 0);
        assert_eq!(state.rules[1].position, 1);
        assert_eq!(state.rules[2].position, 2);
        assert!(state.dirty);
    }

    #[test]
    fn move_down_swaps_rule_with_successor_and_renumbers() {
        let rt = test_rt();
        let mut state = state_with(vec![
            make_literal("A", "a", "", 0),
            make_literal("B", "b", "", 1),
            make_literal("C", "c", "", 2),
        ]);
        let _ = update(&mut state, &rt, TtsFiltersMsg::MoveRuleDown(0));
        assert_eq!(state.rules[0].name, "B");
        assert_eq!(state.rules[1].name, "A");
        assert_eq!(state.rules[2].name, "C");
        assert_eq!(state.rules[0].position, 0);
        assert_eq!(state.rules[1].position, 1);
        assert_eq!(state.rules[2].position, 2);
        assert!(state.dirty);
    }

    // -------------------------------------------------------------------------
    // Delete / Toggle
    // -------------------------------------------------------------------------

    #[test]
    fn delete_rule_removes_the_correct_row() {
        let rt = test_rt();
        let mut state = state_with(vec![
            make_literal("A", "a", "", 0),
            make_literal("B", "b", "", 1),
            make_literal("C", "c", "", 2),
        ]);
        let _ = update(&mut state, &rt, TtsFiltersMsg::DeleteRule(1));
        assert_eq!(state.rules.len(), 2);
        assert_eq!(state.rules[0].name, "A");
        assert_eq!(state.rules[1].name, "C");
        assert!(state.dirty);
    }

    #[test]
    fn delete_rule_out_of_bounds_is_a_noop() {
        let rt = test_rt();
        let mut state = state_with(vec![make_literal("A", "a", "", 0)]);
        let _ = update(&mut state, &rt, TtsFiltersMsg::DeleteRule(5));
        assert_eq!(state.rules.len(), 1);
        assert!(!state.dirty);
    }

    #[test]
    fn toggle_rule_flips_only_that_rows_enabled_flag() {
        let rt = test_rt();
        let mut state = state_with(vec![
            make_literal("A", "a", "", 0),
            make_literal("B", "b", "", 1),
        ]);
        let _ = update(&mut state, &rt, TtsFiltersMsg::ToggleRule(0));
        assert!(!state.rules[0].enabled, "row 0 must be disabled");
        assert!(state.rules[1].enabled, "row 1 must remain enabled");
        assert!(state.dirty);
    }

    #[test]
    fn toggle_rule_twice_restores_original_enabled_state() {
        let rt = test_rt();
        let mut state = state_with(vec![make_literal("A", "a", "", 0)]);
        let _ = update(&mut state, &rt, TtsFiltersMsg::ToggleRule(0));
        let _ = update(&mut state, &rt, TtsFiltersMsg::ToggleRule(0));
        assert!(state.rules[0].enabled);
    }

    // -------------------------------------------------------------------------
    // Draft submit — append
    // -------------------------------------------------------------------------

    #[test]
    fn draft_submit_literal_appends_correct_kind() {
        let rt = test_rt();
        let mut state = TtsFiltersState::new();
        let _ = update(&mut state, &rt, TtsFiltersMsg::AddRuleClicked);
        let _ = update(
            &mut state,
            &rt,
            TtsFiltersMsg::DraftKindChanged(DraftKind::Literal),
        );
        let _ = update(
            &mut state,
            &rt,
            TtsFiltersMsg::DraftNameChanged("my-literal".to_owned()),
        );
        let _ = update(
            &mut state,
            &rt,
            TtsFiltersMsg::DraftPatternChanged("hello".to_owned()),
        );
        let _ = update(
            &mut state,
            &rt,
            TtsFiltersMsg::DraftReplacementChanged("hi".to_owned()),
        );
        let _ = update(&mut state, &rt, TtsFiltersMsg::DraftSubmit);

        assert_eq!(state.rules.len(), 1);
        assert_eq!(state.rules[0].name, "my-literal");
        assert!(
            matches!(
                &state.rules[0].kind,
                FilterRuleKind::Literal { pattern, replacement }
                    if pattern == "hello" && replacement == "hi"
            ),
            "expected Literal kind with correct fields"
        );
        assert!(state.draft.is_none(), "draft must be consumed after submit");
        assert!(state.dirty);
    }

    #[test]
    fn draft_submit_regex_appends_correct_kind() {
        let rt = test_rt();
        let mut state = TtsFiltersState::new();
        let _ = update(&mut state, &rt, TtsFiltersMsg::AddRuleClicked);
        let _ = update(
            &mut state,
            &rt,
            TtsFiltersMsg::DraftKindChanged(DraftKind::Regex),
        );
        let _ = update(
            &mut state,
            &rt,
            TtsFiltersMsg::DraftPatternChanged(r"\bworld\b".to_owned()),
        );
        let _ = update(
            &mut state,
            &rt,
            TtsFiltersMsg::DraftReplacementChanged("earth".to_owned()),
        );
        let _ = update(&mut state, &rt, TtsFiltersMsg::DraftSubmit);

        assert_eq!(state.rules.len(), 1);
        assert!(
            matches!(
                &state.rules[0].kind,
                FilterRuleKind::Regex { pattern, .. } if pattern == r"\bworld\b"
            ),
            "expected Regex kind"
        );
    }

    #[test]
    fn draft_submit_blocklist_parses_comma_separated_words() {
        let rt = test_rt();
        let mut state = TtsFiltersState::new();
        let _ = update(&mut state, &rt, TtsFiltersMsg::AddRuleClicked);
        let _ = update(
            &mut state,
            &rt,
            TtsFiltersMsg::DraftKindChanged(DraftKind::Blocklist),
        );
        let _ = update(
            &mut state,
            &rt,
            TtsFiltersMsg::DraftWordsChanged("foo, bar ,  baz".to_owned()),
        );
        let _ = update(&mut state, &rt, TtsFiltersMsg::DraftSubmit);

        assert_eq!(state.rules.len(), 1);
        assert!(
            matches!(&state.rules[0].kind, FilterRuleKind::Blocklist { .. }),
            "expected Blocklist kind"
        );
        if let FilterRuleKind::Blocklist { words, .. } = &state.rules[0].kind {
            assert_eq!(words, &["foo", "bar", "baz"]);
        }
    }

    #[test]
    fn draft_submit_while_editing_replaces_existing_row_not_append() {
        let rt = test_rt();
        let mut state = state_with(vec![
            make_literal("A", "a", "", 0),
            make_literal("B", "b", "", 1),
        ]);
        // edit row 0
        let _ = update(&mut state, &rt, TtsFiltersMsg::EditRule(0));
        let _ = update(
            &mut state,
            &rt,
            TtsFiltersMsg::DraftKindChanged(DraftKind::Literal),
        );
        let _ = update(
            &mut state,
            &rt,
            TtsFiltersMsg::DraftNameChanged("A-edited".to_owned()),
        );
        let _ = update(
            &mut state,
            &rt,
            TtsFiltersMsg::DraftPatternChanged("edited".to_owned()),
        );
        let _ = update(
            &mut state,
            &rt,
            TtsFiltersMsg::DraftReplacementChanged("done".to_owned()),
        );
        let _ = update(&mut state, &rt, TtsFiltersMsg::DraftSubmit);

        assert_eq!(state.rules.len(), 2);
        assert_eq!(state.rules[0].name, "A-edited");
        assert_eq!(state.rules[1].name, "B");
    }

    #[test]
    fn draft_cancel_clears_draft_without_mutating_rules() {
        let rt = test_rt();
        let mut state = state_with(vec![make_literal("A", "a", "", 0)]);
        let _ = update(&mut state, &rt, TtsFiltersMsg::AddRuleClicked);
        assert!(
            state.draft.is_some(),
            "draft must be open after AddRuleClicked"
        );
        let _ = update(&mut state, &rt, TtsFiltersMsg::DraftCancel);
        assert!(
            state.draft.is_none(),
            "draft must be cleared by DraftCancel"
        );
        assert_eq!(state.rules.len(), 1, "rules must not be mutated by cancel");
        assert!(!state.dirty, "cancel must not set dirty when no prior edit");
    }

    // -------------------------------------------------------------------------
    // Save validation — sync branch only
    // -------------------------------------------------------------------------

    #[test]
    fn save_with_invalid_regex_sets_save_error() {
        // Why: build_config_strict rejects before any persist call — the early-return
        // path is the only observable behaviour testable without a real repo.
        let rt = test_rt();
        let mut state = state_with(vec![make_regex(
            "bad-regex",
            "[unclosed", // deliberately invalid regex
            "",
            0,
        )]);
        let _ = update(&mut state, &rt, TtsFiltersMsg::Save);
        assert!(
            state.save_error.is_some(),
            "save_error must be set for an invalid regex pattern"
        );
    }

    #[test]
    fn save_with_valid_rules_clears_pre_existing_save_error() {
        // Pre-existing save_error from a previous failed attempt must be cleared
        // when build_config_strict succeeds on the new rule set.
        let rt = test_rt();
        let mut state = state_with(vec![make_literal("ok", "hello", "hi", 0)]);
        state.save_error = Some("previous error".to_owned());
        let _ = update(&mut state, &rt, TtsFiltersMsg::Save);
        assert!(
            state.save_error.is_none(),
            "save_error must be cleared when build_config_strict succeeds"
        );
    }

    // -------------------------------------------------------------------------
    // Loaded handler
    // -------------------------------------------------------------------------

    #[test]
    fn loaded_ok_sorts_by_position_ascending_and_resets_dirty() {
        let rt = test_rt();
        let mut state = TtsFiltersState::new();
        state.dirty = true;
        // deliver rules in reverse position order to exercise the sort
        let rules = vec![
            make_literal("C", "c", "", 2),
            make_literal("A", "a", "", 0),
            make_literal("B", "b", "", 1),
        ];
        let settings = TtsPipelineSettings::default();
        let _ = update(
            &mut state,
            &rt,
            TtsFiltersMsg::Loaded(Ok((rules, settings))),
        );
        assert_eq!(state.rules[0].name, "A");
        assert_eq!(state.rules[1].name, "B");
        assert_eq!(state.rules[2].name, "C");
        assert!(!state.dirty, "Loaded(Ok) must reset dirty to false");
        assert!(
            state.save_error.is_none(),
            "Loaded(Ok) must clear save_error"
        );
    }

    #[test]
    fn loaded_err_does_not_panic_and_leaves_existing_rules_intact() {
        // Why: a transient storage failure must surface as a warning, not a panic,
        // and must not wipe the previously-loaded rule list.
        let rt = test_rt();
        let mut state = state_with(vec![make_literal("X", "x", "", 0)]);
        let _ = update(
            &mut state,
            &rt,
            TtsFiltersMsg::Loaded(Err("db error".to_owned())),
        );
        assert_eq!(
            state.rules.len(),
            1,
            "rules must be untouched after Loaded(Err)"
        );
    }
}
