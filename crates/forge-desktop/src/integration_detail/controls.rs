use super::{IntegrationDetail, platform_of};
use crate::async_bridge;
use crate::navigation::{NavRequested, Screen};
use crate::obs_credentials_form::ObsConnected;
use crate::obs_settings_modal::{ObsSettingsModal, ObsSettingsModalEvent};
use forge_components::{PushToast, ToastKind, tr};
use forge_platform_core::{ControlFailure, HeaderAction};
use gpui::{Context, Entity, Window, prelude::*};
use std::sync::Arc;

enum ControlVerb {
    Reconnect,
    Disconnect,
    RefreshToken,
}

impl IntegrationDetail {
    pub(crate) fn navigate_to(&mut self, screen: Screen, cx: &mut Context<Self>) {
        cx.emit(NavRequested(screen));
    }

    pub(super) fn on_header_action(
        &mut self,
        action: HeaderAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            HeaderAction::Disconnect => self.request_disconnect(cx),
            HeaderAction::Reconnect => self.dispatch_control(ControlVerb::Reconnect, cx),
            HeaderAction::RefreshToken => self.dispatch_control(ControlVerb::RefreshToken, cx),
            HeaderAction::Settings if self.is_obs => self.open_obs_settings(window, cx),
            HeaderAction::Settings if self.donation_settings.is_some() => {
                if let Some(link) = &self.donation_settings {
                    link.view
                        .update(cx, |settings, cx| settings.focus_token(window, cx));
                }
            }
            HeaderAction::Settings => {
                cx.push_toast(ToastKind::Info, tr!("integration_settings_coming_soon"));
            }
        }
    }

    pub(super) fn request_disconnect(&mut self, cx: &mut Context<Self>) {
        if self.signing_out {
            return;
        }
        self.pending_disconnect.request(());
        cx.notify();
    }

    fn open_obs_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rt_handle = self.rt_handle.clone();
        let credentials = Arc::clone(&self.credentials);
        let settings = Arc::clone(&self.settings);
        let bus = Arc::clone(&self.bus);
        let seed = self.obs_install_seed.clone();
        let modal =
            cx.new(|cx| ObsSettingsModal::new(rt_handle, credentials, settings, bus, seed, cx));
        modal.update(cx, |m, cx| m.focus(window, cx));
        self._obs_modal_sub = Some(cx.subscribe(&modal, Self::on_obs_settings_event));
        self.obs_settings_modal = Some(modal);
        cx.notify();
    }

    fn on_obs_settings_event(
        &mut self,
        _modal: Entity<ObsSettingsModal>,
        event: &ObsSettingsModalEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            ObsSettingsModalEvent::Close => self.close_obs_settings(cx),
            ObsSettingsModalEvent::Saved => {
                self.close_obs_settings(cx);
                cx.emit(ObsConnected);
            }
            ObsSettingsModalEvent::Disconnect => {
                self.close_obs_settings(cx);
                self.request_disconnect(cx);
            }
        }
    }

    fn close_obs_settings(&mut self, cx: &mut Context<Self>) {
        self.obs_settings_modal = None;
        self._obs_modal_sub = None;
        cx.notify();
    }

    fn dispatch_control(&mut self, verb: ControlVerb, cx: &mut Context<Self>) {
        let Some(ctrl) = self.control.clone() else {
            return;
        };
        if self.is_twitch && matches!(verb, ControlVerb::RefreshToken) {
            let task = self
                .rt_handle
                .spawn(async move { ctrl.refresh_token().await });
            cx.spawn(async move |this, cx| {
                let Ok(result) = task.await else {
                    return;
                };
                this.update(cx, |this, cx| match result {
                    Ok(()) => {
                        this.twitch_reauth_required = false;
                        cx.notify();
                    }
                    Err(ControlFailure::Unauthorized) => {
                        this.twitch_reauth_required = true;
                        cx.notify();
                    }
                    Err(err) => {
                        tracing::warn!(error = %err, "twitch token refresh failed");
                        cx.push_toast(ToastKind::Error, tr!("integration_control_failed"));
                    }
                })
                .ok();
            })
            .detach();
            return;
        }
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                match verb {
                    ControlVerb::Reconnect => ctrl.reconnect().await,
                    ControlVerb::Disconnect => ctrl.disconnect().await,
                    ControlVerb::RefreshToken => ctrl.refresh_token().await,
                }
            },
            move |this, result, cx| match result {
                Ok(()) => this.reload(cx),
                Err(err) => {
                    tracing::warn!(error = %err, "integration control failed");
                    cx.push_toast(ToastKind::Error, tr!("integration_control_failed"));
                    this.reload(cx);
                }
            },
            cx,
        );
    }

    pub(super) fn cancel_disconnect(&mut self, cx: &mut Context<Self>) {
        self.pending_disconnect.cancel();
        cx.notify();
    }

    pub(super) fn confirm_disconnect(&mut self, cx: &mut Context<Self>) {
        if self.pending_disconnect.take().is_some() {
            if self.is_obs {
                self.sign_out_obs(cx);
            } else if self.is_vtube {
                self.sign_out_vtube(cx);
            } else if let Some(platform) = platform_of(self.status.id().as_str()) {
                self.reset_to_connect(platform, cx);
            } else {
                self.dispatch_control(ControlVerb::Disconnect, cx);
            }
        }
        cx.notify();
    }
}
