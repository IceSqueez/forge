use forge_widgets::ForgePalette;
use forge_widgets::tokens::{
    BORDER_THIN, FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius, sp, spf,
};
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Background, Border, Color, Element, Length, Task};

use crate::Message;
use crate::message::{TtsEnginesMsg, TtsMsg};
use crate::runtime_view::RuntimeView;

pub struct EngineVoiceRow {
    pub display_name: String,
    pub locale: String,
    pub quality: String,
    pub gender: String,
}

pub struct TtsEnginesState {
    pub selected_engine: String,
    pub voice_search: String,
    pub piper_voices: Vec<EngineVoiceRow>,
}

impl TtsEnginesState {
    pub fn new() -> Self {
        Self {
            selected_engine: "piper".to_owned(),
            voice_search: String::new(),
            piper_voices: vec![
                EngineVoiceRow {
                    display_name: "Lessac".to_owned(),
                    locale: "en-US".to_owned(),
                    quality: "medium".to_owned(),
                    gender: "M".to_owned(),
                },
                EngineVoiceRow {
                    display_name: "Amy".to_owned(),
                    locale: "en-US".to_owned(),
                    quality: "medium".to_owned(),
                    gender: "F".to_owned(),
                },
                EngineVoiceRow {
                    display_name: "Lada".to_owned(),
                    locale: "uk-UA".to_owned(),
                    quality: "x_low".to_owned(),
                    gender: "F".to_owned(),
                },
                EngineVoiceRow {
                    display_name: "Thorsten".to_owned(),
                    locale: "de-DE".to_owned(),
                    quality: "medium".to_owned(),
                    gender: "M".to_owned(),
                },
            ],
        }
    }
}

impl Default for TtsEnginesState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn update(state: &mut TtsEnginesState, _rt: &RuntimeView, msg: TtsEnginesMsg) -> Task<Message> {
    match msg {
        TtsEnginesMsg::SelectEngine(id) => {
            state.selected_engine = id;
            Task::none()
        }
    }
}

pub fn tts_engines_view<'a>(
    state: &'a TtsEnginesState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let gap_sm = spf(Spacing::Xs);
    let gap_md = spf(Spacing::Sm);

    let engine_list = engine_list_view(state, palette, gap_sm);
    let detail = engine_detail_view(state, palette, gap_sm, gap_md);

    row![engine_list, detail]
        .spacing(gap_md)
        .padding([sp(Spacing::Md), sp(Spacing::Md)])
        .height(Length::Fill)
        .into()
}

struct StaticEngine {
    id: &'static str,
    name: &'static str,
    kind: &'static str,
    voice_count: u8,
    status_color: fn(&ForgePalette) -> Color,
    is_default: bool,
}

const STATIC_ENGINES: &[StaticEngine] = &[StaticEngine {
    id: "piper",
    name: "Piper",
    kind: "local",
    voice_count: 4,
    status_color: |p| p.success,
    is_default: true,
}];

fn engine_list_view<'a>(
    state: &'a TtsEnginesState,
    palette: &'a ForgePalette,
    gap_sm: f32,
) -> Element<'a, Message> {
    let header = text(format!("CONFIGURED \u{b7} {}", STATIC_ENGINES.len()))
        .size(FONT_XS)
        .color(palette.text_muted)
        .font(font(FontRole::Monospace));

    let engine_cards: Vec<Element<'a, Message>> = STATIC_ENGINES
        .iter()
        .map(|e| engine_list_card(e, state.selected_engine == e.id, palette))
        .collect();

    let placeholder = container(
        text("+ More engines in future releases")
            .size(FONT_SM)
            .color(palette.text_muted),
    )
    .style(move |_| container::Style {
        border: Border {
            color: palette.border_regular,
            width: BORDER_THIN,
            radius: radius(Radius::Md).into(),
        },
        ..container::Style::default()
    })
    .padding([sp(Spacing::Sm), sp(Spacing::Sm)])
    .width(Length::Fill);

    container(
        scrollable(
            column![header]
                .push(column(engine_cards).spacing(gap_sm))
                .push(placeholder)
                .spacing(gap_sm),
        )
        .height(Length::Fill),
    )
    .width(220)
    .into()
}

