use std::sync::Arc;

use forge_storage::{ActionRepo, QueueRepo};
use forge_types::{Queue, QueueId};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Task,
    widget::{Space, button, column, container, row, rule, stack, text},
};
use time::OffsetDateTime;

use forge_widgets::{
    ForgePalette, Radius, Spacing, ToggleProps,
    icons::{Icon, tabler_icon},
    primary_button, radius, secondary_button, section_header, sp, spf, text_input_field, toggle,
    tokens::{BORDER_THIN, FONT_SM, FONT_XS, FontRole, font},
};

use crate::Message;
use crate::message::QueuesMsg;
use crate::runtime_view::RuntimeView;

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
    pub paused_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Default)]
pub struct NewQueueForm {
    pub name: String,
    pub blocking: bool,
    pub saving: bool,
}

#[derive(Debug, Clone)]
pub struct EditQueueForm {
    pub id: QueueId,
    pub name: String,
    pub blocking: bool,
    pub saving: bool,
}

pub struct QueuesState {
    pub queues: Vec<QueueSummary>,
    pub loading: bool,
    pub new_queue_form: Option<NewQueueForm>,
    pub edit_queue_form: Option<EditQueueForm>,
}

impl QueuesState {
    pub fn new() -> Self {
        Self {
            queues: vec![],
            loading: false,
            new_queue_form: None,
            edit_queue_form: None,
        }
    }
}

