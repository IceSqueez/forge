use std::sync::Arc;

use forge_events::Event;
use forge_storage::GlobalsRepo;
use forge_widgets::icons::{Icon, tabler_icon};
use forge_widgets::tokens::{FONT_MD, FONT_SM, FONT_XS, Spacing, sp, spf};
use forge_widgets::{FontRole, ForgePalette, Radius, ToastKind, font, radius};
use iced::{Element, Length, Task, Theme};

use crate::app::App;
use crate::connectivity::{Connectivity, Integration};
use crate::message::{HomeMsg, Message, ToastMsg};
use crate::page_chrome::simple_page_header;
use crate::runtime_view::RuntimeView;
use crate::screen::Screen;

/// Rolling history cap for `ev_per_second_samples`, mirroring
/// `server_screen::MAX_BANDWIDTH_SAMPLES` (same 60-sample window as the sparkline's
/// `RING_LEN`).
const MAX_EV_PER_SECOND_SAMPLES: usize = 60;

/// Sentinel returned by `import_action` when the user dismisses the file picker.
/// Treated as a no-op rather than an error toast (cancel-is-not-a-failure).
const IMPORT_CANCELLED: &str = "import cancelled";

#[derive(Default)]
pub struct HomeStats {
    pub actions_count: Option<usize>,
    pub triggers_fired: Option<u64>,
    pub globals_count: Option<usize>,
    /// Rolling events/second history sampled from `EventBus::stats().total_published`
    /// by a periodic Subscription tick (see `subscriptions.rs`), fed into the Home
    /// throughput sparkline. Newest sample is last.
    pub ev_per_second_samples: Vec<f32>,
    /// Last dashboard-stats load failure, surfaced as an in-place `inline_error`
    /// banner with a retry affordance. `None` while a load is pending or succeeded.
    pub stats_error: Option<String>,
}

impl HomeStats {
    pub fn new() -> Self {
        Self::default()
    }
}

pub fn on_event(state: &mut HomeStats, event: &Event) -> Task<Message> {
    if event.kind == "action.done" {
        state.triggers_fired = Some(state.triggers_fired.unwrap_or(0) + 1);
    }
    Task::none()
}

