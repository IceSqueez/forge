use super::*;
use crate::async_bridge;
use crate::presentation::ActivePresentation;
use crate::triggers_screen::platform_dot_color;
use forge_components::{
    ForgePalette, GlyphArt, GridPicker, GridPickerArt, GridPickerConfig, GridPickerEvent,
    GridPickerGroup, GridPickerItem, GridPickerItemState, GridPickerSubtitle, Icon,
    OverlayPosition, overlay, tr,
};
use forge_registry::{TriggerKindDescriptor, TriggerRegistry};
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
        let glyph = Icon::from_name(
            descriptor
                .map(TriggerKindDescriptor::icon_name)
                .unwrap_or("bolt"),
        );
        let kind_label = descriptor
            .map(|d| d.label().to_owned())
            .unwrap_or_else(|| instance.kind_id.clone());
        let condition = crate::triggers_screen::condition_display(
            descriptor,
            &instance.kind_id,
            &instance.overrides,
        );
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
            tint: platform_dot_color(&instance.kind_id, palette),
            name: instance.name.clone().into(),
            desc: desc.into(),
            state,
            matches: None,
        });
    }
    let group = GridPickerGroup {
        label: tr!("action_editor_recent_triggers").into(),
        dot_color: palette.warning,
        scope: SharedString::from("all"),
        items,
    };
    (Some(group), picks)
}

impl ScreenActionsView {
    pub(super) fn open_trigger_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(action_id) = self.selected else {
            return;
        };
        if self.detail.is_none() {
            return;
        }
        self.step_menu_open = None;
        let service = Arc::clone(&self.actions_service);
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let _ = tx.send(
                service
                    .list_linkable_triggers(action_id)
                    .await
                    .map_err(|e| e.to_string()),
            );
        });
        cx.spawn_in(window, async move |this, cx| match rx.await {
            Ok(Ok(instances)) => {
                let _ = this.update_in(cx, |this, window, cx| {
                    this.open_trigger_picker_with(action_id, instances, window, cx)
                });
            }
            Ok(Err(message)) => {
                let _ = this.update(cx, |this, cx| this.on_repo_error(&message, cx));
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
        let (mut kind_groups, picks_kind) =
            crate::triggers_screen::build_kind_groups(&self.trigger_registry, &palette);
        let (recent_group, picks_instance) =
            build_recent_group(&instances, &self.trigger_registry, &palette);
        let mut groups = Vec::with_capacity(kind_groups.len() + 1);
        groups.extend(recent_group);
        groups.append(&mut kind_groups);
        let count = self.trigger_registry.all().count();
        let config = GridPickerConfig {
            accent: palette.warning,
            art: GridPickerArt::default(),
            header_icon: Icon::Bolt,
            title: tr!("action_editor_add_trigger").into(),
            subtitle: GridPickerSubtitle::Context {
                lead: tr!("action_editor_picker_fires").into(),
                name: action_name.into(),
                note: tr!("action_editor_picker_available_count", count = count as i64).into(),
            },
            footer_hint: tr!("action_editor_trigger_picker_footer_hint").into(),
            search_placeholder: tr!("triggers_search_placeholder").into(),
            favorites_label: tr!("picker_favorites").into(),
            favorites_empty: tr!("picker_favorites_empty").into(),
        };
        let favorites = self.trigger_favorites.clone();
        let picker = cx.new(|cx| GridPicker::new(config, groups, favorites, palette, cx));
        let sub = cx.subscribe(&picker, Self::on_trigger_picker_event);
        picker.update(cx, |f, cx| f.focus(window, cx));
        self.add_trigger = Some(AddTriggerStage::Pick(AddTriggerPicker {
            picker,
            picks_kind,
            picks_instance,
            action_id,
            _sub: sub,
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
                let Some(AddTriggerStage::Pick(picker)) = self.add_trigger.as_ref() else {
                    return;
                };
                let action_id = picker.action_id;
                if let Some(instance_id) = picker.picks_instance.get(id).copied() {
                    self.link_existing_trigger(action_id, instance_id, cx);
                } else if let Some(kind_id) = picker.picks_kind.get(id).cloned() {
                    self.enter_trigger_fill(action_id, kind_id, cx);
                }
            }
            GridPickerEvent::FavoriteToggled(id) => {
                if self.trigger_favorites.contains(id) {
                    self.trigger_favorites.remove(id);
                } else {
                    self.trigger_favorites.insert(id.clone());
                }
                let favorites = self.trigger_favorites.clone();
                self.persist_favorites(reserved_keys::PICKER_FAVORITES_TRIGGERS, favorites, cx);
            }
            GridPickerEvent::Dismissed => self.cancel_trigger_picker(cx),
        }
    }

    pub(in crate::actions_screen) fn cancel_trigger_picker(&mut self, cx: &mut Context<Self>) {
        self.add_trigger = None;
        cx.notify();
    }

    fn link_existing_trigger(
        &mut self,
        action_id: ActionId,
        instance_id: TriggerInstanceId,
        cx: &mut Context<Self>,
    ) {
        self.add_trigger = None;
        if self.selected != Some(action_id) {
            cx.notify();
            return;
        }
        let service = Arc::clone(&self.actions_service);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                service
                    .link_trigger_instance(action_id, instance_id)
                    .await
                    .map_err(|e| e.to_string())
            },
            |this, result, cx| match result {
                Ok(()) => this.reload_detail(cx),
                Err(message) => this.on_repo_error(&message, cx),
            },
            cx,
        );
        cx.notify();
    }

    pub(in crate::actions_screen) fn render_add_trigger(
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
            AddTriggerStage::Fill(form) => self.render_trigger_fill(form, palette, cx),
        }
    }
}
