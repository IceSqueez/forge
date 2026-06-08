use std::time::{SystemTime, UNIX_EPOCH};

use forge_events::EventSource;
use forge_widgets::{
    ForgePalette, Radius, Spacing, bearer_token_display, color_for_source,
    icons::{Icon, tabler_icon},
    section_header, sp, spf, throughput_sparkline,
    tokens::{FONT_MD, FONT_SM, FONT_XS, FontRole, font, radius},
};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Task,
    widget::{Column, Row, Space, button, column, container, row, scrollable, text},
};

use crate::Message;
use crate::runtime_view::RuntimeView;

const MAX_BANDWIDTH_SAMPLES: usize = 60;
const MAX_VISIBLE_CHIPS: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ServerStatus {
    Running,
    #[default]
    Stopped,
    Error(String),
}

#[derive(Debug, Clone)]
pub struct OwnedSubscriptionChip {
    pub label: String,
    pub source: EventSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientLiveness {
    Active,
    Idle,
    Disconnecting,
}

#[derive(Debug, Clone)]
pub struct OwnedClientRow {
    pub identification: String,
    pub client_type_label: String,
    pub liveness: ClientLiveness,
    pub subscriptions: Vec<OwnedSubscriptionChip>,
    pub events_per_second: f32,
    pub uptime_short: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnedFileMime {
    Html,
    Css,
    Js,
    Json,
    Image,
    Wasm,
    Other,
}

impl OwnedFileMime {
    pub fn from_path(path: &std::path::Path) -> Self {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        match ext.as_str() {
            "html" | "htm" => Self::Html,
            "css" => Self::Css,
            "js" | "mjs" => Self::Js,
            "json" => Self::Json,
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" => Self::Image,
            "wasm" => Self::Wasm,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnedOverlayKind {
    File { mime: OwnedFileMime },
    Dir,
}

#[derive(Debug, Clone)]
pub struct OwnedOverlayEntry {
    pub name: String,
    pub kind: OwnedOverlayKind,
    pub size_bytes: u64,
    pub child_count: u32,
}

#[derive(Debug, Clone, Default)]
pub struct ServerStats {
    pub events_per_second: f32,
    pub events_per_second_avg: f32,
    pub http_requests: u64,
    pub bandwidth_kbps: f32,
    pub bandwidth_peak_kbps: f32,
    pub total_bytes_sent: u64,
    pub total_events_out: u64,
}

pub struct ServerScreenState {
    pub bind_address: String,
    pub bearer_token: String,
    pub token_revealed: bool,
    pub server_status: ServerStatus,
    pub uptime_seconds: i64,
    pub connected_clients: Vec<OwnedClientRow>,
    pub bandwidth_samples: Vec<f32>,
    pub stats: ServerStats,
    pub overlay_root: String,
    pub overlay_entries: Vec<OwnedOverlayEntry>,
    pub selected_overlay_file: Option<usize>,
}

impl Default for ServerScreenState {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0:8081".to_string(),
            bearer_token: "fg_placeholder00000000000000000000".to_string(),
            token_revealed: false,
            server_status: ServerStatus::default(),
            uptime_seconds: 0,
            connected_clients: Vec::new(),
            bandwidth_samples: Vec::new(),
            stats: ServerStats::default(),
            overlay_root: String::new(),
            overlay_entries: Vec::new(),
            selected_overlay_file: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServerInfoSnapshot {
    pub uptime_seconds: i64,
    pub connected_clients: Vec<OwnedClientRow>,
    pub stats: ServerStats,
}

#[derive(Debug, Clone)]
pub struct OverlayListingSnapshot {
    pub root: String,
    pub entries: Vec<OwnedOverlayEntry>,
}

#[derive(Debug, Clone)]
pub enum ServerScreenMsg {
    ToggleTokenReveal,
    CopyBindAddress,
    CopyToken,
    CopyOverlayUrl(String),
    RegenerateToken,
    RestartServer,
    StopServer,
    OpenOverlayFolder,
    SelectOverlayFile(usize),
    DisconnectClient(usize),
    ServerInfoArrived(ServerInfoSnapshot),
    OverlayListingArrived(OverlayListingSnapshot),
    BandwidthTick(f32),
}

pub fn update(
    state: &mut ServerScreenState,
    _rt: &RuntimeView,
    msg: ServerScreenMsg,
) -> Task<Message> {
    match msg {
        ServerScreenMsg::ToggleTokenReveal => {
            state.token_revealed = !state.token_revealed;
            Task::none()
        }
        ServerScreenMsg::CopyBindAddress => {
            iced::clipboard::write::<Message>(state.bind_address.clone())
        }
        ServerScreenMsg::CopyToken => iced::clipboard::write::<Message>(state.bearer_token.clone()),
        ServerScreenMsg::CopyOverlayUrl(url) => iced::clipboard::write::<Message>(url),
        ServerScreenMsg::RegenerateToken => {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            state.bearer_token = format!(
                "fg_{:016x}{:016x}",
                nanos,
                nanos.wrapping_mul(6_364_136_223_846_793_005_u128)
            );
            Task::none()
        }
        ServerScreenMsg::RestartServer | ServerScreenMsg::StopServer => Task::none(),
        ServerScreenMsg::OpenOverlayFolder => Task::none(),
        ServerScreenMsg::SelectOverlayFile(idx) => {
            state.selected_overlay_file = Some(idx);
            Task::none()
        }
        ServerScreenMsg::DisconnectClient(idx) => {
            if idx < state.connected_clients.len() {
                state.connected_clients.remove(idx);
            }
            Task::none()
        }
        ServerScreenMsg::ServerInfoArrived(snapshot) => {
            state.uptime_seconds = snapshot.uptime_seconds;
            state.connected_clients = snapshot.connected_clients;
            state.stats = snapshot.stats;
            Task::none()
        }
        ServerScreenMsg::BandwidthTick(kbps) => {
            state.bandwidth_samples.push(kbps);
            if state.bandwidth_samples.len() > MAX_BANDWIDTH_SAMPLES {
                let excess = state.bandwidth_samples.len() - MAX_BANDWIDTH_SAMPLES;
                state.bandwidth_samples.drain(..excess);
            }
            Task::none()
        }
        ServerScreenMsg::OverlayListingArrived(snapshot) => {
            state.overlay_root = snapshot.root;
            state.overlay_entries = snapshot.entries;
            if let Some(idx) = state.selected_overlay_file
                && idx >= state.overlay_entries.len()
            {
                state.selected_overlay_file = None;
            }
            Task::none()
        }
    }
}

fn status_color(status: &ServerStatus, palette: &ForgePalette) -> Color {
    match status {
        ServerStatus::Running => palette.success,
        ServerStatus::Stopped => palette.text_faint,
        ServerStatus::Error(_) => palette.random,
    }
}

fn status_label(status: &ServerStatus) -> &'static str {
    match status {
        ServerStatus::Running => "Running",
        ServerStatus::Stopped => "Stopped",
        ServerStatus::Error(_) => "Error",
    }
}

fn format_server_uptime(seconds: i64) -> String {
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3_600 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else {
        format!("{}h {}m", seconds / 3_600, (seconds % 3_600) / 60)
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1_024 {
        format!("{bytes} B")
    } else if bytes < 1_048_576 {
        format!("{:.1} KB", bytes as f64 / 1_024.0)
    } else if bytes < 1_073_741_824 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else {
        format!("{:.2} GB", bytes as f64 / 1_073_741_824.0)
    }
}

fn extract_port(bind_address: &str) -> &str {
    bind_address.split(':').next_back().unwrap_or("8081")
}

fn stat_card<'a>(
    label: impl Into<String>,
    value: impl Into<String>,
    sublabel: impl Into<String>,
    sublabel_color: Color,
    palette: &ForgePalette,
) -> Element<'a, Message> {
    let card_bg = palette.elevated;
    let border_color = palette.border_regular;
    let text_faint = palette.text_faint;
    let text_primary = palette.text_primary;
    let r = radius(Radius::Lg);

    let content = column![
        text(label.into())
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(text_faint),
        text(value.into())
            .font(font(FontRole::Monospace))
            .size(FONT_MD)
            .color(text_primary),
        text(sublabel.into())
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(sublabel_color),
    ]
    .spacing(spf(Spacing::Xxs));

    container(content)
        .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
        .width(Length::Fill)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(card_bg)),
            border: Border {
                color: border_color,
                width: 1.0,
                radius: r.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn chip_container<'a>(label: impl Into<String>, fg: Color, bg: Color) -> Element<'a, Message> {
    let r = radius(Radius::Md);
    container(
        text(label.into())
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(fg),
    )
    .padding([sp(Spacing::Xxs), sp(Spacing::Xxs)])
    .style(move |_: &iced::Theme| container::Style {
        background: Some(Background::Color(bg)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: r.into(),
        },
        ..container::Style::default()
    })
    .into()
}

fn chips_row<'a>(
    chips: &'a [OwnedSubscriptionChip],
    palette: &ForgePalette,
) -> Element<'a, Message> {
    let bg = palette.surface_overlay;
    let text_faint = palette.text_faint;

    if chips.is_empty() {
        return text("—")
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(text_faint)
            .into();
    }

    let visible = chips.len().min(MAX_VISIBLE_CHIPS);
    let overflow = chips.len().saturating_sub(MAX_VISIBLE_CHIPS);

    let mut elems: Vec<Element<'a, Message>> = chips[..visible]
        .iter()
        .map(|c| {
            let fg = color_for_source(c.source, palette);
            chip_container(c.label.clone(), fg, bg)
        })
        .collect();

    if overflow > 0 {
        elems.push(chip_container(format!("+{overflow} more"), text_faint, bg));
    }

    Row::with_children(elems)
        .spacing(spf(Spacing::Xxs))
        .align_y(Alignment::Center)
        .into()
}

fn client_row_elem<'a>(
    idx: usize,
    row_data: &'a OwnedClientRow,
    palette: &ForgePalette,
) -> Element<'a, Message> {
    let dot_color = match row_data.liveness {
        ClientLiveness::Active => palette.success,
        ClientLiveness::Idle => palette.warning,
        ClientLiveness::Disconnecting => palette.random,
    };
    let elevated = palette.elevated;
    let text_faint = palette.text_faint;
    let text_secondary = palette.text_secondary;
    let text_primary = palette.text_primary;
    let text_muted = palette.text_muted;

