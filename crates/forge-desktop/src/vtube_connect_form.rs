use std::sync::Arc;

use forge_components::{
    BORDER_THIN, FONT_XS, FONT_XXS, ForgePalette, Icon, Radius, TextInput, body_family, icon,
    mono_family, pulse_dot, radius, spinner, toggle, tr, with_alpha,
};
use forge_events::{Event, EventPublisher, EventSource};
use forge_runtime::EventBus;
use forge_storage::{CredentialsRepo, SettingsRepo, get_bool_setting, set_bool_setting};
use forge_vtube::{PLUGIN_NAME, VTubeClient, VTubeConfig, VTubeProbeResult};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, Focusable, FontWeight, Pixels, Rgba,
    SharedString, Subscription, Window, div, prelude::*, px,
};

use crate::async_bridge::{self, ErrorSink, EventBatch};
use crate::builtin_sections::grow_cell;
use crate::integrations::VTubeInstallSeed;
use crate::presentation::ActivePresentation;

pub const VTUBE_AUTO_RECONNECT_KEY: &str = "vtube.auto_reconnect";
pub const VTUBE_CONNECT_ON_LAUNCH_KEY: &str = "vtube.connect_on_launch";

pub(crate) const DEFAULT_HOST: &str = "localhost";
pub(crate) const DEFAULT_PORT: u16 = 8001;

const CONNECTION_CHANGED: &str = "vtube.connection.changed";
const REASON_AWAITING_APPROVAL: &str = "awaiting_approval";

const TOGGLE_GLYPH: Pixels = px(14.0);

const FIELD_LABEL_SIZE: Pixels = px(11.0);
const FIELD_LABEL_GAP: Pixels = px(6.0);
const FIELD_GAP: Pixels = px(10.0);
const FIELD_BLOCK_GAP: Pixels = px(12.0);
const BOX_PAD_V: Pixels = px(7.0);
const BOX_PAD_H: Pixels = px(11.0);
const BOX_RADIUS: Pixels = px(7.0);
const BOX_TEXT_SIZE: Pixels = px(12.0);

const TOGGLE_ROW_PAD_V: Pixels = px(8.0);
const TOGGLE_ROW_GAP: Pixels = px(10.0);
const TOGGLE_LABEL_SIZE: Pixels = px(12.5);
const TOGGLE_HINT_SIZE: Pixels = px(11.0);

const STRIP_MARGIN: Pixels = px(12.0);
const IDLE_PAD_V: Pixels = px(8.0);
const IDLE_PAD_H: Pixels = px(11.0);
const IDLE_GAP: Pixels = px(9.0);
const IDLE_GLYPH: Pixels = px(13.0);
const IDLE_TEXT_SIZE: Pixels = px(11.0);
const ACTIVE_PAD_V: Pixels = px(10.0);
const ACTIVE_PAD_H: Pixels = px(12.0);
const ACTIVE_GAP: Pixels = px(10.0);
const ACTIVE_DOT: Pixels = px(8.0);
const ACTIVE_TITLE_SIZE: Pixels = px(11.5);

const BUTTON_ROW_GAP: Pixels = px(8.0);
const BUTTON_PAD_V: Pixels = px(8.0);
const GHOST_PAD_H: Pixels = px(14.0);
const PRIMARY_PAD_H: Pixels = px(16.0);
const BUTTON_GAP: Pixels = px(5.0);
const BUTTON_GLYPH: Pixels = px(13.0);
const BUSY_OPACITY: f32 = 0.6;

const HOST_CELL_GROW: f32 = 16.0;
const PORT_CELL_GROW: f32 = 10.0;

pub struct VTubeConnected;

#[derive(Clone, Copy, PartialEq, Eq)]
enum VTubeFlag {
    AutoReconnect,
    ConnectOnLaunch,
}

