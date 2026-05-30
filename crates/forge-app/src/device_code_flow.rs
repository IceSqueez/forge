use std::sync::Arc;

use forge_storage::CredentialsRepo;
use forge_types::PlatformId;
use forge_widgets::ForgePalette;
use forge_widgets::icons::{Icon, tabler_icon};
use forge_widgets::tokens::{FONT_SM, FONT_XS, FontRole, Spacing, font, sp, spf};
use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow, Task, Theme};

use crate::Screen;
use crate::message::Message;
use crate::runtime_view::RuntimeView;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceCodeFlowPhase {
    Idle,
    Starting,
    Polling,
    Authorized,
    Failed,
}

#[derive(Debug, Clone)]
pub struct DeviceCodeData {
    pub user_code: String,
    pub verification_url: String,
    pub device_code: String,
    pub interval_secs: u64,
}

#[derive(Debug, Clone)]
pub enum DeviceCodeFlowMsg {
    ConnectPressed,
    StartResult(Result<DeviceCodeData, String>),
    WaitResult(Result<(), String>),
    RetryPressed,
    CancelPressed,
    CopyCode,
    OpenVerificationUrl,
}

#[derive(Debug)]
pub struct DeviceCodeFlowState {
    pub platform: PlatformId,
    pub phase: DeviceCodeFlowPhase,
    pub user_code: Option<String>,
    pub verification_url: Option<String>,
    pub device_code: Option<String>,
    pub interval_secs: Option<u64>,
    pub error: Option<String>,
}

impl Default for DeviceCodeFlowState {
    fn default() -> Self {
        Self {
            platform: PlatformId::YouTube,
            phase: DeviceCodeFlowPhase::Idle,
            user_code: None,
            verification_url: None,
            device_code: None,
            interval_secs: None,
            error: None,
        }
    }
}

pub fn update(
    state: &mut DeviceCodeFlowState,
    rt: &RuntimeView,
    msg: DeviceCodeFlowMsg,
) -> Task<Message> {
    match msg {
        DeviceCodeFlowMsg::ConnectPressed => {
            state.phase = DeviceCodeFlowPhase::Starting;
            let platform = state.platform;
            Task::perform(async move { start_device_code(platform).await }, |r| {
                Message::DeviceCodeFlow(DeviceCodeFlowMsg::StartResult(r))
            })
        }
        DeviceCodeFlowMsg::StartResult(Ok(data)) => {
            state.user_code = Some(data.user_code.clone());
            state.verification_url = Some(data.verification_url.clone());
            state.device_code = Some(data.device_code.clone());
            state.interval_secs = Some(data.interval_secs);
            state.phase = DeviceCodeFlowPhase::Polling;
            let device_code = data.device_code;
            let interval_secs = data.interval_secs;
            let platform = state.platform;
            let credentials_repo: Arc<dyn CredentialsRepo> =
                Arc::clone(&rt.backend) as Arc<dyn CredentialsRepo>;
            Task::perform(
                async move {
                    wait_for_authorization(platform, device_code, interval_secs, credentials_repo)
                        .await
                },
                |r| Message::DeviceCodeFlow(DeviceCodeFlowMsg::WaitResult(r)),
            )
        }
        DeviceCodeFlowMsg::StartResult(Err(e)) => {
            state.phase = DeviceCodeFlowPhase::Failed;
            state.error = Some(e);
            Task::none()
        }
        DeviceCodeFlowMsg::WaitResult(Ok(())) => {
            state.phase = DeviceCodeFlowPhase::Authorized;
            Task::none()
        }
        DeviceCodeFlowMsg::WaitResult(Err(e)) => {
            state.phase = DeviceCodeFlowPhase::Failed;
            state.error = Some(e);
            Task::none()
        }
        DeviceCodeFlowMsg::RetryPressed => {
            state.phase = DeviceCodeFlowPhase::Idle;
            state.user_code = None;
            state.verification_url = None;
            state.device_code = None;
            state.interval_secs = None;
            state.error = None;
            Task::none()
        }
        DeviceCodeFlowMsg::CancelPressed => Task::done(Message::Navigate(Screen::Platforms)),
        DeviceCodeFlowMsg::CopyCode => {
            if let Some(code) = &state.user_code {
                iced::clipboard::write::<Message>(code.clone())
            } else {
                Task::none()
            }
        }
        DeviceCodeFlowMsg::OpenVerificationUrl => {
            if let Some(url) = state.verification_url.clone() {
                Task::perform(
                    async move {
                        if let Err(e) = open::that(&url) {
                            tracing::warn!(error = %e, url = %url, "open browser failed");
                        }
                    },
                    |()| Message::Noop,
                )
            } else {
                Task::none()
            }
        }
    }
}

