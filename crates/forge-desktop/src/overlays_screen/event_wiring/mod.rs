mod confirm;
mod plan;
mod readout;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use forge_components::{
    GridPicker, GridPickerConfig, GridPickerEvent, GridPickerSubtitle, Icon, OverlayPosition,
    ToastAction, ToastKind, overlay, tr,
};
use forge_overlay::{OverlayKindRegistry, accepts_event_wiring};
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_runtime::actions::{
    ActionsService, OverlayFeed, OverlayWiringError, OverlayWiringOutcome, OverlayWiringRecords,
    OverlayWiringRefusal, WiredQueue,
};
use forge_runtime::{MembershipOutcome, QueueSchedulerHandle};
use forge_storage::{OverlayDefinition, OverlayId};
use forge_types::{ActionId, Queue};
use gpui::{
    App, Context, Entity, EventEmitter, SharedString, Subscription, Window, div, prelude::*,
};

use crate::async_bridge::{self, Generation};
use crate::presentation::ActivePresentation;
use crate::screen::Screen;
use crate::sidebar::NavRequested;
use crate::toasts::PushToast;

use plan::{FeedRow, WiringDraft};

pub(super) use readout::{entry_button, render_readout};

const TOAST_WITH_ACTION: Duration = Duration::from_secs(8);

pub(super) struct WiringLaunch {
    pub actions: Arc<ActionsService>,
    pub triggers: Arc<TriggerRegistry>,
    pub sub_actions: Arc<SubActionRegistry>,
    pub scheduler: QueueSchedulerHandle,
    pub kinds: Arc<OverlayKindRegistry>,
    pub rt_handle: tokio::runtime::Handle,
}

enum ConfirmState {
    Checking,
    Ready(WiringDraft),
    AlreadyWired {
        action_id: ActionId,
        action_name: String,
    },
    Refused(String),
}

struct ConfirmStage {
    trigger_kind_id: String,
    trigger_label: String,
    state: ConfirmState,
    writing: bool,
}

struct PickStage {
    picker: Entity<GridPicker>,
    picks: HashMap<SharedString, String>,
    _sub: Subscription,
}

enum WiringStage {
    Pick(PickStage),
    Confirm(ConfirmStage),
}

struct QueueLanding {
    name: String,
    live: bool,
}

enum Landing {
    Wired {
        records: OverlayWiringRecords,
        queue: Option<QueueLanding>,
    },
    AlreadyWired(ActionId),
    Incomplete {
        records: OverlayWiringRecords,
        message: String,
    },
    Failed(String),
    Missing,
}

pub(super) struct EventWiringView {
    handles: WiringLaunch,
    overlay: Option<OverlayDefinition>,
    feeds: Vec<FeedRow>,
    feeds_gen: Generation,
    stage: Option<WiringStage>,
    favorites: HashSet<SharedString>,
    delete_count: Option<(OverlayId, usize)>,
}

impl EventWiringView {
    pub(super) fn new(handles: WiringLaunch) -> Self {
        Self {
            handles,
            overlay: None,
            feeds: Vec::new(),
            feeds_gen: Generation::default(),
            stage: None,
            favorites: HashSet::new(),
            delete_count: None,
        }
    }

    pub(super) fn accepts(&self, kind_id: &str) -> bool {
        self.handles
            .kinds
            .get(kind_id)
            .is_some_and(accepts_event_wiring)
    }

    fn eligible(&self) -> bool {
        self.overlay
            .as_ref()
            .is_some_and(|definition| self.accepts(&definition.kind_id))
    }

    pub(super) fn focus_overlay(
        &mut self,
        definition: Option<OverlayDefinition>,
        cx: &mut Context<Self>,
    ) {
        let moved =
            self.overlay.as_ref().map(|held| &held.id) != definition.as_ref().map(|held| &held.id);
        self.overlay = definition;
        if moved {
            self.stage = None;
            self.feeds.clear();
        }
        self.refresh_feeds(cx);
        cx.notify();
    }

