use std::sync::Arc;

use iced::widget::{button, column, container, row, text, text_input};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding, Shadow, Theme};

use forge_events::EventPublisher;
use forge_obs::{ObsError, ObsServerInfo, test_connect};
use forge_runtime::EventBus;
use forge_storage::{CredentialId, CredentialsRepo};
use forge_storage_sqlite::SqliteBackend;

use forge_widgets::ForgePalette;
use forge_widgets::icons::{
    BOOTSTRAP_FONT, ICON_ALERT_TRIANGLE, ICON_BROADCAST, ICON_CHECK_CIRCLE, ICON_EYE,
    ICON_EYE_SLASH, ICON_INFO_CIRCLE, ICON_LIGHTNING, ICON_REFRESH,
};
use forge_widgets::tokens::{
    FONT_BODY_LG, FONT_BODY_MD, FONT_BODY_SM, FONT_CAPS, FONT_CAPS_SM, FontRole, font,
};

use crate::Message;

const OBS_CREDENTIAL_ID: &str = "obs:default";

#[derive(Debug, Clone)]
pub struct ObsConnectionForm {
    pub host: String,
    pub port_text: String,
    pub password: String,
    pub password_revealed: bool,
    pub auto_reconnect: bool,
    pub connect_on_launch: bool,
}

impl Default for ObsConnectionForm {
    fn default() -> Self {
        Self {
            host: "localhost".to_owned(),
            port_text: "4455".to_owned(),
            password: String::new(),
            password_revealed: false,
            auto_reconnect: true,
            connect_on_launch: true,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub enum TestStatus {
    #[default]
    Idle,
    Running,
    Success(ObsServerSummary),
    Failure(String),
}

#[derive(Debug, Clone)]
pub struct ObsServerSummary {
    pub obs_websocket_version: String,
    pub scene_count: usize,
    pub rtt_ms: u32,
}

impl From<ObsServerInfo> for ObsServerSummary {
    fn from(info: ObsServerInfo) -> Self {
        Self {
            obs_websocket_version: info.obs_websocket_version,
            scene_count: info.scene_count,
            rtt_ms: info.rtt_ms,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ObsPanelState {
    pub form: ObsConnectionForm,
    pub test_status: TestStatus,
    pub connecting: bool,
    pub connect_error: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ObsPanelMsg {
    HostChanged(String),
    PortChanged(String),
    PasswordChanged(String),
    TogglePasswordReveal,
    ToggleAutoReconnect,
    ToggleConnectOnLaunch,
    TestRequested,
    TestResult(Result<ObsServerSummary, String>),
    ConnectRequested,
    ConnectError(String),
}

pub async fn run_test_connect(
    host: String,
    port: u16,
    password: Option<String>,
) -> Result<ObsServerSummary, String> {
    test_connect(&host, port, password.as_deref())
        .await
        .map(ObsServerSummary::from)
        .map_err(format_obs_error)
}

pub async fn save_obs_credentials(
    backend: Arc<SqliteBackend>,
    host: String,
    port: u16,
    password: String,
) -> Result<(), String> {
    let bundle = serde_json::json!({
        "url": format!("ws://{host}:{port}"),
        "password": password,
    });
    backend
        .store(&CredentialId::new(OBS_CREDENTIAL_ID), &bundle.to_string())
        .await
        .map_err(|e| e.to_string())
}

pub async fn connect_obs_from_form(
    backend: Arc<SqliteBackend>,
    bus: Arc<EventBus>,
    host: String,
    port: u16,
    password: String,
) -> Result<crate::message::ObsClientRef, String> {
    save_obs_credentials(Arc::clone(&backend), host.clone(), port, password.clone()).await?;
    let publisher: Arc<dyn EventPublisher> = bus;
    let pw: Option<&str> = if password.is_empty() {
        None
    } else {
        Some(&password)
    };
    let client = forge_obs::ObsClient::connect(&format!("ws://{host}:{port}"), pw, publisher)
        .await
        .map_err(format_obs_error)?;
    Ok(crate::message::ObsClientRef::new(Arc::new(client)))
}

fn format_obs_error(e: ObsError) -> String {
    e.to_string()
}

pub fn obs_disconnected_view<'a>(
    state: &'a ObsPanelState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let header = obs_header_card(palette);
    let two_column = row![
        obs_instructions_card(palette),
        obs_form_card(state, palette),
    ]
    .spacing(12.0)
    .width(Length::Fill);
    let tip = obs_tip_card(palette);

    container(column![header, two_column, tip].spacing(14.0))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding::from([18_u16, 22_u16]))
        .into()
}

fn obs_header_card<'a>(palette: &'a ForgePalette) -> Element<'a, Message> {
    let icon = container(
        text(ICON_BROADCAST.to_string())
            .size(24.0)
            .font(BOOTSTRAP_FONT)
            .color(palette.success),
    )
    .width(48.0)
    .height(48.0)
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(palette.surface_overlay)),
        border: Border {
            radius: 11.0.into(),
            color: Color::TRANSPARENT,
            width: 0.0,
        },
        ..container::Style::default()
    });

