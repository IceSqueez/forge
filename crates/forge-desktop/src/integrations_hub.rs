use forge_components::Density;
use forge_components::FONT_MD;
use forge_components::FONT_META;
use forge_components::FONT_ROW;
use forge_components::Spacing;
use forge_components::spacing;
use std::collections::HashMap;
use std::sync::Arc;

use crate::hub_card::{
    BadgeMarker, CompactTone, HUB_TILE_SIZE, HubTileGlyph, compact_button, hub_card, hub_note,
    hub_section_header, hub_tile, lifecycle_badge,
};

use forge_components::{
    BreadcrumbCrumb, ForgePalette, Icon, ToastKind, body_family, icon, mono_family, page_frame,
    scaled, toggle, tr,
};
use forge_platform_core::{
    ConnectionAffordance, ControlFailure, ControlOutcome, IntegrationCategory,
    IntegrationDeclaration,
};
use forge_registry::{IntegrationReferences, SubActionRegistry, TriggerRegistry, tally_references};
use forge_storage::{DataProvider, StorageError};
use forge_types::{Action, IntegrationId, MessageRef, TriggerInstance, TriggerInstanceId};
use gpui::{
    AnyElement, ClickEvent, Context, ElementId, Entity, EventEmitter, FontWeight, Pixels,
    SharedString, Subscription, Window, div, prelude::*, px,
};

use crate::async_bridge::{self, Generation};
use crate::integration_catalog::{
    CoreFeature, activity_key, core_features, core_tint, declaration_of, declarations,
    disclaimer_of, look_of,
};
use crate::integration_disable_modal::{DisablePrompt, render_disable_modal};
use crate::integration_failed::LiveDetailRequested;
use crate::integration_lifecycle::{CardStatus, IntegrationLifecycle};
use crate::integration_switch::category_members;
use crate::integrations::{BuiltinObject, BuiltinRegistry, ObsInstallSeed, VTubeInstallSeed};
use crate::navigation::NavRequested;
use crate::navigation::Screen;
use crate::platforms::PlatformConnectivity;
use crate::start_failure::{StartRemedy, start_failure_text, start_failure_tooltip};
use crate::user_text::resolve_message;
use forge_components::ActivePresentation;
use forge_components::PushToast;
use forge_runtime::{IntegrationSupervisor, LifecycleState};

mod welcome;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConnectFollowUp {
    Done,
    OpenSignIn,
    Report,
}

impl ConnectFollowUp {
    pub(crate) fn resolve(connect: bool, outcome: &ControlOutcome) -> Self {
        match outcome {
            Ok(()) => Self::Done,
            Err(ControlFailure::NotConnected) if connect => Self::OpenSignIn,
            Err(ControlFailure::NotConnected) => Self::Done,
            Err(_) => Self::Report,
        }
    }
}

const BODY_PAD_V: Pixels = px(22.0);
const BODY_PAD_H: Pixels = px(28.0);
const INTRO_MB: Pixels = px(20.0);
const INTRO_TITLE: Pixels = FONT_MD;
const INTRO_TEXT: Pixels = FONT_ROW;
const HEADER_COUNT: Pixels = FONT_META;
const SECTION_MB: Pixels = px(22.0);
const GRID_COLUMNS: usize = 3;
const FOOTER_TEXT: Pixels = FONT_META;
const FOOTER_ICON: Pixels = px(11.0);
const FOOTER_GAP: Pixels = px(5.0);
const OPEN_GAP: Pixels = px(3.0);
const OPEN_CHEVRON: Pixels = px(12.0);

pub(crate) struct HubLaunch {
    pub supervisor: IntegrationSupervisor<BuiltinObject>,
    pub builtins: BuiltinRegistry,
    pub backend: Arc<dyn DataProvider>,
    pub sub_actions: Arc<SubActionRegistry>,
    pub triggers: Arc<TriggerRegistry>,
    pub rt_handle: tokio::runtime::Handle,
    pub obs_install_seed: ObsInstallSeed,
    pub vtube_install_seed: VTubeInstallSeed,
}

