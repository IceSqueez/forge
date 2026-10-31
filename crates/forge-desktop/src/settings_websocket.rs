use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;

use forge_components::{
    BORDER_THIN, BulletItem, BulletKind, ConfirmTone, DISABLED_OPACITY, Density, FONT_SM, FONT_XS,
    FONT_XXS, ForgePalette, Icon, InputEvent, OverlayPosition, Radius, SaveState, Spacing,
    TextArea, TextInput, TypeToConfirm, TypeToConfirmEvent, body_family, confirm_modal, error_row,
    error_row_with_retry, field_title, ghost_button_with_icon, grow_cell, icon, mono_family,
    overlay, radio_row, radius, save_indicator, scaled, setting_block, setting_row, setting_rows,
    setting_sections, spacing, toggle, tr, type_to_confirm,
};
use forge_runtime::{OverlaySampleSource, OverlayServiceHandle};
use forge_server::{
    DEFAULT_PORT, LAN_BIND_ADDRESS, LOCALHOST_BIND_ADDRESS, ServerHandle, ServerSettings,
};
use forge_storage::{CredentialsRepo, DataProvider, SettingsRepo};
use forge_types::Secret;
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, Focusable, FontWeight, Pixels, SharedString,
    Subscription, Window, div, prelude::*, px,
};

use crate::async_bridge::{self, ErrorSink};
use crate::navigation::SettingsSection;
use crate::overlay_build_failure::OverlayBuildFailure;
use crate::overlay_link_token;
use crate::repo_failure::RepoFailure;
use crate::server_bearer;
use crate::server_restart::restart_ignoring_disabled;
use crate::settings_pane_header::SettingsPaneHeader;
use forge_components::ActivePresentation;

const LAN_PHRASE: &str = "expose to LAN";
const MIN_PORT: u16 = 1024;
const ORIGINS_PERSIST_CONTEXT: &str = "websocket additional origins";
const ORIGINS_AREA_HEIGHT: Pixels = px(72.0);
const PORT_COLUMN_MIN_W: Pixels = px(140.0);
const TOKEN_COLUMN_MIN_W: Pixels = px(230.0);
const FIELD_PAD_V: Pixels = px(7.0);
const TOKEN_ACTION_GLYPH: Pixels = px(12.0);
const WARNING_GLYPH: Pixels = px(12.0);
const BADGE_GLYPH: Pixels = px(10.0);

enum TokenRegenerateNote {
    Clean,
    RewriteFailed(String),
    RestoreFailed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AfterSave {
    Nothing,
    ReloadServer,
    ApplyEnabled(bool),
}

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
    require_overlay_token: bool,
    cors_any_origin: bool,
    overlay_root: Option<String>,
    additional_origins: Vec<String>,
    additional_origins_unreadable: bool,
}

pub struct SettingsWebSocketView {
    backend: Arc<dyn DataProvider>,
    rt_handle: tokio::runtime::Handle,
    server: Option<ServerHandle>,
    overlays: OverlayServiceHandle,
    overlay_samples: Arc<dyn OverlaySampleSource>,

    enable_server: bool,
    bind_choice: BindChoice,
    port: u16,
    require_ws_token: bool,
    require_overlay_token: bool,
    cors_any_origin: bool,
    overlay_root: Option<String>,
    additional_origins: Vec<String>,
    additional_origins_unreadable: bool,

    bearer_token: Secret,
    token_revealed: bool,
    overlay_token: Option<Secret>,
    overlay_token_regenerating: bool,
    overlay_token_confirm_open: bool,

    loading: bool,
    load_failure: Option<SharedString>,
    save_state: SaveState,
    restarting: bool,
    restart_queued: bool,
    running: bool,
    origins_error: Option<SharedString>,

    port_input: Entity<TextInput>,
    origins_input: Entity<TextArea>,
    origins_blur: Option<Subscription>,
    regenerate_pending: bool,
    default_overlay_root: String,
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
        overlay_samples: Arc<dyn OverlaySampleSource>,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let port_input = cx.new(|cx| {
            TextInput::new(DEFAULT_PORT.to_string(), cx)
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

        subs.push(overlay_link_token::follow(
            server.as_ref(),
            &rt_handle,
            |this: &mut Self, token| this.overlay_token = token,
            cx,
        ));
        let overlay_token = overlay_link_token::current(cx);

        let running = server
            .as_ref()
            .is_some_and(|handle| *handle.run_state().borrow());
        let mut view = Self {
            backend,
            rt_handle,
            server,
            overlays,
            overlay_samples,
            enable_server: true,
            bind_choice: BindChoice::Localhost,
            port: DEFAULT_PORT,
            require_ws_token: true,
            require_overlay_token: forge_storage::SERVER_HTTP_OVERLAY_REQUIRE_TOKEN_DEFAULT,
            cors_any_origin: true,
            overlay_root: None,
            additional_origins: Vec::new(),
            additional_origins_unreadable: false,
            bearer_token: Secret::new(""),
            token_revealed: false,
            overlay_token,
            overlay_token_regenerating: false,
            overlay_token_confirm_open: false,
            loading: false,
            load_failure: None,
            save_state: SaveState::default(),
            restarting: false,
            restart_queued: false,
            running,
            origins_error: None,
            port_input,
            origins_input,
            origins_blur: None,
            regenerate_pending: false,
            default_overlay_root: forge_paths::overlays_dir().display().to_string(),
            lan_modal: None,
            lan_sub: None,
            _subs: subs,
        };
        async_bridge::attach_to_window(Self::attach_origins_blur, cx);
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
        self.load_failure = None;
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

    fn apply_loaded(
        &mut self,
        result: Result<WebSocketSnapshot, RepoFailure>,
        cx: &mut Context<Self>,
    ) {
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
                self.require_overlay_token = snap.require_overlay_token;
                self.cors_any_origin = snap.cors_any_origin;
                self.overlay_root = snap.overlay_root.filter(|s| !s.is_empty());
                self.additional_origins = snap.additional_origins;
                self.additional_origins_unreadable = snap.additional_origins_unreadable;
                let joined = self.additional_origins.join("\n");
                self.origins_input
                    .update(cx, |i, cx| i.set_content(joined, cx));
                self.origins_error = None;
                self.save_state = SaveState::Saved;
            }
            Err(message) => {
                tracing::warn!(error = %message, "failed to load websocket settings");
                self.load_failure = Some(message.text().into());
            }
        }
        cx.notify();
    }

