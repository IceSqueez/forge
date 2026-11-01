use std::future::Future;
use std::sync::Arc;

use forge_components::{
    ActivePresentation, DISABLED_OPACITY, Density, FONT_SM, FONT_XS, ForgePalette, Icon,
    InputEvent, SaveState, Spacing, TextArea, TextInput, TypeToConfirm, error_row_with_retry,
    ghost_button_with_icon, grow_cell, save_indicator, setting_row, setting_rows, setting_sections,
    spacing, toggle, tr,
};
use forge_runtime::{OverlaySampleSource, OverlayServiceHandle};
use forge_server::{DEFAULT_PORT, ServerHandle, ServerSettings};
use forge_storage::{CredentialsRepo, DataProvider, SettingsRepo};
use forge_types::Secret;
use gpui::{
    ClickEvent, Context, Entity, Pixels, SharedString, Subscription, Task, Window, div, prelude::*,
    px,
};

use crate::async_bridge;
use crate::navigation::SettingsSection;
use crate::overlay_link_token;
use crate::repo_failure::RepoFailure;
use crate::server_bearer;
use crate::server_restart::restart_ignoring_disabled;
use crate::settings_pane_header::SettingsPaneHeader;

mod bind;
mod origins;
mod overlay_folder;
mod tokens;

pub(crate) use origins::host_is_valid;

const ORIGINS_AREA_HEIGHT: Pixels = px(72.0);
const PORT_COLUMN_MIN_W: Pixels = px(140.0);
const TOKEN_COLUMN_MIN_W: Pixels = px(230.0);
const FIELD_PAD_V: Pixels = px(7.0);

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

pub(crate) struct SettingsWebSocketView {
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
    _run_state_bridge: Option<Task<()>>,
}

impl SettingsWebSocketView {
    pub(crate) fn new(
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
            .is_some_and(|handle| handle.run_state().is_running());
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
            default_overlay_root: crate::app_paths::app_paths()
                .overlays_dir()
                .display()
                .to_string(),
            lan_modal: None,
            lan_sub: None,
            _subs: subs,
            _run_state_bridge: None,
        };
        async_bridge::attach_to_window(Self::attach_origins_blur, cx);
        view.load(cx);
        view.fetch_token(cx);
        view.start_run_state_bridge(cx);
        view
    }

    fn start_run_state_bridge(&mut self, cx: &mut Context<Self>) {
        let Some(handle) = self.server.as_ref() else {
            return;
        };
        let mut run_state = handle.run_state();
        self._run_state_bridge = Some(cx.spawn(async move |this, cx| {
            loop {
                let running = run_state.latest();
                if this
                    .update(cx, |this, cx| this.apply_run_state(running, cx))
                    .is_err()
                    || run_state.changed().await.is_none()
                {
                    break;
                }
            }
        }));
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
                    .font_family(cx.body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.warning)
                    .child(tr!("server_btn_restarting")),
            );
        } else if self.enable_server && !self.running {
            row = row.child(
                div()
                    .flex_none()
                    .font_family(cx.body_family())
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

    fn settings_loaded(&self) -> bool {
        !self.loading && self.load_failure.is_none()
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
                    .child(save_indicator(&self.save_state, palette, density, cx)),
            )
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
            .child(grow_cell(self.port_column(&palette, density, cx), 1.0).min_w(PORT_COLUMN_MIN_W))
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
                    cx,
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
    use gpui::{App, TestAppContext};

    use super::*;
    use crate::test_support::{
        StubEventLog, StubOverlays, TestBackend, pump, pump_until, runtime, settle_until,
        stopped_handle, test_backend,
    };

    const STORED_ORIGIN: &str = "http://stored.test:9000";
    const TYPED_ORIGIN: &str = "http://typed.test:9100";
    const RESTART_FAILURE: &str = "address already in use";
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
            date_names: forge_runtime::DateNamesHandle::default(),
            tasks: forge_net::TaskOwner::new(),
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

    fn overlay_page_status(rt: &tokio::runtime::Runtime, server: &ServerHandle) -> String {
        rt.block_on(async {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};

            let addr = server.bind_addr().await;
            let mut stream = tokio::net::TcpStream::connect(addr)
                .await
                .expect("connect to the running server");
            let request = format!(
                "GET /{}/{OVERLAY_PAGE} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n",
                forge_server::OVERLAY_ROUTE
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
        crate::test_support::settle_runtime_work(cx, rt);
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
        let before = server.reads_require_auth();
        let view = settings_view_with_server(cx, &rt, backend, Some(server.clone()));
        settle(cx, &rt);
        while writes.try_recv().is_ok() {}

        view.update(cx, |this, cx| this.toggle_require_ws_token(cx));
        settle(cx, &rt);

        assert_eq!(
            (
                before,
                server.reads_require_auth(),
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
        let before = server.overlay_token();
        let view = settings_view_with_server(cx, &rt, backend, Some(server.clone()));

        view.update(cx, |this, cx| this.request_overlay_token_regenerate(cx));
        assert!(view.read_with(cx, |this, _| this.overlay_token_confirm_open));
        view.update(cx, |this, cx| this.cancel_overlay_token_regenerate(cx));
        settle(cx, &rt);

        assert!(!view.read_with(cx, |this, _| this.overlay_token_confirm_open));
        assert_eq!(server.overlay_token(), before);
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
        let before = server.overlay_token();
        let view = settings_view_with_server(cx, &rt, backend, Some(server.clone()));
        settle(cx, &rt);

        view.update(cx, |this, cx| {
            this.request_overlay_token_regenerate(cx);
            this.confirm_overlay_token_regenerate(cx);
        });
        settle_until(cx, &rt, |cx| {
            view.read_with(cx, |this, _| !this.overlay_token_regenerating)
        });

        let after = server.overlay_token();
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
        let before = server.overlay_token();
        cx.update(|cx| cx.set_global(forge_components::Toasts::new()));
        let view = settings_view_with_server(cx, &rt, backend, Some(server.clone()));
        settle(cx, &rt);

        view.update(cx, |this, cx| {
            this.request_overlay_token_regenerate(cx);
            this.confirm_overlay_token_regenerate(cx);
        });
        settle_until(cx, &rt, |cx| {
            view.read_with(cx, |this, _| !this.overlay_token_regenerating)
        });

        let live = server.overlay_token();
        assert_ne!(live, before);
        assert_eq!(
            view.read_with(cx, |this, _| this.overlay_token.clone()),
            Some(live),
            "the pane offers a token the server already refuses"
        );
    }
}
