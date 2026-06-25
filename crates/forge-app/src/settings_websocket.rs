use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_server::ServerSettings;
use forge_storage::SettingsRepo;
use forge_widgets::{
    BindAddressCardParams, BindBadge, BulletItem, BulletKind, ForgePalette, Radius, ToggleProps,
    TypeToConfirmModalParams, bearer_token_display, bind_address_card,
    icons::{Icon, tabler_icon},
    toggle,
    tokens::{FONT_LG, FONT_SM, FontRole, Spacing, font, radius, spf},
    type_to_confirm_modal,
};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Task,
    widget::{Space, button, column, container, row, rule, scrollable, stack, text, text_input},
};

use crate::Message;
use crate::runtime_view::RuntimeView;
use crate::server_screen::ServerScreenMsg;

fn lan_bind_bullets() -> Vec<BulletItem> {
    vec![
        BulletItem {
            kind: BulletKind::Check,
            text: forge_widgets::tr!("settings_ws_lan_bullet_phone"),
        },
        BulletItem {
            kind: BulletKind::Warning,
            text: forge_widgets::tr!("settings_ws_lan_bullet_token_warning"),
        },
        BulletItem {
            kind: BulletKind::Warning,
            text: forge_widgets::tr!("settings_ws_lan_bullet_public_wifi"),
        },
        BulletItem {
            kind: BulletKind::Info,
            text: forge_widgets::tr!("settings_ws_lan_bullet_firewall"),
        },
    ]
}

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
    BrowseOverlayFolder,
    OverlayFolderPicked(Option<PathBuf>),
    LanBindInputChanged(String),
    LanBindCancelled,
    LanBindConfirmed,
    SaveStatus(Result<(), String>),
}

pub async fn load_settings_websocket(
    settings: Arc<dyn SettingsRepo>,
) -> Result<SettingsWebSocketSnapshot, String> {
    let snap = ServerSettings::load(settings.as_ref())
        .await
        .map_err(|e| e.to_string())?;
    Ok(SettingsWebSocketSnapshot {
        bind_address: snap.bind_address,
        port: snap.port,
        require_ws_token: snap.auth_required_for_reads,
        lan_bind_enabled: snap.lan_bind_enabled,
        require_http_overlay_token: snap.http_overlay_require_token,
        cors_any_origin: snap.overlay_cors_any_origin,
        overlay_root: snap.overlay_root,
    })
}

