use std::collections::HashMap;
use std::sync::Arc;

use forge_components::{
    BadgeMarker, BreadcrumbCrumb, CompactTone, ForgePalette, HUB_TILE_SIZE, Icon, body_family,
    compact_button, hub_card, hub_note, hub_section_header, hub_tile, icon, lifecycle_badge,
    mono_family, page_frame, toggle, tr,
};
use forge_platform_core::{ConnectionAffordance, IntegrationCategory, IntegrationDeclaration};
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_storage::{DataProvider, StorageError};
use forge_types::{Action, IntegrationId, TriggerInstance, TriggerInstanceId};
use gpui::{
    AnyElement, ClickEvent, Context, ElementId, Entity, EventEmitter, FontWeight, Pixels,
    SharedString, Subscription, Window, div, prelude::*, px,
};

use crate::home_stats::Integration;
use crate::integration_catalog::{
    CoreFeature, activity_key, core_features, core_tint, declaration_of, declarations,
    disclaimer_of, look_of,
};
use crate::integration_disable_modal::{DisablePrompt, render_disable_modal};
use crate::integration_lifecycle::{CardStatus, IntegrationLifecycle};
use crate::integration_references::{IntegrationReferences, tally_references};
use crate::integration_supervisor::{IntegrationSupervisor, LifecycleState};
use crate::integrations::BuiltinRegistry;
use crate::platforms::PlatformConnectivity;
use crate::presentation::ActivePresentation;
use crate::screen::Screen;
use crate::sidebar::NavRequested;

const BODY_PAD_V: Pixels = px(22.0);
const BODY_PAD_H: Pixels = px(28.0);
const INTRO_MB: Pixels = px(20.0);
const INTRO_TITLE: Pixels = px(16.0);
const INTRO_TITLE_MB: Pixels = px(4.0);
const INTRO_TEXT: Pixels = px(12.5);
const HEADER_COUNT: Pixels = px(11.0);
const SECTION_MB: Pixels = px(22.0);
const GRID_GAP: Pixels = px(10.0);
const GRID_COLUMNS: usize = 3;
const FOOTER_TEXT: Pixels = px(11.0);
const FOOTER_ICON: Pixels = px(11.0);
const FOOTER_GAP: Pixels = px(5.0);
const OPEN_GAP: Pixels = px(3.0);
const OPEN_CHEVRON: Pixels = px(12.0);

pub struct HubLaunch {
    pub supervisor: IntegrationSupervisor,
    pub builtins: BuiltinRegistry,
    pub backend: Arc<dyn DataProvider>,
    pub sub_actions: Arc<SubActionRegistry>,
    pub triggers: Arc<TriggerRegistry>,
    pub rt_handle: tokio::runtime::Handle,
}

pub struct IntegrationsHubView {
    focus: Option<IntegrationCategory>,
    lifecycle: Entity<IntegrationLifecycle>,
    connectivity: Entity<PlatformConnectivity>,
    launch: HubLaunch,
    references: HashMap<IntegrationId, IntegrationReferences>,
    activity: HashMap<IntegrationId, usize>,
    pending_disable: Option<IntegrationId>,
    _observers: [Subscription; 2],
}

impl EventEmitter<NavRequested> for IntegrationsHubView {}