impl Default for QueuesState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn update(state: &mut QueuesState, rt: &RuntimeView, msg: QueuesMsg) -> Task<Message> {
    match msg {
        QueuesMsg::LoadRequested => {
            state.loading = true;
            let queues = rt.backend.queue_repo();
            let actions = rt.backend.action_repo();
            let scheduler = rt.scheduler.clone();
            Task::perform(
                async move { load_queues(queues, actions, scheduler).await },
                |r| Message::Queues(QueuesMsg::QueuesLoaded(r)),
            )
        }
        QueuesMsg::QueuesLoaded(Ok(qs)) => {
            state.queues = qs;
            state.loading = false;
            Task::none()
        }
        QueuesMsg::QueuesLoaded(Err(e)) => {
            state.loading = false;
            tracing::warn!(error = %e, "queues load failed");
            Task::none()
        }
        QueuesMsg::PauseQueue(id) => {
            if let Some(q) = state.queues.iter_mut().find(|q| q.id == id) {
                q.paused = true;
            }
            let Some(scheduler) = rt.scheduler.clone() else {
                return Task::none();
            };
            Task::perform(
                async move { scheduler.pause(id).await.map_err(|e| e.to_string()) },
                |r| Message::Queues(QueuesMsg::PauseResult(r)),
            )
        }
        QueuesMsg::ResumeQueue(id) => {
            if let Some(q) = state.queues.iter_mut().find(|q| q.id == id) {
                q.paused = false;
            }
            let Some(scheduler) = rt.scheduler.clone() else {
                return Task::none();
            };
            Task::perform(
                async move { scheduler.resume(id).await.map_err(|e| e.to_string()) },
                |r| Message::Queues(QueuesMsg::ResumeResult(r)),
            )
        }
        QueuesMsg::DrainQueue(id) => {
            for q in &mut state.queues {
                if q.id == id {
                    q.paused = true;
                }
            }
            let Some(scheduler) = rt.scheduler.clone() else {
                return Task::none();
            };
            let bus = Arc::clone(&rt.bus);
            Task::perform(
                async move {
                    bus.publish(forge_events::Event::new(
                        forge_events::EventSource::Core,
                        "queue.drain_requested",
                        serde_json::json!({ "queue_id": id.to_string() }),
                    ));
                    scheduler.pause(id).await.map_err(|e| e.to_string())
                },
                |r| Message::Queues(QueuesMsg::PauseResult(r)),
            )
        }
        QueuesMsg::PauseAll => {
            for q in &mut state.queues {
                q.paused = true;
            }
            let ids: Vec<_> = state.queues.iter().map(|q| q.id).collect();
            let Some(scheduler) = rt.scheduler.clone() else {
                return Task::none();
            };
            Task::perform(
                async move {
                    for id in ids {
                        if let Err(e) = scheduler.pause(id).await {
                            tracing::warn!(queue_id = %id, error = %e, "pause queue failed");
                        }
                    }
                },
                |()| Message::Noop,
            )
        }
        QueuesMsg::NewQueue => {
            state.new_queue_form = Some(NewQueueForm::default());
            Task::none()
        }
        QueuesMsg::NewQueueNameChanged(name) => {
            if let Some(form) = state.edit_queue_form.as_mut() {
                form.name = name;
            } else if let Some(form) = state.new_queue_form.as_mut() {
                form.name = name;
            }
            Task::none()
        }
        QueuesMsg::NewQueueBlockingToggled => {
            if let Some(form) = state.edit_queue_form.as_mut() {
                form.blocking = !form.blocking;
            } else if let Some(form) = state.new_queue_form.as_mut() {
                form.blocking = !form.blocking;
            }
            Task::none()
        }
        QueuesMsg::NewQueueCancel => {
            state.new_queue_form = None;
            Task::none()
        }
        QueuesMsg::NewQueueSubmit => {
            let Some(form) = state.new_queue_form.as_mut() else {
                return Task::none();
            };
            let name = form.name.trim().to_string();
            if name.is_empty() {
                return Task::none();
            }
            form.saving = true;
            let queue = Queue {
                id: QueueId::new(),
                name,
                blocking: form.blocking,
            };
            let repo = rt.backend.queue_repo();
            Task::perform(
                async move { repo.save(&queue).await.map_err(|e| e.to_string()) },
                |r| Message::Queues(QueuesMsg::NewQueueSubmitResult(r)),
            )
        }
        QueuesMsg::NewQueueSubmitResult(Ok(())) => {
            state.new_queue_form = None;
            Task::done(Message::Queues(QueuesMsg::LoadRequested))
        }
        QueuesMsg::NewQueueSubmitResult(Err(e)) => {
            if let Some(form) = state.new_queue_form.as_mut() {
                form.saving = false;
            }
            tracing::warn!(error = %e, "create queue failed");
            Task::none()
        }
        QueuesMsg::ConfigureQueue(id, name, blocking) => {
            state.edit_queue_form = Some(EditQueueForm {
                id,
                name,
                blocking,
                saving: false,
            });
            Task::none()
        }
        QueuesMsg::EditQueueSubmit => {
            let Some(form) = state.edit_queue_form.as_mut() else {
                return Task::none();
            };
            let name = form.name.trim().to_string();
            if name.is_empty() {
                return Task::none();
            }
            form.saving = true;
            let queue = Queue {
                id: form.id,
                name,
                blocking: form.blocking,
            };
            let repo = rt.backend.queue_repo();
            Task::perform(
                async move { repo.save(&queue).await.map_err(|e| e.to_string()) },
                |r| Message::Queues(QueuesMsg::EditQueueSubmitResult(r)),
            )
        }
        QueuesMsg::EditQueueSubmitResult(Ok(())) => {
            state.edit_queue_form = None;
            Task::done(Message::Queues(QueuesMsg::LoadRequested))
        }
        QueuesMsg::EditQueueSubmitResult(Err(e)) => {
            if let Some(form) = state.edit_queue_form.as_mut() {
                form.saving = false;
            }
            tracing::warn!(error = %e, "edit queue failed");
            Task::none()
        }
        QueuesMsg::EditQueueCancel => {
            state.edit_queue_form = None;
            Task::none()
        }
        QueuesMsg::PauseResult(Ok(())) => Task::none(),
        QueuesMsg::PauseResult(Err(e)) => {
            tracing::warn!(error = %e, "pause queue failed");
            Task::none()
        }
        QueuesMsg::ResumeResult(Ok(())) => Task::none(),
        QueuesMsg::ResumeResult(Err(e)) => {
            tracing::warn!(error = %e, "resume queue failed");
            Task::none()
        }
    }
}

fn default_description(name: &str) -> Option<String> {
    match name {
        "Default" => Some(forge_widgets::tr!("queues_desc_default")),
        "Alerts" => Some(forge_widgets::tr!("queues_desc_alerts")),
        "Background" => Some(forge_widgets::tr!("queues_desc_background")),
        "Moderation" => Some(forge_widgets::tr!("queues_desc_moderation")),
        _ => None,
    }
}