pub fn update(state: &mut HomeStats, rt: &RuntimeView, msg: HomeMsg) -> Task<Message> {
    match msg {
        HomeMsg::LoadStats => {
            state.stats_error = None;
            let actions = rt.backend.action_repo();
            let globals: Arc<dyn GlobalsRepo> = Arc::clone(&rt.backend) as Arc<dyn GlobalsRepo>;
            let history = rt.backend.history_repo();
            Task::perform(
                async move {
                    forge_runtime::dashboard::compute_stats(&*actions, &*globals, &*history)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| Message::Home(HomeMsg::StatsLoaded(r)),
            )
        }
        HomeMsg::StatsLoaded(Ok(data)) => {
            state.actions_count = Some(data.actions_count);
            state.triggers_fired = Some(data.triggers_fired);
            state.globals_count = Some(data.globals_count);
            state.stats_error = None;
            Task::none()
        }
        HomeMsg::StatsLoaded(Err(e)) => {
            tracing::warn!(error = %e, "home stats load failed");
            state.stats_error = Some(e);
            Task::none()
        }
        HomeMsg::ImportRequested => {
            let dp = Arc::clone(&rt.backend);
            Task::perform(import_action(dp), |r| {
                Message::Home(HomeMsg::ImportCompleted(r))
            })
        }
        HomeMsg::ImportCompleted(Ok(name)) => {
            let toast = Task::done(Message::Toast(ToastMsg::Fired {
                kind: ToastKind::Success,
                message: forge_widgets::tr!("home_import_success", name = name.as_str()),
                duration_ms: 4000,
                action: None,
            }));
            let reload = Task::done(Message::Home(HomeMsg::LoadStats));
            Task::batch([toast, reload])
        }
        HomeMsg::ImportCompleted(Err(e)) => {
            // Dismissing the file picker is not a failure, same
            // "cancel is not an error" convention as the Event Feed export flow.
            if e == IMPORT_CANCELLED {
                return Task::none();
            }
            tracing::warn!(error = %e, "action import failed");
            Task::done(Message::Toast(ToastMsg::Fired {
                kind: ToastKind::Error,
                message: forge_widgets::tr!("home_import_failed", error = e.as_str()),
                duration_ms: 5000,
                action: None,
            }))
        }
        HomeMsg::EvPerSecondTick(eps) => {
            state.ev_per_second_samples.push(eps);
            if state.ev_per_second_samples.len() > MAX_EV_PER_SECOND_SAMPLES {
                let excess = state.ev_per_second_samples.len() - MAX_EV_PER_SECOND_SAMPLES;
                state.ev_per_second_samples.drain(..excess);
            }
            Task::none()
        }
    }
}

/// Pick a JSON file, deserialize a single `Action`, and persist it via
/// `ActionRepo`. Returns the imported action's name on success. Runs entirely
/// off the UI thread (dialog + file read + save are all `.await`ed here, never
/// inline in `update`). A fresh `ActionId` is minted so an import never clobbers
/// an existing action; the imported `queue_id` is remapped onto an existing
/// queue when the source queue is absent (the `actions.queue_id` FK would
/// otherwise reject the insert).
async fn import_action(dp: Arc<dyn forge_storage::DataProvider>) -> Result<String, String> {
    let Some(handle) = rfd::AsyncFileDialog::new()
        .add_filter("JSON", &["json"])
        .pick_file()
        .await
    else {
        return Err(IMPORT_CANCELLED.to_string());
    };
    let path = handle.path().to_path_buf();
    let bytes = tokio::fs::read(&path).await.map_err(|e| e.to_string())?;
    let mut action: forge_types::Action =
        serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;

    action.id = forge_types::ActionId::new();

    let queue_repo = dp.queue_repo();
    let queue_present = queue_repo
        .get(action.queue_id)
        .await
        .map_err(|e| e.to_string())?
        .is_some();
    if !queue_present {
        let fallback = queue_repo
            .list()
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .next()
            .ok_or_else(|| "no queue available to import into".to_string())?;
        action.queue_id = fallback.id;
    }

    dp.action_repo()
        .save(&action)
        .await
        .map_err(|e| e.to_string())?;
    Ok(action.name)
}

fn home_inline_button<'a>(
    icon: Icon,
    label: String,
    on_press: Message,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{button, row, text};
    use iced::{Alignment, Background, Border, Shadow};

    let icon_color = palette.text_secondary;
    let text_color = palette.text_secondary;
    let border_color = palette.border_regular;
    let r = radius(Radius::Md);

    let content = row![
        tabler_icon(icon, 12.0, icon_color),
        text(label).size(FONT_SM).color(text_color),
    ]
    .spacing(spf(Spacing::Xxs))
    .align_y(Alignment::Center);

    button(content)
        .on_press(on_press)
        .padding([6.0, 12.0])
        .style(move |_theme: &Theme, status| {
            let bg = if matches!(status, iced::widget::button::Status::Hovered) {
                Some(Background::Color(iced::Color {
                    a: 0.06,
                    ..border_color
                }))
            } else {
                Some(Background::Color(iced::Color::TRANSPARENT))
            };
            button::Style {
                background: bg,
                text_color,
                border: Border {
                    color: border_color,
                    width: 0.5,
                    radius: r.into(),
                },
                shadow: Shadow::default(),
                snap: false,
            }
        })
        .into()
}

