mod attachments;
mod banners;
mod bridges;
mod connect_lifecycle;
mod controls;
mod header;
mod quick_actions;
mod section_data;
mod state;
#[cfg(test)]
mod tests;

use self::quick_actions::reachable_quick_actions;
use self::state::{
    BuiltinPorts, DetailBackground, DetailModals, LiveTallies, ShownState, SignInState,
};
pub(crate) use self::state::{DetailFeeds, DetailServices};
use crate::builtin_sections::{
    SectionHooks, SectionRefresh, SectionStepRun, StepClick, content_sections, health_grid,
};
use crate::connect_flow::ConnectFlow;
use crate::donation_settings::{DonationSettingsView, remove_confirm};
use crate::hub_crumb::hub_crumb;
use crate::integration_catalog::declaration_of;
use crate::navigation::NavRequested;
use crate::obs_credentials_form::ObsConnected;
use forge_components::OverflowRevealingFocus;
use forge_components::{ActivePresentation, BreadcrumbCrumb, SearchState, page_frame, scaled, tr};
use forge_host::integrations::BuiltinObject;
use forge_platform_core::{ConnectionState, QuickAction};
use forge_runtime::{EventBus, LiveViewerAggregatorHandle};
use forge_types::PlatformId;
use gpui::{
    ClickEvent, Context, Entity, EventEmitter, Subscription, Task, Window, div, prelude::*, px,
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
    pub(crate) quick_actions: Vec<QuickAction>,
    pub(crate) qa_search: SearchState,
    live: LiveTallies,
    modals: DetailModals,
    background: DetailBackground,
}

const VIEWER_DELTA_WINDOW_MINUTES: u64 = 15;

pub(crate) struct ObsSignedOut;
pub(crate) struct VTubeSignedOut;

impl EventEmitter<NavRequested> for IntegrationDetail {}
impl EventEmitter<ObsConnected> for IntegrationDetail {}
impl EventEmitter<ObsSignedOut> for IntegrationDetail {}
impl EventEmitter<VTubeSignedOut> for IntegrationDetail {}

impl IntegrationDetail {
    pub(crate) fn new(
        object: BuiltinObject,
        services: DetailServices,
        feeds: DetailFeeds,
        cx: &mut Context<Self>,
    ) -> Self {
        let DetailFeeds {
            event_bus,
            live_viewers,
            connectivity,
        } = feeds;
        let BuiltinObject {
            icon,
            status,
            health,
            content,
            quick,
            control,
            collections,
            option_source,
            obs_client: obs_source,
            vtube_client: _,
            follow: _,
            ban_list: _,
        } = object;
        let ports = BuiltinPorts {
            status,
            health,
            content,
            quick,
            control,
            collections,
            option_source,
            obs_source,
        };
        let is_obs_detail = ports.status.id().as_str() == "obs";
        let obs_password_rejected = is_obs_detail && connectivity.read(cx).obs_password_rejected();
        let conn_obs = cx.observe(&connectivity, move |this, connectivity, cx| {
            this.shown.obs_password_rejected =
                this.shown.is_obs && connectivity.read(cx).obs_password_rejected();
            this.reload(cx)
        });

        let palette = cx.palette();
        let qa_search = SearchState::from_field(cx.new(|cx| {
            forge_components::search_input(tr!("integration_qa_filter_placeholder"), palette, cx)
                .compact()
                .with_font_size(px(11.5))
        }));
        let qa_search_sub = cx.subscribe(qa_search.field(), Self::on_qa_search);
        let connect_platform =
            connect_platform_for(ports.status.id().as_str(), ports.control.is_some());
        let shown = ShownState::read(icon, obs_password_rejected, &ports);
        let quick_actions = reachable_quick_actions(&*ports.quick, ports.collections.as_deref());

        let health_bridge = Self::spawn_health_bridge(&ports.health, cx);
        let quick_action_bridge = Self::spawn_quick_action_bridge(&*ports.quick, cx);
        let bridges = Self::spawn_platform_bridges(&ports, &shown, &event_bus, &live_viewers, cx);

        let mut this = Self {
            ports,
            services,
            sign_in: SignInState::default(),
            shown,
            quick_actions,
            qa_search,
            live: LiveTallies::default(),
            modals: DetailModals::default(),
            background: DetailBackground {
                _conn_obs: conn_obs,
                _qa_search_sub: qa_search_sub,
                _health_bridge: health_bridge,
                _quick_action_bridge: quick_action_bridge,
                _bridges: bridges,
            },
        };

        if let Some(platform) = connect_platform {
            this.open_connect_flow(platform, cx);
        }
        this
    }

