use std::sync::Arc;

use forge_components::{
    BORDER_THIN, FONT_XS, FONT_XXS, ForgePalette, Icon, Radius, TextInput, body_family, icon,
    mono_family, radius, spinner, toggle, tr, with_alpha,
};
use forge_events::EventPublisher;
use forge_storage::{CredentialsRepo, SettingsRepo, get_bool_setting, set_bool_setting};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, Focusable, FontWeight, Pixels, Rgba,
    SharedString, Subscription, Window, div, prelude::*, px,
};

use crate::async_bridge::{self, ErrorSink};
use crate::builtin_sections::grow_cell;
use crate::integrations::ObsInstallSeed;
use crate::presentation::ActivePresentation;

pub const OBS_AUTO_RECONNECT_KEY: &str = "obs.auto_reconnect";
pub const OBS_CONNECT_ON_LAUNCH_KEY: &str = "obs.connect_on_launch";

pub(crate) const DEFAULT_HOST: &str = "localhost";
pub(crate) const DEFAULT_PORT: u16 = 4455;

const TOGGLE_GLYPH: Pixels = px(14.0);

const FIELD_LABEL_SIZE: Pixels = px(11.0);
const FIELD_LABEL_GAP: Pixels = px(6.0);
const FIELD_GAP: Pixels = px(10.0);
const FIELD_BLOCK_GAP: Pixels = px(12.0);
const BOX_PAD_V: Pixels = px(7.0);
const BOX_PAD_H: Pixels = px(11.0);
const BOX_RADIUS: Pixels = px(7.0);
const BOX_TEXT_SIZE: Pixels = px(12.0);
const EYE_GLYPH: Pixels = px(13.0);

const TOGGLE_ROW_PAD_V: Pixels = px(8.0);
const TOGGLE_ROW_GAP: Pixels = px(10.0);
const TOGGLE_LABEL_SIZE: Pixels = px(12.5);
const TOGGLE_HINT_SIZE: Pixels = px(11.0);

const BANNER_PAD_V: Pixels = px(8.0);
const BANNER_PAD_H: Pixels = px(11.0);
const BANNER_MARGIN: Pixels = px(12.0);
const BANNER_GAP: Pixels = px(9.0);
const BANNER_GLYPH: Pixels = px(13.0);
const BANNER_TITLE_SIZE: Pixels = px(11.5);

const BUTTON_ROW_GAP: Pixels = px(8.0);
const BUTTON_PAD_V: Pixels = px(8.0);
const GHOST_PAD_H: Pixels = px(14.0);
const PRIMARY_PAD_H: Pixels = px(16.0);
const BUTTON_GAP: Pixels = px(5.0);
const BUTTON_GLYPH: Pixels = px(13.0);

const HOST_CELL_GROW: f32 = 16.0;
const PORT_CELL_GROW: f32 = 10.0;

pub struct ObsConnected;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ObsSubmit {
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum ObsFlag {
    AutoReconnect,
    ConnectOnLaunch,
}

impl ObsFlag {
    fn key(self) -> &'static str {
        match self {
            ObsFlag::AutoReconnect => OBS_AUTO_RECONNECT_KEY,
            ObsFlag::ConnectOnLaunch => OBS_CONNECT_ON_LAUNCH_KEY,
        }
    }

    fn element_id(self) -> &'static str {
        match self {
            ObsFlag::AutoReconnect => "obs-connect-auto-reconnect",
            ObsFlag::ConnectOnLaunch => "obs-connect-on-launch",
        }
    }
}

struct ToggleRowSpec {
    glyph: Icon,
    tint: Rgba,
    label: String,
    hint: String,
    on: bool,
    flag: ObsFlag,
    last: bool,
}

struct Prefill {
    host: String,
    port: String,
    password: String,
    auto_reconnect: bool,
    connect_on_launch: bool,
}

struct ProbeReport {
    websocket_version: String,
    scene_count: usize,
    round_trip_ms: u64,
}

enum Banner {
    Hidden,
    Busy(String),
    Success { title: String, detail: String },
    Failure { title: String, detail: String },
}

struct FormValues {
    host: String,
    port: u16,
    password: String,
}

pub struct ObsCredentialsForm {
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
    auto_reconnect: bool,
    connect_on_launch: bool,
    banner: Banner,
    busy: bool,
    _subs: Vec<Subscription>,
}

impl EventEmitter<ObsConnected> for ObsCredentialsForm {}

impl ObsCredentialsForm {
    pub fn new(
        rt_handle: tokio::runtime::Handle,
        credentials: Arc<dyn CredentialsRepo>,
        settings: Arc<dyn SettingsRepo>,
        bus: Arc<dyn EventPublisher>,
        seed: ObsInstallSeed,
        submit: ObsSubmit,
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
        let password = cx.new(|cx| {
            TextInput::new("", cx)
                .plain()
                .mono()
                .secure(true)
                .with_palette(palette)
                .with_font_size(BOX_TEXT_SIZE)
        });

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
            auto_reconnect: true,
            connect_on_launch: true,
            banner: Banner::Hidden,
            busy: false,
            _subs: subs,
        };
        form.load_prefill(cx);
        form
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
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
        self.password
            .update(cx, |input, cx| input.set_content(prefill.password, cx));
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
        let password = self.password.read(cx).content().to_owned();

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
                self.banner = Banner::Failure {
                    title: tr!("obs_connect_error_title"),
                    detail: tr!("obs_connect_error_invalid_port"),
                };
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

