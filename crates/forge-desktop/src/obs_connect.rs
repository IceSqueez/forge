use std::sync::Arc;

use forge_components::{ActivePresentation, ForgePalette, Icon, tr};
use forge_events::EventPublisher;
use forge_storage::{CredentialsRepo, SettingsRepo};
use gpui::{Context, Entity, EventEmitter, SharedString, Subscription, Window, prelude::*};

use crate::connect_scaffold::{ConnectPage, GuideStep, fragment, mono_fragment};
use crate::integrations::ObsInstallSeed;
use crate::obs_credentials_form::{DEFAULT_PORT, ObsConnected, ObsCredentialsForm, ObsSubmit};
use crate::sidebar::NavRequested;

pub struct ObsConnectView {
    form: Entity<ObsCredentialsForm>,
    _form_sub: Subscription,
}

impl EventEmitter<NavRequested> for ObsConnectView {}
impl EventEmitter<ObsConnected> for ObsConnectView {}

impl ObsConnectView {
    pub fn new(
        rt_handle: tokio::runtime::Handle,
        credentials: Arc<dyn CredentialsRepo>,
        settings: Arc<dyn SettingsRepo>,
        bus: Arc<dyn EventPublisher>,
        seed: ObsInstallSeed,
        cx: &mut Context<Self>,
    ) -> Self {
        let form = cx.new(|cx| {
            ObsCredentialsForm::new(
                rt_handle,
                credentials,
                settings,
                bus,
                seed,
                ObsSubmit::Connect,
                cx,
            )
        });
        let form_sub = cx.subscribe(&form, |_, _, _: &ObsConnected, cx| cx.emit(ObsConnected));

        Self {
            form,
            _form_sub: form_sub,
        }
    }
}

fn guide_steps(palette: &ForgePalette) -> Vec<GuideStep> {
    vec![
        vec![
            fragment(tr!("obs_connect_step_menu_prefix"), palette.text_primary),
            fragment(tr!("obs_connect_step_menu_path"), palette.text_muted),
        ],
        vec![
            fragment(tr!("obs_connect_step_enable_prefix"), palette.text_primary),
            fragment(tr!("obs_connect_step_enable_option"), palette.success),
        ],
        vec![
            fragment(tr!("obs_connect_step_port_prefix"), palette.text_primary),
            mono_fragment(tr!("obs_connect_step_port_field"), palette.text_primary),
            fragment(
                tr!("obs_connect_step_port_default_prefix"),
                palette.text_primary,
            ),
            mono_fragment(
                SharedString::from(format!("{DEFAULT_PORT})")),
                palette.text_primary,
            ),
        ],
        vec![
            fragment(tr!("obs_connect_step_reveal_prefix"), palette.text_primary),
            fragment(tr!("obs_connect_step_reveal_button"), palette.success),
            fragment(tr!("obs_connect_step_reveal_suffix"), palette.text_primary),
        ],
    ]
}

impl Render for ObsConnectView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        ConnectPage {
            scroll_id: "obs-connect-scroll",
            crumb_id: "obs-connect-crumb-apps",
            hero_glyph: Icon::Broadcast,
            hero_tint: palette.success,
            step_number_tint: palette.brand,
            title_key: "obs_connect_title",
            subtitle_key: "obs_connect_subtitle",
            guide_title_key: "obs_connect_guide_title",
            guide_intro_key: "obs_connect_guide_intro",
            settings_title_key: "obs_connect_settings_title",
            steps: guide_steps(&palette),
            form: self.form.clone().into(),
        }
        .render(&palette, density, cx)
    }
}
