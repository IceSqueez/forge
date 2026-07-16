use std::borrow::Cow;

use forge_speak_queue::{Priority, RequestId, SpeakCommand, SpeakEvent, SpeakRequest};
use forge_widgets::icons::{Icon, tabler_icon};
use forge_widgets::tokens::{
    BORDER_THIN, FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius, sp, spf,
};
use forge_widgets::{
    ConfirmKind, ConfirmModalParams, ConfirmTone, ForgePalette, ToastKind, confirm_modal,
};
use iced::widget::{Space, button, column, container, row, scrollable, stack, text, text_input};
use iced::{Alignment, Background, Border, Color, Element, Length, Task};

use crate::Message;
use crate::message::{ToastMsg, TtsDashMsg, TtsMsg};
use crate::runtime_view::RuntimeView;

pub struct TtsDashState {
    pub paused: bool,
    pub volume: f32,
    pub test_input: String,
    pub now_speaking: Option<NowSpeakingData>,
    pub queue: Vec<QueueItemData>,
    pub stats: SessionStats,
    /// Two-phase Stop-all gate - armed by the control strip's Stop button,
    /// rendered by the shared `confirm_modal`. `false` = no confirm showing.
    pub pending_stop_all: bool,
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
    pub request_id: RequestId,
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
            pending_stop_all: false,
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
            SpeakEvent::Enqueued {
                request_id,
                viewer_name,
                text,
                is_high_priority,
                ..
            } => {
                self.queue.push(QueueItemData {
                    request_id,
                    viewer_name,
                    engine_voice: String::new(),
                    text,
                    duration_secs: 0,
                    is_high_priority,
                    bits_amount: None,
                });
            }
            SpeakEvent::Started {
                request_id,
                voice_id,
                engine_id,
                viewer_name,
                text,
                duration_secs,
            } => {
                self.queue.retain(|item| item.request_id != request_id);
                self.now_speaking = Some(NowSpeakingData {
                    viewer_name,
                    engine_voice: if voice_id.0.is_empty() {
                        String::new()
                    } else {
                        format!("{}/{}", engine_id.0, voice_id.0)
                    },
                    text,
                    progress: 0.0,
                    elapsed_secs: 0,
                    total_secs: duration_secs,
                });
            }
            SpeakEvent::Finished { .. } => {
                self.now_speaking = None;
                self.stats.spoken = self.stats.spoken.saturating_add(1);
            }
            SpeakEvent::Failed { .. } => {
                self.now_speaking = None;
            }
            SpeakEvent::Skipped { .. } => {
                self.now_speaking = None;
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

pub fn update(state: &mut TtsDashState, rt: &RuntimeView, msg: TtsDashMsg) -> Task<Message> {
    match msg {
        TtsDashMsg::SpeakEventReceived(event) => {
            state.apply_event(event);
            Task::none()
        }
        TtsDashMsg::PauseQueue => {
            let Some(handle) = rt.speak_queue.clone() else {
                return Task::none();
            };
            let cmd = if state.paused {
                SpeakCommand::Resume
            } else {
                SpeakCommand::Pause
            };
            state.paused = !state.paused;
            send_command(handle, cmd)
        }
        TtsDashMsg::SkipCurrent => {
            let Some(handle) = rt.speak_queue.clone() else {
                return Task::none();
            };
            send_command(handle, SpeakCommand::Skip)
        }
        TtsDashMsg::StopAll => {
            // Arms the confirm gate only (TT-01-F4 - was a bare
            // immediate-execute site with no confirm at all).
            state.pending_stop_all = true;
            Task::none()
        }
        TtsDashMsg::StopAllConfirmDismissed => {
            state.pending_stop_all = false;
            Task::none()
        }
        TtsDashMsg::StopAllConfirmAccepted => {
            state.pending_stop_all = false;
            state.queue.clear();
            state.now_speaking = None;
            let Some(handle) = rt.speak_queue.clone() else {
                return Task::none();
            };
            send_command(handle, SpeakCommand::Clear)
        }
        TtsDashMsg::VolumeChanged(v) => {
            state.volume = v;
            let Some(handle) = rt.speak_queue.clone() else {
                return Task::none();
            };
            send_command(handle, SpeakCommand::SetVolume(v))
        }
        TtsDashMsg::TestInputChanged(s) => {
            state.test_input = s;
            Task::none()
        }
        TtsDashMsg::SpeakTest => {
            let text = state.test_input.trim();
            if text.is_empty() {
                return Task::none();
            }
            let Some(handle) = rt.speak_queue.clone() else {
                return Task::none();
            };
            let text = text.to_owned();
            send_command(handle, SpeakCommand::Enqueue(test_speak_request(text)))
        }
        TtsDashMsg::CommandResult(Ok(())) => Task::none(),
        TtsDashMsg::CommandResult(Err(e)) => Task::done(Message::Toast(ToastMsg::Fired {
            kind: ToastKind::Error,
            message: format!("TTS command failed: {e}"),
            duration_ms: 5000,
            action: None,
        })),
    }
}

fn send_command(
    handle: std::sync::Arc<forge_speak_queue::SpeakQueueHandle>,
    cmd: SpeakCommand,
) -> Task<Message> {
    Task::perform(
        async move { handle.send(cmd).await.map_err(|e| e.to_string()) },
        |r| Message::Tts(TtsMsg::Dashboard(TtsDashMsg::CommandResult(r))),
    )
}

fn test_speak_request(text: String) -> SpeakRequest {
    SpeakRequest {
        request_id: RequestId::new(),
        viewer_id: String::new(),
        viewer_name: forge_widgets::tr!("tts_dash_test_speaker_name"),
        text,
        priority: Priority::Normal,
        alias_override: None,
        engine_override: None,
        voice_override: None,
        source_event_id: forge_types::EventId::new(),
        is_reward: false,
    }
}

pub fn tts_dashboard_view<'a>(
    state: &'a TtsDashState,
    rt: &'a RuntimeView,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let gap_sm = spf(Spacing::Xs);
    let gap_md = spf(Spacing::Sm);
    let gap_lg = spf(Spacing::Sm);

    let control_strip = control_strip_view(state, palette, gap_sm, gap_md);
    let now_speaking = now_speaking_view(state, palette, gap_sm);
    let queue_section = queue_section_view(state, palette, gap_sm);
    let right_pane = right_pane_view(state, rt, palette, gap_sm, gap_lg);

    let left_col = column![now_speaking, queue_section]
        .width(Length::Fill)
        .height(Length::Fill);

    let main_row = row![left_col, right_pane].height(Length::Fill);

    let main: Element<'a, Message> = column![control_strip, main_row].into();

    if state.pending_stop_all {
        let modal = confirm_modal(
            ConfirmModalParams {
                kind: ConfirmKind::Action,
                item_name: Cow::Owned(forge_widgets::tr!("tts_dash_stop_all_confirm_name")),
                cascade_hint: Some(Cow::Owned(forge_widgets::tr!(
                    "tts_dash_stop_all_confirm_hint"
                ))),
                tone: ConfirmTone::Destructive,
            },
            Message::Tts(TtsMsg::Dashboard(TtsDashMsg::StopAllConfirmAccepted)),
            Message::Tts(TtsMsg::Dashboard(TtsDashMsg::StopAllConfirmDismissed)),
            palette,
        );
        stack![main, modal].into()
    } else {
        main
    }
}

fn control_strip_view<'a>(
    state: &'a TtsDashState,
    palette: &'a ForgePalette,
    gap_sm: f32,
    gap_md: f32,
) -> Element<'a, Message> {
    let pause_label = if state.paused {
        forge_widgets::tr!("tts_dash_resume_btn")
    } else {
        forge_widgets::tr!("tts_dash_pause_btn")
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
            text(pause_label.clone()).size(FONT_SM),
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
    .padding([sp(Spacing::Xxs), sp(Spacing::Sm)]);

    let skip_btn = button(text(forge_widgets::tr!("tts_dash_skip_btn")).size(FONT_SM))
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
        .padding([sp(Spacing::Xxs), sp(Spacing::Sm)]);

    let stop_btn = button(text(forge_widgets::tr!("tts_dash_stop_all_btn")).size(FONT_SM))
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
        .padding([sp(Spacing::Xxs), sp(Spacing::Sm)]);

    let vol_pct = (state.volume * 100.0).round() as u32;
    let vol_text = text(format!("{vol_pct}%"))
        .size(FONT_SM)
        .color(palette.text_muted)
        .font(font(FontRole::Monospace));

    let vol_slider = forge_widgets::slider(
        0.0..=1.0,
        state.volume,
        |v| Message::Tts(TtsMsg::Dashboard(TtsDashMsg::VolumeChanged(v))),
        palette,
    )
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

    let test_input = text_input(
        &forge_widgets::tr!("tts_dash_test_placeholder"),
        &state.test_input,
    )
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

    let speak_btn = button(text(forge_widgets::tr!("tts_dash_speak_btn")).size(FONT_SM))
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
        .padding([sp(Spacing::Xxs), sp(Spacing::Sm)]);

    let right = row![test_input, speak_btn]
        .align_y(Alignment::Center)
        .spacing(gap_sm);

    forge_widgets::card(
        row![left, right]
            .align_y(Alignment::Center)
            .spacing(gap_md)
            .width(Length::Fill),
        palette,
    )
    .split_radius(0.0, 0.0)
    .padding([sp(Spacing::Xs), sp(Spacing::Md)])
    .width(Length::Fill)
    .into()
}

