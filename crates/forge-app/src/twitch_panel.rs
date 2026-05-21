use std::sync::Arc;
use std::time::{Duration, SystemTime};

use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding, Shadow, Theme};

use forge_platform_twitch::{
    TWITCH_BROADCASTER_SCOPES, TwitchAuthBundle, TwitchAuthFlow, UserInfo,
};
use forge_storage::{CredentialId, CredentialsRepo};
use forge_types::OAuthToken;
use forge_widgets::ForgePalette;
use forge_widgets::icons::{Icon, tabler_icon};
use forge_widgets::tokens::{FONT_BODY, FONT_SM, FONT_XS, FontRole, font};
use tokio::sync::Mutex as TokioMutex;

use crate::Message;

const TWITCH_CREDENTIAL_ID: &str = "twitch:broadcaster";

/// Shared handle to an in-progress device code flow. Wrapped in a tokio Mutex
/// so `request_code` (which calls `TwitchAuthFlow::start`) and `wait_for_auth`
/// (which calls `wait_for_authorization`) operate on the same builder state.
pub type TwitchFlowHandle = Arc<TokioMutex<TwitchAuthFlow>>;

#[derive(Debug, Clone)]
pub struct DcfCodeData {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_at: SystemTime,
}

#[derive(Debug, Clone)]
pub struct TwitchAuthOutcome {
    pub token: OAuthToken,
    pub user_info: UserInfo,
    pub client_id: String,
}

pub async fn request_code(flow: TwitchFlowHandle) -> Result<DcfCodeData, String> {
    let mut guard = flow.lock().await;
    let code = guard.start().await.map_err(|e| e.to_string())?;
    let expires_at = SystemTime::now() + code.expires_in;
    Ok(DcfCodeData {
        user_code: code.user_code,
        verification_uri: code.verification_uri,
        expires_at,
    })
}

pub async fn wait_for_auth(
    flow: TwitchFlowHandle,
    credentials: Arc<dyn CredentialsRepo>,
) -> Result<TwitchAuthOutcome, String> {
    let TwitchAuthBundle {
        access_token,
        user_info,
        client_id,
        expires_at,
    } = {
        let mut guard = flow.lock().await;
        guard
            .wait_for_authorization()
            .await
            .map_err(|e| e.to_string())?
    };

    let expires_at_unix: Option<i64> = expires_at.and_then(|t| {
        t.duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs() as i64)
    });
    let bundle = serde_json::json!({
        "access_token": access_token.expose(),
        "user_id": user_info.id,
        "login": user_info.login,
        "expires_at_unix": expires_at_unix,
    });
    credentials
        .store(
            &CredentialId::new(TWITCH_CREDENTIAL_ID),
            &bundle.to_string(),
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(TwitchAuthOutcome {
        token: access_token,
        user_info,
        client_id,
    })
}

#[derive(Debug, Clone, Default)]
pub enum TwitchPanelState {
    #[default]
    Disconnected,
    Requesting,
    AwaitingAuthorization {
        user_code: String,
        verification_uri: String,
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
    CopyCode,
    OpenVerificationUrl,
    DeviceCodeReceived(Result<DcfCodeData, String>),
    AuthCompleted(Result<TwitchAuthOutcome, String>),
}

pub fn twitch_reauth_banner<'a>(palette: &'a ForgePalette) -> Element<'a, Message> {
    let icon = tabler_icon(Icon::AlertTriangle, 14.0, palette.warning);
    let title = text("Twitch token is missing required scopes")
        .size(FONT_SM)
        .color(palette.text_primary);
    let detail = text(
        "EventSub rejected the chat subscription. Re-authorize to refresh the token with all current scopes.",
    )
    .size(FONT_XS)
    .color(palette.text_muted)
    .wrapping(iced::widget::text::Wrapping::Word);
    let text_col = column![title, detail].spacing(2.0);

    let cta = button(text("Re-authorize").size(FONT_XS).color(palette.shell))
        .on_press(Message::TwitchReauthRequested)
        .padding(Padding::from([6_u16, 12_u16]))
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
    .spacing(10.0)
    .align_y(Alignment::Center);

    container(inner)
        .width(Length::Fill)
        .padding(Padding::from([10_u16, 14_u16]))
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
            user_code,
            verification_uri,
            expires_at,
        } => awaiting_card(user_code, verification_uri, *expires_at, palette),
        TwitchPanelState::Authorizing => authorizing_card(palette),
        TwitchPanelState::Error(msg) => error_card(msg, palette),
        TwitchPanelState::MissingClientId => missing_client_id_card(palette),
    };
    let scopes_card = scopes_preview_card(palette);

    container(column![header_card, flow_card, scopes_card].spacing(14.0))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding::from([18_u16, 22_u16]))
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
        text("Twitch").size(FONT_BODY).color(palette.text_primary),
        text("Connect to enable chat, subs, bits, raids, channel points, and EventSub")
            .size(FONT_SM)
            .color(palette.text_muted),
    ]
    .spacing(2.0);

    let inner = row![brand, container(title_col).width(Length::Fill)]
        .spacing(16.0)
        .align_y(Alignment::Center);

    container(inner)
        .width(Length::Fill)
        .padding(Padding::from([16_u16, 18_u16]))
        .style(card_style(palette))
        .into()
}