    fn on_toggle_flag(&mut self, flag: ObsFlag, cx: &mut Context<Self>) {
        let value = match flag {
            ObsFlag::AutoReconnect => {
                self.auto_reconnect = !self.auto_reconnect;
                self.auto_reconnect
            }
            ObsFlag::ConnectOnLaunch => {
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
            tr!("obs_connect_settings_save_failed"),
            cx,
        );
    }

    fn on_test(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(form) = self.read_form(cx) else {
            return;
        };
        self.busy = true;
        self.banner = Banner::Busy(tr!("obs_connect_testing"));
        async_bridge::run_async(
            &self.rt_handle,
            async move { probe(form).await },
            |this, result, cx| this.apply_probe(result, cx),
            cx,
        );
        cx.notify();
    }

    fn apply_probe(&mut self, result: Result<ProbeReport, String>, cx: &mut Context<Self>) {
        self.busy = false;
        self.banner = match result {
            Ok(report) => Banner::Success {
                title: tr!("obs_connect_test_successful"),
                detail: tr!(
                    "obs_connect_test_detail",
                    version = report.websocket_version,
                    scenes = report.scene_count as i64,
                    rtt = report.round_trip_ms as i64
                ),
            },
            Err(error) => Banner::Failure {
                title: tr!("obs_connect_test_failed"),
                detail: error,
            },
        };
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
        self.banner = Banner::Busy(tr!("obs_connect_connecting"));

        let credentials = Arc::clone(&self.credentials);
        let settings = Arc::clone(&self.settings);
        let bus = Arc::clone(&self.bus);
        let seed = self.seed.clone();
        let auto_reconnect = self.auto_reconnect;
        let connect_on_launch = self.connect_on_launch;
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                connect_obs(
                    credentials,
                    settings,
                    bus,
                    seed,
                    form,
                    auto_reconnect,
                    connect_on_launch,
                )
                .await
            },
            |this, result, cx| this.apply_connect(result, cx),
            cx,
        );
        cx.notify();
    }

    fn apply_connect(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        self.busy = false;
        match result {
            Ok(()) => {
                self.banner = Banner::Hidden;
                cx.emit(ObsConnected);
            }
            Err(error) => {
                self.banner = Banner::Failure {
                    title: tr!("obs_connect_failed"),
                    detail: error,
                };
            }
        }
        cx.notify();
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

    fn banner(&self, palette: &ForgePalette) -> Option<AnyElement> {
        let (accent, glyph, title, detail): (Rgba, Option<Icon>, &str, Option<&str>) =
            match &self.banner {
                Banner::Hidden => return None,
                Banner::Busy(label) => (palette.border_regular, None, label.as_str(), None),
                Banner::Success { title, detail } => (
                    palette.success,
                    Some(Icon::Check),
                    title.as_str(),
                    Some(detail.as_str()),
                ),
                Banner::Failure { title, detail } => (
                    palette.random,
                    Some(Icon::AlertTriangle),
                    title.as_str(),
                    Some(detail.as_str()),
                ),
            };

        let indicator: AnyElement = match glyph {
            Some(glyph) => icon(glyph, BANNER_GLYPH, accent).into_any_element(),
            None => spinner(
                SharedString::from("obs-connect-banner-spin"),
                Icon::Loader2,
                BANNER_GLYPH,
                palette.text_muted,
            )
            .into_any_element(),
        };

        let mut text = div().flex_1().min_w(px(0.0)).flex().flex_col().child(
            div()
                .font_family(body_family())
                .font_weight(FontWeight::MEDIUM)
                .text_size(BANNER_TITLE_SIZE)
                .text_color(palette.text_primary)
                .child(title.to_owned()),
        );
        if let Some(detail) = detail {
            text = text.child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(detail.to_owned()),
            );
        }

        Some(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap(BANNER_GAP)
                .py(BANNER_PAD_V)
                .px(BANNER_PAD_H)
                .my(BANNER_MARGIN)
                .rounded(BOX_RADIUS)
                .border(BORDER_THIN)
                .border_color(accent)
                .bg(palette.shell)
                .child(indicator)
                .child(text)
                .into_any_element(),
        )
    }

    fn buttons(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let test = div()
            .id("obs-connect-test")
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
            .cursor_pointer()
            .hover(|s| s.border_color(palette.border_input))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.on_test(cx)))
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
                    .child(tr!("obs_connect_btn_test")),
            );

        let connect = div()
            .id("obs-connect-submit")
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .gap(BUTTON_GAP)
            .py(BUTTON_PAD_V)
            .px(PRIMARY_PAD_H)
            .rounded(radius(Radius::Sm))
            .bg(palette.brand)
            .cursor_pointer()
            .hover(|s| s.bg(with_alpha(palette.brand, 0.85)))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.on_connect(cx)))
            .child(icon(self.submit.glyph(), BUTTON_GLYPH, palette.shell))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_XS)
                    .text_color(palette.shell)
                    .child(self.submit.label()),
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