fn engine_list_card<'a>(
    engine: &'a StaticEngine,
    selected: bool,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let status_dot = container(text(""))
        .style(move |_| container::Style {
            background: Some(Background::Color((engine.status_color)(palette))),
            border: Border {
                radius: radius(Radius::Pill).into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .width(7)
        .height(7);

    let name_row = row![
        text(engine.name)
            .size(FONT_SM)
            .color(palette.text_primary)
            .width(Length::Fill),
        status_dot,
    ]
    .align_y(Alignment::Center);

    let meta = text(format!(
        "{} \u{b7} {} voices",
        engine.kind, engine.voice_count
    ))
    .size(FONT_XS)
    .color(palette.text_muted)
    .font(font(FontRole::Monospace));

    let border_color = if selected {
        palette.brand
    } else {
        palette.border_regular
    };
    let border_width: f32 = if selected { 1.0 } else { BORDER_THIN };

    button(column![name_row, meta].spacing(spf(Spacing::Xxs)))
        .on_press(Message::Tts(TtsMsg::Engines(TtsEnginesMsg::SelectEngine(
            engine.id.to_owned(),
        ))))
        .style(move |_, _| button::Style {
            background: Some(Background::Color(palette.elevated)),
            border: Border {
                color: border_color,
                width: border_width,
                radius: radius(Radius::Md).into(),
            },
            text_color: palette.text_primary,
            ..button::Style::default()
        })
        .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
        .width(Length::Fill)
        .into()
}

fn engine_detail_view<'a>(
    state: &'a TtsEnginesState,
    palette: &'a ForgePalette,
    gap_sm: f32,
    gap_md: f32,
) -> Element<'a, Message> {
    let engine = STATIC_ENGINES
        .iter()
        .find(|e| e.id == state.selected_engine);

    let detail_inner: Element<'a, Message> = if let Some(eng) = engine {
        engine_detail_pane(eng, state, palette, gap_sm, gap_md)
    } else {
        container(
            text("Select an engine to configure")
                .size(FONT_SM)
                .color(palette.text_muted),
        )
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
    };

    container(detail_inner)
        .style(move |_| container::Style {
            background: Some(Background::Color(palette.elevated)),
            border: Border {
                color: palette.border_regular,
                width: BORDER_THIN,
                radius: radius(Radius::Lg).into(),
            },
            ..container::Style::default()
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn engine_detail_pane<'a>(
    engine: &'a StaticEngine,
    state: &'a TtsEnginesState,
    palette: &'a ForgePalette,
    gap_sm: f32,
    _gap_md: f32,
) -> Element<'a, Message> {
    let detail_header = engine_detail_header(engine, palette, gap_sm);
    let creds = credentials_section(palette);
    let params = params_section(palette, gap_sm);
    let voices = voices_section(&state.piper_voices, &state.voice_search, palette, gap_sm);

    scrollable(
        column![detail_header, creds, params, voices]
            .spacing(0)
            .width(Length::Fill),
    )
    .height(Length::Fill)
    .into()
}

fn engine_detail_header<'a>(
    engine: &'a StaticEngine,
    palette: &'a ForgePalette,
    gap_sm: f32,
) -> Element<'a, Message> {
    let status_color = (engine.status_color)(palette);

    let credentials_status = row![
        container(text(""))
            .style(move |_| container::Style {
                background: Some(Background::Color(status_color)),
                border: Border {
                    radius: radius(Radius::Pill).into(),
                    ..Border::default()
                },
                ..container::Style::default()
            })
            .width(7)
            .height(7),
        text("Ready").size(FONT_SM).color(status_color),
    ]
    .align_y(Alignment::Center)
    .spacing(gap_sm);

    let default_badge: Element<'a, Message> = if engine.is_default {
        container(
            text("DEFAULT")
                .size(FONT_XS)
                .color(palette.brand)
                .font(font(FontRole::Monospace)),
        )
        .style(move |_| container::Style {
            background: Some(Background::Color(palette.surface_overlay)),
            border: Border {
                radius: radius(Radius::Pill).into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
        .into()
    } else {
        text("").size(0.0).into()
    };

    let title_row = row![
        text(engine.name).size(FONT_SM).color(palette.text_primary),
        default_badge,
    ]
    .align_y(Alignment::Center)
    .spacing(gap_sm);

    let sub = text(format!(
        "local TTS engine \u{b7} {} voices",
        engine.voice_count
    ))
    .size(FONT_SM)
    .color(palette.text_muted);

    container(
        row![
            column![title_row, sub]
                .spacing(spf(Spacing::Xxs))
                .width(Length::Fill),
            credentials_status,
        ]
        .align_y(Alignment::Center)
        .spacing(gap_sm),
    )
    .style(move |_| container::Style {
        border: Border {
            color: palette.border_regular,
            width: BORDER_THIN,
            radius: iced::border::Radius::default(),
        },
        ..container::Style::default()
    })
    .padding([sp(Spacing::Sm), sp(Spacing::Md)])
    .width(Length::Fill)
    .into()
}

fn credentials_section<'a>(palette: &'a ForgePalette) -> Element<'a, Message> {
    let header = text("ENGINE")
        .size(FONT_XS)
        .color(palette.text_muted)
        .font(font(FontRole::Monospace));

    let keyring_notice = container(
        row![
            text("Credentials stored in system keyring, never in config files")
                .size(FONT_SM)
                .color(palette.text_muted)
                .width(Length::Fill),
            container(
                text("LOCAL \u{2014} no credentials")
                    .size(FONT_XS)
                    .color(palette.success)
                    .font(font(FontRole::Monospace)),
            )
            .style(move |_| container::Style {
                background: Some(Background::Color(palette.surface_overlay)),
                border: Border {
                    color: palette.border_regular,
                    width: BORDER_THIN,
                    radius: radius(Radius::Sm).into(),
                },
                ..container::Style::default()
            })
            .padding([sp(Spacing::Xxs), sp(Spacing::Xs)]),
        ]
        .align_y(Alignment::Center),
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
    .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
    .width(Length::Fill);

    container(column![header, keyring_notice].spacing(spf(Spacing::Xs)))
        .style(move |_| container::Style {
            border: Border {
                color: palette.border_regular,
                width: BORDER_THIN,
                radius: iced::border::Radius::default(),
            },
            ..container::Style::default()
        })
        .padding([sp(Spacing::Sm), sp(Spacing::Md)])
        .width(Length::Fill)
        .into()
}

