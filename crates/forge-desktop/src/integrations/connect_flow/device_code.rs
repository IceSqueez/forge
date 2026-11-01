use forge_components::CTA_HOVER_ALPHA;
use forge_components::FONT_LABEL;
use forge_components::FONT_MICRO;
use forge_components::ROW_HOVER_WASH_ALPHA;
use gpui::Pixels;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use forge_components::{
    ActivePresentation, BORDER_THIN, Density, FONT_XS, FONT_XXS, ForgePalette, Icon, KeyActivated,
    Radius, Spacing, icon, pulse_dot, radius, scaled, spacing, spinner, tr, with_alpha,
};
use forge_platform_core::{DeviceAuthEnd, PlatformError};
use forge_platform_twitch::{
    DeviceCodeInfo, PollCadence, TWITCH_BROADCASTER_SCOPES, TwitchAuthFlow, TwitchSignIn, UserInfo,
};
use forge_storage::CredentialsRepo;
use forge_types::PlatformId;
use gpui::{
    AnyElement, App, ClipboardItem, Context, FontWeight, HighlightStyle, Hsla, Rgba, SharedString,
    StyledText, div, prelude::*, px,
};
use tokio_util::sync::CancellationToken;

use super::{ConnectFlow, FlowState, credentials_missing, status_dot};
use crate::app::async_bridge;
use crate::app::error::DesktopError;
use crate::app::start_failure::start_failure_text;
use forge_host::integrations::BuiltinObject;
use forge_runtime::IntegrationStartError;

mod view;

pub(super) type TwitchFlowHandle = Arc<tokio::sync::Mutex<Option<TwitchAuthFlow>>>;

const CLIENT_ID_LOG_PREFIX_CHARS: usize = 6;
const COPY_FLIP: Duration = Duration::from_millis(1400);

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
    poll_interval: Duration,
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

pub(super) struct DeviceCodeState {
    sign_in: Option<TwitchSignIn>,
    flow: Option<TwitchFlowHandle>,
    attempt: Option<TwitchDeviceState>,
}

impl DeviceCodeState {
    pub(super) fn new(sign_in: Option<TwitchSignIn>) -> Self {
        Self {
            sign_in,
            flow: None,
            attempt: None,
        }
    }

    pub(super) fn cancel(&self) {
        if let Some(attempt) = &self.attempt {
            attempt.cancel.cancel();
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
    fn device_flow(&self) -> Option<&DeviceCodeState> {
        match &self.state {
            FlowState::DeviceCode(state) => Some(state),
            FlowState::LocalCallback(_) => None,
        }
    }

    fn device_flow_mut(&mut self) -> Option<&mut DeviceCodeState> {
        match &mut self.state {
            FlowState::DeviceCode(state) => Some(state),
            FlowState::LocalCallback(_) => None,
        }
    }

    fn device(&self) -> Option<&TwitchDeviceState> {
        self.device_flow()?.attempt.as_ref()
    }

    fn device_mut(&mut self) -> Option<&mut TwitchDeviceState> {
        self.device_flow_mut()?.attempt.as_mut()
    }

    fn set_device(&mut self, attempt: Option<TwitchDeviceState>) {
        if let Some(flow) = self.device_flow_mut() {
            flow.attempt = attempt;
        }
    }

    pub(super) fn begin_twitch_device(&mut self, cx: &mut Context<Self>) {
        if let Some(dev) = self.device() {
            dev.cancel.cancel();
        }
        let id_source = if std::env::var("FORGE_TWITCH_CLIENT_ID").is_ok() {
            "runtime env"
        } else {
            "compiled default"
        };
        let sign_in = self.device_flow().and_then(|flow| flow.sign_in.clone());
        let (Some(cid), Some(sign_in)) = (forge_platform_twitch::client_id(), sign_in) else {
            self.set_device(Some(TwitchDeviceState::failed(
                TwitchDevicePhase::Failed,
                credentials_missing(PlatformId::Twitch),
            )));
            cx.notify();
            return;
        };
        tracing::info!(
            source = id_source,
            id_prefix = %cid.chars().take(CLIENT_ID_LOG_PREFIX_CHARS).collect::<String>(),
            "starting twitch device flow"
        );
        let handle: TwitchFlowHandle =
            Arc::new(tokio::sync::Mutex::new(Some(sign_in.broadcaster_flow())));
        if let Some(flow) = self.device_flow_mut() {
            flow.flow = Some(Arc::clone(&handle));
        }
        let attempt = TwitchDeviceState::starting();
        let cancel = attempt.cancel.clone();
        self.set_device(Some(attempt));
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
                self.set_device(Some(TwitchDeviceState::failed(
                    TwitchDevicePhase::Failed,
                    e.user_message(),
                )));
                cx.notify();
                return;
            }
        };
        if let Some(dev) = self.device_mut() {
            dev.phase = TwitchDevicePhase::Waiting;
            dev.error = None;
            dev.code = Some(DeviceCodeData {
                user_code: info.user_code,
                verification_uri: info.verification_uri,
                expires_at: info.expires_at,
                poll_interval: info.poll_cadence.current(),
            });
        }
        self.follow_poll_cadence(info.poll_cadence, cancel.clone(), cx);
        let credentials = Arc::clone(&self.credentials);
        async_bridge::run_async(
            &self.rt_handle,
            wait_for_auth(handle, cancel.clone(), credentials),
            move |this, result, cx| this.on_twitch_wait(&cancel, result, cx),
            cx,
        );
        cx.notify();
    }

