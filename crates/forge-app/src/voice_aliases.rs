use forge_widgets::ForgePalette;
use forge_widgets::tokens::{
    BORDER_THIN, FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius, sp, spf,
};
use iced::widget::{Space, button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Background, Border, Element, Length, Task};

use crate::Message;
use crate::message::{TtsMsg, VoiceAliasesMsg};
use crate::runtime_view::RuntimeView;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssignmentStrategyChoice {
    DeterministicByName,
    Random,
    SingleVoice,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewerRole {
    Mod,
    Vip,
    Sub,
}

pub struct VoiceAliasRow {
    pub viewer_name: String,
    pub engine_label: String,
    pub voice_label: String,
    pub pitch_semitones: Option<f32>,
    pub rate_multiplier: Option<f32>,
    pub blocked: bool,
    pub role: Option<ViewerRole>,
}

pub struct VoiceAliasesState {
    pub strategy: AssignmentStrategyChoice,
    pub search: String,
    pub aliases: Vec<VoiceAliasRow>,
    pub total_count: usize,
}

impl VoiceAliasesState {
    pub fn new() -> Self {
        Self {
            strategy: AssignmentStrategyChoice::DeterministicByName,
            search: String::new(),
            aliases: Vec::new(),
            total_count: 0,
        }
    }
}

impl Default for VoiceAliasesState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn update(
    state: &mut VoiceAliasesState,
    _rt: &RuntimeView,
    msg: VoiceAliasesMsg,
) -> Task<Message> {
    match msg {
        VoiceAliasesMsg::SearchChanged(s) => {
            state.search = s;
            Task::none()
        }
        VoiceAliasesMsg::StrategyChanged(s) => {
            state.strategy = s;
            Task::none()
        }
    }
}

pub fn voice_aliases_view<'a>(
    state: &'a VoiceAliasesState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let gap_sm = spf(Spacing::Xs);
    let gap_md = spf(Spacing::Sm);
    let gap_lg = spf(Spacing::Sm);

    let strategy_banner = strategy_banner_view(state, palette, gap_sm, gap_md);
    let toolbar = toolbar_view(state, palette, gap_sm);
    let table = aliases_table_view(state, palette, gap_sm);

    column![strategy_banner, toolbar, table]
        .spacing(gap_lg)
        .padding(iced::Padding {
            top: 0.0,
            right: 0.0,
            bottom: spf(Spacing::Md),
            left: 0.0,
        })
        .into()
}

