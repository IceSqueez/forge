use std::sync::Arc;

use forge_components::{
    ForgePalette, Icon, OverlayPosition, body_family, destructive_button_with_icon, modal, overlay,
    tr,
};
use forge_events::EventPublisher;
use forge_storage::{CredentialsRepo, SettingsRepo};
use gpui::{
    ClickEvent, Context, Entity, EventEmitter, Pixels, Subscription, Window, div, prelude::*, px,
};

use crate::integrations::ObsInstallSeed;
use crate::obs_credentials_form::{ObsConnected, ObsCredentialsForm, ObsSubmit};
use crate::presentation::ActivePresentation;

const MODAL_WIDTH: Pixels = px(460.0);
const FOOTER_GAP: Pixels = px(12.0);
const HINT_SIZE: Pixels = px(11.0);

pub enum ObsSettingsModalEvent {
    Close,
    Saved,
    Disconnect,
}

pub struct ObsSettingsModal {
    form: Entity<ObsCredentialsForm>,
    _form_sub: Subscription,
}

impl EventEmitter<ObsSettingsModalEvent> for ObsSettingsModal {}

impl ObsSettingsModal {
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
                ObsSubmit::SaveAndReconnect,
                cx,
            )
        });
        let form_sub = cx.subscribe(&form, |_, _, _: &ObsConnected, cx| {
            cx.emit(ObsSettingsModalEvent::Saved)
        });

        Self {
            form,
            _form_sub: form_sub,
        }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.form.update(cx, |form, cx| form.focus(window, cx));
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        cx.emit(ObsSettingsModalEvent::Close);
    }

    fn request_disconnect(&mut self, cx: &mut Context<Self>) {
        cx.emit(ObsSettingsModalEvent::Disconnect);
    }

    fn footer(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> impl IntoElement {
        let disconnect = destructive_button_with_icon(
            Icon::Logout,
            tr!("widget_header_action_disconnect"),
            palette,
        )
        .on_click(
            "obs-settings-disconnect",
            cx.listener(|this, _: &ClickEvent, _, cx| this.request_disconnect(cx)),
        );

        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(FOOTER_GAP)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .font_family(body_family())
                    .text_size(HINT_SIZE)
                    .text_color(palette.text_faint)
                    .child(tr!("obs_settings_disconnect_hint")),
            )
            .child(disconnect)
    }
}

impl Render for ObsSettingsModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();

        let card = modal(
            tr!("obs_connect_settings_title"),
            self.form.clone(),
            &palette,
        )
        .header_icon(Icon::Settings, palette.text_secondary)
        .subtitle(tr!("obs_connect_title"))
        .width(MODAL_WIDTH)
        .footer(self.footer(&palette, cx))
        .on_close(
            "obs-settings-close",
            cx.listener(|this, _: &ClickEvent, _, cx| this.close(cx)),
        );

        let view = cx.entity();
        let scrim = overlay(card, &palette)
            .position(OverlayPosition::Center)
            .on_dismiss("obs-settings-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.close(cx));
            });

        div().absolute().top_0().left_0().size_full().child(scrim)
    }
}
