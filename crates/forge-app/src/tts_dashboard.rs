use forge_speak_queue::SpeakEvent;
use forge_widgets::ForgePalette;
use forge_widgets::icons::{Icon, tabler_icon};
use forge_widgets::tokens::{
    BORDER_THIN, Density, FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius, spacing,
};
use iced::widget::{button, column, container, row, scrollable, slider, text, text_input};
use iced::{Alignment, Background, Border, Color, Element, Length, Task};

use crate::Message;
use crate::message::{TtsDashMsg, TtsMsg};

pub struct TtsDashState {
    pub paused: bool,
    pub volume: f32,
    pub test_input: String,
    pub now_speaking: Option<NowSpeakingData>,
    pub queue: Vec<QueueItemData>,
    pub stats: SessionStats,
    pub command_error: Option<String>,
}

pub struct NowSpeakingData {
    pub viewer_name: String,
    pub engine_voice: String,
    pub text: String,
    pub progress: f32,
    pub elapsed_secs: u32,
    pub total_secs: u32,
}

pub struct QueueItemData {
    pub viewer_name: String,
    pub engine_voice: String,
    pub text: String,
    pub duration_secs: u32,
    pub is_high_priority: bool,
    pub bits_amount: Option<u32>,
}

pub struct SessionStats {
    pub spoken: u32,
    pub skipped: u32,
    pub filtered: u32,
    pub avg_latency_ms: Option<u32>,
}

impl TtsDashState {
    pub fn new() -> Self {
        Self {
            paused: false,
            volume: 0.72,
            test_input: String::new(),
            now_speaking: None,
            queue: Vec::new(),
            stats: SessionStats {
                spoken: 0,
                skipped: 0,
                filtered: 0,
                avg_latency_ms: None,
            },
            command_error: None,
        }
    }
}

impl Default for TtsDashState {
    fn default() -> Self {
        Self::new()
    }
}

impl TtsDashState {
    pub fn apply_event(&mut self, event: SpeakEvent) {
        match event {
            SpeakEvent::Enqueued { queue_len, .. } => {
                let _ = queue_len;
            }
            SpeakEvent::Started { .. } => {}
            SpeakEvent::Finished { .. } => {
                self.now_speaking = None;
                self.stats.spoken = self.stats.spoken.saturating_add(1);
            }
            SpeakEvent::Failed { .. } => {
                self.now_speaking = None;
            }
            SpeakEvent::Skipped { .. } => {
                self.stats.skipped = self.stats.skipped.saturating_add(1);
            }
            SpeakEvent::Rejected { .. } => {
                self.stats.filtered = self.stats.filtered.saturating_add(1);
            }
            SpeakEvent::QueueChanged { .. } => {}
            SpeakEvent::Paused { .. } => {
                self.paused = true;
            }
            SpeakEvent::Resumed => {
                self.paused = false;
            }
            SpeakEvent::Cleared => {
                self.queue.clear();
                self.now_speaking = None;
            }
        }
    }
}

pub fn handle_tts_dash_msg(state: &mut TtsDashState, msg: TtsDashMsg) -> Task<Message> {
    match msg {
        TtsDashMsg::SpeakEventReceived(event) => {
            state.apply_event(event);
            Task::none()
        }
        TtsDashMsg::PauseQueue => Task::none(),
        TtsDashMsg::SkipCurrent => Task::none(),
        TtsDashMsg::StopAll => {
            state.queue.clear();
            state.now_speaking = None;
            Task::none()
        }
        TtsDashMsg::VolumeChanged(v) => {
            state.volume = v;
            Task::none()
        }
        TtsDashMsg::TestInputChanged(s) => {
            state.test_input = s;
            Task::none()
        }
        TtsDashMsg::SpeakTest => Task::none(),
        TtsDashMsg::CommandResult(r) => {
            if let Err(e) = r {
                state.command_error = Some(e);
            }
            Task::none()
        }
    }
}

