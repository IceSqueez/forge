use forge_storage::TtsTriggerSettings;
use forge_widgets::tokens::{
    BORDER_THIN, FONT_MD, FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius, sp, spf,
};
use forge_widgets::{ForgePalette, Icon, tabler_icon};
use iced::widget::{Space, column, container, row, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Task};

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
    pub save_error: Option<String>,
}

impl TtsTriggersState {
    pub fn new() -> Self {
        Self::from_settings(TtsTriggerSettings::default())
    }

    fn from_settings(settings: TtsTriggerSettings) -> Self {
        Self {
            command_enabled: settings.command_enabled,
            channel_points_enabled: settings.channel_points_enabled,
            bits_enabled: settings.bits_enabled,
            sub_messages_enabled: settings.sub_messages_enabled,
            read_username: settings.read_username,
            speak_emotes: settings.speak_emotes,
            bits_skip_line: settings.bits_skip_line,
            save_error: None,
        }
    }

    fn to_settings(&self) -> TtsTriggerSettings {
        TtsTriggerSettings {
            command_enabled: self.command_enabled,
            channel_points_enabled: self.channel_points_enabled,
            bits_enabled: self.bits_enabled,
            sub_messages_enabled: self.sub_messages_enabled,
            read_username: self.read_username,
            speak_emotes: self.speak_emotes,
            bits_skip_line: self.bits_skip_line,
        }
    }
}

impl Default for TtsTriggersState {
    fn default() -> Self {
        Self::new()
    }
}

fn persist(state: &TtsTriggersState, rt: &RuntimeView) -> Task<Message> {
    let repo = rt.backend.tts_trigger_settings_repo();
    let handle = rt.tts_trigger_settings.clone();
    let settings = state.to_settings();
    Task::perform(
        async move {
            repo.set_trigger_settings(&settings)
                .await
                .map_err(|e| e.to_string())?;
            if let Some(handle) = handle {
                handle.swap(settings);
            }
            Ok(())
        },
        |r| Message::Tts(TtsMsg::Triggers(TtsTriggersMsg::PersistResult(r))),
    )
}

pub fn update(
    state: &mut TtsTriggersState,
    rt: &RuntimeView,
    msg: TtsTriggersMsg,
) -> Task<Message> {
    match msg {
        TtsTriggersMsg::LoadRequested => {
            let repo = rt.backend.tts_trigger_settings_repo();
            return Task::perform(
                async move { repo.get_trigger_settings().await.map_err(|e| e.to_string()) },
                |r| Message::Tts(TtsMsg::Triggers(TtsTriggersMsg::Loaded(r))),
            );
        }
        TtsTriggersMsg::Loaded(Ok(settings)) => {
            *state = TtsTriggersState::from_settings(settings);
            return Task::none();
        }
        TtsTriggersMsg::Loaded(Err(e)) => {
            tracing::warn!(error = %e, "failed to load tts trigger settings");
            return Task::none();
        }
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
        TtsTriggersMsg::PersistResult(Ok(())) => {
            state.save_error = None;
            return Task::none();
        }
        TtsTriggersMsg::PersistResult(Err(e)) => {
            tracing::warn!(error = %e, "failed to persist tts trigger settings");
            state.save_error = Some(e);
            return Task::none();
        }
    }
    persist(state, rt)
}