pub fn update(
    state: &mut SettingsWebSocketState,
    rt: &RuntimeView,
    msg: SettingsWebSocketMsg,
) -> Task<Message> {
    match msg {
        SettingsWebSocketMsg::LoadRequested => {
            let s: Arc<dyn SettingsRepo> = Arc::clone(&rt.backend) as Arc<dyn SettingsRepo>;
            Task::perform(async move { load_settings_websocket(s).await }, |r| {
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
            let s: Arc<dyn SettingsRepo> = Arc::clone(&rt.backend) as Arc<dyn SettingsRepo>;
            Task::perform(
                async move {
                    s.set_string("server.enabled", if val { "true" } else { "false" })
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
            let s: Arc<dyn SettingsRepo> = Arc::clone(&rt.backend) as Arc<dyn SettingsRepo>;
            Task::perform(
                async move {
                    ServerSettings::save_bind_address(s.as_ref(), "127.0.0.1")
                        .await
                        .map_err(|e| e.to_string())?;
                    ServerSettings::save_lan_bind_enabled(s.as_ref(), false)
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
                let s: Arc<dyn SettingsRepo> = Arc::clone(&rt.backend) as Arc<dyn SettingsRepo>;
                Task::perform(
                    async move {
                        ServerSettings::save_port(s.as_ref(), p)
                            .await
                            .map_err(|e| e.to_string())
                    },
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
            let s: Arc<dyn SettingsRepo> = Arc::clone(&rt.backend) as Arc<dyn SettingsRepo>;
            Task::perform(
                async move {
                    ServerSettings::save_auth_required_for_reads(s.as_ref(), val)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| Message::SettingsWebSocket(SettingsWebSocketMsg::SaveStatus(r)),
            )
        }
        SettingsWebSocketMsg::RequireHttpOverlayToken(val) => {
            state.require_http_overlay_token = val;
            state.all_changes_saved = false;
            let s: Arc<dyn SettingsRepo> = Arc::clone(&rt.backend) as Arc<dyn SettingsRepo>;
            Task::perform(
                async move {
                    ServerSettings::save_http_overlay_require_token(s.as_ref(), val)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| Message::SettingsWebSocket(SettingsWebSocketMsg::SaveStatus(r)),
            )
        }
        SettingsWebSocketMsg::CorsAnyOrigin(val) => {
            state.cors_any_origin = val;
            state.all_changes_saved = false;
            let s: Arc<dyn SettingsRepo> = Arc::clone(&rt.backend) as Arc<dyn SettingsRepo>;
            Task::perform(
                async move {
                    ServerSettings::save_overlay_cors_any_origin(s.as_ref(), val)
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
            let s: Arc<dyn SettingsRepo> = Arc::clone(&rt.backend) as Arc<dyn SettingsRepo>;
            Task::perform(
                async move {
                    ServerSettings::save_overlay_root(s.as_ref(), &path_str)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| Message::SettingsWebSocket(SettingsWebSocketMsg::SaveStatus(r)),
            )
        }
        SettingsWebSocketMsg::BrowseOverlayFolder => Task::perform(
            async move {
                rfd::AsyncFileDialog::new()
                    .pick_folder()
                    .await
                    .map(|h| h.path().to_path_buf())
            },
            |p| Message::SettingsWebSocket(SettingsWebSocketMsg::OverlayFolderPicked(p)),
        ),
        SettingsWebSocketMsg::OverlayFolderPicked(Some(path)) => {
            update(state, rt, SettingsWebSocketMsg::OverlayRootChanged(path))
        }
        SettingsWebSocketMsg::OverlayFolderPicked(None) => Task::none(),
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
            let s: Arc<dyn SettingsRepo> = Arc::clone(&rt.backend) as Arc<dyn SettingsRepo>;
            Task::perform(
                async move {
                    ServerSettings::save_bind_address(s.as_ref(), "0.0.0.0")
                        .await
                        .map_err(|e| e.to_string())?;
                    ServerSettings::save_lan_bind_enabled(s.as_ref(), true)
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
    icon: Icon,
    icon_color: Color,
    label: String,
    sublabel: String,
    value: bool,
    on_toggle: Message,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let icon_el = tabler_icon(icon, 14.0, icon_color);

    let label_el = text(label).size(FONT_SM).color(palette.text_primary);
    let sub_el = text(sublabel)
        .size(FONT_SM)
        .color(palette.text_faint)
        .font(font(FontRole::Body));
    let label_col = column![label_el, sub_el].spacing(spf(Spacing::Xxs));

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
        .spacing(spf(Spacing::Xs))
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
            tabler_icon(Icon::FolderOpen, 12.0, text_sec),
            text(forge_widgets::tr!("settings_ws_browse_btn"))
                .size(FONT_SM)
                .color(text_sec),
        ]
        .spacing(spf(Spacing::Xxs))
        .align_y(Alignment::Center),
    )
    .on_press(Message::SettingsWebSocket(
        SettingsWebSocketMsg::BrowseOverlayFolder,
    ))
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
        .spacing(spf(Spacing::Xs))
        .align_y(Alignment::Center)
        .into()
}

fn section_label<'a>(label: String, palette: &ForgePalette) -> Element<'a, Message> {
    text(label)
        .size(FONT_SM)
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
        text(forge_widgets::tr!(
            "settings_ws_save_failed",
            error = err.as_str()
        ))
        .size(FONT_SM)
        .color(p.random)
        .into()
    } else if state.all_changes_saved {
        row![
            tabler_icon(Icon::CircleCheck, 13.0, p.success),
            text(forge_widgets::tr!("settings_ws_all_saved"))
                .size(FONT_SM)
                .color(p.success),
        ]
        .spacing(spf(Spacing::Xs))
        .align_y(Alignment::Center)
        .into()
    } else {
        text(forge_widgets::tr!("settings_ws_saving"))
            .size(FONT_SM)
            .color(p.text_faint)
            .into()
    };

    let header_row = row![
        tabler_icon(Icon::Server, 20.0, p.brand),
        text(forge_widgets::tr!("settings_ws_title"))
            .size(FONT_LG)
            .color(p.text_primary)
            .font(iced::Font {
                weight: iced::font::Weight::Medium,
                ..font(FontRole::Body)
            }),
        Space::new().width(Length::Fill),
        save_indicator,
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(Alignment::Center);

    let subtitle = text(forge_widgets::tr!("settings_ws_subtitle"))
        .size(FONT_SM)
        .color(p.text_muted);

    let enable_toggle = toggle(
        p,
        ToggleProps {
            label: forge_widgets::tr!("settings_ws_enable_label"),
            description: forge_widgets::tr!("settings_ws_enable_description"),
            value: state.enable_server,
            on_toggle: Message::SettingsWebSocket(SettingsWebSocketMsg::ToggleEnable(
                !state.enable_server,
            )),
        },
    );

    let localhost_card = bind_address_card(
        BindAddressCardParams {
            title: forge_widgets::tr!("settings_ws_bind_localhost_title"),
            tech_label: "127.0.0.1",
            badge: BindBadge::Recommended,
            description: forge_widgets::tr!("settings_ws_bind_localhost_description"),
            selected: state.bind_address_radio == BindAddressChoice::Localhost,
        },
        Message::SettingsWebSocket(SettingsWebSocketMsg::SelectLocalhost),
        p,
    );

    let lan_card = bind_address_card(
        BindAddressCardParams {
            title: forge_widgets::tr!("settings_ws_bind_lan_title"),
            tech_label: "0.0.0.0",
            badge: BindBadge::RequiresConfirmation,
            description: forge_widgets::tr!("settings_ws_bind_lan_description"),
            selected: state.bind_address_radio == BindAddressChoice::Lan,
        },
        Message::SettingsWebSocket(SettingsWebSocketMsg::SelectLan),
        p,
    );

    let mut bind_col = column![
        section_label(forge_widgets::tr!("settings_ws_bind_section_title"), p),
        text(forge_widgets::tr!("settings_ws_bind_section_subtitle"))
            .size(FONT_SM)
            .color(p.text_muted),
        localhost_card,
        lan_card
    ]
    .spacing(spf(Spacing::Xs));

    if state.bind_address_radio == BindAddressChoice::Lan {
        bind_col = bind_col.push(
            text(forge_widgets::tr!("settings_ws_bind_lan_restart_warning"))
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
            section_label(forge_widgets::tr!("settings_ws_port_section_title"), p),
            text(forge_widgets::tr!("settings_ws_port_subtitle"))
                .size(FONT_SM)
                .color(p.text_muted),
            port_field,
        ]
        .spacing(spf(Spacing::Xs)),
    )
    .width(Length::FillPortion(5));

    let token_desc_row = row![
        text(forge_widgets::tr!("settings_ws_token_clients_send"))
            .size(FONT_SM)
            .color(p.text_muted),
        text(" Authorization: Bearer …")
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
            section_label(forge_widgets::tr!("settings_ws_token_section_title"), p),
            token_desc_row,
            token_widget
        ]
        .spacing(spf(Spacing::Xs)),
    )
    .width(Length::FillPortion(8));

    let port_token_row = row![port_col, token_col].spacing(spf(Spacing::Sm));

    let auth_section = column![
        section_label(forge_widgets::tr!("settings_ws_auth_section_title"), p),
        text(forge_widgets::tr!("settings_ws_auth_section_subtitle"))
            .size(FONT_SM)
            .color(p.text_muted),
        auth_toggle_row(
            Icon::Lock,
            p.success,
            forge_widgets::tr!("settings_ws_auth_require_ws_label"),
            forge_widgets::tr!("settings_ws_auth_require_ws_sublabel"),
            state.require_ws_token,
            Message::SettingsWebSocket(SettingsWebSocketMsg::RequireWsToken(
                !state.require_ws_token,
            )),
            p,
        ),
        auth_divider(p.border_regular),
        auth_toggle_row(
            Icon::Globe,
            p.info,
            forge_widgets::tr!("settings_ws_auth_require_http_label"),
            forge_widgets::tr!("settings_ws_auth_require_http_sublabel"),
            state.require_http_overlay_token,
            Message::SettingsWebSocket(SettingsWebSocketMsg::RequireHttpOverlayToken(
                !state.require_http_overlay_token,
            )),
            p,
        ),
        auth_divider(p.border_regular),
        auth_toggle_row(
            Icon::AlertTriangle,
            p.warning,
            forge_widgets::tr!("settings_ws_auth_cors_label"),
            forge_widgets::tr!("settings_ws_auth_cors_sublabel"),
            state.cors_any_origin,
            Message::SettingsWebSocket(SettingsWebSocketMsg::CorsAnyOrigin(!state.cors_any_origin)),
            p,
        ),
    ]
    .spacing(0);

    let overlay_desc_row = row![
        text(forge_widgets::tr!("settings_ws_overlay_folder_prefix"))
            .size(FONT_SM)
            .color(p.text_muted),
        text(" http://<bind>/")
            .font(font(FontRole::Monospace))
            .size(FONT_SM)
            .color(p.text_primary),
    ]
    .align_y(Alignment::Center);

    let overlay_section = column![
        section_label(forge_widgets::tr!("settings_ws_overlay_section_title"), p),
        overlay_desc_row,
        overlay_path_display(state.overlay_root.as_path(), p),
    ]
    .spacing(spf(Spacing::Xs));

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
        .spacing(spf(Spacing::Md))
        .padding([20_u16, 24_u16]),
    )
    .width(Length::Fill)
    .height(Length::Fill);

    if state.lan_bind_modal_visible {
        let modal = type_to_confirm_modal(
            TypeToConfirmModalParams {
                title: forge_widgets::tr!("settings_ws_lan_modal_title"),
                explanation: forge_widgets::tr!("settings_ws_lan_modal_explanation"),
                bullets: lan_bind_bullets(),
                confirmation_phrase: "expose to LAN",
                current_input: &state.lan_bind_input,
                confirm_label: forge_widgets::tr!("settings_ws_lan_modal_confirm_label"),
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
    use forge_runtime::{EventBus, NullEventLogRepo, ScriptRegistry};
    use forge_storage::CredentialsRepo;
    use forge_storage_sqlite::SqliteBackend;

    use crate::runtime_view::RuntimeView;
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
            pipeline_config: None,
            sound_player: None,
            twitch_builtin: None,
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
    fn select_lan_opens_modal_without_committing() {
        let rt = test_rt();
        let mut state = SettingsWebSocketState::default();
        let _ = update(&mut state, &rt, SettingsWebSocketMsg::SelectLan);
        assert!(state.lan_bind_modal_visible);
        assert_eq!(state.bind_address_radio, BindAddressChoice::Localhost);
        assert!(state.lan_bind_input.is_empty());
    }

    #[test]
    fn lan_bind_confirmed_with_wrong_phrase_leaves_modal_open() {
        let rt = test_rt();
        let mut state = SettingsWebSocketState {
            lan_bind_modal_visible: true,
            lan_bind_input: "wrong".to_owned(),
            ..Default::default()
        };
        let _ = update(&mut state, &rt, SettingsWebSocketMsg::LanBindConfirmed);
        assert!(state.lan_bind_modal_visible);
        assert_eq!(state.bind_address_radio, BindAddressChoice::Localhost);
    }

    #[test]
    fn lan_bind_confirmed_with_correct_phrase_sets_lan() {
        let rt = test_rt();
        let mut state = SettingsWebSocketState {
            lan_bind_modal_visible: true,
            lan_bind_input: "expose to LAN".to_owned(),
            ..Default::default()
        };
        let _ = update(&mut state, &rt, SettingsWebSocketMsg::LanBindConfirmed);
        assert!(!state.lan_bind_modal_visible);
        assert_eq!(state.bind_address_radio, BindAddressChoice::Lan);
        assert!(state.lan_bind_input.is_empty());
    }

    #[test]
    fn lan_bind_cancelled_resets_to_localhost() {
        let rt = test_rt();
        let mut state = SettingsWebSocketState {
            lan_bind_modal_visible: true,
            bind_address_radio: BindAddressChoice::Lan,
            lan_bind_input: "partial".to_owned(),
            ..Default::default()
        };
        let _ = update(&mut state, &rt, SettingsWebSocketMsg::LanBindCancelled);
        assert!(!state.lan_bind_modal_visible);
        assert_eq!(state.bind_address_radio, BindAddressChoice::Localhost);
        assert!(state.lan_bind_input.is_empty());
    }

    #[test]
    fn port_focus_lost_valid_port_updates_state() {
        let rt = test_rt();
        let mut state = SettingsWebSocketState {
            port_input: "9000".to_owned(),
            ..Default::default()
        };
        let _ = update(&mut state, &rt, SettingsWebSocketMsg::PortFocusLost);
        assert_eq!(state.port, 9000);
        assert_eq!(state.port_input, "9000");
    }

    #[test]
    fn port_focus_lost_invalid_port_resets_input() {
        let rt = test_rt();
        let mut state = SettingsWebSocketState {
            port_input: "not_a_port".to_owned(),
            ..Default::default()
        };
        let _ = update(&mut state, &rt, SettingsWebSocketMsg::PortFocusLost);
        assert_eq!(state.port, 8081);
        assert_eq!(state.port_input, "8081");
    }

    #[test]
    fn port_focus_lost_below_1024_resets_input() {
        let rt = test_rt();
        let mut state = SettingsWebSocketState {
            port_input: "80".to_owned(),
            ..Default::default()
        };
        let _ = update(&mut state, &rt, SettingsWebSocketMsg::PortFocusLost);
        assert_eq!(state.port, 8081);
        assert_eq!(state.port_input, "8081");
    }

    #[test]
    fn save_status_ok_sets_all_changes_saved() {
        let rt = test_rt();
        let mut state = SettingsWebSocketState {
            all_changes_saved: false,
            save_error: Some("previous error".to_owned()),
            ..Default::default()
        };
        let _ = update(&mut state, &rt, SettingsWebSocketMsg::SaveStatus(Ok(())));
        assert!(state.all_changes_saved);
        assert!(state.save_error.is_none());
    }

    #[test]
    fn save_status_err_records_error() {
        let rt = test_rt();
        let mut state = SettingsWebSocketState::default();
        let _ = update(
            &mut state,
            &rt,
            SettingsWebSocketMsg::SaveStatus(Err("disk full".to_owned())),
        );
        assert!(!state.all_changes_saved);
        assert_eq!(state.save_error.as_deref(), Some("disk full"));
    }
}
