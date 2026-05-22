use forge_tts_pipeline::{
    BlocklistMode, PipelineConfig, PipelineResult, StageAction, StageOutcome, UrlMode,
};
use forge_widgets::tokens::{
    BORDER_THIN, Density, FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius, spacing,
};
use forge_widgets::{ForgePalette, Icon, tabler_icon};
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Background, Border, Color, Element, Length, Task};

use crate::Message;
use crate::message::{TtsFiltersMsg, TtsMsg};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlocklistModeChoice {
    Censor,
    SkipMessage,
}

pub struct ReplacementRuleRow {
    pub is_regex: bool,
    pub pattern: String,
    pub replacement: String,
}

pub struct TtsFiltersState {
    pub preview_input: String,
    pub skip_contains_url: bool,
    pub skip_starts_with_bang: bool,
    pub skip_from_bots: bool,
    pub skip_length_limit: Option<u32>,
    pub blocklist_mode: BlocklistModeChoice,
    pub word_blocklist: Vec<String>,
    pub replacement_rules: Vec<ReplacementRuleRow>,
    pub cached_preview: Option<CachedPreview>,
}

pub struct CachedPreview {
    pub stages: Vec<StageOutcome>,
    pub result: PipelineResult,
}

impl TtsFiltersState {
    pub fn new() -> Self {
        Self {
            preview_input: String::new(),
            skip_contains_url: true,
            skip_starts_with_bang: true,
            skip_from_bots: true,
            skip_length_limit: Some(300),
            blocklist_mode: BlocklistModeChoice::Censor,
            word_blocklist: Vec::new(),
            replacement_rules: Vec::new(),
            cached_preview: None,
        }
    }

    fn build_pipeline_config(&self) -> PipelineConfig {
        let url_mode = if self.skip_contains_url {
            UrlMode::SkipMessage
        } else {
            UrlMode::Passthrough
        };
        let blocklist_mode = match self.blocklist_mode {
            BlocklistModeChoice::Censor => BlocklistMode::Censor,
            BlocklistModeChoice::SkipMessage => BlocklistMode::SkipMessage,
        };
        let max_chars = self.skip_length_limit.unwrap_or(500) as usize;
        PipelineConfig {
            emote_sources: forge_tts_pipeline::EmoteSources::default(),
            emote_tokens: forge_tts_pipeline::EmoteTokenSet {
                tokens: std::collections::HashSet::new(),
            },
            url_mode,
            replacement_rules: Vec::new(),
            word_blocklist: self.word_blocklist.clone(),
            blocklist_mode,
            max_chars,
        }
    }

    fn refresh_preview(&mut self) {
        if self.preview_input.is_empty() {
            self.cached_preview = None;
            return;
        }
        let config = self.build_pipeline_config();
        let (result, stages) = forge_tts_pipeline::preview(&self.preview_input, &config);
        self.cached_preview = Some(CachedPreview { stages, result });
    }
}

impl Default for TtsFiltersState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn handle_tts_filters_msg(state: &mut TtsFiltersState, msg: TtsFiltersMsg) -> Task<Message> {
    match msg {
        TtsFiltersMsg::PreviewInputChanged(s) => {
            state.preview_input = s;
            state.refresh_preview();
            Task::none()
        }
        TtsFiltersMsg::BlocklistModeChanged(m) => {
            state.blocklist_mode = m;
            state.refresh_preview();
            Task::none()
        }
        TtsFiltersMsg::AddRuleClicked => Task::none(),
    }
}

pub fn tts_filters_view<'a>(
    state: &'a TtsFiltersState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let gap_sm = f32::from(spacing(Spacing::Xs, Density::Cozy));
    let gap_md = f32::from(spacing(Spacing::Sm, Density::Cozy));

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
        text("PROCESSING PIPELINE")
            .size(FONT_XS)
            .color(palette.text_muted)
            .font(font(FontRole::Monospace)),
        text("Each message passes through these stages in order before being spoken")
            .size(FONT_SM)
            .color(palette.text_muted),
    ]
    .spacing(4);

    let stage1 = pipeline_stage(
        "1",
        palette.random,
        "Skip rules",
        "message dropped if matched".to_owned(),
        skip_rules_content(state, palette, gap_sm),
        palette,
        true,
    );

    let stage2 = pipeline_stage(
        "2",
        palette.warning,
        "Word blocklist",
        format!("{} words", state.word_blocklist.len()),
        blocklist_content(state, palette, gap_sm),
        palette,
        true,
    );

    let stage3 = pipeline_stage(
        "3",
        palette.brand,
        "Text replacements",
        format!("{} rules", state.replacement_rules.len()),
        replacements_content(state, palette, gap_sm),
        palette,
        true,
    );

    let stage4 = pipeline_stage(
        "\u{2713}",
        palette.success,
        "Sent to voice engine",
        String::new(),
        text("").size(0.0).into(),
        palette,
        false,
    );

    let _ = gap_md;

    scrollable(
        container(column![header, stage1, stage2, stage3, stage4].spacing(0))
            .padding([16, 18])
            .width(Length::Fill),
    )
    .height(Length::Fill)
    .width(Length::Fill)
    .into()
}

