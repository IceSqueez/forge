use forge_widgets::tokens::{
    BORDER_THIN, Density, FONT_MD, FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius,
    spacing,
};
use forge_widgets::{ForgePalette, Icon, tabler_icon};
use iced::widget::{column, container, row, text};
use iced::{Alignment, Background, Border, Element, Length, Task};

use crate::Message;
use crate::message::{TtsMsg, TtsTriggersMsg};
use crate::runtime_view::RuntimeView;

pub struct TtsTriggersState {
    pub command_enabled: bool,
    pub channel_points_enabled: bool,
    pub bits_enabled: bool,
    pub sub_messages_enabled: bool,
    pub read_username: bool,
    pub speak_emotes: bool,
    pub bits_skip_line: bool,
}

impl TtsTriggersState {
    pub fn new() -> Self {
        Self {
            command_enabled: true,
            channel_points_enabled: true,
            bits_enabled: true,
            sub_messages_enabled: false,
            read_username: true,
            speak_emotes: false,
            bits_skip_line: true,
        }
    }
}

impl Default for TtsTriggersState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn update(
    state: &mut TtsTriggersState,
    _rt: &RuntimeView,
    msg: TtsTriggersMsg,
) -> Task<Message> {
    match msg {
        TtsTriggersMsg::CommandEnabledToggled(v) => {
            state.command_enabled = v;
        }
        TtsTriggersMsg::ChannelPointsEnabledToggled(v) => {
            state.channel_points_enabled = v;
        }
        TtsTriggersMsg::BitsEnabledToggled(v) => {
            state.bits_enabled = v;
        }
        TtsTriggersMsg::SubMessagesEnabledToggled(v) => {
            state.sub_messages_enabled = v;
        }
        TtsTriggersMsg::ReadUsernameToggled(v) => {
            state.read_username = v;
        }
        TtsTriggersMsg::SpeakEmotesToggled(v) => {
            state.speak_emotes = v;
        }
        TtsTriggersMsg::BitsSkipLineToggled(v) => {
            state.bits_skip_line = v;
        }
    }
    Task::none()
}