pub(crate) struct IntegrationsHubView {
    focus: Option<IntegrationCategory>,
    lifecycle: Entity<IntegrationLifecycle>,
    connectivity: Entity<PlatformConnectivity>,
    launch: HubLaunch,
    references: HashMap<IntegrationId, IntegrationReferences>,
    references_loaded: bool,
    refresh_gen: Generation,
    activity: HashMap<IntegrationId, usize>,
    pending_disable: Option<IntegrationId>,
    welcome: bool,
    _observers: [Subscription; 2],
}

impl EventEmitter<NavRequested> for IntegrationsHubView {}

impl EventEmitter<LiveDetailRequested> for IntegrationsHubView {}

impl IntegrationsHubView {
    pub(crate) fn new(
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
            references_loaded: false,
            refresh_gen: Generation::default(),
            activity: HashMap::new(),
            pending_disable: None,
            welcome: false,
            _observers: [lifecycle_obs, connectivity_obs],
        };
        view.refresh(cx);
        view
    }

    pub(crate) fn welcome(mut self) -> Self {
        self.welcome = true;
        self
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let ticket = self.refresh_gen.next();
        let backend = Arc::clone(&self.launch.backend);
        let sub_actions = Arc::clone(&self.launch.sub_actions);
        let triggers = Arc::clone(&self.launch.triggers);
        let builtins = self.launch.builtins.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.launch.rt_handle.spawn(async move {
            let references = match load_references(backend.as_ref(), &sub_actions, &triggers).await
            {
                Ok(references) => Some(references),
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
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the view may close before the result arrives"
            )]
            let _ = tx.send((references, activity));
        });
        cx.spawn(async move |this, cx| {
            let Ok((references, activity)) = rx.await else {
                return;
            };
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the view may close before the result arrives"
            )]
            let _ = this.update(cx, |view, cx| {
                if view.refresh_gen.is_current(ticket) {
                    view.apply_refresh(references, activity);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(crate) fn apply_refresh(
        &mut self,
        references: Option<HashMap<IntegrationId, IntegrationReferences>>,
        activity: HashMap<IntegrationId, usize>,
    ) {
        if let Some(references) = references {
            self.references = references;
            self.references_loaded = true;
        }
        self.activity = activity;
    }

    pub(crate) fn references_of(&self, id: &IntegrationId) -> IntegrationReferences {
        self.references.get(id).cloned().unwrap_or_default()
    }

    pub(crate) fn request_enabled(
        &mut self,
        id: IntegrationId,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        let may_be_referenced = !self.references_loaded || self.references_of(&id).total() > 0;
        if !enabled && !self.welcome && may_be_referenced {
            self.pending_disable = Some(id);
            cx.notify();
            return;
        }
        self.dispatch_enabled(&id, enabled);
    }

    pub(crate) fn confirm_disable(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.pending_disable.take() {
            self.dispatch_enabled(&id, false);
        }
        cx.notify();
    }

    pub(crate) fn cancel_disable(&mut self, cx: &mut Context<Self>) {
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
        async_bridge::run_async(
            &self.launch.rt_handle,
            async move {
                if connect {
                    control.reconnect().await
                } else {
                    control.disconnect().await
                }
            },
            move |this, outcome, cx| this.apply_connect_outcome(id, connect, outcome, cx),
            cx,
        );
    }

    pub(crate) fn apply_connect_outcome(
        &mut self,
        id: IntegrationId,
        connect: bool,
        outcome: ControlOutcome,
        cx: &mut Context<Self>,
    ) {
        match ConnectFollowUp::resolve(connect, &outcome) {
            ConnectFollowUp::Done => {}
            ConnectFollowUp::OpenSignIn => self.go(Screen::BuiltinDetail(id), cx),
            ConnectFollowUp::Report => {
                if let Err(e) = outcome {
                    tracing::warn!(integration = %id, error = %e, "integration connection change failed");
                }
                let name = declaration_of(&id)
                    .map(|declaration| declaration.brand_name)
                    .unwrap_or(id.as_str())
                    .to_owned();
                let message = if connect {
                    tr!("integration_connect_failed", name = name)
                } else {
                    tr!("integration_disconnect_failed", name = name)
                };
                cx.push_toast(ToastKind::Error, message);
            }
        }
    }

    fn apply_remedy(&mut self, id: &IntegrationId, remedy: StartRemedy, cx: &mut Context<Self>) {
        if remedy.opens_detail() {
            cx.emit(LiveDetailRequested(id.clone()));
        } else {
            self.retry(id);
        }
    }

    fn remedy_link(
        &self,
        id: &IntegrationId,
        remedy: StartRemedy,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let remedy_id = id.clone();
        div()
            .id(ElementId::Name(format!("hub-remedy-{id}").into()))
            .flex_none()
            .flex()
            .items_center()
            .gap(scaled(OPEN_GAP, density))
            .cursor_pointer()
            .font_family(body_family())
            .text_size(FOOTER_TEXT)
            .text_color(palette.brand)
            .child(icon(remedy.icon(), FOOTER_ICON, palette.brand))
            .child(remedy.label())
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.apply_remedy(&remedy_id, remedy, cx)
            }))
            .into_any_element()
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
        self.connectivity.read(cx).is_integration_connected(id)
    }

    fn connect_failure_of(&self, id: &IntegrationId) -> Option<MessageRef> {
        if *id == forge_obs::OBS_INTEGRATION.id {
            self.launch.obs_install_seed.connect_failure()
        } else if *id == forge_vtube::VTUBE_INTEGRATION.id {
            self.launch.vtube_install_seed.connect_failure()
        } else {
            None
        }
    }

    fn reference_chip(
        &self,
        id: &IntegrationId,
        references: &IntegrationReferences,
        warn: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
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
            .gap(scaled(FOOTER_GAP, density))
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
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.go(target.clone(), cx)
            }))
            .into_any_element()
    }

    fn integration_card(
        &self,
        declaration: &IntegrationDeclaration,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
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
                cx.stop_propagation();
                this.request_enabled(toggle_id.clone(), !on, cx)
            }),
        );

        let disclaimer = disclaimer_of(&id);
        let note = match (&status, &disclaimer) {
            (CardStatus::Failed(error), None) => Some(
                hub_note(
                    Icon::AlertCircle,
                    palette.danger,
                    start_failure_text(error).primary,
                )
                .emphasized()
                .tooltip(start_failure_tooltip(error))
                .action(self.remedy_link(&id, StartRemedy::of(error), palette, cx)),
            ),
            (CardStatus::NotConnected, None) => self.connect_failure_of(&id).map(|failure| {
                let text = resolve_message(&failure);
                hub_note(Icon::AlertTriangle, palette.warning, text.primary.clone())
                    .tooltip(text.inline())
            }),
            (_, Some(disclaimer)) => Some(
                hub_note(
                    Icon::AlertTriangle,
                    palette.warning,
                    tr!(disclaimer.short_key),
                )
                .tooltip(tr!(disclaimer.full_key)),
            ),
            (CardStatus::Active, None) => self.activity.get(&id).map(|count| {
                hub_note(
                    Icon::CircleCheck,
                    palette.success,
                    tr!(
                        &activity_key(&id),
                        count = i64::try_from(*count).unwrap_or(i64::MAX)
                    ),
                )
            }),
            _ => None,
        };
        let footer_retry = match (&status, &disclaimer) {
            (CardStatus::Failed(error), Some(_)) => {
                let remedy = StartRemedy::of(error);
                let remedy_id = id.clone();
                Some(
                    compact_button(
                        ElementId::Name(format!("hub-remedy-{id}").into()),
                        CompactTone::Primary,
                        remedy.icon(),
                        palette,
                    )
                    .label(remedy.label())
                    .tooltip(start_failure_tooltip(error))
                    .on_click(cx.listener(
                        move |this, _: &ClickEvent, _, cx| {
                            this.apply_remedy(&remedy_id, remedy, cx)
                        },
                    )),
                )
            }
            _ => None,
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
        .badge(card_status_badge(&id, &status, palette, density))
        .trailing(switch)
        .note(note);
        if !self.welcome {
            let detail_target = Screen::BuiltinDetail(id.clone());
            card =
                card.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.go(detail_target.clone(), cx)
                }));
        }
        card = if self.welcome {
            card.footer_left(welcome_footer_note(&status, palette))
        } else {
            card.footer_left(self.reference_chip(&id, &references, !on, palette, cx))
        };

        if let Some(retry) = footer_retry {
            card = card.footer_action(retry);
        }

        let running = state == LifecycleState::Running;
        let connectable = running && declaration.connection == ConnectionAffordance::Connectable;
        if connectable && !(self.welcome && connected) {
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

        if self.welcome {
            return card.into_any_element();
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
        let density = cx.density();
        let target = feature.screen.clone();
        let open = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(scaled(OPEN_GAP, density))
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
            HubTileGlyph::Icon(feature.glyph),
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
            density,
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
        let density = cx.density();
        let members: Vec<&IntegrationDeclaration> = hub
            .iter()
            .filter(|declaration| declaration.category == category)
            .collect();
        let cores: Vec<CoreFeature> = core_features()
            .into_iter()
            .filter(|feature| !self.welcome && feature.category == category)
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
                on = i64::try_from(on).unwrap_or(i64::MAX),
                total = i64::try_from(members.len()).unwrap_or(i64::MAX)
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
                    density,
                ))
                .child(card_grid(cards, density))
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
                total = i64::try_from(hub.len()).unwrap_or(i64::MAX)
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
            references_known: self.references_loaded,
        })
    }
}