    fn refresh_feeds(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.overlay.as_ref().map(|held| held.id.clone()) else {
            self.feeds.clear();
            return;
        };
        let ticket = self.feeds_gen.next();
        let actions = Arc::clone(&self.handles.actions);
        let sub_actions = Arc::clone(&self.handles.sub_actions);
        async_bridge::run_async(
            &self.handles.rt_handle,
            async move { actions.overlay_feeds(&id, &sub_actions).await },
            move |this, result: Result<Vec<OverlayFeed>, _>, cx| {
                if !this.feeds_gen.is_current(ticket) {
                    return;
                }
                match result {
                    Ok(feeds) => this.feeds = plan::feed_rows(feeds, &this.handles.triggers),
                    Err(error) => {
                        tracing::warn!(%error, "what feeds this overlay is unreadable");
                        this.feeds.clear();
                    }
                }
                cx.notify();
            },
            cx,
        );
    }

    pub(super) fn count_for_delete(&mut self, id: OverlayId, cx: &mut Context<Self>) {
        self.delete_count = None;
        let actions = Arc::clone(&self.handles.actions);
        let sub_actions = Arc::clone(&self.handles.sub_actions);
        let target = id.clone();
        async_bridge::run_async(
            &self.handles.rt_handle,
            async move { actions.overlay_feeds(&target, &sub_actions).await },
            move |this, result: Result<Vec<OverlayFeed>, _>, cx| {
                if let Ok(feeds) = result {
                    this.delete_count = Some((id, feeds.len()));
                    cx.notify();
                }
            },
            cx,
        );
    }

    pub(super) fn clear_delete_count(&mut self) {
        self.delete_count = None;
    }

    pub(super) fn delete_feed_count(&self, id: &OverlayId) -> Option<usize> {
        self.delete_count
            .as_ref()
            .filter(|(counted, _)| counted == id)
            .map(|(_, count)| *count)
    }

