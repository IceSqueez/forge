mod attachments;
mod banners;
mod bridges;
mod connect_lifecycle;
mod controls;
mod header;
mod row_steps;
mod section_data;
mod state;
#[cfg(test)]
mod tests;

pub(crate) use self::state::DetailServices;
use self::state::{
    BuiltinPorts, DetailBackground, DetailModals, LiveTallies, ShownState, SignInState,
};
use crate::app::navigation::NavRequested;
use crate::integrations::builtin_sections::{
    SectionHooks, SectionRefresh, SectionStepRun, StepClick, content_sections, health_grid,
};
use crate::integrations::connect_flow::ConnectFlow;
use crate::integrations::donation_settings::{DonationSettingsView, remove_confirm};
use crate::integrations::hub_crumb::hub_crumb;
use crate::integrations::integration_catalog::chat_platform_of;
use crate::integrations::integration_quick_actions::{
    QuickActionServices, QuickActionsPanel, QuickActionsPanelEvent,
};
use crate::integrations::obs_credentials_form::ObsConnected;
use forge_components::OverflowRevealingFocus;
use forge_components::{
    ActivePresentation, BreadcrumbCrumb, Density, ForgePalette, page_frame, scaled,
};
use forge_host::integration_catalog::declaration_of;
use forge_host::integrations::BuiltinObject;
use forge_platform_core::ConnectionState;
use forge_types::{IntegrationId, PlatformId};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, Pixels, Subscription, Window, div,
    prelude::*, px,
};
use std::rc::Rc;
use std::sync::Arc;

struct ActiveConnect {
    view: Entity<ConnectFlow>,
    _subs: Vec<Subscription>,
}

struct DonationSettingsLink {
    view: Entity<DonationSettingsView>,
    _repaint: Subscription,
}

pub(crate) struct IntegrationDetail {
    ports: BuiltinPorts,
    services: DetailServices,
    sign_in: SignInState,
    shown: ShownState,
    quick: Entity<QuickActionsPanel>,
    live: LiveTallies,
    modals: DetailModals,
    background: DetailBackground,
}

const VIEWER_DELTA_WINDOW_MINUTES: u64 = 15;
const BODY_GAP: Pixels = px(14.0);
const BODY_PAD_Y: Pixels = px(18.0);
const BODY_PAD_X: Pixels = px(22.0);

pub(crate) struct IntegrationSignedOut;

impl EventEmitter<NavRequested> for IntegrationDetail {}
impl EventEmitter<ObsConnected> for IntegrationDetail {}
impl EventEmitter<IntegrationSignedOut> for IntegrationDetail {}

impl IntegrationDetail {
    pub(crate) fn new(
        object: BuiltinObject,
        services: DetailServices,
        cx: &mut Context<Self>,
    ) -> Self {
        let ports = BuiltinPorts::from_object(object);

        let connect_platform = connect_platform_for(ports.status.id(), ports.control.is_some());
        let shown = ShownState::read(&ports);
        let quick_services = QuickActionServices {
            rt_handle: services.rt_handle.clone(),
            action_engine: services.action_engine.clone(),
            history: Arc::clone(&services.history),
            trigger_registry: Arc::clone(&services.trigger_registry),
        };
        let quick_source = ports.quick_action_source();
        let display_name = shown.display_name.clone();
        let quick =
            cx.new(|cx| QuickActionsPanel::new(quick_services, quick_source, display_name, cx));
        let quick_sub = cx.subscribe(&quick, Self::on_quick_panel_event);

        let health_bridge = Self::spawn_health_bridge(&ports.health, cx);
        let change_bridge = Self::spawn_change_bridge(&ports, cx);

        let mut this = Self {
            ports,
            services,
            sign_in: SignInState::default(),
            shown,
            quick,
            live: LiveTallies::default(),
            modals: DetailModals::default(),
            background: DetailBackground {
                _quick_sub: quick_sub,
                _health_bridge: health_bridge,
                _change_bridge: change_bridge,
                _token_countdown_repaint: None,
            },
        };

        this.sync_token_countdown(cx);
        this.sample_viewers();
        if let Some(platform) = connect_platform {
            this.open_connect_flow(platform, cx);
        }
        this
    }
}