fn flow_intro<'a>(palette: &'a ForgePalette) -> Element<'a, Message> {
    let title_row = row![
        tabler_icon(Icon::Lock, 14.0, palette.brand),
        text("Authorize Forge on Twitch")
            .size(FONT_BODY)
            .color(palette.text_primary),
    ]
    .spacing(8.0)
    .align_y(Alignment::Center);

    let subtitle = text(
        "Twitch uses device code authorization. You'll see a code here, enter it on Twitch's site, \
         and we'll auto-detect when you're done. We never see your password.",
    )
    .size(FONT_XS)
    .color(palette.text_muted)
    .wrapping(iced::widget::text::Wrapping::Word);

    container(column![title_row, subtitle].spacing(3.0))
        .width(Length::Fill)
        .padding(Padding::from([14_u16, 18_u16]))
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
        "Start authorization",
        Message::TwitchPanel(TwitchPanelMsg::StartConnect),
        palette,
    );
    let body = container(cta)
        .width(Length::Fill)
        .padding(18.0)
        .center_x(Length::Fill);

    container(column![intro, body])
        .width(Length::Fill)
        .style(card_style(palette))
        .into()
}

fn requesting_card<'a>(palette: &'a ForgePalette) -> Element<'a, Message> {
    let intro = flow_intro(palette);
    let body = container(
        text("Requesting authorization code from Twitch…")
            .size(FONT_SM)
            .color(palette.text_muted),
    )
    .width(Length::Fill)
    .padding(18.0)
    .center_x(Length::Fill);

    container(column![intro, body])
        .width(Length::Fill)
        .style(card_style(palette))
        .into()
}

fn awaiting_card<'a>(
    user_code: &'a str,
    verification_uri: &'a str,
    expires_at: SystemTime,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let intro = flow_intro(palette);

    let step1 = step_open_url(verification_uri, palette);
    let step2 = step_enter_code(user_code, expires_at, palette);
    let polling = polling_banner(palette);

    let body = container(column![step1, step2, polling].spacing(14.0))
        .width(Length::Fill)
        .padding(18.0);

    container(column![intro, body])
        .width(Length::Fill)
        .style(card_style(palette))
        .into()
}

fn step_open_url<'a>(uri: &'a str, palette: &'a ForgePalette) -> Element<'a, Message> {
    let circle = step_circle(1, false, palette);
    let title = text("Open this URL in any browser")
        .size(FONT_SM)
        .color(palette.text_primary);
    let url_box = container(
        text(uri)
            .size(FONT_SM)
            .color(palette.info)
            .font(font(FontRole::Monospace)),
    )
    .width(Length::Fill)
    .padding(Padding::from([8_u16, 12_u16]))
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
    .spacing(5.0)
    .align_y(Alignment::Center);
    let open_btn = button(open_btn_content)
        .on_press(Message::TwitchPanel(TwitchPanelMsg::OpenVerificationUrl))
        .padding(Padding::from([7_u16, 11_u16]))
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
        .spacing(8.0)
        .align_y(Alignment::Center);

    let content = column![title, url_row].spacing(6.0);

    row![circle, content]
        .spacing(14.0)
        .align_y(Alignment::Start)
        .into()
}

fn step_enter_code<'a>(
    user_code: &'a str,
    expires_at: SystemTime,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
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
    .padding(Padding::from([14_u16, 20_u16]))
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
    .spacing(3.0)
    .align_x(Alignment::Center);
    let copy_btn = button(copy_btn_content)
        .on_press(Message::TwitchPanel(TwitchPanelMsg::CopyCode))
        .padding(Padding::from([14_u16, 12_u16]))
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
        .spacing(10.0)
        .align_y(Alignment::Center);

    let remaining = expires_at
        .duration_since(SystemTime::now())
        .unwrap_or_default();
    let timer_label = format_mm_ss(remaining);
    let timer_row = row![
        tabler_icon(Icon::Clock, 13.0, palette.text_muted),
        text("Expires in ").size(FONT_XS).color(palette.text_muted),
        text(timer_label)
            .size(FONT_XS)
            .color(palette.text_primary)
            .font(font(FontRole::Monospace)),
        text("·").size(FONT_XS).color(palette.text_faint),
        button(
            row![
                tabler_icon(Icon::Refresh, 12.0, palette.brand),
                text("Get new code").size(FONT_XS).color(palette.brand),
            ]
            .spacing(4.0)
            .align_y(Alignment::Center),
        )
        .on_press(Message::TwitchPanel(TwitchPanelMsg::StartConnect))
        .padding(Padding::from([2_u16, 0_u16]))
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
    .spacing(8.0)
    .align_y(Alignment::Center);

    let content = column![title, code_row, timer_row].spacing(10.0);

    row![circle, content]
        .spacing(14.0)
        .align_y(Alignment::Start)
        .into()
}

