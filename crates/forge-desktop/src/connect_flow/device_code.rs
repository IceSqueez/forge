use forge_components::ActivePresentation;
use forge_components::CTA_HOVER_ALPHA;
use forge_components::FONT_LABEL;
use forge_components::FONT_MICRO;
use forge_components::ROW_HOVER_WASH_ALPHA;
use gpui::Pixels;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use forge_components::{
    BORDER_THIN, Density, FONT_XS, FONT_XXS, ForgePalette, Icon, Radius, Spacing, body_family,
    icon, mono_family, pulse_dot, radius, scaled, spacing, spinner, tr, with_alpha,
};
use forge_platform_core::{DeviceAuthEnd, PlatformError};
use forge_platform_twitch::{DeviceCodeInfo, TWITCH_BROADCASTER_SCOPES, TwitchAuthFlow, UserInfo};
use forge_storage::CredentialsRepo;
use forge_types::PlatformId;
use gpui::{
    AnyElement, ClipboardItem, Context, FontWeight, HighlightStyle, Hsla, Rgba, SharedString,
    StyledText, div, prelude::*, px,
};
use tokio_util::sync::CancellationToken;

use super::{ConnectFlow, status_dot};
use crate::async_bridge;
use crate::error::DesktopError;
use crate::integrations::BuiltinObject;
use crate::start_failure::start_failure_text;
use forge_runtime::IntegrationStartError;

pub(super) type TwitchFlowHandle = Arc<tokio::sync::Mutex<Option<TwitchAuthFlow>>>;

const TWITCH_DEVICE_POLL_SECS: &str = "5";
const COPY_FLIP: Duration = Duration::from_millis(1400);
const USER_CODE_FONT: Pixels = px(28.0);

