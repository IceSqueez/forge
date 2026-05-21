use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_storage::SettingsRepo;
use forge_storage_sqlite::SqliteBackend;
use forge_widgets::{
    BindAddressCardParams, BindBadge, BulletItem, BulletKind, ForgePalette, Radius, ToggleProps,
    TypeToConfirmModalParams, bearer_token_display, bind_address_card,
    icons::{
        BOOTSTRAP_FONT, ICON_ALERT_TRIANGLE, ICON_CHECK_CIRCLE, ICON_FOLDER_OPEN, ICON_GLOBE,
        ICON_LOCK, ICON_SERVER,
    },
    toggle,
    tokens::{FONT_BODY, FONT_SM, FontRole, font, radius},
    type_to_confirm_modal,
};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Task,
    widget::{Space, button, column, container, row, rule, scrollable, stack, text, text_input},
};

use crate::Message;
use crate::server_screen::ServerScreenMsg;

static LAN_BIND_BULLETS: [BulletItem<'static>; 4] = [
    BulletItem {
        kind: BulletKind::Check,
        text: "Phone / tablet / second PC can connect to overlays and the WS API",
    },
    BulletItem {
        kind: BulletKind::Warning,
        text: "Anyone on your network can read all events and send chat messages if they know your bearer token",
    },
    BulletItem {
        kind: BulletKind::Warning,
        text: "If you're on public Wi-Fi (café, conference, hotel), do not enable this",
    },
    BulletItem {
        kind: BulletKind::Info,
        text: "Your firewall must also allow the configured port for this to work",
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindAddressChoice {
    Localhost,
    Lan,
}

pub struct SettingsWebSocketState {
    pub enable_server: bool,
    pub bind_address_radio: BindAddressChoice,
    pub port: u16,
    pub port_input: String,
    pub require_ws_token: bool,
    pub require_http_overlay_token: bool,
    pub cors_any_origin: bool,
    pub overlay_root: PathBuf,
    pub lan_bind_modal_visible: bool,
    pub lan_bind_input: String,
    pub all_changes_saved: bool,
    pub save_error: Option<String>,
}

impl Default for SettingsWebSocketState {
    fn default() -> Self {
        Self {
            enable_server: true,
            bind_address_radio: BindAddressChoice::Localhost,
            port: 8081,
            port_input: "8081".to_owned(),
            require_ws_token: true,
            require_http_overlay_token: false,
            cors_any_origin: true,
            overlay_root: PathBuf::new(),
            lan_bind_modal_visible: false,
            lan_bind_input: String::new(),
            all_changes_saved: true,
            save_error: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SettingsWebSocketSnapshot {
    pub bind_address: String,
    pub port: u16,
    pub require_ws_token: bool,
    pub lan_bind_enabled: bool,
    pub require_http_overlay_token: bool,
    pub cors_any_origin: bool,
    pub overlay_root: Option<String>,
}

#[derive(Debug, Clone)]
pub enum SettingsWebSocketMsg {
    LoadRequested,
    LoadResult(Result<SettingsWebSocketSnapshot, String>),
    ToggleEnable(bool),
    SelectLocalhost,
    SelectLan,
    PortChanged(String),
    PortFocusLost,
    RequireWsToken(bool),
    RequireHttpOverlayToken(bool),
    CorsAnyOrigin(bool),
    OverlayRootChanged(PathBuf),
    LanBindInputChanged(String),
    LanBindCancelled,
    LanBindConfirmed,
    SaveStatus(Result<(), String>),
}

pub async fn load_settings_websocket(
    backend: Arc<SqliteBackend>,
) -> Result<SettingsWebSocketSnapshot, String> {
    use forge_storage::SettingsRepo;

    let settings: &dyn SettingsRepo = backend.as_ref();
    let bind_address = settings
        .server_bind_address()
        .await
        .map_err(|e| e.to_string())?;
    let port = settings.server_port().await.map_err(|e| e.to_string())?;
    let require_ws_token = settings
        .server_auth_required_for_reads()
        .await
        .map_err(|e| e.to_string())?;
    let lan_bind_enabled = settings
        .server_lan_bind_enabled()
        .await
        .map_err(|e| e.to_string())?;
    let require_http_overlay_token = settings
        .server_http_overlay_require_token()
        .await
        .map_err(|e| e.to_string())?;
    let cors_any_origin = settings
        .server_overlay_cors_any_origin()
        .await
        .map_err(|e| e.to_string())?;
    let overlay_root = settings
        .server_overlay_root()
        .await
        .map_err(|e| e.to_string())?;
    Ok(SettingsWebSocketSnapshot {
        bind_address,
        port,
        require_ws_token,
        lan_bind_enabled,
        require_http_overlay_token,
        cors_any_origin,
        overlay_root,
    })
}

pub fn handle_settings_websocket_msg(
    state: &mut SettingsWebSocketState,
    msg: SettingsWebSocketMsg,
    backend: &Arc<SqliteBackend>,
) -> Task<Message> {
    match msg {
        SettingsWebSocketMsg::LoadRequested => {
            let b = Arc::clone(backend);
            Task::perform(async move { load_settings_websocket(b).await }, |r| {
                Message::SettingsWebSocket(SettingsWebSocketMsg::LoadResult(r))
            })
        }
        SettingsWebSocketMsg::LoadResult(Ok(snap)) => {
            state.port = snap.port;
            state.port_input = snap.port.to_string();
            state.require_ws_token = snap.require_ws_token;
            state.require_http_overlay_token = snap.require_http_overlay_token;
            state.cors_any_origin = snap.cors_any_origin;
            state.bind_address_radio = if snap.lan_bind_enabled {
                BindAddressChoice::Lan
            } else {
                BindAddressChoice::Localhost
            };
            state.lan_bind_input = snap.bind_address;
            if let Some(root) = snap.overlay_root
                && !root.is_empty()
            {
                state.overlay_root = PathBuf::from(root);
            }
            state.all_changes_saved = true;
            state.save_error = None;
            Task::none()
        }
        SettingsWebSocketMsg::LoadResult(Err(e)) => {
            state.save_error = Some(e);
            Task::none()
        }
        SettingsWebSocketMsg::ToggleEnable(val) => {
            state.enable_server = val;
            state.all_changes_saved = false;
            let b = Arc::clone(backend);
            Task::perform(
                async move {
                    b.set_string("server.enabled", if val { "true" } else { "false" })
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| Message::SettingsWebSocket(SettingsWebSocketMsg::SaveStatus(r)),
            )
        }
        SettingsWebSocketMsg::SelectLocalhost => {
            state.bind_address_radio = BindAddressChoice::Localhost;
            state.lan_bind_modal_visible = false;
            state.all_changes_saved = false;
            let b = Arc::clone(backend);
            Task::perform(
                async move {
                    b.set_server_bind_address("127.0.0.1")
                        .await
                        .map_err(|e| e.to_string())?;
                    b.set_server_lan_bind_enabled(false)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| Message::SettingsWebSocket(SettingsWebSocketMsg::SaveStatus(r)),
            )
        }
        SettingsWebSocketMsg::SelectLan => {
            state.lan_bind_modal_visible = true;
            state.lan_bind_input = String::new();
            Task::none()
        }
        SettingsWebSocketMsg::PortChanged(s) => {
            state.port_input = s;
            Task::none()
        }
        SettingsWebSocketMsg::PortFocusLost => match state.port_input.parse::<u16>() {
            Ok(p) if p >= 1024 => {
                state.port = p;
                state.all_changes_saved = false;
                let b = Arc::clone(backend);
                Task::perform(
                    async move { b.set_server_port(p).await.map_err(|e| e.to_string()) },
                    |r| Message::SettingsWebSocket(SettingsWebSocketMsg::SaveStatus(r)),
                )
            }
            _ => {
                state.port_input = state.port.to_string();
                Task::none()
            }
        },
        SettingsWebSocketMsg::RequireWsToken(val) => {
            state.require_ws_token = val;
            state.all_changes_saved = false;
            let b = Arc::clone(backend);
            Task::perform(
                async move {
                    b.set_server_auth_required_for_reads(val)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| Message::SettingsWebSocket(SettingsWebSocketMsg::SaveStatus(r)),
            )
        }
        SettingsWebSocketMsg::RequireHttpOverlayToken(val) => {
            state.require_http_overlay_token = val;
            state.all_changes_saved = false;
            let b = Arc::clone(backend);
            Task::perform(
                async move {
                    b.set_string(
                        forge_storage::reserved_keys::SERVER_HTTP_OVERLAY_REQUIRE_TOKEN_KEY,
                        if val { "true" } else { "false" },
                    )
                    .await
                    .map_err(|e| e.to_string())
                },
                |r| Message::SettingsWebSocket(SettingsWebSocketMsg::SaveStatus(r)),
            )
        }
        SettingsWebSocketMsg::CorsAnyOrigin(val) => {
            state.cors_any_origin = val;
            state.all_changes_saved = false;
            let b = Arc::clone(backend);
            Task::perform(
                async move {
                    b.set_string(
                        forge_storage::reserved_keys::SERVER_OVERLAY_CORS_ANY_ORIGIN_KEY,
                        if val { "true" } else { "false" },
                    )
                    .await
                    .map_err(|e| e.to_string())
                },
                |r| Message::SettingsWebSocket(SettingsWebSocketMsg::SaveStatus(r)),
            )
        }
        SettingsWebSocketMsg::OverlayRootChanged(path) => {
            let path_str = path.to_string_lossy().into_owned();
            state.overlay_root = path;
            state.all_changes_saved = false;
            let b = Arc::clone(backend);
            Task::perform(
                async move {
                    b.set_string(
                        forge_storage::reserved_keys::SERVER_OVERLAY_ROOT_KEY,
                        &path_str,
                    )
                    .await
                    .map_err(|e| e.to_string())
                },
                |r| Message::SettingsWebSocket(SettingsWebSocketMsg::SaveStatus(r)),
            )
        }
        SettingsWebSocketMsg::LanBindInputChanged(s) => {
            state.lan_bind_input = s;
            Task::none()
        }
        SettingsWebSocketMsg::LanBindCancelled => {
            state.lan_bind_modal_visible = false;
            state.lan_bind_input = String::new();
            state.bind_address_radio = BindAddressChoice::Localhost;
            Task::none()
        }
        SettingsWebSocketMsg::LanBindConfirmed => {
            if state.lan_bind_input != "expose to LAN" {
                return Task::none();
            }
            state.lan_bind_modal_visible = false;
            state.lan_bind_input = String::new();
            state.bind_address_radio = BindAddressChoice::Lan;
            state.all_changes_saved = false;
            let b = Arc::clone(backend);
            Task::perform(
                async move {
                    b.set_server_bind_address("0.0.0.0")
                        .await
                        .map_err(|e| e.to_string())?;
                    b.set_server_lan_bind_enabled(true)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| Message::SettingsWebSocket(SettingsWebSocketMsg::SaveStatus(r)),
            )
        }
        SettingsWebSocketMsg::SaveStatus(Ok(())) => {
            state.all_changes_saved = true;
            state.save_error = None;
            Task::none()
        }
        SettingsWebSocketMsg::SaveStatus(Err(e)) => {
            state.all_changes_saved = false;
            state.save_error = Some(e);
            Task::none()
        }
    }
}

fn section_rule<'a>(border_color: Color) -> Element<'a, Message> {
    rule::horizontal(0.5_f32)
        .style(move |_: &iced::Theme| rule::Style {
            color: border_color,
            radius: 0.0.into(),
            fill_mode: rule::FillMode::Full,
            snap: true,
        })
        .into()
}

fn auth_divider<'a>(border_color: Color) -> Element<'a, Message> {
    container(Space::new().width(Length::Fill).height(0.5_f32))
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(border_color)),
            ..container::Style::default()
        })
        .into()
}

fn auth_toggle_row<'a>(
    icon: char,
    icon_color: Color,
    label: &'a str,
    sublabel: &'a str,
    value: bool,
    on_toggle: Message,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let icon_el = text(icon.to_string())
        .font(BOOTSTRAP_FONT)
        .size(14.0_f32)
        .color(icon_color);

    let label_el = text(label).size(FONT_BODY).color(palette.text_primary);
    let sub_el = text(sublabel)
        .size(FONT_SM)
        .color(palette.text_faint)
        .font(font(FontRole::Body));
    let label_col = column![label_el, sub_el].spacing(2);

    let track_bg = if value {
        palette.brand
    } else {
        palette.surface_overlay
    };
    let thumb_size = 14.0_f32;
    let thumb_offset: f32 = if value { 18.0 } else { 2.0 };

    let thumb = container(Space::new().width(thumb_size).height(thumb_size))
        .width(thumb_size)
        .height(thumb_size)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(Color::WHITE)),
            border: Border {
                radius: (thumb_size / 2.0).into(),
                color: Color::TRANSPARENT,
                width: 0.0,
            },
            ..container::Style::default()
        });
    let thumb_padded = container(thumb).padding(iced::Padding {
        top: 2.0,
        right: 0.0,
        bottom: 0.0,
        left: thumb_offset,
    });
    let track_width = 32.0_f32;
    let track_height = 18.0_f32;
    let track = container(thumb_padded)
        .width(track_width)
        .height(track_height)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(track_bg)),
            border: Border {
                radius: (track_height / 2.0).into(),
                color: Color::TRANSPARENT,
                width: 0.0,
            },
            ..container::Style::default()
        });

    let inner = row![icon_el, container(label_col).width(Length::Fill), track,]
        .spacing(10)
        .align_y(Alignment::Center);

    button(inner)
        .on_press(on_toggle)
        .padding([8_u16, 0_u16])
        .width(Length::Fill)
        .style(|_: &iced::Theme, _status| iced::widget::button::Style {
            background: None,
            border: Border::default(),
            text_color: Color::TRANSPARENT,
            shadow: iced::Shadow::default(),
            snap: false,
        })
        .into()
}

