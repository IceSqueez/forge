use std::sync::Arc;
use std::time::{Duration, SystemTime};

use forge_components::{
    BORDER_THIN, Density, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon, Radius, Spacing,
    body_family, icon, mono_family, radius, spacing, spinner, tr, with_alpha,
};
use forge_platform_core::{RateLimiter, TokenBucketRateLimiter};
use forge_platform_twitch::{
    DeviceCodeInfo, HELIX_BUDGET_CAPACITY, HELIX_BUDGET_WINDOW, TWITCH_BROADCASTER_SCOPES,
    TwitchAuthFlow, UserInfo,
};
use forge_storage::CredentialsRepo;
use forge_types::PlatformId;
use gpui::{
    Animation, AnimationExt, AnyElement, ClipboardItem, Context, FontWeight, HighlightStyle, Hsla,
    Rgba, SharedString, StyledText, div, prelude::*, px,
};
use tokio_util::sync::CancellationToken;

use crate::async_bridge;
use crate::integration_detail::IntegrationDetail;
use crate::screen::Screen;

pub type TwitchFlowHandle = Arc<tokio::sync::Mutex<Option<TwitchAuthFlow>>>;

const TWITCH_DEVICE_POLL_SECS: &str = "5";
const COPY_FLIP: Duration = Duration::from_millis(1400);

pub(crate) struct TwitchAuthOutcome {
    user_info: UserInfo,
    client_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TwitchDevicePhase {
    Starting,
    Waiting,
    Expired,
    Denied,
    Failed,
    Authorized,
}

pub(crate) struct DeviceCodeData {
    pub(crate) user_code: String,
    pub(crate) verification_uri: String,
    pub(crate) expires_at: SystemTime,
}

impl DeviceCodeData {
    fn remaining(&self) -> Duration {
        self.expires_at
            .duration_since(SystemTime::now())
            .unwrap_or(Duration::ZERO)
    }
}

pub(crate) struct TwitchDeviceState {
    pub(crate) phase: TwitchDevicePhase,
    pub(crate) code: Option<DeviceCodeData>,
    pub(crate) copied: bool,
    pub(crate) cancel: CancellationToken,
    pub(crate) error: Option<String>,
}

impl TwitchDeviceState {
    fn starting() -> Self {
        Self {
            phase: TwitchDevicePhase::Starting,
            code: None,
            copied: false,
            cancel: CancellationToken::new(),
            error: None,
        }
    }

    fn failed(phase: TwitchDevicePhase, error: String) -> Self {
        Self {
            phase,
            code: None,
            copied: false,
            cancel: CancellationToken::new(),
            error: Some(error),
        }
    }
}

enum TwitchWaitError {
    Cancelled,
    Denied,
    Expired,
    Other(String),
}

pub(crate) async fn request_code(flow: TwitchFlowHandle) -> Result<DeviceCodeInfo, String> {
    let mut guard = flow.lock().await;
    let inner = guard
        .as_mut()
        .ok_or_else(|| "OAuth flow already consumed".to_owned())?;
    inner.start().await.map_err(|e| e.to_string())
}

async fn wait_for_auth(
    flow: TwitchFlowHandle,
    cancel: CancellationToken,
    credentials: Arc<dyn CredentialsRepo>,
) -> Result<TwitchAuthOutcome, TwitchWaitError> {
    let mut inner = {
        let mut guard = flow.lock().await;
        guard
            .take()
            .ok_or_else(|| TwitchWaitError::Other("OAuth flow already consumed".to_owned()))?
    };
    let bundle = match inner.wait_for_authorization(cancel).await {
        Ok(bundle) => bundle,
        Err(err) => {
            let message = err.to_string();
            return Err(if message.contains("cancelled") {
                TwitchWaitError::Cancelled
            } else if message.contains("denied") {
                TwitchWaitError::Denied
            } else if message.contains("expired") {
                TwitchWaitError::Expired
            } else {
                TwitchWaitError::Other(message)
            });
        }
    };
    forge_platform_twitch::credentials::store(&*credentials, &bundle)
        .await
        .map_err(|e| TwitchWaitError::Other(e.to_string()))?;
    Ok(TwitchAuthOutcome {
        user_info: bundle.user_info,
        client_id: bundle.client_id,
    })
}

fn scopes_preview() -> String {
    TWITCH_BROADCASTER_SCOPES
        .iter()
        .take(4)
        .copied()
        .collect::<Vec<_>>()
        .join(" ")
}

impl IntegrationDetail {
    pub(crate) fn begin_twitch_device(&mut self, cx: &mut Context<Self>) {
        if let Some(dev) = &self.twitch_device {
            dev.cancel.cancel();
        }
        let id_source = if std::env::var("FORGE_TWITCH_CLIENT_ID").is_ok() {
            "runtime env"
        } else {
            "compiled default"
        };
        let Some(cid) = forge_platform_twitch::client_id() else {
            self.twitch_device = Some(TwitchDeviceState::failed(
                TwitchDevicePhase::Failed,
                tr!("auth_error_credentials_missing_twitch").to_string(),
            ));
            cx.notify();
            return;
        };
        tracing::info!(
            source = id_source,
            id_prefix = &cid[..cid.len().min(6)],
            "starting twitch device flow"
        );
        let handle: TwitchFlowHandle =
            Arc::new(tokio::sync::Mutex::new(Some(TwitchAuthFlow::new(cid))));
        self.twitch_flow = Some(Arc::clone(&handle));
        self.twitch_device = Some(TwitchDeviceState::starting());
        async_bridge::run_async(
            &self.rt_handle,
            request_code(handle),
            |this, result, cx| this.apply_twitch_device_start(result, cx),
            cx,
        );
        cx.notify();
    }