fn home_hero<'a>(palette: &'a ForgePalette) -> Element<'a, Message> {
    use iced::widget::{column, container, row, text};
    use iced::{Alignment, Background, Border};

    let brand = palette.brand;
    let shell = palette.shell;

    let brand_box = container(text("F").size(26.0).color(shell).font(iced::Font {
        weight: iced::font::Weight::Semibold,
        ..iced::Font::DEFAULT
    }))
    .width(54.0)
    .height(54.0)
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(move |_theme: &Theme| iced::widget::container::Style {
        background: Some(Background::Color(brand)),
        border: Border {
            radius: 12.0.into(),
            color: iced::Color::TRANSPARENT,
            width: 0.0,
        },
        ..iced::widget::container::Style::default()
    });

    let title_col = column![
        text("Forge").size(22.0).color(palette.text_primary),
        text(forge_widgets::tr!("home_hero_tagline"))
            .size(FONT_SM)
            .color(palette.text_muted),
    ]
    .spacing(spf(Spacing::Xxs));

    let import_btn = home_inline_button(
        Icon::Download,
        forge_widgets::tr!("home_hero_import"),
        Message::Home(HomeMsg::ImportRequested),
        palette,
    );
    let new_action_btn = home_inline_button(
        Icon::Plus,
        forge_widgets::tr!("home_hero_new_action"),
        Message::Navigate(Screen::ActionEditor(None)),
        palette,
    );

    let buttons_row = row![import_btn, new_action_btn].spacing(spf(Spacing::Xs));

    let inner = row![
        brand_box,
        container(title_col).width(Length::Fill),
        buttons_row,
    ]
    .spacing(spf(Spacing::Md))
    .align_y(Alignment::Center);

    let elevated = palette.elevated;
    let border_regular = palette.border_regular;

    container(inner)
        .width(Length::Fill)
        .padding(iced::Padding {
            top: 22.0,
            right: 24.0,
            bottom: 22.0,
            left: 24.0,
        })
        .style(move |_theme: &Theme| iced::widget::container::Style {
            background: Some(Background::Color(elevated)),
            border: Border {
                color: border_regular,
                width: 0.5,
                radius: radius(Radius::Lg).into(),
            },
            ..iced::widget::container::Style::default()
        })
        .into()
}

fn home_jump_cards<'a>(app: &'a App, palette: &'a ForgePalette) -> Element<'a, Message> {
    use forge_widgets::{BigJumpCardProps, big_jump_card};
    use iced::widget::{container, row};

    let actions_count = app.ui.home.actions_count.unwrap_or(0);
    let triggers_fired = app.ui.home.triggers_fired.unwrap_or(0);
    let chat_count = app.ui.live_chat.rows.len();
    let connectivity = Connectivity::resolve(&app.rt);
    let total_integrations = connectivity.total();
    let connected_integrations = connectivity.connected_count();
    let connections_warn = connected_integrations < total_integrations;

    let card_chat = big_jump_card(
        BigJumpCardProps {
            icon: Icon::MessageCircle,
            icon_color: palette.brand,
            section_label: forge_widgets::tr!("home_card_audience_section"),
            title: forge_widgets::tr!("home_card_audience_title"),
            stat: chat_count.to_string(),
            stat_label: forge_widgets::tr!("home_card_audience_stat_label"),
            hint: forge_widgets::tr!("home_card_audience_hint"),
            on_press: Message::Navigate(Screen::LiveChat),
            warn: false,
        },
        palette,
    );

    let card_actions = big_jump_card(
        BigJumpCardProps {
            icon: Icon::Bolt,
            icon_color: palette.warning,
            section_label: forge_widgets::tr!("home_card_automation_section"),
            title: forge_widgets::tr!("home_card_automation_title"),
            stat: actions_count.to_string(),
            stat_label: forge_widgets::tr!(
                "home_card_automation_stat_label",
                count = actions_count as i64,
                fired = triggers_fired as i64
            ),
            hint: forge_widgets::tr!("home_card_automation_hint"),
            on_press: Message::Navigate(Screen::ActionEditor(None)),
            warn: false,
        },
        palette,
    );

    let card_connections = big_jump_card(
        BigJumpCardProps {
            icon: Icon::Plug,
            icon_color: palette.success,
            section_label: forge_widgets::tr!("home_card_connections_section"),
            title: forge_widgets::tr!("home_card_connections_title"),
            stat: format!("{connected_integrations}/{total_integrations}"),
            stat_label: forge_widgets::tr!("home_card_connections_stat_label"),
            hint: forge_widgets::tr!("home_card_connections_hint"),
            on_press: Message::Navigate(Screen::Platforms),
            warn: connections_warn,
        },
        palette,
    );

    row![
        container(card_chat).width(Length::FillPortion(1)),
        container(card_actions).width(Length::FillPortion(1)),
        container(card_connections).width(Length::FillPortion(1)),
    ]
    .spacing(spf(Spacing::Xs))
    .into()
}