    let title_col = column![
        text("OBS Studio")
            .size(FONT_BODY_LG)
            .color(palette.text_primary),
        text("Connect to control scenes, sources, audio, filters, and recording")
            .size(FONT_BODY_SM)
            .color(palette.text_muted),
    ]
    .spacing(2.0);

    let inner = row![icon, container(title_col).width(Length::Fill)]
        .spacing(16.0)
        .align_y(Alignment::Center);

    container(inner)
        .width(Length::Fill)
        .padding(Padding::from([16_u16, 18_u16]))
        .style(card_style(palette))
        .into()
}

fn obs_instructions_card<'a>(palette: &'a ForgePalette) -> Element<'a, Message> {
    let title = row![
        text(ICON_INFO_CIRCLE.to_string())
            .size(14.0)
            .font(BOOTSTRAP_FONT)
            .color(palette.info),
        text("Before you start")
            .size(FONT_BODY_MD)
            .color(palette.text_primary),
    ]
    .spacing(7.0)
    .align_y(Alignment::Center);

    let title_wrap = container(title)
        .width(Length::Fill)
        .padding(Padding::from([12_u16, 14_u16]))
        .style(move |_theme: &Theme| container::Style {
            border: Border {
                color: palette.border_regular,
                width: 0.5,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        });

    let lead =
        text("In OBS Studio, enable the built-in WebSocket server, then copy the settings here.")
            .size(FONT_CAPS)
            .color(palette.text_muted)
            .wrapping(iced::widget::text::Wrapping::Word);

    let steps = column![
        instruction_step(
            1,
            "In OBS: Tools → WebSocket Server Settings",
            true,
            palette
        ),
        instruction_step(2, "Check 'Enable WebSocket server'", true, palette),
        instruction_step(3, "Note the port (default 4455)", true, palette),
        instruction_step(
            4,
            "Click 'Show Connect Info' to reveal password",
            false,
            palette
        ),
    ]
    .spacing(0.0);

    let requirements = container(
        column![
            text("REQUIREMENTS")
                .size(FONT_CAPS_SM)
                .color(palette.text_muted)
                .font(font(FontRole::Monospace)),
            check_row("OBS Studio 28+ (WebSocket v5 built-in)", palette),
            check_row("Running on the same machine or LAN-reachable", palette),
        ]
        .spacing(4.0),
    )
    .width(Length::Fill)
    .padding(Padding::from([10_u16, 12_u16]))
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(palette.shell)),
        border: Border {
            color: palette.border_regular,
            width: 0.5,
            radius: 7.0.into(),
        },
        ..container::Style::default()
    });

    let body = container(column![lead, steps, requirements].spacing(14.0))
        .width(Length::Fill)
        .padding(14.0);

    container(column![title_wrap, body])
        .width(Length::FillPortion(10))
        .style(card_style(palette))
        .into()
}