pub fn tts_dashboard_view<'a>(
    state: &'a TtsDashState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let gap_sm = f32::from(spacing(Spacing::Xs, Density::Cozy));
    let gap_md = f32::from(spacing(Spacing::Sm, Density::Cozy));
    let gap_lg = f32::from(spacing(Spacing::Sm, Density::Cozy));

    let control_strip = control_strip_view(state, palette, gap_sm, gap_md);
    let now_speaking = now_speaking_view(state, palette, gap_sm);
    let queue_section = queue_section_view(state, palette, gap_sm);
    let right_pane = right_pane_view(state, palette, gap_sm, gap_lg);

    let left_col = column![now_speaking, queue_section]
        .width(Length::Fill)
        .height(Length::Fill);

    let main_row = row![left_col, right_pane].height(Length::Fill);

    column![control_strip, main_row].into()
}

fn control_strip_view<'a>(
    state: &'a TtsDashState,
    palette: &'a ForgePalette,
    gap_sm: f32,
    gap_md: f32,
) -> Element<'a, Message> {
    let pause_label = if state.paused {
        "Resume"
    } else {
        "Pause queue"
    };
    let btn_bg = if state.paused {
        palette.success
    } else {
        palette.random
    };

    let pause_icon = if state.paused {
        Icon::PlayerPlay
    } else {
        Icon::PlayerPause
    };
    let pause_btn = button(
        row![
            tabler_icon(pause_icon, 13.0, palette.shell),
            text(pause_label).size(FONT_SM),
        ]
        .align_y(Alignment::Center)
        .spacing(gap_sm),
    )
    .on_press(Message::Tts(TtsMsg::Dashboard(TtsDashMsg::PauseQueue)))
    .style(move |_, _| button::Style {
        background: Some(Background::Color(btn_bg)),
        border: Border {
            radius: radius(Radius::Sm).into(),
            ..Border::default()
        },
        text_color: palette.shell,
        ..button::Style::default()
    })
    .padding([5, 12]);

    let skip_btn = button(text("Skip").size(FONT_SM))
        .on_press(Message::Tts(TtsMsg::Dashboard(TtsDashMsg::SkipCurrent)))
        .style(move |_, _| button::Style {
            background: Some(Background::Color(palette.surface_overlay)),
            border: Border {
                color: palette.border_regular,
                width: BORDER_THIN,
                radius: radius(Radius::Sm).into(),
            },
            text_color: palette.text_secondary,
            ..button::Style::default()
        })
        .padding([5, 11]);

    let stop_btn = button(text("Stop all").size(FONT_SM))
        .on_press(Message::Tts(TtsMsg::Dashboard(TtsDashMsg::StopAll)))
        .style(move |_, _| button::Style {
            background: Some(Background::Color(palette.surface_overlay)),
            border: Border {
                color: palette.border_regular,
                width: BORDER_THIN,
                radius: radius(Radius::Sm).into(),
            },
            text_color: palette.text_secondary,
            ..button::Style::default()
        })
        .padding([5, 11]);

    let vol_pct = (state.volume * 100.0).round() as u32;
    let vol_text = text(format!("{vol_pct}%"))
        .size(FONT_SM)
        .color(palette.text_muted)
        .font(font(FontRole::Monospace));

    let vol_slider = slider(0.0..=1.0, state.volume, |v| {
        Message::Tts(TtsMsg::Dashboard(TtsDashMsg::VolumeChanged(v)))
    })
    .width(90)
    .step(0.01);

    let volume_row = row![
        tabler_icon(Icon::Volume, 14.0, palette.text_muted),
        vol_slider,
        vol_text,
    ]
    .align_y(Alignment::Center)
    .spacing(gap_sm);

    let left = row![pause_btn, skip_btn, stop_btn, volume_row]
        .align_y(Alignment::Center)
        .spacing(gap_sm);

    let test_input = text_input("Type to test a voice...", &state.test_input)
        .on_input(|s| Message::Tts(TtsMsg::Dashboard(TtsDashMsg::TestInputChanged(s))))
        .on_submit(Message::Tts(TtsMsg::Dashboard(TtsDashMsg::SpeakTest)))
        .size(FONT_SM)
        .width(180)
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
        });

    let speak_btn = button(text("Speak").size(FONT_SM))
        .on_press(Message::Tts(TtsMsg::Dashboard(TtsDashMsg::SpeakTest)))
        .style(move |_, _| button::Style {
            background: Some(Background::Color(palette.brand)),
            border: Border {
                radius: radius(Radius::Sm).into(),
                ..Border::default()
            },
            text_color: palette.shell,
            ..button::Style::default()
        })
        .padding([5, 11]);

    let right = row![test_input, speak_btn]
        .align_y(Alignment::Center)
        .spacing(gap_sm);

    container(
        row![left, right]
            .align_y(Alignment::Center)
            .spacing(gap_md)
            .width(Length::Fill),
    )
    .style(move |_| container::Style {
        background: Some(Background::Color(palette.elevated)),
        border: Border {
            color: palette.border_regular,
            width: BORDER_THIN,
            radius: iced::border::Radius::default(),
        },
        ..container::Style::default()
    })
    .padding([9, 16])
    .width(Length::Fill)
    .into()
}

