use super::state::BuiltinPorts;
use super::{ActiveConnect, IntegrationDetail, IntegrationSignedOut};
use crate::app::async_bridge;
use crate::app::navigation::Screen;
use crate::integrations::connect_flow::{
    ConnectFlow, ConnectFlowEvent, ConnectFlowLaunch, ConnectSignIn,
};
use crate::integrations::integration_catalog::chat_platform_of;
use forge_components::{PushToast, ToastKind, tr};
use forge_host::integration_sign_out::SignOutOutcome;
use forge_host::integrations::BuiltinObject;
use forge_platform_core::{ConnectionState, LoopbackSignIn};
use forge_types::PlatformId;
use gpui::{Context, Entity, prelude::*};
use std::sync::Arc;

impl IntegrationDetail {
    pub(super) fn open_connect_flow(&mut self, platform: PlatformId, cx: &mut Context<Self>) {
        let launch = ConnectFlowLaunch {
            platform,
            display_name: self.shown.display_name.clone(),
            rt_handle: self.services.rt_handle.clone(),
            credentials: Arc::clone(&self.services.credentials),
            slot: match platform {
                PlatformId::Twitch => self.services.twitch_slot.clone(),
                PlatformId::YouTube => self.services.youtube_slot.clone(),
                PlatformId::Kick => self.services.kick_slot.clone(),
            },
            sign_in: match platform {
                PlatformId::Twitch => {
                    ConnectSignIn::DeviceCode(self.services.twitch_sign_in.clone())
                }
                PlatformId::YouTube => ConnectSignIn::LocalCallback(
                    self.services
                        .youtube_sign_in
                        .clone()
                        .map(|sign_in| Arc::new(sign_in) as Arc<dyn LoopbackSignIn>),
                ),
                PlatformId::Kick => ConnectSignIn::LocalCallback(
                    self.services
                        .kick_sign_in
                        .clone()
                        .map(|sign_in| Arc::new(sign_in) as Arc<dyn LoopbackSignIn>),
                ),
            },
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
            ConnectFlowEvent::Connected(object) => self.adopt_builtin(object.clone(), cx),
            ConnectFlowEvent::Leave => self.navigate_to(Screen::Integrations(None), cx),
        }
    }

    pub(super) fn reload(&mut self, cx: &mut Context<Self>) {
        self.shown.credential = self.ports.status.credential_state();
        self.shown.display_name = self.ports.status.display_name().to_owned();
        self.shown.version = self.ports.status.version();
        self.shown.connection = self.ports.status.connection();
        self.shown.capability_flags = self.ports.status.capability_flags();
        self.shown.header_actions = self.ports.status.header_actions();
        self.shown.health_metrics = self.ports.health.metrics();
        self.shown.sections = self.ports.content.sections();
        let display_name = self.shown.display_name.clone();
        let connected = self.shown.connection == ConnectionState::Connected;
        self.quick.update(cx, |panel, cx| {
            panel.sync(display_name, connected, cx);
        });
        if let Some(section) = self.sign_in.twitch_bot.clone() {
            let streamer_login = self.ports.status.hero_name();
            section.update(cx, |section, cx| {
                section.set_streamer_login(streamer_login, cx)
            });
        }
        self.refresh_health_display();
        self.sync_token_countdown(cx);
        cx.notify();
    }

    pub(super) fn adopt_builtin(&mut self, object: BuiltinObject, cx: &mut Context<Self>) {
        self.ports = BuiltinPorts::from_object(object);
        self.sign_in.connect = None;
        let source = self.ports.quick_action_source();
        self.quick.update(cx, |panel, cx| panel.rebind(source, cx));
        self.live.viewer_samples.clear();
        self.background._health_bridge = Self::spawn_health_bridge(&self.ports.health, cx);
        self.background._change_bridge = Self::spawn_change_bridge(&self.ports, cx);
        self.reload(cx);
        self.sample_viewers();
    }

    pub(super) fn sign_out(&mut self, cx: &mut Context<Self>) {
        if self.live.signing_out {
            return;
        }
        let Some(entry) = self.services.sign_out.entry(self.ports.status.id()) else {
            return;
        };
        let control = self.ports.control.clone();
        self.live.signing_out = true;
        cx.notify();
        async_bridge::run_async(
            &self.services.rt_handle,
            entry.run(control),
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
                        this.apply_signed_out(cx);
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

    fn apply_signed_out(&mut self, cx: &mut Context<Self>) {
        self.ports.control = None;
        self.quick.update(cx, |panel, cx| {
            panel.close_collection_manager();
            cx.notify();
        });
        self.live.viewer_samples.clear();
        match chat_platform_of(self.ports.status.id()) {
            Some(platform) => self.open_connect_flow(platform, cx),
            None => cx.emit(IntegrationSignedOut),
        }
    }

    pub(super) fn resync_content(&mut self, cx: &mut Context<Self>) {
        self.ports.content.request_refresh();
        self.reload(cx);
    }
}
