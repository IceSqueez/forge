use super::IntegrationDetail;
use crate::async_bridge;
use crate::integration_extensions::{DetailExtension, has_extension};
use crate::navigation::{NavRequested, Screen};
use crate::obs_credentials_form::ObsConnected;
use crate::obs_settings_modal::{ObsSettingsModal, ObsSettingsModalEvent};
use forge_components::focus_inside_modal;
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
            HeaderAction::Settings
                if has_extension(self.ports.status.id(), DetailExtension::ObsSettingsModal) =>
            {
                self.open_obs_settings(window, cx)
            }
            HeaderAction::Settings if self.sign_in.donation_settings.is_some() => {
                if let Some(link) = &self.sign_in.donation_settings {
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
        if self.live.signing_out {
            return;
        }
        self.modals.pending_disconnect.request(());
        cx.notify();
    }

    fn open_obs_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rt_handle = self.services.rt_handle.clone();
        let credentials = Arc::clone(&self.services.credentials);
        let settings = Arc::clone(&self.services.settings);
        let bus = Arc::clone(&self.services.bus);
        let seed = self.services.obs_install_seed.clone();
        let modal =
            cx.new(|cx| ObsSettingsModal::new(rt_handle, credentials, settings, bus, seed, cx));
        focus_inside_modal(window, cx, |window, cx| {
            modal.update(cx, |m, cx| m.focus(window, cx))
        });
        self.modals._obs_modal_sub = Some(cx.subscribe(&modal, Self::on_obs_settings_event));
        self.modals.obs_settings_modal = Some(modal);
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
        self.modals.obs_settings_modal = None;
        self.modals._obs_modal_sub = None;
        cx.notify();
    }

    fn dispatch_control(&mut self, verb: ControlVerb, cx: &mut Context<Self>) {
        let Some(ctrl) = self.ports.control.clone() else {
            return;
        };
        let refreshing = matches!(verb, ControlVerb::RefreshToken);
        async_bridge::run_async(
            &self.services.rt_handle,
            async move {
                match verb {
                    ControlVerb::Reconnect => ctrl.reconnect().await,
                    ControlVerb::Disconnect => ctrl.disconnect().await,
                    ControlVerb::RefreshToken => ctrl.refresh_token().await,
                }
            },
            move |this, result, cx| match result {
                Ok(()) => this.reload(cx),
                Err(ControlFailure::Unauthorized) if refreshing => this.reload(cx),
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
        self.modals.pending_disconnect.cancel();
        cx.notify();
    }

    pub(super) fn confirm_disconnect(&mut self, cx: &mut Context<Self>) {
        if self.modals.pending_disconnect.take().is_some() {
            if self
                .services
                .sign_out
                .entry(self.ports.status.id())
                .is_some()
            {
                self.sign_out(cx);
            } else {
                self.dispatch_control(ControlVerb::Disconnect, cx);
            }
        }
        cx.notify();
    }
}
