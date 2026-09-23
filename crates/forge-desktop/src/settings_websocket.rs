use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;

use forge_components::{
    BORDER_ACCENT, BORDER_THIN, BulletItem, BulletKind, Density, FONT_LG, FONT_SM, FONT_XS,
    FONT_XXS, ForgePalette, Icon, InputEvent, OverlayPosition, Radius, SaveState, Spacing,
    TextArea, TextInput, TypeToConfirm, TypeToConfirmEvent, body_family, field_hint, field_title,
    ghost_button_with_icon, icon, mono_family, overlay, radio_row, radius, save_indicator,
    setting_row, spacing, toggle, tr, type_to_confirm,
};
use forge_runtime::OverlayServiceHandle;
use forge_server::{ServerHandle, ServerSettings};
use forge_storage::{CredentialId, CredentialsRepo, DataProvider, SettingsRepo};
use gpui::{
    AnyElement, App, ClickEvent, Context, Div, Entity, Focusable, FontWeight, Pixels, Rgba,
    SharedString, Subscription, Window, div, prelude::*, px, relative,
};

use crate::async_bridge::{self, ErrorSink};
use crate::presentation::ActivePresentation;
use crate::server_restart::restart_ignoring_disabled;

const BEARER_CREDENTIAL_ID: &str = "server:bearer";
const LAN_PHRASE: &str = "expose to LAN";
const LOCALHOST_ADDR: &str = "127.0.0.1";
const LAN_ADDR: &str = "0.0.0.0";
const MIN_PORT: u16 = 1024;
const DEFAULT_PORT: u16 = 8081;
const DEFAULT_OVERLAY_HINT: &str = "~/.local/share/forge/overlays";
const ORIGINS_PERSIST_CONTEXT: &str = "websocket additional origins";
const ORIGINS_AREA_HEIGHT: Pixels = px(72.0);
const PORT_COLUMN_MIN_W: Pixels = px(140.0);
const TOKEN_COLUMN_MIN_W: Pixels = px(230.0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BindChoice {
    Localhost,
    Lan,
}

struct WebSocketSnapshot {
    enabled: bool,
    lan_bind_enabled: bool,
    port: u16,
    require_ws_token: bool,
    cors_any_origin: bool,
    overlay_root: Option<String>,
    additional_origins: Vec<String>,
}

pub struct SettingsWebSocketView {
    backend: Arc<dyn DataProvider>,
    rt_handle: tokio::runtime::Handle,
    server: Option<ServerHandle>,
    overlays: OverlayServiceHandle,

    enable_server: bool,
    bind_choice: BindChoice,
    port: u16,
    require_ws_token: bool,
    cors_any_origin: bool,
    overlay_root: Option<String>,
    additional_origins: Vec<String>,

    bearer_token: String,
    token_revealed: bool,

    loading: bool,
    save_state: SaveState,
    restarting: bool,
    restart_queued: bool,
    running: bool,
    origins_error: Option<SharedString>,

    port_input: Entity<TextInput>,
    origins_input: Entity<TextArea>,
    origins_blur: Option<Subscription>,
    lan_modal: Option<Entity<TypeToConfirm>>,
    lan_sub: Option<Subscription>,
    _subs: Vec<Subscription>,
}

impl SettingsWebSocketView {
    pub fn new(
        backend: Arc<dyn DataProvider>,
        rt_handle: tokio::runtime::Handle,
        server: Option<ServerHandle>,
        overlays: OverlayServiceHandle,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let port_input = cx.new(|cx| {
            TextInput::new("8081", cx)
                .with_palette(palette)
                .with_font_size(FONT_SM)
        });

        let origins_input = cx.new(|cx| {
            TextArea::new(tr!("settings_ws_origins_placeholder"), cx)
                .with_palette(palette)
                .with_font_size(FONT_XS)
                .with_height(ORIGINS_AREA_HEIGHT)
                .mono()
        });

        let mut subs = Vec::new();
        subs.push(
            cx.subscribe(&port_input, |this, _input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Submitted(_) | InputEvent::Blurred(_)) {
                    this.commit_port(cx);
                }
            }),
        );
        subs.push(
            cx.subscribe(&origins_input, |this, _input, event: &InputEvent, cx| {
                if let InputEvent::Changed(_) = event {
                    this.clear_origins_error(cx);
                }
            }),
        );
        subs.push(cx.on_release(|this, cx| this.persist_origins_on_release(cx)));

        let running = server
            .as_ref()
            .is_some_and(|handle| *handle.run_state().borrow());
        let mut view = Self {
            backend,
            rt_handle,
            server,
            overlays,
            enable_server: true,
            bind_choice: BindChoice::Localhost,
            port: DEFAULT_PORT,
            require_ws_token: true,
            cors_any_origin: true,
            overlay_root: None,
            additional_origins: Vec::new(),
            bearer_token: String::new(),
            token_revealed: false,
            loading: false,
            save_state: SaveState::default(),
            restarting: false,
            restart_queued: false,
            running,
            origins_error: None,
            port_input,
            origins_input,
            origins_blur: None,
            lan_modal: None,
            lan_sub: None,
            _subs: subs,
        };
        view.load(cx);
        view.fetch_token(cx);
        view.start_run_state_bridge(cx);
        view
    }

    fn start_run_state_bridge(&self, cx: &mut Context<Self>) {
        let Some(handle) = self.server.as_ref() else {
            return;
        };
        let mut run_state = handle.run_state();
        cx.spawn(async move |this, cx| {
            loop {
                let running = *run_state.borrow_and_update();
                if this
                    .update(cx, |this, cx| this.apply_run_state(running, cx))
                    .is_err()
                    || run_state.changed().await.is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn apply_run_state(&mut self, running: bool, cx: &mut Context<Self>) {
        if self.running == running {
            return;
        }
        self.running = running;
        cx.notify();
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        self.loading = true;
        self.save_state = SaveState::Saved;
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        async_bridge::run_async(
            &self.rt_handle,
            load_websocket_settings(repo),
            |this, result, cx| this.apply_loaded(result, cx),
            cx,
        );
        cx.notify();
    }

    fn apply_loaded(&mut self, result: Result<WebSocketSnapshot, String>, cx: &mut Context<Self>) {
        self.loading = false;
        match result {
            Ok(snap) => {
                self.enable_server = snap.enabled;
                self.bind_choice = if snap.lan_bind_enabled {
                    BindChoice::Lan
                } else {
                    BindChoice::Localhost
                };
                self.port = snap.port;
                self.port_input
                    .update(cx, |i, cx| i.set_content(snap.port.to_string(), cx));
                self.require_ws_token = snap.require_ws_token;
                self.cors_any_origin = snap.cors_any_origin;
                self.overlay_root = snap.overlay_root.filter(|s| !s.is_empty());
                self.additional_origins = snap.additional_origins;
                let joined = self.additional_origins.join("\n");
                self.origins_input
                    .update(cx, |i, cx| i.set_content(joined, cx));
                self.origins_error = None;
                self.save_state = SaveState::Saved;
            }
            Err(message) => {
                tracing::warn!(error = %message, "failed to load websocket settings");
                self.save_state = SaveState::Error(message.into());
            }
        }
        cx.notify();
    }

    fn fetch_token(&self, cx: &mut Context<Self>) {
        let credentials = Arc::clone(&self.backend) as Arc<dyn CredentialsRepo>;
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                credentials
                    .load(&CredentialId::new(BEARER_CREDENTIAL_ID))
                    .await
                    .ok()
                    .flatten()
            },
            |this, result: Option<String>, cx| {
                if let Some(token) = result {
                    this.bearer_token = token;
                    cx.notify();
                }
            },
            cx,
        );
    }

    fn persist_and_reload(
        &mut self,
        fut: impl Future<Output = Result<(), String>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        self.save_state = SaveState::Saving;
        async_bridge::run_async(
            &self.rt_handle,
            fut,
            |this, result: Result<(), String>, cx| this.apply_persist_outcome(result, cx),
            cx,
        );
        cx.notify();
    }

    fn persist_toggle_and_reload(
        &mut self,
        prev: bool,
        set: fn(&mut Self, bool),
        fut: impl Future<Output = Result<(), String>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        self.save_state = SaveState::Saving;
        async_bridge::run_async(
            &self.rt_handle,
            fut,
            move |this, result: Result<(), String>, cx| {
                if result.is_err() {
                    set(this, prev);
                }
                this.apply_persist_outcome(result, cx);
            },
            cx,
        );
        cx.notify();
    }

    fn apply_persist_outcome(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        match result {
            Ok(()) => {
                self.save_state = SaveState::Saved;
                self.reload_running_server(cx);
            }
            Err(message) => {
                tracing::warn!(error = %message, "failed to save websocket settings");
                self.save_state = SaveState::Error(message.into());
            }
        }
        cx.notify();
    }

    fn reload_running_server(&mut self, cx: &mut Context<Self>) {
        if !self.running && !self.restarting {
            return;
        }
        self.restart_server(cx);
    }

    fn persist_bool(
        &mut self,
        prev: bool,
        set: fn(&mut Self, bool),
        fut: impl Future<Output = Result<(), String>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        self.save_state = SaveState::Saved;
        async_bridge::optimistic(
            &self.rt_handle,
            prev,
            fut,
            move |this, prev, message, cx| {
                set(this, prev);
                if let Some(message) = ErrorSink::Banner.report(message, cx) {
                    this.save_state = SaveState::Error(message.into());
                }
            },
            cx,
        );
        cx.notify();
    }

    fn toggle_enable(&mut self, cx: &mut Context<Self>) {
        let prev = self.enable_server;
        let value = !prev;
        self.enable_server = value;
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        let server = self.server.clone();
        self.persist_bool(
            prev,
            |this, v| this.enable_server = v,
            async move {
                ServerSettings::save_enabled(repo.as_ref(), value)
                    .await
                    .map_err(|e| e.to_string())?;
                if let Some(handle) = server {
                    if value {
                        restart_ignoring_disabled(&handle).await?;
                    } else {
                        handle.stop().await.map_err(|e| e.to_string())?;
                    }
                }
                Ok(())
            },
            cx,
        );
    }

    fn restart_server(&mut self, cx: &mut Context<Self>) {
        let Some(handle) = self.server.clone() else {
            return;
        };
        if self.restarting {
            self.restart_queued = true;
            return;
        }
        self.restarting = true;
        self.restart_queued = false;
        async_bridge::run_async(
            &self.rt_handle,
            async move { restart_ignoring_disabled(&handle).await },
            |this, result: Result<(), String>, cx| this.finish_restart(result, cx),
            cx,
        );
        cx.notify();
    }

    fn finish_restart(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        self.restarting = false;
        if let Err(message) = result {
            tracing::warn!(error = %message, "failed to restart websocket server");
            self.save_state = SaveState::Error(message.into());
        }
        cx.notify();
        if std::mem::take(&mut self.restart_queued) {
            self.restart_server(cx);
        }
    }

    fn restart_disabled(&self) -> bool {
        self.restarting || self.loading || self.server.is_none() || !self.enable_server
    }

    fn restart_button(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> impl IntoElement {
        let label = if self.restarting {
            tr!("server_btn_restarting")
        } else {
            tr!("server_btn_restart")
        };
        ghost_button_with_icon(Icon::Refresh, label, palette)
            .disabled(self.restart_disabled())
            .on_click(
                "settings-ws-restart",
                cx.listener(|this, _: &ClickEvent, _, cx| this.restart_server(cx)),
            )
    }

    fn lifecycle_controls(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut row = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(spacing(Spacing::Xs, density));

        if self.restarting {
            row = row.child(
                div()
                    .flex_none()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.warning)
                    .child(tr!("server_btn_restarting")),
            );
        } else if self.enable_server && !self.running {
            row = row.child(
                div()
                    .flex_none()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(tr!("server_not_running")),
            );
        }

        row.child(toggle(self.enable_server, palette).on_click(
            "settings-ws-enable",
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_enable(cx)),
        ))
    }

    fn select_localhost(&mut self, cx: &mut Context<Self>) {
        self.bind_choice = BindChoice::Localhost;
        self.close_lan_modal();
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        self.persist_and_reload(
            async move {
                ServerSettings::save_bind_address(repo.as_ref(), LOCALHOST_ADDR)
                    .await
                    .map_err(|e| e.to_string())?;
                ServerSettings::save_lan_bind_enabled(repo.as_ref(), false)
                    .await
                    .map_err(|e| e.to_string())
            },
            cx,
        );
    }

    fn open_lan_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let palette = cx.palette();
        let modal = cx.new(|cx| {
            type_to_confirm(LAN_PHRASE, &palette, cx)
                .title(tr!("settings_ws_lan_modal_title"))
                .explanation(tr!("settings_ws_lan_modal_explanation"))
                .instruction(
                    tr!("widget_confirm_type_prefix"),
                    tr!("widget_confirm_type_suffix"),
                )
                .bullets(lan_bind_bullets())
                .confirm_label(tr!("settings_ws_lan_modal_confirm_label"))
                .cancel_label(tr!("common_cancel"))
        });
        let sub = cx.subscribe(
            &modal,
            |this, _modal, event: &TypeToConfirmEvent, cx| match event {
                TypeToConfirmEvent::Confirmed => this.confirm_lan(cx),
                TypeToConfirmEvent::Cancelled => this.cancel_lan(cx),
            },
        );
        modal.update(cx, |m, cx| m.focus_input(window, cx));
        self.lan_modal = Some(modal);
        self.lan_sub = Some(sub);
        cx.notify();
    }

    fn close_lan_modal(&mut self) {
        self.lan_modal = None;
        self.lan_sub = None;
    }

    fn confirm_lan(&mut self, cx: &mut Context<Self>) {
        self.bind_choice = BindChoice::Lan;
        self.close_lan_modal();
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        self.persist_and_reload(
            async move {
                ServerSettings::save_bind_address(repo.as_ref(), LAN_ADDR)
                    .await
                    .map_err(|e| e.to_string())?;
                ServerSettings::save_lan_bind_enabled(repo.as_ref(), true)
                    .await
                    .map_err(|e| e.to_string())
            },
            cx,
        );
    }

    fn cancel_lan(&mut self, cx: &mut Context<Self>) {
        self.bind_choice = BindChoice::Localhost;
        self.close_lan_modal();
        cx.notify();
    }

    fn commit_port(&mut self, cx: &mut Context<Self>) {
        let parsed = self
            .port_input
            .read(cx)
            .content()
            .parse::<u16>()
            .ok()
            .filter(|p| *p >= MIN_PORT);
        match parsed {
            Some(port) => {
                self.port = port;
                let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
                self.persist_and_reload(
                    async move {
                        ServerSettings::save_port(repo.as_ref(), port)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    cx,
                );
            }
            None => {
                let restore = self.port.to_string();
                self.port_input
                    .update(cx, |i, cx| i.set_content(restore, cx));
                cx.notify();
            }
        }
    }

    fn attach_origins_blur(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.origins_blur.is_some() {
            return;
        }
        let handle = self.origins_input.focus_handle(cx);
        self.origins_blur = Some(cx.on_blur(&handle, window, |this, _window, cx| {
            this.commit_origins(cx);
        }));
    }

    fn clear_origins_error(&mut self, cx: &mut Context<Self>) {
        if self.origins_error.is_none() {
            return;
        }
        self.origins_error = None;
        self.origins_input
            .update(cx, |i, cx| i.set_invalid(false, cx));
        cx.notify();
    }

    fn commit_origins(&mut self, cx: &mut Context<Self>) {
        let raw = self.origins_input.read(cx).content().to_owned();
        match parse_origins(&raw) {
            Ok(origins) => {
                self.origins_error = None;
                self.origins_input
                    .update(cx, |i, cx| i.set_invalid(false, cx));
                self.origins_input
                    .update(cx, |i, cx| i.set_content(origins.join("\n"), cx));
                if origins == self.additional_origins {
                    cx.notify();
                    return;
                }
                self.additional_origins = origins.clone();
                let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
                self.persist_and_reload(
                    async move {
                        ServerSettings::save_additional_origins(repo.as_ref(), &origins)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    cx,
                );
            }
            Err(rejected) => {
                self.origins_error = Some(rejected.into());
                self.origins_input
                    .update(cx, |i, cx| i.set_invalid(true, cx));
                cx.notify();
            }
        }
    }

    fn persist_origins_on_release(&mut self, cx: &mut App) {
        let raw = self.origins_input.read(cx).content().to_owned();
        let Ok(origins) = parse_origins(&raw) else {
            return;
        };
        if origins == self.additional_origins {
            return;
        }
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        let reload = self.running || self.restarting;
        let server = self.server.clone().filter(|_| reload);
        async_bridge::detached(&self.rt_handle, ORIGINS_PERSIST_CONTEXT, async move {
            ServerSettings::save_additional_origins(repo.as_ref(), &origins)
                .await
                .map_err(|e| e.to_string())?;
            if let Some(handle) = server {
                restart_ignoring_disabled(&handle).await?;
            }
            Ok::<(), String>(())
        });
    }

    fn toggle_require_ws_token(&mut self, cx: &mut Context<Self>) {
        let prev = self.require_ws_token;
        let value = !prev;
        self.require_ws_token = value;
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        self.persist_bool(
            prev,
            |this, v| this.require_ws_token = v,
            async move {
                ServerSettings::save_auth_required_for_reads(repo.as_ref(), value)
                    .await
                    .map_err(|e| e.to_string())
            },
            cx,
        );
    }

    fn toggle_cors(&mut self, cx: &mut Context<Self>) {
        let prev = self.cors_any_origin;
        let value = !prev;
        self.cors_any_origin = value;
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        self.persist_toggle_and_reload(
            prev,
            |this, v| this.cors_any_origin = v,
            async move {
                ServerSettings::save_overlay_cors_any_origin(repo.as_ref(), value)
                    .await
                    .map_err(|e| e.to_string())
            },
            cx,
        );
    }

    fn browse_overlay_folder(&mut self, cx: &mut Context<Self>) {
        async_bridge::spawn_dialog(
            &self.rt_handle,
            async_bridge::pick_folder(),
            |this, result, cx| {
                if let Ok(path) = result {
                    this.apply_overlay_root(path, cx);
                }
            },
            cx,
        );
    }

    fn apply_overlay_root(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let path_str = path.to_string_lossy().into_owned();
        self.overlay_root = Some(path_str.clone());
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        let overlays = self.overlays.clone();
        self.persist_and_reload(
            async move {
                ServerSettings::save_overlay_root(repo.as_ref(), &path_str)
                    .await
                    .map_err(|e| e.to_string())?;
                overlays
                    .materialize_all()
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            },
            cx,
        );
    }

    fn toggle_token_reveal(&mut self, cx: &mut Context<Self>) {
        self.token_revealed = !self.token_revealed;
        cx.notify();
    }

    fn copy_token(&mut self, cx: &mut Context<Self>) {
        crate::toasts::copy_to_clipboard(self.bearer_token.clone(), cx);
    }

    fn regenerate_token(&mut self, cx: &mut Context<Self>) {
        let Some(handle) = self.server.clone() else {
            return;
        };
        let credentials = Arc::clone(&self.backend) as Arc<dyn CredentialsRepo>;
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let auth = handle.auth_state().await;
                auth.regenerate(credentials.as_ref())
                    .await
                    .map_err(|e| e.to_string())
            },
            |this, result: Result<String, String>, cx| {
                if let Ok(token) = result {
                    this.bearer_token = token;
                    cx.notify();
                }
            },
            cx,
        );
    }

    fn header_row(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(icon(Icon::Server, px(20.0), palette.brand))
            .child(
                div()
                    .min_w(px(0.0))
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_LG)
                    .text_color(palette.text_primary)
                    .child(tr!("settings_ws_title")),
            )
            .child(div().flex_1().min_w(px(0.0)))
            .child(self.restart_button(palette, cx))
            .child(
                div()
                    .flex()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .child(save_indicator(&self.save_state, palette)),
            )
    }

    fn bind_section(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(field_title(tr!("settings_ws_bind_section_title"), palette))
            .child(field_hint(
                tr!("settings_ws_bind_section_subtitle"),
                palette,
            ))
            .child(self.bind_card(BindChoice::Localhost, palette, density, cx))
            .child(self.bind_card(BindChoice::Lan, palette, density, cx))
    }

    fn bind_card(
        &self,
        choice: BindChoice,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected = self.bind_choice == choice;
        let (title, tech, body, accent, badge_glyph, badge_label, badge_color) = match choice {
            BindChoice::Localhost => (
                tr!("settings_ws_bind_localhost_title"),
                LOCALHOST_ADDR.to_owned(),
                tr!("settings_ws_bind_localhost_description"),
                palette.brand,
                Icon::Lock,
                tr!("settings_ws_badge_recommended"),
                palette.success,
            ),
            BindChoice::Lan => (
                tr!("settings_ws_bind_lan_title"),
                LAN_ADDR.to_owned(),
                tr!("settings_ws_bind_lan_description"),
                palette.warning,
                Icon::AlertTriangle,
                tr!("settings_ws_badge_requires_confirmation"),
                palette.warning,
            ),
        };

        let badge = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(3.0))
            .py(px(1.0))
            .px(spacing(Spacing::Xs, Density::Cozy))
            .rounded(radius(Radius::Sm))
            .bg(palette.surface_overlay)
            .child(icon(badge_glyph, px(10.0), badge_color))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XXS)
                    .text_color(badge_color)
                    .child(badge_label),
            );

        let title_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .min_w(px(0.0))
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(title),
            )
            .child(badge)
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(tech),
            );

        let info = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(title_row)
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(body),
            );

        let id: SharedString = match choice {
            BindChoice::Localhost => "settings-ws-bind-localhost".into(),
            BindChoice::Lan => "settings-ws-bind-lan".into(),
        };

        let pad = spacing(Spacing::Sm, density);
        radio_row(id, selected, accent, info, palette)
            .dot_metrics(px(16.0), px(7.0), px(2.0))
            .dot_unselected(palette.border_input)
            .align_start()
            .gap(spacing(Spacing::Sm, density))
            .padding(pad, pad)
            .corner_radius(radius(Radius::Md))
            .row_border(BORDER_ACCENT, BORDER_THIN)
            .row_border_color(palette.border_regular)
            .background(palette.base, palette.base)
            .on_click(
                cx.listener(move |this, _: &ClickEvent, window, cx| match choice {
                    BindChoice::Localhost => this.select_localhost(cx),
                    BindChoice::Lan => this.open_lan_modal(window, cx),
                }),
            )
    }

    fn origins_section(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let apply =
            ghost_button_with_icon(Icon::Check, tr!("settings_ws_origins_apply_btn"), palette)
                .on_click(
                    "settings-ws-origins-apply",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.commit_origins(cx)),
                );

        let mut footer = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density));

        if let Some(rejected) = &self.origins_error {
            footer = footer.child(
                div()
                    .flex()
                    .min_w(px(0.0))
                    .items_center()
                    .gap(spacing(Spacing::Xxs, density))
                    .child(icon(Icon::AlertTriangle, px(12.0), palette.random))
                    .child(
                        div()
                            .flex_none()
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.random)
                            .child(tr!("settings_ws_origins_invalid")),
                    )
                    .child(
                        div()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .text_ellipsis()
                            .font_family(mono_family())
                            .text_size(FONT_XS)
                            .text_color(palette.random)
                            .child(rejected.clone()),
                    ),
            );
        }

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(field_title(
                tr!("settings_ws_origins_section_title"),
                palette,
            ))
            .child(field_hint(tr!("settings_ws_origins_subtitle"), palette))
            .child(self.origins_input.clone())
            .child(footer.child(div().flex_1().min_w(px(0.0))).child(apply))
    }

    fn port_column(&self, palette: &ForgePalette, density: Density) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(field_title(tr!("settings_ws_port_section_title"), palette))
            .child(field_hint(tr!("settings_ws_port_subtitle"), palette))
            .child(self.port_input.clone())
    }

    fn token_column(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let desc = div()
            .flex()
            .items_center()
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(tr!("settings_ws_token_clients_send")),
            )
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(" Authorization: Bearer ..."),
            );

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(field_title(tr!("settings_ws_token_section_title"), palette))
            .child(desc)
            .child(self.token_field(palette, density, cx))
    }

    fn token_field(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let shown = if self.token_revealed {
            self.bearer_token.clone()
        } else {
            mask_token(&self.bearer_token)
        };
        let reveal_glyph = if self.token_revealed {
            Icon::EyeOff
        } else {
            Icon::Eye
        };

        let field = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(spacing(Spacing::Xs, density))
            .py(px(7.0))
            .px(spacing(Spacing::Sm, Density::Cozy))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.shell)
            .child(
                div()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .text_ellipsis()
                    .font_family(mono_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(shown),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(
                        div()
                            .id("settings-ws-token-reveal")
                            .flex()
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.toggle_token_reveal(cx)
                            }))
                            .child(icon(reveal_glyph, px(12.0), palette.text_faint)),
                    )
                    .child(
                        div()
                            .id("settings-ws-token-copy")
                            .flex()
                            .cursor_pointer()
                            .on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| this.copy_token(cx)),
                            )
                            .child(icon(Icon::Copy, px(12.0), palette.text_faint)),
                    ),
            );

        let regenerate = div()
            .id("settings-ws-token-regen")
            .flex()
            .flex_none()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .py(px(7.0))
            .px(spacing(Spacing::Sm, Density::Cozy))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .cursor_pointer()
            .child(icon(Icon::Refresh, px(12.0), palette.warning))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.warning)
                    .child(tr!("server_btn_regenerate")),
            )
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.regenerate_token(cx)));

        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(field)
            .child(regenerate)
    }

    fn auth_section(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(field_title(tr!("settings_ws_auth_section_title"), palette))
            .child(field_hint(
                tr!("settings_ws_auth_section_subtitle"),
                palette,
            ))
            .child(self.auth_row(
                "settings-ws-auth-ws",
                Icon::Lock,
                palette.success,
                tr!("settings_ws_auth_require_ws_label"),
                tr!("settings_ws_auth_require_ws_sublabel"),
                self.require_ws_token,
                palette,
                density,
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_require_ws_token(cx)),
            ))
            .child(hline(palette.border_regular))
            .child(self.auth_row(
                "settings-ws-auth-cors",
                Icon::AlertTriangle,
                palette.warning,
                tr!("settings_ws_auth_cors_label"),
                tr!("settings_ws_auth_cors_sublabel"),
                self.cors_any_origin,
                palette,
                density,
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_cors(cx)),
            ))
    }

    #[allow(clippy::too_many_arguments)]
    fn auth_row(
        &self,
        id: &'static str,
        glyph: Icon,
        glyph_color: Rgba,
        label: impl Into<SharedString>,
        sublabel: impl Into<SharedString>,
        value: bool,
        palette: &ForgePalette,
        density: Density,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> impl IntoElement {
        let labels = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(label.into()),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(sublabel.into()),
            );

        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .py(px(8.0))
            .child(icon(glyph, px(14.0), glyph_color))
            .child(labels)
            .child(toggle(value, palette).on_click(id, handler))
    }

    fn overlay_section(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let desc = div()
            .flex()
            .items_center()
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(tr!("settings_ws_overlay_folder_prefix")),
            )
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(" http://<bind>/"),
            );

        let path_label = self
            .overlay_root
            .clone()
            .unwrap_or_else(|| DEFAULT_OVERLAY_HINT.to_owned());
        let path_box = div()
            .flex_1()
            .min_w(px(0.0))
            .overflow_hidden()
            .text_ellipsis()
            .py(px(7.0))
            .px(spacing(Spacing::Sm, Density::Cozy))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.shell)
            .font_family(mono_family())
            .text_size(FONT_XS)
            .text_color(palette.text_primary)
            .child(path_label);

        let browse =
            ghost_button_with_icon(Icon::FolderOpen, tr!("settings_ws_browse_btn"), palette)
                .on_click(
                    "settings-ws-browse",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.browse_overlay_folder(cx)),
                );

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(field_title(
                tr!("settings_ws_overlay_section_title"),
                palette,
            ))
            .child(desc)
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(path_box)
                    .child(browse),
            )
    }

    fn lan_overlay(
        &self,
        modal: Entity<TypeToConfirm>,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        overlay(modal, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("settings-ws-lan-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_lan(cx));
            })
            .into_any_element()
    }
}