impl VTubeFlag {
    fn key(self) -> &'static str {
        match self {
            VTubeFlag::AutoReconnect => VTUBE_AUTO_RECONNECT_KEY,
            VTubeFlag::ConnectOnLaunch => VTUBE_CONNECT_ON_LAUNCH_KEY,
        }
    }

    fn element_id(self) -> &'static str {
        match self {
            VTubeFlag::AutoReconnect => "vtube-connect-auto-reconnect",
            VTubeFlag::ConnectOnLaunch => "vtube-connect-on-launch",
        }
    }
}

struct ToggleRowSpec {
    glyph: Icon,
    tint: Rgba,
    label: String,
    hint: String,
    on: bool,
    flag: VTubeFlag,
    last: bool,
}

struct Prefill {
    host: String,
    port: String,
    auto_reconnect: bool,
    connect_on_launch: bool,
}

struct FormValues {
    host: String,
    port: u16,
}

enum Status {
    Idle,
    Busy(String),
    AwaitingApproval,
    Success { title: String, detail: String },
    Failure { title: String, detail: String },
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
    auto_reconnect: bool,
    connect_on_launch: bool,
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
        let host = cx.new(|cx| {
            TextInput::new(DEFAULT_HOST, cx)
                .plain()
                .mono()
                .with_palette(palette)
                .with_font_size(BOX_TEXT_SIZE)
        });
        let port = cx.new(|cx| {
            TextInput::new(SharedString::from(DEFAULT_PORT.to_string()), cx)
                .plain()
                .mono()
                .with_palette(palette)
                .with_font_size(BOX_TEXT_SIZE)
        });

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
            auto_reconnect: true,
            connect_on_launch: true,
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
        self.auto_reconnect = prefill.auto_reconnect;
        self.connect_on_launch = prefill.connect_on_launch;
        cx.notify();
    }

    fn read_form(&mut self, cx: &mut Context<Self>) -> Option<FormValues> {
        let host = self.host.read(cx).content().trim().to_owned();
        let host = if host.is_empty() {
            DEFAULT_HOST.to_owned()
        } else {
            host
        };
        let raw_port = self.port.read(cx).content().trim().to_owned();
        let port = if raw_port.is_empty() {
            Some(DEFAULT_PORT)
        } else {
            raw_port.parse::<u16>().ok().filter(|p| *p != 0)
        };

        match port {
            Some(port) => {
                self.port
                    .update(cx, |input, cx| input.set_invalid(false, cx));
                Some(FormValues { host, port })
            }
            None => {
                self.port
                    .update(cx, |input, cx| input.set_invalid(true, cx));
                self.status = Status::Failure {
                    title: tr!("vtube_connect_error_title"),
                    detail: tr!("vtube_connect_error_invalid_port"),
                };
                cx.notify();
                None
            }
        }
    }

    fn on_toggle_flag(&mut self, flag: VTubeFlag, cx: &mut Context<Self>) {
        let value = match flag {
            VTubeFlag::AutoReconnect => {
                self.auto_reconnect = !self.auto_reconnect;
                if let Some(client) = self.seed.live() {
                    client.set_auto_reconnect(self.auto_reconnect);
                }
                self.auto_reconnect
            }
            VTubeFlag::ConnectOnLaunch => {
                self.connect_on_launch = !self.connect_on_launch;
                self.connect_on_launch
            }
        };
        self.persist_flag(flag.key(), value, cx);
        cx.notify();
    }

    fn persist_flag(&self, key: &'static str, value: bool, cx: &mut Context<Self>) {
        let settings = Arc::clone(&self.settings);
        async_bridge::report_failure(
            &self.rt_handle,
            async move { set_bool_setting(&*settings, key, value).await },
            ErrorSink::Toast,
            tr!("vtube_connect_settings_save_failed"),
            cx,
        );
    }

    fn on_test(&mut self, cx: &mut Context<Self>) {
        if self.busy() {
            return;
        }
        let Some(form) = self.read_form(cx) else {
            return;
        };
        self.testing = true;
        self.status = Status::Busy(tr!("vtube_connect_testing"));
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                forge_vtube::probe_connection(&form.host, form.port)
                    .await
                    .map_err(|e| e.to_string())
            },
            |this, result, cx| this.apply_probe(result, cx),
            cx,
        );
        cx.notify();
    }

    fn apply_probe(&mut self, result: Result<VTubeProbeResult, String>, cx: &mut Context<Self>) {
        self.testing = false;
        self.status = match result {
            Ok(report) => Status::Success {
                title: tr!("vtube_connect_test_successful"),
                detail: tr!(
                    "vtube_connect_test_detail",
                    version = report.api_version,
                    rtt = report.round_trip_ms as i64,
                    auth = authorization_label(report.already_authenticated)
                ),
            },
            Err(error) => Status::Failure {
                title: tr!("vtube_connect_test_failed"),
                detail: error,
            },
        };
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
        self.status = Status::Busy(tr!("vtube_connect_connecting"));

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
                client.set_auto_reconnect(self.auto_reconnect);
                self.seed.install(client);
                self.connecting = false;
                self.status = Status::Idle;
                self.persist_flag(VTUBE_AUTO_RECONNECT_KEY, self.auto_reconnect, cx);
                self.persist_flag(VTUBE_CONNECT_ON_LAUNCH_KEY, self.connect_on_launch, cx);
                cx.emit(VTubeConnected);
                true
            }
            ConnectOutcome::Failed(reason) => {
                self.rt_handle.spawn(async move { client.shutdown().await });
                self.connecting = false;
                self.status = Status::Failure {
                    title: tr!("vtube_connect_failed"),
                    detail: failure_detail(&reason),
                };
                true
            }
        };
        cx.notify();
        settled
    }

    fn toggle_row(
        &self,
        spec: ToggleRowSpec,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let flag = spec.flag;
        let control = toggle(spec.on, palette).on_click(
            flag.element_id(),
            cx.listener(move |this, _: &ClickEvent, _, cx| this.on_toggle_flag(flag, cx)),
        );

        let labels = div()
            .flex()
            .flex_col()
            .child(
                div()
                    .font_family(body_family())
                    .text_size(TOGGLE_LABEL_SIZE)
                    .text_color(palette.text_primary)
                    .child(spec.label),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(TOGGLE_HINT_SIZE)
                    .text_color(palette.text_faint)
                    .child(spec.hint),
            );

        let mut row = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .py(TOGGLE_ROW_PAD_V)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(TOGGLE_ROW_GAP)
                    .child(icon(spec.glyph, TOGGLE_GLYPH, spec.tint))
                    .child(labels),
            )
            .child(control);
        if !spec.last {
            row = row
                .border_b(BORDER_THIN)
                .border_color(palette.border_regular);
        }
        row.into_any_element()
    }

    fn status_strip(&self, palette: &ForgePalette) -> AnyElement {
        let (accent, indicator, title, detail, detail_tint): (
            Rgba,
            AnyElement,
            String,
            Option<String>,
            Rgba,
        ) = match &self.status {
            Status::Idle => return self.idle_strip(palette),
            Status::Busy(label) => (
                palette.border_regular,
                spinner(
                    SharedString::from("vtube-connect-status-spin"),
                    Icon::Loader2,
                    IDLE_GLYPH,
                    palette.text_muted,
                )
                .into_any_element(),
                label.clone(),
                None,
                palette.text_muted,
            ),
            Status::AwaitingApproval => (
                palette.warning,
                pulsing_dot(palette.warning),
                tr!("vtube_connect_awaiting_title"),
                Some(tr!("vtube_connect_awaiting_hint")),
                palette.text_faint,
            ),
            Status::Success { title, detail } => (
                palette.success,
                icon(Icon::Check, IDLE_GLYPH, palette.success).into_any_element(),
                title.clone(),
                Some(detail.clone()),
                palette.text_muted,
            ),
            Status::Failure { title, detail } => (
                palette.random,
                icon(Icon::AlertTriangle, IDLE_GLYPH, palette.random).into_any_element(),
                title.clone(),
                Some(detail.clone()),
                palette.text_muted,
            ),
        };

        let mut text = div().flex_1().min_w(px(0.0)).flex().flex_col().child(
            div()
                .font_family(body_family())
                .font_weight(FontWeight::MEDIUM)
                .text_size(ACTIVE_TITLE_SIZE)
                .text_color(palette.text_primary)
                .child(title),
        );
        if let Some(detail) = detail {
            text = text.child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(detail_tint)
                    .child(detail),
            );
        }

        strip_frame(palette)
            .gap(ACTIVE_GAP)
            .py(ACTIVE_PAD_V)
            .px(ACTIVE_PAD_H)
            .border_color(accent)
            .child(indicator)
            .child(text)
            .into_any_element()
    }

    fn idle_strip(&self, palette: &ForgePalette) -> AnyElement {
        strip_frame(palette)
            .gap(IDLE_GAP)
            .py(IDLE_PAD_V)
            .px(IDLE_PAD_H)
            .border_color(palette.border_regular)
            .child(icon(Icon::Clock, IDLE_GLYPH, palette.text_faint))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(IDLE_TEXT_SIZE)
                    .text_color(palette.text_muted)
                    .child(tr!("vtube_connect_idle_hint")),
            )
            .into_any_element()
    }

    fn buttons(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let busy = self.busy();

        let test = div()
            .id("vtube-connect-test")
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .gap(BUTTON_GAP)
            .py(BUTTON_PAD_V)
            .px(GHOST_PAD_H)
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .when(!busy, |el| {
                el.cursor_pointer()
                    .hover(|s| s.border_color(palette.border_input))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.on_test(cx)))
            })
            .when(busy, |el| el.opacity(BUSY_OPACITY))
            .child(icon(
                Icon::PlugConnected,
                BUTTON_GLYPH,
                palette.text_secondary,
            ))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_secondary)
                    .child(tr!("vtube_connect_btn_test")),
            );

        let label = if self.connecting {
            tr!("vtube_connect_btn_authorizing")
        } else {
            tr!("vtube_connect_btn_connect")
        };
        let connect = div()
            .id("vtube-connect-submit")
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .gap(BUTTON_GAP)
            .py(BUTTON_PAD_V)
            .px(PRIMARY_PAD_H)
            .rounded(radius(Radius::Sm))
            .bg(palette.brand)
            .when(!busy, |el| {
                el.cursor_pointer()
                    .hover(|s| s.bg(with_alpha(palette.brand, 0.85)))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.on_connect(cx)))
            })
            .when(busy, |el| el.opacity(BUSY_OPACITY))
            .child(icon(Icon::Plug, BUTTON_GLYPH, palette.shell))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_XS)
                    .text_color(palette.shell)
                    .child(label),
            );

        div()
            .w_full()
            .flex()
            .items_center()
            .gap(BUTTON_ROW_GAP)
            .child(test)
            .child(connect)
            .into_any_element()
    }
}

