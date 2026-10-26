use std::sync::Arc;

use forge_components::{
    ActivePresentation, ForgePalette, Icon, TextInput, body_family, pulse_dot, tr,
};
use forge_events::{Event, EventPublisher, EventSource};
use forge_runtime::EventBus;
use forge_storage::{CredentialsRepo, SettingsRepo};
use forge_vtube::{PLUGIN_NAME, VTubeClient, VTubeConfig, VTubeProbeResult};
use gpui::{
    AnyElement, Context, Entity, EventEmitter, Focusable, Pixels, Rgba, Subscription, Window, div,
    prelude::*, px,
};

use crate::async_bridge::{self, EventBatch};
use crate::connect_scaffold::{
    AttemptStatus, ConnectFlag, ConnectFlagSpec, ConnectFlags, EndpointFields, FIELD_BLOCK_GAP,
    FormButtons, StripContent, StripLook, box_frame, endpoint_input, endpoint_row, field_label,
    flag_rows, form_buttons, mono_box_text, outcome_strip, persist_flag, read_endpoint,
    strip_frame, strip_icon,
};
use crate::integrations::VTubeInstallSeed;

pub const VTUBE_AUTO_RECONNECT_KEY: &str =
    forge_storage::settings::reserved_keys::VTUBE_AUTO_RECONNECT;
pub const VTUBE_CONNECT_ON_LAUNCH_KEY: &str =
    forge_storage::settings::reserved_keys::VTUBE_CONNECT_ON_LAUNCH;

pub(crate) const DEFAULT_HOST: &str = "localhost";
pub(crate) const DEFAULT_PORT: u16 = 8001;

const CONNECTION_CHANGED: &str = "vtube.connection.changed";
const REASON_AWAITING_APPROVAL: &str = "awaiting_approval";

const IDLE_TEXT_SIZE: Pixels = px(11.0);
const ACTIVE_DOT: Pixels = px(8.0);

const IDLE_LOOK: StripLook = StripLook {
    gap: px(9.0),
    pad_v: px(8.0),
    pad_h: px(11.0),
};
const ACTIVE_LOOK: StripLook = StripLook {
    gap: px(10.0),
    pad_v: px(10.0),
    pad_h: px(12.0),
};

const FLAGS: ConnectFlagSpec = ConnectFlagSpec {
    auto_reconnect_key: VTUBE_AUTO_RECONNECT_KEY,
    connect_on_launch_key: VTUBE_CONNECT_ON_LAUNCH_KEY,
    auto_reconnect_id: "vtube-connect-auto-reconnect",
    connect_on_launch_id: "vtube-connect-on-launch",
    auto_reconnect_label_key: "vtube_connect_auto_reconnect_label",
    auto_reconnect_hint_key: "vtube_connect_auto_reconnect_hint",
    connect_on_launch_label_key: "vtube_connect_on_launch_label",
    connect_on_launch_hint_key: "vtube_connect_on_launch_hint",
    save_failed_key: "vtube_connect_settings_save_failed",
};

pub struct VTubeConnected;

struct Prefill {
    host: String,
    port: String,
    flags: ConnectFlags,
}

struct FormValues {
    host: String,
    port: u16,
}

enum Status {
    Idle,
    AwaitingApproval,
    Attempt(AttemptStatus),
}

enum ConnectOutcome {
    AwaitingApproval,
    Connected,
    Failed(String),
}

pub struct VTubeConnectForm {
    rt_handle: tokio::runtime::Handle,
    credentials: Arc<dyn CredentialsRepo>,
    settings: Arc<dyn SettingsRepo>,
    bus: Arc<dyn EventPublisher>,
    event_bus: Arc<EventBus>,
    seed: VTubeInstallSeed,
    host: Entity<TextInput>,
    port: Entity<TextInput>,
    flags: ConnectFlags,
    status: Status,
    testing: bool,
    connecting: bool,
    _subs: Vec<Subscription>,
}