    fn follow_poll_cadence(
        &self,
        mut cadence: PollCadence,
        cancel: CancellationToken,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            while let Some(interval) = cadence.changed().await {
                let updated = this.update(cx, |this, cx| {
                    this.apply_poll_interval(&cancel, interval, cx);
                });
                if updated.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn apply_poll_interval(
        &mut self,
        cancel: &CancellationToken,
        interval: Duration,
        cx: &mut Context<Self>,
    ) {
        if cancel.is_cancelled() {
            return;
        }
        if let Some(code) = self.device_mut().and_then(|device| device.code.as_mut()) {
            code.poll_interval = interval;
            cx.notify();
        }
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
                if let Some(dev) = self.device_mut() {
                    dev.phase = TwitchDevicePhase::Authorized;
                }
                self.install_twitch_outcome(outcome, cx);
                cx.notify();
            }
            Err(TwitchWaitError::Cancelled) => {}
            Err(TwitchWaitError::Denied) => {
                if let Some(dev) = self.device_mut() {
                    dev.phase = TwitchDevicePhase::Denied;
                    dev.error = Some(tr!("twitch_device_denied_detail"));
                }
                cx.notify();
            }
            Err(TwitchWaitError::Expired) => {
                if let Some(dev) = self.device_mut() {
                    dev.phase = TwitchDevicePhase::Expired;
                    dev.error = None;
                }
                cx.notify();
            }
            Err(TwitchWaitError::Other(msg)) => {
                tracing::warn!(error = %msg, "twitch authorization failed");
                if let Some(dev) = self.device_mut() {
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
        let Some(slot) = self.slot.clone() else {
            tracing::error!("twitch integration absent at sign-in; client id was missing at boot");
            if let Some(dev) = self.device_mut() {
                dev.phase = TwitchDevicePhase::Failed;
                dev.error = Some(credentials_missing(PlatformId::Twitch));
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
                if let Some(dev) = self.device_mut() {
                    dev.phase = TwitchDevicePhase::Failed;
                    dev.error = Some(start_failure_text(&error).primary);
                }
                cx.notify();
            }
        }
    }

    fn on_twitch_copy(&mut self, cx: &mut Context<Self>) {
        let Some(code) = self
            .device()
            .and_then(|d| d.code.as_ref())
            .map(|c| c.user_code.clone())
        else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(code));
        if let Some(dev) = self.device_mut() {
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
                if let Some(dev) = this.device_mut() {
                    dev.copied = false;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn twitch_open_url(&self, cx: &mut Context<Self>) {
        let Some(url) = self
            .device()
            .and_then(|d| d.code.as_ref())
            .map(|c| c.verification_uri.clone())
        else {
            return;
        };
        self.open_url(url, cx);
    }

    fn cancel_twitch_and_leave(&mut self, cx: &mut Context<Self>) {
        if let Some(dev) = self.device() {
            dev.cancel.cancel();
        }
        self.set_device(None);
        if let Some(flow) = self.device_flow_mut() {
            flow.flow = None;
        }
        self.leave(cx);
    }
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

    use crate::integrations::connect_flow::{ConnectFlowLaunch, ConnectSignIn};

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
            platform: PlatformId::Twitch,
            display_name: "Twitch".to_owned(),
            rt_handle: runtime.handle().clone(),
            credentials: Arc::new(MemoryCredentials::default()),
            slot: None,
            sign_in: ConnectSignIn::DeviceCode(None),
        };
        let flow = {
            let _runtime_context = runtime.enter();
            cx.new(|_| ConnectFlow::idle(launch))
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
            let dev = this.device().unwrap();
            (dev.phase, dev.error.clone(), dev.code.is_some())
        })
    }

    fn device_code() -> DeviceCodeInfo {
        DeviceCodeInfo {
            user_code: "ABCD-EFGH".to_owned(),
            verification_uri: "https://example.invalid/activate".to_owned(),
            expires_at: std::time::SystemTime::UNIX_EPOCH,
            poll_cadence: PollCadence::fixed(Duration::from_secs(5)),
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
            this.set_device(Some(TwitchDeviceState::starting()));
            stale
        })
    }

    const CLIENT_ID_CUT_INSIDE_A_CHARACTER: &str = "aбвгдежзик";

    fn twitch_sign_in() -> forge_platform_twitch::TwitchSignIn {
        let publisher: Arc<dyn forge_events::EventPublisher> =
            Arc::new(forge_events::test_support::NullPublisher);
        forge_platform_twitch::TwitchAssembly::new(
            &mut forge_registry::SubActionRegistry::new(),
            Arc::new(MemoryCredentials::default()),
            &publisher,
            &forge_platform_core::PlatformEndpoints::default(),
            CLIENT_ID_CUT_INSIDE_A_CHARACTER.to_owned(),
        )
        .unwrap()
        .sign_in()
    }

    fn rerun_with_the_client_id_in_the_environment(test_name: &str) {
        let test_path = module_path!()
            .split_once("::")
            .map_or(module_path!(), |(_, path)| path);
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &format!("{test_path}::{test_name}")])
            .env(
                forge_platform_twitch::TWITCH_CLIENT_ID_ENV,
                CLIENT_ID_CUT_INSIDE_A_CHARACTER,
            )
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success() && stdout.contains("1 passed"),
            "{stdout}\n{}",
            String::from_utf8_lossy(&output.stderr),
        );
    }

    #[gpui::test]
    fn a_client_id_with_a_character_across_the_prefix_cut_logs_whole_characters(
        cx: &mut gpui::TestAppContext,
    ) {
        let environment = std::env::var(forge_platform_twitch::TWITCH_CLIENT_ID_ENV);
        if environment.as_deref() != Ok(CLIENT_ID_CUT_INSIDE_A_CHARACTER) {
            rerun_with_the_client_id_in_the_environment(
                "a_client_id_with_a_character_across_the_prefix_cut_logs_whole_characters",
            );
            return;
        }
        let mounted = mounted_flow(cx);
        let sign_in = {
            let _runtime_context = mounted._runtime.enter();
            twitch_sign_in()
        };
        mounted.flow.update(cx, |this, _| {
            if let Some(flow) = this.device_flow_mut() {
                flow.sign_in = Some(sign_in);
            }
        });

        let ((), lines) = forge_test_support::log_capture::capture(tracing::Level::INFO, || {
            mounted
                .flow
                .update(cx, |this, cx| this.begin_twitch_device(cx));
        });

        let logged = lines
            .iter()
            .find(|line| line.message() == "starting twitch device flow")
            .and_then(|line| line.field("id_prefix"));
        assert_eq!(logged, Some("aбвгде"));
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
    fn a_raised_poll_interval_reaches_the_live_attempt_only(cx: &mut gpui::TestAppContext) {
        const GRANTED: Duration = Duration::from_secs(5);
        const RAISED: Duration = Duration::from_secs(10);
        let mounted = mounted_flow(cx);
        let stale = CancellationToken::new();
        stale.cancel();

        let (ignored, shown) = mounted.flow.update(cx, |this, cx| {
            let mut device = TwitchDeviceState::starting();
            device.code = Some(DeviceCodeData {
                user_code: "ABCD-EFGH".to_owned(),
                verification_uri: "https://example.invalid/activate".to_owned(),
                expires_at: std::time::SystemTime::UNIX_EPOCH,
                poll_interval: GRANTED,
            });
            let live = device.cancel.clone();
            this.set_device(Some(device));
            let interval = |this: &ConnectFlow| {
                this.device()
                    .and_then(|device| device.code.as_ref())
                    .map(|code| code.poll_interval)
            };

            this.apply_poll_interval(&stale, RAISED, cx);
            let ignored = interval(this);
            this.apply_poll_interval(&live, RAISED, cx);
            (ignored, interval(this))
        });

        assert_eq!(ignored, Some(GRANTED));
        assert_eq!(shown, Some(RAISED));
    }

    #[gpui::test]
    fn a_device_code_failure_for_the_live_attempt_shows_on_the_screen(
        cx: &mut gpui::TestAppContext,
    ) {
        let mounted = mounted_flow(cx);
        let live = CancellationToken::new();
        mounted.flow.update(cx, |this, cx| {
            this.set_device(Some(TwitchDeviceState::starting()));
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
            this.set_device(Some(TwitchDeviceState::starting()));
            this.on_twitch_wait(&live, Err(TwitchWaitError::Denied), cx);
            this.device().unwrap().phase
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
            this.set_device(Some(TwitchDeviceState::starting()));
            this.install_twitch_outcome(outcome, cx);
            let dev = this.device().unwrap();
            (dev.phase, dev.error.clone())
        });

        assert_eq!(
            (phase, error),
            (
                TwitchDevicePhase::Failed,
                Some(credentials_missing(PlatformId::Twitch))
            ),
            "a sign-in with no install seed must fail visibly instead of building a bundle"
        );
    }
}