pub async fn load_queues(
    queues_repo: Arc<dyn QueueRepo>,
    actions_repo: Arc<dyn ActionRepo>,
    scheduler: Option<forge_runtime::QueueSchedulerHandle>,
) -> Result<Vec<QueueSummary>, String> {
    let queues = queues_repo.list().await.map_err(|e| e.to_string())?;

    let actions = actions_repo.list().await.map_err(|e| e.to_string())?;

    let paused_ids = match scheduler {
        Some(h) => h.paused_queues().await.unwrap_or_default(),
        None => std::collections::HashSet::new(),
    };

    let summaries = queues
        .into_iter()
        .map(|q| {
            let assigned_actions = actions.iter().filter(|a| a.queue_id == q.id).count() as u32;

            // Queue concurrency is not yet a persisted column; derive from blocking flag.
            let concurrency: u32 = if q.blocking { 1 } else { 8 };

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
                paused_at: None,
            }
        })
        .collect();

    Ok(summaries)
}

pub fn queues_view<'a>(state: &'a QueuesState, palette: &'a ForgePalette) -> Element<'a, Message> {
    let border_col = palette.border_regular;
    let warning = palette.warning;
    let brand = palette.brand;
    let dark = palette.shell;

    let right_side = row![
        pause_all_button(border_col, warning),
        new_queue_button(brand, dark),
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(iced::Alignment::Center);

    let page_header = crate::page_chrome::page_header_with_actions(
        &[
            (forge_widgets::tr!("queues_breadcrumb_automation"), false),
            (forge_widgets::tr!("queues_breadcrumb_queues"), true),
        ],
        Some(right_side.into()),
        palette,
    );

    let grid = build_grid(state, palette);

    let base = palette.base;
    let body = container(grid)
        .width(Length::Fill)
        .padding(sp(Spacing::Md))
        .style(move |_: &iced::Theme| iced::widget::container::Style {
            background: Some(Background::Color(base)),
            ..Default::default()
        });

    let page = container(column![page_header, body].spacing(0))
        .width(Length::Fill)
        .height(Length::Fill);

    if let Some(form) = &state.edit_queue_form {
        stack![page, edit_queue_modal(form, palette)].into()
    } else if let Some(form) = &state.new_queue_form {
        stack![page, new_queue_modal(form, palette)].into()
    } else {
        page.into()
    }
}

fn new_queue_modal<'a>(form: &'a NewQueueForm, palette: &'a ForgePalette) -> Element<'a, Message> {
    let p = *palette;

    let backdrop = button(Space::new().width(Length::Fill).height(Length::Fill))
        .on_press(Message::Queues(QueuesMsg::NewQueueCancel))
        .padding(0)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_: &iced::Theme, _status| iced::widget::button::Style {
            background: Some(Background::Color(Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.5,
            })),
            border: Border::default(),
            text_color: Color::TRANSPARENT,
            shadow: iced::Shadow::default(),
            snap: false,
        });

    let title = container(
        text(forge_widgets::tr!("queues_create_title"))
            .size(FONT_SM)
            .color(p.text_primary)
            .font(font(FontRole::Body)),
    )
    .width(Length::Fill)
    .padding([sp(Spacing::Sm), sp(Spacing::Md)]);

    let name_section = column![
        section_header(
            forge_widgets::tr!("queues_create_name_label"),
            None,
            palette
        ),
        text_input_field(
            forge_widgets::tr!("queues_create_name_placeholder"),
            form.name.as_str(),
            |s| Message::Queues(QueuesMsg::NewQueueNameChanged(s)),
            palette,
        ),
    ]
    .spacing(spf(Spacing::Xs))
    .padding([0, sp(Spacing::Md)]);

    let blocking_section = container(toggle(
        palette,
        ToggleProps {
            label: forge_widgets::tr!("queues_create_blocking_label"),
            description: forge_widgets::tr!("queues_create_blocking_desc"),
            value: form.blocking,
            on_toggle: Message::Queues(QueuesMsg::NewQueueBlockingToggled),
        },
    ))
    .padding([sp(Spacing::Sm), sp(Spacing::Md)]);

    let divider_style = move |_: &iced::Theme| rule::Style {
        color: p.border_regular,
        radius: 0.0.into(),
        fill_mode: rule::FillMode::Full,
        snap: true,
    };

    let can_create = !form.name.trim().is_empty() && !form.saving;
    let create_lbl = forge_widgets::tr!("queues_create_btn");
    let create_el: Element<'_, Message> = if can_create {
        primary_button(
            create_lbl,
            Message::Queues(QueuesMsg::NewQueueSubmit),
            palette,
        )
    } else {
        container(
            text(create_lbl)
                .size(FONT_SM)
                .color(Color { a: 0.5, ..p.shell })
                .font(font(FontRole::Body)),
        )
        .padding([sp(Spacing::Sm), sp(Spacing::Md)])
        .style(move |_: &iced::Theme| iced::widget::container::Style {
            background: Some(Background::Color(Color { a: 0.4, ..p.brand })),
            border: Border {
                radius: radius(Radius::Md).into(),
                color: Color::TRANSPARENT,
                width: 0.0,
            },
            ..iced::widget::container::Style::default()
        })
        .into()
    };

    let footer = container(
        row![
            secondary_button(
                forge_widgets::tr!("queues_create_cancel"),
                Message::Queues(QueuesMsg::NewQueueCancel),
                palette,
            ),
            Space::new().width(Length::Fill),
            create_el,
        ]
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding([sp(Spacing::Xs), sp(Spacing::Md)])
    .style(move |_: &iced::Theme| iced::widget::container::Style {
        border: Border {
            color: p.border_regular,
            width: BORDER_THIN,
            radius: 0.0.into(),
        },
        ..iced::widget::container::Style::default()
    });

    let inner = column![
        title,
        rule::horizontal(1.0).style(divider_style),
        name_section,
        blocking_section,
        rule::horizontal(1.0).style(divider_style),
        footer,
    ]
    .width(Length::Fill);

    let card = container(inner)
        .max_width(440)
        .style(move |_: &iced::Theme| iced::widget::container::Style {
            background: Some(Background::Color(p.elevated)),
            border: Border {
                color: p.border_regular,
                width: BORDER_THIN,
                radius: radius(Radius::Lg).into(),
            },
            ..iced::widget::container::Style::default()
        });

    let centered = container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center);

    stack![backdrop, centered].into()
}

