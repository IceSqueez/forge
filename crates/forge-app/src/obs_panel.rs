use std::sync::Arc;

use iced::widget::{button, column, container, row, text, text_input};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow, Task, Theme};

use forge_events::EventPublisher;
use forge_obs::{ObsError, ObsServerInfo, test_connect};
use forge_runtime::EventBus;
use forge_storage::CredentialsRepo;

use forge_widgets::ForgePalette;
use forge_widgets::icons::{Icon, tabler_icon};
use forge_widgets::tokens::{FONT_SM, FONT_XS, FontRole, Spacing, font, sp, spf};

use crate::Message;
use crate::runtime_view::RuntimeView;

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
    creds: Arc<dyn CredentialsRepo>,
    host: String,
    port: u16,
    password: String,
) -> Result<(), String> {
    forge_obs::credentials::store(&*creds, &host, port, &password)
        .await
        .map_err(|e| e.to_string())
}

pub async fn connect_obs_from_form(
    creds: Arc<dyn CredentialsRepo>,
    bus: Arc<EventBus>,
    host: String,
    port: u16,
    password: String,
) -> Result<crate::message::ObsClientRef, String> {
    forge_obs::credentials::store(&*creds, &host, port, &password)
        .await
        .map_err(|e| e.to_string())?;
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

pub fn update(state: &mut ObsPanelState, rt: &RuntimeView, msg: ObsPanelMsg) -> Task<Message> {
    match msg {
        ObsPanelMsg::HostChanged(v) => {
            state.form.host = v;
            state.test_status = TestStatus::Idle;
            Task::none()
        }
        ObsPanelMsg::PortChanged(v) => {
            state.form.port_text = v;
            state.test_status = TestStatus::Idle;
            Task::none()
        }
        ObsPanelMsg::PasswordChanged(v) => {
            state.form.password = v;
            state.test_status = TestStatus::Idle;
            Task::none()
        }
        ObsPanelMsg::TogglePasswordReveal => {
            state.form.password_revealed = !state.form.password_revealed;
            Task::none()
        }
        ObsPanelMsg::ToggleAutoReconnect => {
            state.form.auto_reconnect = !state.form.auto_reconnect;
            Task::none()
        }
        ObsPanelMsg::ToggleConnectOnLaunch => {
            state.form.connect_on_launch = !state.form.connect_on_launch;
            Task::none()
        }
        ObsPanelMsg::TestRequested => {
            let port = match state.form.port_text.parse::<u16>() {
                Ok(p) => p,
                Err(_) => {
                    state.test_status = TestStatus::Failure("port must be a number 1-65535".into());
                    return Task::none();
                }
            };
            let host = state.form.host.clone();
            let pw = if state.form.password.is_empty() {
                None
            } else {
                Some(state.form.password.clone())
            };
            state.test_status = TestStatus::Running;
            Task::perform(run_test_connect(host, port, pw), |r| {
                Message::ObsPanel(ObsPanelMsg::TestResult(r))
            })
        }
        ObsPanelMsg::TestResult(Ok(info)) => {
            state.test_status = TestStatus::Success(info);
            Task::none()
        }
        ObsPanelMsg::TestResult(Err(e)) => {
            state.test_status = TestStatus::Failure(e);
            Task::none()
        }
        ObsPanelMsg::ConnectRequested => {
            let port = match state.form.port_text.parse::<u16>() {
                Ok(p) => p,
                Err(_) => {
                    state.test_status = TestStatus::Failure("port must be a number 1-65535".into());
                    return Task::none();
                }
            };
            let host = state.form.host.clone();
            let password = state.form.password.clone();
            let creds: Arc<dyn CredentialsRepo> =
                Arc::clone(&rt.backend) as Arc<dyn CredentialsRepo>;
            let bus = Arc::clone(&rt.bus);
            state.connecting = true;
            state.connect_error = None;
            Task::perform(
                connect_obs_from_form(creds, bus, host, port, password),
                |r| match r {
                    Ok(client_ref) => Message::ObsBootResult(Ok(client_ref)),
                    Err(e) => Message::ObsPanel(ObsPanelMsg::ConnectError(e)),
                },
            )
        }
        ObsPanelMsg::ConnectError(e) => {
            state.connecting = false;
            state.connect_error = Some(e.clone());
            state.test_status = TestStatus::Failure(e);
            Task::none()
        }
    }
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
    .spacing(spf(Spacing::Sm))
    .width(Length::Fill);
    let tip = obs_tip_card(palette);

    let page_header = crate::page_chrome::simple_page_header(
        &[("Builtin", false), ("OBS Studio", true)],
        palette,
    );

    let body = container(column![header, two_column, tip].spacing(spf(Spacing::Sm)))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding([sp(Spacing::Md), sp(Spacing::Lg)]);

    column![page_header, body]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn obs_header_card<'a>(palette: &'a ForgePalette) -> Element<'a, Message> {
    let icon = container(tabler_icon(Icon::Broadcast, 24.0, palette.success))
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
        text("OBS Studio").size(FONT_SM).color(palette.text_primary),
        text("Connect to control scenes, sources, audio, filters, and recording")
            .size(FONT_SM)
            .color(palette.text_muted),
    ]
    .spacing(spf(Spacing::Xxs));

    let inner = row![icon, container(title_col).width(Length::Fill)]
        .spacing(spf(Spacing::Md))
        .align_y(Alignment::Center);

    container(inner)
        .width(Length::Fill)
        .padding([sp(Spacing::Md), sp(Spacing::Md)])
        .style(card_style(palette))
        .into()
}