impl EventEmitter<VTubeConnected> for VTubeConnectForm {}

impl VTubeConnectForm {
    pub fn new(
        rt_handle: tokio::runtime::Handle,
        credentials: Arc<dyn CredentialsRepo>,
        settings: Arc<dyn SettingsRepo>,
        bus: Arc<dyn EventPublisher>,
        event_bus: Arc<EventBus>,
        seed: VTubeInstallSeed,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let host = cx.new(|cx| endpoint_input(DEFAULT_HOST, palette, cx));
        let port = cx.new(|cx| endpoint_input(DEFAULT_PORT.to_string(), palette, cx));

        let subs = vec![
            cx.observe(&host, |_, _, cx| cx.notify()),
            cx.observe(&port, |_, _, cx| cx.notify()),
        ];

        let mut form = Self {
            rt_handle,
            credentials,
            settings,
            bus,
            event_bus,
            seed,
            host,
            port,
            flags: ConnectFlags::default(),
            status: Status::Idle,
            testing: false,
            connecting: false,
            _subs: subs,
        };
        form.load_prefill(cx);
        form
    }

    fn busy(&self) -> bool {
        self.testing || self.connecting
    }

    fn load_prefill(&mut self, cx: &mut Context<Self>) {
        let credentials = Arc::clone(&self.credentials);
        let settings = Arc::clone(&self.settings);
        async_bridge::run_async(
            &self.rt_handle,
            async move { load_prefill(credentials, settings).await },
            |this, prefill, cx| this.apply_prefill(prefill, cx),
            cx,
        );
    }

    fn apply_prefill(&mut self, prefill: Prefill, cx: &mut Context<Self>) {
        self.host
            .update(cx, |input, cx| input.set_content(prefill.host, cx));
        self.port
            .update(cx, |input, cx| input.set_content(prefill.port, cx));
        self.flags = prefill.flags;
        cx.notify();
    }

    fn read_form(&mut self, cx: &mut Context<Self>) -> Option<FormValues> {
        let (host, port) = read_endpoint(
            self.host.read(cx).content(),
            self.port.read(cx).content(),
            DEFAULT_HOST,
            DEFAULT_PORT,
        );

        match port {
            Some(port) => {
                self.port
                    .update(cx, |input, cx| input.set_invalid(false, cx));
                Some(FormValues { host, port })
            }
            None => {
                self.port
                    .update(cx, |input, cx| input.set_invalid(true, cx));
                self.status = Status::Attempt(AttemptStatus::Failure {
                    title: tr!("vtube_connect_error_title"),
                    detail: tr!("vtube_connect_error_invalid_port"),
                });
                cx.notify();
                None
            }
        }
    }

    fn on_toggle_flag(&mut self, flag: ConnectFlag, cx: &mut Context<Self>) {
        let value = self.flags.flip(flag);
        if flag == ConnectFlag::AutoReconnect
            && let Some(client) = self.seed.live()
        {
            client.set_auto_reconnect(value);
        }
        persist_flag(&self.rt_handle, &self.settings, &FLAGS, flag, value, cx);
        cx.notify();
    }

    fn on_test(&mut self, cx: &mut Context<Self>) {
        if self.busy() {
            return;
        }
        let Some(form) = self.read_form(cx) else {
            return;
        };
        self.testing = true;
        self.status = Status::Attempt(AttemptStatus::Busy(tr!("vtube_connect_testing")));
        async_bridge::run_async(
            &self.rt_handle,
            async move { forge_vtube::probe_connection(&form.host, form.port).await },
            |this, result, cx| this.apply_probe(result, cx),
            cx,
        );
        cx.notify();
    }