fn edit_queue_modal<'a>(
    form: &'a EditQueueForm,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let p = *palette;

    let backdrop = button(Space::new().width(Length::Fill).height(Length::Fill))
        .on_press(Message::Queues(QueuesMsg::EditQueueCancel))
        .padding(0)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_: &iced::Theme, _status| iced::widget::button::Style {
            background: Some(Background::Color(Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.5,
            })),
            border: Border::default(),
            text_color: Color::TRANSPARENT,
            shadow: iced::Shadow::default(),
            snap: false,
        });

    let title = container(
        text(forge_widgets::tr!("queues_edit_title"))
            .size(FONT_SM)
            .color(p.text_primary)
            .font(font(FontRole::Body)),
    )
    .width(Length::Fill)
    .padding([sp(Spacing::Sm), sp(Spacing::Md)]);

    let name_section = column![
        section_header(
            forge_widgets::tr!("queues_create_name_label"),
            None,
            palette
        ),
        text_input_field(
            forge_widgets::tr!("queues_create_name_placeholder"),
            form.name.as_str(),
            |s| Message::Queues(QueuesMsg::NewQueueNameChanged(s)),
            palette,
        ),
    ]
    .spacing(spf(Spacing::Xs))
    .padding([0, sp(Spacing::Md)]);

    let blocking_section = container(toggle(
        palette,
        ToggleProps {
            label: forge_widgets::tr!("queues_create_blocking_label"),
            description: forge_widgets::tr!("queues_create_blocking_desc"),
            value: form.blocking,
            on_toggle: Message::Queues(QueuesMsg::NewQueueBlockingToggled),
        },
    ))
    .padding([sp(Spacing::Sm), sp(Spacing::Md)]);

    let divider_style = move |_: &iced::Theme| rule::Style {
        color: p.border_regular,
        radius: 0.0.into(),
        fill_mode: rule::FillMode::Full,
        snap: true,
    };

    let can_save = !form.name.trim().is_empty() && !form.saving;
    let save_lbl = forge_widgets::tr!("common_save");
    let save_el: Element<'_, Message> = if can_save {
        primary_button(
            save_lbl,
            Message::Queues(QueuesMsg::EditQueueSubmit),
            palette,
        )
    } else {
        container(
            text(save_lbl)
                .size(FONT_SM)
                .color(Color { a: 0.5, ..p.shell })
                .font(font(FontRole::Body)),
        )
        .padding([sp(Spacing::Sm), sp(Spacing::Md)])
        .style(move |_: &iced::Theme| iced::widget::container::Style {
            background: Some(Background::Color(Color { a: 0.4, ..p.brand })),
            border: Border {
                radius: radius(Radius::Md).into(),
                color: Color::TRANSPARENT,
                width: 0.0,
            },
            ..iced::widget::container::Style::default()
        })
        .into()
    };

    let footer = container(
        row![
            secondary_button(
                forge_widgets::tr!("queues_create_cancel"),
                Message::Queues(QueuesMsg::EditQueueCancel),
                palette,
            ),
            Space::new().width(Length::Fill),
            save_el,
        ]
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding([sp(Spacing::Xs), sp(Spacing::Md)])
    .style(move |_: &iced::Theme| iced::widget::container::Style {
        border: Border {
            color: p.border_regular,
            width: BORDER_THIN,
            radius: 0.0.into(),
        },
        ..iced::widget::container::Style::default()
    });

    let inner = column![
        title,
        rule::horizontal(1.0).style(divider_style),
        name_section,
        blocking_section,
        rule::horizontal(1.0).style(divider_style),
        footer,
    ]
    .width(Length::Fill);

    let card = container(inner)
        .max_width(440)
        .style(move |_: &iced::Theme| iced::widget::container::Style {
            background: Some(Background::Color(p.elevated)),
            border: Border {
                color: p.border_regular,
                width: BORDER_THIN,
                radius: radius(Radius::Lg).into(),
            },
            ..iced::widget::container::Style::default()
        });

    let centered = container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center);

    stack![backdrop, centered].into()
}