impl Render for SettingsWebSocketView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.attach_origins_blur(window, cx);
        let palette = cx.palette();
        let density = cx.density();

        let port_token = div()
            .w_full()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_start()
            .gap(spacing(Spacing::Md, density))
            .child(weighted(1.0, self.port_column(&palette, density)).min_w(PORT_COLUMN_MIN_W))
            .child(
                weighted(1.6, self.token_column(&palette, density, cx)).min_w(TOKEN_COLUMN_MIN_W),
            );

        let mut root = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .child(self.header_row(&palette, density, cx))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.text_muted)
                    .child(tr!("settings_ws_subtitle")),
            )
            .child(setting_row(
                tr!("settings_ws_enable_label"),
                Some(tr!("settings_ws_enable_description").into()),
                self.lifecycle_controls(&palette, density, cx),
                &palette,
                density,
            ))
            .child(hline(palette.border_regular))
            .child(self.bind_section(&palette, density, cx))
            .child(hline(palette.border_regular))
            .child(self.origins_section(&palette, density, cx))
            .child(hline(palette.border_regular))
            .child(port_token)
            .child(hline(palette.border_regular))
            .child(self.auth_section(&palette, density, cx))
            .child(hline(palette.border_regular))
            .child(self.overlay_section(&palette, density, cx));

        if let Some(modal) = &self.lan_modal {
            root = root.child(self.lan_overlay(modal.clone(), &palette, cx));
        }
        root
    }
}

