use std::sync::Arc;

use forge_storage::DataProvider;
use forge_storage_sqlite::SqliteBackend;
use forge_types::QueueId;
use iced::{
    Background, Border, Color, Element, Length,
    widget::{Space, column, container, row, text},
};
use time::OffsetDateTime;

use forge_widgets::{
    ForgePalette, Radius,
    icons::{Icon, tabler_icon},
    radius,
    tokens::{BORDER_THIN, FONT_SM, FONT_XS, FontRole, font},
};

use crate::Message;
use crate::message::QueuesMsg;

#[derive(Debug, Clone)]
pub struct QueueSummary {
    pub id: QueueId,
    pub name: String,
    pub blocking: bool,
    pub concurrency: u32,
    pub paused: bool,
    pub assigned_actions: u32,
    pub pending: u32,
    pub in_flight: u32,
    pub running_now: Vec<String>,
    pub description: Option<String>,
    pub paused_at: Option<OffsetDateTime>,
}

pub struct QueuesState {
    pub queues: Vec<QueueSummary>,
    pub loading: bool,
}

impl QueuesState {
    pub fn new() -> Self {
        Self {
            queues: vec![],
            loading: false,
        }
    }
}

impl Default for QueuesState {
    fn default() -> Self {
        Self::new()
    }
}

fn default_description(name: &str) -> Option<String> {
    match name {
        "Default" => {
            Some("Catch-all queue for actions without explicit queue assignment".to_owned())
        }
        "Alerts" => Some("Subs, raids, cheers · serialized so overlays don't overlap".to_owned()),
        "Background" => {
            Some("Logging, analytics, side-effect-free tasks · parallel execution".to_owned())
        }
        "Moderation" => {
            Some("Auto-bans, timeouts, message deletions · paused for review".to_owned())
        }
        _ => None,
    }
}

pub async fn load_queues(
    dp: Arc<SqliteBackend>,
    scheduler: Option<forge_runtime::QueueSchedulerHandle>,
) -> Result<Vec<QueueSummary>, String> {
    let queues = dp.queue_repo().list().await.map_err(|e| e.to_string())?;

    let actions = dp.action_repo().list().await.map_err(|e| e.to_string())?;

    let paused_ids = match scheduler {
        Some(h) => h.paused_queues().await.unwrap_or_default(),
        None => std::collections::HashSet::new(),
    };

    let summaries = queues
        .into_iter()
        .map(|q| {
            let assigned_actions = actions.iter().filter(|a| a.queue_id == q.id).count() as u32;

            // Queue concurrency is not yet a persisted column — derive from
            // blocking flag. Schema bump will land with Queue::concurrency once
            // the scheduler supports tunable parallelism.
            let concurrency: u32 = if q.blocking { 1 } else { 8 };

            let description = default_description(&q.name);
            let paused = paused_ids.contains(&q.id);

            QueueSummary {
                id: q.id,
                name: q.name,
                blocking: q.blocking,
                concurrency,
                paused,
                assigned_actions,
                pending: 0,
                in_flight: 0,
                running_now: vec![],
                description,
                paused_at: None,
            }
        })
        .collect();

    Ok(summaries)
}