fn pipeline_stage<'a>(
    number: &'static str,
    num_color: Color,
    title: &'a str,
    subtitle: String,
    content: Element<'a, Message>,
    palette: &'a ForgePalette,
    has_connector: bool,
) -> Element<'a, Message> {
    let gap_sm = f32::from(spacing(Spacing::Xs, Density::Cozy));
    let badge = container(
        text(number)
            .size(FONT_XS)
            .color(palette.shell)
            .font(font(FontRole::Monospace)),
    )
    .style(move |_| container::Style {
        background: Some(Background::Color(num_color)),
        border: Border {
            radius: radius(Radius::Pill).into(),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .width(22)
    .height(22)
    .align_x(iced::alignment::Horizontal::Center)
    .align_y(iced::alignment::Vertical::Center);

    let connector: Element<'a, Message> = if has_connector {
        container(text(""))
            .style(move |_| container::Style {
                background: Some(Background::Color(palette.border_regular)),
                ..container::Style::default()
            })
            .width(2)
            .height(8)
            .into()
    } else {
        text("").size(0.0).into()
    };

    let left_col = column![badge, connector]
        .align_x(iced::alignment::Horizontal::Center)
        .width(24);

    let stage_header = row![
        text(title)
            .size(FONT_SM)
            .color(palette.text_primary)
            .width(Length::Fill),
        text(subtitle)
            .size(FONT_XS)
            .color(palette.text_muted)
            .font(font(FontRole::Monospace)),
    ]
    .align_y(Alignment::Center);

    let inner = column![stage_header, content].spacing(gap_sm);

    let card = container(inner)
        .style(move |_| container::Style {
            background: Some(Background::Color(palette.elevated)),
            border: Border {
                color: palette.border_regular,
                width: BORDER_THIN,
                radius: radius(Radius::Md).into(),
            },
            ..container::Style::default()
        })
        .padding([11, 13])
        .width(Length::Fill);

    row![left_col, card]
        .spacing(10)
        .padding(iced::Padding {
            top: 0.0,
            right: 0.0,
            bottom: 8.0,
            left: 0.0,
        })
        .into()
}

fn skip_rules_content<'a>(
    state: &'a TtsFiltersState,
    palette: &'a ForgePalette,
    gap_sm: f32,
) -> Element<'a, Message> {
    fn chip<'b>(label: &'static str, palette: &'b ForgePalette) -> Element<'b, Message> {
        container(text(label).size(FONT_XS).color(palette.text_primary))
            .style(move |_| container::Style {
                background: Some(Background::Color(palette.shell)),
                border: Border {
                    color: palette.border_regular,
                    width: BORDER_THIN,
                    radius: radius(Radius::Pill).into(),
                },
                ..container::Style::default()
            })
            .padding([3, 8])
            .into()
    }

    fn add_chip<'b>(palette: &'b ForgePalette) -> Element<'b, Message> {
        button(text("+ Add rule").size(FONT_XS).color(palette.text_muted))
            .on_press(Message::Tts(TtsMsg::Filters(TtsFiltersMsg::AddRuleClicked)))
            .style(move |_, _| button::Style {
                background: None,
                border: Border {
                    color: palette.border_regular,
                    width: BORDER_THIN,
                    radius: radius(Radius::Pill).into(),
                },
                text_color: palette.text_muted,
                ..button::Style::default()
            })
            .padding([3, 8])
            .into()
    }

    let mut chips: Vec<Element<'a, Message>> = Vec::new();
    if state.skip_contains_url {
        chips.push(chip("Contains URL", palette));
    }
    if state.skip_starts_with_bang {
        chips.push(chip("Starts with !", palette));
    }
    if state.skip_from_bots {
        chips.push(chip("From bots", palette));
    }
    if let Some(limit) = state.skip_length_limit {
        chips.push(chip(
            Box::leak(format!("Length > {limit}").into_boxed_str()),
            palette,
        ));
    }
    chips.push(add_chip(palette));

    row(chips).spacing(gap_sm).wrap().into()
}

