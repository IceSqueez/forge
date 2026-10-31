use std::sync::Arc;

use forge_components::{ActivePresentation, Icon, TextInput, icon, scaled, tr};
use forge_events::EventPublisher;
use forge_storage::SettingDisclosure::Withheld;
use forge_storage::SettingPortability::Portable;
use forge_storage::{CredentialsRepo, Flag, Setting, SettingsRepo, write_setting};
use forge_types::Secret;
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, Focusable, Pixels, Subscription, Window,
    div, prelude::*, px,
};

use crate::async_bridge;
use crate::connect_scaffold::{
    AttemptStatus, BOX_PAD_H, ConnectFlag, ConnectFlagSpec, ConnectFlags, EndpointFields,
    FIELD_BLOCK_GAP, FormButtons, StripLook, endpoint_input, endpoint_row, field_label, flag_rows,
    form_buttons, input_box, persist_flag, read_endpoint,
};
use crate::integrations::ObsInstallSeed;

pub(crate) const OBS_AUTO_RECONNECT: Setting<Flag> = Setting::new(
    "obs.auto_reconnect",
    Flag { default: true },
    Portable,
    Withheld,
);

pub(crate) const OBS_CONNECT_ON_LAUNCH: Setting<Flag> = Setting::new(
    "obs.connect_on_launch",
    Flag { default: true },
    Portable,
    Withheld,
);

pub(crate) const DEFAULT_HOST: &str = "localhost";
pub(crate) const DEFAULT_PORT: u16 = 4455;

const EYE_GLYPH: Pixels = px(13.0);

const BANNER_LOOK: StripLook = StripLook {
    gap: px(9.0),
    pad_v: px(8.0),
    pad_h: px(11.0),
};

const FLAGS: ConnectFlagSpec = ConnectFlagSpec {
    auto_reconnect: &OBS_AUTO_RECONNECT,
    connect_on_launch: &OBS_CONNECT_ON_LAUNCH,
    auto_reconnect_id: "obs-connect-auto-reconnect",
    connect_on_launch_id: "obs-connect-on-launch",
    auto_reconnect_label_key: "obs_connect_auto_reconnect_label",
    auto_reconnect_hint_key: "obs_connect_auto_reconnect_hint",
    connect_on_launch_label_key: "obs_connect_on_launch_label",
    connect_on_launch_hint_key: "obs_connect_on_launch_hint",
    save_failed_key: "obs_connect_settings_save_failed",
};

pub(crate) struct ObsConnected;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ObsSubmit {
    Connect,
    SaveAndReconnect,
}

impl ObsSubmit {
    fn glyph(self) -> Icon {
        match self {
            ObsSubmit::Connect => Icon::Plug,
            ObsSubmit::SaveAndReconnect => Icon::Refresh,
        }
    }

    fn label(self) -> String {
        match self {
            ObsSubmit::Connect => tr!("obs_connect_btn_connect"),
            ObsSubmit::SaveAndReconnect => tr!("obs_connect_btn_save_reconnect"),
        }
    }
}

struct Prefill {
    host: String,
    port: String,
    password: Secret,
    flags: ConnectFlags,
}

struct ProbeReport {
    websocket_version: String,
    scene_count: usize,
    round_trip_ms: u64,
}

struct FormValues {
    host: String,
    port: u16,
    password: Secret,
}

pub(crate) struct ObsCredentialsForm {
    rt_handle: tokio::runtime::Handle,
    credentials: Arc<dyn CredentialsRepo>,
    settings: Arc<dyn SettingsRepo>,
    bus: Arc<dyn EventPublisher>,
    seed: ObsInstallSeed,
    submit: ObsSubmit,
    host: Entity<TextInput>,
    port: Entity<TextInput>,
    password: Entity<TextInput>,
    password_visible: bool,
    flags: ConnectFlags,
    banner: Option<AttemptStatus>,
    busy: bool,
    _subs: Vec<Subscription>,
}

impl EventEmitter<ObsConnected> for ObsCredentialsForm {}

impl ObsCredentialsForm {
    pub(crate) fn new(
        rt_handle: tokio::runtime::Handle,
        credentials: Arc<dyn CredentialsRepo>,
        settings: Arc<dyn SettingsRepo>,
        bus: Arc<dyn EventPublisher>,
        seed: ObsInstallSeed,
        submit: ObsSubmit,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let host = cx.new(|cx| endpoint_input(DEFAULT_HOST, palette, cx));
        let port = cx.new(|cx| endpoint_input(DEFAULT_PORT.to_string(), palette, cx));
        let password = cx.new(|cx| endpoint_input("", palette, cx).secure(true));

        let subs = vec![
            cx.observe(&host, |_, _, cx| cx.notify()),
            cx.observe(&port, |_, _, cx| cx.notify()),
            cx.observe(&password, |_, _, cx| cx.notify()),
        ];

        let mut form = Self {
            rt_handle,
            credentials,
            settings,
            bus,
            seed,
            submit,
            host,
            port,
            password,
            password_visible: false,
            flags: ConnectFlags::default(),
            banner: None,
            busy: false,
            _subs: subs,
        };
        form.load_prefill(cx);
        form
    }