fn port_input_style(
    p: ForgePalette,
) -> impl Fn(&iced::Theme, iced::widget::text_input::Status) -> iced::widget::text_input::Style {
    move |_theme, _status| iced::widget::text_input::Style {
        background: Background::Color(p.shell),
        border: Border {
            color: p.border_input,
            width: 0.5,
            radius: radius(Radius::Md).into(),
        },
        icon: p.text_muted,
        placeholder: p.text_muted,
        value: p.text_primary,
        selection: Color { a: 0.25, ..p.brand },
    }
}

fn overlay_path_display<'a>(root: &'a Path, palette: &'a ForgePalette) -> Element<'a, Message> {
    let path_str: &str = if root.as_os_str().is_empty() {
        "~/.local/share/forge/overlays"
    } else {
        root.to_str().unwrap_or("~/.local/share/forge/overlays")
    };

    let path_box = container(
        text(path_str)
            .font(font(FontRole::Monospace))
            .size(FONT_SM)
            .color(palette.text_primary),
    )
    .width(Length::Fill)
    .padding([7_u16, 12_u16])
    .style(move |_| container::Style {
        background: Some(Background::Color(palette.shell)),
        border: Border {
            color: palette.border_regular,
            width: 0.5,
            radius: radius(Radius::Md).into(),
        },
        ..container::Style::default()
    });

    let border_color = palette.border_regular;
    let text_sec = palette.text_secondary;
    let browse_btn = button(
        row![
            text(ICON_FOLDER_OPEN.to_string())
                .font(BOOTSTRAP_FONT)
                .size(12.0_f32)
                .color(text_sec),
            text("Browse").size(FONT_SM).color(text_sec),
        ]
        .spacing(5)
        .align_y(Alignment::Center),
    )
    .on_press(Message::Noop)
    .padding([7_u16, 12_u16])
    .style(
        move |_: &iced::Theme, _status| iced::widget::button::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            border: Border {
                color: border_color,
                width: 0.5,
                radius: radius(Radius::Md).into(),
            },
            text_color: text_sec,
            shadow: iced::Shadow::default(),
            snap: false,
        },
    );

    row![path_box, browse_btn]
        .spacing(6)
        .align_y(Alignment::Center)
        .into()
}