fn home_stream_health<'a>(app: &'a App, palette: &'a ForgePalette) -> Element<'a, Message> {
    use iced::widget::{column, container, row, text};
    use iced::{Alignment, Background, Border};

    let elevated = palette.elevated;
    let border_regular = palette.border_regular;
    let success = palette.success;
    let text_faint = palette.text_faint;
    let text_primary = palette.text_primary;
    let text_muted = palette.text_muted;

    let header_icon = tabler_icon(Icon::ChartLine, 14.0, success);
    let live_dot = container(iced::widget::Space::new())
        .width(6.0)
        .height(6.0)
        .style(move |_theme: &Theme| iced::widget::container::Style {
            background: Some(Background::Color(success)),
            border: Border {
                radius: 3.0.into(),
                color: iced::Color::TRANSPARENT,
                width: 0.0,
            },
            ..iced::widget::container::Style::default()
        });

    let live_badge = row![
        live_dot,
        text(forge_widgets::tr!("home_health_live"))
            .size(FONT_XS)
            .color(success)
            .font(font(FontRole::Monospace)),
    ]
    .spacing(spf(Spacing::Xxs))
    .align_y(Alignment::Center);

    let header_left = row![
        header_icon,
        text(forge_widgets::tr!("home_health_title"))
            .size(FONT_SM)
            .color(text_primary),
        live_badge,
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(Alignment::Center);

    let header_right = text(forge_widgets::tr!("home_health_refresh_hint"))
        .size(FONT_XS)
        .color(text_faint)
        .font(font(FontRole::Monospace));

    let header = row![
        header_left,
        iced::widget::Space::new().width(Length::Fill),
        header_right,
    ]
    .align_y(Alignment::Center);

    let throughput_label = forge_widgets::tr!("home_health_throughput_label");
    let sparkline_col = column![
        text(throughput_label)
            .size(FONT_XS)
            .color(text_faint)
            .font(font(FontRole::Monospace)),
        forge_widgets::throughput_sparkline(&app.ui.home.ev_per_second_samples, "ev/s", palette),
    ]
    .spacing(spf(Spacing::Xxs))
    .width(Length::FillPortion(14));

    let health_stat =
        |label: String, value: String, unit: Option<&'static str>| -> Element<'a, Message> {
            let val_el: Element<'a, Message> = if let Some(u) = unit {
                row![
                    text(value)
                        .size(FONT_MD)
                        .color(text_primary)
                        .font(font(FontRole::Monospace)),
                    text(u).size(FONT_XS).color(text_muted),
                ]
                .spacing(spf(Spacing::Xxs))
                .align_y(Alignment::Center)
                .into()
            } else {
                text(value)
                    .size(FONT_MD)
                    .color(text_primary)
                    .font(font(FontRole::Monospace))
                    .into()
            };
            column![
                text(label)
                    .size(FONT_XS)
                    .color(text_faint)
                    .font(font(FontRole::Monospace)),
                val_el,
            ]
            .spacing(spf(Spacing::Xxs))
            .width(Length::FillPortion(10))
            .into()
        };

    let (fps_val, cpu_val, dropped_val) = if let Some(client) = &app.rt.obs_client {
        let snap = client.health_snapshot();
        let fps = format!("{:.1}", snap.fps);
        let cpu = format!("{:.1}", snap.cpu_percent);
        let dropped = if snap.total_frames > 0 {
            format!(
                "{} ({:.2}%)",
                snap.dropped_frames,
                (snap.dropped_frames as f64 / snap.total_frames as f64) * 100.0
            )
        } else {
            snap.dropped_frames.to_string()
        };
        (fps, cpu, dropped)
    } else {
        (
            "\u{2014}".to_owned(),
            "\u{2014}".to_owned(),
            "\u{2014}".to_owned(),
        )
    };

    let stats_row = row![
        sparkline_col,
        health_stat(
            forge_widgets::tr!("home_health_bitrate_label"),
            "\u{2014}".to_owned(),
            Some("kbps")
        ),
        health_stat(
            forge_widgets::tr!("home_health_dropped_label"),
            dropped_val,
            None
        ),
        health_stat(forge_widgets::tr!("home_health_fps_label"), fps_val, None),
        health_stat(
            forge_widgets::tr!("home_health_cpu_label"),
            cpu_val,
            Some("%")
        ),
    ]
    .spacing(spf(Spacing::Sm))
    .align_y(Alignment::End);

    let card_content = column![header, stats_row].spacing(spf(Spacing::Xs));

    container(card_content)
        .width(Length::Fill)
        .padding(sp(Spacing::Sm))
        .style(move |_theme: &Theme| iced::widget::container::Style {
            background: Some(Background::Color(elevated)),
            border: Border {
                color: border_regular,
                width: 0.5,
                radius: radius(Radius::Md).into(),
            },
            ..iced::widget::container::Style::default()
        })
        .into()
}