fn strategy_banner_view<'a>(
    state: &'a VoiceAliasesState,
    palette: &'a ForgePalette,
    gap_sm: f32,
    gap_md: f32,
) -> Element<'a, Message> {
    fn strategy_btn<'b>(
        label: &'static str,
        choice: AssignmentStrategyChoice,
        current: &AssignmentStrategyChoice,
        palette: &'b ForgePalette,
    ) -> Element<'b, Message> {
        let active = &choice == current;
        button(text(label).size(FONT_XS))
            .on_press(Message::Tts(TtsMsg::Aliases(
                VoiceAliasesMsg::StrategyChanged(choice),
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
            .padding([sp(Spacing::Xxs), sp(Spacing::Sm)])
            .into()
    }

    let _ = gap_sm;

    let segmented = container(
        row![
            strategy_btn(
                Box::leak(
                    forge_widgets::tr!("tts_aliases_strategy_deterministic").into_boxed_str()
                ),
                AssignmentStrategyChoice::DeterministicByName,
                &state.strategy,
                palette,
            ),
            strategy_btn(
                Box::leak(forge_widgets::tr!("tts_aliases_strategy_random").into_boxed_str()),
                AssignmentStrategyChoice::Random,
                &state.strategy,
                palette
            ),
            strategy_btn(
                Box::leak(forge_widgets::tr!("tts_aliases_strategy_single").into_boxed_str()),
                AssignmentStrategyChoice::SingleVoice,
                &state.strategy,
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
    .padding(sp(Spacing::Xxs));

    let inner = container(
        row![
            text(forge_widgets::tr!("tts_aliases_strategy_label"))
                .size(FONT_SM)
                .color(palette.text_primary)
                .width(Length::Fill),
            segmented,
        ]
        .align_y(Alignment::Center)
        .spacing(gap_md),
    )
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
    .width(Length::Fill);

    container(inner)
        .padding([sp(Spacing::Sm), sp(Spacing::Md)])
        .width(Length::Fill)
        .into()
}

fn toolbar_view<'a>(
    state: &'a VoiceAliasesState,
    palette: &'a ForgePalette,
    gap_sm: f32,
) -> Element<'a, Message> {
    let search_placeholder = forge_widgets::tr!("tts_aliases_search_placeholder");
    let search = text_input(&search_placeholder, &state.search)
        .on_input(|s| Message::Tts(TtsMsg::Aliases(VoiceAliasesMsg::SearchChanged(s))))
        .size(FONT_SM)
        .width(240)
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

    let count_text = text(forge_widgets::tr!(
        "tts_aliases_count",
        count = state.total_count as i64
    ))
    .size(FONT_SM)
    .color(palette.text_muted);

    let assign_btn = button(text(forge_widgets::tr!("tts_aliases_assign_btn")).size(FONT_SM))
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
        .padding([sp(Spacing::Xs), sp(Spacing::Sm)]);

    container(
        row![
            search,
            row![count_text, assign_btn]
                .align_y(Alignment::Center)
                .spacing(gap_sm),
        ]
        .align_y(Alignment::Center)
        .spacing(gap_sm)
        .width(Length::Fill),
    )
    .padding([0, sp(Spacing::Md)])
    .width(Length::Fill)
    .into()
}

fn aliases_table_view<'a>(
    state: &'a VoiceAliasesState,
    palette: &'a ForgePalette,
    gap_sm: f32,
) -> Element<'a, Message> {
    let mono = font(FontRole::Monospace);

    let header = container(
        row![
            text(forge_widgets::tr!("tts_aliases_col_viewer"))
                .size(FONT_XS)
                .color(palette.text_muted)
                .font(mono)
                .width(Length::FillPortion(14)),
            text(forge_widgets::tr!("tts_aliases_col_voice"))
                .size(FONT_XS)
                .color(palette.text_muted)
                .font(mono)
                .width(Length::FillPortion(16)),
            text(forge_widgets::tr!("tts_aliases_col_pitch"))
                .size(FONT_XS)
                .color(palette.text_muted)
                .font(mono)
                .width(Length::FillPortion(8)),
            text(forge_widgets::tr!("tts_aliases_col_speed"))
                .size(FONT_XS)
                .color(palette.text_muted)
                .font(mono)
                .width(Length::FillPortion(8)),
            text(forge_widgets::tr!("tts_aliases_col_actions"))
                .size(FONT_XS)
                .color(palette.text_muted)
                .font(mono)
                .width(90),
        ]
        .align_y(Alignment::Center)
        .spacing(0),
    )
    .style(move |_| container::Style {
        background: Some(Background::Color(palette.shell)),
        border: Border {
            color: palette.border_regular,
            width: BORDER_THIN,
            radius: iced::border::Radius {
                top_left: 8.0,
                top_right: 8.0,
                bottom_left: 0.0,
                bottom_right: 0.0,
            },
        },
        ..container::Style::default()
    })
    .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
    .width(Length::Fill);

    let visible: Vec<&VoiceAliasRow> = state
        .aliases
        .iter()
        .filter(|a| {
            state.search.is_empty()
                || a.viewer_name
                    .to_lowercase()
                    .contains(&state.search.to_lowercase())
        })
        .collect();

    let rows: Element<'a, Message> = if visible.is_empty() {
        container(
            text(forge_widgets::tr!("tts_aliases_empty"))
                .size(FONT_SM)
                .color(palette.text_muted),
        )
        .padding([sp(Spacing::Lg), sp(Spacing::Sm)])
        .width(Length::Fill)
        .into()
    } else {
        let total = visible.len();
        let row_els: Vec<Element<'a, Message>> = visible
            .iter()
            .enumerate()
            .map(|(i, alias)| alias_row(i, alias, palette, gap_sm, total))
            .collect();
        scrollable(column(row_els)).height(Length::Fill).into()
    };

    let body = container(rows)
        .style(move |_| container::Style {
            border: Border {
                color: palette.border_regular,
                width: BORDER_THIN,
                radius: iced::border::Radius {
                    top_left: 0.0,
                    top_right: 0.0,
                    bottom_left: 8.0,
                    bottom_right: 8.0,
                },
            },
            ..container::Style::default()
        })
        .width(Length::Fill);

    container(column![header, body].width(Length::Fill))
        .padding([0, sp(Spacing::Md)])
        .width(Length::Fill)
        .into()
}