impl IntegrationsHubView {
    pub fn new(
        focus: Option<IntegrationCategory>,
        lifecycle: Entity<IntegrationLifecycle>,
        connectivity: Entity<PlatformConnectivity>,
        launch: HubLaunch,
        cx: &mut Context<Self>,
    ) -> Self {
        let lifecycle_obs = cx.observe(&lifecycle, |this, _, cx| {
            this.refresh(cx);
            cx.notify();
        });
        let connectivity_obs = cx.observe(&connectivity, |_, _, cx| cx.notify());
        let mut view = Self {
            focus,
            lifecycle,
            connectivity,
            launch,
            references: HashMap::new(),
            activity: HashMap::new(),
            pending_disable: None,
            _observers: [lifecycle_obs, connectivity_obs],
        };
        view.refresh(cx);
        view
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let backend = Arc::clone(&self.launch.backend);
        let sub_actions = Arc::clone(&self.launch.sub_actions);
        let triggers = Arc::clone(&self.launch.triggers);
        let builtins = self.launch.builtins.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.launch.rt_handle.spawn(async move {
            let references = match wired_triggers(backend.as_ref()).await {
                Ok((actions, wired)) => {
                    Some(tally_references(&actions, &wired, &sub_actions, &triggers))
                }
                Err(e) => {
                    tracing::warn!(error = %e, "could not count integration references");
                    None
                }
            };
            let activity: HashMap<IntegrationId, usize> = builtins
                .snapshot()
                .into_iter()
                .filter_map(|object| {
                    let count = object.status.activity_count()?;
                    Some((object.status.id().clone(), count))
                })
                .collect();
            let _ = tx.send((references, activity));
        });
        cx.spawn(async move |this, cx| {
            let Ok((references, activity)) = rx.await else {
                return;
            };
            let _ = this.update(cx, |view, cx| {
                view.apply_refresh(references, activity);
                cx.notify();
            });
        })
        .detach();
    }

    pub fn apply_refresh(
        &mut self,
        references: Option<HashMap<IntegrationId, IntegrationReferences>>,
        activity: HashMap<IntegrationId, usize>,
    ) {
        if let Some(references) = references {
            self.references = references;
        }
        self.activity = activity;
    }

    pub fn references_of(&self, id: &IntegrationId) -> IntegrationReferences {
        self.references.get(id).cloned().unwrap_or_default()
    }

    pub fn request_enabled(&mut self, id: IntegrationId, enabled: bool, cx: &mut Context<Self>) {
        if !enabled && self.references_of(&id).total() > 0 {
            self.pending_disable = Some(id);
            cx.notify();
            return;
        }
        self.dispatch_enabled(&id, enabled);
    }

    pub fn confirm_disable(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.pending_disable.take() {
            self.dispatch_enabled(&id, false);
        }
        cx.notify();
    }

    pub fn cancel_disable(&mut self, cx: &mut Context<Self>) {
        if self.pending_disable.take().is_some() {
            cx.notify();
        }
    }

    fn retry(&mut self, id: &IntegrationId) {
        self.dispatch_enabled(id, true);
    }

    fn dispatch_enabled(&self, id: &IntegrationId, enabled: bool) {
        let Some(slot) = self.launch.supervisor.slot(id) else {
            return;
        };
        self.launch.rt_handle.spawn(async move {
            if let Err(e) = slot.set_enabled(enabled).await {
                tracing::warn!(integration = %slot.id(), error = %e, "could not change whether the integration is enabled");
            }
        });
    }

    fn set_connected(&mut self, id: &IntegrationId, connect: bool, cx: &mut Context<Self>) {
        let control = self
            .launch
            .builtins
            .get(id)
            .and_then(|object| object.control);
        let Some(control) = control else {
            self.go(Screen::BuiltinDetail(id.clone()), cx);
            return;
        };
        let id = id.clone();
        self.launch.rt_handle.spawn(async move {
            let outcome = if connect {
                control.reconnect().await
            } else {
                control.disconnect().await
            };
            if let Err(e) = outcome {
                tracing::warn!(integration = %id, error = %e, "integration connection change failed");
            }
        });
    }

    fn go(&mut self, screen: Screen, cx: &mut Context<Self>) {
        cx.emit(NavRequested(screen));
    }

    fn visible_categories(&self) -> Vec<IntegrationCategory> {
        IntegrationCategory::DISPLAY_ORDER
            .iter()
            .copied()
            .filter(|category| self.focus.is_none_or(|focus| focus == *category))
            .collect()
    }