pub fn queues_view<'a>(state: &'a QueuesState, palette: &'a ForgePalette) -> Element<'a, Message> {
    let total = state.queues.len();
    let running = state.queues.iter().filter(|q| !q.paused).count();
    let paused_count = state.queues.iter().filter(|q| q.paused).count();

    let stat_strip = row![
        text(total.to_string())
            .size(FONT_SM)
            .color(palette.text_primary),
        text(" queues").size(FONT_SM).color(palette.text_secondary),
        text("  ·  ").size(FONT_SM).color(palette.text_faint),
        text(running.to_string())
            .size(FONT_SM)
            .color(palette.success),
        text(" running").size(FONT_SM).color(palette.text_secondary),
        text("  ·  ").size(FONT_SM).color(palette.text_faint),
        text(paused_count.to_string())
            .size(FONT_SM)
            .color(palette.warning),
        text(" paused").size(FONT_SM).color(palette.text_secondary),
    ]
    .align_y(iced::Alignment::Center);

    let shell = palette.shell;
    let border = palette.border_regular;
    let top_bar = container(
        row![Space::new().width(Length::Fill), stat_strip,].align_y(iced::Alignment::Center),
    )
    .width(Length::Fill)
    .padding([8, 16])
    .style(move |_: &iced::Theme| iced::widget::container::Style {
        background: Some(Background::Color(shell)),
        border: Border {
            color: border,
            width: BORDER_THIN,
            radius: 0.0.into(),
        },
        ..Default::default()
    });

    let elevated = palette.elevated;
    let border_col = palette.border_regular;
    let desc_color = palette.text_secondary;
    let warning = palette.warning;
    let brand = palette.brand;
    let dark = palette.shell;

    let toolbar = container(
        row![
            text("Manage action queues, their concurrency, and pause state")
                .size(FONT_SM)
                .color(desc_color),
            Space::new().width(Length::Fill),
            pause_all_button(border_col, warning),
            new_queue_button(brand, dark),
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center),
    )
    .width(Length::Fill)
    .padding([8, 14])
    .style(move |_: &iced::Theme| iced::widget::container::Style {
        background: Some(Background::Color(elevated)),
        border: Border {
            color: border_col,
            width: BORDER_THIN,
            radius: 0.0.into(),
        },
        ..Default::default()
    });

    let grid = build_grid(state, palette);

    let base = palette.base;
    let body = container(grid)
        .width(Length::Fill)
        .padding(18)
        .style(move |_: &iced::Theme| iced::widget::container::Style {
            background: Some(Background::Color(base)),
            ..Default::default()
        });

    container(column![top_bar, toolbar, body].spacing(0))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn pause_all_button<'a>(border_col: Color, warning: Color) -> Element<'a, Message> {
    use iced::widget::button;

    let icon = tabler_icon(Icon::PlayerPause, 13.0, warning);

    let label = text("Pause all").size(FONT_SM).color(warning);

    button(
        row![icon, label]
            .spacing(5)
            .align_y(iced::Alignment::Center),
    )
    .on_press(Message::Queues(QueuesMsg::PauseAll))
    .padding([5, 11])
    .style(
        move |_: &iced::Theme, _status| iced::widget::button::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            border: Border {
                color: border_col,
                width: BORDER_THIN,
                radius: radius(Radius::Sm).into(),
            },
            text_color: warning,
            shadow: iced::Shadow::default(),
            snap: false,
        },
    )
    .into()
}

fn new_queue_button<'a>(brand: Color, dark: Color) -> Element<'a, Message> {
    use iced::widget::button;

    let icon = tabler_icon(Icon::Plus, 13.0, dark);

    let label = text("New queue").size(FONT_SM).color(dark);

    button(
        row![icon, label]
            .spacing(5)
            .align_y(iced::Alignment::Center),
    )
    .on_press(Message::Queues(QueuesMsg::NewQueue))
    .padding([5, 12])
    .style(
        move |_: &iced::Theme, _status| iced::widget::button::Style {
            background: Some(Background::Color(brand)),
            border: Border::default(),
            text_color: dark,
            shadow: iced::Shadow::default(),
            snap: false,
        },
    )
    .into()
}

fn build_grid<'a>(state: &'a QueuesState, palette: &'a ForgePalette) -> Element<'a, Message> {
    if state.queues.is_empty() {
        return column![
            text("No queues configured.")
                .size(FONT_SM)
                .color(palette.text_secondary)
        ]
        .into();
    }

    let mut rows: Vec<Element<'a, Message>> = vec![];
    let mut iter = state.queues.iter().peekable();

    while iter.peek().is_some() {
        let left = iter.next().map(|q| queue_card(q, palette));
        let right = iter.next().map(|q| queue_card(q, palette));

        let row_el: Element<'a, Message> = match (left, right) {
            (Some(l), Some(r)) => row![l, r].spacing(10).into(),
            (Some(l), None) => row![
                container(l).width(Length::FillPortion(1)),
                Space::new().width(Length::FillPortion(1)),
            ]
            .spacing(10)
            .into(),
            _ => unreachable!(),
        };

        rows.push(row_el);
    }

    column(rows).spacing(10).into()
}

fn queue_card<'a>(q: &'a QueueSummary, palette: &'a ForgePalette) -> Element<'a, Message> {
    let card_bg = palette.elevated;
    let border_color = if q.paused {
        Color {
            a: 0.35,
            ..palette.warning
        }
    } else {
        palette.border_regular
    };

    let header = queue_card_header(q, palette);
    let metrics = queue_card_metrics(q, palette);
    let running_panel = queue_running_panel(q, palette);
    let buttons = queue_card_buttons(q, palette);

    let inner = column![header, metrics, running_panel, buttons].spacing(10);

    container(inner)
        .width(Length::FillPortion(1))
        .padding(14)
        .style(move |_: &iced::Theme| iced::widget::container::Style {
            background: Some(Background::Color(card_bg)),
            border: Border {
                color: border_color,
                width: BORDER_THIN,
                radius: radius(Radius::Md).into(),
            },
            ..Default::default()
        })
        .into()
}