async fn start_device_code(platform: PlatformId) -> Result<DeviceCodeData, String> {
    match platform {
        PlatformId::YouTube => {
            let cid = std::env::var("FORGE_YOUTUBE_CLIENT_ID")
                .map_err(|_| "FORGE_YOUTUBE_CLIENT_ID is not set".to_owned())?;
            let csec = std::env::var("FORGE_YOUTUBE_CLIENT_SECRET")
                .map_err(|_| "FORGE_YOUTUBE_CLIENT_SECRET is not set".to_owned())?;
            let flow = forge_platform_youtube::GoogleAuthFlow::new(cid, csec);
            let code = flow.start().await.map_err(|e| e.to_string())?;
            Ok(DeviceCodeData {
                user_code: code.user_code,
                verification_url: code.verification_url,
                device_code: code.device_code,
                interval_secs: code.interval.as_secs(),
            })
        }
        PlatformId::Twitch => Err(
            "DeviceCodeFlow screen not yet wired for Twitch — use the Twitch integration panel"
                .to_owned(),
        ),
        PlatformId::Kick | PlatformId::Trovo => Err(format!(
            "{} does not support device code flow",
            platform_display_name(platform)
        )),
    }
}

async fn wait_for_authorization(
    platform: PlatformId,
    device_code: String,
    interval_secs: u64,
    credentials_repo: Arc<dyn CredentialsRepo>,
) -> Result<(), String> {
    match platform {
        PlatformId::YouTube => {
            let cid = std::env::var("FORGE_YOUTUBE_CLIENT_ID")
                .map_err(|_| "FORGE_YOUTUBE_CLIENT_ID is not set".to_owned())?;
            let csec = std::env::var("FORGE_YOUTUBE_CLIENT_SECRET")
                .map_err(|_| "FORGE_YOUTUBE_CLIENT_SECRET is not set".to_owned())?;
            let flow = forge_platform_youtube::GoogleAuthFlow::new(cid, csec);
            let bundle = flow
                .wait_for_authorization(&device_code, std::time::Duration::from_secs(interval_secs))
                .await
                .map_err(|e| e.to_string())?;
            let manager =
                forge_platform_youtube::YoutubeCredentialsManager::new(credentials_repo, flow);
            manager
                .save_from_bundle(bundle)
                .await
                .map_err(|e| e.to_string())
        }
        PlatformId::Twitch => Err(
            "DeviceCodeFlow screen not yet wired for Twitch — use the Twitch integration panel"
                .to_owned(),
        ),
        PlatformId::Kick | PlatformId::Trovo => Err(format!(
            "{} does not support device code flow",
            platform_display_name(platform)
        )),
    }
}

fn platform_display_name(p: PlatformId) -> &'static str {
    match p {
        PlatformId::Twitch => "Twitch",
        PlatformId::YouTube => "YouTube",
        PlatformId::Kick => "Kick",
        PlatformId::Trovo => "Trovo",
    }
}

fn platform_dot_color(p: PlatformId, palette: &ForgePalette) -> Color {
    match p {
        PlatformId::Twitch => palette.brand,
        PlatformId::YouTube => palette.random,
        PlatformId::Kick => palette.info,
        PlatformId::Trovo => palette.success,
    }
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

fn section_border_style(palette: &ForgePalette) -> impl Fn(&Theme) -> container::Style + '_ {
    move |_theme: &Theme| container::Style {
        border: Border {
            color: palette.border_regular,
            width: 0.5,
            radius: 0.0.into(),
        },
        ..container::Style::default()
    }
}

