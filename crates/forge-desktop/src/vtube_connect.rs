use std::sync::Arc;

use forge_components::{ActivePresentation, ForgePalette, Icon, tr};
use forge_events::EventPublisher;
use forge_runtime::EventBus;
use forge_storage::{CredentialsRepo, SettingsRepo};
use gpui::{Context, Entity, EventEmitter, SharedString, Subscription, Window, prelude::*};

use crate::connect_scaffold::{ConnectPage, GuideStep, fragment, mono_fragment};
use crate::integrations::VTubeInstallSeed;
use crate::navigation::NavRequested;
use crate::vtube_connect_form::{DEFAULT_PORT, VTubeConnectForm, VTubeConnected};

pub struct VTubeConnectView {
    form: Entity<VTubeConnectForm>,
    _form_sub: Subscription,
}

impl EventEmitter<NavRequested> for VTubeConnectView {}
impl EventEmitter<VTubeConnected> for VTubeConnectView {}

impl VTubeConnectView {
    pub fn new(
        rt_handle: tokio::runtime::Handle,
        credentials: Arc<dyn CredentialsRepo>,
        settings: Arc<dyn SettingsRepo>,
        bus: Arc<dyn EventPublisher>,
        event_bus: Arc<EventBus>,
        seed: VTubeInstallSeed,
        cx: &mut Context<Self>,
    ) -> Self {
        let form = cx.new(|cx| {
            VTubeConnectForm::new(rt_handle, credentials, settings, bus, event_bus, seed, cx)
        });
        let form_sub = cx.subscribe(&form, |_, _, _: &VTubeConnected, cx| {
            cx.emit(VTubeConnected)
        });

        Self {
            form,
            _form_sub: form_sub,
        }
    }
}

fn guide_steps(palette: &ForgePalette) -> Vec<GuideStep> {
    vec![
        vec![
            fragment(tr!("vtube_connect_step_open_prefix"), palette.text_primary),
            fragment(tr!("vtube_connect_step_open_path"), palette.text_muted),
        ],
        vec![
            fragment(
                tr!("vtube_connect_step_enable_prefix"),
                palette.text_primary,
            ),
            fragment(tr!("vtube_connect_step_enable_option"), palette.success),
        ],
        vec![
            fragment(tr!("vtube_connect_step_port_prefix"), palette.text_primary),
            mono_fragment(tr!("vtube_connect_step_port_field"), palette.text_primary),
            fragment(
                tr!("vtube_connect_step_port_default_prefix"),
                palette.text_primary,
            ),
            mono_fragment(
                SharedString::from(format!("{DEFAULT_PORT})")),
                palette.text_primary,
            ),
        ],
        vec![
            fragment(
                tr!("vtube_connect_step_approve_prefix"),
                palette.text_primary,
            ),
            fragment(tr!("vtube_connect_step_approve_button"), palette.success),
            fragment(
                tr!("vtube_connect_step_approve_suffix"),
                palette.text_primary,
            ),
        ],
    ]
}

impl Render for VTubeConnectView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        ConnectPage {
            scroll_id: "vtube-connect-scroll",
            crumb_id: "vtube-connect-crumb-apps",
            hero_glyph: Icon::MoodTongue,
            hero_tint: palette.warning,
            step_number_tint: palette.warning,
            title_key: "vtube_connect_title",
            subtitle_key: "vtube_connect_subtitle",
            guide_title_key: "vtube_connect_guide_title",
            guide_intro_key: "vtube_connect_guide_intro",
            settings_title_key: "vtube_connect_settings_title",
            steps: guide_steps(&palette),
            form: self.form.clone().into(),
        }
        .render(&palette, density, cx)
    }
}
