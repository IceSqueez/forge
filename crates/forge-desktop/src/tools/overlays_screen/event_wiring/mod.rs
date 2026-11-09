mod confirm;
mod plan;
mod readout;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use forge_components::focus_inside_modal;
use forge_components::{
    ActivePresentation, GridPicker, GridPickerArt, GridPickerConfig, GridPickerEvent,
    GridPickerSubtitle, Icon, OverlayPosition, ToastAction, ToastKind, overlay, tr,
};
use forge_overlay::{OverlayKindRegistry, accepts_event_wiring};
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_runtime::{
    ActionsService, OverlayFeed, OverlayWiringOutcome, OverlayWiringPlan, OverlayWiringPreview,
    OverlayWiringRecords, OverlayWiringRefusal, OverlayWiringRequest, RuntimeError, WiringTarget,
    suggest_overlay_content,
};
use forge_storage::{OverlayDefinition, OverlayId, SettingsRepo};
use forge_types::{ActionId, NamesLanguage};
use gpui::{
    App, Context, Entity, EventEmitter, SharedString, Subscription, Window, div, prelude::*,
};

use crate::app::async_bridge::{self, Generation};
use crate::app::navigation::NavRequested;
use crate::app::navigation::Screen;
use crate::shared::picker_favorites::{
    StoredFavorites, TRIGGER_FAVORITES, bind_picker, follow_picker_event,
};
use crate::shared::repo_failure::RepoFailure;
use forge_components::PushToast;

use plan::{FeedRow, WiringDraft};

pub(super) use readout::{entry_button, render_readout};

const TOAST_WITH_ACTION: Duration = Duration::from_secs(8);

pub(super) struct WiringLaunch {
    pub actions: Arc<ActionsService>,
    pub triggers: Arc<TriggerRegistry>,
    pub sub_actions: Arc<SubActionRegistry>,
    pub kinds: Arc<OverlayKindRegistry>,
    pub settings: Arc<dyn SettingsRepo>,
    pub rt_handle: tokio::runtime::Handle,
}

enum ConfirmState {
    Checking,
    Ready(Box<WiringDraft>),
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
    _favorites_sub: Subscription,
}

enum WiringStage {
    Pick(PickStage),
    Confirm(ConfirmStage),
}

enum Check {
    Wired(ActionId),
    Planned(Box<OverlayWiringPlan>),
    Refused(OverlayWiringRefusal),
    Failed(RepoFailure),
}

enum Landing {
    Wired(OverlayWiringRecords),
    AlreadyWired(ActionId),
    Failed(RepoFailure),
    Missing,
}

pub(super) struct EventWiringView {
    handles: WiringLaunch,
    overlay: Option<OverlayDefinition>,
    feeds: Vec<FeedRow>,
    feeds_gen: Generation,
    stage: Option<WiringStage>,
    favorites: Entity<StoredFavorites>,
    delete_count: Option<(OverlayId, usize)>,
}