fn platform_header_card<'a>(
    name: &'a str,
    dot_color: Color,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let dot = container(iced::widget::Space::new())
        .width(40.0)
        .height(40.0)
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(dot_color)),
            border: Border {
                radius: 10.0.into(),
                color: Color::TRANSPARENT,
                width: 0.0,
            },
            ..container::Style::default()
        });

    let title = text(name).size(FONT_SM).color(palette.text_primary);
    let subtitle = text("Connect to enable live chat and events")
        .size(FONT_XS)
        .color(palette.text_muted);
    let title_col = column![title, subtitle].spacing(spf(Spacing::Xxs));

    let inner = row![dot, container(title_col).width(Length::Fill)]
        .spacing(spf(Spacing::Md))
        .align_y(Alignment::Center);

    container(inner)
        .width(Length::Fill)
        .padding([sp(Spacing::Md), sp(Spacing::Md)])
        .style(card_style(palette))
        .into()
}

fn flow_intro<'a>(name: &'a str, palette: &'a ForgePalette) -> Element<'a, Message> {
    let title_row = row![
        tabler_icon(Icon::Lock, 14.0, palette.brand),
        text(format!("Authorize Forge on {name}"))
            .size(FONT_SM)
            .color(palette.text_primary),
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(Alignment::Center);

    let subtitle = text(
        "This platform uses device code authorization. \
         You will see a code below — enter it on the platform's site \
         and we will detect when you are done. We never see your password.",
    )
    .size(FONT_XS)
    .color(palette.text_muted)
    .wrapping(iced::widget::text::Wrapping::Word);

    container(column![title_row, subtitle].spacing(spf(Spacing::Xxs)))
        .width(Length::Fill)
        .padding([sp(Spacing::Sm), sp(Spacing::Md)])
        .style(section_border_style(palette))
        .into()
}

fn primary_btn<'a>(
    label: &'a str,
    msg: Message,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    button(text(label).size(FONT_SM).color(palette.shell))
        .on_press(msg)
        .padding([sp(Spacing::Xs), sp(Spacing::Md)])
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
        })
        .into()
}

fn ghost_btn<'a>(label: &'a str, msg: Message, palette: &'a ForgePalette) -> Element<'a, Message> {
    button(text(label).size(FONT_XS).color(palette.text_secondary))
        .on_press(msg)
        .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
        .style(move |_theme: &Theme, _status| button::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            text_color: palette.text_secondary,
            border: Border {
                color: palette.border_regular,
                width: 0.5,
                radius: 6.0.into(),
            },
            shadow: Shadow::default(),
            snap: false,
        })
        .into()
}

fn idle_card<'a>(name: &'a str, palette: &'a ForgePalette) -> Element<'a, Message> {
    let intro = flow_intro(name, palette);
    let cta = primary_btn(
        "Connect",
        Message::DeviceCodeFlow(DeviceCodeFlowMsg::ConnectPressed),
        palette,
    );
    let body = container(cta)
        .width(Length::Fill)
        .padding(spf(Spacing::Md))
        .center_x(Length::Fill);

    container(column![intro, body])
        .width(Length::Fill)
        .style(card_style(palette))
        .into()
}

fn starting_card<'a>(name: &'a str, palette: &'a ForgePalette) -> Element<'a, Message> {
    let intro = flow_intro(name, palette);
    let body = container(
        text("Requesting authorization code…")
            .size(FONT_SM)
            .color(palette.text_muted),
    )
    .width(Length::Fill)
    .padding(spf(Spacing::Md))
    .center_x(Length::Fill);

    container(column![intro, body])
        .width(Length::Fill)
        .style(card_style(palette))
        .into()
}

fn step_circle<'a>(n: u8, active: bool, palette: &'a ForgePalette) -> Element<'a, Message> {
    let (bg, fg) = if active {
        (palette.brand, palette.shell)
    } else {
        (palette.surface_overlay, palette.text_primary)
    };
    container(
        text(n.to_string())
            .size(FONT_XS)
            .color(fg)
            .font(iced::Font {
                weight: if active {
                    iced::font::Weight::Semibold
                } else {
                    iced::font::Weight::Medium
                },
                ..iced::Font::DEFAULT
            }),
    )
    .width(24.0)
    .height(24.0)
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(bg)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 12.0.into(),
        },
        ..container::Style::default()
    })
    .into()
}