    fn open_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(definition) = self.overlay.clone() else {
            return;
        };
        let palette = cx.palette();
        let (groups, picks) = plan::offered_events(&self.handles.triggers, &palette);
        let count: usize = groups.iter().map(|group| group.items.len()).sum();
        let config = GridPickerConfig {
            accent: palette.brand,
            header_icon: Icon::Bolt,
            title: tr!("overlays_wire_picker_title").into(),
            subtitle: GridPickerSubtitle::Context {
                lead: tr!("overlays_wire_picker_lead").into(),
                name: definition.display_name.clone().into(),
                note: tr!("overlays_wire_picker_count", count = count as i64).into(),
            },
            footer_hint: tr!("overlays_wire_picker_hint").into(),
            search_placeholder: tr!("overlays_wire_picker_search").into(),
            favorites_label: tr!("picker_favorites").into(),
            favorites_empty: tr!("picker_favorites_empty").into(),
        };
        let favorites = self.favorites.clone();
        let picker = cx.new(|cx| GridPicker::new(config, groups, favorites, palette, cx));
        let sub = cx.subscribe(&picker, Self::on_picker_event);
        picker.update(cx, |picker, cx| picker.focus(window, cx));
        self.stage = Some(WiringStage::Pick(PickStage {
            picker,
            picks,
            _sub: sub,
        }));
        cx.notify();
    }

    fn on_picker_event(
        &mut self,
        _picker: Entity<GridPicker>,
        event: &GridPickerEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            GridPickerEvent::Picked(id) => {
                let kind_id = match self.stage.as_ref() {
                    Some(WiringStage::Pick(stage)) => stage.picks.get(id).cloned(),
                    _ => None,
                };
                if let Some(kind_id) = kind_id {
                    self.begin_confirm(kind_id, cx);
                }
            }
            GridPickerEvent::FavoriteToggled(id) => {
                if !self.favorites.remove(id) {
                    self.favorites.insert(id.clone());
                }
                cx.notify();
            }
            GridPickerEvent::Dismissed => self.close(cx),
        }
    }

    fn begin_confirm(&mut self, kind_id: String, cx: &mut Context<Self>) {
        let Some(definition) = self.overlay.clone() else {
            return;
        };
        let label = self
            .handles
            .triggers
            .get(&kind_id)
            .map_or_else(|| kind_id.clone(), |held| held.label().to_owned());
        self.stage = Some(WiringStage::Confirm(ConfirmStage {
            trigger_kind_id: kind_id.clone(),
            trigger_label: label,
            state: ConfirmState::Checking,
            writing: false,
        }));
        cx.notify();

        let actions = Arc::clone(&self.handles.actions);
        let sub_actions = Arc::clone(&self.handles.sub_actions);
        let probe = kind_id.clone();
        async_bridge::run_async(
            &self.handles.rt_handle,
            async move {
                actions
                    .overlay_wired_to(&definition.id, &probe, &sub_actions)
                    .await
            },
            move |this, result: Result<Option<ActionId>, _>, cx| {
                let wired = match result {
                    Ok(wired) => wired,
                    Err(error) => {
                        tracing::warn!(%error, "whether this overlay is already wired is unreadable");
                        None
                    }
                };
                this.settle_check(&kind_id, wired, cx);
            },
            cx,
        );
    }

    fn settle_check(&mut self, kind_id: &str, wired: Option<ActionId>, cx: &mut Context<Self>) {
        let Some(WiringStage::Confirm(stage)) = self.stage.as_ref() else {
            return;
        };
        if stage.trigger_kind_id != kind_id {
            return;
        }

        let next = match wired {
            Some(action_id) => ConfirmState::AlreadyWired {
                action_id,
                action_name: self.action_name(action_id),
            },
            None => match self.build_draft(kind_id) {
                Ok(draft) => ConfirmState::Ready(draft),
                Err(refusal) => ConfirmState::Refused(plan::refusal_message(&refusal)),
            },
        };

        if let Some(WiringStage::Confirm(stage)) = self.stage.as_mut() {
            stage.state = next;
        }
        cx.notify();
    }

    fn action_name(&self, action_id: ActionId) -> String {
        self.feeds
            .iter()
            .find(|row| row.action_id == action_id)
            .map_or_else(String::new, |row| row.action_name.clone())
    }

    fn build_draft(&self, kind_id: &str) -> Result<WiringDraft, OverlayWiringRefusal> {
        let no_variables = || OverlayWiringRefusal::TriggerDeclaresNoVariables {
            trigger_kind_id: kind_id.to_owned(),
        };
        let overlay = self.overlay.as_ref().ok_or_else(no_variables)?;
        let kind = self.handles.kinds.get(&overlay.kind_id).ok_or_else(|| {
            OverlayWiringRefusal::OverlayKindNotEligible {
                overlay_kind_id: overlay.kind_id.clone(),
            }
        })?;
        let trigger = self
            .handles
            .triggers
            .get(kind_id)
            .ok_or_else(no_variables)?;
        plan::draft_wiring(overlay, kind, trigger)
    }

    fn confirm(&mut self, cx: &mut Context<Self>) {
        let Some(WiringStage::Confirm(stage)) = self.stage.as_ref() else {
            return;
        };
        if stage.writing || !matches!(stage.state, ConfirmState::Ready(_)) {
            return;
        }
        let Some(definition) = self.overlay.clone() else {
            return;
        };
        let kind_id = stage.trigger_kind_id.clone();

        if let Some(WiringStage::Confirm(stage)) = self.stage.as_mut() {
            stage.writing = true;
        }
        cx.notify();

        let actions = Arc::clone(&self.handles.actions);
        let sub_actions = Arc::clone(&self.handles.sub_actions);
        let triggers = Arc::clone(&self.handles.triggers);
        let kinds = Arc::clone(&self.handles.kinds);
        let scheduler = self.handles.scheduler.clone();
        async_bridge::run_async(
            &self.handles.rt_handle,
            async move {
                let Some(kind) = kinds.get(&definition.kind_id) else {
                    return Landing::Missing;
                };
                let Some(trigger) = triggers.get(&kind_id) else {
                    return Landing::Missing;
                };
                match actions
                    .wire_overlay_to_event(&definition, kind, trigger, &sub_actions)
                    .await
                {
                    Ok(OverlayWiringOutcome::AlreadyWired { action_id }) => {
                        Landing::AlreadyWired(action_id)
                    }
                    Ok(OverlayWiringOutcome::Wired(records)) => {
                        let queue = register_created_queue(&scheduler, &records).await;
                        Landing::Wired { records, queue }
                    }
                    Err(OverlayWiringError::Incomplete { records, source }) => {
                        Landing::Incomplete {
                            records,
                            message: source.to_string(),
                        }
                    }
                    Err(error) => Landing::Failed(error.to_string()),
                }
            },
            Self::settle_wiring,
            cx,
        );
    }

    fn settle_wiring(&mut self, landing: Landing, cx: &mut Context<Self>) {
        self.stage = None;
        match landing {
            Landing::Wired { records, queue } => {
                let created = tr!("overlays_wire_toast_created");
                match records.action_id {
                    Some(action_id) => {
                        self.raise_open_toast(ToastKind::Success, created, action_id, cx)
                    }
                    None => cx.push_toast(ToastKind::Success, created),
                }
                if let Some(warning) = queue_warning(queue.as_ref()) {
                    cx.push_toast(ToastKind::Error, warning);
                }
                self.refresh_feeds(cx);
            }
            Landing::AlreadyWired(action_id) => self.raise_open_toast(
                ToastKind::Info,
                tr!("overlays_wire_toast_already"),
                action_id,
                cx,
            ),
            Landing::Incomplete { records, message } => {
                tracing::warn!(error = %message, "overlay event wiring stopped part-way");
                let landed = plan::landed_note(&records);
                let note = tr!("overlays_wire_toast_incomplete", landed = landed.as_str());
                match records.action_id {
                    Some(action_id) => self.raise_open_toast(ToastKind::Error, note, action_id, cx),
                    None => cx.push_toast(ToastKind::Error, note),
                }
                self.refresh_feeds(cx);
            }
            Landing::Failed(message) => {
                tracing::warn!(error = %message, "overlay event wiring failed");
                cx.push_toast(ToastKind::Error, message);
            }
            Landing::Missing => cx.push_toast(ToastKind::Error, tr!("overlays_wire_toast_missing")),
        }
        cx.notify();
    }

    fn raise_open_toast(
        &self,
        kind: ToastKind,
        message: String,
        action_id: ActionId,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        cx.push_toast_full(
            kind,
            message,
            None,
            Some(ToastAction::new(
                tr!("overlays_wire_open_action"),
                move |_window, app: &mut App| {
                    view.update(app, |_, cx| {
                        cx.emit(NavRequested(Screen::Actions(Some(action_id))));
                    });
                },
            )),
            TOAST_WITH_ACTION,
        );
    }

    fn open_action(&mut self, action_id: ActionId, cx: &mut Context<Self>) {
        self.stage = None;
        cx.emit(NavRequested(Screen::Actions(Some(action_id))));
        cx.notify();
    }

    fn back_to_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_picker(window, cx);
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.stage = None;
        cx.notify();
    }
}