    fn spawn_platform_bridges(
        ports: &BuiltinPorts,
        shown: &ShownState,
        event_bus: &Arc<EventBus>,
        live_viewers: &LiveViewerAggregatorHandle,
        cx: &mut Context<Self>,
    ) -> Vec<Task<()>> {
        let mut bridges = Vec::new();
        if shown.is_twitch {
            bridges.push(Self::spawn_eventsub_tally(event_bus, cx));
        }
        if shown.is_twitch || shown.is_obs {
            bridges.push(Self::spawn_descriptor_reload_watch(event_bus, cx));
        }
        if shown.is_obs {
            bridges.push(Self::spawn_obs_content_watch(event_bus, cx));
        }
        if shown.is_twitch || matches!(ports.status.id().as_str(), "youtube" | "kick") {
            bridges.push(Self::spawn_viewer_sampler(live_viewers, cx));
            bridges.push(Self::spawn_detail_ticker(cx));
        }
        bridges
    }
}

impl Render for IntegrationDetail {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let body = match &self.sign_in.connect {
            Some(connect) => {
                let twitch_bot = self
                    .sign_in
                    .twitch_bot
                    .clone()
                    .filter(|section| section.read(cx).has_bot_account(cx));
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(scaled(px(14.0), density))
                    .child(connect.view.clone())
                    .children(twitch_bot)
                    .into_any_element()
            }
            None => {
                let header_card = self.header_card(&palette, density, cx);
                let reconnecting = matches!(
                    self.shown.connection,
                    ConnectionState::Connecting | ConnectionState::Reconnecting
                );
                let state_banner = self.state_banner(&palette, density);
                let reauth_banner = (self.shown.is_twitch && self.shown.twitch_reauth_required)
                    .then(|| self.twitch_reauth_banner(&palette, density, cx));
                let health = health_grid(&self.augmented_health(), reconnecting, &palette, density);
                let refresh: SectionRefresh =
                    Rc::new(cx.listener(|this, _: &ClickEvent, _, cx| this.resync_content(cx)));
                let run_step: SectionStepRun = Rc::new(
                    cx.listener(|this, click: &StepClick, _, cx| this.run_row_step(click, cx)),
                );
                let hooks = SectionHooks {
                    refresh: Some(refresh),
                    run_step: Some(run_step),
                };
                let content =
                    content_sections(&self.augmented_sections(), &hooks, &palette, density);
                let quick = self.quick_actions_card(&palette, density, cx);
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
                    .gap(scaled(px(14.0), density))
                    .children(reauth_banner)
                    .children(state_banner)
                    .child(header_card)
                    .child(health)
                    .child(content)
                    .children(twitch_bot)
                    .children(donation_settings)
                    .child(quick)
                    .into_any_element()
            }
        };

        let scroll = div()
            .id("integration-detail-scroll")
            .flex_1()
            .overflow_y_revealing_focus()
            .bg(palette.base)
            .child(
                div()
                    .w_full()
                    .py(scaled(px(18.0), density))
                    .px(scaled(px(22.0), density))
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

        let is_oauth_platform = self.is_oauth_platform();
        let header_right = match &self.sign_in.connect {
            Some(connect) => Some(connect.view.read(cx).status_indicator(&palette, density)),
            None if is_oauth_platform && self.shown.connection == ConnectionState::Connected => {
                Some(self.header_right_connected(&palette, density))
            }
            None if is_oauth_platform => None,
            None => Some(self.header_right_connection(&palette, density)),
        };
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

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(frame)
            .children(disconnect_overlay)
            .children(donation_remove_confirm)
            .children(self.modals.quick_action_modal.clone())
            .children(self.modals.collection_manager.clone())
            .children(self.modals.obs_settings_modal.clone())
            .children(self.modals.history_modal.clone())
    }
}

fn platform_of(id: &str) -> Option<PlatformId> {
    match id {
        "twitch" => Some(PlatformId::Twitch),
        "youtube" => Some(PlatformId::YouTube),
        "kick" => Some(PlatformId::Kick),
        _ => None,
    }
}

fn connect_platform_for(id: &str, has_control: bool) -> Option<PlatformId> {
    if has_control {
        return None;
    }
    platform_of(id)
}