    let dot = container(Space::new().width(6.0f32).height(6.0f32)).style(move |_: &iced::Theme| {
        container::Style {
            background: Some(Background::Color(dot_color)),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 3.0.into(),
            },
            ..container::Style::default()
        }
    });

    let dot_cell = container(dot).width(Length::Fixed(24.0));

    let id_col = column![
        text(row_data.identification.clone())
            .font(font(FontRole::Monospace))
            .size(FONT_SM)
            .color(text_primary),
        text(row_data.client_type_label.clone())
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(text_faint),
    ]
    .spacing(spf(Spacing::Xxs));

    let id_cell = container(id_col).width(Length::FillPortion(14));
    let subs_cell =
        container(chips_row(&row_data.subscriptions, palette)).width(Length::FillPortion(16));

    let evs_cell = container(
        text(format!("{:.1}", row_data.events_per_second))
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(text_primary),
    )
    .width(Length::Fixed(80.0));

    let uptime_cell = container(
        text(row_data.uptime_short.clone())
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(text_muted),
    )
    .width(Length::Fixed(70.0));

    let x_btn = button(tabler_icon(Icon::X, 13.0, text_faint))
        .on_press(Message::Server(ServerScreenMsg::DisconnectClient(idx)))
        .padding([sp(Spacing::Xxs), sp(Spacing::Xxs)])
        .style(move |_theme: &iced::Theme, status| {
            use iced::widget::button::Status;
            iced::widget::button::Style {
                background: match status {
                    Status::Hovered => Some(Background::Color(Color {
                        a: 0.06,
                        ..text_secondary
                    })),
                    _ => None,
                },
                text_color: match status {
                    Status::Hovered => text_secondary,
                    _ => text_faint,
                },
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: 4.0.into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            }
        });