fn home_connection_cell<'a>(
    label: &'a str,
    dot_color: iced::Color,
    state: forge_platform_core::ConnectionState,
    on_press: Message,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{button, column, container, row, text};
    use iced::{Alignment, Background, Border, Shadow};

    let ok = matches!(state, forge_platform_core::ConnectionState::Connected);
    let text_primary = palette.text_primary;
    let elevated = palette.elevated;
    let shell = palette.shell;
    let border_regular = palette.border_regular;

    let platform_dot = container(iced::widget::Space::new())
        .width(10.0)
        .height(10.0)
        .style(move |_theme: &Theme| iced::widget::container::Style {
            background: Some(Background::Color(dot_color)),
            border: Border {
                radius: 3.0.into(),
                color: iced::Color::TRANSPARENT,
                width: 0.0,
            },
            ..iced::widget::container::Style::default()
        });

    let status_color = crate::connectivity::state_color(state, palette);
    let status_str = if ok {
        forge_widgets::tr!("home_conn_connected")
    } else {
        forge_widgets::tr!("home_conn_offline")
    };

    let label_col = column![
        text(label).size(FONT_XS).color(text_primary),
        text(status_str)
            .size(FONT_XS)
            .color(status_color)
            .font(font(FontRole::Monospace)),
    ]
    .spacing(spf(Spacing::Xxs))
    .width(Length::Fill);

    let status_dot = container(iced::widget::Space::new())
        .width(8.0)
        .height(8.0)
        .style(move |_theme: &Theme| iced::widget::container::Style {
            background: Some(Background::Color(status_color)),
            border: Border {
                radius: 4.0.into(),
                color: iced::Color::TRANSPARENT,
                width: 0.0,
            },
            ..iced::widget::container::Style::default()
        });

    let content = row![platform_dot, label_col, status_dot]
        .spacing(spf(Spacing::Xs))
        .align_y(Alignment::Center);

    button(content)
        .on_press(on_press)
        .padding(iced::Padding {
            top: 12.0,
            right: 14.0,
            bottom: 12.0,
            left: 14.0,
        })
        .width(Length::Fill)
        .style(move |_theme: &Theme, status| button::Style {
            background: Some(Background::Color(
                if matches!(status, iced::widget::button::Status::Hovered) {
                    shell
                } else {
                    elevated
                },
            )),
            border: Border {
                color: border_regular,
                width: 0.0,
                radius: 0.0.into(),
            },
            text_color: text_primary,
            shadow: Shadow::default(),
            snap: false,
        })
        .into()
}