    fn hub_declarations(&self, cx: &Context<Self>) -> Vec<IntegrationDeclaration> {
        let lifecycle = self.lifecycle.read(cx);
        declarations()
            .into_iter()
            .filter(|declaration| lifecycle.is_known(&declaration.id))
            .collect()
    }

    fn is_connected(&self, id: &IntegrationId, cx: &Context<Self>) -> bool {
        Integration::from_id(id.as_str())
            .is_some_and(|integ| self.connectivity.read(cx).is_connected(integ))
    }

    fn status_badge(
        &self,
        id: &IntegrationId,
        status: &CardStatus,
        palette: &ForgePalette,
    ) -> AnyElement {
        let spinner_id = ElementId::Name(format!("hub-badge-spin-{id}").into());
        let (marker, label, color) = match status {
            CardStatus::Disabled => (
                BadgeMarker::Dot(palette.text_extreme_faint),
                tr!("integration_status_disabled"),
                palette.text_faint,
            ),
            CardStatus::Starting => (
                BadgeMarker::Spinner(spinner_id),
                tr!("integration_status_starting"),
                palette.brand,
            ),
            CardStatus::Stopping => (
                BadgeMarker::Spinner(spinner_id),
                tr!("integration_status_stopping"),
                palette.text_muted,
            ),
            CardStatus::Failed(_) => (
                BadgeMarker::Glyph(Icon::AlertCircle),
                tr!("integration_status_failed"),
                palette.random,
            ),
            CardStatus::Active => (
                BadgeMarker::Dot(palette.success),
                tr!("integration_status_active"),
                palette.success,
            ),
            CardStatus::Connected => (
                BadgeMarker::Dot(palette.success),
                tr!("integration_status_connected"),
                palette.success,
            ),
            CardStatus::NotConnected => (
                BadgeMarker::Dot(palette.warning),
                tr!("integration_status_not_connected"),
                palette.warning,
            ),
        };
        lifecycle_badge(marker, label, color, palette).into_any_element()
    }

