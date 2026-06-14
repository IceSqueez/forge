use std::sync::Arc;
use std::time::{Duration, SystemTime};

use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow, Task, Theme};

use forge_platform_core::{
    BuiltinContent, BuiltinHealth, BuiltinId, BuiltinStatus, QuickActions, SectionIcon,
};
use forge_platform_twitch::{
    TWITCH_BROADCASTER_SCOPES, TwitchAuthFlow, TwitchIntegrationBundle, UserInfo,
};
use forge_storage::{CredentialId, CredentialsRepo};
use forge_types::OAuthToken;
use forge_widgets::ForgePalette;
use forge_widgets::icons::{Icon, tabler_icon};
use forge_widgets::tokens::{FONT_SM, FONT_XS, FontRole, Spacing, font, sp, spf};
use tokio::sync::Mutex as TokioMutex;

use crate::Message;
use crate::builtin_detail::BuiltinDetailState;
use crate::runtime_view::RuntimeView;

/// Shared handle to an in-progress device code flow. Wrapped in a tokio Mutex
/// so `request_code` (which calls `TwitchAuthFlow::start`) and `wait_for_auth`
/// (which calls `wait_for_authorization`) operate on the same builder state.
pub type TwitchFlowHandle = Arc<TokioMutex<TwitchAuthFlow>>;

#[derive(Debug, Clone)]
pub struct LoopbackData {
    pub auth_url: String,
    pub expires_at: SystemTime,
}

#[derive(Debug, Clone)]
pub struct TwitchAuthOutcome {
    pub token: OAuthToken,
    pub user_info: UserInfo,
    pub client_id: String,
}

const AUTH_TIMEOUT: Duration = Duration::from_secs(300);

pub async fn request_code(flow: TwitchFlowHandle) -> Result<LoopbackData, String> {
    let mut guard = flow.lock().await;
    let code = guard.start().await.map_err(|e| e.to_string())?;
    Ok(LoopbackData {
        auth_url: code.auth_url,
        expires_at: SystemTime::now() + AUTH_TIMEOUT,
    })
}

pub async fn wait_for_auth(
    flow: TwitchFlowHandle,
    credentials: Arc<dyn CredentialsRepo>,
) -> Result<TwitchAuthOutcome, String> {
    let bundle = {
        let mut guard = flow.lock().await;
        guard
            .wait_for_authorization(AUTH_TIMEOUT)
            .await
            .map_err(|e| e.to_string())?
    };
    forge_platform_twitch::credentials::store(&*credentials, &bundle)
        .await
        .map_err(|e| e.to_string())?;
    Ok(TwitchAuthOutcome {
        token: bundle.access_token,
        user_info: bundle.user_info,
        client_id: bundle.client_id,
    })
}