async fn load_websocket_settings(repo: Arc<dyn SettingsRepo>) -> Result<WebSocketSnapshot, String> {
    let snap = ServerSettings::load(repo.as_ref())
        .await
        .map_err(|e| e.to_string())?;
    Ok(WebSocketSnapshot {
        enabled: snap.enabled,
        lan_bind_enabled: snap.lan_bind_enabled,
        port: snap.port,
        require_ws_token: snap.auth_required_for_reads,
        cors_any_origin: snap.overlay_cors_any_origin,
        overlay_root: snap.overlay_root,
        additional_origins: snap.additional_origins,
    })
}

/// Blank lines are dropped; the `Err` carries the first line that failed so the pane can name it.
fn parse_origins(raw: &str) -> Result<Vec<String>, String> {
    let mut accepted: Vec<String> = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some(origin) = normalize_origin(trimmed) else {
            return Err(trimmed.to_owned());
        };
        if !accepted.contains(&origin) {
            accepted.push(origin);
        }
    }
    Ok(accepted)
}

/// Lowercases to match how the server keys its allowlist; rejects anything past the authority (a browser `Origin` header carries no path).
fn normalize_origin(raw: &str) -> Option<String> {
    let lowered = raw.trim().to_ascii_lowercase();
    let authority = lowered
        .strip_prefix("http://")
        .or_else(|| lowered.strip_prefix("https://"))?;
    let (host, port) = split_host_port(authority)?;
    if !host_is_valid(host) {
        return None;
    }
    if let Some(port) = port
        && !port.parse::<u16>().is_ok_and(|p| p > 0)
    {
        return None;
    }
    Some(lowered)
}

