use super::trigger_fill::{TriggerFillDeps, TriggerFillEvent, TriggerFillModal};
use super::*;
use crate::app::async_bridge;
use crate::automation::triggers_screen::{platform_dot_color, platform_kind_line};
use crate::shared::picker_favorites::{bind_picker, follow_picker_event};
use crate::shared::repo_failure::RepoFailure;
use forge_components::GridScope;
use forge_components::focus_inside_modal;
use forge_components::{
    ActivePresentation, ForgePalette, GlyphArt, GridPicker, GridPickerArt, GridPickerConfig,
    GridPickerEvent, GridPickerGroup, GridPickerItem, GridPickerItemState, GridPickerSubtitle,
    Icon, OverlayPosition, overlay, tr,
};
use forge_registry::TriggerRegistry;
use forge_types::{TriggerInstance, TriggerInstanceId};
use gpui::{AnyElement, Context, Entity, SharedString, Window};
use std::collections::HashMap;

fn build_recent_group(
    instances: &[TriggerInstance],
    registry: &TriggerRegistry,
    palette: &ForgePalette,
) -> (
    Option<GridPickerGroup>,
    HashMap<SharedString, TriggerInstanceId>,
) {
    let mut picks: HashMap<SharedString, TriggerInstanceId> = HashMap::new();
    if instances.is_empty() {
        return (None, picks);
    }
    let mut items: Vec<GridPickerItem> = Vec::with_capacity(instances.len());
    for instance in instances {
        let descriptor = registry.get(&instance.kind_id);
        let id = SharedString::from(format!("recent-{}", instance.id));
        picks.insert(id.clone(), instance.id);
        let glyph = Icon::from_name(descriptor.map(|kind| kind.icon_name()).unwrap_or("bolt"));
        let kind_label = platform_kind_line(&instance.kind_id, registry);
        let condition =
            crate::automation::triggers_screen::condition_display(descriptor, &instance.overrides);
        let desc = if condition.is_empty() {
            kind_label
        } else {
            format!("{kind_label} \u{b7} {condition}")
        };
        let state = if instance.enabled {
            GridPickerItemState::Normal
        } else {
            GridPickerItemState::Disabled
        };
        items.push(GridPickerItem {
            id,
            glyph: GlyphArt::Icon(glyph),
            tint: platform_dot_color(&instance.kind_id, registry, palette),
            name: crate::shared::kind_copy::trigger_instance_name(&instance.name, descriptor)
                .into(),
            desc: desc.into(),
            state,
            keywords: descriptor
                .map(|kind| kind.search_text().to_owned())
                .unwrap_or_default()
                .into(),
            matches: None,
        });
    }
    let group = GridPickerGroup {
        label: tr!("action_editor_recent_triggers").into(),
        dot_color: palette.warning,
        scope: GridScope::Leading,
        items,
    };
    (Some(group), picks)
}