fn param_slider_row<'a>(
    label: &'static str,
    value_label: &'static str,
    palette: &'a ForgePalette,
    gap_sm: f32,
) -> Element<'a, Message> {
    row![
        text(label)
            .size(FONT_SM)
            .color(palette.text_muted)
            .width(70),
        container(text(""))
            .style(move |_| container::Style {
                background: Some(Background::Color(palette.brand)),
                ..container::Style::default()
            })
            .height(4)
            .width(Length::Fill),
        text(value_label)
            .size(FONT_SM)
            .color(palette.text_muted)
            .font(font(FontRole::Monospace))
            .width(42),
    ]
    .align_y(Alignment::Center)
    .spacing(gap_sm)
    .into()
}

fn params_section<'a>(palette: &'a ForgePalette, gap_sm: f32) -> Element<'a, Message> {
    let header = text("DEFAULT VOICE PARAMETERS")
        .size(FONT_XS)
        .color(palette.text_muted)
        .font(font(FontRole::Monospace));

    container(
        column![
            header,
            param_slider_row("Pitch", "0 st", palette, gap_sm),
            param_slider_row("Speed", "1.0x", palette, gap_sm),
            param_slider_row("Volume", "0 dB", palette, gap_sm),
        ]
        .spacing(spf(Spacing::Xs)),
    )
    .style(move |_| container::Style {
        border: Border {
            color: palette.border_regular,
            width: BORDER_THIN,
            radius: iced::border::Radius::default(),
        },
        ..container::Style::default()
    })
    .padding([sp(Spacing::Sm), sp(Spacing::Md)])
    .width(Length::Fill)
    .into()
}

fn voices_section<'a>(
    voices: &'a [EngineVoiceRow],
    search: &'a str,
    palette: &'a ForgePalette,
    gap_sm: f32,
) -> Element<'a, Message> {
    let count = voices.len();

    let header_row = row![
        text(format!("AVAILABLE VOICES \u{b7} {count}"))
            .size(FONT_XS)
            .color(palette.text_muted)
            .font(font(FontRole::Monospace))
            .width(Length::Fill),
        container(
            text_input("Filter voices...", search)
                .on_input(|_| Message::Noop)
                .size(FONT_XS)
                .width(90)
                .style(move |_, _| text_input::Style {
                    background: Background::Color(palette.shell),
                    border: Border {
                        color: palette.border_regular,
                        width: BORDER_THIN,
                        radius: radius(Radius::Sm).into(),
                    },
                    icon: palette.text_muted,
                    placeholder: palette.text_muted,
                    value: palette.text_primary,
                    selection: palette.brand,
                }),
        )
        .padding([sp(Spacing::Xxs), sp(Spacing::Xxs)]),
    ]
    .align_y(Alignment::Center)
    .spacing(gap_sm);

    let visible_voices: Vec<&EngineVoiceRow> = voices
        .iter()
        .filter(|v| {
            search.is_empty()
                || v.display_name
                    .to_ascii_lowercase()
                    .contains(&search.to_ascii_lowercase())
        })
        .collect();

    let voice_cells: Vec<Element<'a, Message>> = visible_voices
        .iter()
        .map(|v| voice_cell(v, palette))
        .collect();

    let grid = row(voice_cells).spacing(gap_sm).wrap();

    container(column![header_row, grid].spacing(spf(Spacing::Xs)))
        .style(move |_| container::Style {
            border: Border {
                color: palette.border_regular,
                width: BORDER_THIN,
                radius: iced::border::Radius::default(),
            },
            ..container::Style::default()
        })
        .padding([sp(Spacing::Sm), sp(Spacing::Md)])
        .width(Length::Fill)
        .into()
}

fn voice_cell<'a>(voice: &'a EngineVoiceRow, palette: &'a ForgePalette) -> Element<'a, Message> {
    let meta = format!(
        "{} \u{b7} {} \u{b7} {}",
        voice.locale, voice.quality, voice.gender
    );

    container(
        column![
            text(&voice.display_name)
                .size(FONT_SM)
                .color(palette.text_primary),
            text(meta)
                .size(FONT_XS)
                .color(palette.text_muted)
                .font(font(FontRole::Monospace)),
        ]
        .spacing(spf(Spacing::Xxs)),
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
    .width(Length::Fixed(140.0))
    .into()
}