fn now_speaking_view<'a>(
    state: &'a TtsDashState,
    palette: &'a ForgePalette,
    gap_sm: f32,
) -> Element<'a, Message> {
    let header = text("NOW SPEAKING")
        .size(FONT_XS)
        .color(palette.text_muted)
        .font(font(FontRole::Monospace));

    let body: Element<'a, Message> = if let Some(ns) = &state.now_speaking {
        let progress_text = format!(
            "{}:{:02} / {}:{:02}",
            ns.elapsed_secs / 60,
            ns.elapsed_secs % 60,
            ns.total_secs / 60,
            ns.total_secs % 60
        );
        column![
            header,
            row![
                text(&ns.viewer_name).size(FONT_SM).color(palette.success),
                text(&ns.engine_voice)
                    .size(FONT_SM)
                    .color(palette.text_muted)
                    .font(font(FontRole::Monospace)),
            ]
            .spacing(gap_sm)
            .align_y(Alignment::Center),
            text(&ns.text).size(FONT_SM).color(palette.text_primary),
            text(progress_text)
                .size(FONT_SM)
                .color(palette.text_muted)
                .font(font(FontRole::Monospace)),
        ]
        .spacing(gap_sm)
        .into()
    } else {
        column![header, text("—").size(FONT_SM).color(palette.text_muted),]
            .spacing(gap_sm)
            .into()
    };

    container(body)
        .style(move |_| container::Style {
            background: Some(Background::Color(palette.elevated)),
            border: Border {
                color: palette.border_regular,
                width: BORDER_THIN,
                radius: iced::border::Radius::default(),
            },
            ..container::Style::default()
        })
        .padding([14, 16])
        .width(Length::Fill)
        .into()
}

fn queue_section_view<'a>(
    state: &'a TtsDashState,
    palette: &'a ForgePalette,
    gap_sm: f32,
) -> Element<'a, Message> {
    let count = state.queue.len();
    let header = container(
        row![
            text("Up next").size(FONT_SM).color(palette.text_primary),
            container(
                text(format!("{count}"))
                    .size(FONT_SM)
                    .color(palette.text_muted),
            )
            .style(move |_| container::Style {
                background: Some(Background::Color(palette.surface_overlay)),
                border: Border {
                    color: palette.border_regular,
                    width: BORDER_THIN,
                    radius: radius(Radius::Pill).into(),
                },
                ..container::Style::default()
            })
            .padding([0, 6]),
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
    .padding([9, 16])
    .width(Length::Fill);

    let items: Element<'a, Message> = if state.queue.is_empty() {
        container(
            text("Queue is empty")
                .size(FONT_SM)
                .color(palette.text_muted),
        )
        .padding([16, 16])
        .width(Length::Fill)
        .into()
    } else {
        let rows: Vec<Element<'a, Message>> = state
            .queue
            .iter()
            .enumerate()
            .map(|(i, item)| queue_item_row(i, item, palette, gap_sm))
            .collect();
        column(rows).into()
    };

    column![header, scrollable(items).height(Length::Fill)]
        .height(Length::Fill)
        .into()
}