pub fn update(
    state: &mut TwitchPanelState,
    builtin_detail: &mut Option<BuiltinDetailState>,
    rt: &mut RuntimeView,
    msg: TwitchPanelMsg,
) -> Task<Message> {
    match msg {
        TwitchPanelMsg::StartConnect => {
            let Some(cid) = forge_platform_twitch::client_id() else {
                *state = TwitchPanelState::MissingClientId;
                return Task::none();
            };
            *state = TwitchPanelState::Requesting;
            let flow = Arc::new(TokioMutex::new(TwitchAuthFlow::new(cid)));
            rt.twitch_flow = Some(Arc::clone(&flow));
            Task::perform(request_code(flow), |r| {
                Message::TwitchPanel(TwitchPanelMsg::DeviceCodeReceived(r))
            })
        }
        TwitchPanelMsg::Cancel => {
            *state = TwitchPanelState::Disconnected;
            Task::none()
        }
        TwitchPanelMsg::OpenAuthUrl => {
            if let TwitchPanelState::AwaitingAuthorization { auth_url, .. } = &*state {
                let uri = auth_url.clone();
                Task::perform(
                    async move {
                        if let Err(e) = open::that(&uri) {
                            tracing::warn!(error = %e, url = %uri, "open browser failed");
                        }
                    },
                    |()| Message::Noop,
                )
            } else {
                Task::none()
            }
        }
        TwitchPanelMsg::DeviceCodeReceived(Ok(data)) => {
            let auth_url = data.auth_url.clone();
            *state = TwitchPanelState::AwaitingAuthorization {
                auth_url: data.auth_url,
                expires_at: data.expires_at,
            };
            let Some(flow) = rt.twitch_flow.clone() else {
                *state = TwitchPanelState::Error("no active flow handle".into());
                return Task::none();
            };
            let creds: Arc<dyn CredentialsRepo> =
                Arc::clone(&rt.backend) as Arc<dyn CredentialsRepo>;
            let wait_task = Task::perform(wait_for_auth(flow, creds), |r| {
                Message::TwitchPanel(TwitchPanelMsg::AuthCompleted(r))
            });
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
        TwitchPanelMsg::DeviceCodeReceived(Err(e)) => {
            tracing::warn!(error = %e, "twitch authorization start failed");
            *state = TwitchPanelState::Error(e);
            Task::none()
        }
        TwitchPanelMsg::AuthCompleted(Ok(outcome)) => {
            tracing::info!(
                login = %outcome.user_info.login,
                id = %outcome.user_info.id,
                "twitch authorization complete",
            );
            let login = Some(outcome.user_info.login.clone());
            rt.twitch_login = login.clone();
            let tracker = forge_platform_twitch::SubscriptionTracker::default();
            let chat = forge_platform_twitch::TwitchChat::new(
                outcome.token,
                outcome.client_id,
                outcome.user_info.id.clone(),
                outcome.user_info.id,
                Arc::clone(&rt.bus),
                Arc::clone(&tracker),
            );
            let handle = chat.start();
            let state_rx = handle.state_receiver();
            let (twitch_bundle, _health_tx) =
                TwitchIntegrationBundle::new(login, state_rx, tracker);
            let id = BuiltinId::new("twitch");
            let icon = SectionIcon::new("brand-twitch");
            let status: Arc<dyn BuiltinStatus> = twitch_bundle.clone();
            let health: Arc<dyn BuiltinHealth> = twitch_bundle.clone();
            let content: Arc<dyn BuiltinContent> = twitch_bundle.clone();
            let quick_actions: Arc<dyn QuickActions> = twitch_bundle.clone();
            *builtin_detail = Some(BuiltinDetailState::new(
                id,
                icon,
                status,
                health,
                content,
                quick_actions,
            ));
            rt.twitch_chat_handle = Some(handle);
            *state = TwitchPanelState::Disconnected;
            Task::none()
        }
        TwitchPanelMsg::AuthCompleted(Err(e)) => {
            tracing::warn!(error = %e, "twitch authorization failed");
            *state = TwitchPanelState::Error(e);
            Task::none()
        }
        TwitchPanelMsg::ReauthRequested => {
            if let Some(handle) = rt.twitch_chat_handle.take() {
                handle.shutdown();
            }
            *builtin_detail = None;
            rt.twitch_login = None;
            rt.twitch_reauth_required = false;
            let backend = Arc::clone(&rt.backend);
            Task::perform(
                async move {
                    let id = CredentialId::new("twitch:broadcaster");
                    let creds: &dyn CredentialsRepo = &*backend;
                    let _ = creds.delete(&id).await;
                },
                |()| Message::Noop,
            )
        }
    }
}

#[derive(Debug, Clone, Default)]
pub enum TwitchPanelState {
    #[default]
    Disconnected,
    Requesting,
    AwaitingAuthorization {
        auth_url: String,
        expires_at: SystemTime,
    },
    Authorizing,
    Error(String),
    MissingClientId,
}

#[derive(Debug, Clone)]
pub enum TwitchPanelMsg {
    StartConnect,
    Cancel,
    OpenAuthUrl,
    DeviceCodeReceived(Result<LoopbackData, String>),
    AuthCompleted(Result<TwitchAuthOutcome, String>),
    ReauthRequested,
}

pub fn twitch_reauth_banner<'a>(palette: &'a ForgePalette) -> Element<'a, Message> {
    let icon = tabler_icon(Icon::AlertTriangle, 14.0, palette.warning);
    let title = text(forge_widgets::tr!("twitch.reauth.title"))
        .size(FONT_SM)
        .color(palette.text_primary);
    let detail = text(forge_widgets::tr!("twitch.reauth.detail"))
        .size(FONT_XS)
        .color(palette.text_muted)
        .wrapping(iced::widget::text::Wrapping::Word);
    let text_col = column![title, detail].spacing(spf(Spacing::Xxs));

    let cta = button(
        text(forge_widgets::tr!("twitch.reauth.btn"))
            .size(FONT_XS)
            .color(palette.shell),
    )
    .on_press(Message::TwitchPanel(TwitchPanelMsg::ReauthRequested))
    .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
    .style(move |_theme: &Theme, _status| button::Style {
        background: Some(Background::Color(palette.warning)),
        text_color: palette.shell,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 6.0.into(),
        },
        shadow: Shadow::default(),
        snap: false,
    });

    let inner = row![
        icon,
        text_col,
        iced::widget::Space::new().width(Length::Fill),
        cta,
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(Alignment::Center);

    container(inner)
        .width(Length::Fill)
        .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(palette.shell)),
            border: Border {
                color: palette.warning,
                width: 0.5,
                radius: 9.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

pub fn twitch_disconnected_view<'a>(
    state: &'a TwitchPanelState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let header_card = twitch_header_card(palette);
    let flow_card = match state {
        TwitchPanelState::Disconnected => disconnected_idle_card(palette),
        TwitchPanelState::Requesting => requesting_card(palette),
        TwitchPanelState::AwaitingAuthorization {
            auth_url,
            expires_at,
        } => awaiting_card(auth_url, *expires_at, palette),
        TwitchPanelState::Authorizing => authorizing_card(palette),
        TwitchPanelState::Error(msg) => error_card(msg, palette),
        TwitchPanelState::MissingClientId => missing_client_id_card(palette),
    };
    let scopes_card = scopes_preview_card(palette);

    let page_header = crate::page_chrome::simple_page_header(
        &[
            (forge_widgets::tr!("twitch.breadcrumb.platforms"), false),
            ("Twitch".to_owned(), true),
        ],
        palette,
    );

    let body = container(column![header_card, flow_card, scopes_card].spacing(spf(Spacing::Sm)))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding([sp(Spacing::Md), sp(Spacing::Lg)]);

    column![page_header, body]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn twitch_header_card<'a>(palette: &'a ForgePalette) -> Element<'a, Message> {
    let brand = container(text("T").size(24.0).color(palette.shell).font(iced::Font {
        weight: iced::font::Weight::Semibold,
        ..iced::Font::DEFAULT
    }))
    .width(48.0)
    .height(48.0)
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(palette.brand)),
        border: Border {
            radius: 11.0.into(),
            color: Color::TRANSPARENT,
            width: 0.0,
        },
        ..container::Style::default()
    });

    let title_col = column![
        text("Twitch").size(FONT_SM).color(palette.text_primary),
        text(forge_widgets::tr!("twitch.header.subtitle"))
            .size(FONT_SM)
            .color(palette.text_muted),
    ]
    .spacing(spf(Spacing::Xxs));

    let inner = row![brand, container(title_col).width(Length::Fill)]
        .spacing(spf(Spacing::Md))
        .align_y(Alignment::Center);

    container(inner)
        .width(Length::Fill)
        .padding([sp(Spacing::Md), sp(Spacing::Md)])
        .style(card_style(palette))
        .into()
}