fn pause_all_button<'a>(border_col: Color, warning: Color) -> Element<'a, Message> {
    use iced::widget::button;

    let icon = tabler_icon(Icon::PlayerPause, 13.0, warning);

    let label = text(forge_widgets::tr!("queues_pause_all_btn"))
        .size(FONT_SM)
        .color(warning);

    button(
        row![icon, label]
            .spacing(spf(Spacing::Xxs))
            .align_y(iced::Alignment::Center),
    )
    .on_press(Message::Queues(QueuesMsg::PauseAll))
    .padding([sp(Spacing::Xxs), sp(Spacing::Sm)])
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

    let label = text(forge_widgets::tr!("queues_new_queue_btn"))
        .size(FONT_SM)
        .color(dark);

    button(
        row![icon, label]
            .spacing(spf(Spacing::Xxs))
            .align_y(iced::Alignment::Center),
    )
    .on_press(Message::Queues(QueuesMsg::NewQueue))
    .padding([sp(Spacing::Xxs), sp(Spacing::Sm)])
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
            text(forge_widgets::tr!("queues_empty"))
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
            (Some(l), Some(r)) => row![l, r].spacing(spf(Spacing::Xs)).into(),
            (Some(l), None) => row![
                container(l).width(Length::FillPortion(1)),
                Space::new().width(Length::FillPortion(1)),
            ]
            .spacing(spf(Spacing::Xs))
            .into(),
            _ => unreachable!(),
        };

        rows.push(row_el);
    }

    column(rows).spacing(spf(Spacing::Xs)).into()
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

    let inner = column![header, metrics, running_panel, buttons].spacing(spf(Spacing::Xs));

    container(inner)
        .width(Length::FillPortion(1))
        .padding(sp(Spacing::Sm))
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
        .size(FONT_SM)
        .font(font(FontRole::Monospace))
        .color(palette.text_primary);

    let badge = status_badge(q.paused, palette);

    let dots_color = palette.text_faint;
    let dots = tabler_icon(Icon::DotsVertical, 14.0, dots_color);

    let desc_color = palette.text_secondary;
    let desc = match default_description(&q.name) {
        Some(d) => text(d).size(FONT_XS).color(desc_color),
        None => text("").size(FONT_XS).color(desc_color),
    };

    let name_row = row![name, badge]
        .spacing(spf(Spacing::Xs))
        .align_y(iced::Alignment::Center);

    let left = column![name_row, desc].spacing(spf(Spacing::Xxs));

    row![left, Space::new().width(Length::Fill), dots]
        .align_y(iced::Alignment::Start)
        .into()
}