    fn apply_twitch_device_start(
        &mut self,
        result: Result<DeviceCodeInfo, String>,
        cx: &mut Context<Self>,
    ) {
        let info = match result {
            Ok(info) => info,
            Err(e) => {
                tracing::warn!(error = %e, "twitch device code request failed");
                self.twitch_device = Some(TwitchDeviceState::failed(TwitchDevicePhase::Failed, e));
                cx.notify();
                return;
            }
        };
        let Some(cancel) = self.twitch_device.as_ref().map(|d| d.cancel.clone()) else {
            return;
        };
        if let Some(dev) = &mut self.twitch_device {
            dev.phase = TwitchDevicePhase::Waiting;
            dev.error = None;
            dev.code = Some(DeviceCodeData {
                user_code: info.user_code,
                verification_uri: info.verification_uri,
                expires_at: info.expires_at,
            });
        }
        let Some(handle) = self.twitch_flow.clone() else {
            self.twitch_device = Some(TwitchDeviceState::failed(
                TwitchDevicePhase::Failed,
                "no active Twitch flow handle".to_owned(),
            ));
            cx.notify();
            return;
        };
        let credentials = Arc::clone(&self.credentials);
        async_bridge::run_async(
            &self.rt_handle,
            wait_for_auth(handle, cancel, credentials),
            |this, result, cx| this.apply_twitch_wait(result, cx),
            cx,
        );
        cx.notify();
    }

    fn apply_twitch_wait(
        &mut self,
        result: Result<TwitchAuthOutcome, TwitchWaitError>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(outcome) => {
                if let Some(dev) = &mut self.twitch_device {
                    dev.phase = TwitchDevicePhase::Authorized;
                }
                self.install_twitch_outcome(outcome, cx);
                cx.notify();
            }
            Err(TwitchWaitError::Cancelled) => {}
            Err(TwitchWaitError::Denied) => {
                if let Some(dev) = &mut self.twitch_device {
                    dev.phase = TwitchDevicePhase::Denied;
                    dev.error = Some(tr!("twitch_device_denied_detail").to_string());
                }
                cx.notify();
            }
            Err(TwitchWaitError::Expired) => {
                if let Some(dev) = &mut self.twitch_device {
                    dev.phase = TwitchDevicePhase::Expired;
                    dev.error = None;
                }
                cx.notify();
            }
            Err(TwitchWaitError::Other(msg)) => {
                tracing::warn!(error = %msg, "twitch authorization failed");
                if let Some(dev) = &mut self.twitch_device {
                    dev.phase = TwitchDevicePhase::Failed;
                    dev.error = Some(msg);
                }
                cx.notify();
            }
        }
    }

