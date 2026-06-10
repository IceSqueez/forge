use forge_widgets::ForgePalette;
use forge_widgets::tokens::{
    BORDER_THIN, FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius, sp, spf,
};
use iced::widget::{Space, button, column, container, row, scrollable, text, text_input};
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
    rt: &'a RuntimeView,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let gap_sm = spf(Spacing::Xs);
    let gap_md = spf(Spacing::Sm);

    let engine_list = engine_list_view(state, rt, palette, gap_sm);
    let detail = engine_detail_view(state, rt, palette, gap_sm, gap_md);

    row![engine_list, detail]
        .spacing(gap_md)
        .padding([sp(Spacing::Md), sp(Spacing::Md)])
        .height(Length::Fill)
        .into()
}

fn current_engines(rt: &RuntimeView) -> Vec<EngineCard> {
    rt.tts_engine_ids
        .iter()
        .enumerate()
        .map(|(i, id)| engine_meta(&id.0, i == 0))
        .collect()
}

struct EngineCard {
    id: String,
    name: Option<&'static str>,
    kind: &'static str,
    voice_count: u8,
    status_color: fn(&ForgePalette) -> Color,
    is_default: bool,
}

type EngineMetaTuple = (
    Option<&'static str>,
    &'static str,
    u8,
    fn(&ForgePalette) -> Color,
);

fn engine_meta(id: &str, is_default: bool) -> EngineCard {
    let (name, kind, voice_count, status_color): EngineMetaTuple = match id {
        "piper" => (Some("Piper"), "local", 4, |p| p.success),
        "espeak" => (Some("eSpeak-NG"), "local", 0, |p| p.success),
        "sapi" => (Some("Microsoft SAPI 5"), "system", 0, |p| p.info),
        "nsspeech" => (Some("Apple AVSpeech"), "system", 0, |p| p.info),
        _ => (None, "external", 0, |p: &ForgePalette| p.text_muted),
    };
    EngineCard {
        id: id.to_owned(),
        name,
        kind,
        voice_count,
        status_color,
        is_default,
    }
}

fn engine_list_view<'a>(
    state: &'a TtsEnginesState,
    rt: &'a RuntimeView,
    palette: &'a ForgePalette,
    gap_sm: f32,
) -> Element<'a, Message> {
    let engines = current_engines(rt);
    let header = text(format!(
        "{} \u{b7} {}",
        forge_widgets::tr!("tts_engines_header_prefix"),
        engines.len()
    ))
    .size(FONT_XS)
    .color(palette.text_muted)
    .font(font(FontRole::Monospace));

    let engine_cards: Vec<Element<'a, Message>> = engines
        .into_iter()
        .map(|e| {
            let selected = state.selected_engine == e.id;
            engine_list_card(e, selected, palette)
        })
        .collect();

    let placeholder = container(
        text(forge_widgets::tr!("tts_engines_more_placeholder"))
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
    engine: EngineCard,
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

    let display_name = engine
        .name
        .map(str::to_owned)
        .unwrap_or_else(|| forge_widgets::tr!("tts_engines_unknown"));
    let name_row = row![
        text(display_name)
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
            engine.id,
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
    rt: &'a RuntimeView,
    palette: &'a ForgePalette,
    gap_sm: f32,
    gap_md: f32,
) -> Element<'a, Message> {
    let engine = current_engines(rt)
        .into_iter()
        .find(|e| e.id == state.selected_engine);

    let detail_inner: Element<'a, Message> = if let Some(eng) = engine {
        engine_detail_pane(eng, state, palette, gap_sm, gap_md)
    } else {
        container(
            text(forge_widgets::tr!("tts_engines_select_hint"))
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
    engine: EngineCard,
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
    engine: EngineCard,
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
        text(forge_widgets::tr!("tts_engines_status_ready"))
            .size(FONT_SM)
            .color(status_color),
    ]
    .align_y(Alignment::Center)
    .spacing(gap_sm);

    let default_badge: Element<'a, Message> = if engine.is_default {
        container(
            text(forge_widgets::tr!("tts_engines_default_badge"))
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
        Space::new().into()
    };

    let engine_display_name = engine
        .name
        .map(str::to_owned)
        .unwrap_or_else(|| forge_widgets::tr!("tts_engines_unknown"));
    let title_row = row![
        text(engine_display_name)
            .size(FONT_SM)
            .color(palette.text_primary),
        default_badge,
    ]
    .align_y(Alignment::Center)
    .spacing(gap_sm);

    let sub = text(format!(
        "{} \u{b7} {} voices",
        forge_widgets::tr!("tts_engines_local_meta"),
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
    let header = text(forge_widgets::tr!("tts_engines_section_engine"))
        .size(FONT_XS)
        .color(palette.text_muted)
        .font(font(FontRole::Monospace));

    let keyring_notice = container(
        row![
            text(forge_widgets::tr!("tts_engines_credentials_notice"))
                .size(FONT_SM)
                .color(palette.text_muted)
                .width(Length::Fill),
            container(
                text(forge_widgets::tr!("tts_engines_no_credentials"))
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
    label: &str,
    value_label: &'static str,
    palette: &'a ForgePalette,
    gap_sm: f32,
) -> Element<'a, Message> {
    row![
        text(label.to_owned())
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
    let header = text(forge_widgets::tr!("tts_engines_section_params"))
        .size(FONT_XS)
        .color(palette.text_muted)
        .font(font(FontRole::Monospace));

    container(
        column![
            header,
            param_slider_row(
                &forge_widgets::tr!("tts_engines_param_pitch"),
                "0 st",
                palette,
                gap_sm
            ),
            param_slider_row(
                &forge_widgets::tr!("tts_engines_param_speed"),
                "1.0x",
                palette,
                gap_sm
            ),
            param_slider_row(
                &forge_widgets::tr!("tts_engines_param_volume"),
                "0 dB",
                palette,
                gap_sm
            ),
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
        text(format!(
            "{} \u{b7} {count}",
            forge_widgets::tr!("tts_engines_voices_header_prefix")
        ))
        .size(FONT_XS)
        .color(palette.text_muted)
        .font(font(FontRole::Monospace))
        .width(Length::Fill),
        container(
            text_input(
                &forge_widgets::tr!("tts_engines_voices_filter_placeholder"),
                search
            )
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
