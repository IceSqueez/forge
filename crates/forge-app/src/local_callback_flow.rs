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
pub enum LocalCallbackFlowPhase {
    Idle,
    Starting,
    Waiting,
    Authorized,
    Failed,
}

#[derive(Debug, Clone)]
pub struct LocalCallbackData {
    pub auth_url: String,
}

#[derive(Debug, Clone)]
pub enum LocalCallbackFlowMsg {
    ConnectPressed,
    StartResult(Result<LocalCallbackData, String>),
    WaitResult(Result<(), String>),
    RetryPressed,
    CancelPressed,
    OpenAuthUrl,
}

#[derive(Debug)]
pub struct LocalCallbackFlowState {
    pub platform: PlatformId,
    pub phase: LocalCallbackFlowPhase,
    pub auth_url: Option<String>,
    pub error: Option<String>,
}

impl Default for LocalCallbackFlowState {
    fn default() -> Self {
        Self {
            platform: PlatformId::YouTube,
            phase: LocalCallbackFlowPhase::Idle,
            auth_url: None,
            error: None,
        }
    }
}

pub fn update(
    state: &mut LocalCallbackFlowState,
    rt: &mut RuntimeView,
    msg: LocalCallbackFlowMsg,
) -> Task<Message> {
    match msg {
        LocalCallbackFlowMsg::ConnectPressed => {
            state.phase = LocalCallbackFlowPhase::Starting;
            let platform = state.platform;
            match platform {
                PlatformId::YouTube => {
                    let Some((cid, csec)) = forge_platform_youtube::client_credentials() else {
                        state.phase = LocalCallbackFlowPhase::Failed;
                        state.error =
                            Some("YouTube OAuth client credentials are not configured".to_owned());
                        return Task::none();
                    };
                    let handle = Arc::new(tokio::sync::Mutex::new(Some(
                        forge_platform_youtube::GoogleAuthFlow::new(cid, csec),
                    )));
                    rt.youtube_flow = Some(Arc::clone(&handle));
                    Task::perform(async move { start_youtube_oauth(handle).await }, |r| {
                        Message::LocalCallbackFlow(LocalCallbackFlowMsg::StartResult(r))
                    })
                }
                PlatformId::Trovo => {
                    let Some((cid, csec)) = forge_platform_trovo::client_credentials() else {
                        state.phase = LocalCallbackFlowPhase::Failed;
                        state.error =
                            Some("Trovo OAuth client credentials are not configured".to_owned());
                        return Task::none();
                    };
                    let handle = Arc::new(tokio::sync::Mutex::new(Some(
                        forge_platform_trovo::TrovoAuthFlow::new(cid, csec),
                    )));
                    rt.trovo_flow = Some(Arc::clone(&handle));
                    Task::perform(async move { start_trovo_oauth(handle).await }, |r| {
                        Message::LocalCallbackFlow(LocalCallbackFlowMsg::StartResult(r))
                    })
                }
                PlatformId::Twitch | PlatformId::Kick => {
                    state.phase = LocalCallbackFlowPhase::Failed;
                    state.error = Some(format!(
                        "{} is not wired through LocalCallbackFlow",
                        platform_display_name(platform)
                    ));
                    Task::none()
                }
            }
        }
        LocalCallbackFlowMsg::StartResult(Ok(data)) => {
            let auth_url = data.auth_url.clone();
            state.auth_url = Some(data.auth_url);
            state.phase = LocalCallbackFlowPhase::Waiting;
            let platform = state.platform;
            let credentials_repo: Arc<dyn CredentialsRepo> =
                Arc::clone(&rt.backend) as Arc<dyn CredentialsRepo>;
            let wait_task = match platform {
                PlatformId::YouTube => {
                    let Some(flow_handle) = rt.youtube_flow.clone() else {
                        state.phase = LocalCallbackFlowPhase::Failed;
                        state.error = Some("no active YouTube flow handle".to_owned());
                        return Task::none();
                    };
                    Task::perform(
                        async move {
                            wait_for_youtube_authorization(flow_handle, credentials_repo).await
                        },
                        |r| Message::LocalCallbackFlow(LocalCallbackFlowMsg::WaitResult(r)),
                    )
                }
                PlatformId::Trovo => {
                    let Some(flow_handle) = rt.trovo_flow.clone() else {
                        state.phase = LocalCallbackFlowPhase::Failed;
                        state.error = Some("no active Trovo flow handle".to_owned());
                        return Task::none();
                    };
                    Task::perform(
                        async move { wait_for_trovo_authorization(flow_handle, credentials_repo).await },
                        |r| Message::LocalCallbackFlow(LocalCallbackFlowMsg::WaitResult(r)),
                    )
                }
                PlatformId::Twitch | PlatformId::Kick => {
                    state.phase = LocalCallbackFlowPhase::Failed;
                    state.error = Some(format!(
                        "{} is not wired through LocalCallbackFlow",
                        platform_display_name(platform)
                    ));
                    return Task::none();
                }
            };
            let open_task = Task::perform(
                async move {
                    if let Err(e) = open::that(&auth_url) {
                        tracing::warn!(error = %e, url = %auth_url, "open browser failed");
                    }
                },
                |()| Message::Noop,
            );
            Task::batch([open_task, wait_task])
        }
        LocalCallbackFlowMsg::StartResult(Err(e)) => {
            state.phase = LocalCallbackFlowPhase::Failed;
            state.error = Some(e);
            Task::none()
        }
        LocalCallbackFlowMsg::WaitResult(Ok(())) => {
            state.phase = LocalCallbackFlowPhase::Authorized;
            Task::none()
        }
        LocalCallbackFlowMsg::WaitResult(Err(e)) => {
            state.phase = LocalCallbackFlowPhase::Failed;
            state.error = Some(e);
            Task::none()
        }
        LocalCallbackFlowMsg::RetryPressed => {
            state.phase = LocalCallbackFlowPhase::Idle;
            state.auth_url = None;
            state.error = None;
            Task::none()
        }
        LocalCallbackFlowMsg::CancelPressed => Task::done(Message::Navigate(Screen::Platforms)),
        LocalCallbackFlowMsg::OpenAuthUrl => {
            if let Some(url) = state.auth_url.clone() {
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

type YoutubeFlowHandle = Arc<tokio::sync::Mutex<Option<forge_platform_youtube::GoogleAuthFlow>>>;
type TrovoFlowHandle = Arc<tokio::sync::Mutex<Option<forge_platform_trovo::TrovoAuthFlow>>>;

async fn start_youtube_oauth(flow_handle: YoutubeFlowHandle) -> Result<LocalCallbackData, String> {
    let mut guard = flow_handle.lock().await;
    let flow = guard
        .as_mut()
        .ok_or_else(|| "OAuth flow already consumed".to_owned())?;
    let code = flow.start().await.map_err(|e| e.to_string())?;
    Ok(LocalCallbackData {
        auth_url: code.auth_url,
    })
}

async fn start_trovo_oauth(flow_handle: TrovoFlowHandle) -> Result<LocalCallbackData, String> {
    let mut guard = flow_handle.lock().await;
    let flow = guard
        .as_mut()
        .ok_or_else(|| "OAuth flow already consumed".to_owned())?;
    let code = flow.start().await.map_err(|e| e.to_string())?;
    Ok(LocalCallbackData {
        auth_url: code.auth_url,
    })
}

async fn wait_for_youtube_authorization(
    flow_handle: YoutubeFlowHandle,
    credentials_repo: Arc<dyn CredentialsRepo>,
) -> Result<(), String> {
    let mut flow = {
        let mut guard = flow_handle.lock().await;
        guard
            .take()
            .ok_or_else(|| "OAuth flow already consumed".to_owned())?
    };
    let bundle = flow
        .wait_for_authorization(std::time::Duration::from_secs(300))
        .await
        .map_err(|e| e.to_string())?;
    let manager = forge_platform_youtube::YoutubeCredentialsManager::new(credentials_repo, flow);
    manager
        .save_from_bundle(bundle)
        .await
        .map_err(|e| e.to_string())
}

async fn wait_for_trovo_authorization(
    flow_handle: TrovoFlowHandle,
    credentials_repo: Arc<dyn CredentialsRepo>,
) -> Result<(), String> {
    let mut flow = {
        let mut guard = flow_handle.lock().await;
        guard
            .take()
            .ok_or_else(|| "OAuth flow already consumed".to_owned())?
    };
    // 60s listener cap per beta-2 roadmap exit criteria.
    let bundle = flow
        .wait_for_authorization(std::time::Duration::from_secs(60))
        .await
        .map_err(|e| e.to_string())?;
    let Some((cid, csec)) = forge_platform_trovo::client_credentials() else {
        return Err("Trovo OAuth client credentials are not configured".to_owned());
    };
    let manager = forge_platform_trovo::TrovoCredentialsManager::new(
        credentials_repo,
        reqwest::Client::new(),
        cid,
        csec,
    );
    manager
        .save_from_bundle(bundle)
        .await
        .map_err(|e| e.to_string())
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
        Message::LocalCallbackFlow(LocalCallbackFlowMsg::ConnectPressed),
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
        .on_press(Message::LocalCallbackFlow(
            LocalCallbackFlowMsg::OpenAuthUrl,
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

fn step_wait_for_browser<'a>(palette: &'a ForgePalette) -> Element<'a, Message> {
    let circle = step_circle(2, true, palette);
    let title = text("Approve in your browser")
        .size(FONT_SM)
        .color(palette.text_primary);
    let detail = text("forge is listening on a local port for the OAuth callback. The window will refresh once you approve.")
        .size(FONT_XS)
        .color(palette.text_muted)
        .wrapping(iced::widget::text::Wrapping::Word);

    let content = column![title, detail].spacing(spf(Spacing::Xxs));
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
        Message::LocalCallbackFlow(LocalCallbackFlowMsg::CancelPressed),
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
    auth_url: &'a str,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let intro = flow_intro(name, palette);
    let step1 = step_open_url(auth_url, palette);
    let step2 = step_wait_for_browser(palette);
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
        Message::LocalCallbackFlow(LocalCallbackFlowMsg::CancelPressed),
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
        Message::LocalCallbackFlow(LocalCallbackFlowMsg::RetryPressed),
        palette,
    );
    let cancel = ghost_btn(
        "Cancel",
        Message::LocalCallbackFlow(LocalCallbackFlowMsg::CancelPressed),
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

pub fn view<'a>(
    state: &'a LocalCallbackFlowState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let name = platform_display_name(state.platform);
    let dot_color = platform_dot_color(state.platform, palette);

    let page_header = crate::page_chrome::simple_page_header(
        &[("Platforms", false), (name, false), ("Connect", true)],
        palette,
    );

    let header_card = platform_header_card(name, dot_color, palette);

    let phase_card = match &state.phase {
        LocalCallbackFlowPhase::Idle => idle_card(name, palette),
        LocalCallbackFlowPhase::Starting => starting_card(name, palette),
        LocalCallbackFlowPhase::Waiting => {
            let url = state.auth_url.as_deref().unwrap_or("");
            polling_card(name, url, palette)
        }
        LocalCallbackFlowPhase::Authorized => authorized_card(name, palette),
        LocalCallbackFlowPhase::Failed => {
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
            youtube_flow: None,
            trovo_flow: None,
            twitch_login: None,
            twitch_token_expires: None,
            twitch_reauth_required: false,
            sub_action_registry: Arc::new(forge_registry::SubActionRegistry::new()),
            trigger_registry: Arc::new(forge_registry::TriggerRegistry::new()),
        }
    }

    fn idle_state() -> LocalCallbackFlowState {
        LocalCallbackFlowState {
            platform: PlatformId::YouTube,
            ..Default::default()
        }
    }

    #[test]
    fn start_result_ok_transitions_to_polling() {
        let mut rt = make_rt();
        // ConnectPressed normally populates rt.youtube_flow; simulate it so the
        // StartResult handler doesn't bail with "no active flow handle".
        rt.youtube_flow = Some(std::sync::Arc::new(tokio::sync::Mutex::new(None)));
        let mut state = idle_state();
        state.phase = LocalCallbackFlowPhase::Starting;
        let data = LocalCallbackData {
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth?code_challenge=abc".to_owned(),
        };
        let _ = update(
            &mut state,
            &mut rt,
            LocalCallbackFlowMsg::StartResult(Ok(data)),
        );
        assert_eq!(state.phase, LocalCallbackFlowPhase::Waiting);
        assert_eq!(
            state.auth_url.as_deref(),
            Some("https://accounts.google.com/o/oauth2/v2/auth?code_challenge=abc"),
        );
    }

    #[test]
    fn start_result_err_transitions_to_failed() {
        let mut rt = make_rt();
        let mut state = idle_state();
        state.phase = LocalCallbackFlowPhase::Starting;
        let _ = update(
            &mut state,
            &mut rt,
            LocalCallbackFlowMsg::StartResult(Err("network error".to_owned())),
        );
        assert_eq!(state.phase, LocalCallbackFlowPhase::Failed);
        assert_eq!(state.error.as_deref(), Some("network error"));
    }

    #[test]
    fn wait_result_ok_transitions_to_authorized() {
        let mut rt = make_rt();
        let mut state = idle_state();
        state.phase = LocalCallbackFlowPhase::Waiting;
        let _ = update(
            &mut state,
            &mut rt,
            LocalCallbackFlowMsg::WaitResult(Ok(())),
        );
        assert_eq!(state.phase, LocalCallbackFlowPhase::Authorized);
    }

    #[test]
    fn wait_result_err_transitions_to_failed() {
        let mut rt = make_rt();
        let mut state = idle_state();
        state.phase = LocalCallbackFlowPhase::Waiting;
        let _ = update(
            &mut state,
            &mut rt,
            LocalCallbackFlowMsg::WaitResult(Err("access_denied".to_owned())),
        );
        assert_eq!(state.phase, LocalCallbackFlowPhase::Failed);
        assert_eq!(state.error.as_deref(), Some("access_denied"));
    }

    #[test]
    fn retry_pressed_resets_to_idle() {
        let mut rt = make_rt();
        let mut state = LocalCallbackFlowState {
            platform: PlatformId::YouTube,
            phase: LocalCallbackFlowPhase::Failed,
            auth_url: Some("https://accounts.google.com/o/oauth2/v2/auth?...".to_owned()),
            error: Some("auth failed".to_owned()),
        };
        let _ = update(&mut state, &mut rt, LocalCallbackFlowMsg::RetryPressed);
        assert_eq!(state.phase, LocalCallbackFlowPhase::Idle);
        assert!(state.auth_url.is_none());
        assert!(state.error.is_none());
    }

    #[test]
    fn cancel_pressed_returns_navigate_task() {
        let mut rt = make_rt();
        let mut state = idle_state();
        let task = update(&mut state, &mut rt, LocalCallbackFlowMsg::CancelPressed);
        assert_eq!(state.phase, LocalCallbackFlowPhase::Idle);
        drop(task);
    }
}