pub fn tts_triggers_view<'a>(
    state: &'a TtsTriggersState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let gap_sm = f32::from(spacing(Spacing::Xs, Density::Cozy));
    let gap_md = f32::from(spacing(Spacing::Sm, Density::Cozy));

    let header_group = column![
        text("WHAT GETS SPOKEN")
            .size(FONT_XS)
            .color(palette.text_muted)
            .font(font(FontRole::Monospace)),
        text("Enable sources and set who can trigger them")
            .size(FONT_SM)
            .color(palette.text_muted),
    ]
    .spacing(4);

    let trigger_row1 = row![
        trigger_card_command(state, palette, gap_sm),
        trigger_card_channel_points(state, palette, gap_sm),
    ]
    .spacing(gap_md)
    .width(Length::Fill);

    let trigger_row2 = row![
        trigger_card_bits(state, palette, gap_sm),
        trigger_card_subs(state, palette, gap_sm),
    ]
    .spacing(gap_md)
    .width(Length::Fill);

    let bottom_row = row![
        format_card(state, palette, gap_sm),
        queue_behavior_card(state, palette, gap_sm),
    ]
    .spacing(gap_md)
    .width(Length::Fill);

    container(
        column![header_group, trigger_row1, trigger_row2, bottom_row]
            .spacing(gap_md)
            .width(Length::Fill),
    )
    .padding([16, 18])
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn toggle_switch<'a>(on: bool, msg: Message, palette: &'a ForgePalette) -> Element<'a, Message> {
    use iced::widget::button;

    let knob_offset = if on { 14.0_f32 } else { 2.0_f32 };
    let track_color = if on {
        palette.success
    } else {
        palette.surface_overlay
    };

    button(
        container(
            container(text(""))
                .style(move |_| container::Style {
                    background: Some(Background::Color(palette.text_primary)),
                    border: Border {
                        radius: radius(Radius::Pill).into(),
                        ..Border::default()
                    },
                    ..container::Style::default()
                })
                .width(14)
                .height(14),
        )
        .style(move |_| container::Style {
            background: Some(Background::Color(track_color)),
            border: Border {
                radius: radius(Radius::Pill).into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .width(32)
        .height(18)
        .padding(iced::Padding {
            top: 2.0,
            bottom: 2.0,
            left: knob_offset,
            right: 2.0,
        }),
    )
    .on_press(msg)
    .style(|_, _| button::Style {
        background: None,
        border: Border::default(),
        text_color: iced::Color::TRANSPARENT,
        ..button::Style::default()
    })
    .padding(0)
    .into()
}

fn role_chip<'a>(
    label: &'static str,
    color: iced::Color,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    container(text(label).size(FONT_XS).color(color))
        .style(move |_| container::Style {
            background: Some(Background::Color(palette.surface_overlay)),
            border: Border {
                radius: radius(Radius::Pill).into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .padding([2, 7])
        .into()
}

fn trigger_card_command<'a>(
    state: &'a TtsTriggersState,
    palette: &'a ForgePalette,
    gap_sm: f32,
) -> Element<'a, Message> {
    let border_color = if state.command_enabled {
        palette.brand
    } else {
        palette.border_regular
    };

    let header = row![
        container(text("!").size(FONT_SM).color(palette.brand))
            .style(move |_| container::Style {
                background: Some(Background::Color(palette.surface_overlay)),
                border: Border {
                    radius: radius(Radius::Sm).into(),
                    ..Border::default()
                },
                ..container::Style::default()
            })
            .width(30)
            .height(30)
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Center),
        column![
            text("Chat command")
                .size(FONT_SM)
                .color(palette.text_primary),
            text("!tts <message>")
                .size(FONT_XS)
                .color(palette.text_muted)
                .font(font(FontRole::Monospace)),
        ]
        .spacing(2)
        .width(Length::Fill),
        toggle_switch(
            state.command_enabled,
            Message::Tts(TtsMsg::Triggers(TtsTriggersMsg::CommandEnabledToggled(
                !state.command_enabled,
            ))),
            palette,
        ),
    ]
    .align_y(Alignment::Center)
    .spacing(gap_sm);

    let chips = row![
        role_chip("Subscribers", palette.success, palette),
        role_chip("VIPs", palette.brand, palette),
        role_chip("Mods", palette.warning, palette),
    ]
    .spacing(gap_sm)
    .wrap();

    let meta = text("cooldown 8s \u{b7} max 250 chars")
        .size(FONT_XS)
        .color(palette.text_muted)
        .font(font(FontRole::Monospace));

    container(column![header, chips, meta].spacing(gap_sm))
        .style(move |_| container::Style {
            background: Some(Background::Color(palette.elevated)),
            border: Border {
                color: border_color,
                width: BORDER_THIN,
                radius: radius(Radius::Lg).into(),
            },
            ..container::Style::default()
        })
        .padding([13, 14])
        .width(Length::Fill)
        .into()
}

fn trigger_card_channel_points<'a>(
    state: &'a TtsTriggersState,
    palette: &'a ForgePalette,
    gap_sm: f32,
) -> Element<'a, Message> {
    let header = row![
        container(tabler_icon(Icon::Diamond, FONT_MD, palette.brand))
            .style(move |_| container::Style {
                background: Some(Background::Color(palette.surface_overlay)),
                border: Border {
                    radius: radius(Radius::Sm).into(),
                    ..Border::default()
                },
                ..container::Style::default()
            })
            .width(30)
            .height(30)
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Center),
        column![
            text("Channel point reward")
                .size(FONT_SM)
                .color(palette.text_primary),
            text("\"Speak my message\" \u{b7} 500 pts")
                .size(FONT_XS)
                .color(palette.text_muted),
        ]
        .spacing(2)
        .width(Length::Fill),
        toggle_switch(
            state.channel_points_enabled,
            Message::Tts(TtsMsg::Triggers(
                TtsTriggersMsg::ChannelPointsEnabledToggled(!state.channel_points_enabled,)
            )),
            palette,
        ),
    ]
    .align_y(Alignment::Center)
    .spacing(gap_sm);

    let chips = row![role_chip("Everyone", palette.text_primary, palette)]
        .spacing(gap_sm)
        .wrap();

    let meta = text("no cooldown \u{b7} priority queue")
        .size(FONT_XS)
        .color(palette.text_muted)
        .font(font(FontRole::Monospace));

    container(column![header, chips, meta].spacing(gap_sm))
        .style(move |_| container::Style {
            background: Some(Background::Color(palette.elevated)),
            border: Border {
                color: palette.border_regular,
                width: BORDER_THIN,
                radius: radius(Radius::Lg).into(),
            },
            ..container::Style::default()
        })
        .padding([13, 14])
        .width(Length::Fill)
        .into()
}

