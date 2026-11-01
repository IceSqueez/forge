use super::*;
use std::sync::Arc;

use forge_components::{
    ActivePresentation, ConfirmTone, Density, FONT_XS, ForgePalette, Icon, KeyActivated,
    OverlayPosition, Spacing, confirm_modal, field_title, ghost_button_with_icon, icon, overlay,
    readonly_mono_field, setting_block, setting_row, setting_rows, spacing, toggle, tr,
};
use forge_server::ServerSettings;
use forge_storage::SettingsRepo;
use forge_types::Secret;
use gpui::{AnyElement, ClickEvent, Context, Pixels, div, px};

use crate::app::async_bridge::{self, ErrorSink};
use crate::shared::repo_failure::RepoFailure;
use crate::tools::overlay_link_token;
use crate::tools::server_bearer;

const TOKEN_ACTION_GLYPH: Pixels = px(12.0);

impl SettingsWebSocketView {
    pub(super) fn toggle_require_ws_token(&mut self, cx: &mut Context<Self>) {
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

    pub(super) fn toggle_require_overlay_token(&mut self, cx: &mut Context<Self>) {
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
                    handle.set_http_overlay_token_required(value);
                }
                Ok(())
            },
            cx,
        );
    }

    fn copy_overlay_token(&mut self, cx: &mut Context<Self>) {
        if let Some(token) = &self.overlay_token {
            crate::app::clipboard::copy_to_clipboard(token.expose().to_owned(), cx);
        }
    }

    fn overlay_token_regenerate_disabled(&self) -> bool {
        self.overlay_token_regenerating || self.server.is_none()
    }

    pub(super) fn request_overlay_token_regenerate(&mut self, cx: &mut Context<Self>) {
        if self.overlay_token_regenerate_disabled() {
            return;
        }
        self.overlay_token_confirm_open = true;
        cx.notify();
    }

    pub(super) fn cancel_overlay_token_regenerate(&mut self, cx: &mut Context<Self>) {
        self.overlay_token_confirm_open = false;
        cx.notify();
    }

    pub(super) fn confirm_overlay_token_regenerate(&mut self, cx: &mut Context<Self>) {
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

    fn toggle_token_reveal(&mut self, cx: &mut Context<Self>) {
        self.token_revealed = !self.token_revealed;
        cx.notify();
    }

    fn copy_token(&mut self, cx: &mut Context<Self>) {
        crate::app::clipboard::copy_to_clipboard(self.bearer_token.expose().to_owned(), cx);
    }

    fn request_regenerate(&mut self, cx: &mut Context<Self>) {
        self.regenerate_pending = true;
        cx.notify();
    }

    pub(super) fn cancel_regenerate(&mut self, cx: &mut Context<Self>) {
        self.regenerate_pending = false;
        cx.notify();
    }

    pub(super) fn confirm_regenerate(&mut self, cx: &mut Context<Self>) {
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

    pub(super) fn token_column(
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
                    .font_family(cx.body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(tr!("settings_ws_token_clients_send")),
            )
            .child(
                div()
                    .font_family(cx.mono_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(" Authorization: Bearer ..."),
            );

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(field_title(
                tr!("settings_ws_token_section_title"),
                palette,
                cx,
            ))
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

        let field = readonly_mono_field(shown, palette, density, cx).child(
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
                        .key_activated(palette.text_primary)
                        .on_click(
                            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_token_reveal(cx)),
                        )
                        .child(icon(reveal_glyph, TOKEN_ACTION_GLYPH, palette.text_faint)),
                )
                .child(
                    div()
                        .id("settings-ws-token-copy")
                        .flex()
                        .cursor_pointer()
                        .key_activated(palette.text_primary)
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.copy_token(cx)))
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

    pub(super) fn auth_section(
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
            cx,
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

        let mut field = readonly_mono_field(shown, palette, density, cx);
        if self.overlay_token.is_some() {
            field = field.child(
                div()
                    .id("settings-ws-overlay-token-copy")
                    .flex()
                    .flex_none()
                    .cursor_pointer()
                    .key_activated(palette.text_primary)
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
            .child(field_title(
                tr!("settings_ws_overlay_token_title"),
                palette,
                cx,
            ))
            .child(
                div()
                    .font_family(cx.body_family())
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

    pub(super) fn overlay_token_confirm(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let confirm = confirm_modal(
            tr!("settings_ws_overlay_token_regen_title"),
            tr!("settings_ws_overlay_token_regen_message"),
            ConfirmTone::Warning,
            palette,
        )
        .esc_hint(tr!("widget_confirm_esc_to_cancel"))
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
}