fn now_speaking_view<'a>(
    state: &'a TtsDashState,
    palette: &'a ForgePalette,
    gap_sm: f32,
) -> Element<'a, Message> {
    let header = text(forge_widgets::tr!("tts_dash_now_speaking_header"))
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
        column![
            header,
            text(forge_widgets::tr!("tts_dash_no_speaking"))
                .size(FONT_SM)
                .color(palette.text_muted),
        ]
        .spacing(gap_sm)
        .into()
    };

    forge_widgets::card(body, palette)
        .split_radius(0.0, 0.0)
        .padding([sp(Spacing::Sm), sp(Spacing::Md)])
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
            text(forge_widgets::tr!("tts_dash_queue_header"))
                .size(FONT_SM)
                .color(palette.text_primary),
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
            .padding([0, sp(Spacing::Xs)]),
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
    .padding([sp(Spacing::Xs), sp(Spacing::Md)])
    .width(Length::Fill);

    let items: Element<'a, Message> = if state.queue.is_empty() {
        container(
            text(forge_widgets::tr!("tts_dash_queue_empty"))
                .size(FONT_SM)
                .color(palette.text_muted),
        )
        .padding([sp(Spacing::Md), sp(Spacing::Md)])
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
            forge_widgets::tr!("tts_dash_priority_high")
        };
        container(
            text(bits_label)
                .size(FONT_XS)
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
        .padding([sp(Spacing::Xxs), sp(Spacing::Xxs)])
        .into()
    } else {
        Space::new().into()
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
    .spacing(spf(Spacing::Xxs))
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
    .padding([sp(Spacing::Xs), sp(Spacing::Md)])
    .width(Length::Fill)
    .into()
}

