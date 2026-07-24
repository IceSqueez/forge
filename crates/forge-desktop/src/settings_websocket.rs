use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;

use forge_components::{
    BORDER_ACCENT, BORDER_THIN, BulletItem, BulletKind, Density, FONT_LG, FONT_SM, FONT_XS,
    FONT_XXS, ForgePalette, Icon, InputEvent, OverlayPosition, Radius, SaveState, Spacing,
    TextInput, TypeToConfirm, TypeToConfirmEvent, body_family, field_hint, field_title,
    ghost_button_with_icon, icon, mono_family, overlay, radio_row, radius, save_indicator,
    setting_row, spacing, toggle, tr, type_to_confirm,
};
use forge_server::{ServerHandle, ServerSettings};
use forge_storage::{CredentialId, CredentialsRepo, DataProvider, SettingsRepo};
use gpui::{
    AnyElement, App, ClickEvent, Context, Div, Entity, FontWeight, Rgba, SharedString,
    Subscription, Window, div, prelude::*, px, relative,
};

use crate::async_bridge::{self, ErrorSink};
use crate::presentation::ActivePresentation;

const BEARER_CREDENTIAL_ID: &str = "server:bearer";
const LAN_PHRASE: &str = "expose to LAN";
const LOCALHOST_ADDR: &str = "127.0.0.1";
const LAN_ADDR: &str = "0.0.0.0";
const MIN_PORT: u16 = 1024;
const DEFAULT_PORT: u16 = 8081;
const DEFAULT_OVERLAY_HINT: &str = "~/.local/share/forge/overlays";

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
    require_http_overlay_token: bool,
    cors_any_origin: bool,
    overlay_root: Option<String>,
}

pub struct SettingsWebSocketView {
    backend: Arc<dyn DataProvider>,
    rt_handle: tokio::runtime::Handle,
    server: Option<ServerHandle>,

    enable_server: bool,
    bind_choice: BindChoice,
    port: u16,
    require_ws_token: bool,
    require_http_overlay_token: bool,
    cors_any_origin: bool,
    overlay_root: Option<String>,

    bearer_token: String,
    token_revealed: bool,

    loading: bool,
    save_state: SaveState,

    port_input: Entity<TextInput>,
    lan_modal: Option<Entity<TypeToConfirm>>,
    lan_sub: Option<Subscription>,
    _subs: Vec<Subscription>,
}

impl SettingsWebSocketView {
    pub fn new(
        backend: Arc<dyn DataProvider>,
        rt_handle: tokio::runtime::Handle,
        server: Option<ServerHandle>,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let port_input = cx.new(|cx| {
            TextInput::new("8081", cx)
                .with_palette(palette)
                .with_font_size(FONT_SM)
        });

        let mut subs = Vec::new();
        subs.push(
            cx.subscribe(&port_input, |this, _input, event: &InputEvent, cx| {
                if let InputEvent::Submitted(_) = event {
                    this.commit_port(cx);
                }
            }),
        );

        let mut view = Self {
            backend,
            rt_handle,
            server,
            enable_server: true,
            bind_choice: BindChoice::Localhost,
            port: DEFAULT_PORT,
            require_ws_token: true,
            require_http_overlay_token: false,
            cors_any_origin: true,
            overlay_root: None,
            bearer_token: String::new(),
            token_revealed: false,
            loading: false,
            save_state: SaveState::default(),
            port_input,
            lan_modal: None,
            lan_sub: None,
            _subs: subs,
        };
        view.load(cx);
        view.fetch_token(cx);
        view
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
                self.require_http_overlay_token = snap.require_http_overlay_token;
                self.cors_any_origin = snap.cors_any_origin;
                self.overlay_root = snap.overlay_root.filter(|s| !s.is_empty());
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

    fn apply_persist(
        &mut self,
        fut: impl Future<Output = Result<(), String>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        self.save_state = SaveState::Saving;
        async_bridge::run_async(
            &self.rt_handle,
            fut,
            |this, result, cx| this.apply_save_result(result, cx),
            cx,
        );
        cx.notify();
    }

    fn apply_save_result(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        match result {
            Ok(()) => self.save_state = SaveState::Saved,
            Err(message) => {
                tracing::warn!(error = %message, "failed to save websocket settings");
                self.save_state = SaveState::Error(message.into());
            }
        }
        cx.notify();
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
                        handle.restart().await.map_err(|e| e.to_string())?;
                    } else {
                        handle.stop().await.map_err(|e| e.to_string())?;
                    }
                }
                Ok(())
            },
            cx,
        );
    }

    fn select_localhost(&mut self, cx: &mut Context<Self>) {
        self.bind_choice = BindChoice::Localhost;
        self.close_lan_modal();
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        self.apply_persist(
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
        self.apply_persist(
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
                self.apply_persist(
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

    fn toggle_require_http_token(&mut self, cx: &mut Context<Self>) {
        let prev = self.require_http_overlay_token;
        let value = !prev;
        self.require_http_overlay_token = value;
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        self.persist_bool(
            prev,
            |this, v| this.require_http_overlay_token = v,
            async move {
                ServerSettings::save_http_overlay_require_token(repo.as_ref(), value)
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
        self.persist_bool(
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
        self.apply_persist(
            async move {
                ServerSettings::save_overlay_root(repo.as_ref(), &path_str)
                    .await
                    .map_err(|e| e.to_string())
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

    fn header_row(&self, palette: &ForgePalette, density: Density) -> impl IntoElement {
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(icon(Icon::Server, px(20.0), palette.brand))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_LG)
                    .text_color(palette.text_primary)
                    .child(tr!("settings_ws_title")),
            )
            .child(div().flex_1())
            .child(save_indicator(&self.save_state, palette))
    }

    fn bind_section(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut section = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(field_title(tr!("settings_ws_bind_section_title"), palette))
            .child(field_hint(
                tr!("settings_ws_bind_section_subtitle"),
                palette,
            ))
            .child(self.bind_card(BindChoice::Localhost, palette, density, cx))
            .child(self.bind_card(BindChoice::Lan, palette, density, cx));

        if self.bind_choice == BindChoice::Lan {
            section = section.child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.warning)
                    .child(tr!("settings_ws_bind_lan_restart_warning")),
            );
        }
        section
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
                    .child(" Authorization: Bearer …"),
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
                    .font_family(mono_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(shown),
            )
            .child(
                div()
                    .flex()
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
                "settings-ws-auth-http",
                Icon::Globe,
                palette.info,
                tr!("settings_ws_auth_require_http_label"),
                tr!("settings_ws_auth_require_http_sublabel"),
                self.require_http_overlay_token,
                palette,
                density,
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_require_http_token(cx)),
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
            .items_start()
            .gap(spacing(Spacing::Md, density))
            .child(weighted(1.0, self.port_column(&palette, density)))
            .child(weighted(1.6, self.token_column(&palette, density, cx)));

        let mut root = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .child(self.header_row(&palette, density))
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
                toggle(self.enable_server, &palette).on_click(
                    "settings-ws-enable",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_enable(cx)),
                ),
                &palette,
                density,
            ))
            .child(hline(palette.border_regular))
            .child(self.bind_section(&palette, density, cx))
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
        require_http_overlay_token: snap.http_overlay_require_token,
        cors_any_origin: snap.overlay_cors_any_origin,
        overlay_root: snap.overlay_root,
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