fn queue_card_header<'a>(q: &'a QueueSummary, palette: &'a ForgePalette) -> Element<'a, Message> {
    let name = text(q.name.clone())
        .size(14.0)
        .font(font(FontRole::Monospace))
        .color(palette.text_primary);

    let badge = status_badge(q.paused, palette);

    let dots_color = palette.text_faint;
    let dots = tabler_icon(Icon::DotsVertical, 14.0, dots_color);

    let desc_color = palette.text_secondary;
    let desc = if let Some(d) = &q.description {
        text(d.clone()).size(11.0).color(desc_color)
    } else {
        text("").size(11.0).color(desc_color)
    };

    let name_row = row![name, badge]
        .spacing(8)
        .align_y(iced::Alignment::Center);

    let left = column![name_row, desc].spacing(3);

    row![left, Space::new().width(Length::Fill), dots]
        .align_y(iced::Alignment::Start)
        .into()
}

fn status_badge<'a>(paused: bool, palette: &'a ForgePalette) -> Element<'a, Message> {
    let bg = palette.border_regular;
    if paused {
        let warning = palette.warning;
        let icon = tabler_icon(Icon::PlayerPause, 9.0, warning);
        let label = text("PAUSED")
            .size(9.5)
            .font(font(FontRole::Monospace))
            .color(warning);
        container(
            row![icon, label]
                .spacing(4)
                .align_y(iced::Alignment::Center),
        )
        .padding([1, 6])
        .style(move |_: &iced::Theme| iced::widget::container::Style {
            background: Some(Background::Color(bg)),
            border: Border {
                radius: 8.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
    } else {
        let success = palette.success;
        let dot = container(Space::new().width(5).height(5)).style(move |_: &iced::Theme| {
            iced::widget::container::Style {
                background: Some(Background::Color(success)),
                border: Border {
                    radius: 50.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        });
        let label = text("RUNNING")
            .size(9.5)
            .font(font(FontRole::Monospace))
            .color(success);
        container(row![dot, label].spacing(4).align_y(iced::Alignment::Center))
            .padding([1, 6])
            .style(move |_: &iced::Theme| iced::widget::container::Style {
                background: Some(Background::Color(bg)),
                border: Border {
                    radius: 8.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
    }
}

fn queue_card_metrics<'a>(q: &'a QueueSummary, palette: &'a ForgePalette) -> Element<'a, Message> {
    let label_color = palette.text_secondary;
    let value_color = palette.text_primary;
    let sub_color = palette.text_faint;
    let warning = palette.warning;

    let border_color = palette.border_regular;

    let concurrency_label_str = if q.blocking { "serial" } else { "parallel" };

    let pending_value_color = if q.paused { warning } else { value_color };
    let pending_sub_color = if q.paused { warning } else { sub_color };
    let pending_sub_str = if q.paused {
        "held"
    } else if q.in_flight > 0 {
        "in flight"
    } else {
        "idle"
    };

    let concurrency_col = column![
        text("CONCURRENCY")
            .size(FONT_XS)
            .font(font(FontRole::Monospace))
            .color(label_color),
        text(q.concurrency.to_string())
            .size(13.0)
            .font(font(FontRole::Monospace))
            .color(value_color),
        text(concurrency_label_str)
            .size(FONT_XS)
            .font(font(FontRole::Monospace))
            .color(sub_color),
    ]
    .spacing(3)
    .width(Length::FillPortion(1));

    let pending_col = column![
        text("PENDING")
            .size(FONT_XS)
            .font(font(FontRole::Monospace))
            .color(label_color),
        text(q.pending.to_string())
            .size(13.0)
            .font(font(FontRole::Monospace))
            .color(pending_value_color),
        text(pending_sub_str)
            .size(FONT_XS)
            .font(font(FontRole::Monospace))
            .color(pending_sub_color),
    ]
    .spacing(3)
    .width(Length::FillPortion(1));

    let actions_col = column![
        text("ACTIONS")
            .size(FONT_XS)
            .font(font(FontRole::Monospace))
            .color(label_color),
        text(q.assigned_actions.to_string())
            .size(13.0)
            .font(font(FontRole::Monospace))
            .color(value_color),
        text("assigned")
            .size(FONT_XS)
            .font(font(FontRole::Monospace))
            .color(sub_color),
    ]
    .spacing(3)
    .width(Length::FillPortion(1));

    let metrics_row = row![concurrency_col, pending_col, actions_col];

    container(metrics_row)
        .width(Length::Fill)
        .padding(iced::Padding {
            top: 10.0,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        })
        .style(move |_: &iced::Theme| iced::widget::container::Style {
            border: Border {
                color: border_color,
                width: BORDER_THIN,
                radius: 0.0.into(),
            },
            ..Default::default()
        })
        .into()
}

fn queue_running_panel<'a>(q: &'a QueueSummary, palette: &'a ForgePalette) -> Element<'a, Message> {
    let shell = palette.shell;

    if q.paused {
        let warning = palette.warning;
        let border_warning = Color {
            a: 0.2,
            ..palette.warning
        };
        let bg_warning = Color {
            a: 0.06,
            ..palette.warning
        };

        let paused_text = if let Some(at) = &q.paused_at {
            let now = OffsetDateTime::now_utc();
            let diff = now - *at;
            let mins = diff.whole_minutes();
            format!("{} actions waiting — paused {} min ago", q.pending, mins)
        } else {
            "queue is paused".to_owned()
        };

        let icon = tabler_icon(Icon::AlertTriangle, 12.0, warning);

        let msg = text(paused_text).size(11.0).color(palette.text_primary);

        container(row![icon, msg].spacing(8).align_y(iced::Alignment::Center))
            .width(Length::Fill)
            .padding([8, 10])
            .style(move |_: &iced::Theme| iced::widget::container::Style {
                background: Some(Background::Color(bg_warning)),
                border: Border {
                    color: border_warning,
                    width: BORDER_THIN,
                    radius: radius(Radius::Sm).into(),
                },
                ..Default::default()
            })
            .into()
    } else if q.running_now.is_empty() {
        let muted = palette.text_faint;
        let icon = tabler_icon(Icon::CircleDashed, 12.0, muted);
        let label = text("No actions running").size(11.0).color(muted);

        container(
            row![icon, label]
                .spacing(8)
                .align_y(iced::Alignment::Center),
        )
        .width(Length::Fill)
        .padding([6, 10])
        .style(move |_: &iced::Theme| iced::widget::container::Style {
            background: Some(Background::Color(shell)),
            border: Border {
                radius: radius(Radius::Sm).into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
    } else if !q.blocking && q.running_now.len() > 1 {
        concurrent_running_panel(q, palette, shell)
    } else {
        serial_running_panel(q, palette, shell)
    }
}

fn serial_running_panel<'a>(
    q: &'a QueueSummary,
    palette: &'a ForgePalette,
    shell: Color,
) -> Element<'a, Message> {
    let brand = palette.brand;
    let muted = palette.text_faint;

    let icon = tabler_icon(Icon::Loader2, 12.0, brand);

    let action_name = q.running_now.first().cloned().unwrap_or_default();
    let name_label = text(action_name)
        .size(11.0)
        .font(font(FontRole::Monospace))
        .color(palette.text_primary);

    let running_label = text("running —")
        .size(10.0)
        .font(font(FontRole::Monospace))
        .color(muted);

    container(
        row![
            icon,
            name_label,
            Space::new().width(Length::Fill),
            running_label,
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
    )
    .width(Length::Fill)
    .padding([6, 10])
    .style(move |_: &iced::Theme| iced::widget::container::Style {
        background: Some(Background::Color(shell)),
        border: Border {
            radius: radius(Radius::Sm).into(),
            ..Default::default()
        },
        ..Default::default()
    })
    .into()
}

fn concurrent_running_panel<'a>(
    q: &'a QueueSummary,
    palette: &'a ForgePalette,
    shell: Color,
) -> Element<'a, Message> {
    let muted = palette.text_faint;
    let pill_bg = palette.border_regular;
    let text_col = palette.text_primary;

    let header = text("RUNNING NOW")
        .size(FONT_XS)
        .font(font(FontRole::Monospace))
        .color(muted);

    const MAX_PILLS: usize = 3;
    let shown = &q.running_now[..q.running_now.len().min(MAX_PILLS)];
    let overflow = q.running_now.len().saturating_sub(MAX_PILLS);

    let mut pills: Vec<Element<'a, Message>> = shown
        .iter()
        .map(|name| {
            let n = name.clone();
            let bg = pill_bg;
            let tc = text_col;
            container(text(n).size(10.0).font(font(FontRole::Monospace)).color(tc))
                .padding([2, 6])
                .style(move |_: &iced::Theme| iced::widget::container::Style {
                    background: Some(Background::Color(bg)),
                    border: Border {
                        radius: 5.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .into()
        })
        .collect();

    if overflow > 0 {
        let bg = pill_bg;
        let tc = text_col;
        let overflow_pill: Element<'a, Message> = container(
            text(format!("+{overflow} more"))
                .size(10.0)
                .font(font(FontRole::Monospace))
                .color(tc),
        )
        .padding([2, 6])
        .style(move |_: &iced::Theme| iced::widget::container::Style {
            background: Some(Background::Color(bg)),
            border: Border {
                radius: 5.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into();
        pills.push(overflow_pill);
    }

    let pills_row = row(pills).spacing(4).wrap();

    container(column![header, pills_row].spacing(4))
        .width(Length::Fill)
        .padding([6, 10])
        .style(move |_: &iced::Theme| iced::widget::container::Style {
            background: Some(Background::Color(shell)),
            border: Border {
                radius: radius(Radius::Sm).into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

fn queue_card_buttons<'a>(q: &'a QueueSummary, palette: &'a ForgePalette) -> Element<'a, Message> {
    use iced::widget::button;

    let id = q.id;
    let border_col = palette.border_regular;
    let muted = palette.text_secondary;
    let warning = palette.warning;
    let success = palette.success;
    let dark = palette.shell;

    let action_btn: Element<'a, Message> = if q.paused {
        let icon = tabler_icon(Icon::PlayerPlay, 12.0, dark);
        let label = text("Resume").size(FONT_SM).color(dark);
        button(
            row![icon, label]
                .spacing(5)
                .align_y(iced::Alignment::Center),
        )
        .on_press(Message::Queues(QueuesMsg::ResumeQueue(id)))
        .padding([5, 10])
        .width(Length::FillPortion(1))
        .style(
            move |_: &iced::Theme, _status| iced::widget::button::Style {
                background: Some(Background::Color(success)),
                border: Border::default(),
                text_color: dark,
                shadow: iced::Shadow::default(),
                snap: false,
            },
        )
        .into()
    } else {
        let icon = tabler_icon(Icon::PlayerPause, 12.0, warning);
        let label = text("Pause").size(FONT_SM).color(warning);
        button(
            row![icon, label]
                .spacing(5)
                .align_y(iced::Alignment::Center),
        )
        .on_press(Message::Queues(QueuesMsg::PauseQueue(id)))
        .padding([5, 10])
        .width(Length::FillPortion(1))
        .style(
            move |_: &iced::Theme, _status| iced::widget::button::Style {
                background: Some(Background::Color(Color::TRANSPARENT)),
                border: Border {
                    color: border_col,
                    width: BORDER_THIN,
                    radius: radius(Radius::Sm).into(),
                },
                text_color: warning,
                shadow: iced::Shadow::default(),
                snap: false,
            },
        )
        .into()
    };

    let drain_icon = tabler_icon(Icon::Eraser, 12.0, muted);
    let drain_label = text("Drain").size(FONT_SM).color(muted);
    let drain_btn: Element<'a, Message> = button(
        row![drain_icon, drain_label]
            .spacing(5)
            .align_y(iced::Alignment::Center),
    )
    .on_press(Message::Queues(QueuesMsg::DrainQueue(id)))
    .padding([5, 10])
    .width(Length::FillPortion(1))
    .style(
        move |_: &iced::Theme, _status| iced::widget::button::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            border: Border {
                color: border_col,
                width: BORDER_THIN,
                radius: radius(Radius::Sm).into(),
            },
            text_color: muted,
            shadow: iced::Shadow::default(),
            snap: false,
        },
    )
    .into();

    let cfg_icon = tabler_icon(Icon::Settings, 12.0, muted);
    let cfg_label = text("Configure").size(FONT_SM).color(muted);
    let cfg_btn: Element<'a, Message> = button(
        row![cfg_icon, cfg_label]
            .spacing(5)
            .align_y(iced::Alignment::Center),
    )
    .on_press(Message::Noop)
    .padding([5, 10])
    .width(Length::FillPortion(1))
    .style(
        move |_: &iced::Theme, _status| iced::widget::button::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            border: Border {
                color: border_col,
                width: BORDER_THIN,
                radius: radius(Radius::Sm).into(),
            },
            text_color: muted,
            shadow: iced::Shadow::default(),
            snap: false,
        },
    )
    .into();

    row![action_btn, drain_btn, cfg_btn]
        .spacing(6)
        .width(Length::Fill)
        .into()
}
