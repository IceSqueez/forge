use forge_components::FONT_META;
use std::sync::Arc;

use forge_components::{
    ActivePresentation, Density, ForgePalette, Icon, TextInput, body_family, pulse_dot, scaled, tr,
};
use forge_events::EventPublisher;
use forge_storage::{CredentialsRepo, Flag, Setting, SettingsRepo};
use forge_vtube::{
    PLUGIN_NAME, VTubeAttemptPhase, VTubeClient, VTubeConfig, VTubeConnectAttempt, VTubeProbeResult,
};
use gpui::{
    AnyElement, AsyncApp, Context, Entity, EventEmitter, Focusable, Pixels, Rgba, Subscription,
    Task, WeakEntity, Window, div, prelude::*, px,
};

use crate::async_bridge;
use crate::connect_scaffold::endpoint_host_is_valid;
use crate::connect_scaffold::{
    AttemptStatus, ConnectFlag, ConnectFlagSpec, ConnectFlags, EndpointFields, FIELD_BLOCK_GAP,
    FormButtons, StripContent, StripLook, box_frame, endpoint_input, endpoint_row, field_label,
    flag_rows, form_buttons, mono_box_text, outcome_strip, persist_flag, read_endpoint,
    strip_frame, strip_icon,
};
use crate::integrations::VTubeInstallSeed;
use crate::runtime_event::VTubeDisconnect;

pub(crate) const VTUBE_AUTO_RECONNECT: Setting<Flag> = Setting::new(
    "vtube.auto_reconnect",
    Flag { default: true },
    forge_storage::SettingPortability::Portable,
    forge_storage::SettingDisclosure::Withheld,
);

pub(crate) const VTUBE_CONNECT_ON_LAUNCH: Setting<Flag> = Setting::new(
    "vtube.connect_on_launch",
    Flag { default: true },
    forge_storage::SettingPortability::Portable,
    forge_storage::SettingDisclosure::Withheld,
);

pub(crate) const DEFAULT_HOST: &str = "localhost";
pub(crate) const DEFAULT_PORT: u16 = 8001;

const IDLE_TEXT_SIZE: Pixels = FONT_META;
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
    auto_reconnect: &VTUBE_AUTO_RECONNECT,
    connect_on_launch: &VTUBE_CONNECT_ON_LAUNCH,
    auto_reconnect_id: "vtube-connect-auto-reconnect",
    connect_on_launch_id: "vtube-connect-on-launch",
    auto_reconnect_label_key: "vtube_connect_auto_reconnect_label",
    auto_reconnect_hint_key: "vtube_connect_auto_reconnect_hint",
    connect_on_launch_label_key: "vtube_connect_on_launch_label",
    connect_on_launch_hint_key: "vtube_connect_on_launch_hint",
    save_failed_key: "vtube_connect_settings_save_failed",
};

pub(crate) struct VTubeConnected;

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

pub(crate) struct VTubeConnectForm {
    rt_handle: tokio::runtime::Handle,
    credentials: Arc<dyn CredentialsRepo>,
    settings: Arc<dyn SettingsRepo>,
    bus: Arc<dyn EventPublisher>,
    seed: VTubeInstallSeed,
    host: Entity<TextInput>,
    port: Entity<TextInput>,
    flags: ConnectFlags,
    status: Status,
    testing: bool,
    attempt: Option<Task<gpui::Result<()>>>,
    _subs: Vec<Subscription>,
}

impl EventEmitter<VTubeConnected> for VTubeConnectForm {}