impl ActionEditor {
    pub(super) fn open_trigger_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(action_id) = self.selected else {
            return;
        };
        if self.detail.is_none() {
            return;
        }
        self.step_open = None;
        let service = Arc::clone(&self.deps.actions_service);
        let reply = crate::app::async_bridge::on_runtime(&self.deps.rt_handle, async move {
            service
                .list_linkable_triggers(action_id)
                .await
                .map_err(RepoFailure::from)
        });
        cx.spawn_in(window, async move |this, cx| match reply.await {
            Ok(Ok(instances)) => {
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the view may close before the result arrives"
                )]
                let _ = this.update_in(cx, |this, window, cx| {
                    this.open_trigger_picker_with(action_id, instances, window, cx)
                });
            }
            Ok(Err(message)) => {
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the view may close before the result arrives"
                )]
                let _ = this.update(cx, |_, cx| on_repo_error(&message, cx));
            }
            Err(_) => {}
        })
        .detach();
    }

    fn open_trigger_picker_with(
        &mut self,
        action_id: ActionId,
        instances: Vec<TriggerInstance>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected != Some(action_id) || self.detail.is_none() {
            return;
        }
        let palette = cx.palette();
        let action_name = self
            .detail
            .as_ref()
            .map(|d| d.action.name.clone())
            .unwrap_or_else(|| tr!("action_editor_this_action"));
        let (mut kind_groups, picks_kind) = crate::automation::triggers_screen::build_kind_groups(
            &self.deps.trigger_registry,
            &palette,
        );
        let (recent_group, picks_instance) =
            build_recent_group(&instances, &self.deps.trigger_registry, &palette);
        let mut groups = Vec::with_capacity(kind_groups.len() + 1);
        groups.extend(recent_group);
        groups.append(&mut kind_groups);
        let count = self.deps.trigger_registry.all().count();
        let config = GridPickerConfig {
            accent: palette.warning,
            art: GridPickerArt::default(),
            header_icon: Icon::Bolt,
            title: tr!("action_editor_add_trigger").into(),
            subtitle: GridPickerSubtitle::Context {
                lead: tr!("action_editor_picker_fires").into(),
                name: action_name.into(),
                note: tr!(
                    "action_editor_picker_available_count",
                    count = i64::try_from(count).unwrap_or(i64::MAX)
                )
                .into(),
            },
            footer_hint: tr!("action_editor_trigger_picker_footer_hint").into(),
            search_placeholder: tr!("triggers_search_placeholder").into(),
            favorites_label: tr!("picker_favorites").into(),
            favorites_empty: tr!("picker_favorites_empty").into(),
        };
        let favorites = self
            .trigger_favorites
            .read(cx)
            .ids()
            .cloned()
            .unwrap_or_default();
        let picker = cx.new(|cx| GridPicker::new(config, groups, favorites, cx));
        let sub = cx.subscribe(&picker, Self::on_trigger_picker_event);
        let favorites_sub = bind_picker(&self.trigger_favorites, &picker, cx);
        focus_inside_modal(window, cx, |window, cx| {
            picker.update(cx, |f, cx| f.focus(window, cx))
        });
        self.modals.add_trigger = Some(AddTriggerStage::Pick(AddTriggerPicker {
            picker,
            picks_kind,
            picks_instance,
            action_id,
            _sub: sub,
            _favorites: favorites_sub,
        }));
        cx.notify();
    }

    fn on_trigger_picker_event(
        &mut self,
        _picker: Entity<GridPicker>,
        event: &GridPickerEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            GridPickerEvent::Picked(id) => {
                let Some(AddTriggerStage::Pick(picker)) = self.modals.add_trigger.as_ref() else {
                    return;
                };
                let action_id = picker.action_id;
                if let Some(instance_id) = picker.picks_instance.get(id).copied() {
                    self.link_existing_trigger(action_id, instance_id, cx);
                } else if let Some(kind_id) = picker.picks_kind.get(id).cloned() {
                    self.enter_trigger_fill(action_id, kind_id, cx);
                }
            }
            GridPickerEvent::FavoriteToggled(_) | GridPickerEvent::FavoritesRetry => {
                follow_picker_event(&self.trigger_favorites, event, cx);
            }
            GridPickerEvent::Dismissed => self.cancel_trigger_picker(cx),
        }
    }

    pub(in crate::automation::actions_screen) fn cancel_trigger_picker(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.modals.add_trigger = None;
        cx.notify();
    }

    fn link_existing_trigger(
        &mut self,
        action_id: ActionId,
        instance_id: TriggerInstanceId,
        cx: &mut Context<Self>,
    ) {
        self.modals.add_trigger = None;
        if self.selected != Some(action_id) {
            cx.notify();
            return;
        }
        let service = Arc::clone(&self.deps.actions_service);
        async_bridge::run_async(
            &self.deps.rt_handle,
            async move {
                service
                    .link_trigger_instance(action_id, instance_id)
                    .await
                    .map_err(RepoFailure::from)
            },
            |this, result, cx| match result {
                Ok(()) => this.reload_detail(cx),
                Err(failure) => on_repo_error(&failure, cx),
            },
            cx,
        );
        cx.notify();
    }

    pub(super) fn render_add_trigger(
        &self,
        stage: &AddTriggerStage,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match stage {
            AddTriggerStage::Pick(picker) => {
                let view = cx.entity();
                overlay(picker.picker.clone(), palette)
                    .position(OverlayPosition::Center)
                    .on_dismiss("actions-trigger-grid-scrim", move |_window, cx| {
                        view.update(cx, |this, cx| this.cancel_trigger_picker(cx));
                    })
                    .into_any_element()
            }
            AddTriggerStage::Fill(host) => host.form.clone().into_any_element(),
        }
    }

    fn enter_trigger_fill(&mut self, action_id: ActionId, kind_id: String, cx: &mut Context<Self>) {
        let deps = TriggerFillDeps {
            editor: cx.weak_entity(),
            trigger_registry: Arc::clone(&self.deps.trigger_registry),
            actions_service: Arc::clone(&self.deps.actions_service),
            rt_handle: self.deps.rt_handle.clone(),
        };
        let form = cx.new(|cx| TriggerFillModal::new(deps, action_id, kind_id, cx));
        let sub = cx.subscribe(&form, Self::on_trigger_fill_event);
        self.modals.add_trigger = Some(AddTriggerStage::Fill(TriggerFillHost { form, _sub: sub }));
        self.start_option_choices(cx);
        cx.notify();
    }

    fn on_trigger_fill_event(
        &mut self,
        form: Entity<TriggerFillModal>,
        event: &TriggerFillEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            TriggerFillEvent::Cancelled => self.cancel_trigger_picker(cx),
            TriggerFillEvent::SaveRequested(action_id) => {
                if self.selected == Some(*action_id) {
                    form.update(cx, |form, cx| form.save(cx));
                } else {
                    self.cancel_trigger_picker(cx);
                }
            }
            TriggerFillEvent::Created => {
                self.cancel_trigger_picker(cx);
                self.reload_detail(cx);
            }
        }
    }

    pub(in crate::automation::actions_screen) fn submit_trigger_fill(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if let Some(AddTriggerStage::Fill(host)) = self.modals.add_trigger.as_ref() {
            host.form.update(cx, |form, cx| form.request_save(cx));
        }
    }
}