struct TwitchAuthOutcome {
    user_info: UserInfo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TwitchDevicePhase {
    Starting,
    Waiting,
    Expired,
    Denied,
    Failed,
    Authorized,
}

struct DeviceCodeData {
    user_code: String,
    verification_uri: String,
    expires_at: SystemTime,
}

impl DeviceCodeData {
    fn remaining(&self) -> Duration {
        self.expires_at
            .duration_since(SystemTime::now())
            .unwrap_or(Duration::ZERO)
    }
}

pub(super) struct TwitchDeviceState {
    phase: TwitchDevicePhase,
    code: Option<DeviceCodeData>,
    copied: bool,
    pub(super) cancel: CancellationToken,
    error: Option<String>,
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
    Other(DesktopError),
}

async fn request_code(flow: TwitchFlowHandle) -> Result<DeviceCodeInfo, DesktopError> {
    let mut guard = flow.lock().await;
    let inner = guard.as_mut().ok_or(DesktopError::SignInFlowConsumed)?;
    inner.start().await.map_err(DesktopError::from)
}

fn wait_failure(error: PlatformError) -> TwitchWaitError {
    match error {
        PlatformError::DeviceAuthEnded(DeviceAuthEnd::Cancelled) => TwitchWaitError::Cancelled,
        PlatformError::DeviceAuthEnded(DeviceAuthEnd::Denied) => TwitchWaitError::Denied,
        PlatformError::DeviceAuthEnded(DeviceAuthEnd::Expired) => TwitchWaitError::Expired,
        other => TwitchWaitError::Other(other.into()),
    }
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
            .ok_or(TwitchWaitError::Other(DesktopError::SignInFlowConsumed))?
    };
    let bundle = inner
        .wait_for_authorization(cancel)
        .await
        .map_err(wait_failure)?;
    forge_platform_twitch::credentials::store(&*credentials, &bundle)
        .await
        .map_err(|e| TwitchWaitError::Other(e.into()))?;
    Ok(TwitchAuthOutcome {
        user_info: bundle.user_info,
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

impl ConnectFlow {
    pub(super) fn begin_twitch_device(&mut self, cx: &mut Context<Self>) {
        if let Some(dev) = &self.twitch_device {
            dev.cancel.cancel();
        }
        let id_source = if std::env::var("FORGE_TWITCH_CLIENT_ID").is_ok() {
            "runtime env"
        } else {
            "compiled default"
        };
        let (Some(cid), Some(sign_in)) = (
            forge_platform_twitch::client_id(),
            self.twitch_sign_in.clone(),
        ) else {
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
            Arc::new(tokio::sync::Mutex::new(Some(sign_in.broadcaster_flow())));
        self.twitch_flow = Some(Arc::clone(&handle));
        let attempt = TwitchDeviceState::starting();
        let cancel = attempt.cancel.clone();
        self.twitch_device = Some(attempt);
        async_bridge::run_async(
            &self.rt_handle,
            request_code(Arc::clone(&handle)),
            move |this, result, cx| this.on_twitch_device_start(handle, cancel, result, cx),
            cx,
        );
        cx.notify();
    }

    fn on_twitch_device_start(
        &mut self,
        handle: TwitchFlowHandle,
        cancel: CancellationToken,
        result: Result<DeviceCodeInfo, DesktopError>,
        cx: &mut Context<Self>,
    ) {
        if !cancel.is_cancelled() {
            self.apply_twitch_device_start(handle, cancel, result, cx);
        }
    }

    fn apply_twitch_device_start(
        &mut self,
        handle: TwitchFlowHandle,
        cancel: CancellationToken,
        result: Result<DeviceCodeInfo, DesktopError>,
        cx: &mut Context<Self>,
    ) {
        let info = match result {
            Ok(info) => info,
            Err(e) => {
                tracing::warn!(error = %e, "twitch device code request failed");
                self.twitch_device = Some(TwitchDeviceState::failed(
                    TwitchDevicePhase::Failed,
                    e.user_message(),
                ));
                cx.notify();
                return;
            }
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
        let credentials = Arc::clone(&self.credentials);
        async_bridge::run_async(
            &self.rt_handle,
            wait_for_auth(handle, cancel.clone(), credentials),
            move |this, result, cx| this.on_twitch_wait(&cancel, result, cx),
            cx,
        );
        cx.notify();
    }

    fn on_twitch_wait(
        &mut self,
        cancel: &CancellationToken,
        result: Result<TwitchAuthOutcome, TwitchWaitError>,
        cx: &mut Context<Self>,
    ) {
        if !cancel.is_cancelled() {
            self.apply_twitch_wait(result, cx);
        }
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
                    dev.error = Some(msg.user_message());
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
        let Some(slot) = self.twitch_slot.clone() else {
            tracing::error!("twitch integration absent at sign-in; client id was missing at boot");
            if let Some(dev) = &mut self.twitch_device {
                dev.phase = TwitchDevicePhase::Failed;
                dev.error = Some(tr!("auth_error_credentials_missing_twitch").to_string());
            }
            cx.notify();
            return;
        };
        async_bridge::run_async(
            &self.rt_handle,
            async move { slot.activate().await },
            |this, result, cx| this.apply_twitch_activation(result, cx),
            cx,
        );
    }

    fn apply_twitch_activation(
        &mut self,
        result: Result<BuiltinObject, IntegrationStartError>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(object) => self.finish(object, cx),
            Err(error) => {
                tracing::warn!(error = %error, "twitch in-session connect failed");
                if let Some(dev) = &mut self.twitch_device {
                    dev.phase = TwitchDevicePhase::Failed;
                    dev.error = Some(start_failure_text(&error).primary);
                }
                cx.notify();
            }
        }
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
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the view may close before the result arrives"
            )]
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
        self.leave(cx);
    }

    pub(super) fn twitch_device_status(
        &self,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
        let accent = crate::platform_visuals::platform_color(PlatformId::Twitch, palette);
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
                status_dot(palette.danger).into_any_element(),
                tr!("common_status_not_connected"),
                palette.danger,
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

    pub(super) fn twitch_device_column(
        &self,
        accent: Rgba,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let phase = self
            .twitch_device
            .as_ref()
            .map_or(TwitchDevicePhase::Starting, |d| d.phase);
        let status_card = match phase {
            TwitchDevicePhase::Starting => {
                self.twitch_progress_card(true, accent, palette, density)
            }
            TwitchDevicePhase::Waiting => {
                self.twitch_progress_card(false, accent, palette, density)
            }
            TwitchDevicePhase::Authorized => self.twitch_done_card(palette, density),
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
            .child(self.twitch_explainer(palette, density));
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

    fn twitch_explainer(&self, palette: &ForgePalette, density: Density) -> AnyElement {
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
            .mb(scaled(px(22.0), density))
            .child(styled)
            .into_any_element()
    }

    fn twitch_url_card(
        &self,
        accent: Rgba,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
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
            .py(scaled(px(8.0), density))
            .px(scaled(px(12.0), density))
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
            .gap(scaled(px(5.0), density))
            .py(scaled(px(5.0), density))
            .px(scaled(px(11.0), density))
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
        step_card(
            1,
            palette.surface_overlay,
            palette.text_primary,
            palette,
            density,
        )
        .border_color(palette.border_regular)
        .mb(spacing(Spacing::Sm, density))
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
                        .mb(scaled(px(8.0), density))
                        .child(tr!("twitch_device_open_title")),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(scaled(px(8.0), density))
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
        let density = cx.density();
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
                    .text_size(USER_CODE_FONT)
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
            .gap(scaled(px(8.0), density))
            .py(scaled(px(14.0), density))
            .px(scaled(px(20.0), density))
            .rounded(radius(Radius::Md))
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
            .gap(scaled(px(3.0), density))
            .p(scaled(px(14.0), density))
            .rounded(radius(Radius::Md))
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
                    .text_size(FONT_MICRO)
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
            .gap(scaled(px(14.0), density))
            .mt(spacing(Spacing::Sm, density))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(scaled(px(5.0), density))
                    .child(icon(Icon::Clock, px(13.0), palette.text_muted))
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_LABEL)
                            .text_color(palette.text_muted)
                            .child(tr!("twitch_device_expires_in")),
                    )
                    .child(
                        div()
                            .font_family(mono_family())
                            .text_size(FONT_LABEL)
                            .text_color(palette.text_primary)
                            .child(clock),
                    ),
            )
            .child(
                div()
                    .text_size(FONT_LABEL)
                    .text_color(palette.text_faint)
                    .child("\u{00b7}"),
            )
            .child(
                div()
                    .id("twitch-device-new-code")
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xxs, density))
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| this.begin_twitch_device(cx)))
                    .child(icon(Icon::Refresh, px(12.0), accent))
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_LABEL)
                            .text_color(accent)
                            .child(tr!("twitch_device_get_new_code")),
                    ),
            );

        step_card(2, accent, palette.shell, palette, density)
            .border_color(accent)
            .mb(scaled(px(14.0), density))
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
                            .mb(spacing(Spacing::Sm, density))
                            .child(tr!("twitch_device_enter_title")),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(spacing(Spacing::Sm, density))
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
        density: Density,
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
        let pulse = pulse_dot("twitch-device-pulse", accent, px(8.0));
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .py(scaled(px(11.0), density))
            .px(scaled(px(14.0), density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.elevated)
            .mb(scaled(px(14.0), density))
            .child(pulse)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(scaled(px(1.0), density))
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

    fn twitch_done_card(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .py(scaled(px(11.0), density))
            .px(scaled(px(14.0), density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.success)
            .bg(palette.elevated)
            .mb(scaled(px(14.0), density))
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
            .mb(scaled(px(14.0), density))
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
            .hover(|s| s.bg(with_alpha(accent, CTA_HOVER_ALPHA)))
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
            .border_color(palette.danger)
            .bg(palette.elevated)
            .mb(scaled(px(14.0), density))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(icon(Icon::AlertTriangle, px(16.0), palette.danger))
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
                    .text_color(palette.danger)
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
            .hover(|s| s.bg(with_alpha(palette.border_regular, ROW_HOVER_WASH_ALPHA)))
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
            .pt(spacing(Spacing::Xxs, density))
            .child(back)
            .child(later)
            .into_any_element()
    }
}

fn step_card(
    n: u8,
    circle_bg: Rgba,
    circle_fg: Rgba,
    palette: &ForgePalette,
    density: Density,
) -> gpui::Div {
    let circle = div()
        .flex_none()
        .size(px(24.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(radius(Radius::Lg))
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
        .gap(scaled(px(14.0), density))
        .p(spacing(Spacing::Md, density))
        .rounded(radius(Radius::Md))
        .border(BORDER_THIN)
        .bg(palette.elevated)
        .child(circle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_storage::test_support::MemoryCredentials;

    #[test]
    fn each_ended_device_flow_maps_to_its_own_wait_failure() {
        assert!(matches!(
            wait_failure(PlatformError::DeviceAuthEnded(DeviceAuthEnd::Cancelled)),
            TwitchWaitError::Cancelled
        ));
        assert!(matches!(
            wait_failure(PlatformError::DeviceAuthEnded(DeviceAuthEnd::Denied)),
            TwitchWaitError::Denied
        ));
        assert!(matches!(
            wait_failure(PlatformError::DeviceAuthEnded(DeviceAuthEnd::Expired)),
            TwitchWaitError::Expired
        ));
        assert!(matches!(
            wait_failure(PlatformError::Auth {
                reason: "refused".into(),
            }),
            TwitchWaitError::Other(_)
        ));
    }

    use crate::connect_flow::ConnectFlowLaunch;

    use forge_types::PlatformId;
    use gpui::Entity;

    struct Mounted {
        flow: Entity<ConnectFlow>,
        _runtime: tokio::runtime::Runtime,
    }

    fn mounted_flow(cx: &mut gpui::TestAppContext) -> Mounted {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let launch = ConnectFlowLaunch {
            platform: PlatformId::YouTube,
            display_name: "YouTube".to_owned(),
            rt_handle: runtime.handle().clone(),
            credentials: Arc::new(MemoryCredentials::default()),
            twitch_slot: None,
            kick_slot: None,
            youtube_slot: None,
            twitch_sign_in: None,
            youtube_sign_in: None,
            kick_sign_in: None,
        };
        let flow = {
            let _runtime_context = runtime.enter();
            cx.new(|cx| ConnectFlow::new(launch, cx))
        };
        Mounted {
            flow,
            _runtime: runtime,
        }
    }

    fn device_state(
        flow: &Entity<ConnectFlow>,
        cx: &mut gpui::TestAppContext,
    ) -> (TwitchDevicePhase, Option<String>, bool) {
        flow.read_with(cx, |this, _| {
            let dev = this.twitch_device.as_ref().unwrap();
            (dev.phase, dev.error.clone(), dev.code.is_some())
        })
    }

    fn device_code() -> DeviceCodeInfo {
        DeviceCodeInfo {
            user_code: "ABCD-EFGH".to_owned(),
            verification_uri: "https://example.invalid/activate".to_owned(),
            expires_at: std::time::SystemTime::UNIX_EPOCH,
        }
    }

    fn no_flow() -> TwitchFlowHandle {
        Arc::new(tokio::sync::Mutex::new(None))
    }

    fn newer_attempt_after_cancelling_one(
        flow: &Entity<ConnectFlow>,
        cx: &mut gpui::TestAppContext,
    ) -> CancellationToken {
        flow.update(cx, |this, _| {
            let stale = CancellationToken::new();
            stale.cancel();
            this.twitch_device = Some(TwitchDeviceState::starting());
            stale
        })
    }

    #[gpui::test]
    fn a_device_code_arriving_for_a_cancelled_attempt_leaves_the_newer_one_alone(
        cx: &mut gpui::TestAppContext,
    ) {
        for late in [Ok(device_code()), Err(DesktopError::TaskStopped)] {
            let mounted = mounted_flow(cx);
            let stale = newer_attempt_after_cancelling_one(&mounted.flow, cx);

            mounted.flow.update(cx, |this, cx| {
                this.on_twitch_device_start(no_flow(), stale, late, cx);
            });

            assert_eq!(
                device_state(&mounted.flow, cx),
                (TwitchDevicePhase::Starting, None, false)
            );
        }
    }

    #[gpui::test]
    fn a_device_code_failure_for_the_live_attempt_shows_on_the_screen(
        cx: &mut gpui::TestAppContext,
    ) {
        let mounted = mounted_flow(cx);
        let live = CancellationToken::new();
        mounted.flow.update(cx, |this, cx| {
            this.twitch_device = Some(TwitchDeviceState::starting());
            this.on_twitch_device_start(no_flow(), live, Err(DesktopError::TaskStopped), cx);
        });

        assert_eq!(
            device_state(&mounted.flow, cx),
            (
                TwitchDevicePhase::Failed,
                Some(DesktopError::TaskStopped.user_message()),
                false
            )
        );
    }

    #[gpui::test]
    fn a_sign_in_result_for_a_cancelled_attempt_is_dropped(cx: &mut gpui::TestAppContext) {
        let mounted = mounted_flow(cx);
        let stale = newer_attempt_after_cancelling_one(&mounted.flow, cx);

        mounted.flow.update(cx, |this, cx| {
            this.on_twitch_wait(&stale, Err(TwitchWaitError::Denied), cx);
        });

        assert_eq!(
            device_state(&mounted.flow, cx),
            (TwitchDevicePhase::Starting, None, false)
        );
    }

    #[gpui::test]
    fn a_denied_sign_in_for_the_live_attempt_shows_as_denied(cx: &mut gpui::TestAppContext) {
        let mounted = mounted_flow(cx);
        let live = CancellationToken::new();

        let phase = mounted.flow.update(cx, |this, cx| {
            this.twitch_device = Some(TwitchDeviceState::starting());
            this.on_twitch_wait(&live, Err(TwitchWaitError::Denied), cx);
            this.twitch_device.as_ref().unwrap().phase
        });

        assert_eq!(phase, TwitchDevicePhase::Denied);
    }

    #[gpui::test]
    fn authorized_sign_in_without_an_install_seed_fails_with_credentials_missing(
        cx: &mut gpui::TestAppContext,
    ) {
        let Mounted { flow, _runtime } = mounted_flow(cx);
        let _runtime_context = _runtime.enter();
        let outcome = TwitchAuthOutcome {
            user_info: UserInfo {
                id: "1234".to_owned(),
                login: "streamer".to_owned(),
                display_name: "Streamer".to_owned(),
                broadcaster_type: forge_platform_twitch::BroadcasterTier::Standard,
            },
        };

        let (phase, error) = flow.update(cx, |this, cx| {
            this.twitch_device = Some(TwitchDeviceState::starting());
            this.install_twitch_outcome(outcome, cx);
            let dev = this.twitch_device.as_ref().unwrap();
            (dev.phase, dev.error.clone())
        });

        assert_eq!(
            (phase, error),
            (
                TwitchDevicePhase::Failed,
                Some(tr!("auth_error_credentials_missing_twitch").to_string())
            ),
            "a sign-in with no install seed must fail visibly instead of building a bundle"
        );
    }
}