impl Render for VTubeConnectForm {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();

        let host_focused = self.host.focus_handle(cx).is_focused(window);
        let port_focused = self.port.focus_handle(cx).is_focused(window);

        let host_field = div()
            .flex()
            .flex_col()
            .child(field_label(tr!("vtube_connect_field_host"), &palette))
            .child(input_box(host_focused, &palette).child(self.host.clone()));
        let port_field = div()
            .flex()
            .flex_col()
            .child(field_label(tr!("vtube_connect_field_port"), &palette))
            .child(input_box(port_focused, &palette).child(self.port.clone()));

        let endpoint_row = div()
            .w_full()
            .flex()
            .items_start()
            .gap(FIELD_GAP)
            .mb(FIELD_BLOCK_GAP)
            .child(grow_cell(host_field, HOST_CELL_GROW))
            .child(grow_cell(port_field, PORT_CELL_GROW));

        let plugin_field = div()
            .w_full()
            .flex()
            .flex_col()
            .mb(FIELD_BLOCK_GAP)
            .child(field_label(tr!("vtube_connect_field_plugin"), &palette))
            .child(
                box_frame(&palette)
                    .border_color(palette.border_input)
                    .child(
                        div()
                            .font_family(mono_family())
                            .text_size(BOX_TEXT_SIZE)
                            .text_color(palette.text_primary)
                            .child(PLUGIN_NAME),
                    ),
            );