    fn apply_probe(
        &mut self,
        result: Result<VTubeProbeResult, forge_vtube::VTubeError>,
        cx: &mut Context<Self>,
    ) {
        self.testing = false;
        self.status = Status::Attempt(match result {
            Ok(report) => AttemptStatus::Success {
                title: tr!("vtube_connect_test_successful"),
                detail: tr!(
                    "vtube_connect_test_detail",
                    version = report.api_version,
                    rtt = report.round_trip_ms as i64,
                    auth = authorization_label(report.already_authenticated)
                ),
            },
            Err(error) => AttemptStatus::Failure {
                title: tr!("vtube_connect_test_failed"),
                detail: friendly_vtube_error(&error),
            },
        });
        cx.notify();
    }

    fn on_connect(&mut self, cx: &mut Context<Self>) {
        if self.busy() {
            return;
        }
        let Some(form) = self.read_form(cx) else {
            return;
        };
        self.connecting = true;
        self.status = Status::Attempt(AttemptStatus::Busy(tr!("vtube_connect_connecting")));

        let config = VTubeConfig {
            endpoint: format!("ws://{}:{}/", form.host, form.port),
        };
        let bus = Arc::clone(&self.bus);
        let credentials = Arc::clone(&self.credentials);
        let mut sub = self.event_bus.subscribe();
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let client = Arc::new(VTubeClient::connect(config, bus, credentials));
            client.set_auto_reconnect(false);
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the view may close before the result arrives"
            )]
            let _ = tx.send(client);
        });

        cx.spawn(async move |this, cx| {
            let Ok(client) = rx.await else {
                return;
            };
            while let EventBatch::Ready(batch) = async_bridge::recv_event_batch(&mut sub).await {
                let Some(outcome) = batch.iter().find_map(connect_outcome) else {
                    continue;
                };
                let settled = this.update(cx, |this, cx| {
                    this.apply_outcome(outcome, Arc::clone(&client), cx)
                });
                if !matches!(settled, Ok(false)) {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    fn apply_outcome(
        &mut self,
        outcome: ConnectOutcome,
        client: Arc<VTubeClient>,
        cx: &mut Context<Self>,
    ) -> bool {
        let settled = match outcome {
            ConnectOutcome::AwaitingApproval => {
                self.status = Status::AwaitingApproval;
                false
            }
            ConnectOutcome::Connected => {
                client.set_auto_reconnect(self.flags.auto_reconnect);
                self.seed.install(client);
                self.connecting = false;
                self.status = Status::Idle;
                persist_flag(
                    &self.rt_handle,
                    &self.settings,
                    &FLAGS,
                    ConnectFlag::AutoReconnect,
                    self.flags.auto_reconnect,
                    cx,
                );
                persist_flag(
                    &self.rt_handle,
                    &self.settings,
                    &FLAGS,
                    ConnectFlag::ConnectOnLaunch,
                    self.flags.connect_on_launch,
                    cx,
                );
                cx.emit(VTubeConnected);
                true
            }
            ConnectOutcome::Failed(reason) => {
                self.rt_handle.spawn(async move { client.shutdown().await });
                self.connecting = false;
                self.status = Status::Attempt(AttemptStatus::Failure {
                    title: tr!("vtube_connect_failed"),
                    detail: failure_detail(&reason),
                });
                true
            }
        };
        cx.notify();
        settled
    }

    fn status_strip(&self, palette: &ForgePalette) -> AnyElement {
        match &self.status {
            Status::Idle => idle_strip(palette),
            Status::AwaitingApproval => outcome_strip(
                StripContent {
                    accent: palette.warning,
                    indicator: pulsing_dot(palette.warning),
                    title: tr!("vtube_connect_awaiting_title"),
                    detail: Some((tr!("vtube_connect_awaiting_hint"), palette.text_faint)),
                },
                ACTIVE_LOOK,
                palette,
            ),
            Status::Attempt(attempt) => {
                attempt.strip("vtube-connect-status-spin", ACTIVE_LOOK, palette)
            }
        }
    }
}

impl Render for VTubeConnectForm {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();

        let endpoint = endpoint_row(
            EndpointFields {
                host_label: tr!("vtube_connect_field_host"),
                port_label: tr!("vtube_connect_field_port"),
                host: &self.host,
                port: &self.port,
                host_focused: self.host.focus_handle(cx).is_focused(window),
                port_focused: self.port.focus_handle(cx).is_focused(window),
            },
            &palette,
        );

        let plugin_field = div()
            .w_full()
            .flex()
            .flex_col()
            .mb(FIELD_BLOCK_GAP)
            .child(field_label(
                tr!("vtube_connect_field_plugin"),
                None,
                &palette,
            ))
            .child(
                box_frame(&palette)
                    .border_color(palette.border_input)
                    .child(mono_box_text(PLUGIN_NAME, &palette)),
            );

        let flag_rows = flag_rows(&FLAGS, self.flags, &palette, Self::on_toggle_flag, cx);
        let submit_label = if self.connecting {
            tr!("vtube_connect_btn_authorizing")
        } else {
            tr!("vtube_connect_btn_connect")
        };
        let buttons = form_buttons(
            FormButtons {
                test_id: "vtube-connect-test",
                submit_id: "vtube-connect-submit",
                test_label: tr!("vtube_connect_btn_test"),
                submit_glyph: Icon::Plug,
                submit_label,
                disabled: self.busy(),
            },
            &palette,
            Self::on_test,
            Self::on_connect,
            cx,
        );

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(endpoint)
            .child(plugin_field)
            .children(flag_rows)
            .child(self.status_strip(&palette))
            .child(buttons)
    }
}

fn idle_strip(palette: &ForgePalette) -> AnyElement {
    strip_frame(IDLE_LOOK, palette)
        .border_color(palette.border_regular)
        .child(strip_icon(Icon::Clock, palette.text_faint))
        .child(
            div()
                .font_family(body_family())
                .text_size(IDLE_TEXT_SIZE)
                .text_color(palette.text_muted)
                .child(tr!("vtube_connect_idle_hint")),
        )
        .into_any_element()
}

fn pulsing_dot(tint: Rgba) -> AnyElement {
    pulse_dot("vtube-connect-awaiting-pulse", tint, ACTIVE_DOT).into_any_element()
}

fn authorization_label(already_authenticated: bool) -> String {
    if already_authenticated {
        tr!("vtube_connect_test_authorized")
    } else {
        tr!("vtube_connect_test_unauthorized")
    }
}

fn connect_outcome(event: &Event) -> Option<ConnectOutcome> {
    if event.source != EventSource::VTube || event.kind != CONNECTION_CHANGED {
        return None;
    }
    if event
        .payload
        .get("is_connected")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return Some(ConnectOutcome::Connected);
    }
    match event.payload.get("reason").and_then(|v| v.as_str()) {
        Some(REASON_AWAITING_APPROVAL) => Some(ConnectOutcome::AwaitingApproval),
        Some(reason) => Some(ConnectOutcome::Failed(reason.to_owned())),
        None => None,
    }
}

fn friendly_vtube_error(error: &forge_vtube::VTubeError) -> String {
    tracing::warn!(error = %error, "vtube probe failed");
    match error {
        forge_vtube::VTubeError::Connect(_) => failure_detail("connect_failed"),
        forge_vtube::VTubeError::TokenRejected => failure_detail("auth_required"),
        forge_vtube::VTubeError::TokenDenied => failure_detail("auth_denied"),
        forge_vtube::VTubeError::TokenTimeout => failure_detail("auth_timeout"),
        _ => failure_detail(""),
    }
}

fn failure_detail(reason: &str) -> String {
    match reason {
        "connect_failed" => tr!("vtube_connect_error_unreachable"),
        "auth_required" => tr!("vtube_connect_error_token_rejected"),
        "auth_denied" => tr!("vtube_connect_error_denied"),
        "auth_timeout" => tr!("vtube_connect_error_timeout"),
        "auth_failed" => tr!("vtube_connect_error_auth"),
        "subscribe_failed" => tr!("vtube_connect_error_subscribe"),
        "socket_closed" => tr!("vtube_connect_error_socket_closed"),
        "unresponsive" => tr!("vtube_connect_error_unresponsive"),
        _ => tr!("vtube_connect_error_unknown"),
    }
}

async fn load_prefill(
    credentials: Arc<dyn CredentialsRepo>,
    settings: Arc<dyn SettingsRepo>,
) -> Prefill {
    let stored = match forge_vtube::credentials::load(&*credentials).await {
        Ok(stored) => stored,
        Err(e) => {
            tracing::warn!(error = %e, "vtube stored credentials could not be read");
            None
        }
    };
    let (host, port) = match stored {
        Some(cred) => (cred.host, cred.port),
        None => (DEFAULT_HOST.to_owned(), DEFAULT_PORT),
    };

    Prefill {
        host,
        port: port.to_string(),
        flags: ConnectFlags::load(&*settings, &FLAGS).await,
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn outcome_tag(outcome: Option<ConnectOutcome>) -> String {
        match outcome {
            None => "ignored".to_owned(),
            Some(ConnectOutcome::Connected) => "connected".to_owned(),
            Some(ConnectOutcome::AwaitingApproval) => "awaiting".to_owned(),
            Some(ConnectOutcome::Failed(reason)) => format!("failed:{reason}"),
        }
    }

    #[test]
    fn each_connection_payload_resolves_to_a_phase_the_connect_screen_can_show() {
        for (payload, expected) in [
            (
                serde_json::json!({ "is_connected": true, "reason": null }),
                "connected",
            ),
            (
                serde_json::json!({ "is_connected": false, "reason": "awaiting_approval" }),
                "awaiting",
            ),
            (
                serde_json::json!({ "is_connected": false, "reason": "auth_denied" }),
                "failed:auth_denied",
            ),
            (
                serde_json::json!({ "is_connected": false, "reason": "connect_failed" }),
                "failed:connect_failed",
            ),
            (
                serde_json::json!({ "is_connected": false, "reason": null }),
                "ignored",
            ),
        ] {
            let event = Event::new(EventSource::VTube, CONNECTION_CHANGED, payload.clone());
            assert_eq!(
                outcome_tag(connect_outcome(&event)),
                expected,
                "unexpected phase for {payload}"
            );
        }
    }

    #[test]
    fn events_from_another_source_or_kind_are_ignored() {
        let foreign_source = Event::new(
            EventSource::Obs,
            CONNECTION_CHANGED,
            serde_json::json!({ "is_connected": true }),
        );
        let foreign_kind = Event::new(
            EventSource::VTube,
            "vtube.model.loaded",
            serde_json::json!({ "is_connected": true }),
        );

        assert_eq!(outcome_tag(connect_outcome(&foreign_source)), "ignored");
        assert_eq!(outcome_tag(connect_outcome(&foreign_kind)), "ignored");
    }

    #[test]
    fn every_failure_reason_the_vtube_client_reports_gets_its_own_translated_message() {
        const REASONS: [&str; 8] = [
            "connect_failed",
            "auth_required",
            "auth_denied",
            "auth_timeout",
            "auth_failed",
            "subscribe_failed",
            "socket_closed",
            "unresponsive",
        ];
        for lang in [forge_storage::Language::En, forge_storage::Language::Uk] {
            crate::i18n::install_language(lang);
            let unknown = failure_detail("something_new");
            let details = REASONS.map(failure_detail);

            for (reason, detail) in REASONS.iter().zip(&details) {
                assert!(
                    *detail != unknown && !detail.starts_with("vtube_connect_error_"),
                    "{lang:?} {reason} falls back to {detail:?}"
                );
            }
            let distinct: std::collections::HashSet<&String> = details.iter().collect();
            assert_eq!(distinct.len(), REASONS.len(), "{lang:?} {details:?}");
        }
    }
}