fn step_open_url<'a>(verification_url: &'a str, palette: &'a ForgePalette) -> Element<'a, Message> {
    let circle = step_circle(1, false, palette);
    let title = text("Open this URL in any browser")
        .size(FONT_SM)
        .color(palette.text_primary);

    let url_box = container(
        text(verification_url)
            .size(FONT_SM)
            .color(palette.info)
            .font(font(FontRole::Monospace)),
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

    let open_btn_content = row![
        tabler_icon(Icon::ExternalLink, 13.0, palette.brand),
        text("Open").size(FONT_SM).color(palette.brand),
    ]
    .spacing(spf(Spacing::Xxs))
    .align_y(Alignment::Center);
    let open_btn = button(open_btn_content)
        .on_press(Message::DeviceCodeFlow(
            DeviceCodeFlowMsg::OpenVerificationUrl,
        ))
        .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
        .style(move |_theme: &Theme, _status| button::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            text_color: palette.brand,
            border: Border {
                color: palette.border_regular,
                width: 0.5,
                radius: 7.0.into(),
            },
            shadow: Shadow::default(),
            snap: false,
        });

    let url_row = row![url_box, open_btn]
        .spacing(spf(Spacing::Xs))
        .align_y(Alignment::Center);

    let content = column![title, url_row].spacing(spf(Spacing::Xs));
    row![circle, content]
        .spacing(spf(Spacing::Sm))
        .align_y(Alignment::Start)
        .into()
}

fn step_enter_code<'a>(user_code: &'a str, palette: &'a ForgePalette) -> Element<'a, Message> {
    let circle = step_circle(2, true, palette);
    let title = text("Enter this code on the page")
        .size(FONT_SM)
        .color(palette.text_primary);

    let code_display = container(
        text(user_code)
            .size(28.0)
            .color(palette.brand)
            .font(font(FontRole::Monospace)),
    )
    .width(Length::Fill)
    .padding([sp(Spacing::Sm), sp(Spacing::Lg)])
    .center_x(Length::Fill)
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(palette.shell)),
        border: Border {
            color: palette.brand,
            width: 1.0,
            radius: 9.0.into(),
        },
        ..container::Style::default()
    });

    let copy_btn_content = column![
        tabler_icon(Icon::Copy, 18.0, palette.text_secondary),
        text("Copy").size(FONT_XS).color(palette.text_secondary),
    ]
    .spacing(spf(Spacing::Xxs))
    .align_x(Alignment::Center);
    let copy_btn = button(copy_btn_content)
        .on_press(Message::DeviceCodeFlow(DeviceCodeFlowMsg::CopyCode))
        .padding([sp(Spacing::Sm), sp(Spacing::Sm)])
        .style(move |_theme: &Theme, _status| button::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            text_color: palette.text_secondary,
            border: Border {
                color: palette.border_regular,
                width: 0.5,
                radius: 9.0.into(),
            },
            shadow: Shadow::default(),
            snap: false,
        });

    let code_row = row![code_display, copy_btn]
        .spacing(spf(Spacing::Xs))
        .align_y(Alignment::Center);

    let content = column![title, code_row].spacing(spf(Spacing::Xs));
    row![circle, content]
        .spacing(spf(Spacing::Sm))
        .align_y(Alignment::Start)
        .into()
}

fn polling_banner<'a>(palette: &'a ForgePalette) -> Element<'a, Message> {
    let dot = container(iced::widget::Space::new())
        .width(8.0)
        .height(8.0)
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(palette.brand)),
            border: Border {
                radius: 4.0.into(),
                color: Color::TRANSPARENT,
                width: 0.0,
            },
            ..container::Style::default()
        });

    let primary = text("Waiting for you to authorize on the platform…")
        .size(FONT_SM)
        .color(palette.text_primary);
    let secondary = text("polling every 5s")
        .size(FONT_XS)
        .color(palette.text_faint)
        .font(font(FontRole::Monospace));
    let text_col = column![primary, secondary].spacing(spf(Spacing::Xxs));

    let cancel = ghost_btn(
        "Cancel",
        Message::DeviceCodeFlow(DeviceCodeFlowMsg::CancelPressed),
        palette,
    );

    let inner = row![
        dot,
        text_col,
        iced::widget::Space::new().width(Length::Fill),
        cancel,
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(Alignment::Center);

    container(inner)
        .width(Length::Fill)
        .padding([sp(Spacing::Sm), sp(Spacing::Sm)])
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(palette.shell)),
            border: Border {
                color: palette.border_regular,
                width: 0.5,
                radius: 9.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn polling_card<'a>(
    name: &'a str,
    user_code: &'a str,
    verification_url: &'a str,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let intro = flow_intro(name, palette);
    let step1 = step_open_url(verification_url, palette);
    let step2 = step_enter_code(user_code, palette);
    let polling = polling_banner(palette);

    let body = container(column![step1, step2, polling].spacing(spf(Spacing::Sm)))
        .width(Length::Fill)
        .padding(spf(Spacing::Md));

    container(column![intro, body])
        .width(Length::Fill)
        .style(card_style(palette))
        .into()
}