fn step_circle<'a>(n: u8, active: bool, palette: &'a ForgePalette) -> Element<'a, Message> {
    let (bg, fg) = if active {
        (palette.brand, palette.shell)
    } else {
        (palette.surface_overlay, palette.text_primary)
    };
    container(text(n.to_string()).size(11.0).color(fg).font(iced::Font {
        weight: if active {
            iced::font::Weight::Semibold
        } else {
            iced::font::Weight::Medium
        },
        ..iced::Font::DEFAULT
    }))
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

    let primary = text("Waiting for you to authorize on Twitch…")
        .size(FONT_SM)
        .color(palette.text_primary);
    let secondary = text("polling every 5s")
        .size(FONT_XS)
        .color(palette.text_faint)
        .font(font(FontRole::Monospace));

    let text_col = column![primary, secondary].spacing(1.0);

    let cancel = button(text("Cancel").size(FONT_XS).color(palette.text_secondary))
        .on_press(Message::TwitchPanel(TwitchPanelMsg::Cancel))
        .padding(Padding::from([5_u16, 10_u16]))
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
    .spacing(10.0)
    .align_y(Alignment::Center);

    container(inner)
        .width(Length::Fill)
        .padding(Padding::from([11_u16, 14_u16]))
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
        text("Code accepted. Finalising authorization…")
            .size(FONT_SM)
            .color(palette.text_muted),
    )
    .width(Length::Fill)
    .padding(18.0)
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
        "Try again",
        Message::TwitchPanel(TwitchPanelMsg::StartConnect),
        palette,
    );
    let body = container(
        column![detail, retry]
            .spacing(12.0)
            .align_x(Alignment::Start),
    )
    .width(Length::Fill)
    .padding(18.0);

    container(column![intro, body])
        .width(Length::Fill)
        .style(card_style(palette))
        .into()
}

fn missing_client_id_card<'a>(palette: &'a ForgePalette) -> Element<'a, Message> {
    let intro = flow_intro(palette);
    let detail = text(
        "Twitch integration is not configured. Set FORGE_TWITCH_CLIENT_ID with your own \
         registered application's client_id and restart the app.",
    )
    .size(FONT_XS)
    .color(palette.text_muted)
    .wrapping(iced::widget::text::Wrapping::Word);
    let body = container(detail).width(Length::Fill).padding(18.0);

    container(column![intro, body])
        .width(Length::Fill)
        .style(card_style(palette))
        .into()
}

fn scopes_preview_card<'a>(palette: &'a ForgePalette) -> Element<'a, Message> {
    let header_left = row![
        tabler_icon(Icon::CircleCheck, 13.0, palette.success),
        text("Permissions Forge will request")
            .size(FONT_SM)
            .color(palette.text_primary),
    ]
    .spacing(7.0)
    .align_y(Alignment::Center);
    let header_right = text(format!("{} scopes", TWITCH_BROADCASTER_SCOPES.len()))
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
        .padding(Padding::from([10_u16, 14_u16]))
        .style(move |_theme: &Theme| container::Style {
            border: Border {
                color: palette.border_regular,
                width: 0.5,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        });

    let mut pills_col: iced::widget::Column<'a, Message> = column![].spacing(5.0);
    let mut current_row: iced::widget::Row<'a, Message> = iced::widget::Row::new().spacing(5.0);
    let mut row_count: usize = 0;
    const SCOPES_PER_ROW: usize = 3;
    for scope in TWITCH_BROADCASTER_SCOPES {
        current_row = current_row.push(scope_pill(scope, palette));
        row_count += 1;
        if row_count == SCOPES_PER_ROW {
            pills_col = pills_col.push(current_row);
            current_row = iced::widget::Row::new().spacing(5.0);
            row_count = 0;
        }
    }
    if row_count > 0 {
        pills_col = pills_col.push(current_row);
    }
    let pills_wrap = container(pills_col)
        .width(Length::Fill)
        .padding(Padding::from([10_u16, 14_u16]));

    container(column![header_wrap, pills_wrap])
        .width(Length::Fill)
        .style(card_style(palette))
        .into()
}

fn scope_pill<'a>(scope: &'a str, palette: &'a ForgePalette) -> Element<'a, Message> {
    container(
        text(scope)
            .size(10.0)
            .color(palette.success)
            .font(font(FontRole::Monospace)),
    )
    .padding(Padding::from([2_u16, 7_u16]))
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
    label: &'a str,
    msg: Message,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    button(text(label).size(FONT_SM).color(palette.shell))
        .on_press(msg)
        .padding(Padding::from([8_u16, 16_u16]))
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