fn split_host_port(authority: &str) -> Option<(&str, Option<&str>)> {
    if authority.starts_with('[') {
        let close = authority.find(']')?;
        let host = &authority[..=close];
        let tail = &authority[close + 1..];
        return match tail {
            "" => Some((host, None)),
            _ => Some((host, Some(tail.strip_prefix(':')?))),
        };
    }
    match authority.split_once(':') {
        Some((_, port)) if port.contains(':') => None,
        Some((host, port)) => Some((host, Some(port))),
        None => Some((authority, None)),
    }
}

fn host_is_valid(host: &str) -> bool {
    if let Some(inner) = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
        return !inner.is_empty()
            && inner
                .chars()
                .all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.');
    }
    !host.starts_with('.')
        && !host.ends_with('.')
        && host.split('.').all(|label| {
            !label.is_empty() && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
}

fn lan_bind_bullets() -> Vec<BulletItem> {
    vec![
        BulletItem::new(BulletKind::Check, tr!("settings_ws_lan_bullet_phone")),
        BulletItem::new(
            BulletKind::Warning,
            tr!("settings_ws_lan_bullet_token_warning"),
        ),
        BulletItem::new(
            BulletKind::Warning,
            tr!("settings_ws_lan_bullet_public_wifi"),
        ),
        BulletItem::new(BulletKind::Info, tr!("settings_ws_lan_bullet_firewall")),
    ]
}

fn hline(color: Rgba) -> Div {
    div().w_full().h(BORDER_THIN).bg(color)
}

fn weighted(grow: f32, child: impl IntoElement) -> Div {
    let mut cell = div().min_w(px(0.0)).child(child);
    let style = cell.style();
    style.flex_grow = Some(grow);
    style.flex_basis = Some(relative(0.0).into());
    cell
}

fn mask_token(token: &str) -> String {
    if token.is_empty() {
        return "-".to_owned();
    }
    let tail: String = token
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("fg_•••••{tail}")
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
mod tests {
    use forge_components::ThemeId;
    use forge_overlay::OverlayKindRegistry;
    use forge_runtime::EventBus;
    use forge_storage::reserved_keys;
    use gpui::TestAppContext;

    use super::*;
    use crate::test_support::{
        StubEventLog, StubOverlays, TestBackend, pump, runtime, stopped_handle, test_backend,
    };

    const STORED_ORIGIN: &str = "http://stored.test:9000";
    const TYPED_ORIGIN: &str = "http://typed.test:9100";
    const RESTART_FAILURE: &str = "address already in use";

    fn settings_view(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        backend: Arc<TestBackend>,
    ) -> Entity<SettingsWebSocketView> {
        settings_view_with_server(cx, rt, backend, None)
    }

    fn settings_view_with_server(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        backend: Arc<TestBackend>,
        server: Option<ServerHandle>,
    ) -> Entity<SettingsWebSocketView> {
        let overlays = OverlayServiceHandle::new(
            Arc::new(StubOverlays),
            Arc::clone(&backend) as Arc<dyn SettingsRepo>,
            Arc::new(OverlayKindRegistry::new()),
            EventBus::new(Arc::new(StubEventLog)),
            None,
        );
        let handle = rt.handle().clone();
        cx.update(|cx| {
            cx.set_global(crate::presentation::Presentation::new(
                ThemeId::default(),
                Density::default(),
            ));
        });
        cx.new(|cx| {
            SettingsWebSocketView::new(
                backend as Arc<dyn DataProvider>,
                handle,
                server,
                overlays,
                cx,
            )
        })
    }

    fn type_origins(cx: &mut TestAppContext, view: &Entity<SettingsWebSocketView>, text: &str) {
        let text = text.to_owned();
        view.update(cx, |this, cx| {
            this.origins_input
                .update(cx, |input, cx| input.set_content(text, cx));
        });
    }

    /// Why: dropping the last handle only queues the release; gpui runs release
    /// callbacks on the next app update, so the pump has to include one.
    fn release(cx: &mut TestAppContext, view: Entity<SettingsWebSocketView>) {
        drop(view);
        cx.update(|_cx| {});
        cx.run_until_parked();
    }

    #[gpui::test]
    fn leaving_the_pane_persists_origins_typed_but_never_committed(cx: &mut TestAppContext) {
        let rt = runtime();
        let (backend, mut writes) = test_backend();
        let view = settings_view(cx, &rt, backend);

        type_origins(cx, &view, TYPED_ORIGIN);
        release(cx, view);
        pump(&rt);

        let (key, value) = writes
            .try_recv()
            .expect("the released pane never wrote the typed origins");
        assert_eq!(key, reserved_keys::SERVER_ADDITIONAL_ORIGINS);
        assert_eq!(
            serde_json::from_str::<Vec<String>>(&value).expect("origins json"),
            vec![TYPED_ORIGIN.to_owned()],
        );
    }

    #[gpui::test]
    fn leaving_the_pane_writes_nothing_when_the_typed_origins_add_nothing(cx: &mut TestAppContext) {
        for (typed, case) in [
            ("http://:not-an-origin", "text that is not a valid origin"),
            (STORED_ORIGIN, "the list that is already stored"),
        ] {
            let rt = runtime();
            let (backend, mut writes) = test_backend();
            let view = settings_view(cx, &rt, backend);
            view.update(cx, |this, _| {
                this.additional_origins = vec![STORED_ORIGIN.to_owned()];
            });

            type_origins(cx, &view, typed);
            release(cx, view);
            pump(&rt);

            assert!(writes.try_recv().is_err(), "{case}");
        }
    }
    #[gpui::test]
    fn restart_is_offered_only_once_an_enabled_server_is_loaded_and_idle(cx: &mut TestAppContext) {
        for (with_server, restarting, loading, enable_server, disabled, case) in [
            (
                true,
                false,
                false,
                true,
                false,
                "an enabled, loaded, idle server",
            ),
            (true, true, false, true, true, "a restart already in flight"),
            (
                true,
                false,
                true,
                true,
                true,
                "the settings are still loading",
            ),
            (
                true,
                false,
                false,
                false,
                true,
                "the server is switched off",
            ),
            (false, false, false, true, true, "there is no server handle"),
        ] {
            let rt = runtime();
            let (backend, _writes) = test_backend();
            let server = with_server.then(|| stopped_handle(&rt, &backend));
            let view = settings_view_with_server(cx, &rt, backend, server);

            view.update(cx, |this, _| {
                this.restarting = restarting;
                this.loading = loading;
                this.enable_server = enable_server;
                assert_eq!(this.restart_disabled(), disabled, "{case}");
            });
        }
    }

    #[gpui::test]
    fn a_change_during_an_in_flight_restart_queues_exactly_one_rerun(cx: &mut TestAppContext) {
        let rt = runtime();
        let (backend, _writes) = test_backend();
        let server = stopped_handle(&rt, &backend);
        let view = settings_view_with_server(cx, &rt, backend, Some(server));

        view.update(cx, |this, cx| {
            this.restart_server(cx);
            assert!(this.restarting, "the first request goes out immediately");
            assert!(!this.restart_queued);

            this.restart_server(cx);
            this.restart_server(cx);
            assert!(
                this.restart_queued,
                "changes arriving mid-flight must queue a rerun",
            );

            this.finish_restart(Ok(()), cx);
            assert!(this.restarting, "the queued rerun must go out");
            assert!(
                !this.restart_queued,
                "two mid-flight changes must not book two reruns",
            );

            this.finish_restart(Ok(()), cx);
            assert!(!this.restarting);
        });
    }

    #[gpui::test]
    fn a_failed_restart_never_leaves_a_rerun_stuck_in_the_queue(cx: &mut TestAppContext) {
        for (changed_mid_flight, expect_rerun, case) in [
            (
                false,
                false,
                "nothing changed while the restart was in flight",
            ),
            (
                true,
                true,
                "a change landed while the restart was in flight",
            ),
        ] {
            let rt = runtime();
            let (backend, _writes) = test_backend();
            let server = stopped_handle(&rt, &backend);
            let view = settings_view_with_server(cx, &rt, backend, Some(server));

            view.update(cx, |this, cx| {
                this.restart_server(cx);
                if changed_mid_flight {
                    this.restart_server(cx);
                }

                this.finish_restart(Err(RESTART_FAILURE.to_owned()), cx);

                assert!(!this.restart_queued, "{case}");
                assert_eq!(this.restarting, expect_rerun, "{case}");
            });
        }
    }
}