    fn fetch_token(&self, cx: &mut Context<Self>) {
        let credentials = Arc::clone(&self.backend) as Arc<dyn CredentialsRepo>;
        let server = self.server.clone();
        async_bridge::run_async(
            &self.rt_handle,
            server_bearer::current_bearer_token(server, credentials),
            |this, result: Result<Option<Secret>, RepoFailure>, cx| match result {
                Ok(Some(token)) => {
                    this.bearer_token = token;
                    cx.notify();
                }
                Ok(None) => {}
                Err(reason) => server_bearer::token_load_failed(&reason.text(), cx),
            },
            cx,
        );
    }

    fn persist_field<T: Send + 'static>(
        &mut self,
        prev: T,
        restore: impl FnOnce(&mut Self, T, &mut Context<Self>) + Send + 'static,
        after_save: AfterSave,
        fut: impl Future<Output = Result<(), RepoFailure>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        self.save_state = SaveState::Saving;
        async_bridge::run_async(
            &self.rt_handle,
            fut,
            move |this, result: Result<(), RepoFailure>, cx| {
                if result.is_err() {
                    restore(this, prev, cx);
                }
                this.apply_persist_outcome(result, after_save, cx);
            },
            cx,
        );
        cx.notify();
    }

    fn apply_persist_outcome(
        &mut self,
        result: Result<(), RepoFailure>,
        after_save: AfterSave,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(()) => {
                self.save_state = SaveState::Saved;
                match after_save {
                    AfterSave::Nothing => {}
                    AfterSave::ReloadServer => self.reload_running_server(cx),
                    AfterSave::ApplyEnabled(enabled) => self.apply_enabled(enabled, cx),
                }
            }
            Err(message) => {
                tracing::warn!(error = %message, "failed to save websocket settings");
                self.save_state = SaveState::Error(message.text().into());
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

    fn toggle_enable(&mut self, cx: &mut Context<Self>) {
        if !self.settings_loaded() {
            return;
        }
        let prev = self.enable_server;
        let value = !prev;
        self.enable_server = value;
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        self.persist_field(
            prev,
            |this, prev, _| this.enable_server = prev,
            AfterSave::ApplyEnabled(value),
            async move {
                ServerSettings::save_enabled(repo.as_ref(), value)
                    .await
                    .map_err(RepoFailure::from)
            },
            cx,
        );
    }

    fn apply_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        let Some(handle) = self.server.clone() else {
            return;
        };
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                if enabled {
                    restart_ignoring_disabled(&handle).await
                } else {
                    handle.stop().await.map_err(RepoFailure::from)
                }
            },
            |this, result: Result<(), RepoFailure>, cx| {
                if let Err(message) = result {
                    tracing::warn!(error = %message, "saved server state could not be applied");
                    this.save_state = SaveState::Error(message.text().into());
                    cx.notify();
                }
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
            |this, result: Result<(), RepoFailure>, cx| this.finish_restart(result, cx),
            cx,
        );
        cx.notify();
    }

    fn finish_restart(&mut self, result: Result<(), RepoFailure>, cx: &mut Context<Self>) {
        self.restarting = false;
        if let Err(message) = result {
            tracing::warn!(error = %message, "failed to restart websocket server");
            self.save_state = SaveState::Error(message.text().into());
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
            .busy(self.restarting)
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
        if !self.settings_loaded() {
            return;
        }
        let prev = self.bind_choice;
        self.bind_choice = BindChoice::Localhost;
        self.close_lan_modal();
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        self.persist_field(
            prev,
            |this, prev, _| this.bind_choice = prev,
            AfterSave::ReloadServer,
            async move {
                ServerSettings::save_bind_address(repo.as_ref(), LOCALHOST_BIND_ADDRESS)
                    .await
                    .map_err(RepoFailure::from)?;
                ServerSettings::save_lan_bind_enabled(repo.as_ref(), false)
                    .await
                    .map_err(RepoFailure::from)
            },
            cx,
        );
    }

    fn open_lan_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.settings_loaded() {
            return;
        }
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
        if !self.settings_loaded() {
            return;
        }
        self.bind_choice = BindChoice::Lan;
        self.close_lan_modal();
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        self.persist_field(
            BindChoice::Localhost,
            |this, prev, _| this.bind_choice = prev,
            AfterSave::ReloadServer,
            async move {
                ServerSettings::save_bind_address(repo.as_ref(), LAN_BIND_ADDRESS)
                    .await
                    .map_err(RepoFailure::from)?;
                ServerSettings::save_lan_bind_enabled(repo.as_ref(), true)
                    .await
                    .map_err(RepoFailure::from)
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
        if !self.settings_loaded() {
            return;
        }
        let parsed = self
            .port_input
            .read(cx)
            .content()
            .parse::<u16>()
            .ok()
            .filter(|p| *p >= MIN_PORT);
        match parsed {
            Some(port) if port == self.port => {}
            Some(port) => {
                let prev = self.port;
                self.port = port;
                let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
                self.persist_field(
                    prev,
                    |this, prev, cx| {
                        this.port = prev;
                        this.port_input
                            .update(cx, |i, cx| i.set_content(prev.to_string(), cx));
                    },
                    AfterSave::ReloadServer,
                    async move {
                        ServerSettings::save_port(repo.as_ref(), port)
                            .await
                            .map_err(RepoFailure::from)
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

    fn settings_loaded(&self) -> bool {
        !self.loading && self.load_failure.is_none()
    }

    fn origins_editable(&self) -> bool {
        self.settings_loaded() && !self.additional_origins_unreadable
    }

    fn commit_origins(&mut self, cx: &mut Context<Self>) {
        if !self.origins_editable() {
            return;
        }
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
                let prev = std::mem::replace(&mut self.additional_origins, origins.clone());
                let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
                self.persist_field(
                    prev,
                    |this, prev, cx| {
                        this.origins_input
                            .update(cx, |i, cx| i.set_content(prev.join("\n"), cx));
                        this.additional_origins = prev;
                    },
                    AfterSave::ReloadServer,
                    async move {
                        ServerSettings::save_additional_origins(repo.as_ref(), &origins)
                            .await
                            .map_err(RepoFailure::from)
                    },
                    cx,
                );
            }
            Err(RejectedOrigin(rejected)) => {
                self.origins_error = Some(rejected.into());
                self.origins_input
                    .update(cx, |i, cx| i.set_invalid(true, cx));
                cx.notify();
            }
        }
    }

    fn persist_origins_on_release(&mut self, cx: &mut App) {
        if !self.origins_editable() {
            return;
        }
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
                .map_err(RepoFailure::from)?;
            if let Some(handle) = server {
                restart_ignoring_disabled(&handle).await?;
            }
            Ok::<(), RepoFailure>(())
        });
    }

    fn toggle_require_ws_token(&mut self, cx: &mut Context<Self>) {
        if !self.settings_loaded() {
            return;
        }
        let prev = self.require_ws_token;
        let value = !prev;
        self.require_ws_token = value;
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        let server = self.server.clone();
        self.persist_field(
            prev,
            |this, prev, _| this.require_ws_token = prev,
            AfterSave::Nothing,
            async move {
                match server {
                    Some(handle) => handle
                        .set_reads_require_auth(value)
                        .await
                        .map_err(RepoFailure::from),
                    None => ServerSettings::save_auth_required_for_reads(repo.as_ref(), value)
                        .await
                        .map_err(RepoFailure::from),
                }
            },
            cx,
        );
    }

    fn toggle_require_overlay_token(&mut self, cx: &mut Context<Self>) {
        if !self.settings_loaded() {
            return;
        }
        let prev = self.require_overlay_token;
        let value = !prev;
        self.require_overlay_token = value;
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        let server = self.server.clone();
        self.persist_field(
            prev,
            |this, prev, _| this.require_overlay_token = prev,
            AfterSave::Nothing,
            async move {
                ServerSettings::save_http_overlay_require_token(repo.as_ref(), value)
                    .await
                    .map_err(RepoFailure::from)?;
                if let Some(handle) = server {
                    handle.set_http_overlay_token_required(value).await;
                }
                Ok(())
            },
            cx,
        );
    }

    fn copy_overlay_token(&mut self, cx: &mut Context<Self>) {
        if let Some(token) = &self.overlay_token {
            crate::toasts::copy_to_clipboard(token.expose().to_owned(), cx);
        }
    }

    fn overlay_token_regenerate_disabled(&self) -> bool {
        self.overlay_token_regenerating || self.server.is_none()
    }

    fn request_overlay_token_regenerate(&mut self, cx: &mut Context<Self>) {
        if self.overlay_token_regenerate_disabled() {
            return;
        }
        self.overlay_token_confirm_open = true;
        cx.notify();
    }

    fn cancel_overlay_token_regenerate(&mut self, cx: &mut Context<Self>) {
        self.overlay_token_confirm_open = false;
        cx.notify();
    }

    fn confirm_overlay_token_regenerate(&mut self, cx: &mut Context<Self>) {
        self.overlay_token_confirm_open = false;
        let Some(handle) = self.server.clone() else {
            cx.notify();
            return;
        };
        if self.overlay_token_regenerating {
            cx.notify();
            return;
        }
        self.overlay_token_regenerating = true;
        let overlays = self.overlays.clone();
        let samples = Arc::clone(&self.overlay_samples);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                match handle
                    .regenerate_overlay_token(&overlays, samples.as_ref())
                    .await
                {
                    Ok(token) => Ok((token, TokenRegenerateNote::Clean)),
                    Err(forge_server::ServerError::OverlayRotationIncomplete { token, reason }) => {
                        Ok((token, TokenRegenerateNote::RewriteFailed(reason)))
                    }
                    Err(forge_server::ServerError::OverlayTokenRestoreFailed { token, reason }) => {
                        Ok((token, TokenRegenerateNote::RestoreFailed(reason)))
                    }
                    Err(e) => Err(RepoFailure::from(e)),
                }
            },
            |this, result: Result<(Secret, TokenRegenerateNote), RepoFailure>, cx| {
                this.finish_overlay_token_regenerate(result, cx)
            },
            cx,
        );
        cx.notify();
    }

    fn finish_overlay_token_regenerate(
        &mut self,
        result: Result<(Secret, TokenRegenerateNote), RepoFailure>,
        cx: &mut Context<Self>,
    ) {
        self.overlay_token_regenerating = false;
        match result {
            Ok((token, note)) => {
                overlay_link_token::publish(token, cx);
                match note {
                    TokenRegenerateNote::Clean => {}
                    TokenRegenerateNote::RewriteFailed(reason) => {
                        tracing::warn!(error = %reason, "overlay pages were not fully rewritten after regenerating the token");
                        ErrorSink::Toast.report(reason, cx);
                    }
                    TokenRegenerateNote::RestoreFailed(reason) => {
                        tracing::warn!(error = %reason, "previous overlay token could not be restored after a failed regenerate");
                        ErrorSink::Toast
                            .report(tr!("settings_ws_overlay_token_restore_failed"), cx);
                    }
                }
            }
            Err(message) => {
                tracing::warn!(error = %message, "failed to regenerate the overlay token");
                ErrorSink::Toast.report(message.text(), cx);
            }
        }
        cx.notify();
    }

    fn toggle_cors(&mut self, cx: &mut Context<Self>) {
        if !self.settings_loaded() {
            return;
        }
        let prev = self.cors_any_origin;
        let value = !prev;
        self.cors_any_origin = value;
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        self.persist_field(
            prev,
            |this, prev, _| this.cors_any_origin = prev,
            AfterSave::ReloadServer,
            async move {
                ServerSettings::save_overlay_cors_any_origin(repo.as_ref(), value)
                    .await
                    .map_err(RepoFailure::from)
            },
            cx,
        );
    }

    fn browse_overlay_folder(&mut self, cx: &mut Context<Self>) {
        async_bridge::run_async(
            &self.rt_handle,
            async_bridge::pick_folder(),
            |this, picked, cx| {
                if let Some(path) = picked {
                    this.apply_overlay_root(path, cx);
                }
            },
            cx,
        );
    }

    fn apply_overlay_root(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if !self.settings_loaded() {
            return;
        }
        let path_str = path.to_string_lossy().into_owned();
        let prev_root = self.overlay_root.replace(path_str.clone());
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        let overlays = self.overlays.clone();
        let samples = Arc::clone(&self.overlay_samples);
        self.save_state = SaveState::Saving;
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                ServerSettings::save_overlay_root(repo.as_ref(), &path_str)
                    .await
                    .map_err(RepoFailure::from)?;
                let pass = overlays.materialize_all(samples.as_ref()).await;
                Ok(OverlayBuildFailure::from_outcome(&pass))
            },
            |this, result: Result<Option<OverlayBuildFailure>, RepoFailure>, cx| {
                let build_failure = result.as_ref().ok().and_then(Option::as_ref);
                if let Some(failure) = build_failure {
                    failure.raise(cx);
                }
                if result.is_err() {
                    this.overlay_root = prev_root;
                }
                this.apply_persist_outcome(result.map(|_| ()), AfterSave::ReloadServer, cx);
            },
            cx,
        );
        cx.notify();
    }

    fn toggle_token_reveal(&mut self, cx: &mut Context<Self>) {
        self.token_revealed = !self.token_revealed;
        cx.notify();
    }

    fn copy_token(&mut self, cx: &mut Context<Self>) {
        crate::toasts::copy_to_clipboard(self.bearer_token.expose().to_owned(), cx);
    }

    fn request_regenerate(&mut self, cx: &mut Context<Self>) {
        self.regenerate_pending = true;
        cx.notify();
    }

    fn cancel_regenerate(&mut self, cx: &mut Context<Self>) {
        self.regenerate_pending = false;
        cx.notify();
    }

    fn confirm_regenerate(&mut self, cx: &mut Context<Self>) {
        self.regenerate_pending = false;
        self.regenerate_token(cx);
        cx.notify();
    }

    fn regenerate_token(&mut self, cx: &mut Context<Self>) {
        let Some(handle) = self.server.clone() else {
            return;
        };
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                handle
                    .regenerate_bearer_token()
                    .await
                    .map_err(RepoFailure::from)
            },
            |this, result: Result<Secret, RepoFailure>, cx| match result {
                Ok(token) => {
                    this.bearer_token = token;
                    cx.notify();
                }
                Err(reason) => server_bearer::token_regenerate_failed(&reason.text(), cx),
            },
            cx,
        );
    }

    fn header(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> SettingsPaneHeader {
        let density = cx.density();
        SettingsPaneHeader::new(SettingsSection::WebSocket)
            .subtitle(tr!("settings_ws_subtitle"))
            .trailing(self.restart_button(palette, cx))
            .trailing(
                div()
                    .flex()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .child(save_indicator(&self.save_state, palette, density)),
            )
    }

    fn bind_section(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        setting_block(
            tr!("settings_ws_bind_section_title"),
            Some(tr!("settings_ws_bind_section_subtitle").into()),
            div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xs, density))
                .child(self.bind_card(BindChoice::Localhost, palette, density, cx))
                .child(self.bind_card(BindChoice::Lan, palette, density, cx)),
            palette,
            density,
        )
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
                LOCALHOST_BIND_ADDRESS.to_owned(),
                tr!("settings_ws_bind_localhost_description"),
                palette.brand,
                Icon::Lock,
                tr!("settings_ws_badge_recommended"),
                palette.success,
            ),
            BindChoice::Lan => (
                tr!("settings_ws_bind_lan_title"),
                LAN_BIND_ADDRESS.to_owned(),
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
            .gap(scaled(px(3.0), density))
            .py(scaled(px(1.0), density))
            .px(spacing(Spacing::Xs, density))
            .rounded(radius(Radius::Sm))
            .bg(palette.surface_overlay)
            .child(icon(badge_glyph, BADGE_GLYPH, badge_color))
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
            .gap(scaled(px(2.0), density))
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
            .row_border(BORDER_THIN, BORDER_THIN)
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
        if self.additional_origins_unreadable {
            return setting_block(
                tr!("settings_ws_origins_section_title"),
                Some(tr!("settings_ws_origins_subtitle").into()),
                error_row(tr!("settings_ws_origins_unreadable"), palette, density),
                palette,
                density,
            );
        }
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
                    .child(icon(Icon::AlertTriangle, WARNING_GLYPH, palette.danger))
                    .child(
                        div()
                            .flex_none()
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.danger)
                            .child(tr!("settings_ws_origins_invalid")),
                    )
                    .child(
                        div()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .text_ellipsis()
                            .font_family(mono_family())
                            .text_size(FONT_XS)
                            .text_color(palette.danger)
                            .child(rejected.clone()),
                    ),
            );
        }

        setting_block(
            tr!("settings_ws_origins_section_title"),
            Some(tr!("settings_ws_origins_subtitle").into()),
            div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xs, density))
                .child(self.origins_input.clone())
                .child(footer.child(div().flex_1().min_w(px(0.0))).child(apply)),
            palette,
            density,
        )
    }

    fn port_column(&self, palette: &ForgePalette, density: Density) -> impl IntoElement {
        setting_block(
            tr!("settings_ws_port_section_title"),
            Some(tr!("settings_ws_port_subtitle").into()),
            self.port_input.clone(),
            palette,
            density,
        )
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
            self.bearer_token.expose().to_owned()
        } else {
            forge_server::masked_token(self.bearer_token.expose())
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
            .py(scaled(FIELD_PAD_V, density))
            .px(spacing(Spacing::Sm, density))
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
                            .child(icon(reveal_glyph, TOKEN_ACTION_GLYPH, palette.text_faint)),
                    )
                    .child(
                        div()
                            .id("settings-ws-token-copy")
                            .flex()
                            .cursor_pointer()
                            .on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| this.copy_token(cx)),
                            )
                            .child(icon(Icon::Copy, TOKEN_ACTION_GLYPH, palette.text_faint)),
                    ),
            );

        let regenerate =
            ghost_button_with_icon(Icon::Refresh, tr!("server_btn_regenerate"), palette)
                .ink(palette.warning)
                .on_click(
                    "settings-ws-token-regen",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.request_regenerate(cx)),
                );

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
        let rows = setting_rows(palette)
            .row(
                setting_row(
                    tr!("settings_ws_auth_require_ws_label"),
                    Some(tr!("settings_ws_auth_require_ws_sublabel").into()),
                    toggle(self.require_ws_token, palette).on_click(
                        "settings-ws-auth-ws",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_require_ws_token(cx)),
                    ),
                    palette,
                    density,
                )
                .leading(Icon::Lock, palette.success),
            )
            .row(
                setting_row(
                    tr!("settings_ws_auth_require_http_label"),
                    Some(tr!("settings_ws_auth_require_http_sublabel").into()),
                    toggle(self.require_overlay_token, palette).on_click(
                        "settings-ws-auth-http",
                        cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.toggle_require_overlay_token(cx)
                        }),
                    ),
                    palette,
                    density,
                )
                .leading(Icon::Globe, palette.info),
            )
            .row(
                setting_row(
                    tr!("settings_ws_auth_cors_label"),
                    Some(tr!("settings_ws_auth_cors_sublabel").into()),
                    toggle(self.cors_any_origin, palette).on_click(
                        "settings-ws-auth-cors",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_cors(cx)),
                    ),
                    palette,
                    density,
                )
                .leading(Icon::AlertTriangle, palette.warning),
            );
        setting_block(
            tr!("settings_ws_auth_section_title"),
            Some(tr!("settings_ws_auth_section_subtitle").into()),
            div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Md, density))
                .child(rows)
                .child(self.overlay_token_block(palette, density, cx)),
            palette,
            density,
        )
    }

    fn overlay_token_block(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let shown =
            overlay_link_token::displayed(self.overlay_token.as_ref().map_or("", Secret::expose));

        let mut field = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(spacing(Spacing::Xs, density))
            .py(scaled(FIELD_PAD_V, density))
            .px(spacing(Spacing::Sm, density))
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
            );
        if self.overlay_token.is_some() {
            field = field.child(
                div()
                    .id("settings-ws-overlay-token-copy")
                    .flex()
                    .flex_none()
                    .cursor_pointer()
                    .on_click(
                        cx.listener(|this, _: &ClickEvent, _, cx| this.copy_overlay_token(cx)),
                    )
                    .child(icon(Icon::Copy, TOKEN_ACTION_GLYPH, palette.text_faint)),
            );
        }

        let regenerate =
            ghost_button_with_icon(Icon::Refresh, tr!("server_btn_regenerate"), palette)
                .ink(palette.warning)
                .disabled(self.overlay_token_regenerate_disabled())
                .busy(self.overlay_token_regenerating)
                .on_click(
                    "settings-ws-overlay-token-regen",
                    cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.request_overlay_token_regenerate(cx)
                    }),
                );

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(field_title(tr!("settings_ws_overlay_token_title"), palette))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(tr!("settings_ws_overlay_token_subtitle")),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(field)
                    .child(regenerate),
            )
    }

    fn overlay_token_confirm(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let confirm = confirm_modal(
            tr!("settings_ws_overlay_token_regen_title"),
            tr!("settings_ws_overlay_token_regen_message"),
            ConfirmTone::Warning,
            palette,
        )
        .on_cancel(
            "settings-ws-overlay-token-regen-cancel",
            tr!("common_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_overlay_token_regenerate(cx)),
        )
        .on_confirm(
            "settings-ws-overlay-token-regen-confirm",
            tr!("server_btn_regenerate"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_overlay_token_regenerate(cx)),
        );
        let view = cx.entity();
        overlay(confirm, palette)
            .position(OverlayPosition::Center)
            .on_dismiss(
                "settings-ws-overlay-token-regen-scrim",
                move |_window, cx| {
                    view.update(cx, |this, cx| this.cancel_overlay_token_regenerate(cx));
                },
            )
            .into_any_element()
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
            .unwrap_or_else(|| self.default_overlay_root.clone());
        let path_box = div()
            .flex_1()
            .min_w(px(0.0))
            .overflow_hidden()
            .text_ellipsis()
            .py(scaled(FIELD_PAD_V, density))
            .px(spacing(Spacing::Sm, density))
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let port_token = div()
            .w_full()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_start()
            .gap(spacing(Spacing::Md, density))
            .child(grow_cell(self.port_column(&palette, density), 1.0).min_w(PORT_COLUMN_MIN_W))
            .child(
                grow_cell(self.token_column(&palette, density, cx), 1.6).min_w(TOKEN_COLUMN_MIN_W),
            );

        let enable_row = setting_rows(&palette).row(
            setting_row(
                tr!("settings_ws_enable_label"),
                Some(tr!("settings_ws_enable_description").into()),
                self.lifecycle_controls(&palette, density, cx),
                &palette,
                density,
            )
            .leading(Icon::Power, palette.success),
        );

        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .children(crate::server_unavailable::unavailable_banner(cx, &palette))
            .children(self.load_failure.clone().map(|reason| {
                error_row_with_retry(
                    "settings-ws-load-retry",
                    tr!("settings_ws_load_failed", reason = reason.as_ref()),
                    &palette,
                    cx.listener(|this, _: &ClickEvent, _, cx| this.load(cx)),
                    density,
                )
            }))
            .child(
                div()
                    .when(!self.settings_loaded(), |d| d.opacity(DISABLED_OPACITY))
                    .child(setting_sections(
                        [
                            enable_row.into_any_element(),
                            self.bind_section(&palette, density, cx).into_any_element(),
                            self.origins_section(&palette, density, cx)
                                .into_any_element(),
                            port_token.into_any_element(),
                            self.auth_section(&palette, density, cx).into_any_element(),
                            self.overlay_section(&palette, density, cx)
                                .into_any_element(),
                        ],
                        &palette,
                        density,
                    )),
            );

        let mut root = div()
            .flex()
            .flex_col()
            .child(self.header(&palette, cx))
            .child(body);
        if let Some(modal) = &self.lan_modal {
            root = root.child(self.lan_overlay(modal.clone(), &palette, cx));
        }
        if self.overlay_token_confirm_open {
            root = root.child(self.overlay_token_confirm(&palette, cx));
        }
        if self.regenerate_pending {
            root = root.child(server_bearer::regenerate_confirm(
                &palette,
                Self::cancel_regenerate,
                Self::confirm_regenerate,
                cx,
            ));
        }
        root
    }
}