fn instruction_step<'a>(
    n: u8,
    body: &'a str,
    has_border: bool,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let inner = row![
        text(format!("{n}."))
            .size(FONT_CAPS_SM)
            .color(palette.brand)
            .font(font(FontRole::Monospace)),
        text(body)
            .size(FONT_CAPS)
            .color(palette.text_primary)
            .wrapping(iced::widget::text::Wrapping::Word),
    ]
    .spacing(10.0)
    .align_y(Alignment::Start);

    let border_width = if has_border { 0.5 } else { 0.0 };

    container(inner)
        .width(Length::Fill)
        .padding(Padding::from([7_u16, 0_u16]))
        .style(move |_theme: &Theme| container::Style {
            border: Border {
                color: palette.border_regular,
                width: border_width,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn check_row<'a>(label: &'a str, palette: &'a ForgePalette) -> Element<'a, Message> {
    row![
        text(ICON_CHECK_CIRCLE.to_string())
            .size(11.0)
            .font(BOOTSTRAP_FONT)
            .color(palette.success),
        text(label)
            .size(FONT_CAPS)
            .color(palette.text_secondary)
            .wrapping(iced::widget::text::Wrapping::Word),
    ]
    .spacing(6.0)
    .align_y(Alignment::Center)
    .into()
}

fn obs_form_card<'a>(state: &'a ObsPanelState, palette: &'a ForgePalette) -> Element<'a, Message> {
    let title = row![
        text(ICON_LIGHTNING.to_string())
            .size(14.0)
            .font(BOOTSTRAP_FONT)
            .color(palette.success),
        text("Connection settings")
            .size(FONT_BODY_MD)
            .color(palette.text_primary),
    ]
    .spacing(7.0)
    .align_y(Alignment::Center);

    let title_wrap = container(title)
        .width(Length::Fill)
        .padding(Padding::from([12_u16, 14_u16]))
        .style(move |_theme: &Theme| container::Style {
            border: Border {
                color: palette.border_regular,
                width: 0.5,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        });

    let host_input = text_input("localhost", &state.form.host)
        .on_input(|s| Message::ObsPanel(ObsPanelMsg::HostChanged(s)))
        .size(FONT_BODY_SM)
        .padding(Padding::from([7_u16, 11_u16]));
    let host_field = labeled_field("HOST", host_input.into(), palette);

    let port_input = text_input("4455", &state.form.port_text)
        .on_input(|s| Message::ObsPanel(ObsPanelMsg::PortChanged(s)))
        .size(FONT_BODY_SM)
        .padding(Padding::from([7_u16, 11_u16]));
    let port_field = labeled_field("PORT", port_input.into(), palette);

    let host_port = row![
        container(host_field).width(Length::FillPortion(8)),
        container(port_field).width(Length::FillPortion(5)),
    ]
    .spacing(10.0);

    let password_field = password_row(state, palette);

    let toggles = column![
        toggle_row(
            ICON_REFRESH,
            palette.info,
            "Auto-reconnect on disconnect",
            "Retry with exponential backoff",
            state.form.auto_reconnect,
            ObsPanelMsg::ToggleAutoReconnect,
            true,
            palette,
        ),
        toggle_row(
            ICON_LIGHTNING,
            palette.warning,
            "Connect on app launch",
            "Start connecting when Forge opens",
            state.form.connect_on_launch,
            ObsPanelMsg::ToggleConnectOnLaunch,
            false,
            palette,
        ),
    ]
    .spacing(0.0);

    let test_preview = test_status_preview(state, palette);

    let buttons = row![
        secondary_button(
            "Test connection",
            Message::ObsPanel(ObsPanelMsg::TestRequested),
            !state.connecting,
            palette,
        ),
        primary_button(
            "Connect",
            Message::ObsPanel(ObsPanelMsg::ConnectRequested),
            !state.connecting,
            palette,
        ),
    ]
    .spacing(8.0)
    .width(Length::Fill);

    let body =
        container(column![host_port, password_field, toggles, test_preview, buttons].spacing(12.0))
            .width(Length::Fill)
            .padding(14.0);

    container(column![title_wrap, body])
        .width(Length::FillPortion(12))
        .style(card_style(palette))
        .into()
}

fn labeled_field<'a>(
    label: &'a str,
    field: Element<'a, Message>,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    column![
        text(label)
            .size(FONT_CAPS_SM)
            .color(palette.text_muted)
            .font(font(FontRole::Monospace)),
        field,
    ]
    .spacing(5.0)
    .into()
}

fn password_row<'a>(state: &'a ObsPanelState, palette: &'a ForgePalette) -> Element<'a, Message> {
    let label = row![
        text("PASSWORD")
            .size(FONT_CAPS_SM)
            .color(palette.text_muted)
            .font(font(FontRole::Monospace)),
        iced::widget::Space::new().width(Length::Fill),
        text("stored in OS keychain")
            .size(FONT_CAPS_SM)
            .color(palette.text_faint),
    ]
    .align_y(Alignment::Center);

    let mut input = text_input("••••••••", &state.form.password)
        .on_input(|s| Message::ObsPanel(ObsPanelMsg::PasswordChanged(s)))
        .size(FONT_BODY_SM)
        .padding(Padding::from([7_u16, 11_u16]));
    if !state.form.password_revealed {
        input = input.secure(true);
    }

    let eye_icon = if state.form.password_revealed {
        ICON_EYE_SLASH
    } else {
        ICON_EYE
    };
    let eye_btn = button(
        text(eye_icon.to_string())
            .size(13.0)
            .font(BOOTSTRAP_FONT)
            .color(palette.text_muted),
    )
    .on_press(Message::ObsPanel(ObsPanelMsg::TogglePasswordReveal))
    .padding(Padding::from([7_u16, 10_u16]))
    .style(move |_theme: &Theme, _status| button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: palette.text_muted,
        border: Border {
            color: palette.border_regular,
            width: 0.5,
            radius: 7.0.into(),
        },
        shadow: Shadow::default(),
        snap: false,
    });

    let input_row = row![container(input).width(Length::Fill), eye_btn]
        .spacing(8.0)
        .align_y(Alignment::Center);

    column![label, input_row].spacing(5.0).into()
}

#[allow(clippy::too_many_arguments)]
fn toggle_row<'a>(
    icon: char,
    icon_color: Color,
    title: &'a str,
    subtitle: &'a str,
    on: bool,
    msg: ObsPanelMsg,
    has_border: bool,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let icon_el = text(icon.to_string())
        .size(13.0)
        .font(BOOTSTRAP_FONT)
        .color(icon_color);
    let text_col = column![
        text(title).size(FONT_BODY_SM).color(palette.text_primary),
        text(subtitle).size(FONT_CAPS_SM).color(palette.text_faint),
    ]
    .spacing(1.0);
    let left = row![icon_el, text_col]
        .spacing(9.0)
        .align_y(Alignment::Center);

    let knob_bg = if on {
        palette.success
    } else {
        palette.surface_overlay
    };
    let knob = container(iced::widget::Space::new())
        .width(28.0)
        .height(16.0)
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(knob_bg)),
            border: Border {
                radius: 8.0.into(),
                color: Color::TRANSPARENT,
                width: 0.0,
            },
            ..container::Style::default()
        });
    let toggle_btn = button(knob)
        .on_press(Message::ObsPanel(msg))
        .padding(0)
        .style(move |_theme: &Theme, _status| button::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            text_color: Color::TRANSPARENT,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 8.0.into(),
            },
            shadow: Shadow::default(),
            snap: false,
        });

    let inner = row![
        left,
        iced::widget::Space::new().width(Length::Fill),
        toggle_btn
    ]
    .align_y(Alignment::Center);

    let border_width = if has_border { 0.5 } else { 0.0 };

    container(inner)
        .width(Length::Fill)
        .padding(Padding::from([7_u16, 0_u16]))
        .style(move |_theme: &Theme| container::Style {
            border: Border {
                color: palette.border_regular,
                width: border_width,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn test_status_preview<'a>(
    state: &'a ObsPanelState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    match &state.test_status {
        TestStatus::Idle => iced::widget::Space::new().height(0.0).into(),
        TestStatus::Running => banner_card(
            ICON_REFRESH,
            palette.info,
            "Testing connection…",
            None,
            palette,
        ),
        TestStatus::Success(info) => banner_card(
            ICON_CHECK_CIRCLE,
            palette.success,
            "Test successful",
            Some(format!(
                "obs-websocket v{} · {} scenes · {}ms RTT",
                info.obs_websocket_version, info.scene_count, info.rtt_ms
            )),
            palette,
        ),
        TestStatus::Failure(msg) => banner_card(
            ICON_ALERT_TRIANGLE,
            palette.random,
            "Test failed",
            Some(msg.clone()),
            palette,
        ),
    }
}

fn banner_card<'a>(
    icon: char,
    accent: Color,
    title: &'a str,
    detail: Option<String>,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let icon_el = text(icon.to_string())
        .size(13.0)
        .font(BOOTSTRAP_FONT)
        .color(accent);
    let mut text_col = column![text(title).size(FONT_BODY_SM).color(palette.text_primary)];
    if let Some(d) = detail {
        text_col = text_col.push(
            text(d)
                .size(FONT_CAPS_SM)
                .color(palette.text_muted)
                .font(font(FontRole::Monospace))
                .wrapping(iced::widget::text::Wrapping::Word),
        );
    }
    let inner = row![icon_el, text_col]
        .spacing(9.0)
        .align_y(Alignment::Start);

    container(inner)
        .width(Length::Fill)
        .padding(Padding::from([8_u16, 11_u16]))
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(palette.shell)),
            border: Border {
                color: accent,
                width: 0.5,
                radius: 7.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn obs_tip_card<'a>(palette: &'a ForgePalette) -> Element<'a, Message> {
    let icon = text(ICON_INFO_CIRCLE.to_string())
        .size(14.0)
        .font(BOOTSTRAP_FONT)
        .color(palette.warning);
    let body = text(
        "Running OBS on a different PC? Set host to that machine's IP. Make sure OBS WebSocket is \
         configured to bind to 0.0.0.0 instead of localhost, and the port is open in firewall.",
    )
    .size(FONT_CAPS)
    .color(palette.text_muted)
    .wrapping(iced::widget::text::Wrapping::Word);

    container(row![icon, body].spacing(10.0).align_y(Alignment::Start))
        .width(Length::Fill)
        .padding(Padding::from([10_u16, 13_u16]))
        .style(card_style(palette))
        .into()
}