    pub(crate) fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.host.update(cx, |input, cx| input.focus(window, cx));
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
        self.password.update(cx, |input, cx| {
            input.set_content(prefill.password.expose().to_owned(), cx)
        });
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
        let password = Secret::new(self.password.read(cx).content());

        match port {
            Some(port) => {
                self.port
                    .update(cx, |input, cx| input.set_invalid(false, cx));
                Some(FormValues {
                    host,
                    port,
                    password,
                })
            }
            None => {
                self.port
                    .update(cx, |input, cx| input.set_invalid(true, cx));
                self.banner = Some(AttemptStatus::Failure {
                    title: tr!("obs_connect_error_title"),
                    detail: tr!("obs_connect_error_invalid_port"),
                });
                cx.notify();
                None
            }
        }
    }

    fn on_toggle_password(&mut self, cx: &mut Context<Self>) {
        self.password_visible = !self.password_visible;
        let secure = !self.password_visible;
        self.password
            .update(cx, |input, cx| input.set_secure(secure, cx));
        cx.notify();
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
        if self.busy {
            return;
        }
        let Some(form) = self.read_form(cx) else {
            return;
        };
        self.busy = true;
        self.banner = Some(AttemptStatus::Busy(tr!("obs_connect_testing")));
        async_bridge::run_async(
            &self.rt_handle,
            async move { probe(form).await },
            |this, result, cx| this.apply_probe(result, cx),
            cx,
        );
        cx.notify();
    }

    fn apply_probe(&mut self, result: Result<ProbeReport, AttemptError>, cx: &mut Context<Self>) {
        self.busy = false;
        self.banner = Some(match result {
            Ok(report) => AttemptStatus::Success {
                title: tr!("obs_connect_test_successful"),
                detail: tr!(
                    "obs_connect_test_detail",
                    version = report.websocket_version,
                    scenes = i64::try_from(report.scene_count).unwrap_or(i64::MAX),
                    rtt = i64::try_from(report.round_trip_ms).unwrap_or(i64::MAX)
                ),
            },
            Err(error) => AttemptStatus::Failure {
                title: tr!("obs_connect_test_failed"),
                detail: error.describe(),
            },
        });
        cx.notify();
    }

    fn on_connect(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(form) = self.read_form(cx) else {
            return;
        };
        self.busy = true;
        self.banner = Some(AttemptStatus::Busy(tr!("obs_connect_connecting")));

        let credentials = Arc::clone(&self.credentials);
        let settings = Arc::clone(&self.settings);
        let bus = Arc::clone(&self.bus);
        let seed = self.seed.clone();
        let flags = self.flags;
        async_bridge::run_async(
            &self.rt_handle,
            async move { connect_obs(credentials, settings, bus, seed, form, flags).await },
            |this, result, cx| this.apply_connect(result, cx),
            cx,
        );
        cx.notify();
    }

    fn apply_connect(&mut self, result: Result<(), AttemptError>, cx: &mut Context<Self>) {
        self.busy = false;
        match result {
            Ok(()) => {
                self.banner = None;
                cx.emit(ObsConnected);
            }
            Err(error) => {
                self.banner = Some(AttemptStatus::Failure {
                    title: tr!("obs_connect_failed"),
                    detail: error.describe(),
                });
            }
        }
        cx.notify();
    }
}

impl Render for ObsCredentialsForm {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let density = cx.density();
        let palette = cx.palette();

        let password_focused = self.password.focus_handle(cx).is_focused(window);
        let endpoint = endpoint_row(
            EndpointFields {
                host_label: tr!("obs_connect_field_host"),
                port_label: tr!("obs_connect_field_port"),
                host: &self.host,
                port: &self.port,
                host_focused: self.host.focus_handle(cx).is_focused(window),
                port_focused: self.port.focus_handle(cx).is_focused(window),
            },
            &palette,
            density,
        );