    let x_cell = container(x_btn).width(Length::Fixed(22.0));

    let content_row = row![dot_cell, id_cell, subs_cell, evs_cell, uptime_cell, x_cell]
        .align_y(Alignment::Center);

    let surface_overlay = palette.surface_overlay;
    let row_button = button(content_row)
        .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
        .width(Length::Fill)
        .on_press(Message::Noop)
        .style(move |_theme: &iced::Theme, status| {
            use iced::widget::button::Status;
            iced::widget::button::Style {
                background: match status {
                    Status::Hovered => Some(Background::Color(surface_overlay)),
                    _ => None,
                },
                text_color: text_primary,
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: 0.0.into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            }
        });

    let separator =
        container(Space::new().width(Length::Fill).height(1.0f32)).style(move |_: &iced::Theme| {
            container::Style {
                background: Some(Background::Color(elevated)),
                ..container::Style::default()
            }
        });

    container(column![row_button, separator])
        .width(Length::Fill)
        .into()
}

fn overlay_kind_tag(kind: &OwnedOverlayKind) -> &'static str {
    match kind {
        OwnedOverlayKind::Dir => "dir",
        OwnedOverlayKind::File { mime } => match mime {
            OwnedFileMime::Html => "html",
            OwnedFileMime::Css => "css",
            OwnedFileMime::Js => "js",
            OwnedFileMime::Json => "json",
            OwnedFileMime::Image => "img",
            OwnedFileMime::Wasm => "wasm",
            OwnedFileMime::Other => "file",
        },
    }
}

