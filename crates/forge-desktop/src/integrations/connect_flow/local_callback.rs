use forge_components::CTA_HOVER_ALPHA;
use forge_components::FONT_FIELD;
use forge_components::FONT_LABEL;
use forge_components::FONT_META;
use forge_components::FONT_ROW;
use forge_components::ROW_HOVER_WASH_ALPHA;
use std::sync::Arc;
use std::time::Duration;

const AUTHORIZATION_TIMEOUT: Duration = Duration::from_secs(300);

use crate::app::async_bridge;
use crate::app::error::DesktopError;
use forge_components::{
    ActivePresentation, BORDER_THIN, Density, FONT_XS, FONT_XXS, ForgePalette, Icon, KeyActivated,
    Radius, Spacing, icon, pulse_dot, radius, scaled, spacing, spinner, tr, with_alpha,
};
use forge_types::PlatformId;
use gpui::{
    AnyElement, App, ClickEvent, Context, FontWeight, HighlightStyle, Hsla, Rgba, SharedString,
    StyledText, div, prelude::*, px,
};
use tokio_util::sync::CancellationToken;

use super::{ConnectFlow, FlowState, credentials_missing, status_dot};
use crate::app::start_failure::start_failure_text;
use crate::shared::user_text::resolve_message;
use forge_host::integration_catalog::loopback_authorize_endpoint;
use forge_host::integrations::BuiltinObject;
use forge_platform_core::{LoopbackAttempt, LoopbackSignIn};
use forge_runtime::IntegrationStartError;

mod view;

type AttemptHandle = Arc<tokio::sync::Mutex<Option<Box<dyn LoopbackAttempt>>>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LocalCallbackFlowPhase {
    Idle,
    Starting,
    Waiting,
    Authorized,
    Failed,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StepState {
    Pending,
    Active,
    Done,
}

struct LocalCallbackData {
    auth_url: String,
}

pub(super) struct LocalCallbackState {
    sign_in: Option<Arc<dyn LoopbackSignIn>>,
    attempt: Option<AttemptHandle>,
    phase: LocalCallbackFlowPhase,
    auth_url: Option<String>,
    error: Option<String>,
    cancel: CancellationToken,
}

impl LocalCallbackState {
    pub(super) fn new(sign_in: Option<Arc<dyn LoopbackSignIn>>) -> Self {
        Self {
            sign_in,
            attempt: None,
            phase: LocalCallbackFlowPhase::Idle,
            auth_url: None,
            error: None,
            cancel: CancellationToken::new(),
        }
    }

    pub(super) fn cancel(&self) {
        self.cancel.cancel();
    }

    pub(super) fn abandon(&mut self) {
        self.cancel();
        self.attempt = None;
    }

    fn fail(&mut self, error: String) {
        self.phase = LocalCallbackFlowPhase::Failed;
        self.error = Some(error);
    }
}

impl ConnectFlow {
    pub(super) fn local_mut(&mut self) -> Option<&mut LocalCallbackState> {
        match &mut self.state {
            FlowState::LocalCallback(state) => Some(state),
            FlowState::DeviceCode(_) => None,
        }
    }

    fn local(&self) -> Option<&LocalCallbackState> {
        match &self.state {
            FlowState::LocalCallback(state) => Some(state),
            FlowState::DeviceCode(_) => None,
        }
    }

    fn phase(&self) -> LocalCallbackFlowPhase {
        self.local()
            .map_or(LocalCallbackFlowPhase::Idle, |local| local.phase)
    }

    fn auth_url(&self) -> Option<&str> {
        self.local()?.auth_url.as_deref()
    }

    fn fail_local(&mut self, error: String, cx: &mut Context<Self>) {
        if let Some(local) = self.local_mut() {
            local.fail(error);
        }
        cx.notify();
    }

    fn start_connect(&mut self, cx: &mut Context<Self>) {
        let platform = self.platform;
        let Some(local) = self.local_mut() else {
            return;
        };
        local.phase = LocalCallbackFlowPhase::Starting;
        local.error = None;
        local.cancel.cancel();
        let cancel = CancellationToken::new();
        local.cancel = cancel.clone();
        let Some(sign_in) = local.sign_in.clone() else {
            self.fail_local(credentials_missing(platform), cx);
            return;
        };
        let handle: AttemptHandle = Arc::new(tokio::sync::Mutex::new(Some(sign_in.attempt())));
        local.attempt = Some(Arc::clone(&handle));
        self.spawn_start(cancel, start_attempt(handle), cx);
        cx.notify();
    }

    fn spawn_start(
        &self,
        cancel: CancellationToken,
        fut: impl std::future::Future<Output = Result<LocalCallbackData, DesktopError>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        async_bridge::run_async(
            &self.rt_handle,
            fut,
            move |this, result, cx| {
                if cancel.is_cancelled() {
                    return;
                }
                this.apply_start_result(result, cancel, cx);
            },
            cx,
        );
    }

    fn apply_start_result(
        &mut self,
        result: Result<LocalCallbackData, DesktopError>,
        cancel: CancellationToken,
        cx: &mut Context<Self>,
    ) {
        let data = match result {
            Ok(data) => data,
            Err(e) => {
                tracing::warn!(error = %e, "oauth sign-in failed");
                self.fail_local(e.user_message(), cx);
                return;
            }
        };
        let auth_url = data.auth_url.clone();
        let Some(local) = self.local_mut() else {
            return;
        };
        local.auth_url = Some(data.auth_url);
        local.phase = LocalCallbackFlowPhase::Waiting;
        let attempt = local.attempt.clone();
        self.open_url(auth_url, cx);

        let Some(attempt) = attempt else {
            self.fail_local(DesktopError::SignInFlowConsumed.user_message(), cx);
            return;
        };
        let wait_cancel = cancel.clone();
        self.spawn_wait(
            cancel,
            async move { wait_for_authorization(attempt, wait_cancel).await },
            cx,
        );
        cx.notify();
    }

    fn spawn_wait(
        &self,
        cancel: CancellationToken,
        fut: impl std::future::Future<Output = Result<(), DesktopError>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        async_bridge::run_async(
            &self.rt_handle,
            fut,
            move |this, result, cx| {
                if cancel.is_cancelled() {
                    return;
                }
                this.apply_wait_result(result, cx);
            },
            cx,
        );
    }

    fn apply_wait_result(&mut self, result: Result<(), DesktopError>, cx: &mut Context<Self>) {
        match result {
            Ok(()) => {
                if let Some(local) = self.local_mut() {
                    local.phase = LocalCallbackFlowPhase::Authorized;
                }
                self.activate(cx);
            }
            Err(e) => {
                tracing::warn!(error = %e, "oauth sign-in failed");
                self.fail_local(e.user_message(), cx);
            }
        }
        cx.notify();
    }

    fn activate(&mut self, cx: &mut Context<Self>) {
        let Some(slot) = self.slot.clone() else {
            self.fail_local(credentials_missing(self.platform), cx);
            return;
        };
        async_bridge::run_async(
            &self.rt_handle,
            async move { slot.activate().await },
            |this, result, cx| this.apply_activation(result, cx),
            cx,
        );
    }

    fn apply_activation(
        &mut self,
        result: Result<BuiltinObject, IntegrationStartError>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(object) => self.finish(object, cx),
            Err(error) => {
                tracing::warn!(error = %error, "in-session connect failed");
                self.fail_local(start_failure_text(&error).primary, cx);
            }
        }
    }

    fn reset_flow(&mut self, cx: &mut Context<Self>) {
        if let Some(local) = self.local_mut() {
            local.abandon();
            local.phase = LocalCallbackFlowPhase::Idle;
            local.auth_url = None;
            local.error = None;
        }
        cx.notify();
    }
}