        let eye_glyph = if self.password_visible {
            Icon::EyeOff
        } else {
            Icon::Eye
        };
        let eye = div()
            .id("obs-connect-password-eye")
            .flex_none()
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.on_toggle_password(cx)))
            .child(icon(eye_glyph, EYE_GLYPH, palette.text_faint));

        let password_field = div()
            .w_full()
            .flex()
            .flex_col()
            .mb(scaled(FIELD_BLOCK_GAP, density))
            .child(field_label(
                tr!("obs_connect_field_password"),
                Some(tr!("obs_connect_password_note")),
                &palette,
                density,
            ))
            .child(
                input_box(password_focused, &palette, density)
                    .gap(scaled(BOX_PAD_H, density))
                    .child(self.password.clone())
                    .child(eye),
            );

        let flag_rows = flag_rows(&FLAGS, self.flags, &palette, Self::on_toggle_flag, cx);
        let banner: Option<AnyElement> = self
            .banner
            .as_ref()
            .map(|status| status.strip("obs-connect-banner-spin", BANNER_LOOK, &palette, density));
        let buttons = form_buttons(
            FormButtons {
                test_id: "obs-connect-test",
                submit_id: "obs-connect-submit",
                test_label: tr!("obs_connect_btn_test"),
                submit_glyph: self.submit.glyph(),
                submit_label: self.submit.label(),
                test_disabled: self.busy,
                submit_disabled: self.busy,
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
            .child(password_field)
            .children(flag_rows)
            .children(banner)
            .child(buttons)
    }
}

async fn load_prefill(
    credentials: Arc<dyn CredentialsRepo>,
    settings: Arc<dyn SettingsRepo>,
) -> Prefill {
    let stored = match forge_obs::credentials::load(&*credentials).await {
        Ok(stored) => stored,
        Err(e) => {
            tracing::warn!(error = %e, "obs stored credentials could not be read");
            None
        }
    };
    let (host, port, password) = match stored {
        Some(cred) => {
            let (host, port) = forge_obs::parse_endpoint(&cred.url)
                .unwrap_or_else(|_| (DEFAULT_HOST.to_owned(), DEFAULT_PORT));
            (host, port, cred.password)
        }
        None => (DEFAULT_HOST.to_owned(), DEFAULT_PORT, Secret::new("")),
    };

    Prefill {
        host,
        port: port.to_string(),
        password,
        flags: ConnectFlags::load(&*settings, &FLAGS).await,
    }
}

enum AttemptError {
    Obs(forge_obs::ObsError),
    Other(String),
}

impl AttemptError {
    fn describe(&self) -> String {
        match self {
            Self::Obs(error) => {
                tracing::warn!(error = %error, "obs connection attempt failed");
                match error {
                    forge_obs::ObsError::Connect(_) => tr!("obs_connect_error_unreachable"),
                    forge_obs::ObsError::Authentication => tr!("obs_connect_error_auth"),
                    forge_obs::ObsError::Timeout => tr!("obs_connect_error_timeout"),
                    _ => tr!("obs_connect_error_unknown"),
                }
            }
            Self::Other(detail) => detail.clone(),
        }
    }
}

async fn probe(form: FormValues) -> Result<ProbeReport, AttemptError> {
    let result = forge_obs::probe_connection(&form.host, form.port, form.password.expose())
        .await
        .map_err(AttemptError::Obs)?;
    Ok(ProbeReport {
        websocket_version: result.obs_websocket_version,
        scene_count: result.scene_count,
        round_trip_ms: result.round_trip_ms,
    })
}

async fn connect_obs(
    credentials: Arc<dyn CredentialsRepo>,
    settings: Arc<dyn SettingsRepo>,
    bus: Arc<dyn EventPublisher>,
    seed: ObsInstallSeed,
    form: FormValues,
    flags: ConnectFlags,
) -> Result<(), AttemptError> {
    forge_obs::probe_connection(&form.host, form.port, form.password.expose())
        .await
        .map_err(AttemptError::Obs)?;

    forge_obs::credentials::store(&*credentials, &form.host, form.port, form.password.expose())
        .await
        .map_err(|e| AttemptError::Other(e.to_string()))?;
    write_setting(&*settings, &OBS_AUTO_RECONNECT, &flags.auto_reconnect)
        .await
        .map_err(|e| AttemptError::Other(e.to_string()))?;
    write_setting(&*settings, &OBS_CONNECT_ON_LAUNCH, &flags.connect_on_launch)
        .await
        .map_err(|e| AttemptError::Other(e.to_string()))?;

    seed.disconnect_live().await;

    let client = forge_obs::credentials::load_and_connect(&*credentials, bus)
        .await
        .map_err(|e| AttemptError::Other(e.to_string()))?;
    client.set_auto_reconnect(flags.auto_reconnect);
    seed.install(client);
    Ok(())
}