impl VTubeConnectForm {
    pub(crate) fn new(
        rt_handle: tokio::runtime::Handle,
        credentials: Arc<dyn CredentialsRepo>,
        settings: Arc<dyn SettingsRepo>,
        bus: Arc<dyn EventPublisher>,
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
            seed,
            host,
            port,
            flags: ConnectFlags::default(),
            status: Status::Idle,
            testing: false,
            attempt: None,
            _subs: subs,
        };
        form.load_prefill(cx);
        form
    }

    fn busy(&self) -> bool {
        self.testing || self.attempt.is_some()
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

        let host_valid = endpoint_host_is_valid(&host);
        self.host
            .update(cx, |input, cx| input.set_invalid(!host_valid, cx));
        if !host_valid {
            self.status = Status::Attempt(AttemptStatus::Failure {
                title: tr!("vtube_connect_error_title"),
                detail: tr!("vtube_connect_error_invalid_host"),
            });
            cx.notify();
            return None;
        }

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
                    rtt = i64::try_from(report.round_trip_ms).unwrap_or(i64::MAX),
                    auth = authorization_label(report.already_authenticated)
                ),
            },
            Err(error) => {
                tracing::warn!(error = %error, "vtube probe failed");
                AttemptStatus::Failure {
                    title: tr!("vtube_connect_test_failed"),
                    detail: failure_detail(VTubeDisconnect::from(&error)),
                }
            }
        });
        cx.notify();
    }

    fn on_submit(&mut self, cx: &mut Context<Self>) {
        if self.attempt.is_some() {
            self.cancel_attempt(cx);
        } else {
            self.on_connect(cx);
        }
    }

    fn cancel_attempt(&mut self, cx: &mut Context<Self>) {
        self.attempt = None;
        self.status = Status::Idle;
        cx.notify();
    }

    fn on_connect(&mut self, cx: &mut Context<Self>) {
        if self.busy() {
            return;
        }
        let Some(form) = self.read_form(cx) else {
            return;
        };
        self.status = Status::Attempt(AttemptStatus::Busy(tr!("vtube_connect_connecting")));

        let config = VTubeConfig {
            endpoint: format!("ws://{}:{}/", form.host, form.port),
        };
        let attempt = {
            let _runtime = self.rt_handle.enter();
            VTubeConnectAttempt::begin(config, Arc::clone(&self.bus), Arc::clone(&self.credentials))
        };
        self.attempt =
            Some(cx.spawn(async move |this, cx| follow_attempt(attempt, this, cx).await));
        cx.notify();
    }

    fn show_awaiting_approval(&mut self, cx: &mut Context<Self>) {
        self.status = Status::AwaitingApproval;
        cx.notify();
    }

    fn adopt_client(&mut self, client: Arc<VTubeClient>, cx: &mut Context<Self>) {
        client.set_auto_reconnect(self.flags.auto_reconnect);
        self.seed.install(client);
        self.attempt = None;
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
        cx.notify();
    }

    fn show_failure(&mut self, reason: VTubeDisconnect, cx: &mut Context<Self>) {
        self.attempt = None;
        self.status = Status::Attempt(AttemptStatus::Failure {
            title: tr!("vtube_connect_failed"),
            detail: failure_detail(reason),
        });
        cx.notify();
    }

    fn status_strip(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        match &self.status {
            Status::Idle => idle_strip(palette, density),
            Status::AwaitingApproval => outcome_strip(
                StripContent {
                    accent: palette.warning,
                    indicator: pulsing_dot(palette.warning),
                    title: tr!("vtube_connect_awaiting_title"),
                    detail: Some((tr!("vtube_connect_awaiting_hint"), palette.text_faint)),
                },
                ACTIVE_LOOK,
                palette,
                density,
            ),
            Status::Attempt(attempt) => {
                attempt.strip("vtube-connect-status-spin", ACTIVE_LOOK, palette, density)
            }
        }
    }
}

impl Render for VTubeConnectForm {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let density = cx.density();
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
            density,
        );

        let plugin_field = div()
            .w_full()
            .flex()
            .flex_col()
            .mb(scaled(FIELD_BLOCK_GAP, density))
            .child(field_label(
                tr!("vtube_connect_field_plugin"),
                None,
                &palette,
                density,
            ))
            .child(
                box_frame(&palette, density)
                    .border_color(palette.border_input)
                    .child(mono_box_text(PLUGIN_NAME, &palette)),
            );

        let flag_rows = flag_rows(&FLAGS, self.flags, &palette, Self::on_toggle_flag, cx);
        let (submit_glyph, submit_label) = if self.attempt.is_some() {
            (Icon::X, tr!("common_cancel"))
        } else {
            (Icon::Plug, tr!("vtube_connect_btn_connect"))
        };
        let buttons = form_buttons(
            FormButtons {
                test_id: "vtube-connect-test",
                submit_id: "vtube-connect-submit",
                test_label: tr!("vtube_connect_btn_test"),
                submit_glyph,
                submit_label,
                test_disabled: self.busy(),
                submit_disabled: self.testing,
            },
            &palette,
            Self::on_test,
            Self::on_submit,
            cx,
        );

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(endpoint)
            .child(plugin_field)
            .children(flag_rows)
            .child(self.status_strip(&palette, density))
            .child(buttons)
    }
}

