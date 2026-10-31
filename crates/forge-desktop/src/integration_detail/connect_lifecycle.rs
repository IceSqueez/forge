use super::quick_actions::reachable_quick_actions;
use super::state::BuiltinPorts;
use super::{ActiveConnect, IntegrationDetail, ObsSignedOut, VTubeSignedOut};
use crate::async_bridge;
use crate::connect_flow::{ConnectFlow, ConnectFlowEvent, ConnectFlowLaunch};
use crate::integration_sign_out::{SignOutOutcome, forget_then_disconnect};
use crate::navigation::Screen;
use forge_components::{PushToast, ToastKind, tr};
use forge_host::integrations::BuiltinObject;
use forge_platform_core::{ConnectionState, PlatformError};
use forge_runtime::IntegrationSlot;
use forge_types::PlatformId;
use gpui::{Context, Entity, prelude::*};
use std::future::Future;
use std::sync::Arc;

impl IntegrationDetail {
    pub(super) fn open_connect_flow(&mut self, platform: PlatformId, cx: &mut Context<Self>) {
        let launch = ConnectFlowLaunch {
            platform,
            display_name: self.shown.display_name.clone(),
            rt_handle: self.services.rt_handle.clone(),
            credentials: Arc::clone(&self.services.credentials),
            twitch_slot: self.services.twitch_slot.clone(),
            kick_slot: self.services.kick_slot.clone(),
            youtube_slot: self.services.youtube_slot.clone(),
            twitch_sign_in: self.sign_in.twitch_sign_in.clone(),
            youtube_sign_in: self.sign_in.youtube_sign_in.clone(),
            kick_sign_in: self.sign_in.kick_sign_in.clone(),
        };
        let view = cx.new(|cx| ConnectFlow::new(launch, cx));
        let subs = vec![
            cx.subscribe(&view, Self::on_connect_flow_event),
            cx.observe(&view, |_, _, cx| cx.notify()),
        ];
        self.sign_in.connect = Some(ActiveConnect { view, _subs: subs });
        cx.notify();
    }