        let auto_reconnect_row = self.toggle_row(
            ToggleRowSpec {
                glyph: Icon::Refresh,
                tint: palette.info,
                label: tr!("vtube_connect_auto_reconnect_label"),
                hint: tr!("vtube_connect_auto_reconnect_hint"),
                on: self.auto_reconnect,
                flag: VTubeFlag::AutoReconnect,
                last: false,
            },
            &palette,
            cx,
        );
        let launch_row = self.toggle_row(
            ToggleRowSpec {
                glyph: Icon::Bolt,
                tint: palette.warning,
                label: tr!("vtube_connect_on_launch_label"),
                hint: tr!("vtube_connect_on_launch_hint"),
                on: self.connect_on_launch,
                flag: VTubeFlag::ConnectOnLaunch,
                last: true,
            },
            &palette,
            cx,
        );

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(endpoint_row)
            .child(plugin_field)
            .child(auto_reconnect_row)
            .child(launch_row)
            .child(self.status_strip(&palette))
            .child(self.buttons(&palette, cx))
    }
}

fn field_label(label: String, palette: &ForgePalette) -> impl IntoElement {
    div()
        .w_full()
        .mb(FIELD_LABEL_GAP)
        .font_family(mono_family())
        .text_size(FIELD_LABEL_SIZE)
        .text_color(palette.text_muted)
        .child(label)
}