fn overlay_entry_row<'a>(
    idx: usize,
    entry: &'a OwnedOverlayEntry,
    selected: bool,
    port: &str,
    palette: &ForgePalette,
) -> Element<'a, Message> {
    let size_label = match &entry.kind {
        OwnedOverlayKind::Dir => format!("{} items", entry.child_count),
        OwnedOverlayKind::File { .. } => format_bytes(entry.size_bytes),
    };

    let kind_color = if matches!(
        entry.kind,
        OwnedOverlayKind::File {
            mime: OwnedFileMime::Html
        }
    ) {
        palette.brand
    } else if matches!(entry.kind, OwnedOverlayKind::Dir) {
        palette.warning
    } else {
        palette.text_muted
    };

    let bg = if selected {
        palette.shell
    } else {
        palette.base
    };
    let text_primary = palette.text_primary;
    let text_faint = palette.text_faint;
    let text_secondary = palette.text_secondary;
    let text_muted = palette.text_muted;
    let r = radius(Radius::Sm);
    let kind_bg = Color {
        a: 0.12,
        ..kind_color
    };

    let kind_badge = container(
        text(overlay_kind_tag(&entry.kind))
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(kind_color),
    )
    .padding([sp(Spacing::Xxs), sp(Spacing::Xxs)])
    .style(move |_: &iced::Theme| container::Style {
        background: Some(Background::Color(kind_bg)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 4.0.into(),
        },
        ..container::Style::default()
    });

    let name_row = row![
        kind_badge,
        text(entry.name.clone())
            .font(font(FontRole::Monospace))
            .size(FONT_SM)
            .color(text_primary),
        Space::new().width(Length::Fill),
        text(size_label)
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(text_faint),
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(Alignment::Center);

    let url = format!("http://127.0.0.1:{}/{}", port, entry.name);
    let url_for_copy = url.clone();

    let url_row: Option<Element<'a, Message>> = if selected {
        let copy_btn = button(
            row![
                tabler_icon(Icon::Copy, 11.0, palette.text_secondary),
                tabler_icon(Icon::ExternalLink, 11.0, palette.text_secondary),
            ]
            .spacing(spf(Spacing::Xxs))
            .align_y(Alignment::Center),
        )
        .on_press(Message::Server(ServerScreenMsg::CopyOverlayUrl(
            url_for_copy,
        )))
        .padding([sp(Spacing::Xxs), sp(Spacing::Xxs)])
        .style(
            move |_theme: &iced::Theme, _status| iced::widget::button::Style {
                background: None,
                text_color: text_secondary,
                border: Border::default(),
                shadow: iced::Shadow::default(),
                snap: false,
            },
        );

        Some(
            row![
                text(url)
                    .font(font(FontRole::Monospace))
                    .size(FONT_XS)
                    .color(text_muted),
                Space::new().width(Length::Fill),
                copy_btn,
            ]
            .align_y(Alignment::Center)
            .into(),
        )
    } else {
        None
    };

    let mut content_col = Column::new().push(name_row).spacing(spf(Spacing::Xxs));
    if let Some(url_elem) = url_row {
        content_col = content_col.push(url_elem);
    }

    button(container(content_col).padding([sp(Spacing::Xs), sp(Spacing::Xs)]))
        .on_press(Message::Server(ServerScreenMsg::SelectOverlayFile(idx)))
        .style(
            move |_theme: &iced::Theme, _status| iced::widget::button::Style {
                background: Some(Background::Color(bg)),
                text_color: text_primary,
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: r.into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            },
        )
        .width(Length::Fill)
        .into()
}