    fn reference_chip(
        &self,
        id: &IntegrationId,
        references: &IntegrationReferences,
        warn: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if references.total() == 0 {
            return div()
                .font_family(body_family())
                .text_size(FOOTER_TEXT)
                .italic()
                .text_color(palette.text_faint)
                .child(tr!("integration_refs_none"))
                .into_any_element();
        }
        let (glyph, ink) = if warn {
            (Icon::AlertTriangle, palette.warning)
        } else {
            (Icon::Link, palette.text_secondary)
        };
        let target = Screen::BuiltinDetail(id.clone());
        div()
            .id(ElementId::Name(format!("hub-refs-{id}").into()))
            .flex()
            .items_center()
            .gap(FOOTER_GAP)
            .cursor_pointer()
            .font_family(body_family())
            .text_size(FOOTER_TEXT)
            .text_color(ink)
            .child(icon(glyph, FOOTER_ICON, ink))
            .child(
                div()
                    .underline()
                    .text_decoration_color(palette.border_input)
                    .child(reference_summary(references)),
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.go(target.clone(), cx)))
            .into_any_element()
    }

    fn integration_card(
        &self,
        declaration: &IntegrationDeclaration,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = declaration.id.clone();
        let state = self.lifecycle.read(cx).state_of(&id);
        let connected = self.is_connected(&id, cx);
        let status = CardStatus::resolve(&state, declaration.connection, connected);
        let on = crate::integration_lifecycle::is_desired_on(&state);
        let look = look_of(&id, palette);
        let references = self.references_of(&id);

        let toggle_id = id.clone();
        let switch = toggle(on, palette).on_click(
            ElementId::Name(format!("hub-toggle-{id}").into()),
            cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.request_enabled(toggle_id.clone(), !on, cx)
            }),
        );

        let note = match &status {
            CardStatus::Failed(reason) => {
                let retry_id = id.clone();
                let retry = div()
                    .id(ElementId::Name(format!("hub-retry-{id}").into()))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(OPEN_GAP)
                    .cursor_pointer()
                    .font_family(body_family())
                    .text_size(FOOTER_TEXT)
                    .text_color(palette.brand)
                    .child(icon(Icon::Refresh, FOOTER_ICON, palette.brand))
                    .child(tr!("integration_retry"))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, _| this.retry(&retry_id)));
                Some(
                    hub_note(Icon::AlertCircle, palette.random, reason.clone())
                        .emphasized()
                        .tooltip(reason.clone())
                        .action(retry),
                )
            }
            _ => match disclaimer_of(&id) {
                Some(disclaimer) => Some(
                    hub_note(
                        Icon::AlertTriangle,
                        palette.warning,
                        tr!(disclaimer.short_key),
                    )
                    .tooltip(tr!(disclaimer.full_key)),
                ),
                None if status == CardStatus::Active => self.activity.get(&id).map(|count| {
                    hub_note(
                        Icon::CircleCheck,
                        palette.success,
                        tr!(&activity_key(&id), count = *count as i64),
                    )
                }),
                None => None,
            },
        };

        let mut card = hub_card(
            ElementId::Name(format!("hub-card-{id}").into()),
            declaration.brand_name,
            tr!(declaration.description_key),
            palette,
        )
        .muted(!on)
        .tile(hub_tile(
            look.tile_glyph(),
            look.tint,
            !on,
            HUB_TILE_SIZE,
            palette,
        ))
        .badge(self.status_badge(&id, &status, palette))
        .trailing(switch)
        .note(note)
        .footer_left(self.reference_chip(&id, &references, !on, palette, cx));

        let running = state == LifecycleState::Running;
        if running && declaration.connection == ConnectionAffordance::Connectable {
            let connect_id = id.clone();
            let button = if connected {
                compact_button(
                    ElementId::Name(format!("hub-disconnect-{id}").into()),
                    CompactTone::Ghost,
                    Icon::PlugOff,
                    palette,
                )
                .label(tr!("integration_disconnect"))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.set_connected(&connect_id, false, cx)
                }))
            } else {
                compact_button(
                    ElementId::Name(format!("hub-connect-{id}").into()),
                    CompactTone::Primary,
                    Icon::Plug,
                    palette,
                )
                .label(tr!("integration_connect"))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.set_connected(&connect_id, true, cx)
                }))
            };
            card = card.footer_action(button);
        }

        let settings_target = Screen::BuiltinDetail(id.clone());
        card =
            card.footer_action(
                compact_button(
                    ElementId::Name(format!("hub-settings-{id}").into()),
                    CompactTone::Ghost,
                    Icon::Settings,
                    palette,
                )
                .tooltip(tr!(
                    "integration_settings_tooltip",
                    name = declaration.brand_name
                ))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.go(settings_target.clone(), cx)
                })),
            );
        card.into_any_element()
    }

    fn core_card(
        &self,
        feature: &CoreFeature,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let target = feature.screen.clone();
        let open = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(OPEN_GAP)
            .font_family(body_family())
            .text_size(FOOTER_TEXT)
            .text_color(palette.text_secondary)
            .child(tr!("integration_core_open"))
            .child(icon(Icon::ChevronRight, OPEN_CHEVRON, palette.text_faint));
        hub_card(
            ElementId::Name(format!("hub-core-{}", feature.key).into()),
            tr!(feature.name_key),
            tr!(feature.description_key),
            palette,
        )
        .tile(hub_tile(
            forge_components::HubTileGlyph::Icon(feature.glyph),
            core_tint(feature.key, palette),
            false,
            HUB_TILE_SIZE,
            palette,
        ))
        .badge(lifecycle_badge(
            BadgeMarker::None,
            tr!("integration_status_built_in"),
            palette.text_muted,
            palette,
        ))
        .footer_left(
            div()
                .font_family(body_family())
                .text_size(FOOTER_TEXT)
                .text_color(palette.text_faint)
                .child(tr!("integration_core_always_on")),
        )
        .footer_action(open)
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.go(target.clone(), cx)))
        .into_any_element()
    }

    fn section(
        &self,
        category: IntegrationCategory,
        hub: &[IntegrationDeclaration],
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let members: Vec<&IntegrationDeclaration> = hub
            .iter()
            .filter(|declaration| declaration.category == category)
            .collect();
        let cores: Vec<CoreFeature> = core_features()
            .into_iter()
            .filter(|feature| feature.category == category)
            .collect();
        if members.is_empty() && cores.is_empty() {
            return None;
        }
        let count = (!members.is_empty()).then(|| {
            let on = self
                .lifecycle
                .read(cx)
                .on_count(members.iter().map(|declaration| &declaration.id));
            SharedString::from(tr!(
                "integration_category_on_count",
                on = on as i64,
                total = members.len() as i64
            ))
        });
        let mut cards: Vec<AnyElement> = members
            .iter()
            .map(|declaration| self.integration_card(declaration, palette, cx))
            .collect();
        cards.extend(
            cores
                .iter()
                .map(|feature| self.core_card(feature, palette, cx)),
        );
        Some(
            div()
                .mb(SECTION_MB)
                .child(hub_section_header(
                    tr!(category.label_key()),
                    tr!(category.blurb_key()),
                    count,
                    palette,
                ))
                .child(card_grid(cards))
                .into_any_element(),
        )
    }

    fn header_count(
        &self,
        hub: &[IntegrationDeclaration],
        palette: &ForgePalette,
        cx: &Context<Self>,
    ) -> AnyElement {
        let on = self
            .lifecycle
            .read(cx)
            .on_count(hub.iter().map(|declaration| &declaration.id));
        div()
            .flex()
            .items_center()
            .font_family(mono_family())
            .text_size(HEADER_COUNT)
            .text_color(palette.text_muted)
            .child(
                div()
                    .text_color(palette.text_primary)
                    .child(SharedString::from(on.to_string())),
            )
            .child(SharedString::from(tr!(
                "integration_enabled_of_total",
                total = hub.len() as i64
            )))
            .into_any_element()
    }

    fn crumbs(&self, cx: &mut Context<Self>) -> Vec<BreadcrumbCrumb> {
        match self.focus {
            None => vec![BreadcrumbCrumb::leaf(tr!("integrations_breadcrumb"))],
            Some(category) => vec![
                BreadcrumbCrumb::link(
                    tr!("integrations_breadcrumb"),
                    "integrations-crumb-hub",
                    cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.go(Screen::Integrations(None), cx)
                    }),
                ),
                BreadcrumbCrumb::leaf(tr!(category.label_key())),
            ],
        }
    }

    fn disable_prompt(&self) -> Option<DisablePrompt> {
        let id = self.pending_disable.as_ref()?;
        let declaration = declaration_of(id)?;
        Some(DisablePrompt {
            name: declaration.brand_name,
            references: self.references_of(id),
        })
    }
}