fn idle_strip(palette: &ForgePalette, density: Density) -> AnyElement {
    strip_frame(IDLE_LOOK, palette, density)
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

async fn follow_attempt(
    mut attempt: VTubeConnectAttempt,
    this: WeakEntity<VTubeConnectForm>,
    cx: &mut AsyncApp,
) -> gpui::Result<()> {
    loop {
        match attempt.next_phase().await {
            VTubeAttemptPhase::Connecting => {}
            VTubeAttemptPhase::AwaitingApproval => {
                this.update(cx, |form, cx| form.show_awaiting_approval(cx))?;
            }
            VTubeAttemptPhase::Connected => {
                return this.update(cx, |form, cx| form.adopt_client(attempt.into_client(), cx));
            }
            VTubeAttemptPhase::Failed(reason) => {
                return this.update(cx, |form, cx| form.show_failure(reason.into(), cx));
            }
            VTubeAttemptPhase::TimedOut => {
                return this.update(cx, |form, cx| {
                    form.show_failure(VTubeDisconnect::NoAnswer, cx);
                });
            }
        }
    }
}

fn failure_detail(reason: VTubeDisconnect) -> String {
    match reason {
        VTubeDisconnect::Unreachable => tr!("vtube_connect_error_unreachable"),
        VTubeDisconnect::TokenRejected => tr!("vtube_connect_error_token_rejected"),
        VTubeDisconnect::Denied => tr!("vtube_connect_error_denied"),
        VTubeDisconnect::TimedOut => tr!("vtube_connect_error_timeout"),
        VTubeDisconnect::AuthFailed => tr!("vtube_connect_error_auth"),
        VTubeDisconnect::SubscribeFailed => tr!("vtube_connect_error_subscribe"),
        VTubeDisconnect::SocketClosed => tr!("vtube_connect_error_socket_closed"),
        VTubeDisconnect::Unresponsive => tr!("vtube_connect_error_unresponsive"),
        VTubeDisconnect::NoAnswer => tr!("vtube_connect_error_no_answer"),
        VTubeDisconnect::AwaitingApproval | VTubeDisconnect::Unknown => {
            tr!("vtube_connect_error_unknown")
        }
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
    use std::sync::Mutex;

    use forge_events::{Event, TypedEvent};
    use forge_vtube::VTubeError;
    use forge_vtube::vocabulary::{VTubeConnectionChanged, VTubeDisconnectReason};
    use gpui::TestAppContext;

    use super::*;
    use crate::test_support::{install_presentation, pump, pump_until, runtime, test_backend};

    #[derive(Default)]
    struct RecordingBus(Mutex<Vec<Event>>);

    impl RecordingBus {
        fn saw_client_stop(&self) -> bool {
            self.0.lock().unwrap().iter().any(|event| {
                VTubeConnectionChanged::from_event(event)
                    .is_some_and(|changed| !changed.is_connected && changed.reason.is_none())
            })
        }
    }

    impl EventPublisher for RecordingBus {
        fn publish(&self, event: Event) {
            self.0.lock().unwrap().push(event);
        }
    }

    struct Mounted {
        form: Entity<VTubeConnectForm>,
        bus: Arc<RecordingBus>,
        rt: tokio::runtime::Runtime,
        _silent_vts: std::net::TcpListener,
    }

    fn mounted_form(cx: &mut TestAppContext) -> Mounted {
        install_presentation(cx);
        let rt = runtime();
        let silent_vts = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = silent_vts.local_addr().unwrap().port();
        let (backend, _writes) = test_backend();
        let bus = Arc::new(RecordingBus::default());
        let form = cx.new(|cx| {
            VTubeConnectForm::new(
                rt.handle().clone(),
                Arc::clone(&backend) as Arc<dyn CredentialsRepo>,
                backend as Arc<dyn SettingsRepo>,
                Arc::clone(&bus) as Arc<dyn EventPublisher>,
                VTubeInstallSeed::new(forge_vtube::SwitchableVTubeSink::new()),
                cx,
            )
        });
        pump(&rt);
        cx.run_until_parked();
        form.update(cx, |form, cx| {
            form.port
                .update(cx, |input, cx| input.set_content(port.to_string(), cx));
        });
        Mounted {
            form,
            bus,
            rt,
            _silent_vts: silent_vts,
        }
    }

    fn running_attempt(mounted: &Mounted, cx: &mut TestAppContext) {
        mounted.form.update(cx, |form, cx| form.on_submit(cx));
        assert!(mounted.form.read_with(cx, |form, _| form.attempt.is_some()));
        pump(&mounted.rt);
    }

    #[gpui::test]
    fn cancel_returns_the_form_to_idle_and_stops_the_attempt(cx: &mut TestAppContext) {
        let mounted = mounted_form(cx);
        running_attempt(&mounted, cx);
        assert!(!mounted.bus.saw_client_stop());

        mounted.form.update(cx, |form, cx| form.on_submit(cx));
        cx.run_until_parked();

        mounted.form.read_with(cx, |form, _| {
            assert!(form.attempt.is_none());
            assert!(matches!(form.status, Status::Idle));
        });
        pump_until(&mounted.rt, || mounted.bus.saw_client_stop());
    }

    #[gpui::test]
    fn closing_the_form_stops_the_running_attempt(cx: &mut TestAppContext) {
        let mounted = mounted_form(cx);
        running_attempt(&mounted, cx);
        assert!(!mounted.bus.saw_client_stop());
        let Mounted {
            form,
            bus,
            rt,
            _silent_vts,
        } = mounted;

        drop(form);
        cx.update(|_| {});
        cx.run_until_parked();

        pump_until(&rt, || bus.saw_client_stop());
    }

    #[gpui::test]
    fn a_host_that_is_not_a_host_is_refused_before_any_attempt(cx: &mut TestAppContext) {
        crate::i18n::install_language(forge_storage::Language::En);
        for host in ["ws://localhost", "local host", "::1", "user@host"] {
            let mounted = mounted_form(cx);
            mounted.form.update(cx, |form, cx| {
                form.host
                    .update(cx, |input, cx| input.set_content(host, cx));
            });

            mounted.form.update(cx, |form, cx| form.on_submit(cx));

            mounted.form.read_with(cx, |form, _| {
                let refused = matches!(
                    &form.status,
                    Status::Attempt(AttemptStatus::Failure { detail, .. })
                        if *detail == tr!("vtube_connect_error_invalid_host")
                );
                assert!(form.attempt.is_none() && refused, "{host:?}");
            });
        }
    }

    #[gpui::test]
    fn a_bracketed_ipv6_host_passes_the_host_check(cx: &mut TestAppContext) {
        crate::i18n::install_language(forge_storage::Language::En);
        let mounted = mounted_form(cx);
        mounted.form.update(cx, |form, cx| {
            form.host
                .update(cx, |input, cx| input.set_content("[::1]", cx));
        });

        let values = mounted.form.update(cx, |form, cx| form.read_form(cx));

        assert_eq!(values.map(|values| values.host), Some("[::1]".to_owned()));
    }

    #[gpui::test]
    fn submitting_while_a_test_runs_starts_no_attempt(cx: &mut TestAppContext) {
        let mounted = mounted_form(cx);
        mounted.form.update(cx, |form, cx| form.on_test(cx));

        mounted.form.update(cx, |form, cx| form.on_submit(cx));

        mounted.form.read_with(cx, |form, _| {
            assert!(form.testing);
            assert!(form.attempt.is_none());
        });
    }

    #[test]
    fn every_failure_the_connect_screen_reports_gets_its_own_translated_message() {
        const REASONS: [VTubeDisconnectReason; 8] = [
            VTubeDisconnectReason::ConnectFailed,
            VTubeDisconnectReason::AuthRequired,
            VTubeDisconnectReason::AuthDenied,
            VTubeDisconnectReason::AuthTimeout,
            VTubeDisconnectReason::AuthFailed,
            VTubeDisconnectReason::SubscribeFailed,
            VTubeDisconnectReason::SocketClosed,
            VTubeDisconnectReason::Unresponsive,
        ];
        let failures: Vec<VTubeDisconnect> = REASONS
            .into_iter()
            .map(VTubeDisconnect::from)
            .chain([VTubeDisconnect::NoAnswer])
            .collect();
        for lang in [forge_storage::Language::En, forge_storage::Language::Uk] {
            crate::i18n::install_language(lang);
            let unknown = failure_detail(VTubeDisconnect::Unknown);
            let details: Vec<String> = failures.iter().copied().map(failure_detail).collect();

            for (failure, detail) in failures.iter().zip(&details) {
                assert!(
                    *detail != unknown && !detail.starts_with("vtube_connect_error_"),
                    "{lang:?} {failure:?} falls back to {detail:?}"
                );
            }
            let distinct: std::collections::HashSet<&String> = details.iter().collect();
            assert_eq!(distinct.len(), failures.len(), "{lang:?} {details:?}");
        }
    }

    #[test]
    fn a_test_that_times_out_reports_no_answer() {
        assert_eq!(
            VTubeDisconnect::from(&VTubeError::Timeout),
            VTubeDisconnect::NoAnswer
        );
    }
}