    fn on_connect_flow_event(
        &mut self,
        _view: Entity<ConnectFlow>,
        event: &ConnectFlowEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            ConnectFlowEvent::Connected(object) => self.adopt_connected(object, cx),
            ConnectFlowEvent::Leave => self.navigate_to(Screen::Integrations(None), cx),
        }
    }

    fn adopt_connected(&mut self, object: &BuiltinObject, cx: &mut Context<Self>) {
        self.adopt_builtin(object.clone(), cx);
    }

    pub(super) fn reload(&mut self, cx: &mut Context<Self>) {
        self.shown.display_name = self.ports.status.display_name().to_owned();
        self.shown.version = self.ports.status.version();
        self.shown.connection = self.ports.status.connection();
        self.shown.capability_flags = self.ports.status.capability_flags();
        self.shown.header_actions = self.ports.status.header_actions();
        self.shown.health_metrics = self.ports.health.metrics();
        self.shown.sections = self.ports.content.sections();
        self.quick_actions =
            reachable_quick_actions(&*self.ports.quick, self.ports.collections.as_deref());
        if let Some(manager) = self.modals.collection_manager.clone() {
            let connected = self.shown.connection == ConnectionState::Connected;
            manager.update(cx, |manager, cx| manager.set_connected(connected, cx));
        }
        if let Some(section) = self.sign_in.twitch_bot.clone() {
            let streamer_login = self.ports.status.hero_name();
            section.update(cx, |section, cx| {
                section.set_streamer_login(streamer_login, cx)
            });
        }
        self.sync_token_countdown(cx);
        cx.notify();
    }

    pub(super) fn adopt_builtin(&mut self, object: BuiltinObject, cx: &mut Context<Self>) {
        self.ports = BuiltinPorts::from_object(object);
        self.sign_in.connect = None;
        self.close_collection_manager();
        self.live.viewer_samples.clear();
        self.background._health_bridge = Self::spawn_health_bridge(&self.ports.health, cx);
        self.background._change_bridge = Self::spawn_change_bridge(&self.ports, cx);
        self.reload(cx);
        self.sample_viewers();
    }

    pub(super) fn reset_to_connect(&mut self, platform: PlatformId, cx: &mut Context<Self>) {
        let credentials = Arc::clone(&self.services.credentials);
        let managers = self.sign_in.platform_credentials.clone();
        let slot = self.slot_of(platform);
        let forget = async move {
            let outcome = forget_then_disconnect(
                async move { managers.forget(platform, credentials.as_ref()).await },
                None,
            )
            .await?;
            if let Some(slot) = slot {
                slot.rebuild().await;
            }
            Ok::<SignOutOutcome, PlatformError>(outcome)
        };
        self.run_sign_out(
            forget,
            move |this, cx| {
                this.ports.control = None;
                this.close_collection_manager();
                this.shown.twitch_reauth_required = false;
                this.live.viewer_samples.clear();
                this.open_connect_flow(platform, cx);
            },
            cx,
        );
    }

    fn run_sign_out<F, E>(
        &mut self,
        forget: F,
        apply_signed_out: impl FnOnce(&mut Self, &mut Context<Self>) + Send + 'static,
        cx: &mut Context<Self>,
    ) where
        F: Future<Output = Result<SignOutOutcome, E>> + Send + 'static,
        E: std::fmt::Display + Send + 'static,
    {
        if self.live.signing_out {
            return;
        }
        self.live.signing_out = true;
        cx.notify();
        async_bridge::run_async(
            &self.services.rt_handle,
            forget,
            move |this, result, cx| {
                this.live.signing_out = false;
                match result {
                    Ok(outcome) => {
                        if outcome == SignOutOutcome::StillConnected {
                            cx.push_toast(
                                ToastKind::Warn,
                                tr!(
                                    "integration_sign_out_still_connected",
                                    name = this.shown.display_name.clone()
                                ),
                            );
                        }
                        apply_signed_out(this, cx);
                    }
                    Err(err) => {
                        tracing::warn!(
                            error = %err,
                            integration = %this.ports.status.id(),
                            "sign-out could not forget stored credentials"
                        );
                        cx.push_toast(
                            ToastKind::Error,
                            tr!(
                                "integration_sign_out_failed",
                                name = this.shown.display_name.clone()
                            ),
                        );
                    }
                }
                cx.notify();
            },
            cx,
        );
    }

    fn slot_of(&self, platform: PlatformId) -> Option<IntegrationSlot<BuiltinObject>> {
        match platform {
            PlatformId::Twitch => self.services.twitch_slot.clone(),
            PlatformId::YouTube => self.services.youtube_slot.clone(),
            PlatformId::Kick => self.services.kick_slot.clone(),
        }
    }

    pub(super) fn sign_out_obs(&mut self, cx: &mut Context<Self>) {
        let credentials = Arc::clone(&self.services.credentials);
        let forget = forget_then_disconnect(
            async move { forge_obs::credentials::clear(&*credentials).await },
            self.ports.control.clone(),
        );
        self.run_sign_out(
            forget,
            |this, cx| {
                this.ports.control = None;
                this.services.obs_install_seed.clear();
                cx.emit(ObsSignedOut);
            },
            cx,
        );
    }

    pub(super) fn sign_out_vtube(&mut self, cx: &mut Context<Self>) {
        let credentials = Arc::clone(&self.services.credentials);
        let forget = forget_then_disconnect(
            async move { forge_vtube::credentials::clear(&*credentials).await },
            self.ports.control.clone(),
        );
        self.run_sign_out(
            forget,
            |this, cx| {
                this.ports.control = None;
                this.services.vtube_install_seed.clear();
                cx.emit(VTubeSignedOut);
            },
            cx,
        );
    }

    pub(super) fn resync_content(&mut self, cx: &mut Context<Self>) {
        self.ports.content.request_refresh();
        self.reload(cx);
    }
}