impl IntegrationDetail {
    fn on_quick_panel_event(
        &mut self,
        _panel: Entity<QuickActionsPanel>,
        event: &QuickActionsPanelEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            QuickActionsPanelEvent::Ran => self.reload(cx),
            QuickActionsPanelEvent::Navigate(screen) => self.navigate_to(screen.clone(), cx),
        }
    }

    fn connect_body(
        &self,
        connect: &ActiveConnect,
        density: Density,
        cx: &Context<Self>,
    ) -> AnyElement {
        let twitch_bot = self
            .sign_in
            .twitch_bot
            .clone()
            .filter(|section| section.read(cx).has_bot_account(cx));
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(scaled(BODY_GAP, density))
            .child(connect.view.clone())
            .children(twitch_bot)
            .into_any_element()
    }

    fn detail_body(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let header_card = self.header_card(palette, density, cx);
        let reconnecting = matches!(
            self.shown.connection,
            ConnectionState::Connecting | ConnectionState::Reconnecting
        );
        let state_banner = self.state_banner(palette, density, cx);
        let credential_banner = self
            .needed_credential_recovery()
            .map(|recovery| self.credential_banner(recovery, palette, density, cx));
        let health = health_grid(
            &self.shown.health_display,
            reconnecting,
            palette,
            density,
            cx,
        );
        let refresh: SectionRefresh =
            Rc::new(cx.listener(|this, _: &ClickEvent, _, cx| this.resync_content(cx)));
        let run_step: SectionStepRun =
            Rc::new(cx.listener(|this, click: &StepClick, _, cx| this.run_row_step(click, cx)));
        let hooks = SectionHooks {
            refresh: Some(refresh),
            run_step: Some(run_step),
        };
        let content = content_sections(&self.shown.sections, &hooks, palette, density, cx);
        let donation_settings = self
            .sign_in
            .donation_settings
            .as_ref()
            .map(|link| link.view.clone());
        let twitch_bot = self.sign_in.twitch_bot.clone();

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(scaled(BODY_GAP, density))
            .children(credential_banner)
            .children(state_banner)
            .child(header_card)
            .child(health)
            .child(content)
            .children(twitch_bot)
            .children(donation_settings)
            .child(self.quick.clone())
            .into_any_element()
    }

    fn header_status(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let is_oauth_platform = self.is_oauth_platform();
        match &self.sign_in.connect {
            Some(connect) => Some(connect.view.read(cx).status_indicator(palette, density, cx)),
            None if is_oauth_platform && self.shown.connection == ConnectionState::Connected => {
                Some(self.header_right_connected(palette, density, cx))
            }
            None if is_oauth_platform => None,
            None => Some(self.header_right_connection(palette, density, cx)),
        }
    }
}

impl Render for IntegrationDetail {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let body = match &self.sign_in.connect {
            Some(connect) => self.connect_body(connect, density, cx),
            None => self.detail_body(&palette, density, cx),
        };

        let scroll = div()
            .id("integration-detail-scroll")
            .flex_1()
            .overflow_y_revealing_focus()
            .bg(palette.base)
            .child(
                div()
                    .w_full()
                    .py(scaled(BODY_PAD_Y, density))
                    .px(scaled(BODY_PAD_X, density))
                    .child(body),
            );

        let donation_remove_confirm = self
            .sign_in
            .donation_settings
            .as_ref()
            .and_then(|link| remove_confirm(&link.view, &palette, cx));
        let disconnect_overlay = self
            .modals
            .pending_disconnect
            .is_pending()
            .then(|| self.disconnect_overlay(&palette, cx));

        let ancestor_crumb = hub_crumb(
            declaration_of(self.ports.status.id()).map(|declaration| declaration.category),
            cx,
        );

        let header_right = self.header_status(&palette, density, cx);
        let mut frame = page_frame(
            vec![
                ancestor_crumb,
                BreadcrumbCrumb::leaf(self.shown.display_name.clone()),
            ],
            &palette,
        );
        if let Some(status) = header_right {
            frame = frame.header_right(status);
        }
        let frame = frame.body(scroll);

        let panel = self.quick.read(cx);
        let quick_layers = panel.modal_layers();
        let history_layer = panel.history_layer();

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(frame)
            .children(disconnect_overlay)
            .children(donation_remove_confirm)
            .children(quick_layers)
            .children(self.modals.obs_settings_modal.clone())
            .children(history_layer)
    }
}

fn connect_platform_for(id: &IntegrationId, has_control: bool) -> Option<PlatformId> {
    if has_control {
        return None;
    }
    chat_platform_of(id)
}