impl EventWiringView {
    pub(super) fn new(handles: WiringLaunch, cx: &mut Context<Self>) -> Self {
        let favorites = StoredFavorites::shared(
            TRIGGER_FAVORITES,
            Arc::clone(&handles.settings),
            &handles.rt_handle,
            cx,
        );
        Self {
            handles,
            overlay: None,
            feeds: Vec::new(),
            feeds_gen: Generation::default(),
            stage: None,
            favorites,
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
            move |this, result: Result<Vec<OverlayFeed>, _>, cx| match result {
                Ok(feeds) => {
                    this.delete_count = Some((id, feeds.len()));
                    cx.notify();
                }
                Err(error) => tracing::warn!(%error, "overlay feed count unreadable"),
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
            art: GridPickerArt::default(),
            header_icon: Icon::Bolt,
            title: tr!("overlays_wire_picker_title").into(),
            subtitle: GridPickerSubtitle::Context {
                lead: tr!("overlays_wire_picker_lead").into(),
                name: definition.display_name.clone().into(),
                note: tr!(
                    "overlays_wire_picker_count",
                    count = i64::try_from(count).unwrap_or(i64::MAX)
                )
                .into(),
            },
            footer_hint: tr!("overlays_wire_picker_hint").into(),
            search_placeholder: tr!("overlays_wire_picker_search").into(),
            favorites_label: tr!("picker_favorites").into(),
            favorites_empty: tr!("picker_favorites_empty").into(),
        };
        let favorites = self.favorites.read(cx).ids().cloned().unwrap_or_default();
        let picker = cx.new(|cx| GridPicker::new(config, groups, favorites, cx));
        let favorites_sub = bind_picker(&self.favorites, &picker, cx);
        let sub = cx.subscribe(&picker, Self::on_picker_event);
        focus_inside_modal(window, cx, |window, cx| {
            picker.update(cx, |picker, cx| picker.focus(window, cx))
        });
        self.stage = Some(WiringStage::Pick(PickStage {
            picker,
            picks,
            _favorites_sub: favorites_sub,
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
            GridPickerEvent::FavoriteToggled(_) | GridPickerEvent::FavoritesRetry => {
                follow_picker_event(&self.favorites, event, cx);
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
            .map_or_else(|| kind_id.clone(), crate::shared::kind_copy::trigger_name);
        self.stage = Some(WiringStage::Confirm(ConfirmStage {
            trigger_kind_id: kind_id.clone(),
            trigger_label: label,
            state: ConfirmState::Checking,
            writing: false,
        }));
        cx.notify();

        let actions = Arc::clone(&self.handles.actions);
        let sub_actions = Arc::clone(&self.handles.sub_actions);
        let triggers = Arc::clone(&self.handles.triggers);
        let kinds = Arc::clone(&self.handles.kinds);
        let probe = kind_id.clone();
        async_bridge::run_async(
            &self.handles.rt_handle,
            async move {
                match actions
                    .overlay_wired_to(&definition.id, &probe, &sub_actions)
                    .await
                {
                    Ok(Some(action_id)) => return Check::Wired(action_id),
                    Ok(None) => {}
                    Err(error) => {
                        tracing::warn!(%error, "whether this overlay is already wired is unreadable");
                    }
                }
                let Some(kind) = kinds.get(&definition.kind_id) else {
                    return Check::Refused(OverlayWiringRefusal::OverlayKindNotEligible {
                        overlay_kind_id: definition.kind_id.clone(),
                    });
                };
                let Some(trigger) = triggers.get(&probe) else {
                    return Check::Refused(OverlayWiringRefusal::TargetMissing {
                        trigger_kind_id: probe,
                    });
                };
                let request = OverlayWiringRequest {
                    overlay: &definition,
                    kind,
                    trigger,
                    target: WiringTarget::NewAction {
                        name: plan::default_action_name(&definition, trigger),
                        queue_id: None,
                    },
                    content: suggest_overlay_content(kind, trigger),
                };
                match actions.plan_overlay_wiring(request, &sub_actions).await {
                    Ok(OverlayWiringPreview::Ready(plan)) => Check::Planned(plan),
                    Ok(OverlayWiringPreview::AlreadyWired { action_id }) => Check::Wired(action_id),
                    Err(RuntimeError::OverlayWiringRefused(refusal)) => Check::Refused(refusal),
                    Err(error) => Check::Failed(RepoFailure::operation(error)),
                }
            },
            move |this, check, cx| this.settle_check(&kind_id, check, cx),
            cx,
        );
    }

    fn settle_check(&mut self, kind_id: &str, check: Check, cx: &mut Context<Self>) {
        let Some(WiringStage::Confirm(stage)) = self.stage.as_ref() else {
            return;
        };
        if stage.trigger_kind_id != kind_id {
            return;
        }

        let next = match check {
            Check::Wired(action_id) => ConfirmState::AlreadyWired {
                action_id,
                action_name: self.action_name(action_id),
            },
            Check::Planned(planned) => match self.build_draft(
                kind_id,
                planned,
                crate::app::presentation::active_names_language(cx),
            ) {
                Ok(draft) => ConfirmState::Ready(Box::new(draft)),
                Err(refusal) => ConfirmState::Refused(plan::refusal_message(&refusal)),
            },
            Check::Refused(refusal) => ConfirmState::Refused(plan::refusal_message(&refusal)),
            Check::Failed(failure) => ConfirmState::Refused(failure.text()),
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

    fn build_draft(
        &self,
        kind_id: &str,
        planned: Box<OverlayWiringPlan>,
        language: NamesLanguage,
    ) -> Result<WiringDraft, OverlayWiringRefusal> {
        let target_missing = || OverlayWiringRefusal::TargetMissing {
            trigger_kind_id: kind_id.to_owned(),
        };
        let overlay = self.overlay.as_ref().ok_or_else(target_missing)?;
        let kind = self.handles.kinds.get(&overlay.kind_id).ok_or_else(|| {
            OverlayWiringRefusal::OverlayKindNotEligible {
                overlay_kind_id: overlay.kind_id.clone(),
            }
        })?;
        let trigger = self
            .handles
            .triggers
            .get(kind_id)
            .ok_or_else(target_missing)?;
        Ok(plan::draft_wiring(
            overlay, kind, trigger, *planned, language,
        ))
    }

    fn confirm(&mut self, cx: &mut Context<Self>) {
        let Some(WiringStage::Confirm(stage)) = self.stage.as_mut() else {
            return;
        };
        if stage.writing {
            return;
        }
        let ConfirmState::Ready(draft) = &stage.state else {
            return;
        };
        let planned = draft.plan.clone();
        stage.writing = true;
        cx.notify();

        let actions = Arc::clone(&self.handles.actions);
        let sub_actions = Arc::clone(&self.handles.sub_actions);
        async_bridge::run_async(
            &self.handles.rt_handle,
            async move {
                match actions.commit_overlay_wiring(&planned, &sub_actions).await {
                    Ok(OverlayWiringOutcome::AlreadyWired { action_id }) => {
                        Landing::AlreadyWired(action_id)
                    }
                    Ok(OverlayWiringOutcome::Wired(records)) => Landing::Wired(records),
                    Err(RuntimeError::OverlayWiringRefused(_)) => Landing::Missing,
                    Err(error) => Landing::Failed(RepoFailure::operation(error)),
                }
            },
            Self::settle_wiring,
            cx,
        );
    }

    fn settle_wiring(&mut self, landing: Landing, cx: &mut Context<Self>) {
        self.stage = None;
        match landing {
            Landing::Wired(records) => {
                self.raise_open_toast(
                    ToastKind::Success,
                    tr!("overlays_wire_toast_created"),
                    records.action_id,
                    cx,
                );
                self.refresh_feeds(cx);
            }
            Landing::AlreadyWired(action_id) => self.raise_open_toast(
                ToastKind::Info,
                tr!("overlays_wire_toast_already"),
                action_id,
                cx,
            ),
            Landing::Failed(failure) => {
                tracing::warn!(error = %failure, "overlay event wiring failed");
                cx.push_toast(ToastKind::Error, failure.text());
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

#[cfg(test)]
mod tests {
    use forge_components::{Density, ThemeId};
    use forge_overlay::kinds::alert::{self, AlertOverlayKind};
    use forge_overlay::{OverlayConfig, OverlayKindRegistry};
    use forge_registry::TriggerCategory;
    use forge_storage::Language;
    use forge_storage::MockActionRepo;
    use forge_storage::MockHistoryRepo;
    use forge_storage::MockQueueRepo;
    use forge_storage::MockTriggerInstanceRepo;
    use forge_storage::test_support::MemorySettings;
    use gpui::TestAppContext;

    use super::*;
    use crate::app::i18n::install_language;
    use crate::test_support::{
        Declares, StubTrigger, overlay_definition, pump_until, runtime, trigger_registry,
    };
    use forge_components::Presentation;
    use forge_components::Toasts;

    const SUBSCRIBER: &str = "twitch.support.subscriber";
    const FOLLOW: &str = "twitch.channel.follow";

    fn launch(rt: &tokio::runtime::Runtime) -> WiringLaunch {
        let mut kinds = OverlayKindRegistry::new();
        kinds
            .register(Box::new(AlertOverlayKind))
            .expect("the alert kind registers once");
        WiringLaunch {
            actions: Arc::new(ActionsService::new(
                Arc::new(MockActionRepo::new()),
                Arc::new(MockQueueRepo::new()),
                Arc::new(MockHistoryRepo::new()),
                Arc::new(MockTriggerInstanceRepo::new()),
            )),
            triggers: Arc::new(trigger_registry(vec![
                StubTrigger::new(
                    SUBSCRIBER,
                    "Subscribed",
                    TriggerCategory::Subscriptions,
                    Declares::APrincipal,
                ),
                StubTrigger::new(
                    FOLLOW,
                    "Followed",
                    TriggerCategory::Subscriptions,
                    Declares::APrincipal,
                ),
            ])),
            sub_actions: Arc::new(SubActionRegistry::new()),
            kinds: Arc::new(kinds),
            settings: Arc::new(forge_storage::test_support::MemorySettings::new()),
            rt_handle: rt.handle().clone(),
        }
    }

    fn mount(cx: &mut TestAppContext) -> (Entity<EventWiringView>, tokio::runtime::Runtime) {
        install_language(Language::En);
        cx.update(|cx| {
            cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
            cx.set_global(Toasts::new());
        });
        let rt = runtime();
        let view = cx.update(|cx| cx.new(|cx| EventWiringView::new(launch(&rt), cx)));
        (view, rt)
    }

    fn stored_favorites(
        settings: &Arc<MemorySettings>,
        rt: &tokio::runtime::Runtime,
    ) -> Option<Vec<String>> {
        rt.block_on(forge_storage::read_stored_setting(
            settings.as_ref(),
            TRIGGER_FAVORITES.setting,
        ))
        .expect("the memory store reads")
    }

    fn open_wiring_picker(
        settings: &Arc<MemorySettings>,
        rt: &tokio::runtime::Runtime,
        cx: &mut TestAppContext,
    ) -> Entity<GridPicker> {
        install_language(Language::En);
        cx.update(|cx| {
            cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
            cx.set_global(Toasts::new());
        });
        let handles = WiringLaunch {
            settings: Arc::clone(settings) as Arc<dyn SettingsRepo>,
            ..launch(rt)
        };
        let (view, vcx) = cx.add_window_view(|_, cx| EventWiringView::new(handles, cx));
        pump_until(rt, || {
            vcx.run_until_parked();
            vcx.update(|_, cx| view.read(cx).favorites.read(cx).ids().is_some())
        });
        vcx.update(|window, cx| {
            view.update(cx, |this, cx| {
                this.overlay = Some(overlay_definition(alert::KIND_ID, OverlayConfig::new()));
                this.open_picker(window, cx);
            });
        });
        vcx.update(|_, cx| match view.read(cx).stage.as_ref() {
            Some(WiringStage::Pick(stage)) => stage.picker.clone(),
            _ => panic!("the event picker did not open"),
        })
    }

    #[gpui::test]
    fn a_favorite_starred_in_the_event_picker_is_stored_for_the_next_launch(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let settings = Arc::new(MemorySettings::new());
        let picker = open_wiring_picker(&settings, &rt, cx);

        picker.update(cx, |_, cx| {
            cx.emit(GridPickerEvent::FavoriteToggled(SUBSCRIBER.into()));
        });
        pump_until(&rt, || {
            cx.run_until_parked();
            stored_favorites(&settings, &rt).is_some()
        });

        assert_eq!(
            stored_favorites(&settings, &rt),
            Some(vec![SUBSCRIBER.to_owned()])
        );
    }

    #[gpui::test]
    fn the_event_picker_opens_with_the_favorites_already_stored(cx: &mut TestAppContext) {
        let rt = runtime();
        let settings = Arc::new(MemorySettings::new());
        rt.block_on(forge_storage::write_setting(
            settings.as_ref(),
            TRIGGER_FAVORITES.setting,
            &vec![FOLLOW.to_owned()],
        ))
        .expect("the memory store writes");

        let picker = open_wiring_picker(&settings, &rt, cx);
        picker.update(cx, |_, cx| {
            cx.emit(GridPickerEvent::FavoriteToggled(SUBSCRIBER.into()));
        });
        pump_until(&rt, || {
            cx.run_until_parked();
            stored_favorites(&settings, &rt).is_some_and(|ids| ids.len() == 2)
        });

        let mut stored = stored_favorites(&settings, &rt).unwrap_or_default();
        stored.sort();
        assert_eq!(stored, [FOLLOW, SUBSCRIBER]);
    }

    fn checking_on(view: &Entity<EventWiringView>, cx: &mut TestAppContext, kind_id: &str) {
        view.update(cx, |this, _| {
            this.overlay = Some(overlay_definition(alert::KIND_ID, OverlayConfig::new()));
            this.stage = Some(WiringStage::Confirm(ConfirmStage {
                trigger_kind_id: kind_id.to_owned(),
                trigger_label: "Subscribed".to_owned(),
                state: ConfirmState::Checking,
                writing: false,
            }));
        });
    }

    fn confirm_stage<T>(
        view: &Entity<EventWiringView>,
        cx: &mut TestAppContext,
        read: impl FnOnce(&ConfirmStage) -> T,
    ) -> T {
        view.read_with(cx, |this, _| match this.stage.as_ref() {
            Some(WiringStage::Confirm(stage)) => read(stage),
            _ => panic!("the confirmation stage is gone"),
        })
    }

    #[gpui::test]
    fn a_check_that_finds_an_existing_action_offers_it_instead_of_a_second_wiring(
        cx: &mut TestAppContext,
    ) {
        let (view, _rt) = mount(cx);
        checking_on(&view, cx, SUBSCRIBER);
        let already = ActionId::new();

        view.update(cx, |this, cx| {
            this.settle_check(SUBSCRIBER, Check::Wired(already), cx)
        });

        assert!(confirm_stage(&view, cx, |stage| matches!(
            stage.state,
            ConfirmState::AlreadyWired { action_id, .. } if action_id == already
        )));
    }

    #[gpui::test]
    fn a_check_answering_for_an_event_the_user_has_left_behind_is_discarded(
        cx: &mut TestAppContext,
    ) {
        let (view, _rt) = mount(cx);
        checking_on(&view, cx, SUBSCRIBER);

        view.update(cx, |this, cx| {
            this.settle_check(FOLLOW, Check::Wired(ActionId::new()), cx)
        });

        assert!(confirm_stage(&view, cx, |stage| matches!(
            stage.state,
            ConfirmState::Checking
        )));
    }

    #[gpui::test]
    fn create_on_a_stage_that_already_found_an_action_starts_no_write(cx: &mut TestAppContext) {
        let (view, _rt) = mount(cx);
        checking_on(&view, cx, SUBSCRIBER);
        view.update(cx, |this, cx| {
            this.settle_check(SUBSCRIBER, Check::Wired(ActionId::new()), cx)
        });

        view.update(cx, |this, cx| this.confirm(cx));

        assert!(!confirm_stage(&view, cx, |stage| stage.writing));
    }
}