fn authorized_card<'a>(name: &'a str, palette: &'a ForgePalette) -> Element<'a, Message> {
    let icon = tabler_icon(Icon::CircleCheck, 28.0, palette.success);
    let title = text(format!("Connected to {name}!"))
        .size(FONT_SM)
        .color(palette.text_primary);
    let subtitle = text("Authorization complete.")
        .size(FONT_XS)
        .color(palette.text_muted);

    let return_btn = primary_btn(
        "Return to Platforms",
        Message::DeviceCodeFlow(DeviceCodeFlowMsg::CancelPressed),
        palette,
    );

    let body = container(
        column![icon, title, subtitle, return_btn]
            .spacing(spf(Spacing::Sm))
            .align_x(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(spf(Spacing::Lg))
    .center_x(Length::Fill);

    container(body)
        .width(Length::Fill)
        .style(card_style(palette))
        .into()
}

fn failed_card<'a>(error: &'a str, palette: &'a ForgePalette) -> Element<'a, Message> {
    let icon = tabler_icon(Icon::AlertTriangle, 20.0, palette.random);
    let title = text("Authorization failed")
        .size(FONT_SM)
        .color(palette.text_primary);
    let error_text = text(error)
        .size(FONT_XS)
        .color(palette.random)
        .wrapping(iced::widget::text::Wrapping::Word);

    let retry = primary_btn(
        "Retry",
        Message::DeviceCodeFlow(DeviceCodeFlowMsg::RetryPressed),
        palette,
    );
    let cancel = ghost_btn(
        "Cancel",
        Message::DeviceCodeFlow(DeviceCodeFlowMsg::CancelPressed),
        palette,
    );
    let btn_row = row![retry, cancel]
        .spacing(spf(Spacing::Xs))
        .align_y(Alignment::Center);

    let inner = column![
        row![icon, title]
            .spacing(spf(Spacing::Xs))
            .align_y(Alignment::Center),
        error_text,
        btn_row,
    ]
    .spacing(spf(Spacing::Sm));

    container(inner)
        .width(Length::Fill)
        .padding(spf(Spacing::Md))
        .style(card_style(palette))
        .into()
}

pub fn view<'a>(state: &'a DeviceCodeFlowState, palette: &'a ForgePalette) -> Element<'a, Message> {
    let name = platform_display_name(state.platform);
    let dot_color = platform_dot_color(state.platform, palette);

    let page_header = crate::page_chrome::simple_page_header(
        &[("Platforms", false), (name, false), ("Connect", true)],
        palette,
    );

    let header_card = platform_header_card(name, dot_color, palette);

    let phase_card = match &state.phase {
        DeviceCodeFlowPhase::Idle => idle_card(name, palette),
        DeviceCodeFlowPhase::Starting => starting_card(name, palette),
        DeviceCodeFlowPhase::Polling => {
            let code = state.user_code.as_deref().unwrap_or("");
            let url = state.verification_url.as_deref().unwrap_or("");
            polling_card(name, code, url, palette)
        }
        DeviceCodeFlowPhase::Authorized => authorized_card(name, palette),
        DeviceCodeFlowPhase::Failed => {
            let err = state.error.as_deref().unwrap_or("Unknown error");
            failed_card(err, palette)
        }
    };

    let body = container(column![header_card, phase_card].spacing(spf(Spacing::Sm)))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding([sp(Spacing::Md), sp(Spacing::Lg)]);

    column![page_header, body]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn make_rt() -> RuntimeView {
        use crate::server_subsystem::ServerSubsystem;
        use forge_runtime::{EventBus, NullEventLogRepo, ScriptRegistry};
        use forge_storage::CredentialsRepo;
        use forge_storage_sqlite::SqliteBackend;
        use std::sync::Arc;

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
            speak_queue: None,
            sound_player: None,
            twitch_chat_handle: None,
            chat_send_bridge: None,
            twitch_flow: None,
            twitch_login: None,
            twitch_token_expires: None,
            twitch_reauth_required: false,
            sub_action_registry: Arc::new(forge_registry::SubActionRegistry::new()),
            trigger_registry: Arc::new(forge_registry::TriggerRegistry::new()),
        }
    }

    fn idle_state() -> DeviceCodeFlowState {
        DeviceCodeFlowState {
            platform: PlatformId::YouTube,
            ..Default::default()
        }
    }

    #[test]
    fn connect_pressed_transitions_to_starting() {
        let rt = make_rt();
        let mut state = idle_state();
        let _ = update(&mut state, &rt, DeviceCodeFlowMsg::ConnectPressed);
        assert_eq!(state.phase, DeviceCodeFlowPhase::Starting);
    }

    #[test]
    fn start_result_ok_transitions_to_polling() {
        let rt = make_rt();
        let mut state = idle_state();
        state.phase = DeviceCodeFlowPhase::Starting;
        let data = DeviceCodeData {
            user_code: "ABCD-1234".to_owned(),
            verification_url: "https://google.com/device".to_owned(),
            device_code: "device_code_xyz".to_owned(),
            interval_secs: 5,
        };
        let _ = update(&mut state, &rt, DeviceCodeFlowMsg::StartResult(Ok(data)));
        assert_eq!(state.phase, DeviceCodeFlowPhase::Polling);
        assert_eq!(state.user_code.as_deref(), Some("ABCD-1234"));
        assert_eq!(
            state.verification_url.as_deref(),
            Some("https://google.com/device")
        );
    }

    #[test]
    fn start_result_err_transitions_to_failed() {
        let rt = make_rt();
        let mut state = idle_state();
        state.phase = DeviceCodeFlowPhase::Starting;
        let _ = update(
            &mut state,
            &rt,
            DeviceCodeFlowMsg::StartResult(Err("network error".to_owned())),
        );
        assert_eq!(state.phase, DeviceCodeFlowPhase::Failed);
        assert_eq!(state.error.as_deref(), Some("network error"));
    }

    #[test]
    fn wait_result_ok_transitions_to_authorized() {
        let rt = make_rt();
        let mut state = idle_state();
        state.phase = DeviceCodeFlowPhase::Polling;
        let _ = update(&mut state, &rt, DeviceCodeFlowMsg::WaitResult(Ok(())));
        assert_eq!(state.phase, DeviceCodeFlowPhase::Authorized);
    }

    #[test]
    fn wait_result_err_transitions_to_failed() {
        let rt = make_rt();
        let mut state = idle_state();
        state.phase = DeviceCodeFlowPhase::Polling;
        let _ = update(
            &mut state,
            &rt,
            DeviceCodeFlowMsg::WaitResult(Err("access_denied".to_owned())),
        );
        assert_eq!(state.phase, DeviceCodeFlowPhase::Failed);
        assert_eq!(state.error.as_deref(), Some("access_denied"));
    }

    #[test]
    fn retry_pressed_resets_to_idle() {
        let rt = make_rt();
        let mut state = DeviceCodeFlowState {
            platform: PlatformId::YouTube,
            phase: DeviceCodeFlowPhase::Failed,
            user_code: Some("XXXX".to_owned()),
            verification_url: Some("https://example.com".to_owned()),
            device_code: Some("dc".to_owned()),
            interval_secs: Some(5),
            error: Some("auth failed".to_owned()),
        };
        let _ = update(&mut state, &rt, DeviceCodeFlowMsg::RetryPressed);
        assert_eq!(state.phase, DeviceCodeFlowPhase::Idle);
        assert!(state.user_code.is_none());
        assert!(state.verification_url.is_none());
        assert!(state.device_code.is_none());
        assert!(state.error.is_none());
    }

    #[test]
    fn cancel_pressed_returns_navigate_task() {
        let rt = make_rt();
        let mut state = idle_state();
        let task = update(&mut state, &rt, DeviceCodeFlowMsg::CancelPressed);
        assert_eq!(state.phase, DeviceCodeFlowPhase::Idle);
        drop(task);
    }
}