fn status_badge<'a>(paused: bool, palette: &'a ForgePalette) -> Element<'a, Message> {
    let bg = palette.border_regular;
    if paused {
        let warning = palette.warning;
        let icon = tabler_icon(Icon::PlayerPause, 9.0, warning);
        let label = text(forge_widgets::tr!("queues_status_paused"))
            .size(FONT_XS)
            .font(font(FontRole::Monospace))
            .color(warning);
        container(
            row![icon, label]
                .spacing(spf(Spacing::Xxs))
                .align_y(iced::Alignment::Center),
        )
        .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
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
        let label = text(forge_widgets::tr!("queues_status_running"))
            .size(FONT_XS)
            .font(font(FontRole::Monospace))
            .color(success);
        container(
            row![dot, label]
                .spacing(spf(Spacing::Xxs))
                .align_y(iced::Alignment::Center),
        )
        .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
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

    let concurrency_label_str = if q.blocking {
        forge_widgets::tr!("queues_metric_serial")
    } else {
        forge_widgets::tr!("queues_metric_parallel")
    };

    let pending_value_color = if q.paused { warning } else { value_color };
    let pending_sub_color = if q.paused { warning } else { sub_color };
    let pending_sub_str = if q.paused {
        forge_widgets::tr!("queues_metric_held")
    } else if q.in_flight > 0 {
        forge_widgets::tr!("queues_metric_in_flight")
    } else {
        forge_widgets::tr!("queues_metric_idle")
    };

    let concurrency_col = column![
        text(forge_widgets::tr!("queues_metric_concurrency"))
            .size(FONT_XS)
            .font(font(FontRole::Monospace))
            .color(label_color),
        text(q.concurrency.to_string())
            .size(FONT_SM)
            .font(font(FontRole::Monospace))
            .color(value_color),
        text(concurrency_label_str)
            .size(FONT_XS)
            .font(font(FontRole::Monospace))
            .color(sub_color),
    ]
    .spacing(spf(Spacing::Xxs))
    .width(Length::FillPortion(1));

    let pending_col = column![
        text(forge_widgets::tr!("queues_metric_pending"))
            .size(FONT_XS)
            .font(font(FontRole::Monospace))
            .color(label_color),
        text(q.pending.to_string())
            .size(FONT_SM)
            .font(font(FontRole::Monospace))
            .color(pending_value_color),
        text(pending_sub_str)
            .size(FONT_XS)
            .font(font(FontRole::Monospace))
            .color(pending_sub_color),
    ]
    .spacing(spf(Spacing::Xxs))
    .width(Length::FillPortion(1));

    let actions_col = column![
        text(forge_widgets::tr!("queues_metric_actions"))
            .size(FONT_XS)
            .font(font(FontRole::Monospace))
            .color(label_color),
        text(q.assigned_actions.to_string())
            .size(FONT_SM)
            .font(font(FontRole::Monospace))
            .color(value_color),
        text(forge_widgets::tr!("queues_metric_assigned"))
            .size(FONT_XS)
            .font(font(FontRole::Monospace))
            .color(sub_color),
    ]
    .spacing(spf(Spacing::Xxs))
    .width(Length::FillPortion(1));

    let metrics_row = row![concurrency_col, pending_col, actions_col];

    container(metrics_row)
        .width(Length::Fill)
        .padding(iced::Padding {
            top: spf(Spacing::Xs),
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
            forge_widgets::tr!(
                "queues_paused_with_time",
                pending = q.pending as i64,
                mins = mins
            )
        } else {
            forge_widgets::tr!("queues_paused_simple")
        };

        let icon = tabler_icon(Icon::AlertTriangle, 12.0, warning);

        let msg = text(paused_text).size(FONT_XS).color(palette.text_primary);

        container(
            row![icon, msg]
                .spacing(spf(Spacing::Xs))
                .align_y(iced::Alignment::Center),
        )
        .width(Length::Fill)
        .padding([sp(Spacing::Xs), sp(Spacing::Xs)])
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
        let label = text(forge_widgets::tr!("queues_no_actions_running"))
            .size(FONT_XS)
            .color(muted);

        container(
            row![icon, label]
                .spacing(spf(Spacing::Xs))
                .align_y(iced::Alignment::Center),
        )
        .width(Length::Fill)
        .padding([sp(Spacing::Xs), sp(Spacing::Xs)])
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
        .size(FONT_XS)
        .font(font(FontRole::Monospace))
        .color(palette.text_primary);

    let running_label = text(forge_widgets::tr!("queues_running_label"))
        .size(FONT_XS)
        .font(font(FontRole::Monospace))
        .color(muted);

    container(
        row![
            icon,
            name_label,
            Space::new().width(Length::Fill),
            running_label,
        ]
        .spacing(spf(Spacing::Xs))
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

    let header = text(forge_widgets::tr!("queues_running_now_header"))
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
            container(
                text(n)
                    .size(FONT_XS)
                    .font(font(FontRole::Monospace))
                    .color(tc),
            )
            .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
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
            text(forge_widgets::tr!(
                "queues_overflow_more",
                count = overflow as i64
            ))
            .size(FONT_XS)
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

    let pills_row = row(pills).spacing(spf(Spacing::Xxs)).wrap();

    container(column![header, pills_row].spacing(spf(Spacing::Xxs)))
        .width(Length::Fill)
        .padding([sp(Spacing::Xs), sp(Spacing::Xs)])
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
        let label = text(forge_widgets::tr!("queues_resume_btn"))
            .size(FONT_SM)
            .color(dark);
        button(
            row![icon, label]
                .spacing(spf(Spacing::Xxs))
                .align_y(iced::Alignment::Center),
        )
        .on_press(Message::Queues(QueuesMsg::ResumeQueue(id)))
        .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
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
        let label = text(forge_widgets::tr!("queues_pause_btn"))
            .size(FONT_SM)
            .color(warning);
        button(
            row![icon, label]
                .spacing(spf(Spacing::Xxs))
                .align_y(iced::Alignment::Center),
        )
        .on_press(Message::Queues(QueuesMsg::PauseQueue(id)))
        .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
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
    let drain_label = text(forge_widgets::tr!("queues_drain_btn"))
        .size(FONT_SM)
        .color(muted);
    let drain_btn: Element<'a, Message> = button(
        row![drain_icon, drain_label]
            .spacing(spf(Spacing::Xxs))
            .align_y(iced::Alignment::Center),
    )
    .on_press(Message::Queues(QueuesMsg::DrainQueue(id)))
    .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
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

    let cfg_name = q.name.clone();
    let cfg_blocking = q.blocking;
    let cfg_icon = tabler_icon(Icon::Settings, 12.0, muted);
    let cfg_label = text(forge_widgets::tr!("queues_configure_btn"))
        .size(FONT_SM)
        .color(muted);
    let cfg_btn: Element<'a, Message> = button(
        row![cfg_icon, cfg_label]
            .spacing(spf(Spacing::Xxs))
            .align_y(iced::Alignment::Center),
    )
    .on_press(Message::Queues(QueuesMsg::ConfigureQueue(
        id,
        cfg_name,
        cfg_blocking,
    )))
    .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
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
        .spacing(spf(Spacing::Xs))
        .width(Length::Fill)
        .into()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use forge_runtime::{EventBus, NullEventLogRepo, ScriptRegistry};
    use forge_storage::CredentialsRepo;
    use forge_storage_sqlite::SqliteBackend;

    use crate::server_subsystem::ServerSubsystem;

    fn test_rt() -> RuntimeView {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let backend = Arc::new(
            tokio_rt
                .block_on(SqliteBackend::open_with_key("sqlite::memory:", [0xab; 32]))
                .unwrap(),
        );
        let server_subsystem = Arc::new(ServerSubsystem::new(
            Arc::clone(&backend) as Arc<dyn CredentialsRepo>
        ));
        let backend: Arc<dyn forge_storage::DataProvider> = backend;
        RuntimeView {
            actions: Arc::new(forge_runtime::actions::ActionsService::new(
                backend.action_repo(),
                backend.queue_repo(),
                backend.history_repo(),
                backend.trigger_instance_repo(),
                backend.soundboard_clips_repo(),
            )),
            backend,
            bus: EventBus::new(Arc::new(NullEventLogRepo)),
            script_registry: Arc::new(ScriptRegistry::new()),
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
            sound_player: None,
            twitch_chat_handle: None,
            chat_send_bridge: None,
            twitch_flow: None,
            youtube_flow: None,
            kick_flow: None,
            tts_engine_ids: Vec::new(),
            twitch_login: None,
            twitch_token_expires: None,
            twitch_reauth_required: false,
            sub_action_registry: Arc::new(forge_registry::SubActionRegistry::new()),
            trigger_registry: Arc::new(forge_registry::TriggerRegistry::new()),
        }
    }

    #[test]
    fn new_queue_opens_blank_form() {
        let rt = test_rt();
        let mut state = QueuesState::new();
        let _ = update(&mut state, &rt, QueuesMsg::NewQueue);
        let form = state.new_queue_form.expect("form should open");
        assert!(form.name.is_empty());
        assert!(!form.blocking);
        assert!(!form.saving);
    }

    #[test]
    fn name_changed_writes_typed_text_into_open_form() {
        let rt = test_rt();
        let mut state = QueuesState {
            new_queue_form: Some(NewQueueForm::default()),
            ..QueuesState::new()
        };
        let _ = update(
            &mut state,
            &rt,
            QueuesMsg::NewQueueNameChanged("Alerts".to_owned()),
        );
        assert_eq!(state.new_queue_form.unwrap().name, "Alerts");
    }

    #[test]
    fn name_changed_with_no_open_form_is_noop() {
        let rt = test_rt();
        let mut state = QueuesState::new();
        let _ = update(
            &mut state,
            &rt,
            QueuesMsg::NewQueueNameChanged("ignored".to_owned()),
        );
        assert!(state.new_queue_form.is_none());
    }

    #[test]
    fn blocking_toggle_flips_then_flips_back() {
        let rt = test_rt();
        let mut state = QueuesState {
            new_queue_form: Some(NewQueueForm::default()),
            ..QueuesState::new()
        };
        let _ = update(&mut state, &rt, QueuesMsg::NewQueueBlockingToggled);
        assert!(state.new_queue_form.as_ref().unwrap().blocking);
        let _ = update(&mut state, &rt, QueuesMsg::NewQueueBlockingToggled);
        assert!(!state.new_queue_form.unwrap().blocking);
    }

    #[test]
    fn cancel_discards_open_form() {
        let rt = test_rt();
        let mut state = QueuesState {
            new_queue_form: Some(NewQueueForm {
                name: "half typed".to_owned(),
                ..NewQueueForm::default()
            }),
            ..QueuesState::new()
        };
        let _ = update(&mut state, &rt, QueuesMsg::NewQueueCancel);
        assert!(state.new_queue_form.is_none());
    }

    #[test]
    fn submit_with_blank_name_keeps_form_open_and_does_not_save() {
        let rt = test_rt();
        for blank in ["", "   ", "\t\n"] {
            let mut state = QueuesState {
                new_queue_form: Some(NewQueueForm {
                    name: blank.to_owned(),
                    ..NewQueueForm::default()
                }),
                ..QueuesState::new()
            };
            let _ = update(&mut state, &rt, QueuesMsg::NewQueueSubmit);
            let form = state
                .new_queue_form
                .as_ref()
                .unwrap_or_else(|| panic!("form should stay open for {blank:?}"));
            assert!(!form.saving, "must not enter saving for {blank:?}");
        }
    }

    #[test]
    fn submit_with_nonblank_name_enters_saving_with_form_still_open() {
        let rt = test_rt();
        let mut state = QueuesState {
            new_queue_form: Some(NewQueueForm {
                name: "Background".to_owned(),
                ..NewQueueForm::default()
            }),
            ..QueuesState::new()
        };
        let _ = update(&mut state, &rt, QueuesMsg::NewQueueSubmit);
        let form = state.new_queue_form.expect("form stays open while saving");
        assert!(form.saving);
    }

    #[test]
    fn submit_result_ok_closes_form() {
        let rt = test_rt();
        let mut state = QueuesState {
            new_queue_form: Some(NewQueueForm {
                name: "Background".to_owned(),
                saving: true,
                ..NewQueueForm::default()
            }),
            ..QueuesState::new()
        };
        let _ = update(&mut state, &rt, QueuesMsg::NewQueueSubmitResult(Ok(())));
        assert!(state.new_queue_form.is_none());
    }

    #[test]
    fn submit_result_err_reopens_for_retry_by_clearing_saving() {
        let rt = test_rt();
        let mut state = QueuesState {
            new_queue_form: Some(NewQueueForm {
                name: "Background".to_owned(),
                saving: true,
                ..NewQueueForm::default()
            }),
            ..QueuesState::new()
        };
        let _ = update(
            &mut state,
            &rt,
            QueuesMsg::NewQueueSubmitResult(Err("db down".to_owned())),
        );
        let form = state.new_queue_form.expect("form stays open after error");
        assert!(!form.saving);
    }
}