fn obs_instructions_card<'a>(palette: &'a ForgePalette) -> Element<'a, Message> {
    let title = row![
        tabler_icon(Icon::InfoCircle, 14.0, palette.info),
        text("Before you start")
            .size(FONT_SM)
            .color(palette.text_primary),
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(Alignment::Center);

    let title_wrap = container(title)
        .width(Length::Fill)
        .padding([sp(Spacing::Sm), sp(Spacing::Sm)])
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
            .size(FONT_XS)
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
    .spacing(0);

    let requirements = container(
        column![
            text("REQUIREMENTS")
                .size(FONT_XS)
                .color(palette.text_muted)
                .font(font(FontRole::Monospace)),
            check_row("OBS Studio 28+ (WebSocket v5 built-in)", palette),
            check_row("Running on the same machine or LAN-reachable", palette),
        ]
        .spacing(spf(Spacing::Xxs)),
    )
    .width(Length::Fill)
    .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(palette.shell)),
        border: Border {
            color: palette.border_regular,
            width: 0.5,
            radius: 7.0.into(),
        },
        ..container::Style::default()
    });

    let body = container(column![lead, steps, requirements].spacing(spf(Spacing::Sm)))
        .width(Length::Fill)
        .padding(sp(Spacing::Sm));

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
            .size(FONT_XS)
            .color(palette.brand)
            .font(font(FontRole::Monospace)),
        text(body)
            .size(FONT_XS)
            .color(palette.text_primary)
            .wrapping(iced::widget::text::Wrapping::Word),
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(Alignment::Start);

    let border_width = if has_border { 0.5 } else { 0.0 };

    container(inner)
        .width(Length::Fill)
        .padding([sp(Spacing::Xs), 0])
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
        tabler_icon(Icon::CircleCheck, 11.0, palette.success),
        text(label)
            .size(FONT_XS)
            .color(palette.text_secondary)
            .wrapping(iced::widget::text::Wrapping::Word),
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(Alignment::Center)
    .into()
}

fn obs_form_card<'a>(state: &'a ObsPanelState, palette: &'a ForgePalette) -> Element<'a, Message> {
    let title = row![
        tabler_icon(Icon::Bolt, 14.0, palette.success),
        text("Connection settings")
            .size(FONT_SM)
            .color(palette.text_primary),
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(Alignment::Center);

    let title_wrap = container(title)
        .width(Length::Fill)
        .padding([sp(Spacing::Sm), sp(Spacing::Sm)])
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
        .size(FONT_SM)
        .padding([sp(Spacing::Xs), sp(Spacing::Sm)]);
    let host_field = labeled_field("HOST", host_input.into(), palette);

    let port_input = text_input("4455", &state.form.port_text)
        .on_input(|s| Message::ObsPanel(ObsPanelMsg::PortChanged(s)))
        .size(FONT_SM)
        .padding([sp(Spacing::Xs), sp(Spacing::Sm)]);
    let port_field = labeled_field("PORT", port_input.into(), palette);

    let host_port = row![
        container(host_field).width(Length::FillPortion(8)),
        container(port_field).width(Length::FillPortion(5)),
    ]
    .spacing(spf(Spacing::Xs));

    let password_field = password_row(state, palette);

    let toggles = column![
        toggle_row(
            Icon::Refresh,
            palette.info,
            "Auto-reconnect on disconnect",
            "Retry with exponential backoff",
            state.form.auto_reconnect,
            ObsPanelMsg::ToggleAutoReconnect,
            true,
            palette,
        ),
        toggle_row(
            Icon::Bolt,
            palette.warning,
            "Connect on app launch",
            "Start connecting when Forge opens",
            state.form.connect_on_launch,
            ObsPanelMsg::ToggleConnectOnLaunch,
            false,
            palette,
        ),
    ]
    .spacing(0);

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
    .spacing(spf(Spacing::Xs))
    .width(Length::Fill);

    let body = container(
        column![host_port, password_field, toggles, test_preview, buttons]
            .spacing(spf(Spacing::Sm)),
    )
    .width(Length::Fill)
    .padding(sp(Spacing::Sm));

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
            .size(FONT_XS)
            .color(palette.text_muted)
            .font(font(FontRole::Monospace)),
        field,
    ]
    .spacing(spf(Spacing::Xxs))
    .into()
}

