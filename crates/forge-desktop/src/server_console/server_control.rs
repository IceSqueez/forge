use super::ServerConsoleView;
use crate::async_bridge::{self, ErrorSink};
use crate::navigation::{NavRequested, Screen, SettingsSection};
use crate::repo_failure::RepoFailure;
use crate::server_restart::restart_ignoring_disabled;
use forge_components::{
    Density, FONT_XXS, ForgePalette, Icon, Spacing, body_family, ghost_button_with_icon, spacing,
    tr,
};
use gpui::{AnyElement, ClickEvent, Context, div, prelude::*};

impl ServerConsoleView {
    fn restart_server(&mut self, cx: &mut Context<Self>) {
        let Some(handle) = self.server.clone() else {
            return;
        };
        if self.restarting {
            return;
        }
        self.restarting = true;
        async_bridge::run_async(
            &self.rt_handle,
            async move { restart_ignoring_disabled(&handle).await },
            |this, result: Result<(), RepoFailure>, cx| {
                this.restarting = false;
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
        self.restarting || !self.running
    }

    pub(super) fn restart_control(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = if self.restarting {
            tr!("server_btn_restarting")
        } else {
            tr!("server_btn_restart")
        };
        ghost_button_with_icon(Icon::Refresh, label, palette)
            .disabled(self.restart_disabled())
            .busy(self.restarting)
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
        !self.enabled
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
                        .font_family(body_family())
                        .text_size(FONT_XXS)
                        .text_color(palette.text_faint)
                        .child(tr!("server_console_disabled_hint")),
                )
                .child(
                    div()
                        .id("srv-open-ws-settings")
                        .cursor_pointer()
                        .font_family(body_family())
                        .text_size(FONT_XXS)
                        .text_color(palette.info)
                        .child(tr!("server_console_disabled_hint_link"))
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.open_websocket_settings(cx)
                        })),
                )
                .into_any_element(),
        )
    }

    pub(super) fn is_running(&self) -> bool {
        self.running
    }

    pub(super) fn start_run_state_bridge(&self, cx: &mut Context<Self>) {
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
        if !running {
            self.connected_clients.clear();
            self.recent_clients = 0;
            self.pending_disconnect.cancel();
            self.throughput_samples.clear();
        }
        cx.notify();
    }
}