fn trigger_card_bits<'a>(
    state: &'a TtsTriggersState,
    palette: &'a ForgePalette,
    gap_sm: f32,
) -> Element<'a, Message> {
    let header = row![
        container(tabler_icon(Icon::Diamond, FONT_MD, palette.warning))
            .style(move |_| container::Style {
                background: Some(Background::Color(palette.surface_overlay)),
                border: Border {
                    radius: radius(Radius::Sm).into(),
                    ..Border::default()
                },
                ..container::Style::default()
            })
            .width(30)
            .height(30)
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Center),
        column![
            text("Bits / cheers")
                .size(FONT_SM)
                .color(palette.text_primary),
            text("Speak cheer message")
                .size(FONT_XS)
                .color(palette.text_muted),
        ]
        .spacing(2)
        .width(Length::Fill),
        toggle_switch(
            state.bits_enabled,
            Message::Tts(TtsMsg::Triggers(TtsTriggersMsg::BitsEnabledToggled(
                !state.bits_enabled,
            ))),
            palette,
        ),
    ]
    .align_y(Alignment::Center)
    .spacing(gap_sm);

    let min_bits = row![
        text("Minimum").size(FONT_XS).color(palette.text_muted),
        container(
            text("100 bits")
                .size(FONT_SM)
                .color(palette.warning)
                .font(font(FontRole::Monospace)),
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
        .padding([3, 9]),
    ]
    .align_y(Alignment::Center)
    .spacing(gap_sm);

    let meta = text("louder = longer message")
        .size(FONT_XS)
        .color(palette.text_muted)
        .font(font(FontRole::Monospace));

    container(column![header, min_bits, meta].spacing(gap_sm))
        .style(move |_| container::Style {
            background: Some(Background::Color(palette.elevated)),
            border: Border {
                color: palette.border_regular,
                width: BORDER_THIN,
                radius: radius(Radius::Lg).into(),
            },
            ..container::Style::default()
        })
        .padding([13, 14])
        .width(Length::Fill)
        .into()
}

fn trigger_card_subs<'a>(
    state: &'a TtsTriggersState,
    palette: &'a ForgePalette,
    gap_sm: f32,
) -> Element<'a, Message> {
    let header = row![
        container(tabler_icon(Icon::Star, FONT_MD, palette.brand))
            .style(move |_| container::Style {
                background: Some(Background::Color(palette.surface_overlay)),
                border: Border {
                    radius: radius(Radius::Sm).into(),
                    ..Border::default()
                },
                ..container::Style::default()
            })
            .width(30)
            .height(30)
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Center),
        column![
            text("Sub messages")
                .size(FONT_SM)
                .color(palette.text_primary),
            text("Speak resub / gift messages")
                .size(FONT_XS)
                .color(palette.text_muted),
        ]
        .spacing(2)
        .width(Length::Fill),
        toggle_switch(
            state.sub_messages_enabled,
            Message::Tts(TtsMsg::Triggers(TtsTriggersMsg::SubMessagesEnabledToggled(
                !state.sub_messages_enabled,
            ))),
            palette,
        ),
    ]
    .align_y(Alignment::Center)
    .spacing(gap_sm);

    let disabled_note: Element<'a, Message> = if state.sub_messages_enabled {
        text("").size(0.0).into()
    } else {
        text("Disabled \u{2014} toggle to enable")
            .size(FONT_XS)
            .color(palette.text_muted)
            .into()
    };

    container(column![header, disabled_note].spacing(gap_sm))
        .style(move |_| container::Style {
            background: Some(Background::Color(palette.elevated)),
            border: Border {
                color: palette.border_regular,
                width: BORDER_THIN,
                radius: radius(Radius::Lg).into(),
            },
            ..container::Style::default()
        })
        .padding([13, 14])
        .width(Length::Fill)
        .into()
}

fn divider_line<'a>(palette: &'a ForgePalette) -> Element<'a, Message> {
    container(text(""))
        .style(move |_| container::Style {
            background: Some(Background::Color(palette.border_regular)),
            ..container::Style::default()
        })
        .width(Length::Fill)
        .height(BORDER_THIN)
        .into()
}