fn password_row<'a>(state: &'a ObsPanelState, palette: &'a ForgePalette) -> Element<'a, Message> {
    let label = row![
        text("PASSWORD")
            .size(FONT_XS)
            .color(palette.text_muted)
            .font(font(FontRole::Monospace)),
        iced::widget::Space::new().width(Length::Fill),
        text("stored in OS keychain")
            .size(FONT_XS)
            .color(palette.text_faint),
    ]
    .align_y(Alignment::Center);

    let mut input = text_input("••••••••", &state.form.password)
        .on_input(|s| Message::ObsPanel(ObsPanelMsg::PasswordChanged(s)))
        .size(FONT_SM)
        .padding([sp(Spacing::Xs), sp(Spacing::Sm)]);
    if !state.form.password_revealed {
        input = input.secure(true);
    }

    let eye_icon = if state.form.password_revealed {
        Icon::EyeOff
    } else {
        Icon::Eye
    };
    let eye_btn = button(tabler_icon(eye_icon, 13.0, palette.text_muted))
        .on_press(Message::ObsPanel(ObsPanelMsg::TogglePasswordReveal))
        .padding([sp(Spacing::Xs), sp(Spacing::Xs)])
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
        .spacing(spf(Spacing::Xs))
        .align_y(Alignment::Center);

    column![label, input_row].spacing(spf(Spacing::Xxs)).into()
}

#[allow(clippy::too_many_arguments)]
fn toggle_row<'a>(
    icon: Icon,
    icon_color: Color,
    title: &'a str,
    subtitle: &'a str,
    on: bool,
    msg: ObsPanelMsg,
    has_border: bool,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let icon_el = tabler_icon(icon, 13.0, icon_color);
    let text_col = column![
        text(title).size(FONT_SM).color(palette.text_primary),
        text(subtitle).size(FONT_XS).color(palette.text_faint),
    ]
    .spacing(spf(Spacing::Xxs));
    let left = row![icon_el, text_col]
        .spacing(spf(Spacing::Xs))
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
        .padding([sp(Spacing::Xs), 0])
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
            Icon::Refresh,
            palette.info,
            "Testing connection…",
            None,
            palette,
        ),
        TestStatus::Success(info) => banner_card(
            Icon::CircleCheck,
            palette.success,
            "Test successful",
            Some(format!(
                "obs-websocket v{} · {} scenes · {}ms RTT",
                info.obs_websocket_version, info.scene_count, info.rtt_ms
            )),
            palette,
        ),
        TestStatus::Failure(msg) => banner_card(
            Icon::AlertTriangle,
            palette.random,
            "Test failed",
            Some(msg.clone()),
            palette,
        ),
    }
}

fn banner_card<'a>(
    icon: Icon,
    accent: Color,
    title: &'a str,
    detail: Option<String>,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let icon_el = tabler_icon(icon, 13.0, accent);
    let mut text_col = column![text(title).size(FONT_SM).color(palette.text_primary)];
    if let Some(d) = detail {
        text_col = text_col.push(
            text(d)
                .size(FONT_XS)
                .color(palette.text_muted)
                .font(font(FontRole::Monospace))
                .wrapping(iced::widget::text::Wrapping::Word),
        );
    }
    let inner = row![icon_el, text_col]
        .spacing(spf(Spacing::Xs))
        .align_y(Alignment::Start);

    container(inner)
        .width(Length::Fill)
        .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
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
    let icon = tabler_icon(Icon::InfoCircle, 14.0, palette.warning);
    let body = text(
        "Running OBS on a different PC? Set host to that machine's IP. Make sure OBS WebSocket is \
         configured to bind to 0.0.0.0 instead of localhost, and the port is open in firewall.",
    )
    .size(FONT_XS)
    .color(palette.text_muted)
    .wrapping(iced::widget::text::Wrapping::Word);

    container(
        row![icon, body]
            .spacing(spf(Spacing::Xs))
            .align_y(Alignment::Start),
    )
    .width(Length::Fill)
    .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
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
            .size(FONT_SM)
            .color(palette.shell)
            .align_x(Alignment::Center),
    )
    .padding([sp(Spacing::Xs), sp(Spacing::Md)])
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
            .size(FONT_SM)
            .color(palette.text_secondary)
            .align_x(Alignment::Center),
    )
    .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
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