async fn wired_triggers(
    backend: &dyn DataProvider,
) -> Result<(Vec<Action>, Vec<TriggerInstance>), StorageError> {
    let actions = backend.action_repo().list().await?;
    let instances = backend.trigger_instance_repo();
    let mut wired: HashMap<TriggerInstanceId, TriggerInstance> = HashMap::new();
    for action in &actions {
        for instance in instances.list_for_action(action.id).await? {
            wired.entry(instance.id).or_insert(instance);
        }
    }
    Ok((actions, wired.into_values().collect()))
}

fn reference_summary(references: &IntegrationReferences) -> String {
    let mut parts = Vec::new();
    if !references.actions.is_empty() {
        parts.push(tr!(
            "integration_refs_actions",
            count = references.actions.len() as i64
        ));
    }
    if references.triggers > 0 {
        parts.push(tr!(
            "integration_refs_triggers",
            count = references.triggers as i64
        ));
    }
    tr!("integration_refs_used_by", parts = parts.join(" · "))
}

fn card_grid(cards: Vec<AnyElement>) -> impl IntoElement {
    let mut grid = div().w_full().flex().flex_col().gap(GRID_GAP);
    let mut cards = cards.into_iter().peekable();
    while cards.peek().is_some() {
        let mut row = div().w_full().flex().flex_row().gap(GRID_GAP);
        for _ in 0..GRID_COLUMNS {
            let cell = div().flex_1().min_w_0();
            row = row.child(match cards.next() {
                Some(card) => cell.child(card),
                None => cell,
            });
        }
        grid = grid.child(row);
    }
    grid
}