fn home_connections_strip<'a>(app: &'a App, palette: &'a ForgePalette) -> Element<'a, Message> {
    use iced::widget::{column, container, row, text};
    use iced::{Alignment, Background, Border};

    let connectivity = Connectivity::resolve(&app.rt);
    let connected = connectivity.connected_count();
    let disconnected = connectivity.total().saturating_sub(connected);

    let connections_summary = forge_widgets::tr!(
        "home_connections_summary",
        active = connected as i64,
        disconnected = disconnected as i64
    );
    let header_icon = tabler_icon(Icon::PlugConnected, 14.0, palette.success);
    let header_title = text(forge_widgets::tr!("home_connections_title"))
        .size(FONT_SM)
        .color(palette.text_primary);
    let header_sub = text(connections_summary)
        .size(FONT_XS)
        .color(palette.text_faint);

    let header = row![
        header_icon,
        header_title,
        header_sub,
        iced::widget::Space::new().width(Length::Fill),
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(Alignment::Center);

    let elevated = palette.elevated;
    let border_regular = palette.border_regular;
    let surface_overlay = palette.surface_overlay;

    let header_bar = container(header)
        .width(Length::Fill)
        .padding(iced::Padding {
            top: 10.0,
            right: 14.0,
            bottom: 10.0,
            left: 14.0,
        })
        .style(move |_theme: &Theme| iced::widget::container::Style {
            background: Some(Background::Color(elevated)),
            border: Border {
                color: border_regular,
                width: 0.5,
                radius: iced::border::Radius {
                    top_left: radius(Radius::Md),
                    top_right: radius(Radius::Md),
                    bottom_left: 0.0,
                    bottom_right: 0.0,
                },
            },
            ..iced::widget::container::Style::default()
        });

    let mut cells = row![].spacing(spf(Spacing::Xxs));
    for status in connectivity.statuses() {
        let integration = status.integration;
        let short_label = match integration {
            Integration::Obs => "OBS",
            Integration::VTube => "VTube",
            other => other.label(),
        };
        cells = cells.push(
            container(home_connection_cell(
                short_label,
                integration.brand_color(palette),
                status.state,
                Message::Navigate(Screen::BuiltinDetail(integration.builtin_id())),
                palette,
            ))
            .width(Length::FillPortion(1)),
        );
    }

    let cells_container = container(cells)
        .width(Length::Fill)
        .style(move |_theme: &Theme| iced::widget::container::Style {
            background: Some(Background::Color(surface_overlay)),
            border: Border {
                color: border_regular,
                width: 0.5,
                radius: iced::border::Radius {
                    top_left: 0.0,
                    top_right: 0.0,
                    bottom_left: radius(Radius::Md),
                    bottom_right: radius(Radius::Md),
                },
            },
            ..iced::widget::container::Style::default()
        });

    column![header_bar, cells_container]
        .spacing(0.0)
        .width(Length::Fill)
        .into()
}

fn home_system_event_row<'a>(
    event: &'a forge_events::Event,
    has_bottom_border: bool,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use forge_widgets::{color_for_source, source_label};
    use iced::widget::{button, container, row as irow, text};
    use iced::{Alignment, Background, Border, Shadow};

    let dot_color = color_for_source(event.source, palette);
    let elevated = palette.elevated;
    let border_regular = palette.border_regular;
    let shell = palette.shell;
    let text_primary = palette.text_primary;

    let dot = container(iced::widget::Space::new())
        .width(6.0)
        .height(6.0)
        .style(move |_theme: &Theme| iced::widget::container::Style {
            background: Some(Background::Color(dot_color)),
            border: Border {
                radius: 3.0.into(),
                color: iced::Color::TRANSPARENT,
                width: 0.0,
            },
            ..iced::widget::container::Style::default()
        });

    let ts_str = format!(
        "{:02}:{:02}:{:02}",
        event.timestamp.hour(),
        event.timestamp.minute(),
        event.timestamp.second()
    );

    let ts_col = container(
        text(ts_str)
            .size(FONT_XS)
            .color(palette.text_muted)
            .font(font(FontRole::Monospace)),
    )
    .width(60.0);

    let source_str = source_label(event.source);
    let summary_str = crate::event_feed::format_summary(event);
    let full = format!("{}: {}", source_str, summary_str);

    let description: Element<'a, Message> = text(full)
        .size(FONT_XS)
        .color(text_primary)
        .width(Length::Fill)
        .into();

    let inner = irow![dot, ts_col, description]
        .spacing(spf(Spacing::Xs))
        .align_y(Alignment::Center);

    let border_width = if has_bottom_border { 0.5 } else { 0.0 };

    let styled_row = container(inner)
        .width(Length::Fill)
        .padding(iced::Padding {
            top: 7.0,
            right: 4.0,
            bottom: 7.0,
            left: 4.0,
        })
        .style(move |_theme: &Theme| iced::widget::container::Style {
            border: Border {
                color: border_regular,
                width: border_width,
                radius: 0.0.into(),
            },
            ..iced::widget::container::Style::default()
        });

    button(styled_row)
        .on_press(Message::Navigate(Screen::EventFeed))
        .style(move |_theme: &Theme, status| button::Style {
            background: Some(Background::Color(
                if matches!(status, iced::widget::button::Status::Hovered) {
                    shell
                } else {
                    elevated
                },
            )),
            border: Border {
                color: iced::Color::TRANSPARENT,
                width: 0.0,
                radius: 4.0.into(),
            },
            text_color: text_primary,
            shadow: Shadow::default(),
            snap: false,
        })
        .padding(0)
        .width(Length::Fill)
        .into()
}

