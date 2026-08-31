mod device_code;
mod local_callback;

use std::sync::Arc;

use forge_components::{
    Density, ForgePalette, PlatformKind, Spacing, platform_color, platform_hero, spacing, tr,
};
use forge_events::EventPublisher;
use forge_platform_kick::KickIntegrationBundle;
use forge_platform_twitch::TwitchIntegrationBundle;
use forge_platform_youtube::YoutubeIntegrationBundle;
use forge_runtime::{EventBus, LiveViewerAggregatorHandle};
use forge_storage::CredentialsRepo;
use forge_types::PlatformId;
use gpui::{AnyElement, Context, EventEmitter, Rgba, Window, div, prelude::*, px};
use tokio_util::sync::CancellationToken;

use crate::async_bridge::{self, ErrorSink};
use crate::integrations::{KickInstallSeed, TwitchInstallSeed, YoutubeInstallSeed};
use crate::presentation::ActivePresentation;

use device_code::{TwitchDeviceState, TwitchFlowHandle};
use local_callback::{KickFlowHandle, LocalCallbackFlowPhase, YoutubeFlowHandle};

pub enum ConnectedBundle {
    Twitch(Arc<TwitchIntegrationBundle>),
    Youtube(Arc<YoutubeIntegrationBundle>),
    Kick(Arc<KickIntegrationBundle>),
}

pub enum ConnectFlowEvent {
    Connected(ConnectedBundle),
    Leave,
}

pub struct ConnectFlowLaunch {
    pub platform: PlatformId,
    pub display_name: String,
    pub rt_handle: tokio::runtime::Handle,
    pub credentials: Arc<dyn CredentialsRepo>,
    pub bus: Arc<dyn EventPublisher>,
    pub event_bus: Arc<EventBus>,
    pub live_viewers: LiveViewerAggregatorHandle,
    pub twitch_install_seed: Option<TwitchInstallSeed>,
    pub kick_install_seed: Option<KickInstallSeed>,
    pub youtube_install_seed: Option<YoutubeInstallSeed>,
}

pub struct ConnectFlow {
    platform: PlatformId,
    display_name: String,
    rt_handle: tokio::runtime::Handle,
    credentials: Arc<dyn CredentialsRepo>,
    bus: Arc<dyn EventPublisher>,
    event_bus: Arc<EventBus>,
    live_viewers: LiveViewerAggregatorHandle,
    twitch_install_seed: Option<TwitchInstallSeed>,
    kick_install_seed: Option<KickInstallSeed>,
    youtube_install_seed: Option<YoutubeInstallSeed>,
    phase: LocalCallbackFlowPhase,
    auth_url: Option<String>,
    error: Option<String>,
    youtube_flow: Option<YoutubeFlowHandle>,
    kick_flow: Option<KickFlowHandle>,
    local_cancel: CancellationToken,
    twitch_flow: Option<TwitchFlowHandle>,
    twitch_device: Option<TwitchDeviceState>,
}

impl EventEmitter<ConnectFlowEvent> for ConnectFlow {}

impl Drop for ConnectFlow {
    fn drop(&mut self) {
        if let Some(dev) = &self.twitch_device {
            dev.cancel.cancel();
        }
        self.local_cancel.cancel();
    }
}

impl ConnectFlow {
    pub fn new(launch: ConnectFlowLaunch, cx: &mut Context<Self>) -> Self {
        let ConnectFlowLaunch {
            platform,
            display_name,
            rt_handle,
            credentials,
            bus,
            event_bus,
            live_viewers,
            twitch_install_seed,
            kick_install_seed,
            youtube_install_seed,
        } = launch;

        if platform == PlatformId::Twitch {
            cx.spawn(async move |this, cx| {
                let _ = this.update(cx, |this, cx| this.begin_twitch_device(cx));
            })
            .detach();
        }

        Self {
            platform,
            display_name,
            rt_handle,
            credentials,
            bus,
            event_bus,
            live_viewers,
            twitch_install_seed,
            kick_install_seed,
            youtube_install_seed,
            phase: LocalCallbackFlowPhase::Idle,
            auth_url: None,
            error: None,
            youtube_flow: None,
            kick_flow: None,
            local_cancel: CancellationToken::new(),
            twitch_flow: None,
            twitch_device: None,
        }
    }

    pub(crate) fn status_indicator(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        match self.platform {
            PlatformId::Twitch => self.twitch_device_status(palette, density),
            PlatformId::YouTube | PlatformId::Kick => self.connect_status(palette, density),
        }
    }

    fn open_url(&self, url: String, cx: &mut Context<Self>) {
        async_bridge::open_external(
            &self.rt_handle,
            url,
            ErrorSink::Toast,
            tr!("integration_open_url_failed"),
            cx,
        );
    }

    fn finish(&mut self, bundle: ConnectedBundle, cx: &mut Context<Self>) {
        cx.emit(ConnectFlowEvent::Connected(bundle));
    }

    fn leave(&mut self, cx: &mut Context<Self>) {
        self.abandon_local_flow();
        cx.emit(ConnectFlowEvent::Leave);
    }
}

impl Render for ConnectFlow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();
        let accent = platform_accent(self.platform, &palette);
        let (letter, desc) = connect_copy(self.platform);
        let hero = platform_hero(letter, accent, self.display_name.clone(), desc, &palette)
            .density(density);
        let column = match self.platform {
            PlatformId::Twitch => self.twitch_device_column(accent, &palette, density, cx),
            PlatformId::YouTube | PlatformId::Kick => {
                self.local_callback_column(accent, &palette, density, cx)
            }
        };
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .child(hero)
            .child(div().w_full().flex().justify_center().child(column))
    }
}

fn platform_accent(platform: PlatformId, palette: &ForgePalette) -> Rgba {
    match platform {
        PlatformId::Twitch => platform_color(PlatformKind::Twitch, palette),
        PlatformId::YouTube => platform_color(PlatformKind::YouTube, palette),
        PlatformId::Kick => platform_color(PlatformKind::Kick, palette),
    }
}

fn connect_copy(platform: PlatformId) -> (&'static str, String) {
    match platform {
        PlatformId::Twitch => ("T", tr!("twitch_description")),
        PlatformId::Kick => ("K", tr!("kick_description")),
        PlatformId::YouTube => ("Y", tr!("youtube_description")),
    }
}

fn status_dot(color: Rgba) -> impl IntoElement {
    div().flex_none().size(px(8.0)).rounded(px(4.0)).bg(color)
}
