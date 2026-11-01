use std::sync::Arc;

use forge_components::{ActivePresentation, ForgePalette, Icon, tr};
use forge_events::EventPublisher;
use forge_storage::{CredentialsRepo, SettingsRepo};
use gpui::{App, Context, Entity, EventEmitter, Subscription, Window, prelude::*};

use crate::connect_scaffold::{ConnectPage, GuideStep, fragment, mono_fragment};
use crate::navigation::NavRequested;
use crate::vtube_connect_form::{DEFAULT_PORT, VTubeConnectForm, VTubeConnected};
use forge_host::integrations::VTubeInstallSeed;

pub(crate) struct VTubeConnectView {
    form: Entity<VTubeConnectForm>,
    _form_sub: Subscription,
}

impl EventEmitter<NavRequested> for VTubeConnectView {}
impl EventEmitter<VTubeConnected> for VTubeConnectView {}

impl VTubeConnectView {
    pub(crate) fn new(
        rt_handle: tokio::runtime::Handle,
        credentials: Arc<dyn CredentialsRepo>,
        settings: Arc<dyn SettingsRepo>,
        bus: Arc<dyn EventPublisher>,
        seed: VTubeInstallSeed,
        cx: &mut Context<Self>,
    ) -> Self {
        let form =
            cx.new(|cx| VTubeConnectForm::new(rt_handle, credentials, settings, bus, seed, cx));
        let form_sub = cx.subscribe(&form, |_, _, _: &VTubeConnected, cx| {
            cx.emit(VTubeConnected)
        });

        Self {
            form,
            _form_sub: form_sub,
        }
    }
}

fn guide_steps(palette: &ForgePalette, cx: &App) -> Vec<GuideStep> {
    vec![
        vec![
            fragment(
                tr!("vtube_connect_step_open_prefix"),
                palette.text_primary,
                cx,
            ),
            fragment(tr!("vtube_connect_step_open_path"), palette.text_muted, cx),
        ],
        vec![
            fragment(
                tr!("vtube_connect_step_enable_prefix"),
                palette.text_primary,
                cx,
            ),
            fragment(tr!("vtube_connect_step_enable_option"), palette.success, cx),
        ],
        vec![
            fragment(
                tr!("vtube_connect_step_port_prefix"),
                palette.text_primary,
                cx,
            ),
            mono_fragment(
                tr!("vtube_connect_step_port_field"),
                palette.text_primary,
                cx,
            ),
            fragment(
                tr!("vtube_connect_step_port_default_prefix"),
                palette.text_primary,
                cx,
            ),
            mono_fragment(
                tr!(
                    "connect_step_port_default_value",
                    port = DEFAULT_PORT.to_string()
                ),
                palette.text_primary,
                cx,
            ),
        ],
        vec![
            fragment(
                tr!("vtube_connect_step_approve_prefix"),
                palette.text_primary,
                cx,
            ),
            fragment(
                tr!("vtube_connect_step_approve_button"),
                palette.success,
                cx,
            ),
            fragment(
                tr!("vtube_connect_step_approve_suffix"),
                palette.text_primary,
                cx,
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
            steps: guide_steps(&palette, cx),
            form: self.form.clone().into(),
        }
        .render(&palette, density, cx)
    }
}