fn section_label<'a>(label: &'a str, palette: &'a ForgePalette) -> Element<'a, Message> {
    text(label)
        .size(FONT_BODY)
        .color(palette.text_primary)
        .font(iced::Font {
            weight: iced::font::Weight::Medium,
            ..font(FontRole::Body)
        })
        .into()
}

pub fn settings_websocket_view<'a>(
    state: &'a SettingsWebSocketState,
    token: &'a str,
    token_revealed: bool,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let p = palette;

    let save_indicator: Element<'a, Message> = if let Some(ref err) = state.save_error {
        text(format!("Save failed: {err}"))
            .size(FONT_SM)
            .color(p.random)
            .into()
    } else if state.all_changes_saved {
        row![
            text(ICON_CHECK_CIRCLE.to_string())
                .font(BOOTSTRAP_FONT)
                .size(13.0_f32)
                .color(p.success),
            text("All changes saved").size(FONT_SM).color(p.success),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .into()
    } else {
        text("Saving…").size(FONT_SM).color(p.text_faint).into()
    };

    let header_row = row![
        text(ICON_SERVER.to_string())
            .font(BOOTSTRAP_FONT)
            .size(20.0_f32)
            .color(p.brand),
        text("WebSocket server")
            .size(18.0_f32)
            .color(p.text_primary)
            .font(iced::Font {
                weight: iced::font::Weight::Medium,
                ..font(FontRole::Body)
            }),
        Space::new().width(Length::Fill),
        save_indicator,
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    let subtitle = text("Configure how overlays and third-party tools connect to Forge.")
        .size(FONT_SM)
        .color(p.text_muted);

    let enable_toggle = toggle(
        p,
        ToggleProps {
            label: "Enable server",
            description: "Starts on app launch, hosts overlays, accepts WS clients",
            value: state.enable_server,
            on_toggle: Message::SettingsWebSocket(SettingsWebSocketMsg::ToggleEnable(
                !state.enable_server,
            )),
        },
    );

    let localhost_card = bind_address_card(
        BindAddressCardParams {
            title: "Localhost only",
            tech_label: "127.0.0.1",
            badge: BindBadge::Recommended,
            description: "Only apps on this machine can connect. Browser sources in OBS and local Stream Deck plugins work normally. Safe default.",
            selected: state.bind_address_radio == BindAddressChoice::Localhost,
        },
        Message::SettingsWebSocket(SettingsWebSocketMsg::SelectLocalhost),
        p,
    );

    let lan_card = bind_address_card(
        BindAddressCardParams {
            title: "All interfaces (LAN)",
            tech_label: "0.0.0.0",
            badge: BindBadge::RequiresConfirmation,
            description: "Lets other devices on your network (phone, tablet, second PC) connect to Forge. Exposes the server to anyone on the same Wi-Fi or LAN.",
            selected: state.bind_address_radio == BindAddressChoice::Lan,
        },
        Message::SettingsWebSocket(SettingsWebSocketMsg::SelectLan),
        p,
    );

    let mut bind_col = column![
        section_label("Bind address", p),
        text("Which interface the server listens on")
            .size(FONT_SM)
            .color(p.text_muted),
        localhost_card,
        lan_card
    ]
    .spacing(8);

    if state.bind_address_radio == BindAddressChoice::Lan {
        bind_col = bind_col.push(
            text("Restart server to apply bind address change.")
                .size(FONT_SM)
                .color(p.warning),
        );
    }

    let port_field = text_input("8081", &state.port_input)
        .on_input(|s| Message::SettingsWebSocket(SettingsWebSocketMsg::PortChanged(s)))
        .on_submit(Message::SettingsWebSocket(
            SettingsWebSocketMsg::PortFocusLost,
        ))
        .font(font(FontRole::Monospace))
        .size(FONT_SM)
        .padding([7_u16, 12_u16])
        .width(Length::Fill)
        .style(port_input_style(*p));

    let port_col = container(
        column![
            section_label("Port", p),
            text("Default 8081 · range 1024–65535")
                .size(FONT_SM)
                .color(p.text_muted),
            port_field,
        ]
        .spacing(8),
    )
    .width(Length::FillPortion(5));

    let token_desc_row = row![
        text("Clients send this in ")
            .size(FONT_SM)
            .color(p.text_muted),
        text("Authorization: Bearer …")
            .font(font(FontRole::Monospace))
            .size(FONT_SM)
            .color(p.text_primary),
    ]
    .align_y(Alignment::Center);

    let token_widget = bearer_token_display(
        token,
        token_revealed,
        Message::Server(ServerScreenMsg::ToggleTokenReveal),
        Message::Server(ServerScreenMsg::CopyToken),
        Message::Server(ServerScreenMsg::RegenerateToken),
        p,
    );

    let token_col = container(
        column![
            section_label("Bearer token", p),
            token_desc_row,
            token_widget
        ]
        .spacing(8),
    )
    .width(Length::FillPortion(8));

    let port_token_row = row![port_col, token_col].spacing(14);

    let auth_section = column![
        section_label("Authentication", p),
        text("Which clients need to authenticate")
            .size(FONT_SM)
            .color(p.text_muted),
        auth_toggle_row(
            ICON_LOCK,
            p.success,
            "Require token for WebSocket clients",
            "Reject WS handshake without valid bearer token",
            state.require_ws_token,
            Message::SettingsWebSocket(SettingsWebSocketMsg::RequireWsToken(
                !state.require_ws_token,
            )),
            p,
        ),
        auth_divider(p.border_regular),
        auth_toggle_row(
            ICON_GLOBE,
            p.info,
            "Require token for HTTP overlay files",
            "Browser sources need ?token=… in URL",
            state.require_http_overlay_token,
            Message::SettingsWebSocket(SettingsWebSocketMsg::RequireHttpOverlayToken(
                !state.require_http_overlay_token,
            )),
            p,
        ),
        auth_divider(p.border_regular),
        auth_toggle_row(
            ICON_ALERT_TRIANGLE,
            p.warning,
            "Allow CORS from any origin",
            "Disable to restrict to overlay browser sources only",
            state.cors_any_origin,
            Message::SettingsWebSocket(SettingsWebSocketMsg::CorsAnyOrigin(!state.cors_any_origin)),
            p,
        ),
    ]
    .spacing(0);

    let overlay_desc_row = row![
        text("Folder served at ").size(FONT_SM).color(p.text_muted),
        text("http://<bind>/")
            .font(font(FontRole::Monospace))
            .size(FONT_SM)
            .color(p.text_primary),
    ]
    .align_y(Alignment::Center);

    let overlay_section = column![
        section_label("Overlay host root", p),
        overlay_desc_row,
        overlay_path_display(state.overlay_root.as_path(), p),
    ]
    .spacing(8);

    let content = scrollable(
        column![
            header_row,
            subtitle,
            enable_toggle,
            section_rule(p.border_regular),
            bind_col,
            section_rule(p.border_regular),
            port_token_row,
            section_rule(p.border_regular),
            auth_section,
            section_rule(p.border_regular),
            overlay_section,
        ]
        .spacing(18)
        .padding([20_u16, 24_u16]),
    )
    .width(Length::Fill)
    .height(Length::Fill);

    if state.lan_bind_modal_visible {
        let modal = type_to_confirm_modal(
            TypeToConfirmModalParams {
                title: "Expose Forge to your network?",
                explanation: "You're switching from 127.0.0.1 (localhost only) to 0.0.0.0 (all network interfaces). Other devices on your LAN — and anyone on the same Wi-Fi — will be able to reach the Forge server.",
                bullets: &LAN_BIND_BULLETS,
                confirmation_phrase: "expose to LAN",
                current_input: &state.lan_bind_input,
                confirm_label: "Expose to LAN",
            },
            |s| Message::SettingsWebSocket(SettingsWebSocketMsg::LanBindInputChanged(s)),
            Message::SettingsWebSocket(SettingsWebSocketMsg::LanBindCancelled),
            Message::SettingsWebSocket(SettingsWebSocketMsg::LanBindConfirmed),
            p,
        );
        stack![content, modal].into()
    } else {
        content.into()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const TEST_KEY: [u8; 32] = [0xab; 32];

    fn make_backend() -> Arc<SqliteBackend> {
        let rt = tokio::runtime::Runtime::new().unwrap();
        Arc::new(
            rt.block_on(SqliteBackend::open_with_key("sqlite::memory:", TEST_KEY))
                .unwrap(),
        )
    }

    #[test]
    fn select_lan_opens_modal_without_committing() {
        let backend = make_backend();
        let mut state = SettingsWebSocketState::default();
        let _ =
            handle_settings_websocket_msg(&mut state, SettingsWebSocketMsg::SelectLan, &backend);
        assert!(state.lan_bind_modal_visible);
        assert_eq!(state.bind_address_radio, BindAddressChoice::Localhost);
        assert!(state.lan_bind_input.is_empty());
    }

    #[test]
    fn lan_bind_confirmed_with_wrong_phrase_leaves_modal_open() {
        let backend = make_backend();
        let mut state = SettingsWebSocketState {
            lan_bind_modal_visible: true,
            lan_bind_input: "wrong".to_owned(),
            ..Default::default()
        };
        let _ = handle_settings_websocket_msg(
            &mut state,
            SettingsWebSocketMsg::LanBindConfirmed,
            &backend,
        );
        assert!(state.lan_bind_modal_visible);
        assert_eq!(state.bind_address_radio, BindAddressChoice::Localhost);
    }

    #[test]
    fn lan_bind_confirmed_with_correct_phrase_sets_lan() {
        let backend = make_backend();
        let mut state = SettingsWebSocketState {
            lan_bind_modal_visible: true,
            lan_bind_input: "expose to LAN".to_owned(),
            ..Default::default()
        };
        let _ = handle_settings_websocket_msg(
            &mut state,
            SettingsWebSocketMsg::LanBindConfirmed,
            &backend,
        );
        assert!(!state.lan_bind_modal_visible);
        assert_eq!(state.bind_address_radio, BindAddressChoice::Lan);
        assert!(state.lan_bind_input.is_empty());
    }

    #[test]
    fn lan_bind_cancelled_resets_to_localhost() {
        let backend = make_backend();
        let mut state = SettingsWebSocketState {
            lan_bind_modal_visible: true,
            bind_address_radio: BindAddressChoice::Lan,
            lan_bind_input: "partial".to_owned(),
            ..Default::default()
        };
        let _ = handle_settings_websocket_msg(
            &mut state,
            SettingsWebSocketMsg::LanBindCancelled,
            &backend,
        );
        assert!(!state.lan_bind_modal_visible);
        assert_eq!(state.bind_address_radio, BindAddressChoice::Localhost);
        assert!(state.lan_bind_input.is_empty());
    }

    #[test]
    fn port_focus_lost_valid_port_updates_state() {
        let backend = make_backend();
        let mut state = SettingsWebSocketState {
            port_input: "9000".to_owned(),
            ..Default::default()
        };
        let _ = handle_settings_websocket_msg(
            &mut state,
            SettingsWebSocketMsg::PortFocusLost,
            &backend,
        );
        assert_eq!(state.port, 9000);
        assert_eq!(state.port_input, "9000");
    }

    #[test]
    fn port_focus_lost_invalid_port_resets_input() {
        let backend = make_backend();
        let mut state = SettingsWebSocketState {
            port_input: "not_a_port".to_owned(),
            ..Default::default()
        };
        let _ = handle_settings_websocket_msg(
            &mut state,
            SettingsWebSocketMsg::PortFocusLost,
            &backend,
        );
        assert_eq!(state.port, 8081);
        assert_eq!(state.port_input, "8081");
    }

    #[test]
    fn port_focus_lost_below_1024_resets_input() {
        let backend = make_backend();
        let mut state = SettingsWebSocketState {
            port_input: "80".to_owned(),
            ..Default::default()
        };
        let _ = handle_settings_websocket_msg(
            &mut state,
            SettingsWebSocketMsg::PortFocusLost,
            &backend,
        );
        assert_eq!(state.port, 8081);
        assert_eq!(state.port_input, "8081");
    }

    #[test]
    fn save_status_ok_sets_all_changes_saved() {
        let backend = make_backend();
        let mut state = SettingsWebSocketState {
            all_changes_saved: false,
            save_error: Some("previous error".to_owned()),
            ..Default::default()
        };
        let _ = handle_settings_websocket_msg(
            &mut state,
            SettingsWebSocketMsg::SaveStatus(Ok(())),
            &backend,
        );
        assert!(state.all_changes_saved);
        assert!(state.save_error.is_none());
    }

    #[test]
    fn save_status_err_records_error() {
        let backend = make_backend();
        let mut state = SettingsWebSocketState::default();
        let _ = handle_settings_websocket_msg(
            &mut state,
            SettingsWebSocketMsg::SaveStatus(Err("disk full".to_owned())),
            &backend,
        );
        assert!(!state.all_changes_saved);
        assert_eq!(state.save_error.as_deref(), Some("disk full"));
    }
}