fn step_state(phase: LocalCallbackFlowPhase, index: usize) -> StepState {
    match phase {
        LocalCallbackFlowPhase::Idle | LocalCallbackFlowPhase::Failed => StepState::Pending,
        LocalCallbackFlowPhase::Starting => {
            if index == 0 {
                StepState::Active
            } else {
                StepState::Pending
            }
        }
        LocalCallbackFlowPhase::Waiting => match index {
            0 => StepState::Done,
            1 => StepState::Active,
            _ => StepState::Pending,
        },
        LocalCallbackFlowPhase::Authorized => StepState::Done,
    }
}

fn idle_auth_url_template(platform: PlatformId) -> String {
    format!(
        "{}?response_type=code&code_challenge=\u{2026}&redirect_uri=http%3A%2F%2F127.0.0.1%3A\u{2026}%2Fcallback",
        loopback_authorize_endpoint(platform)
    )
}

fn param_value<'a>(url: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("{key}=");
    let start = url.find(&needle)? + needle.len();
    let rest = &url[start..];
    let end = rest.find('&').unwrap_or(rest.len());
    Some(&rest[..end])
}

fn elide_code_challenge(url: &str) -> String {
    let needle = "code_challenge=";
    let Some(i) = url.find(needle) else {
        return url.to_owned();
    };
    let start = i + needle.len();
    let end = url[start..].find('&').map_or(url.len(), |j| start + j);
    format!("{}\u{2026}{}", &url[..start], &url[end..])
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => match (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                (Some(h), Some(l)) => {
                    out.push(h * 16 + l);
                    i += 3;
                }
                _ => {
                    out.push(b'%');
                    i += 1;
                }
            },
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn loopback_display(url: &str) -> Option<String> {
    param_value(url, "redirect_uri").map(percent_decode)
}

fn loopback_port(url: &str) -> Option<String> {
    let display = loopback_display(url)?;
    let after = display.rsplit_once(':')?.1;
    Some(after.split('/').next()?.to_owned())
}

fn scopes_display(url: &str) -> Option<String> {
    let raw = param_value(url, "scope")?;
    let joined = raw
        .split('+')
        .take(3)
        .map(percent_decode)
        .collect::<Vec<_>>()
        .join(" ");
    if joined.is_empty() {
        None
    } else {
        Some(joined)
    }
}

async fn start_attempt(handle: AttemptHandle) -> Result<LocalCallbackData, DesktopError> {
    let mut guard = handle.lock().await;
    let attempt = guard.as_mut().ok_or(DesktopError::SignInFlowConsumed)?;
    let url = attempt.start().await?;
    Ok(LocalCallbackData {
        auth_url: url.auth_url,
    })
}

async fn wait_for_authorization(
    handle: AttemptHandle,
    cancel: CancellationToken,
) -> Result<(), DesktopError> {
    let attempt = {
        let mut guard = handle.lock().await;
        guard.take().ok_or(DesktopError::SignInFlowConsumed)?
    };
    tokio::select! {
        biased;
        () = cancel.cancelled() => Err(DesktopError::SignInCancelled),
        outcome = attempt.complete(AUTHORIZATION_TIMEOUT) => outcome.map_err(DesktopError::from),
    }
}