fn box_frame(palette: &ForgePalette) -> gpui::Div {
    div()
        .w_full()
        .flex()
        .items_center()
        .py(BOX_PAD_V)
        .px(BOX_PAD_H)
        .rounded(BOX_RADIUS)
        .border(BORDER_THIN)
        .bg(palette.shell)
}

fn input_box(focused: bool, palette: &ForgePalette) -> gpui::Div {
    let border = if focused {
        palette.border_active
    } else {
        palette.border_input
    };
    box_frame(palette).border_color(border)
}

fn strip_frame(palette: &ForgePalette) -> gpui::Div {
    div()
        .w_full()
        .flex()
        .items_center()
        .my(STRIP_MARGIN)
        .rounded(BOX_RADIUS)
        .border(BORDER_THIN)
        .bg(palette.shell)
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

fn failure_detail(reason: &str) -> String {
    match reason {
        "connect_failed" => tr!("vtube_connect_error_unreachable"),
        "auth_required" => tr!("vtube_connect_error_token_rejected"),
        "auth_denied" => tr!("vtube_connect_error_denied"),
        "auth_timeout" => tr!("vtube_connect_error_timeout"),
        "auth_failed" => tr!("vtube_connect_error_auth"),
        "subscribe_failed" => tr!("vtube_connect_error_subscribe"),
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
        auto_reconnect: get_bool_setting(&*settings, VTUBE_AUTO_RECONNECT_KEY, true).await,
        connect_on_launch: get_bool_setting(&*settings, VTUBE_CONNECT_ON_LAUNCH_KEY, true).await,
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

    // Why: "ignored" leaves the screen spinning on "Connecting" forever, so every payload the
    // supervisor can emit has to resolve to a phase the user can act on.
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

    // Why: the form drains the shared bus, so an unrelated event must not push it out of the
    // connecting phase.
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
}