impl Render for IntegrationsHubView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let hub = self.hub_declarations(cx);

        let intro = div()
            .mb(INTRO_MB)
            .child(
                div()
                    .mb(INTRO_TITLE_MB)
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(INTRO_TITLE)
                    .text_color(palette.text_primary)
                    .child(tr!("integrations_title")),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(INTRO_TEXT)
                    .text_color(palette.text_muted)
                    .child(tr!("integrations_subtitle")),
            );

        let sections: Vec<AnyElement> = self
            .visible_categories()
            .into_iter()
            .filter_map(|category| self.section(category, &hub, &palette, cx))
            .collect();

        let scroll = div()
            .id("integrations-hub-scroll")
            .flex_1()
            .overflow_y_scroll()
            .bg(palette.base)
            .child(
                div()
                    .w_full()
                    .py(BODY_PAD_V)
                    .px(BODY_PAD_H)
                    .child(intro)
                    .children(sections),
            );

        let modal = self
            .disable_prompt()
            .map(|prompt| render_disable_modal(&prompt, &palette, cx));

        let crumbs = self.crumbs(cx);
        let header_count = self.header_count(&hub, &palette, cx);
        div()
            .size_full()
            .relative()
            .child(
                page_frame(crumbs, &palette)
                    .header_right(header_count)
                    .body(scroll),
            )
            .children(modal)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use std::collections::HashSet;
    use std::time::Duration;

    use async_trait::async_trait;
    use forge_runtime::{IntegrationGate, spawn_live_viewer_aggregator};
    use forge_storage::{SettingsRepo, integration_enabled_key};
    use forge_types::{ActionId, QueueId, SubActionStep};
    use gpui::TestAppContext;
    use tokio::sync::mpsc::UnboundedReceiver;

    use super::*;
    use crate::integration_supervisor::{IntegrationFactory, LifecycleStates, RunningIntegration};
    use crate::test_support::{
        Sandboxed, SettingWrite, action_running, owned_sub_actions, owned_triggers, pump, runtime,
        sandboxed_backend, step, test_backend, trigger_of,
    };

    const TEST_KEY: [u8; 32] = [0x11; 32];
    const OBS_SCENE: &str = "obs.scene.set";
    const TWITCH_CHAT: &str = "twitch.chat";
    const SETTLE_ROUNDS: usize = 500;

    fn twitch() -> IntegrationId {
        IntegrationId::new("twitch")
    }

    fn obs() -> IntegrationId {
        IntegrationId::new("obs")
    }

    fn kick() -> IntegrationId {
        IntegrationId::new("kick")
    }

    struct IdleFactory(IntegrationId);

    #[async_trait]
    impl IntegrationFactory for IdleFactory {
        fn id(&self) -> IntegrationId {
            self.0.clone()
        }

        async fn is_configured(&self) -> Result<bool, StorageError> {
            Ok(true)
        }

        async fn start(&self) -> Result<RunningIntegration, String> {
            Ok(RunningIntegration::idle())
        }
    }

    async fn provider() -> Sandboxed<Arc<dyn DataProvider>> {
        sandboxed_backend("sqlite::memory:", TEST_KEY)
            .await
            .map(|backend| Arc::new(backend) as Arc<dyn DataProvider>)
    }

    async fn default_queue(backend: &Arc<dyn DataProvider>) -> QueueId {
        backend
            .queue_repo()
            .get_by_name("Default")
            .await
            .unwrap()
            .expect("migrations seed the default queue")
            .id
    }

    async fn seed_action(
        backend: &Arc<dyn DataProvider>,
        name: &str,
        steps: Vec<SubActionStep>,
    ) -> ActionId {
        let action = Action {
            queue_id: default_queue(backend).await,
            ..action_running(name, steps)
        };
        backend.action_repo().save(&action).await.unwrap();
        action.id
    }

    async fn seed_trigger(backend: &Arc<dyn DataProvider>, kind_id: &str) -> TriggerInstanceId {
        let instance = trigger_of(kind_id);
        backend
            .trigger_instance_repo()
            .save(&instance)
            .await
            .unwrap();
        instance.id
    }

    async fn link(backend: &Arc<dyn DataProvider>, action: ActionId, trigger: TriggerInstanceId) {
        backend
            .trigger_instance_repo()
            .link_action(action, trigger, 0)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn only_triggers_wired_to_an_action_are_counted_each_once() {
        let storage = provider().await;
        let first = seed_action(&storage, "Scene", Vec::new()).await;
        let second = seed_action(&storage, "Alert", Vec::new()).await;
        let shared = seed_trigger(&storage, TWITCH_CHAT).await;
        let single = seed_trigger(&storage, TWITCH_CHAT).await;
        seed_trigger(&storage, TWITCH_CHAT).await;
        link(&storage, first, shared).await;
        link(&storage, second, shared).await;
        link(&storage, second, single).await;

        let (_, wired) = wired_triggers(storage.as_ref()).await.unwrap();

        let ids: Vec<TriggerInstanceId> = wired.iter().map(|instance| instance.id).collect();
        assert_eq!(ids.len(), 2);
        assert_eq!(
            ids.into_iter().collect::<HashSet<_>>(),
            HashSet::from([shared, single])
        );
    }

    struct Hub {
        view: Entity<IntegrationsHubView>,
        writes: UnboundedReceiver<SettingWrite>,
        _storage: Sandboxed<Arc<dyn DataProvider>>,
    }

    impl Hub {
        fn drain_writes(&mut self, rt: &tokio::runtime::Runtime) -> Vec<SettingWrite> {
            pump(rt);
            std::iter::from_fn(|| self.writes.try_recv().ok()).collect()
        }

        fn pending(&self, cx: &mut TestAppContext) -> Option<IntegrationId> {
            self.view
                .read_with(cx, |view, _| view.pending_disable.clone())
        }
    }

    fn enabled_write(id: &IntegrationId, enabled: bool) -> SettingWrite {
        (integration_enabled_key(id), enabled.to_string())
    }

    fn mount(cx: &mut TestAppContext, rt: &tokio::runtime::Runtime) -> Hub {
        let storage = rt.block_on(async {
            let storage = provider().await;
            let scene = seed_action(&storage, "Scene", vec![step(OBS_SCENE, true)]).await;
            let chat = seed_trigger(&storage, TWITCH_CHAT).await;
            link(&storage, scene, chat).await;
            storage
        });
        let (settings, writes) = test_backend();
        let supervisor = {
            let _entered = rt.enter();
            IntegrationSupervisor::launch(
                [twitch(), obs(), kick()]
                    .into_iter()
                    .map(|id| Arc::new(IdleFactory(id)) as Arc<dyn IntegrationFactory>)
                    .collect(),
                settings as Arc<dyn SettingsRepo>,
                IntegrationGate::new(),
                BuiltinRegistry::default(),
                spawn_live_viewer_aggregator(),
            )
        };
        let launch = HubLaunch {
            supervisor,
            builtins: BuiltinRegistry::default(),
            backend: Arc::clone(&storage),
            sub_actions: Arc::new(owned_sub_actions(&[(OBS_SCENE, "obs")])),
            triggers: Arc::new(owned_triggers(&[(TWITCH_CHAT, "twitch")])),
            rt_handle: rt.handle().clone(),
        };
        let view = cx.update(|cx| {
            let lifecycle = cx.new(|_| IntegrationLifecycle::new(LifecycleStates::new()));
            let connectivity = cx.new(|_| PlatformConnectivity::new());
            cx.new(|cx| IntegrationsHubView::new(None, lifecycle, connectivity, launch, cx))
        });
        let mut hub = Hub {
            view,
            writes,
            _storage: storage,
        };
        let counted = (0..SETTLE_ROUNDS).any(|_| {
            rt.block_on(async { tokio::time::sleep(Duration::from_millis(1)).await });
            cx.run_until_parked();
            hub.view
                .read_with(cx, |view, _| view.references_of(&obs()).total() > 0)
        });
        assert!(counted, "the hub never counted the seeded references");
        hub.drain_writes(rt);
        hub
    }

    #[gpui::test]
    fn disabling_an_integration_something_references_waits_for_confirmation(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let mut hub = mount(cx, &rt);

        for (id, case) in [
            (twitch(), "referenced only by a wired trigger"),
            (obs(), "referenced only by an action step"),
        ] {
            hub.view
                .update(cx, |view, cx| view.request_enabled(id.clone(), false, cx));

            assert_eq!(hub.pending(cx), Some(id), "{case}");
            assert_eq!(hub.drain_writes(&rt), Vec::new(), "{case}");
            hub.view.update(cx, |view, cx| view.cancel_disable(cx));
        }
    }

    #[gpui::test]
    fn disabling_an_unreferenced_integration_applies_at_once(cx: &mut TestAppContext) {
        let rt = runtime();
        let mut hub = mount(cx, &rt);

        hub.view
            .update(cx, |view, cx| view.request_enabled(kick(), false, cx));

        assert_eq!(hub.pending(cx), None);
        assert_eq!(hub.drain_writes(&rt), vec![enabled_write(&kick(), false)]);
    }

    #[gpui::test]
    fn confirming_the_prompt_disables_the_integration_and_closes_it(cx: &mut TestAppContext) {
        let rt = runtime();
        let mut hub = mount(cx, &rt);
        hub.view
            .update(cx, |view, cx| view.request_enabled(twitch(), false, cx));

        hub.view.update(cx, |view, cx| view.confirm_disable(cx));

        assert_eq!(hub.pending(cx), None);
        assert_eq!(hub.drain_writes(&rt), vec![enabled_write(&twitch(), false)]);
    }

    #[gpui::test]
    fn cancelling_the_prompt_keeps_the_integration_enabled(cx: &mut TestAppContext) {
        let rt = runtime();
        let mut hub = mount(cx, &rt);
        hub.view
            .update(cx, |view, cx| view.request_enabled(twitch(), false, cx));

        hub.view.update(cx, |view, cx| view.cancel_disable(cx));

        assert_eq!(hub.pending(cx), None);
        assert_eq!(hub.drain_writes(&rt), Vec::new());
    }

    #[gpui::test]
    fn confirming_with_no_prompt_open_disables_nothing(cx: &mut TestAppContext) {
        let rt = runtime();
        let mut hub = mount(cx, &rt);

        hub.view.update(cx, |view, cx| view.confirm_disable(cx));

        assert_eq!(hub.drain_writes(&rt), Vec::new());
    }

    #[gpui::test]
    fn enabling_a_referenced_integration_never_asks(cx: &mut TestAppContext) {
        let rt = runtime();
        let mut hub = mount(cx, &rt);

        hub.view
            .update(cx, |view, cx| view.request_enabled(obs(), true, cx));

        assert_eq!(hub.pending(cx), None);
        assert_eq!(hub.drain_writes(&rt), vec![enabled_write(&obs(), true)]);
    }
}