fn header_card<'a>(
    state: &'a ServerScreenState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let card_bg = palette.elevated;
    let border_color = palette.border_regular;
    let r = radius(Radius::Lg);
    let brand = palette.brand;
    let brand_bg = Color { a: 0.1, ..brand };
    let brand_border = Color { a: 0.2, ..brand };
    let info = palette.info;
    let info_bg = Color { a: 0.12, ..info };

    let icon_box = container(tabler_icon(Icon::Server, 20.0, brand))
        .width(Length::Fixed(48.0))
        .height(Length::Fixed(48.0))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(brand_bg)),
            border: Border {
                color: brand_border,
                width: 1.0,
                radius: radius(Radius::Lg).into(),
            },
            ..container::Style::default()
        });

    let ws_badge = container(
        text("WS + HTTP")
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(info),
    )
    .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
    .style(move |_: &iced::Theme| container::Style {
        background: Some(Background::Color(info_bg)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 4.0.into(),
        },
        ..container::Style::default()
    });

    let s_color = status_color(&state.server_status, palette);
    let s_label = status_label(&state.server_status);

    let status_dot =
        container(Space::new().width(7.0f32).height(7.0f32)).style(move |_: &iced::Theme| {
            container::Style {
                background: Some(Background::Color(s_color)),
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: 3.5.into(),
                },
                ..container::Style::default()
            }
        });

    let title_row = row![
        text("Built-in Server")
            .size(FONT_SM)
            .color(palette.text_primary),
        ws_badge,
        Space::new().width(Length::Fill),
        status_dot,
        text(s_label)
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(s_color),
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(Alignment::Center);

    let description = text("Internal HTTP + WebSocket server for overlays and remote control")
        .size(FONT_SM)
        .color(palette.text_muted);

    let actions_row = row![
        Space::new().width(Length::Fill),
        restart_btn(palette),
        stop_btn(palette),
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(Alignment::Center);

    let top_section = column![title_row, description, actions_row].spacing(spf(Spacing::Xs));

    let header_row = row![icon_box, top_section]
        .spacing(spf(Spacing::Md))
        .align_y(Alignment::Start);

    let separator =
        container(Space::new().width(Length::Fill).height(1.0f32)).style(move |_: &iced::Theme| {
            container::Style {
                background: Some(Background::Color(border_color)),
                ..container::Style::default()
            }
        });

    let uptime_str = if state.uptime_seconds > 0 {
        format!("Up {}", format_server_uptime(state.uptime_seconds))
    } else {
        "Not running".to_string()
    };

    let bind_col = column![
        text("BIND ADDRESS")
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(palette.text_faint),
        text(state.bind_address.as_str())
            .font(font(FontRole::Monospace))
            .size(FONT_SM)
            .color(palette.text_primary),
        text(uptime_str)
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(palette.text_faint),
        copy_address_btn(palette),
    ]
    .spacing(spf(Spacing::Xs))
    .width(Length::FillPortion(1));

    let token_col = column![
        text("BEARER TOKEN")
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(palette.text_faint),
        bearer_token_display(
            state.bearer_token.as_str(),
            state.token_revealed,
            Message::Server(ServerScreenMsg::ToggleTokenReveal),
            Message::Server(ServerScreenMsg::CopyToken),
            Message::Server(ServerScreenMsg::RegenerateToken),
            palette,
        ),
    ]
    .spacing(spf(Spacing::Xs))
    .width(Length::FillPortion(2));

    let credentials_row = row![bind_col, token_col]
        .spacing(spf(Spacing::Lg))
        .align_y(Alignment::Start);

    let card_content = column![header_row, separator, credentials_row].spacing(spf(Spacing::Md));

    container(card_content)
        .padding(sp(Spacing::Lg))
        .width(Length::Fill)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(card_bg)),
            border: Border {
                color: border_color,
                width: 1.0,
                radius: r.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn restart_btn<'a>(palette: &ForgePalette) -> Element<'a, Message> {
    let border = palette.success;
    let hover_bg = Color { a: 0.08, ..border };

    button(
        row![
            tabler_icon(Icon::Refresh, 12.0, palette.success),
            text("Restart")
                .font(font(FontRole::Monospace))
                .size(FONT_XS),
        ]
        .spacing(spf(Spacing::Xxs))
        .align_y(Alignment::Center),
    )
    .on_press(Message::Server(ServerScreenMsg::RestartServer))
    .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
    .style(move |_theme: &iced::Theme, status| {
        use iced::widget::button::Status;
        iced::widget::button::Style {
            background: match status {
                Status::Hovered | Status::Pressed => Some(Background::Color(hover_bg)),
                _ => None,
            },
            text_color: border,
            border: Border {
                color: border,
                width: 0.5,
                radius: radius(Radius::Md).into(),
            },
            shadow: iced::Shadow::default(),
            snap: false,
        }
    })
    .into()
}

fn stop_btn<'a>(palette: &ForgePalette) -> Element<'a, Message> {
    let border = palette.random;
    let hover_bg = Color { a: 0.08, ..border };

    button(text("Stop").font(font(FontRole::Monospace)).size(FONT_XS))
        .on_press(Message::Server(ServerScreenMsg::StopServer))
        .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
        .style(move |_theme: &iced::Theme, status| {
            use iced::widget::button::Status;
            iced::widget::button::Style {
                background: match status {
                    Status::Hovered | Status::Pressed => Some(Background::Color(hover_bg)),
                    _ => None,
                },
                text_color: border,
                border: Border {
                    color: border,
                    width: 0.5,
                    radius: radius(Radius::Md).into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            }
        })
        .into()
}

fn copy_address_btn<'a>(palette: &ForgePalette) -> Element<'a, Message> {
    let border = palette.border_regular;
    let normal = palette.text_secondary;
    let hover = palette.text_primary;
    let hover_bg = Color { a: 0.06, ..hover };

    button(
        row![
            tabler_icon(Icon::Copy, 12.0, normal),
            text("COPY").font(font(FontRole::Monospace)).size(FONT_XS),
        ]
        .spacing(spf(Spacing::Xxs))
        .align_y(Alignment::Center),
    )
    .on_press(Message::Server(ServerScreenMsg::CopyBindAddress))
    .padding([sp(Spacing::Xs), sp(Spacing::Xs)])
    .style(move |_theme: &iced::Theme, status| {
        use iced::widget::button::Status;
        iced::widget::button::Style {
            background: match status {
                Status::Hovered => Some(Background::Color(hover_bg)),
                _ => None,
            },
            text_color: match status {
                Status::Hovered => hover,
                _ => normal,
            },
            border: Border {
                color: border,
                width: 0.5,
                radius: radius(Radius::Md).into(),
            },
            shadow: iced::Shadow::default(),
            snap: false,
        }
    })
    .into()
}

fn stats_grid<'a>(state: &'a ServerScreenState, palette: &ForgePalette) -> Element<'a, Message> {
    let clients_n = state.connected_clients.len();
    let success = palette.success;
    let text_faint = palette.text_faint;

    row![
        stat_card(
            "CLIENTS",
            format!("{clients_n}"),
            "connected",
            success,
            palette
        ),
        stat_card(
            "EVENTS OUT",
            format!("{:.1} ev/s", state.stats.events_per_second),
            format!("avg {:.1} ev/s", state.stats.events_per_second_avg),
            text_faint,
            palette
        ),
        stat_card(
            "HTTP REQUESTS",
            format!("{}", state.stats.http_requests),
            "overlays served",
            text_faint,
            palette
        ),
        stat_card(
            "BANDWIDTH",
            format!("{:.0} KB/s", state.stats.bandwidth_kbps),
            format!("peak {:.0} KB/s", state.stats.bandwidth_peak_kbps),
            success,
            palette
        ),
    ]
    .spacing(spf(Spacing::Xs))
    .into()
}

fn clients_panel<'a>(
    state: &'a ServerScreenState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let card_bg = palette.elevated;
    let border_color = palette.border_regular;
    let r = radius(Radius::Lg);
    let surface_overlay = palette.surface_overlay;
    let text_faint = palette.text_faint;
    let text_primary = palette.text_primary;

    let count_badge = container(
        text(format!("{}", state.connected_clients.len()))
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(text_primary),
    )
    .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
    .style(move |_: &iced::Theme| container::Style {
        background: Some(Background::Color(surface_overlay)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 8.0.into(),
        },
        ..container::Style::default()
    });

    let kick_hint = text("press K on a row to disconnect")
        .font(font(FontRole::Monospace))
        .size(FONT_XS)
        .color(text_faint);

    let header = row![
        tabler_icon(Icon::Users, 14.0, text_faint),
        text("Connected Clients")
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(text_faint),
        Space::new().width(Length::Fill),
        kick_hint,
        count_badge,
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(Alignment::Center)
    .padding([sp(Spacing::Xs), sp(Spacing::Sm)]);

    let col_header = row![
        Space::new().width(Length::Fixed(24.0)),
        text("CLIENT")
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(text_faint)
            .width(Length::FillPortion(14)),
        text("SUBSCRIPTIONS")
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(text_faint)
            .width(Length::FillPortion(16)),
        text("EV/S")
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(text_faint)
            .width(Length::Fixed(80.0)),
        text("UPTIME")
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(text_faint)
            .width(Length::Fixed(70.0)),
        Space::new().width(Length::Fixed(22.0)),
    ]
    .padding([sp(Spacing::Xxs), sp(Spacing::Sm)]);

    let sep =
        container(Space::new().width(Length::Fill).height(1.0f32)).style(move |_: &iced::Theme| {
            container::Style {
                background: Some(Background::Color(border_color)),
                ..container::Style::default()
            }
        });

    let rows_col: Element<'a, Message> = if state.connected_clients.is_empty() {
        container(
            text("No clients connected")
                .font(font(FontRole::Monospace))
                .size(FONT_SM)
                .color(text_faint),
        )
        .padding([sp(Spacing::Lg), sp(Spacing::Sm)])
        .width(Length::Fill)
        .into()
    } else {
        let rows: Vec<Element<'a, Message>> = state
            .connected_clients
            .iter()
            .enumerate()
            .map(|(i, row)| client_row_elem(i, row, palette))
            .collect();
        scrollable(Column::with_children(rows).width(Length::Fill))
            .height(Length::Shrink)
            .into()
    };

    let inner = column![header, sep, col_header, rows_col];

    container(inner)
        .width(Length::Fill)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(card_bg)),
            border: Border {
                color: border_color,
                width: 1.0,
                radius: r.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn overlay_panel<'a>(
    state: &'a ServerScreenState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let card_bg = palette.elevated;
    let border_color = palette.border_regular;
    let r = radius(Radius::Lg);
    let text_secondary = palette.text_secondary;
    let open_btn_border = palette.border_regular;
    let text_muted = palette.text_muted;
    let text_faint = palette.text_faint;
    let port = extract_port(&state.bind_address);

    let open_btn = button(
        row![
            tabler_icon(Icon::FolderOpen, 12.0, text_muted),
            text("OPEN").font(font(FontRole::Monospace)).size(FONT_XS),
        ]
        .spacing(spf(Spacing::Xxs))
        .align_y(Alignment::Center),
    )
    .on_press(Message::Server(ServerScreenMsg::OpenOverlayFolder))
    .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
    .style(
        move |_theme: &iced::Theme, _status| iced::widget::button::Style {
            background: None,
            text_color: text_secondary,
            border: Border {
                color: open_btn_border,
                width: 0.5,
                radius: radius(Radius::Sm).into(),
            },
            shadow: iced::Shadow::default(),
            snap: false,
        },
    );

    let root_label = if state.overlay_root.is_empty() {
        "—".to_string()
    } else {
        state.overlay_root.clone()
    };

    let path_row = row![
        text(root_label)
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(text_muted),
        Space::new().width(Length::Fill),
        open_btn,
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(Alignment::Center)
    .padding([sp(Spacing::Xs), sp(Spacing::Sm)]);

    let sep =
        container(Space::new().width(Length::Fill).height(1.0f32)).style(move |_: &iced::Theme| {
            container::Style {
                background: Some(Background::Color(border_color)),
                ..container::Style::default()
            }
        });

    let files_section: Element<'a, Message> = if state.overlay_entries.is_empty() {
        container(
            text("No overlay files found")
                .font(font(FontRole::Monospace))
                .size(FONT_SM)
                .color(text_faint),
        )
        .padding([sp(Spacing::Lg), sp(Spacing::Sm)])
        .width(Length::Fill)
        .into()
    } else {
        let rows: Vec<Element<'a, Message>> = state
            .overlay_entries
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let selected = state.selected_overlay_file == Some(i);
                overlay_entry_row(i, entry, selected, port, palette)
            })
            .collect();
        scrollable(
            Column::with_children(rows)
                .width(Length::Fill)
                .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
                .spacing(spf(Spacing::Xxs)),
        )
        .height(Length::Shrink)
        .into()
    };

    let header_label = section_header::<Message>("Overlay Files", None, palette);

    let inner = column![header_label, path_row, sep, files_section];

    container(inner)
        .width(Length::Fill)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(card_bg)),
            border: Border {
                color: border_color,
                width: 1.0,
                radius: r.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn footer_bar<'a>(state: &'a ServerScreenState, palette: &ForgePalette) -> Element<'a, Message> {
    let shell = palette.shell;
    let border_color = palette.border_regular;
    let text_faint = palette.text_faint;
    let port = extract_port(&state.bind_address);
    let s_color = status_color(&state.server_status, palette);
    let s_label = status_label(&state.server_status);

    let ws_http_info = format!("WebSocket :{port}/ws · HTTP :{port}/");
    let total_sent = format_bytes(state.stats.total_bytes_sent);
    let total_events = state.stats.total_events_out;
    let stats_info = format!("Total sent: {total_sent} · Total events: {total_events}");

    let status_dot =
        container(Space::new().width(7.0f32).height(7.0f32)).style(move |_: &iced::Theme| {
            container::Style {
                background: Some(Background::Color(s_color)),
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: 3.5.into(),
                },
                ..container::Style::default()
            }
        });

    let left = text(ws_http_info)
        .font(font(FontRole::Monospace))
        .size(FONT_XS)
        .color(text_faint);

    let right = row![
        text(stats_info)
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(text_faint),
        text("·")
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(text_faint),
        status_dot,
        text(s_label)
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(s_color),
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(Alignment::Center);

    let content = row![left, Space::new().width(Length::Fill), right]
        .align_y(Alignment::Center)
        .padding([sp(Spacing::Xs), sp(Spacing::Md)]);

    let top_border =
        container(Space::new().width(Length::Fill).height(1.0f32)).style(move |_: &iced::Theme| {
            container::Style {
                background: Some(Background::Color(border_color)),
                ..container::Style::default()
            }
        });

    container(column![top_border, content])
        .width(Length::Fill)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(shell)),
            ..container::Style::default()
        })
        .into()
}