    fn install_twitch_outcome(&mut self, outcome: TwitchAuthOutcome, cx: &mut Context<Self>) {
        tracing::info!(
            login = %outcome.user_info.login,
            id = %outcome.user_info.id,
            "twitch authorization complete",
        );
        let bus = Arc::clone(&self.bus);
        let credentials = Arc::clone(&self.credentials);
        let live_viewers = self.live_viewers.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let login = Some(outcome.user_info.login.clone());
                let tracker = forge_platform_twitch::SubscriptionTracker::default();
                let config = forge_platform_twitch::ChatSessionConfig {
                    client_id: outcome.client_id,
                    broadcaster_id: outcome.user_info.id.clone(),
                    user_id: outcome.user_info.id,
                };
                let manager = Arc::new(forge_platform_twitch::TwitchCredentialsManager::new(
                    Arc::clone(&credentials),
                    config.client_id.clone(),
                ));
                let chat = forge_platform_twitch::TwitchChat::new(
                    manager,
                    config.client_id.clone(),
                    config.broadcaster_id.clone(),
                    config.user_id.clone(),
                    Arc::clone(&bus),
                    Arc::clone(&tracker),
                );
                let handle = chat.start();
                let rate_limiter: Arc<dyn RateLimiter> = Arc::new(TokenBucketRateLimiter::new(
                    HELIX_BUDGET_CAPACITY,
                    HELIX_BUDGET_WINDOW,
                ));
                let bundle = forge_platform_twitch::TwitchIntegrationBundle::new(
                    login,
                    config,
                    bus,
                    credentials,
                    tracker,
                    handle,
                    rate_limiter,
                );
                live_viewers.register(bundle.viewer_source());
                bundle
            },
            |this, bundle, cx| this.install_twitch_bundle(bundle, cx),
            cx,
        );
    }

    fn on_twitch_copy(&mut self, cx: &mut Context<Self>) {
        let Some(code) = self
            .twitch_device
            .as_ref()
            .and_then(|d| d.code.as_ref())
            .map(|c| c.user_code.clone())
        else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(code));
        if let Some(dev) = &mut self.twitch_device {
            dev.copied = true;
        }
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(COPY_FLIP).await;
            let _ = this.update(cx, |this, cx| {
                if let Some(dev) = &mut this.twitch_device {
                    dev.copied = false;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn twitch_open_url(&self, cx: &mut Context<Self>) {
        let Some(url) = self
            .twitch_device
            .as_ref()
            .and_then(|d| d.code.as_ref())
            .map(|c| c.verification_uri.clone())
        else {
            return;
        };
        self.open_url(url, cx);
    }

    fn cancel_twitch_and_leave(&mut self, cx: &mut Context<Self>) {
        if let Some(dev) = &self.twitch_device {
            dev.cancel.cancel();
        }
        self.twitch_device = None;
        self.twitch_flow = None;
        self.navigate_to(Screen::Platforms, cx);
    }

    pub(crate) fn twitch_device_status(
        &self,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
        let accent = crate::oauth_connect::twitch_accent(palette);
        let phase = self
            .twitch_device
            .as_ref()
            .map_or(TwitchDevicePhase::Starting, |d| d.phase);
        let (indicator, label, color): (AnyElement, String, Rgba) = match phase {
            TwitchDevicePhase::Starting | TwitchDevicePhase::Waiting => (
                spinner("twitch-device-status-spin", Icon::Loader2, px(11.0), accent)
                    .into_any_element(),
                tr!("oauth_status_authorizing"),
                accent,
            ),
            TwitchDevicePhase::Authorized => (
                status_dot(palette.success).into_any_element(),
                tr!("oauth_status_authorized"),
                palette.success,
            ),
            TwitchDevicePhase::Denied | TwitchDevicePhase::Failed => (
                status_dot(palette.random).into_any_element(),
                tr!("common_status_not_connected"),
                palette.random,
            ),
            TwitchDevicePhase::Expired => (
                status_dot(palette.text_faint).into_any_element(),
                tr!("common_status_not_connected"),
                palette.text_faint,
            ),
        };
        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .child(indicator)
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(color)
                    .child(label),
            )
            .into_any_element()
    }

    pub(crate) fn twitch_device_column(
        &self,
        accent: Rgba,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let eyebrow = div()
            .font_family(mono_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_muted)
            .mb(spacing(Spacing::Xs, density))
            .child(tr!("oauth_connect_eyebrow"));

        let phase = self
            .twitch_device
            .as_ref()
            .map_or(TwitchDevicePhase::Starting, |d| d.phase);
        let status_card = match phase {
            TwitchDevicePhase::Starting => self.twitch_progress_card(true, accent, palette),
            TwitchDevicePhase::Waiting => self.twitch_progress_card(false, accent, palette),
            TwitchDevicePhase::Authorized => self.twitch_done_card(palette),
            TwitchDevicePhase::Expired => self.twitch_expired_card(palette, density),
            TwitchDevicePhase::Denied => self.twitch_error_card(
                tr!("twitch_device_denied_title"),
                accent,
                palette,
                density,
                cx,
            ),
            TwitchDevicePhase::Failed => self.twitch_error_card(
                tr!("twitch_device_failed_title"),
                accent,
                palette,
                density,
                cx,
            ),
        };

        let has_code = self
            .twitch_device
            .as_ref()
            .is_some_and(|d| d.code.is_some());
        let mut column = div()
            .w_full()
            .max_w(px(640.0))
            .flex()
            .flex_col()
            .child(eyebrow)
            .child(self.twitch_explainer(palette));
        if has_code {
            column = column
                .child(self.twitch_url_card(accent, palette, cx))
                .child(self.twitch_code_card(accent, palette, cx));
        }
        column
            .child(status_card)
            .child(self.twitch_device_footer(palette, density, cx))
            .into_any_element()
    }

    fn twitch_explainer(&self, palette: &ForgePalette) -> AnyElement {
        let prefix = tr!("twitch_device_explainer_prefix");
        let emphasis = tr!("twitch_device_explainer_emphasis");
        let suffix = tr!("twitch_device_explainer_suffix");
        let start = prefix.len();
        let end = start + emphasis.len();
        let full = format!("{prefix}{emphasis}{suffix}");
        let styled = StyledText::new(SharedString::from(full)).with_highlights(vec![(
            start..end,
            HighlightStyle::from(Hsla::from(palette.text_primary)),
        )]);
        div()
            .w_full()
            .font_family(body_family())
            .text_size(FONT_XS)
            .text_color(palette.text_muted)
            .line_height(px(20.0))
            .mb(px(22.0))
            .child(styled)
            .into_any_element()
    }

    fn twitch_url_card(
        &self,
        accent: Rgba,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let url: SharedString = self
            .twitch_device
            .as_ref()
            .and_then(|d| d.code.as_ref())
            .map_or_else(
                || SharedString::from("\u{2014}"),
                |c| SharedString::from(c.verification_uri.clone()),
            );
        let url_box = div()
            .flex_1()
            .min_w(px(0.0))
            .py(px(8.0))
            .px(px(12.0))
            .rounded(px(7.0))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.shell)
            .font_family(mono_family())
            .text_size(FONT_XS)
            .text_color(palette.info)
            .child(url);
        let open_btn = div()
            .id("twitch-device-open")
            .flex_none()
            .flex()
            .items_center()
            .gap(px(5.0))
            .py(px(5.0))
            .px(px(11.0))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .cursor_pointer()
            .hover(|s| s.border_color(palette.border_active))
            .on_click(cx.listener(|this, _, _, cx| this.twitch_open_url(cx)))
            .child(icon(Icon::ExternalLink, px(13.0), accent))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(accent)
                    .child(tr!("twitch_device_open_btn")),
            );
        step_card(1, palette.surface_overlay, palette.text_primary, palette)
            .border_color(palette.border_regular)
            .mb(px(10.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .font_family(body_family())
                            .font_weight(FontWeight::MEDIUM)
                            .text_size(FONT_XS)
                            .text_color(palette.text_primary)
                            .mb(px(8.0))
                            .child(tr!("twitch_device_open_title")),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(url_box)
                            .child(open_btn),
                    ),
            )
            .into_any_element()
    }

    fn twitch_code_card(
        &self,
        accent: Rgba,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let code = self.twitch_device.as_ref().and_then(|d| d.code.as_ref());
        let copied = self.twitch_device.as_ref().is_some_and(|d| d.copied);

        let glyphs: Vec<AnyElement> = code
            .map(|c| c.user_code.chars().collect::<Vec<_>>())
            .unwrap_or_else(|| vec!['\u{2014}'])
            .into_iter()
            .map(|ch| {
                div()
                    .font_family(mono_family())
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(28.0))
                    .text_color(accent)
                    .child(ch.to_string())
                    .into_any_element()
            })
            .collect();
        let code_box = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .py(px(14.0))
            .px(px(20.0))
            .rounded(px(9.0))
            .border(BORDER_THIN)
            .border_color(accent)
            .bg(palette.shell)
            .children(glyphs);

        let copy_color = if copied {
            palette.success
        } else {
            palette.text_secondary
        };
        let copy_btn = div()
            .id("twitch-device-copy")
            .flex_none()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(3.0))
            .p(px(14.0))
            .rounded(px(9.0))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .cursor_pointer()
            .hover(|s| s.border_color(palette.border_active))
            .on_click(cx.listener(|this, _, _, cx| this.on_twitch_copy(cx)))
            .child(icon(
                if copied { Icon::Check } else { Icon::Copy },
                px(18.0),
                copy_color,
            ))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(px(10.0))
                    .text_color(copy_color)
                    .child(if copied {
                        tr!("twitch_device_copied")
                    } else {
                        tr!("twitch_device_copy")
                    }),
            );

        let remaining = code.map_or(Duration::ZERO, DeviceCodeData::remaining);
        let total = remaining.as_secs();
        let clock: SharedString = SharedString::from(format!("{}:{:02}", total / 60, total % 60));
        let meta_row = div()
            .flex()
            .items_center()
            .gap(px(14.0))
            .mt(px(10.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .child(icon(Icon::Clock, px(13.0), palette.text_muted))
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(px(11.5))
                            .text_color(palette.text_muted)
                            .child(tr!("twitch_device_expires_in")),
                    )
                    .child(
                        div()
                            .font_family(mono_family())
                            .text_size(px(11.5))
                            .text_color(palette.text_primary)
                            .child(clock),
                    ),
            )
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(palette.text_faint)
                    .child("\u{00b7}"),
            )
            .child(
                div()
                    .id("twitch-device-new-code")
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| this.begin_twitch_device(cx)))
                    .child(icon(Icon::Refresh, px(12.0), accent))
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(px(11.5))
                            .text_color(accent)
                            .child(tr!("twitch_device_get_new_code")),
                    ),
            );

        step_card(2, accent, palette.shell, palette)
            .border_color(accent)
            .mb(px(14.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .font_family(body_family())
                            .font_weight(FontWeight::MEDIUM)
                            .text_size(FONT_XS)
                            .text_color(palette.text_primary)
                            .mb(px(10.0))
                            .child(tr!("twitch_device_enter_title")),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .child(code_box)
                            .child(copy_btn),
                    )
                    .child(meta_row),
            )
            .into_any_element()
    }

    fn twitch_progress_card(
        &self,
        requesting: bool,
        accent: Rgba,
        palette: &ForgePalette,
    ) -> AnyElement {
        let name = self.display_name.clone();
        let scopes = scopes_preview();
        let headline = if requesting {
            tr!("twitch_device_requesting", name = name.as_str())
        } else {
            tr!("twitch_device_waiting", name = name.as_str())
        };
        let subline = tr!(
            "twitch_device_polling_subline",
            interval = TWITCH_DEVICE_POLL_SECS,
            scopes = scopes.as_str()
        );
        let pulse = div()
            .flex_none()
            .size(px(8.0))
            .rounded(px(4.0))
            .bg(accent)
            .with_animation(
                SharedString::from("twitch-device-pulse"),
                Animation::new(Duration::from_millis(1400)).repeat(),
                |el, delta| el.opacity(1.0 - (delta * 2.0 - 1.0).abs() * 0.6),
            );
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(10.0))
            .py(px(11.0))
            .px(px(14.0))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.elevated)
            .mb(px(14.0))
            .child(pulse)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(1.0))
                    .child(
                        div()
                            .font_family(body_family())
                            .font_weight(FontWeight::MEDIUM)
                            .text_size(FONT_XS)
                            .text_color(palette.text_primary)
                            .child(headline),
                    )
                    .children((!requesting).then(|| {
                        div()
                            .font_family(mono_family())
                            .text_size(FONT_XXS)
                            .text_color(palette.text_faint)
                            .child(subline)
                    })),
            )
            .into_any_element()
    }

    fn twitch_done_card(&self, palette: &ForgePalette) -> AnyElement {
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(10.0))
            .py(px(11.0))
            .px(px(14.0))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.success)
            .bg(palette.elevated)
            .mb(px(14.0))
            .child(icon(Icon::CircleCheckFilled, px(16.0), palette.success))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(tr!("oauth_done_authorized")),
            )
            .into_any_element()
    }

    fn twitch_expired_card(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Md, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.warning)
            .bg(palette.elevated)
            .mb(px(14.0))
            .child(icon(Icon::Clock, px(16.0), palette.warning))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(tr!("twitch_device_expired_title")),
            )
            .into_any_element()
    }

    fn twitch_error_card(
        &self,
        title: String,
        accent: Rgba,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let detail = self
            .twitch_device
            .as_ref()
            .and_then(|d| d.error.clone())
            .unwrap_or_else(|| tr!("twitch_device_denied_detail").to_string());
        let retry = div()
            .id("twitch-denied-retry")
            .flex_none()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Md, density))
            .rounded(radius(Radius::Sm))
            .bg(accent)
            .cursor_pointer()
            .hover(|s| s.bg(with_alpha(accent, 0.85)))
            .on_click(cx.listener(|this, _, _, cx| this.begin_twitch_device(cx)))
            .child(icon(Icon::Refresh, px(12.0), palette.shell))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.shell)
                    .child(tr!("oauth_btn_retry")),
            );
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Md, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.random)
            .bg(palette.elevated)
            .mb(px(14.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(icon(Icon::AlertTriangle, px(16.0), palette.random))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .font_family(body_family())
                            .font_weight(FontWeight::MEDIUM)
                            .text_size(FONT_XS)
                            .text_color(palette.text_primary)
                            .child(title),
                    )
                    .child(retry),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.random)
                    .child(detail),
            )
            .into_any_element()
    }

    fn twitch_device_footer(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let back = div()
            .id("twitch-device-choose-different")
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .cursor_pointer()
            .on_click(cx.listener(|this, _, _, cx| this.cancel_twitch_and_leave(cx)))
            .child(icon(Icon::ChevronLeft, px(13.0), palette.text_muted))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(tr!("oauth_footer_choose_different")),
            );
        let later = div()
            .id("twitch-device-later")
            .flex_none()
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Xs, density))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .cursor_pointer()
            .hover(|s| s.bg(with_alpha(palette.border_regular, 0.06)))
            .on_click(cx.listener(|this, _, _, cx| this.cancel_twitch_and_leave(cx)))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_secondary)
                    .child(tr!("twitch_device_do_later")),
            );
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .pt(px(4.0))
            .child(back)
            .child(later)
            .into_any_element()
    }

    pub(crate) fn twitch_reauth_banner(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let cta = div()
            .id("twitch-reauth")
            .flex_none()
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .bg(palette.warning)
            .cursor_pointer()
            .hover(|s| s.bg(with_alpha(palette.warning, 0.85)))
            .on_click(cx.listener(|this, _, _, cx| this.reset_to_connect(PlatformId::Twitch, cx)))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.shell)
                    .child(tr!("twitch_reauth_btn")),
            );
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.warning)
            .bg(palette.shell)
            .child(icon(Icon::AlertTriangle, px(14.0), palette.warning))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(spacing(Spacing::Xxs, density))
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_SM)
                            .text_color(palette.text_primary)
                            .child(tr!("twitch_reauth_title")),
                    )
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_muted)
                            .child(tr!("twitch_reauth_detail")),
                    ),
            )
            .child(cta)
            .into_any_element()
    }
}

fn status_dot(color: Rgba) -> impl IntoElement {
    div().flex_none().size(px(8.0)).rounded(px(4.0)).bg(color)
}

fn step_card(n: u8, circle_bg: Rgba, circle_fg: Rgba, palette: &ForgePalette) -> gpui::Div {
    let circle = div()
        .flex_none()
        .size(px(24.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(12.0))
        .bg(circle_bg)
        .child(
            div()
                .font_family(body_family())
                .font_weight(FontWeight::MEDIUM)
                .text_size(FONT_XXS)
                .text_color(circle_fg)
                .child(n.to_string()),
        );
    div()
        .w_full()
        .flex()
        .items_start()
        .gap(px(14.0))
        .p(px(16.0))
        .rounded(radius(Radius::Md))
        .border(BORDER_THIN)
        .bg(palette.elevated)
        .child(circle)
}
