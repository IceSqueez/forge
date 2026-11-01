use super::ServerConsoleView;
use crate::app::async_bridge::{self, ErrorSink};
use crate::app::navigation::{NavRequested, Screen, SettingsSection};
use crate::shared::repo_failure::RepoFailure;
use crate::tools::server_restart::restart_ignoring_disabled;
use forge_components::{
    ActivePresentation, Density, FONT_XXS, ForgePalette, Icon, KeyActivated, Spacing,
    ghost_button_with_icon, spacing, tr,
};
use gpui::{AnyElement, ClickEvent, Context, div, prelude::*};

impl ServerConsoleView {
    fn restart_server(&mut self, cx: &mut Context<Self>) {
        let Some(handle) = self.services.server.clone() else {
            return;
        };
        if self.lifecycle.restarting {
            return;
        }
        self.lifecycle.restarting = true;
        async_bridge::run_async(
            &self.services.rt_handle,
            async move { restart_ignoring_disabled(&handle).await },
            |this, result: Result<(), RepoFailure>, cx| {
                this.lifecycle.restarting = false;
                if let Err(message) = result {
                    tracing::warn!(error = %message, "failed to restart websocket server");
                    ErrorSink::Toast.report(message.text(), cx);
                }
                cx.notify();
            },
            cx,
        );
        cx.notify();
    }

    pub(super) fn restart_disabled(&self) -> bool {
        self.lifecycle.restarting || !self.lifecycle.running
    }

    pub(super) fn restart_control(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = if self.lifecycle.restarting {
            tr!("server_btn_restarting")
        } else {
            tr!("server_btn_restart")
        };
        ghost_button_with_icon(Icon::Refresh, label, palette)
            .disabled(self.restart_disabled())
            .busy(self.lifecycle.restarting)
            .on_click(
                "srv-restart",
                cx.listener(|this, _: &ClickEvent, _, cx| this.restart_server(cx)),
            )
            .into_any_element()
    }

    fn open_websocket_settings(&mut self, cx: &mut Context<Self>) {
        cx.emit(NavRequested(Screen::Settings(Some(
            SettingsSection::WebSocket,
        ))));
    }

    fn switched_off_hint_visible(&self) -> bool {
        !self.lifecycle.enabled
    }

    pub(super) fn disabled_hint(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.switched_off_hint_visible() {
            return None;
        }
        Some(
            div()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xxs, density))
                .child(
                    div()
                        .font_family(cx.body_family())
                        .text_size(FONT_XXS)
                        .text_color(palette.text_faint)
                        .child(tr!("server_console_disabled_hint")),
                )
                .child(
                    div()
                        .id("srv-open-ws-settings")
                        .cursor_pointer()
                        .font_family(cx.body_family())
                        .text_size(FONT_XXS)
                        .text_color(palette.info)
                        .child(tr!("server_console_disabled_hint_link"))
                        .key_activated(palette.text_primary)
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.open_websocket_settings(cx)
                        })),
                )
                .into_any_element(),
        )
    }

    pub(super) fn is_running(&self) -> bool {
        self.lifecycle.running
    }

    pub(super) fn start_run_state_bridge(&self, cx: &mut Context<Self>) {
        let Some(handle) = self.services.server.as_ref() else {
            return;
        };
        let mut run_state = handle.run_state();
        cx.spawn(async move |this, cx| {
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
        })
        .detach();
    }

    fn apply_run_state(&mut self, running: bool, cx: &mut Context<Self>) {
        if self.lifecycle.running == running {
            return;
        }
        self.lifecycle.running = running;
        if !running {
            self.clients.connected.clear();
            self.clients.recent = 0;
            self.clients.pending_disconnect.cancel();
            self.traffic.throughput_samples.clear();
        }
        cx.notify();
    }
}