async fn load_websocket_settings(
    repo: Arc<dyn SettingsRepo>,
) -> Result<WebSocketSnapshot, RepoFailure> {
    let snap = ServerSettings::load(repo.as_ref())
        .await
        .map_err(RepoFailure::from)?;
    Ok(WebSocketSnapshot {
        enabled: snap.enabled,
        lan_bind_enabled: snap.lan_bind_enabled,
        port: snap.port,
        require_ws_token: snap.auth_required_for_reads,
        require_overlay_token: snap.http_overlay_require_token,
        cors_any_origin: snap.overlay_cors_any_origin,
        overlay_root: snap.overlay_root,
        additional_origins: snap.additional_origins,
        additional_origins_unreadable: snap.additional_origins_unreadable,
    })
}

struct RejectedOrigin(String);

fn parse_origins(raw: &str) -> Result<Vec<String>, RejectedOrigin> {
    let mut accepted: Vec<String> = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some(origin) = normalize_origin(trimmed) else {
            return Err(RejectedOrigin(trimmed.to_owned()));
        };
        if !accepted.contains(&origin) {
            accepted.push(origin);
        }
    }
    Ok(accepted)
}

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

pub(crate) fn host_is_valid(host: &str) -> bool {
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

#[cfg(test)]
mod tests {
    use forge_components::ThemeId;
    use forge_overlay::OverlayKindRegistry;
    use forge_runtime::EventBus;
    use forge_server::settings::{
        ADDITIONAL_ORIGINS, AUTH_REQUIRED_FOR_READS, BIND_ADDRESS, ENABLED,
        HTTP_OVERLAY_REQUIRE_TOKEN, OVERLAY_CORS_ANY_ORIGIN, PORT,
    };
    use forge_storage::settings::core_settings::SERVER_OVERLAY_ROOT;
    use gpui::TestAppContext;

    use super::*;
    use crate::test_support::{
        StubEventLog, StubOverlays, TestBackend, pump, pump_until, runtime, stopped_handle,
        test_backend,
    };

    const STORED_ORIGIN: &str = "http://stored.test:9000";
    const TYPED_ORIGIN: &str = "http://typed.test:9100";
    const RESTART_FAILURE: &str = "address already in use";
    const SETTLE_ROUNDS: usize = 2_000;
    const SETTLE_NAP: std::time::Duration = std::time::Duration::from_millis(1);
    const OVERLAY_PAGE: &str = "page.html";
    const EPHEMERAL_PORT: &str = "0";
    const PAGE_SERVED: &str = "200";
    const PAGE_REFUSED: &str = "404";

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
        let overlays = OverlayServiceHandle::new(forge_runtime::OverlayServiceParts {
            repo: Arc::new(StubOverlays),
            settings: Arc::clone(&backend) as Arc<dyn SettingsRepo>,
            kinds: Arc::new(OverlayKindRegistry::new()),
            bus: EventBus::new(Arc::new(StubEventLog)),
            frames: Arc::new(crate::test_support::NoOverlayPages),
            media: None,
            speaker: None,
            latest: None,
        });
        let handle = rt.handle().clone();
        cx.update(|cx| {
            cx.set_global(forge_components::Presentation::new(
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
                Arc::new(crate::test_support::NeutralOverlaySamples),
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
        settle(cx, &rt);

        type_origins(cx, &view, TYPED_ORIGIN);
        release(cx, view);
        pump(&rt);

        let written: Vec<Vec<String>> = origin_writes(&mut writes)
            .iter()
            .map(|value| serde_json::from_str(value).expect("origins json"))
            .collect();
        assert_eq!(written, vec![vec![TYPED_ORIGIN.to_owned()]]);
    }

    #[gpui::test]
    fn leaving_the_pane_writes_nothing_when_the_typed_origins_add_nothing(cx: &mut TestAppContext) {
        for (typed, case) in [
            ("http://:not-an-origin", "text that is not a valid origin"),
            (STORED_ORIGIN, "the list that is already stored"),
        ] {
            let rt = runtime();
            let (backend, mut writes) = test_backend();
            backend.seed_raw(
                &ADDITIONAL_ORIGINS,
                &serde_json::to_string(&[STORED_ORIGIN]).unwrap(),
            );
            let view = settings_view(cx, &rt, backend);
            settle(cx, &rt);

            type_origins(cx, &view, typed);
            release(cx, view);
            pump(&rt);

            assert_eq!(origin_writes(&mut writes), Vec::<String>::new(), "{case}");
        }
    }

    fn origin_writes(
        writes: &mut tokio::sync::mpsc::UnboundedReceiver<crate::test_support::SettingWrite>,
    ) -> Vec<String> {
        crate::test_support::writes_to(writes, ADDITIONAL_ORIGINS.name())
    }

    fn commit_and_leave(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        view: Entity<SettingsWebSocketView>,
    ) {
        type_origins(cx, &view, TYPED_ORIGIN);
        view.update(cx, |this, cx| this.commit_origins(cx));
        settle(cx, rt);
        type_origins(cx, &view, STORED_ORIGIN);
        release(cx, view);
        pump(rt);
    }

    #[gpui::test]
    fn unreadable_stored_origins_lock_origin_edits_while_other_toggles_still_save(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let (backend, mut writes) = test_backend();
        backend.seed_raw(&ADDITIONAL_ORIGINS, "not json");
        let view = settings_view(cx, &rt, Arc::clone(&backend));
        settle(cx, &rt);

        let flagged = view.read_with(cx, |this, _| this.additional_origins_unreadable);
        view.update(cx, |this, cx| this.toggle_require_ws_token(cx));
        settle(cx, &rt);
        commit_and_leave(cx, &rt, view);
        let written: Vec<_> = std::iter::from_fn(|| writes.try_recv().ok()).collect();
        let written_to = |key: &str| -> Vec<String> {
            written
                .iter()
                .filter(|(written, _)| written == key)
                .map(|(_, value)| value.clone())
                .collect()
        };

        assert_eq!(
            (
                flagged,
                written_to(AUTH_REQUIRED_FOR_READS.name()),
                written_to(ADDITIONAL_ORIGINS.name()),
                backend.row(ADDITIONAL_ORIGINS.name()).map(|row| row.value),
            ),
            (
                true,
                vec!["true".to_owned()],
                Vec::<String>::new(),
                Some("not json".to_owned()),
            )
        );
    }

    #[gpui::test]
    fn no_origin_edit_writes_while_the_settings_are_still_loading(cx: &mut TestAppContext) {
        let rt = runtime();
        let (backend, mut writes) = test_backend();
        backend.fault_reads(
            ADDITIONAL_ORIGINS.name(),
            crate::test_support::ReadFault::Held(Arc::new(tokio::sync::Notify::new())),
        );
        let view = settings_view(cx, &rt, backend);
        settle(cx, &rt);

        commit_and_leave(cx, &rt, view);

        assert_eq!(origin_writes(&mut writes), Vec::<String>::new());
    }

    #[gpui::test]
    fn no_origin_edit_writes_after_the_settings_fail_to_load(cx: &mut TestAppContext) {
        let rt = runtime();
        let (backend, mut writes) = test_backend();
        backend.fault_reads(
            ADDITIONAL_ORIGINS.name(),
            crate::test_support::ReadFault::Fails,
        );
        let view = settings_view(cx, &rt, backend);
        settle(cx, &rt);

        commit_and_leave(cx, &rt, view);

        assert_eq!(origin_writes(&mut writes), Vec::<String>::new());
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

                this.finish_restart(Err(RepoFailure::operation(RESTART_FAILURE)), cx);

                assert!(!this.restart_queued, "{case}");
                assert_eq!(this.restarting, expect_rerun, "{case}");
            });
        }
    }

    fn live_overlay_token(
        rt: &tokio::runtime::Runtime,
        server: &ServerHandle,
    ) -> forge_types::Secret {
        rt.block_on(server.overlay_token())
    }

    fn overlay_page_status(rt: &tokio::runtime::Runtime, server: &ServerHandle) -> String {
        rt.block_on(async {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};

            let addr = server.bind_addr().await;
            let mut stream = tokio::net::TcpStream::connect(addr)
                .await
                .expect("connect to the running server");
            let request = format!(
                "GET /{}/{OVERLAY_PAGE} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n",
                forge_server::overlay_url::OVERLAY_ROUTE
            );
            stream
                .write_all(request.as_bytes())
                .await
                .expect("send the overlay request");
            let mut response = String::new();
            stream
                .read_to_string(&mut response)
                .await
                .expect("read the overlay response");
            response
                .split_whitespace()
                .nth(1)
                .expect("status code")
                .to_owned()
        })
    }

    fn settle(cx: &mut TestAppContext, rt: &tokio::runtime::Runtime) {
        for _ in 0..4 {
            pump(rt);
            cx.run_until_parked();
        }
    }

    fn settle_until(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        view: &Entity<SettingsWebSocketView>,
        done: impl Fn(&SettingsWebSocketView) -> bool,
    ) {
        for _ in 0..SETTLE_ROUNDS {
            settle(cx, rt);
            if view.read_with(cx, |this, _| done(this)) {
                return;
            }
            rt.block_on(async { tokio::time::sleep(SETTLE_NAP).await });
        }
        panic!("the pane never settled");
    }

    fn running_server_serving_one_overlay_page(
        rt: &tokio::runtime::Runtime,
        backend: &Arc<TestBackend>,
        overlay_root: &std::path::Path,
    ) -> ServerHandle {
        std::fs::write(overlay_root.join(OVERLAY_PAGE), b"<html></html>").expect("overlay page");
        backend.seed_raw(&PORT, EPHEMERAL_PORT);
        backend.seed_raw(&SERVER_OVERLAY_ROOT, &overlay_root.to_string_lossy());
        let server = stopped_handle(rt, backend);
        rt.block_on(server.restart()).expect("server starts");
        server
    }

    type ShownFields = (
        bool,
        bool,
        bool,
        bool,
        BindChoice,
        u16,
        Vec<String>,
        String,
        String,
    );

    fn shown_fields(this: &SettingsWebSocketView, cx: &App) -> ShownFields {
        (
            this.enable_server,
            this.require_ws_token,
            this.require_overlay_token,
            this.cors_any_origin,
            this.bind_choice,
            this.port,
            this.additional_origins.clone(),
            this.port_input.read(cx).content().to_owned(),
            this.origins_input.read(cx).content().to_owned(),
        )
    }

    #[gpui::test]
    fn a_failed_save_puts_the_edited_field_back_and_reports_the_error(cx: &mut TestAppContext) {
        type Edit = fn(&mut SettingsWebSocketView, &mut Context<SettingsWebSocketView>);
        let edits: [(&str, Edit); 7] = [
            (ENABLED.name(), |this, cx| this.toggle_enable(cx)),
            (AUTH_REQUIRED_FOR_READS.name(), |this, cx| {
                this.toggle_require_ws_token(cx)
            }),
            (HTTP_OVERLAY_REQUIRE_TOKEN.name(), |this, cx| {
                this.toggle_require_overlay_token(cx)
            }),
            (OVERLAY_CORS_ANY_ORIGIN.name(), |this, cx| {
                this.toggle_cors(cx)
            }),
            (BIND_ADDRESS.name(), |this, cx| this.confirm_lan(cx)),
            (PORT.name(), |this, cx| {
                this.port_input
                    .update(cx, |input, cx| input.set_content("9321", cx));
                this.commit_port(cx);
            }),
            (ADDITIONAL_ORIGINS.name(), |this, cx| {
                this.origins_input
                    .update(cx, |input, cx| input.set_content(TYPED_ORIGIN, cx));
                this.commit_origins(cx);
            }),
        ];
        for (key, edit) in edits {
            let rt = runtime();
            let (backend, _writes) = test_backend();
            backend.fail_writes(key);
            let view = settings_view(cx, &rt, backend);
            settle(cx, &rt);
            let before = view.read_with(cx, shown_fields);

            view.update(cx, edit);
            pump_until(&rt, || {
                cx.run_until_parked();
                view.read_with(cx, |this, _| matches!(this.save_state, SaveState::Error(_)))
            });

            assert_eq!(view.read_with(cx, shown_fields), before, "{key}");
        }
    }

    #[gpui::test]
    fn committing_an_unchanged_port_writes_nothing(cx: &mut TestAppContext) {
        let rt = runtime();
        let (backend, mut writes) = test_backend();
        let view = settings_view(cx, &rt, backend);
        settle(cx, &rt);
        let shown = view.read_with(cx, |this, _| this.port.to_string());

        view.update(cx, |this, cx| {
            this.port_input
                .update(cx, |input, cx| input.set_content(shown.clone(), cx));
            this.commit_port(cx);
        });
        settle(cx, &rt);

        assert_eq!(
            crate::test_support::writes_to(&mut writes, PORT.name()),
            Vec::<String>::new()
        );
    }

    #[gpui::test]
    fn committing_a_new_port_writes_it_once(cx: &mut TestAppContext) {
        const NEXT_PORT: &str = "9321";
        let rt = runtime();
        let (backend, mut writes) = test_backend();
        let view = settings_view(cx, &rt, backend);
        settle(cx, &rt);

        view.update(cx, |this, cx| {
            this.port_input
                .update(cx, |input, cx| input.set_content(NEXT_PORT, cx));
            this.commit_port(cx);
        });
        settle(cx, &rt);

        assert_eq!(
            crate::test_support::writes_to(&mut writes, PORT.name()),
            [NEXT_PORT]
        );
    }

    #[gpui::test]
    fn enabling_keeps_the_saved_switch_on_and_reports_a_server_that_cannot_start(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let squatter = rt
            .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
            .expect("squatter bind");
        let port = squatter.local_addr().expect("squatter addr").port();
        let (backend, _writes) = test_backend();
        backend.seed_raw(&ENABLED, "false");
        backend.seed_raw(&BIND_ADDRESS, "127.0.0.1");
        backend.seed_raw(&PORT, &port.to_string());
        let server = stopped_handle(&rt, &backend);
        let view = settings_view_with_server(cx, &rt, Arc::clone(&backend), Some(server));
        settle(cx, &rt);

        view.update(cx, |this, cx| this.toggle_enable(cx));
        pump_until(&rt, || {
            cx.run_until_parked();
            view.read_with(cx, |this, _| matches!(this.save_state, SaveState::Error(_)))
        });

        assert_eq!(
            (
                view.read_with(cx, |this, _| this.enable_server),
                backend.row(ENABLED.name()).map(|row| row.value),
            ),
            (true, Some("true".to_owned()))
        );
        drop(squatter);
    }

    #[gpui::test]
    fn switching_off_the_overlay_token_requirement_persists_it(cx: &mut TestAppContext) {
        let rt = runtime();
        let (backend, mut writes) = test_backend();
        let server = stopped_handle(&rt, &backend);
        let view = settings_view_with_server(cx, &rt, backend, Some(server));
        settle(cx, &rt);
        while writes.try_recv().is_ok() {}

        view.update(cx, |this, cx| this.toggle_require_overlay_token(cx));
        settle(cx, &rt);

        assert_eq!(
            crate::test_support::writes_to(&mut writes, HTTP_OVERLAY_REQUIRE_TOKEN.name()),
            vec!["false".to_owned()]
        );
    }

    #[gpui::test]
    fn switching_off_the_overlay_token_requirement_opens_the_running_gate(cx: &mut TestAppContext) {
        let rt = runtime();
        let overlay_root = tempfile::tempdir().expect("tempdir");
        let (backend, _writes) = test_backend();
        let server = running_server_serving_one_overlay_page(&rt, &backend, overlay_root.path());
        let refused_before = overlay_page_status(&rt, &server);
        let view = settings_view_with_server(cx, &rt, backend, Some(server.clone()));
        settle(cx, &rt);

        view.update(cx, |this, cx| this.toggle_require_overlay_token(cx));
        settle(cx, &rt);

        assert_eq!(
            (refused_before, overlay_page_status(&rt, &server)),
            (PAGE_REFUSED.to_owned(), PAGE_SERVED.to_owned())
        );
        rt.block_on(server.stop()).expect("server stops");
    }

    #[gpui::test]
    fn toggling_the_read_auth_requirement_with_a_server_persists_it_and_flips_the_live_policy(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let (backend, mut writes) = test_backend();
        let server = stopped_handle(&rt, &backend);
        let before = rt.block_on(server.reads_require_auth());
        let view = settings_view_with_server(cx, &rt, backend, Some(server.clone()));
        settle(cx, &rt);
        while writes.try_recv().is_ok() {}

        view.update(cx, |this, cx| this.toggle_require_ws_token(cx));
        settle(cx, &rt);

        assert_eq!(
            (
                before,
                rt.block_on(server.reads_require_auth()),
                crate::test_support::writes_to(&mut writes, AUTH_REQUIRED_FOR_READS.name()),
            ),
            (false, true, vec!["true".to_owned()])
        );
    }

    #[gpui::test]
    fn regenerate_waits_for_confirmation_and_cancel_keeps_the_token(cx: &mut TestAppContext) {
        let rt = runtime();
        let (backend, _writes) = test_backend();
        let server = stopped_handle(&rt, &backend);
        let before = live_overlay_token(&rt, &server);
        let view = settings_view_with_server(cx, &rt, backend, Some(server.clone()));

        view.update(cx, |this, cx| this.request_overlay_token_regenerate(cx));
        assert!(view.read_with(cx, |this, _| this.overlay_token_confirm_open));
        view.update(cx, |this, cx| this.cancel_overlay_token_regenerate(cx));
        settle(cx, &rt);

        assert!(!view.read_with(cx, |this, _| this.overlay_token_confirm_open));
        assert_eq!(live_overlay_token(&rt, &server), before);
    }

    #[gpui::test]
    fn regenerate_is_not_offered_without_a_server(cx: &mut TestAppContext) {
        let rt = runtime();
        let (backend, _writes) = test_backend();
        let view = settings_view(cx, &rt, backend);

        view.update(cx, |this, cx| this.request_overlay_token_regenerate(cx));

        assert!(!view.read_with(cx, |this, _| this.overlay_token_confirm_open));
    }

    #[gpui::test]
    fn a_confirmed_regenerate_rotates_the_live_token_and_shows_the_new_one(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let overlay_root = tempfile::tempdir().expect("tempdir");
        let (backend, _writes) = test_backend();
        backend.seed_raw(&SERVER_OVERLAY_ROOT, &overlay_root.path().to_string_lossy());
        let server = stopped_handle(&rt, &backend);
        let before = live_overlay_token(&rt, &server);
        let view = settings_view_with_server(cx, &rt, backend, Some(server.clone()));
        settle(cx, &rt);

        view.update(cx, |this, cx| {
            this.request_overlay_token_regenerate(cx);
            this.confirm_overlay_token_regenerate(cx);
        });
        settle_until(cx, &rt, &view, |this| !this.overlay_token_regenerating);

        let after = live_overlay_token(&rt, &server);
        assert_ne!(
            after, before,
            "the confirmed regenerate never rotated the token"
        );
        assert_eq!(
            view.read_with(cx, |this, _| this.overlay_token.clone()),
            Some(after),
            "the pane still shows the retired token"
        );
    }

    #[gpui::test]
    fn a_regenerate_whose_pages_fail_to_rewrite_still_shows_the_live_token(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let scratch = tempfile::tempdir().expect("tempdir");
        let not_a_directory = scratch.path().join("overlays-file");
        std::fs::write(&not_a_directory, b"x").expect("write file");
        let (backend, _writes) = test_backend();
        backend.seed_raw(&SERVER_OVERLAY_ROOT, &not_a_directory.to_string_lossy());
        let server = stopped_handle(&rt, &backend);
        let before = live_overlay_token(&rt, &server);
        cx.update(|cx| cx.set_global(forge_components::Toasts::new()));
        let view = settings_view_with_server(cx, &rt, backend, Some(server.clone()));
        settle(cx, &rt);

        view.update(cx, |this, cx| {
            this.request_overlay_token_regenerate(cx);
            this.confirm_overlay_token_regenerate(cx);
        });
        settle_until(cx, &rt, &view, |this| !this.overlay_token_regenerating);

        let live = live_overlay_token(&rt, &server);
        assert_ne!(live, before);
        assert_eq!(
            view.read_with(cx, |this, _| this.overlay_token.clone()),
            Some(live),
            "the pane offers a token the server already refuses"
        );
    }
}