fn blocklist_content<'a>(
    state: &'a TtsFiltersState,
    palette: &'a ForgePalette,
    gap_sm: f32,
) -> Element<'a, Message> {
    fn mode_btn<'b>(
        label: &'static str,
        choice: BlocklistModeChoice,
        current: &BlocklistModeChoice,
        palette: &'b ForgePalette,
    ) -> Element<'b, Message> {
        let active = &choice == current;
        button(text(label).size(FONT_XS))
            .on_press(Message::Tts(TtsMsg::Filters(
                TtsFiltersMsg::BlocklistModeChanged(choice),
            )))
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
            .padding([4, 9])
            .into()
    }

    let manage_box = container(
        text("Manage blocklist...")
            .size(FONT_SM)
            .color(palette.text_muted),
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
    .padding([6, 10])
    .width(Length::Fill);

    let mode_toggle = container(
        row![
            mode_btn(
                "Censor",
                BlocklistModeChoice::Censor,
                &state.blocklist_mode,
                palette
            ),
            mode_btn(
                "Skip msg",
                BlocklistModeChoice::SkipMessage,
                &state.blocklist_mode,
                palette,
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
    .padding(2);

    row![manage_box, mode_toggle]
        .align_y(Alignment::Center)
        .spacing(gap_sm)
        .into()
}

fn replacements_content<'a>(
    state: &'a TtsFiltersState,
    palette: &'a ForgePalette,
    gap_sm: f32,
) -> Element<'a, Message> {
    if state.replacement_rules.is_empty() {
        return container(
            text("No replacement rules")
                .size(FONT_SM)
                .color(palette.text_muted),
        )
        .padding([4, 0])
        .into();
    }

    let rows: Vec<Element<'a, Message>> = state
        .replacement_rules
        .iter()
        .map(|rule| replacement_rule_row(rule, palette, gap_sm))
        .collect();

    column(rows).spacing(gap_sm).into()
}

fn replacement_rule_row<'a>(
    rule: &'a ReplacementRuleRow,
    palette: &'a ForgePalette,
    gap_sm: f32,
) -> Element<'a, Message> {
    let badge_color = if rule.is_regex {
        palette.brand
    } else {
        palette.info
    };
    let badge_label = if rule.is_regex { "REGEX" } else { "TEXT" };

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
    .padding([1, 5]);

    container(
        row![
            badge,
            text(&rule.pattern)
                .size(FONT_XS)
                .color(palette.text_primary)
                .font(font(FontRole::Monospace)),
            tabler_icon(Icon::ArrowRight, FONT_XS, palette.text_muted),
            text(&rule.replacement)
                .size(FONT_XS)
                .color(palette.success)
                .font(font(FontRole::Monospace))
                .width(Length::Fill),
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
    .padding([6, 9])
    .width(Length::Fill)
    .into()
}

fn preview_column_view<'a>(
    state: &'a TtsFiltersState,
    palette: &'a ForgePalette,
    gap_sm: f32,
    gap_md: f32,
) -> Element<'a, Message> {
    let header = text("PIPELINE PREVIEW")
        .size(FONT_XS)
        .color(palette.text_muted)
        .font(font(FontRole::Monospace));

    let input_label = text("INPUT MESSAGE")
        .size(FONT_XS)
        .color(palette.text_muted)
        .font(font(FontRole::Monospace));

    let input_box = text_input("Type a message to preview...", &state.preview_input)
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
        text("Enter a message above to preview")
            .size(FONT_SM)
            .color(palette.text_muted)
            .into()
    };

    let output_label = text("FINAL OUTPUT")
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
    .padding([9, 11])
    .width(Length::Fill);

    let speak_btn = button(text("Speak preview").size(FONT_SM))
        .on_press(Message::Noop)
        .style(move |_, _| button::Style {
            background: Some(Background::Color(palette.brand)),
            border: Border {
                radius: radius(Radius::Sm).into(),
                ..Border::default()
            },
            text_color: palette.shell,
            ..button::Style::default()
        })
        .padding([8, 0])
        .width(Length::Fill);

    let tip = container(
        text("Type any message above to see how filters transform it in real time")
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
    .padding([8, 10])
    .width(Length::Fill);

    container(
        scrollable(
            column![
                header,
                column![input_label, input_box].spacing(5),
                stage_results,
                column![output_label, output_box].spacing(5),
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
    .padding([16, 16])
    .width(300)
    .into()
}

fn preview_stage_rows<'a>(
    preview: &'a CachedPreview,
    palette: &'a ForgePalette,
    gap_sm: f32,
) -> Element<'a, Message> {
    let _ = gap_sm;
    let stage_names = [
        "1 · SKIP RULES",
        "2 · URL CHECK",
        "3 · REPLACEMENTS",
        "4 · BLOCKLIST",
        "5 · LENGTH CAP",
    ];

    let cards: Vec<Element<'a, Message>> = preview
        .stages
        .iter()
        .enumerate()
        .map(|(i, outcome)| {
            let label = stage_names.get(i).copied().unwrap_or("STAGE");
            preview_stage_card(label, outcome, palette)
        })
        .collect();

    column(cards).spacing(6).into()
}

fn preview_stage_card<'a>(
    label: &'a str,
    outcome: &'a StageOutcome,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let p = *palette;

    let label_el = text(label.to_owned())
        .size(FONT_XS)
        .color(p.text_faint)
        .font(font(FontRole::Monospace));

    let body_el: Element<'a, Message> = match &outcome.action {
        StageAction::PassedThrough => row![
            text("\u{2713}").size(FONT_SM).color(p.success),
            text(" pass").size(FONT_SM).color(p.text_primary),
        ]
        .spacing(2)
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
        .spacing(2)
        .align_y(Alignment::Center)
        .into(),
    };

    container(column![label_el, body_el].spacing(3))
        .padding([8_u16, 11_u16])
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