fn flow_intro<'a>(palette: &'a ForgePalette) -> Element<'a, Message> {
    let title_row = row![
        tabler_icon(Icon::Lock, 14.0, palette.brand),
        text(forge_widgets::tr!("twitch.auth.title"))
            .size(FONT_SM)
            .color(palette.text_primary),
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(Alignment::Center);

    let subtitle = text(forge_widgets::tr!("twitch.auth.subtitle"))
        .size(FONT_XS)
        .color(palette.text_muted)
        .wrapping(iced::widget::text::Wrapping::Word);

    container(column![title_row, subtitle].spacing(spf(Spacing::Xxs)))
        .width(Length::Fill)
        .padding([sp(Spacing::Sm), sp(Spacing::Md)])
        .style(move |_theme: &Theme| container::Style {
            border: Border {
                color: palette.border_regular,
                width: 0.5,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn disconnected_idle_card<'a>(palette: &'a ForgePalette) -> Element<'a, Message> {
    let intro = flow_intro(palette);
    let cta = primary_button(
        forge_widgets::tr!("twitch.btn.start"),
        Message::TwitchPanel(TwitchPanelMsg::StartConnect),
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

fn requesting_card<'a>(palette: &'a ForgePalette) -> Element<'a, Message> {
    let intro = flow_intro(palette);
    let body = container(
        text(forge_widgets::tr!("twitch.requesting"))
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

fn awaiting_card<'a>(
    auth_url: &'a str,
    expires_at: SystemTime,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let intro = flow_intro(palette);

    let step1 = step_open_url(auth_url, palette);
    let step2 = step_wait_for_browser(expires_at, palette);
    let polling = polling_banner(palette);

    let body = container(column![step1, step2, polling].spacing(spf(Spacing::Sm)))
        .width(Length::Fill)
        .padding(spf(Spacing::Md));

    container(column![intro, body])
        .width(Length::Fill)
        .style(card_style(palette))
        .into()
}

fn step_open_url<'a>(uri: &'a str, palette: &'a ForgePalette) -> Element<'a, Message> {
    let circle = step_circle(1, false, palette);
    let title = text(forge_widgets::tr!("twitch.step1.title"))
        .size(FONT_SM)
        .color(palette.text_primary);
    let url_box = container(
        text(uri)
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
        text(forge_widgets::tr!("twitch.btn.open"))
            .size(FONT_SM)
            .color(palette.brand),
    ]
    .spacing(spf(Spacing::Xxs))
    .align_y(Alignment::Center);
    let open_btn = button(open_btn_content)
        .on_press(Message::TwitchPanel(TwitchPanelMsg::OpenAuthUrl))
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

fn step_wait_for_browser<'a>(
    expires_at: SystemTime,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let circle = step_circle(2, true, palette);
    let title = text(forge_widgets::tr!("twitch.step2.title"))
        .size(FONT_SM)
        .color(palette.text_primary);
    let detail = text(forge_widgets::tr!("twitch.step2.detail"))
        .size(FONT_XS)
        .color(palette.text_muted)
        .wrapping(iced::widget::text::Wrapping::Word);

    let remaining = expires_at
        .duration_since(SystemTime::now())
        .unwrap_or_default();
    let timer_label = format_mm_ss(remaining);
    let timer_prefix = forge_widgets::tr!("twitch.timer.prefix");
    let timer_row = row![
        tabler_icon(Icon::Clock, 13.0, palette.text_muted),
        text(format!("{timer_prefix} "))
            .size(FONT_XS)
            .color(palette.text_muted),
        text(timer_label)
            .size(FONT_XS)
            .color(palette.text_primary)
            .font(font(FontRole::Monospace)),
        text("·").size(FONT_XS).color(palette.text_faint),
        button(
            row![
                tabler_icon(Icon::Refresh, 12.0, palette.brand),
                text(forge_widgets::tr!("twitch.btn.restart"))
                    .size(FONT_XS)
                    .color(palette.brand),
            ]
            .spacing(spf(Spacing::Xxs))
            .align_y(Alignment::Center),
        )
        .on_press(Message::TwitchPanel(TwitchPanelMsg::StartConnect))
        .padding([sp(Spacing::Xxs), 0])
        .style(move |_theme: &Theme, _status| button::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            text_color: palette.brand,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 0.0.into(),
            },
            shadow: Shadow::default(),
            snap: false,
        }),
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(Alignment::Center);

    let content = column![title, detail, timer_row].spacing(spf(Spacing::Xs));

    row![circle, content]
        .spacing(spf(Spacing::Sm))
        .align_y(Alignment::Start)
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

    let primary = text(forge_widgets::tr!("twitch.polling.primary"))
        .size(FONT_SM)
        .color(palette.text_primary);
    let secondary = text(forge_widgets::tr!("twitch.polling.secondary"))
        .size(FONT_XS)
        .color(palette.text_faint)
        .font(font(FontRole::Monospace));

    let text_col = column![primary, secondary].spacing(spf(Spacing::Xxs));

    let cancel = button(
        text(forge_widgets::tr!("twitch.btn.cancel"))
            .size(FONT_XS)
            .color(palette.text_secondary),
    )
    .on_press(Message::TwitchPanel(TwitchPanelMsg::Cancel))
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
    });

    let inner = row![
        dot,
        text_col,
        iced::widget::Space::new().width(Length::Fill),
        cancel
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

fn authorizing_card<'a>(palette: &'a ForgePalette) -> Element<'a, Message> {
    let intro = flow_intro(palette);
    let body = container(
        text(forge_widgets::tr!("twitch.authorizing"))
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

fn error_card<'a>(msg: &'a str, palette: &'a ForgePalette) -> Element<'a, Message> {
    let intro = flow_intro(palette);
    let detail = text(msg)
        .size(FONT_XS)
        .color(palette.random)
        .wrapping(iced::widget::text::Wrapping::Word);
    let retry = primary_button(
        forge_widgets::tr!("twitch.btn.try_again"),
        Message::TwitchPanel(TwitchPanelMsg::StartConnect),
        palette,
    );
    let body = container(
        column![detail, retry]
            .spacing(spf(Spacing::Sm))
            .align_x(Alignment::Start),
    )
    .width(Length::Fill)
    .padding(spf(Spacing::Md));

    container(column![intro, body])
        .width(Length::Fill)
        .style(card_style(palette))
        .into()
}

fn missing_client_id_card<'a>(palette: &'a ForgePalette) -> Element<'a, Message> {
    let intro = flow_intro(palette);
    let detail = text(forge_widgets::tr!("twitch.missing.client_id"))
        .size(FONT_XS)
        .color(palette.text_muted)
        .wrapping(iced::widget::text::Wrapping::Word);
    let body = container(detail)
        .width(Length::Fill)
        .padding(spf(Spacing::Md));

    container(column![intro, body])
        .width(Length::Fill)
        .style(card_style(palette))
        .into()
}