fn primary_button<'a>(
    label: &'a str,
    msg: Message,
    enabled: bool,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let mut b = button(
        text(label)
            .size(FONT_BODY_SM)
            .color(palette.shell)
            .align_x(Alignment::Center),
    )
    .padding(Padding::from([8_u16, 16_u16]))
    .width(Length::Fill)
    .style(move |_theme: &Theme, _status| button::Style {
        background: Some(Background::Color(palette.brand)),
        text_color: palette.shell,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 7.0.into(),
        },
        shadow: Shadow::default(),
        snap: false,
    });
    if enabled {
        b = b.on_press(msg);
    }
    b.into()
}

fn secondary_button<'a>(
    label: &'a str,
    msg: Message,
    enabled: bool,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let mut b = button(
        text(label)
            .size(FONT_BODY_SM)
            .color(palette.text_secondary)
            .align_x(Alignment::Center),
    )
    .padding(Padding::from([8_u16, 14_u16]))
    .width(Length::Fill)
    .style(move |_theme: &Theme, _status| button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: palette.text_secondary,
        border: Border {
            color: palette.border_regular,
            width: 0.5,
            radius: 7.0.into(),
        },
        shadow: Shadow::default(),
        snap: false,
    });
    if enabled {
        b = b.on_press(msg);
    }
    b.into()
}

fn card_style(palette: &ForgePalette) -> impl Fn(&Theme) -> container::Style + '_ {
    move |_theme: &Theme| container::Style {
        background: Some(Background::Color(palette.elevated)),
        border: Border {
            color: palette.border_regular,
            width: 0.5,
            radius: 12.0.into(),
        },
        ..container::Style::default()
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn default_form_has_localhost_4455() {
        let f = ObsConnectionForm::default();
        assert_eq!(f.host, "localhost");
        assert_eq!(f.port_text, "4455");
        assert!(f.auto_reconnect);
        assert!(f.connect_on_launch);
    }

    #[test]
    fn default_status_is_idle() {
        assert!(matches!(TestStatus::default(), TestStatus::Idle));
    }
}
