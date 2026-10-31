use super::quick_actions::reachable_quick_actions;
use super::{ActiveConnect, IntegrationDetail, ObsSignedOut, VTubeSignedOut};
use crate::async_bridge;
use crate::connect_flow::{ConnectFlow, ConnectFlowEvent, ConnectFlowLaunch};
use crate::integration_sign_out::{SignOutOutcome, forget_then_disconnect};
use crate::integrations::BuiltinObject;
use crate::navigation::Screen;
use forge_components::{PushToast, ToastKind, tr};
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
            display_name: self.display_name.clone(),
            rt_handle: self.rt_handle.clone(),
            credentials: Arc::clone(&self.credentials),
            twitch_slot: self.twitch_slot.clone(),
            kick_slot: self.kick_slot.clone(),
            youtube_slot: self.youtube_slot.clone(),
            twitch_sign_in: self.twitch_sign_in.clone(),
            youtube_sign_in: self.youtube_sign_in.clone(),
            kick_sign_in: self.kick_sign_in.clone(),
        };
        let view = cx.new(|cx| ConnectFlow::new(launch, cx));
        let subs = vec![
            cx.subscribe(&view, Self::on_connect_flow_event),
            cx.observe(&view, |_, _, cx| cx.notify()),
        ];
        self.connect = Some(ActiveConnect { view, _subs: subs });
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
        self.display_name = self.status.display_name().to_owned();
        self.version = self.status.version();
        self.connection = self.status.connection();
        self.capability_flags = self.status.capability_flags();
        self.header_actions = self.status.header_actions();
        self.health_metrics = self.health.metrics();
        self.sections = self.content.sections();
        self.quick_actions = reachable_quick_actions(&*self.quick, self.collections.as_deref());
        if let Some(manager) = self.collection_manager.clone() {
            let connected = self.connection == ConnectionState::Connected;
            manager.update(cx, |manager, cx| manager.set_connected(connected, cx));
        }
        if let Some(section) = self.twitch_bot.clone() {
            let streamer_login = self.status.hero_name();
            section.update(cx, |section, cx| {
                section.set_streamer_login(streamer_login, cx)
            });
        }
        cx.notify();
    }

    pub(super) fn adopt_builtin(&mut self, object: BuiltinObject, cx: &mut Context<Self>) {
        self.icon = object.icon;
        self.status = object.status;
        self.health = object.health;
        self.content = object.content;
        self.quick = object.quick;
        self.control = object.control;
        self.collections = object.collections;
        self.connect = None;
        self.close_collection_manager();
        self.eventsub_tally.clear();
        self.viewer_samples.clear();
        self._health_bridge = Self::spawn_health_bridge(&self.health, cx);
        self._quick_action_bridge = Self::spawn_quick_action_bridge(&*self.quick, cx);
        self.reload(cx);
    }

    pub(super) fn reset_to_connect(&mut self, platform: PlatformId, cx: &mut Context<Self>) {
        let credentials = Arc::clone(&self.credentials);
        let managers = self.platform_credentials.clone();
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
                this.control = None;
                this.close_collection_manager();
                this.twitch_reauth_required = false;
                this.eventsub_tally.clear();
                this.viewer_samples.clear();
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
        if self.signing_out {
            return;
        }
        self.signing_out = true;
        cx.notify();
        async_bridge::run_async(
            &self.rt_handle,
            forget,
            move |this, result, cx| {
                this.signing_out = false;
                match result {
                    Ok(outcome) => {
                        if outcome == SignOutOutcome::StillConnected {
                            cx.push_toast(
                                ToastKind::Warn,
                                tr!(
                                    "integration_sign_out_still_connected",
                                    name = this.display_name.clone()
                                ),
                            );
                        }
                        apply_signed_out(this, cx);
                    }
                    Err(err) => {
                        tracing::warn!(
                            error = %err,
                            integration = %this.status.id(),
                            "sign-out could not forget stored credentials"
                        );
                        cx.push_toast(
                            ToastKind::Error,
                            tr!(
                                "integration_sign_out_failed",
                                name = this.display_name.clone()
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
            PlatformId::Twitch => self.twitch_slot.clone(),
            PlatformId::YouTube => self.youtube_slot.clone(),
            PlatformId::Kick => self.kick_slot.clone(),
        }
    }

    pub(super) fn sign_out_obs(&mut self, cx: &mut Context<Self>) {
        let credentials = Arc::clone(&self.credentials);
        let forget = forget_then_disconnect(
            async move { forge_obs::credentials::clear(&*credentials).await },
            self.control.clone(),
        );
        self.run_sign_out(
            forget,
            |this, cx| {
                this.control = None;
                this.obs_install_seed.clear();
                this.obs_source = None;
                cx.emit(ObsSignedOut);
            },
            cx,
        );
    }

    pub(super) fn sign_out_vtube(&mut self, cx: &mut Context<Self>) {
        let credentials = Arc::clone(&self.credentials);
        let forget = forget_then_disconnect(
            async move { forge_vtube::credentials::clear(&*credentials).await },
            self.control.clone(),
        );
        self.run_sign_out(
            forget,
            |this, cx| {
                this.control = None;
                this.vtube_install_seed.clear();
                cx.emit(VTubeSignedOut);
            },
            cx,
        );
    }

    pub(super) fn resync_content(&mut self, cx: &mut Context<Self>) {
        if let Some(obs) = &self.obs_source {
            obs.request_catalog_resync();
        }
        self.reload(cx);
    }
}