pub(crate) async fn load_references(
    backend: &dyn DataProvider,
    sub_actions: &SubActionRegistry,
    triggers: &TriggerRegistry,
) -> Result<HashMap<IntegrationId, IntegrationReferences>, StorageError> {
    let (actions, wired) = wired_triggers(backend).await?;
    Ok(tally_references(
        &actions,
        &wired,
        sub_actions,
        triggers,
        &category_members,
    ))
}

pub(crate) fn card_status_badge(
    id: &IntegrationId,
    status: &CardStatus,
    palette: &ForgePalette,
    density: Density,
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
            palette.danger,
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
    lifecycle_badge(marker, label, color, palette, density).into_any_element()
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

fn welcome_footer_note(status: &CardStatus, palette: &ForgePalette) -> AnyElement {
    let label = match status {
        CardStatus::Disabled => tr!("integration_welcome_off"),
        CardStatus::Active | CardStatus::Connected | CardStatus::NotConnected => {
            tr!("integration_welcome_on")
        }
        CardStatus::Starting => tr!("integration_status_starting"),
        CardStatus::Stopping => tr!("integration_status_stopping"),
        CardStatus::Failed(_) => tr!("integration_status_failed"),
    };
    div()
        .font_family(body_family())
        .text_size(FOOTER_TEXT)
        .text_color(palette.text_faint)
        .child(label)
        .into_any_element()
}

fn reference_summary(references: &IntegrationReferences) -> String {
    let mut parts = Vec::new();
    if !references.actions.is_empty() {
        parts.push(tr!(
            "integration_refs_actions",
            count = i64::try_from(references.actions.len()).unwrap_or(i64::MAX)
        ));
    }
    if references.triggers > 0 {
        parts.push(tr!(
            "integration_refs_triggers",
            count = i64::try_from(references.triggers).unwrap_or(i64::MAX)
        ));
    }
    tr!("integration_refs_used_by", parts = parts.join(" · "))
}

fn card_grid(cards: Vec<AnyElement>, density: Density) -> impl IntoElement {
    let mut grid = div()
        .w_full()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Sm, density));
    let mut cards = cards.into_iter().peekable();
    while cards.peek().is_some() {
        let mut row = div()
            .w_full()
            .flex()
            .flex_row()
            .gap(spacing(Spacing::Sm, density));
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
        let density = cx.density();
        let palette = cx.palette();
        let hub = self.hub_declarations(cx);
        if self.welcome {
            return self.render_welcome(&hub, &palette, cx);
        }

        let intro = div()
            .mb(INTRO_MB)
            .child(
                div()
                    .mb(spacing(Spacing::Xxs, density))
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
                    .py(scaled(BODY_PAD_V, density))
                    .px(scaled(BODY_PAD_H, density))
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
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use forge_storage::{
        MockActionRepo, MockTriggerInstanceRepo, SettingsRepo, integration_enabled_key,
    };
    use gpui::TestAppContext;
    use std::sync::Mutex;
    use tokio::sync::mpsc::UnboundedReceiver;

    use super::*;
    use crate::test_support::{
        ClickArea, Sandboxed, SettingWrite, action_running, alive_tasks, click_grid, error_toasts,
        idle_supervisor, install_presentation, link, nav_log, owned_sub_actions, owned_triggers,
        pump, pump_until, runtime, sandboxed_over, sandboxed_provider, seed_action, seed_trigger,
        settle_until, settle_until_tasks_drain_to, step, test_backend,
        test_backend_with_actions_and_triggers, trigger_of, yield_until,
    };
    use forge_components::Toasts;
    use forge_runtime::LifecycleStates;

    const OBS_SCENE: &str = "obs.scene.set";
    const TWITCH_CHAT: &str = "twitch.chat";

    fn twitch() -> IntegrationId {
        IntegrationId::new("twitch")
    }

    fn obs() -> IntegrationId {
        IntegrationId::new("obs")
    }

    fn kick() -> IntegrationId {
        IntegrationId::new("kick")
    }

    #[tokio::test]
    async fn only_triggers_wired_to_an_action_are_counted_each_once() {
        let storage = sandboxed_provider().await;
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
        mount_as(cx, rt, false)
    }

    fn mount_as(cx: &mut TestAppContext, rt: &tokio::runtime::Runtime, welcome: bool) -> Hub {
        let mut hub = open_uncounted(cx, rt, welcome);
        settle_until(cx, rt, |cx| {
            hub.view
                .read_with(cx, |view, _| view.references_of(&obs()).total() > 0)
        });
        hub.drain_writes(rt);
        hub
    }

    fn open_uncounted(cx: &mut TestAppContext, rt: &tokio::runtime::Runtime, welcome: bool) -> Hub {
        let storage = rt.block_on(async {
            let storage = sandboxed_provider().await;
            let scene = seed_action(&storage, "Scene", vec![step(OBS_SCENE, true)]).await;
            let chat = seed_trigger(&storage, TWITCH_CHAT).await;
            link(&storage, scene, chat).await;
            storage
        });
        open_over(cx, rt, welcome, storage)
    }

    fn open_over(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        welcome: bool,
        storage: Sandboxed<Arc<dyn DataProvider>>,
    ) -> Hub {
        let (settings, writes) = test_backend();
        let supervisor = idle_supervisor(
            rt,
            settings as Arc<dyn SettingsRepo>,
            &[twitch(), obs(), kick()],
        );
        let launch = HubLaunch {
            supervisor,
            builtins: BuiltinRegistry::default(),
            backend: Arc::clone(&storage),
            sub_actions: Arc::new(owned_sub_actions(&[(OBS_SCENE, "obs")])),
            triggers: Arc::new(owned_triggers(&[(TWITCH_CHAT, "twitch")])),
            rt_handle: rt.handle().clone(),
            obs_install_seed: ObsInstallSeed::new(forge_obs::SwitchableObsSink::new()),
            vtube_install_seed: VTubeInstallSeed::new(forge_vtube::SwitchableVTubeSink::new()),
        };
        let view = cx.update(|cx| {
            let lifecycle = cx.new(|_| IntegrationLifecycle::new(LifecycleStates::new()));
            let connectivity = cx.new(|_| PlatformConnectivity::new());
            cx.new(|cx| {
                let hub = IntegrationsHubView::new(None, lifecycle, connectivity, launch, cx);
                if welcome { hub.welcome() } else { hub }
            })
        });
        Hub {
            view,
            writes,
            _storage: storage,
        }
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
    fn disabling_before_the_references_are_counted_waits_for_confirmation(cx: &mut TestAppContext) {
        let rt = runtime();
        let mut hub = open_uncounted(cx, &rt, false);

        hub.view
            .update(cx, |view, cx| view.request_enabled(kick(), false, cx));

        assert_eq!(
            (hub.pending(cx), hub.drain_writes(&rt)),
            (Some(kick()), Vec::new())
        );
    }

    #[gpui::test]
    fn a_failed_count_leaves_disabling_behind_the_confirmation(cx: &mut TestAppContext) {
        let rt = runtime();
        let hub = open_uncounted(cx, &rt, false);

        hub.view.update(cx, |view, cx| {
            view.apply_refresh(None, HashMap::new());
            view.request_enabled(kick(), false, cx);
        });

        assert_eq!(hub.pending(cx), Some(kick()));
    }

    #[gpui::test]
    fn a_refresh_overtaken_by_a_newer_one_leaves_the_counts_alone(cx: &mut TestAppContext) {
        let rt = runtime();
        let actions = Arc::new(Mutex::new(vec![action_running(
            "Scene",
            vec![step(OBS_SCENE, true)],
        )]));
        let chat = trigger_of(TWITCH_CHAT);
        let mut action_repo = MockActionRepo::new();
        let listed = Arc::clone(&actions);
        action_repo
            .expect_list()
            .returning(move || Ok(listed.lock().unwrap().clone()));
        let mut trigger_repo = MockTriggerInstanceRepo::new();
        trigger_repo
            .expect_list_for_action()
            .returning(move |_| Ok(vec![chat.clone()]));
        let storage = sandboxed_over(test_backend_with_actions_and_triggers(
            Arc::new(action_repo),
            Arc::new(trigger_repo),
        ));
        let mut hub = open_over(cx, &rt, false, storage);
        settle_until(cx, &rt, |cx| {
            hub.view
                .read_with(cx, |view, _| view.references_of(&obs()).total() > 0)
        });
        hub.drain_writes(&rt);
        let counted = hub
            .view
            .read_with(cx, |view, _| view.references_of(&obs()).total());
        actions
            .lock()
            .unwrap()
            .push(action_running("Second scene", vec![step(OBS_SCENE, true)]));
        let background_tasks = alive_tasks(&rt);

        hub.view.update(cx, |view, cx| {
            view.refresh(cx);
            view.refresh_gen.next();
        });
        settle_until_tasks_drain_to(cx, &rt, background_tasks);

        assert_eq!(
            hub.view
                .read_with(cx, |view, _| view.references_of(&obs()).total()),
            counted
        );
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

    #[gpui::test]
    fn the_welcome_hub_disables_a_referenced_integration_without_asking(cx: &mut TestAppContext) {
        let rt = runtime();
        let mut hub = mount_as(cx, &rt, true);

        hub.view
            .update(cx, |view, cx| view.request_enabled(obs(), false, cx));

        assert_eq!(hub.pending(cx), None);
        assert_eq!(hub.drain_writes(&rt), vec![enabled_write(&obs(), false)]);
    }

    #[test]
    fn connect_follow_up_depends_on_the_direction_and_the_failure() {
        let cases = [
            (true, Ok(()), ConnectFollowUp::Done),
            (
                true,
                Err(ControlFailure::NotConnected),
                ConnectFollowUp::OpenSignIn,
            ),
            (
                true,
                Err(ControlFailure::Unauthorized),
                ConnectFollowUp::Report,
            ),
            (
                true,
                Err(ControlFailure::Transport),
                ConnectFollowUp::Report,
            ),
            (
                true,
                Err(ControlFailure::Unsupported),
                ConnectFollowUp::Report,
            ),
            (false, Ok(()), ConnectFollowUp::Done),
            (
                false,
                Err(ControlFailure::NotConnected),
                ConnectFollowUp::Done,
            ),
            (
                false,
                Err(ControlFailure::Unauthorized),
                ConnectFollowUp::Report,
            ),
            (
                false,
                Err(ControlFailure::Transport),
                ConnectFollowUp::Report,
            ),
            (
                false,
                Err(ControlFailure::Unsupported),
                ConnectFollowUp::Report,
            ),
        ];

        for (connect, outcome, expected) in cases {
            assert_eq!(
                ConnectFollowUp::resolve(connect, &outcome),
                expected,
                "connect={connect} outcome={outcome:?}"
            );
        }
    }

    struct Applied {
        screens: Vec<Screen>,
        error_toasts: usize,
    }

    fn apply(
        cx: &mut TestAppContext,
        hub: &Hub,
        connect: bool,
        outcome: ControlOutcome,
    ) -> Applied {
        cx.update(|cx| cx.set_global(Toasts::new()));
        let log = cx.update(|cx| nav_log(&hub.view, cx));
        hub.view.update(cx, |view, cx| {
            view.apply_connect_outcome(kick(), connect, outcome, cx)
        });
        Applied {
            screens: log.read_with(cx, |log, _| log.screens()),
            error_toasts: cx.update(|cx| error_toasts(cx)),
        }
    }

    #[gpui::test]
    fn a_connect_refused_for_want_of_sign_in_opens_the_integration_page(cx: &mut TestAppContext) {
        let rt = runtime();
        for welcome in [false, true] {
            let hub = mount_as(cx, &rt, welcome);

            let applied = apply(cx, &hub, true, Err(ControlFailure::NotConnected));

            assert_eq!(
                applied.screens,
                vec![Screen::BuiltinDetail(kick())],
                "welcome={welcome}"
            );
            assert_eq!(applied.error_toasts, 0, "welcome={welcome}");
        }
    }

    #[gpui::test]
    fn a_failed_connection_change_raises_an_error_toast_and_stays_put(cx: &mut TestAppContext) {
        let rt = runtime();
        let hub = mount(cx, &rt);

        for (connect, failure) in [
            (true, ControlFailure::Transport),
            (true, ControlFailure::Unauthorized),
            (false, ControlFailure::Transport),
        ] {
            let applied = apply(cx, &hub, connect, Err(failure.clone()));

            assert_eq!(applied.error_toasts, 1, "connect={connect} {failure:?}");
            assert_eq!(applied.screens, Vec::new(), "connect={connect} {failure:?}");
        }
    }

    #[gpui::test]
    fn a_settled_connection_change_neither_navigates_nor_toasts(cx: &mut TestAppContext) {
        let rt = runtime();
        let hub = mount(cx, &rt);

        for (connect, outcome) in [
            (true, Ok(())),
            (false, Ok(())),
            (false, Err(ControlFailure::NotConnected)),
        ] {
            let applied = apply(cx, &hub, connect, outcome.clone());

            assert_eq!(applied.error_toasts, 0, "connect={connect} {outcome:?}");
            assert_eq!(applied.screens, Vec::new(), "connect={connect} {outcome:?}");
        }
    }

    const CARD_WINDOW_W: f32 = 1200.0;
    const CARD_WINDOW_H: f32 = 600.0;
    const CARD_FOOTER_BAND: std::ops::Range<f32> = 150.0..400.0;
    const CARD_SCAN_STEP_X: f32 = 16.0;
    const CARD_SCAN_STEP_Y: f32 = 6.0;

    #[gpui::test]
    fn a_failed_card_offers_retry_with_and_without_a_disclaimer(cx: &mut TestAppContext) {
        let rt = runtime();
        let storage = rt.block_on(sandboxed_provider());
        let (settings, mut writes) = test_backend();
        let supervisor =
            idle_supervisor(&rt, settings as Arc<dyn SettingsRepo>, &[twitch(), kick()]);
        pump(&rt);
        while writes.try_recv().is_ok() {}
        let launch = HubLaunch {
            supervisor,
            builtins: BuiltinRegistry::default(),
            backend: Arc::clone(&storage),
            sub_actions: Arc::new(owned_sub_actions(&[])),
            triggers: Arc::new(owned_triggers(&[])),
            rt_handle: rt.handle().clone(),
            obs_install_seed: ObsInstallSeed::new(forge_obs::SwitchableObsSink::new()),
            vtube_install_seed: VTubeInstallSeed::new(forge_vtube::SwitchableVTubeSink::new()),
        };
        let focus = declaration_of(&kick()).map(|declaration| declaration.category);
        assert_eq!(
            focus,
            declaration_of(&twitch()).map(|declaration| declaration.category),
            "both cards must share the focused section"
        );
        install_presentation(cx);
        let failed = LifecycleStates::from([
            (
                twitch(),
                LifecycleState::Failed(crate::test_support::start_failure("boom")),
            ),
            (
                kick(),
                LifecycleState::Failed(crate::test_support::start_failure("boom")),
            ),
        ]);
        let (_view, vcx) = cx.add_window_view(|_, cx| {
            let lifecycle = cx.new(|_| IntegrationLifecycle::new(failed));
            let connectivity = cx.new(|_| PlatformConnectivity::new());
            IntegrationsHubView::new(focus, lifecycle, connectivity, launch, cx)
        });
        vcx.simulate_resize(gpui::size(px(CARD_WINDOW_W), px(CARD_WINDOW_H)));
        vcx.run_until_parked();

        click_grid(
            vcx,
            ClickArea {
                x: 0.0..CARD_WINDOW_W,
                y: CARD_FOOTER_BAND,
                step_x: CARD_SCAN_STEP_X,
                step_y: CARD_SCAN_STEP_Y,
            },
        );
        let mut retried: HashSet<IntegrationId> = HashSet::new();
        pump_until(&rt, || {
            retried.extend(
                std::iter::from_fn(|| writes.try_recv().ok())
                    .filter(|(_, value)| value == "true")
                    .filter_map(|(key, _)| {
                        [twitch(), kick()]
                            .into_iter()
                            .find(|id| key == integration_enabled_key(id))
                    }),
            );
            retried.len() == 2
        });

        assert_eq!(retried, HashSet::from([twitch(), kick()]));
    }

    struct SilentPublisher;

    impl forge_events::EventPublisher for SilentPublisher {
        fn publish(&self, _event: forge_events::Event) {}
    }

    fn refused_obs_seed(rt: &tokio::runtime::Runtime) -> ObsInstallSeed {
        let seed = ObsInstallSeed::new(forge_obs::SwitchableObsSink::new());
        rt.block_on(async {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            drop(listener);
            let client = Arc::new(
                forge_obs::ObsClient::connect(
                    &format!("127.0.0.1:{port}"),
                    None,
                    Arc::new(SilentPublisher),
                )
                .await
                .unwrap(),
            );
            client.set_auto_reconnect(false);
            seed.install(client);
            yield_until(|| seed.connect_failure().is_some()).await;
        });
        seed
    }

    #[gpui::test]
    fn the_hub_reads_each_integrations_connect_failure_from_its_own_seed(cx: &mut TestAppContext) {
        let rt = runtime();
        let storage = rt.block_on(sandboxed_provider());
        let (settings, _writes) = test_backend();
        let supervisor =
            idle_supervisor(&rt, settings as Arc<dyn SettingsRepo>, &[twitch(), obs()]);
        let launch = HubLaunch {
            supervisor,
            builtins: BuiltinRegistry::default(),
            backend: Arc::clone(&storage),
            sub_actions: Arc::new(owned_sub_actions(&[])),
            triggers: Arc::new(owned_triggers(&[])),
            rt_handle: rt.handle().clone(),
            obs_install_seed: refused_obs_seed(&rt),
            vtube_install_seed: VTubeInstallSeed::new(forge_vtube::SwitchableVTubeSink::new()),
        };
        let view = cx.update(|cx| {
            let lifecycle = cx.new(|_| IntegrationLifecycle::new(LifecycleStates::new()));
            let connectivity = cx.new(|_| PlatformConnectivity::new());
            cx.new(|cx| IntegrationsHubView::new(None, lifecycle, connectivity, launch, cx))
        });

        let codes: Vec<Option<String>> = [obs(), IntegrationId::new("vtube"), twitch()]
            .iter()
            .map(|id| {
                view.read_with(cx, |view, _| {
                    view.connect_failure_of(id)
                        .map(|failure| failure.code().name().to_owned())
                })
            })
            .collect();

        assert_eq!(codes, vec![Some("connect_refused".to_owned()), None, None]);
    }
}