pub fn tts_triggers_view<'a>(
    state: &'a TtsTriggersState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let gap_sm = spf(Spacing::Xs);
    let gap_md = spf(Spacing::Sm);

    let header_group = column![
        text(forge_widgets::tr!("tts_triggers_header"))
            .size(FONT_XS)
            .color(palette.text_muted)
            .font(font(FontRole::Monospace)),
        text(forge_widgets::tr!("tts_triggers_hint"))
            .size(FONT_SM)
            .color(palette.text_muted),
    ]
    .spacing(spf(Spacing::Xxs));

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

    let error_banner: Element<'a, Message> = match &state.save_error {
        Some(err) => container(
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
        None => Space::new().into(),
    };

    container(
        column![
            header_group,
            error_banner,
            trigger_row1,
            trigger_row2,
            bottom_row
        ]
        .spacing(gap_md)
        .width(Length::Fill),
    )
    .padding([sp(Spacing::Md), sp(Spacing::Md)])
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
            top: spf(Spacing::Xxs),
            bottom: spf(Spacing::Xxs),
            left: knob_offset,
            right: spf(Spacing::Xxs),
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
        .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
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
            text(forge_widgets::tr!("tts_triggers_cmd_title"))
                .size(FONT_SM)
                .color(palette.text_primary),
            text(forge_widgets::tr!("tts_triggers_cmd_subtitle"))
                .size(FONT_XS)
                .color(palette.text_muted)
                .font(font(FontRole::Monospace)),
        ]
        .spacing(spf(Spacing::Xxs))
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
        role_chip(
            Box::leak(forge_widgets::tr!("tts_triggers_role_subscribers").into_boxed_str()),
            palette.success,
            palette
        ),
        role_chip(
            Box::leak(forge_widgets::tr!("tts_triggers_role_vips").into_boxed_str()),
            palette.brand,
            palette
        ),
        role_chip(
            Box::leak(forge_widgets::tr!("tts_triggers_role_mods").into_boxed_str()),
            palette.warning,
            palette
        ),
    ]
    .spacing(gap_sm)
    .wrap();

    let meta = text(forge_widgets::tr!("tts_triggers_cmd_meta"))
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
        .padding([sp(Spacing::Sm), sp(Spacing::Sm)])
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
            text(forge_widgets::tr!("tts_triggers_points_title"))
                .size(FONT_SM)
                .color(palette.text_primary),
            text(forge_widgets::tr!("tts_triggers_points_subtitle"))
                .size(FONT_XS)
                .color(palette.text_muted),
        ]
        .spacing(spf(Spacing::Xxs))
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

    let chips = row![role_chip(
        Box::leak(forge_widgets::tr!("tts_triggers_role_everyone").into_boxed_str()),
        palette.text_primary,
        palette
    )]
    .spacing(gap_sm)
    .wrap();

    let meta = text(forge_widgets::tr!("tts_triggers_points_meta"))
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
        .padding([sp(Spacing::Sm), sp(Spacing::Sm)])
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
            text(forge_widgets::tr!("tts_triggers_bits_title"))
                .size(FONT_SM)
                .color(palette.text_primary),
            text(forge_widgets::tr!("tts_triggers_bits_subtitle"))
                .size(FONT_XS)
                .color(palette.text_muted),
        ]
        .spacing(spf(Spacing::Xxs))
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
        text(forge_widgets::tr!("tts_triggers_bits_min_label"))
            .size(FONT_XS)
            .color(palette.text_muted),
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

    let meta = text(forge_widgets::tr!("tts_triggers_bits_meta"))
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
        .padding([sp(Spacing::Sm), sp(Spacing::Sm)])
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
            text(forge_widgets::tr!("tts_triggers_subs_title"))
                .size(FONT_SM)
                .color(palette.text_primary),
            text(forge_widgets::tr!("tts_triggers_subs_subtitle"))
                .size(FONT_XS)
                .color(palette.text_muted),
        ]
        .spacing(spf(Spacing::Xxs))
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
        Space::new().into()
    } else {
        text(forge_widgets::tr!("tts_triggers_subs_disabled"))
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
        .padding([sp(Spacing::Sm), sp(Spacing::Sm)])
        .width(Length::Fill)
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
    .padding([sp(Spacing::Xs), 0])
    .width(Length::Fill)
    .into()
}

fn format_card<'a>(
    state: &'a TtsTriggersState,
    palette: &'a ForgePalette,
    gap_sm: f32,
) -> Element<'a, Message> {
    let header = text(forge_widgets::tr!("tts_triggers_format_header"))
        .size(FONT_XS)
        .color(palette.text_muted)
        .font(font(FontRole::Monospace));

    let username_row = format_row_toggle(
        Box::leak(forge_widgets::tr!("tts_triggers_format_read_username").into_boxed_str()),
        state.read_username,
        Message::Tts(TtsMsg::Triggers(TtsTriggersMsg::ReadUsernameToggled(
            !state.read_username,
        ))),
        palette,
    );

    let template_section = column![
        text(forge_widgets::tr!("tts_triggers_format_template_header"))
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
        .padding([sp(Spacing::Xs), sp(Spacing::Xs)])
        .width(Length::Fill),
    ]
    .spacing(spf(Spacing::Xxs));

    let emotes_row = format_row_toggle(
        Box::leak(forge_widgets::tr!("tts_triggers_format_speak_emotes").into_boxed_str()),
        state.speak_emotes,
        Message::Tts(TtsMsg::Triggers(TtsTriggersMsg::SpeakEmotesToggled(
            !state.speak_emotes,
        ))),
        palette,
    );

    container(
        column![
            header,
            forge_widgets::divider(palette, forge_widgets::DividerAxis::Horizontal),
            username_row,
            forge_widgets::divider(palette, forge_widgets::DividerAxis::Horizontal),
            template_section,
            forge_widgets::divider(palette, forge_widgets::DividerAxis::Horizontal),
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
            .padding([sp(Spacing::Xxs), sp(Spacing::Xs)]),
        ]
        .align_y(Alignment::Center),
    )
    .padding([sp(Spacing::Xs), 0])
    .width(Length::Fill)
    .into()
}

fn queue_behavior_card<'a>(
    state: &'a TtsTriggersState,
    palette: &'a ForgePalette,
    gap_sm: f32,
) -> Element<'a, Message> {
    let header = text(forge_widgets::tr!("tts_triggers_queue_header"))
        .size(FONT_XS)
        .color(palette.text_muted)
        .font(font(FontRole::Monospace));

    let skip_row = container(
        row![
            text(forge_widgets::tr!("tts_triggers_queue_bits_skip"))
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
    .padding([sp(Spacing::Xs), 0])
    .width(Length::Fill);

    container(
        column![
            header,
            forge_widgets::divider(palette, forge_widgets::DividerAxis::Horizontal),
            queue_value_row(
                Box::leak(forge_widgets::tr!("tts_triggers_queue_max_length").into_boxed_str()),
                "20",
                palette
            ),
            forge_widgets::divider(palette, forge_widgets::DividerAxis::Horizontal),
            queue_value_row(
                Box::leak(forge_widgets::tr!("tts_triggers_queue_per_user_limit").into_boxed_str()),
                "2",
                palette
            ),
            forge_widgets::divider(palette, forge_widgets::DividerAxis::Horizontal),
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
