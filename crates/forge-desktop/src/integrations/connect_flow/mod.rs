use crate::integrations::platform_visuals::{platform_color, platform_letter};
mod device_code;
mod local_callback;

use std::sync::Arc;

use forge_components::{
    ActivePresentation, Density, ForgePalette, Spacing, platform_hero, spacing, tr,
};
use forge_storage::CredentialsRepo;
use forge_types::PlatformId;
use gpui::{AnyElement, App, Context, EventEmitter, Rgba, Window, div, prelude::*, px};

use crate::app::async_bridge::{self, ErrorSink};
use forge_host::integrations::BuiltinObject;
use forge_runtime::IntegrationSlot;

use device_code::DeviceCodeState;
use forge_platform_core::LoopbackSignIn;
use local_callback::LocalCallbackState;

const DOT_RADIUS: gpui::Pixels = px(4.0);
const STATUS_DOT: gpui::Pixels = px(8.0);

pub(crate) enum ConnectFlowEvent {
    Connected(BuiltinObject),
    Leave,
}

pub(crate) enum ConnectSignIn {
    DeviceCode(Option<forge_platform_twitch::TwitchSignIn>),
    LocalCallback(Option<Arc<dyn LoopbackSignIn>>),
}

pub(crate) struct ConnectFlowLaunch {
    pub platform: PlatformId,
    pub display_name: String,
    pub rt_handle: tokio::runtime::Handle,
    pub credentials: Arc<dyn CredentialsRepo>,
    pub slot: Option<IntegrationSlot<BuiltinObject>>,
    pub sign_in: ConnectSignIn,
}

enum FlowState {
    DeviceCode(DeviceCodeState),
    LocalCallback(LocalCallbackState),
}

pub(crate) struct ConnectFlow {
    platform: PlatformId,
    display_name: String,
    rt_handle: tokio::runtime::Handle,
    credentials: Arc<dyn CredentialsRepo>,
    slot: Option<IntegrationSlot<BuiltinObject>>,
    state: FlowState,
}

impl EventEmitter<ConnectFlowEvent> for ConnectFlow {}

impl Drop for ConnectFlow {
    fn drop(&mut self) {
        match &self.state {
            FlowState::DeviceCode(state) => state.cancel(),
            FlowState::LocalCallback(state) => state.cancel(),
        }
    }
}

impl ConnectFlow {
    pub(crate) fn new(launch: ConnectFlowLaunch, cx: &mut Context<Self>) -> Self {
        if matches!(launch.sign_in, ConnectSignIn::DeviceCode(_)) {
            cx.spawn(async move |this, cx| {
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the view may close before the result arrives"
                )]
                let _ = this.update(cx, |this, cx| this.begin_twitch_device(cx));
            })
            .detach();
        }
        Self::idle(launch)
    }

    fn idle(launch: ConnectFlowLaunch) -> Self {
        let ConnectFlowLaunch {
            platform,
            display_name,
            rt_handle,
            credentials,
            slot,
            sign_in,
        } = launch;
        Self {
            platform,
            display_name,
            rt_handle,
            credentials,
            slot,
            state: match sign_in {
                ConnectSignIn::DeviceCode(sign_in) => {
                    FlowState::DeviceCode(DeviceCodeState::new(sign_in))
                }
                ConnectSignIn::LocalCallback(sign_in) => {
                    FlowState::LocalCallback(LocalCallbackState::new(sign_in))
                }
            },
        }
    }

    pub(crate) fn status_indicator(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &App,
    ) -> AnyElement {
        match &self.state {
            FlowState::DeviceCode(_) => self.twitch_device_status(palette, density, cx),
            FlowState::LocalCallback(_) => self.connect_status(palette, density, cx),
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

    fn finish(&mut self, object: BuiltinObject, cx: &mut Context<Self>) {
        cx.emit(ConnectFlowEvent::Connected(object));
    }

    fn leave(&mut self, cx: &mut Context<Self>) {
        if let Some(local) = self.local_mut() {
            local.abandon();
        }
        cx.emit(ConnectFlowEvent::Leave);
    }
}

impl Render for ConnectFlow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();
        let accent = platform_color(self.platform, &palette);
        let (letter, desc) = connect_copy(self.platform);
        let hero = platform_hero(letter, accent, self.display_name.clone(), desc, &palette)
            .density(density);
        let column = match &self.state {
            FlowState::DeviceCode(_) => self.twitch_device_column(accent, &palette, density, cx),
            FlowState::LocalCallback(_) => {
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

fn connect_copy(platform: PlatformId) -> (&'static str, String) {
    let description = match platform {
        PlatformId::Twitch => tr!("twitch_description"),
        PlatformId::Kick => tr!("kick_description"),
        PlatformId::YouTube => tr!("youtube_description"),
    };
    (platform_letter(platform), description)
}

fn credentials_missing(platform: PlatformId) -> String {
    match platform {
        PlatformId::Twitch => tr!("auth_error_credentials_missing_twitch"),
        PlatformId::YouTube => tr!("auth_error_credentials_missing_youtube"),
        PlatformId::Kick => tr!("auth_error_credentials_missing_kick"),
    }
}

fn status_dot(color: Rgba) -> impl IntoElement {
    div()
        .flex_none()
        .size(STATUS_DOT)
        .rounded(DOT_RADIUS)
        .bg(color)
}