fn alias_row<'a>(
    index: usize,
    alias: &'a VoiceAliasRow,
    palette: &'a ForgePalette,
    gap_sm: f32,
    total: usize,
) -> Element<'a, Message> {
    let muted = alias.blocked;
    let mono = font(FontRole::Monospace);
    let text_color = if muted {
        palette.text_muted
    } else {
        palette.text_primary
    };

    let initial = alias
        .viewer_name
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .next()
        .unwrap_or('?');

    let avatar_color = if muted {
        palette.surface_overlay
    } else {
        avatar_color_for(&alias.viewer_name, palette)
    };

    let avatar = container(
        text(initial.to_string())
            .size(FONT_XS)
            .color(if muted {
                palette.text_muted
            } else {
                palette.shell
            })
            .font(mono),
    )
    .style(move |_| container::Style {
        background: Some(Background::Color(avatar_color)),
        border: Border {
            radius: radius(Radius::Sm).into(),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .width(22)
    .height(22)
    .align_x(iced::alignment::Horizontal::Center)
    .align_y(iced::alignment::Vertical::Center);

    let role_badge: Element<'a, Message> = match &alias.role {
        Some(ViewerRole::Mod) => role_badge_el(
            forge_widgets::tr!("tts_aliases_role_mod"),
            palette.warning,
            palette,
        ),
        Some(ViewerRole::Vip) => role_badge_el(
            forge_widgets::tr!("tts_aliases_role_vip"),
            palette.brand,
            palette,
        ),
        Some(ViewerRole::Sub) => role_badge_el(
            forge_widgets::tr!("tts_aliases_role_sub"),
            palette.success,
            palette,
        ),
        None if muted => role_badge_el(
            forge_widgets::tr!("tts_aliases_role_blocked"),
            palette.random,
            palette,
        ),
        None => Space::new().into(),
    };

    let viewer_col = row![
        avatar,
        text(&alias.viewer_name).size(FONT_SM).color(text_color),
        role_badge,
    ]
    .align_y(Alignment::Center)
    .spacing(gap_sm)
    .width(Length::FillPortion(14));

    let voice_col: Element<'a, Message> = if muted {
        text(forge_widgets::tr!("tts_aliases_never_speak"))
            .size(FONT_SM)
            .color(palette.random)
            .font(mono)
            .width(Length::FillPortion(16))
            .into()
    } else {
        text(format!("{} · {}", alias.engine_label, alias.voice_label))
            .size(FONT_SM)
            .color(palette.text_primary)
            .font(mono)
            .width(Length::FillPortion(16))
            .into()
    };

    let pitch_str = if muted {
        "—".to_string()
    } else {
        alias
            .pitch_semitones
            .map(|p| {
                if p >= 0.0 {
                    format!("+{p:.0} st")
                } else {
                    format!("{p:.0} st")
                }
            })
            .unwrap_or_else(|| "0 st".to_string())
    };

    let speed_str = if muted {
        "—".to_string()
    } else {
        alias
            .rate_multiplier
            .map(|r| format!("{r:.1}x"))
            .unwrap_or_else(|| "1.0x".to_string())
    };

    let faint = palette.surface_overlay;

    let pitch_col = text(pitch_str)
        .size(FONT_SM)
        .color(if muted { faint } else { palette.text_muted })
        .font(mono)
        .width(Length::FillPortion(8));

    let speed_col = text(speed_str)
        .size(FONT_SM)
        .color(if muted { faint } else { palette.text_muted })
        .font(mono)
        .width(Length::FillPortion(8));

    let play_color = if muted {
        palette.surface_overlay
    } else {
        palette.success
    };
    let actions = row![
        button(text("▶").size(FONT_SM).color(play_color))
            .on_press(Message::Noop)
            .style(|_, _| button::Style::default())
            .padding(0),
        button(text("✎").size(FONT_SM).color(palette.text_muted))
            .on_press(Message::Noop)
            .style(|_, _| button::Style::default())
            .padding(0),
        button(text("✕").size(FONT_SM).color(palette.text_muted))
            .on_press(Message::Noop)
            .style(|_, _| button::Style::default())
            .padding(0),
    ]
    .align_y(Alignment::Center)
    .spacing(gap_sm)
    .width(90);

    let is_last = index + 1 == total;
    let bg = if index.is_multiple_of(2) {
        palette.elevated
    } else {
        palette.shell
    };

    container(
        row![viewer_col, voice_col, pitch_col, speed_col, actions]
            .align_y(Alignment::Center)
            .spacing(0)
            .width(Length::Fill),
    )
    .style(move |_| container::Style {
        background: Some(Background::Color(bg)),
        border: Border {
            color: palette.border_regular,
            width: if is_last { 0.0 } else { BORDER_THIN },
            radius: iced::border::Radius::default(),
        },
        ..container::Style::default()
    })
    .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
    .width(Length::Fill)
    .into()
}

fn role_badge_el<'a>(
    label: String,
    color: iced::Color,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    container(
        text(label)
            .size(8.5)
            .color(color)
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
    .padding([sp(Spacing::Xxs), sp(Spacing::Xxs)])
    .into()
}

fn avatar_color_for(name: &str, palette: &ForgePalette) -> iced::Color {
    let hash: u32 = name
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
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