fn queue_item_row<'a>(
    index: usize,
    item: &'a QueueItemData,
    palette: &'a ForgePalette,
    gap_sm: f32,
) -> Element<'a, Message> {
    let pos_text = text(format!("{}", index + 1))
        .size(FONT_SM)
        .color(palette.text_muted)
        .font(font(FontRole::Monospace))
        .width(14);

    let priority_badge: Element<'a, Message> = if item.is_high_priority {
        let bits_label = if let Some(b) = item.bits_amount {
            format!("BITS {b}")
        } else {
            "HIGH".to_string()
        };
        container(
            text(bits_label)
                .size(9.0)
                .color(palette.shell)
                .font(font(FontRole::Monospace)),
        )
        .style(move |_| container::Style {
            background: Some(Background::Color(palette.warning)),
            border: Border {
                radius: radius(Radius::Sm).into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .padding([1, 5])
        .into()
    } else {
        text("").size(0.0).into()
    };

    let name_row = row![
        text(&item.viewer_name).size(FONT_SM).color(palette.success),
        priority_badge,
        text(&item.engine_voice)
            .size(FONT_SM)
            .color(palette.text_muted)
            .font(font(FontRole::Monospace)),
    ]
    .align_y(Alignment::Center)
    .spacing(gap_sm);

    let content = column![
        name_row,
        text(&item.text)
            .size(FONT_SM)
            .color(palette.text_muted)
            .width(Length::Fill),
    ]
    .spacing(2)
    .width(Length::Fill);

    let dur_text = text(format!("0:{:02}", item.duration_secs))
        .size(FONT_SM)
        .color(palette.text_muted)
        .font(font(FontRole::Monospace));

    container(
        row![pos_text, content, dur_text]
            .align_y(Alignment::Center)
            .spacing(gap_sm)
            .width(Length::Fill),
    )
    .style(move |_| container::Style {
        border: Border {
            color: palette.border_regular,
            width: BORDER_THIN,
            radius: iced::border::Radius::default(),
        },
        ..container::Style::default()
    })
    .padding([9, 16])
    .width(Length::Fill)
    .into()
}

fn right_pane_view<'a>(
    state: &'a TtsDashState,
    palette: &'a ForgePalette,
    gap_sm: f32,
    gap_lg: f32,
) -> Element<'a, Message> {
    let session_header = text("SESSION")
        .size(FONT_XS)
        .color(palette.text_muted)
        .font(font(FontRole::Monospace));

    fn stat_row<'b>(
        label: &'static str,
        value: String,
        value_color: Color,
        palette: &'b ForgePalette,
        border_bottom: bool,
    ) -> Element<'b, Message> {
        let row_el = row![
            text(label)
                .size(FONT_SM)
                .color(palette.text_muted)
                .width(Length::Fill),
            text(value).size(FONT_SM).color(value_color),
        ]
        .align_y(Alignment::Center)
        .padding([5, 0]);

        if border_bottom {
            container(row_el)
                .style(move |_| container::Style {
                    border: Border {
                        color: palette.border_regular,
                        width: BORDER_THIN,
                        radius: iced::border::Radius::default(),
                    },
                    ..container::Style::default()
                })
                .width(Length::Fill)
                .into()
        } else {
            row_el.into()
        }
    }

    let latency_str = state
        .stats
        .avg_latency_ms
        .map(|ms| format!("{ms}ms"))
        .unwrap_or_else(|| "—".to_string());

    let stats_col = column![
        session_header,
        stat_row(
            "Spoken",
            state.stats.spoken.to_string(),
            palette.brand,
            palette,
            true
        ),
        stat_row(
            "Skipped",
            state.stats.skipped.to_string(),
            palette.warning,
            palette,
            true
        ),
        stat_row(
            "Filtered",
            state.stats.filtered.to_string(),
            palette.random,
            palette,
            true
        ),
        stat_row("Avg latency", latency_str, palette.success, palette, false),
    ]
    .spacing(gap_sm);

    let engines_header = text("ENGINES")
        .size(FONT_XS)
        .color(palette.text_muted)
        .font(font(FontRole::Monospace));

    let piper_card = engine_card("Piper", "local · ready", palette.success, palette, gap_sm);
    let engines_col = column![engines_header, piper_card].spacing(gap_sm);

    container(
        scrollable(
            column![stats_col, engines_col]
                .spacing(gap_lg)
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
    .padding([14, 14])
    .width(236)
    .into()
}

fn engine_card<'a>(
    name: &'a str,
    meta: &'a str,
    status_color: Color,
    palette: &'a ForgePalette,
    _gap_sm: f32,
) -> Element<'a, Message> {
    container(
        column![
            row![
                text(name)
                    .size(FONT_SM)
                    .color(palette.text_primary)
                    .width(Length::Fill),
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
            ]
            .align_y(Alignment::Center),
            text(meta)
                .size(10.0)
                .color(palette.text_muted)
                .font(font(FontRole::Monospace)),
        ]
        .spacing(3),
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
    .padding([9, 11])
    .width(Length::Fill)
    .into()
}