fn home_recent_events_card<'a>(app: &'a App, palette: &'a ForgePalette) -> Element<'a, Message> {
    use iced::widget::{column, container, row, text};
    use iced::{Alignment, Background, Border};

    let elevated = palette.elevated;
    let border_regular = palette.border_regular;
    let success = palette.success;
    let text_faint = palette.text_faint;
    let text_primary = palette.text_primary;
    let text_muted = palette.text_muted;

    let live_dot = container(iced::widget::Space::new())
        .width(6.0)
        .height(6.0)
        .style(move |_theme: &Theme| iced::widget::container::Style {
            background: Some(Background::Color(success)),
            border: Border {
                radius: 3.0.into(),
                color: iced::Color::TRANSPARENT,
                width: 0.0,
            },
            ..iced::widget::container::Style::default()
        });

    let live_label = row![
        live_dot,
        text(forge_widgets::tr!("home_health_live"))
            .size(FONT_XS)
            .color(text_faint)
            .font(font(FontRole::Monospace)),
    ]
    .spacing(spf(Spacing::Xxs))
    .align_y(Alignment::Center);

    let header = row![
        text(forge_widgets::tr!("home_events_title"))
            .size(FONT_SM)
            .color(text_primary),
        iced::widget::Space::new().width(Length::Fill),
        live_label,
    ]
    .align_y(Alignment::Center);

    let recent: Vec<&forge_events::Event> = app.ui.event_feed.events.iter().rev().take(5).collect();

    let body: Element<'a, Message> = if recent.is_empty() {
        text(forge_widgets::tr!("home_events_empty"))
            .size(FONT_XS)
            .color(text_muted)
            .into()
    } else {
        let count = recent.len();
        let mut col = column![].spacing(0.0);
        for (i, row_data) in recent.into_iter().enumerate() {
            col = col.push(home_system_event_row(row_data, i + 1 < count, palette));
        }
        col.into()
    };

    container(column![header, body].spacing(spf(Spacing::Xs)))
        .width(Length::FillPortion(14))
        .padding(sp(Spacing::Sm))
        .style(move |_theme: &Theme| iced::widget::container::Style {
            background: Some(Background::Color(elevated)),
            border: Border {
                color: border_regular,
                width: 0.5,
                radius: radius(Radius::Md).into(),
            },
            ..iced::widget::container::Style::default()
        })
        .into()
}