pub fn server_screen_view<'a>(
    state: &'a ServerScreenState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let body = column![
        header_card(state, palette),
        stats_grid(state, palette),
        throughput_sparkline(&state.bandwidth_samples, "KB/s", palette),
        row![overlay_panel(state, palette), clients_panel(state, palette),]
            .spacing(spf(Spacing::Sm)),
    ]
    .spacing(spf(Spacing::Sm))
    .padding([sp(Spacing::Md), sp(Spacing::Lg)]);

    let page_header =
        crate::page_chrome::simple_page_header(&[("Builtin", false), ("Server", true)], palette);

    column![
        page_header,
        scrollable(body).height(Length::Fill),
        footer_bar(state, palette),
    ]
    .into()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::Message;
    use crate::app::{App, update as app_update};

    #[test]
    fn toggle_token_reveal_flips_bool() {
        let mut app = App::default();
        assert!(!app.ui.server_screen.token_revealed);
        let _ = app_update(
            &mut app,
            Message::Server(ServerScreenMsg::ToggleTokenReveal),
        );
        assert!(app.ui.server_screen.token_revealed);
        let _ = app_update(
            &mut app,
            Message::Server(ServerScreenMsg::ToggleTokenReveal),
        );
        assert!(!app.ui.server_screen.token_revealed);
    }

    #[test]
    fn bandwidth_tick_pushes_and_trims_to_sixty() {
        let mut app = App::default();
        for i in 0..70 {
            let _ = app_update(
                &mut app,
                Message::Server(ServerScreenMsg::BandwidthTick(i as f32)),
            );
        }
        assert_eq!(
            app.ui.server_screen.bandwidth_samples.len(),
            MAX_BANDWIDTH_SAMPLES
        );
        assert_eq!(app.ui.server_screen.bandwidth_samples[0], 10.0);
        assert_eq!(app.ui.server_screen.bandwidth_samples[59], 69.0);
    }
}