fn scopes_preview_card<'a>(palette: &'a ForgePalette) -> Element<'a, Message> {
    let header_left = row![
        tabler_icon(Icon::CircleCheck, 13.0, palette.success),
        text(forge_widgets::tr!("twitch.scopes.header"))
            .size(FONT_SM)
            .color(palette.text_primary),
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(Alignment::Center);
    let header_right = text(forge_widgets::tr!(
        "twitch.scopes.count",
        count = TWITCH_BROADCASTER_SCOPES.len() as i64
    ))
    .size(FONT_XS)
    .color(palette.text_faint);
    let header = row![
        header_left,
        iced::widget::Space::new().width(Length::Fill),
        header_right
    ]
    .align_y(Alignment::Center);
    let header_wrap = container(header)
        .width(Length::Fill)
        .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
        .style(move |_theme: &Theme| container::Style {
            border: Border {
                color: palette.border_regular,
                width: 0.5,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        });

    let mut pills_col: iced::widget::Column<'a, Message> = column![].spacing(spf(Spacing::Xxs));
    let mut current_row: iced::widget::Row<'a, Message> =
        iced::widget::Row::new().spacing(spf(Spacing::Xxs));
    let mut row_count: usize = 0;
    const SCOPES_PER_ROW: usize = 3;
    for scope in TWITCH_BROADCASTER_SCOPES {
        current_row = current_row.push(scope_pill(scope, palette));
        row_count += 1;
        if row_count == SCOPES_PER_ROW {
            pills_col = pills_col.push(current_row);
            current_row = iced::widget::Row::new().spacing(spf(Spacing::Xxs));
            row_count = 0;
        }
    }
    if row_count > 0 {
        pills_col = pills_col.push(current_row);
    }
    let pills_wrap = container(pills_col)
        .width(Length::Fill)
        .padding([sp(Spacing::Xs), sp(Spacing::Sm)]);

    container(column![header_wrap, pills_wrap])
        .width(Length::Fill)
        .style(card_style(palette))
        .into()
}

fn scope_pill<'a>(scope: &'a str, palette: &'a ForgePalette) -> Element<'a, Message> {
    container(
        text(scope)
            .size(FONT_XS)
            .color(palette.success)
            .font(font(FontRole::Monospace)),
    )
    .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(palette.surface_overlay)),
        border: Border {
            radius: 8.0.into(),
            color: Color::TRANSPARENT,
            width: 0.0,
        },
        ..container::Style::default()
    })
    .into()
}

fn primary_button<'a>(
    label: impl Into<String>,
    msg: Message,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    button(text(label.into()).size(FONT_SM).color(palette.shell))
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

fn format_mm_ss(d: Duration) -> String {
    let secs = d.as_secs();
    format!("{}:{:02}", secs / 60, secs % 60)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn format_mm_ss_basic() {
        assert_eq!(format_mm_ss(Duration::from_secs(0)), "0:00");
        assert_eq!(format_mm_ss(Duration::from_secs(5)), "0:05");
        assert_eq!(format_mm_ss(Duration::from_secs(65)), "1:05");
        assert_eq!(format_mm_ss(Duration::from_secs(600)), "10:00");
    }

    #[test]
    fn default_state_is_disconnected() {
        assert!(matches!(
            TwitchPanelState::default(),
            TwitchPanelState::Disconnected
        ));
    }
}