fn home_glance_row<'a>(
    label: String,
    value: String,
    color: iced::Color,
    last: bool,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{container, row, text};
    use iced::{Alignment, Border};

    let border_regular = palette.border_regular;
    let text_muted = palette.text_muted;

    let inner = row![
        text(label)
            .size(FONT_XS)
            .color(text_muted)
            .width(Length::Fill),
        text(value)
            .size(FONT_SM)
            .color(color)
            .font(font(FontRole::Monospace)),
    ]
    .align_y(Alignment::Center)
    .padding(iced::Padding {
        top: 5.0,
        right: 0.0,
        bottom: 5.0,
        left: 0.0,
    });

    let border_width = if last { 0.0 } else { 0.5 };

    container(inner)
        .width(Length::Fill)
        .style(move |_theme: &Theme| iced::widget::container::Style {
            border: Border {
                color: border_regular,
                width: border_width,
                radius: 0.0.into(),
            },
            ..iced::widget::container::Style::default()
        })
        .into()
}

fn home_glance_card<'a>(app: &'a App, palette: &'a ForgePalette) -> Element<'a, Message> {
    use iced::widget::{column, container, text};
    use iced::{Background, Border};

    let elevated = palette.elevated;
    let border_regular = palette.border_regular;
    let text_primary = palette.text_primary;

    let actions_val = app
        .ui
        .home
        .actions_count
        .map_or_else(|| "\u{2014}".to_string(), |n| n.to_string());
    let fired_val = app
        .ui
        .home
        .triggers_fired
        .map_or_else(|| "\u{2014}".to_string(), |n| n.to_string());
    let globals_val = app
        .ui
        .home
        .globals_count
        .map_or_else(|| "\u{2014}".to_string(), |n| n.to_string());

    let header = text(forge_widgets::tr!("home_glance_title"))
        .size(FONT_SM)
        .color(text_primary);

    let content = column![
        header,
        home_glance_row(
            forge_widgets::tr!("home_glance_actions"),
            actions_val,
            palette.brand,
            false,
            palette
        ),
        home_glance_row(
            forge_widgets::tr!("home_glance_fired"),
            fired_val,
            palette.success,
            false,
            palette
        ),
        home_glance_row(
            forge_widgets::tr!("home_glance_globals"),
            globals_val,
            palette.warning,
            true,
            palette
        ),
    ]
    .spacing(0.0);

    container(content)
        .width(Length::FillPortion(10))
        .padding(sp(Spacing::Sm))
        .style(move |_theme: &Theme| iced::widget::container::Style {
            background: Some(Background::Color(elevated)),
            border: Border {
                color: border_regular,
                width: 0.5,
                radius: radius(Radius::Md).into(),
            },
            ..iced::widget::container::Style::default()
        })
        .into()
}

pub(crate) fn home_view<'a>(app: &'a App, palette: &'a ForgePalette) -> Element<'a, Message> {
    use iced::widget::{column, container};

    let page_header = simple_page_header(&[(forge_widgets::tr!("nav_home"), true)], palette);

    let hero = home_hero(palette);
    let jump_cards = home_jump_cards(app, palette);
    let connections = home_connections_strip(app, palette);
    let bottom = iced::widget::row![
        home_recent_events_card(app, palette),
        home_glance_card(app, palette),
    ]
    .spacing(spf(Spacing::Sm));

    let mut content = column![hero, jump_cards,]
        .spacing(spf(Spacing::Md))
        .width(Length::Fill);

    if let Some(err) = &app.ui.home.stats_error {
        content = content.push(forge_widgets::inline_error(
            forge_widgets::tr!("home_stats_error", error = err.as_str()),
            forge_widgets::tr!("home_stats_retry"),
            Message::Home(HomeMsg::LoadStats),
            palette,
        ));
    }

    if app.rt.obs_client.is_some() {
        content = content.push(home_stream_health(app, palette));
    }

    content = content.push(connections).push(bottom);

    let body = container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(iced::Padding {
            top: 22.0,
            right: 28.0,
            bottom: 22.0,
            left: 28.0,
        })
        .style(move |_theme: &Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(palette.base)),
            ..iced::widget::container::Style::default()
        });

    column![page_header, body]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