fn right_pane_view<'a>(
    state: &'a TtsDashState,
    rt: &'a RuntimeView,
    palette: &'a ForgePalette,
    gap_sm: f32,
    gap_lg: f32,
) -> Element<'a, Message> {
    let session_header = text(forge_widgets::tr!("tts_dash_session_header"))
        .size(FONT_XS)
        .color(palette.text_muted)
        .font(font(FontRole::Monospace));

    fn stat_row<'b>(
        label: String,
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
        .padding([sp(Spacing::Xxs), 0]);

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
        .unwrap_or_else(|| "-".to_string());

    let stats_col = column![
        session_header,
        stat_row(
            forge_widgets::tr!("tts_dash_stat_spoken"),
            state.stats.spoken.to_string(),
            palette.brand,
            palette,
            true
        ),
        stat_row(
            forge_widgets::tr!("tts_dash_stat_skipped"),
            state.stats.skipped.to_string(),
            palette.warning,
            palette,
            true
        ),
        stat_row(
            forge_widgets::tr!("tts_dash_stat_filtered"),
            state.stats.filtered.to_string(),
            palette.random,
            palette,
            true
        ),
        stat_row(
            forge_widgets::tr!("tts_dash_stat_avg_latency"),
            latency_str,
            palette.success,
            palette,
            false
        ),
    ]
    .spacing(gap_sm);

    let engines_header = text(forge_widgets::tr!("tts_dash_engines_header"))
        .size(FONT_XS)
        .color(palette.text_muted)
        .font(font(FontRole::Monospace));

    let engine_cards: Vec<Element<'a, Message>> = rt
        .tts_engine_ids
        .iter()
        .map(|id| {
            let kind = crate::tts_engines::engine_kind(&id.0);
            let status_color = if kind == "system" {
                palette.info
            } else {
                palette.success
            };
            engine_card(
                crate::tts_engines::engine_display_label(&id.0),
                format!("{kind} \u{b7} ready"),
                status_color,
                palette,
                gap_sm,
            )
        })
        .collect();
    let engines_col = column![engines_header]
        .push(column(engine_cards).spacing(gap_sm))
        .spacing(gap_sm);

    forge_widgets::card(
        scrollable(
            column![stats_col, engines_col]
                .spacing(gap_lg)
                .width(Length::Fill),
        )
        .height(Length::Fill),
        palette,
    )
    .background(palette.shell)
    .split_radius(0.0, 0.0)
    .padding([sp(Spacing::Sm), sp(Spacing::Sm)])
    .width(236)
    .into()
}

fn engine_card<'a>(
    name: String,
    meta: String,
    status_color: Color,
    palette: &'a ForgePalette,
    _gap_sm: f32,
) -> Element<'a, Message> {
    forge_widgets::card(
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
                .size(FONT_XS)
                .color(palette.text_muted)
                .font(font(FontRole::Monospace)),
        ]
        .spacing(spf(Spacing::Xxs)),
        palette,
    )
    .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
    .width(Length::Fill)
    .into()
}