fn format_row_toggle<'a>(
    label: &'static str,
    on: bool,
    msg: Message,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    container(
        row![
            text(label)
                .size(FONT_SM)
                .color(palette.text_primary)
                .width(Length::Fill),
            toggle_switch(on, msg, palette),
        ]
        .align_y(Alignment::Center),
    )
    .padding([6, 0])
    .width(Length::Fill)
    .into()
}

fn format_card<'a>(
    state: &'a TtsTriggersState,
    palette: &'a ForgePalette,
    gap_sm: f32,
) -> Element<'a, Message> {
    let header = text("MESSAGE FORMAT")
        .size(FONT_XS)
        .color(palette.text_muted)
        .font(font(FontRole::Monospace));

    let username_row = format_row_toggle(
        "Read username before message",
        state.read_username,
        Message::Tts(TtsMsg::Triggers(TtsTriggersMsg::ReadUsernameToggled(
            !state.read_username,
        ))),
        palette,
    );

    let template_section = column![
        text("TEMPLATE")
            .size(FONT_XS)
            .color(palette.text_muted)
            .font(font(FontRole::Monospace)),
        container(
            text("{user} says: {message}")
                .size(FONT_SM)
                .color(palette.text_primary)
                .font(font(FontRole::Monospace))
                .width(Length::Fill),
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
        .width(Length::Fill),
    ]
    .spacing(5);

    let emotes_row = format_row_toggle(
        "Speak emotes as words",
        state.speak_emotes,
        Message::Tts(TtsMsg::Triggers(TtsTriggersMsg::SpeakEmotesToggled(
            !state.speak_emotes,
        ))),
        palette,
    );

    container(
        column![
            header,
            divider_line(palette),
            username_row,
            divider_line(palette),
            template_section,
            divider_line(palette),
            emotes_row,
        ]
        .spacing(gap_sm),
    )
    .style(move |_| container::Style {
        background: Some(Background::Color(palette.elevated)),
        border: Border {
            color: palette.border_regular,
            width: BORDER_THIN,
            radius: radius(Radius::Lg).into(),
        },
        ..container::Style::default()
    })
    .padding([13, 14])
    .width(Length::Fill)
    .into()
}

fn queue_value_row<'a>(
    label: &'static str,
    value: &'static str,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    container(
        row![
            text(label)
                .size(FONT_SM)
                .color(palette.text_primary)
                .width(Length::Fill),
            container(
                text(value)
                    .size(FONT_SM)
                    .color(palette.text_primary)
                    .font(font(FontRole::Monospace)),
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
            .padding([3, 9]),
        ]
        .align_y(Alignment::Center),
    )
    .padding([6, 0])
    .width(Length::Fill)
    .into()
}

fn queue_behavior_card<'a>(
    state: &'a TtsTriggersState,
    palette: &'a ForgePalette,
    gap_sm: f32,
) -> Element<'a, Message> {
    let header = text("QUEUE BEHAVIOR")
        .size(FONT_XS)
        .color(palette.text_muted)
        .font(font(FontRole::Monospace));

    let skip_row = container(
        row![
            text("Bits & points skip the line")
                .size(FONT_SM)
                .color(palette.text_primary)
                .width(Length::Fill),
            toggle_switch(
                state.bits_skip_line,
                Message::Tts(TtsMsg::Triggers(TtsTriggersMsg::BitsSkipLineToggled(
                    !state.bits_skip_line,
                ))),
                palette,
            ),
        ]
        .align_y(Alignment::Center),
    )
    .padding([6, 0])
    .width(Length::Fill);

    container(
        column![
            header,
            divider_line(palette),
            queue_value_row("Max queue length", "20", palette),
            divider_line(palette),
            queue_value_row("Per-user limit in queue", "2", palette),
            divider_line(palette),
            skip_row,
        ]
        .spacing(gap_sm),
    )
    .style(move |_| container::Style {
        background: Some(Background::Color(palette.elevated)),
        border: Border {
            color: palette.border_regular,
            width: BORDER_THIN,
            radius: radius(Radius::Lg).into(),
        },
        ..container::Style::default()
    })
    .padding([13, 14])
    .width(Length::Fill)
    .into()
}