impl Render for ObsCredentialsForm {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();

        let host_focused = self.host.focus_handle(cx).is_focused(window);
        let port_focused = self.port.focus_handle(cx).is_focused(window);
        let password_focused = self.password.focus_handle(cx).is_focused(window);

        let host_field = div()
            .flex()
            .flex_col()
            .child(field_label(tr!("obs_connect_field_host"), None, &palette))
            .child(input_box(host_focused, &palette).child(self.host.clone()));
        let port_field = div()
            .flex()
            .flex_col()
            .child(field_label(tr!("obs_connect_field_port"), None, &palette))
            .child(input_box(port_focused, &palette).child(self.port.clone()));

        let endpoint_row = div()
            .w_full()
            .flex()
            .items_start()
            .gap(FIELD_GAP)
            .mb(FIELD_BLOCK_GAP)
            .child(grow_cell(host_field, HOST_CELL_GROW))
            .child(grow_cell(port_field, PORT_CELL_GROW));

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
            .mb(FIELD_BLOCK_GAP)
            .child(field_label(
                tr!("obs_connect_field_password"),
                Some(tr!("obs_connect_password_note")),
                &palette,
            ))
            .child(
                input_box(password_focused, &palette)
                    .gap(BOX_PAD_H)
                    .child(self.password.clone())
                    .child(eye),
            );

        let auto_reconnect_row = self.toggle_row(
            ToggleRowSpec {
                glyph: Icon::Refresh,
                tint: palette.info,
                label: tr!("obs_connect_auto_reconnect_label"),
                hint: tr!("obs_connect_auto_reconnect_hint"),
                on: self.auto_reconnect,
                flag: ObsFlag::AutoReconnect,
                last: false,
            },
            &palette,
            cx,
        );
        let launch_row = self.toggle_row(
            ToggleRowSpec {
                glyph: Icon::Bolt,
                tint: palette.warning,
                label: tr!("obs_connect_on_launch_label"),
                hint: tr!("obs_connect_on_launch_hint"),
                on: self.connect_on_launch,
                flag: ObsFlag::ConnectOnLaunch,
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
            .child(password_field)
            .child(auto_reconnect_row)
            .child(launch_row)
            .children(self.banner(&palette))
            .child(self.buttons(&palette, cx))
    }
}

fn field_label(label: String, note: Option<String>, palette: &ForgePalette) -> impl IntoElement {
    let mut row = div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .mb(FIELD_LABEL_GAP)
        .child(
            div()
                .font_family(mono_family())
                .text_size(FIELD_LABEL_SIZE)
                .text_color(palette.text_muted)
                .child(label),
        );
    if let Some(note) = note {
        row = row.child(
            div()
                .font_family(body_family())
                .text_size(FIELD_LABEL_SIZE)
                .text_color(palette.text_faint)
                .child(note),
        );
    }
    row
}

fn input_box(focused: bool, palette: &ForgePalette) -> gpui::Div {
    let border = if focused {
        palette.border_active
    } else {
        palette.border_input
    };
    div()
        .w_full()
        .flex()
        .items_center()
        .py(BOX_PAD_V)
        .px(BOX_PAD_H)
        .rounded(BOX_RADIUS)
        .border(BORDER_THIN)
        .border_color(border)
        .bg(palette.shell)
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
        None => (DEFAULT_HOST.to_owned(), DEFAULT_PORT, String::new()),
    };

    Prefill {
        host,
        port: port.to_string(),
        password,
        auto_reconnect: get_bool_setting(&*settings, OBS_AUTO_RECONNECT_KEY, true).await,
        connect_on_launch: get_bool_setting(&*settings, OBS_CONNECT_ON_LAUNCH_KEY, true).await,
    }
}

async fn probe(form: FormValues) -> Result<ProbeReport, String> {
    let result = forge_obs::probe_connection(&form.host, form.port, &form.password)
        .await
        .map_err(|e| e.to_string())?;
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
    auto_reconnect: bool,
    connect_on_launch: bool,
) -> Result<(), String> {
    forge_obs::credentials::store(&*credentials, &form.host, form.port, &form.password)
        .await
        .map_err(|e| e.to_string())?;
    set_bool_setting(&*settings, OBS_AUTO_RECONNECT_KEY, auto_reconnect)
        .await
        .map_err(|e| e.to_string())?;
    set_bool_setting(&*settings, OBS_CONNECT_ON_LAUNCH_KEY, connect_on_launch)
        .await
        .map_err(|e| e.to_string())?;

    let client = forge_obs::credentials::load_and_connect(&*credentials, bus)
        .await
        .map_err(|e| e.to_string())?;
    client.set_auto_reconnect(auto_reconnect);
    seed.install(client);
    Ok(())
}