/// A queue created by the wiring is not live until the scheduler is told about it, so the action
/// it was created for would never run.
async fn register_created_queue(
    scheduler: &QueueSchedulerHandle,
    records: &OverlayWiringRecords,
) -> Option<QueueLanding> {
    let queue: &Queue = records.queue.as_ref().and_then(WiredQueue::created)?;
    let live = match scheduler.register(queue.clone()).await {
        Ok(MembershipOutcome::Applied | MembershipOutcome::AlreadyRegistered) => true,
        Ok(MembershipOutcome::NotFound) => false,
        Err(error) => {
            tracing::warn!(%error, "the scheduler refused the queue the wiring created");
            false
        }
    };
    Some(QueueLanding {
        name: queue.name.clone(),
        live,
    })
}

fn queue_warning(queue: Option<&QueueLanding>) -> Option<String> {
    let landing = queue.filter(|landing| !landing.live)?;
    Some(tr!(
        "overlays_wire_queue_not_live",
        queue = landing.name.as_str()
    ))
}

impl EventEmitter<NavRequested> for EventWiringView {}

impl Render for EventWiringView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let layer = match self.stage.as_ref() {
            Some(WiringStage::Pick(stage)) => {
                let view = cx.entity();
                Some(
                    overlay(stage.picker.clone(), &palette)
                        .position(OverlayPosition::Center)
                        .on_dismiss("overlays-wire-pick-scrim", move |_window, cx| {
                            view.update(cx, |this, cx| this.close(cx));
                        })
                        .into_any_element(),
                )
            }
            Some(WiringStage::Confirm(stage)) => Some(self.render_confirm(stage, &palette, cx)),
            None => None,
        };

        let Some(layer) = layer else {
            return div().into_any_element();
        };
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(layer)
            .into_any_element()
    }
}
