use super::mapping_store::{load_mappings, save_mapping};
use super::{Capture, MidiScreenView, OpenModal};
use crate::async_bridge;
use crate::midi_mapping_modal::{
    MappingDraft, MappingModalLaunch, MidiMappingModal, MidiMappingModalEvent,
};
use crate::midi_signal::MidiSignal;
use crate::repo_failure::RepoFailure;
use forge_components::{PushToast, tr};
use forge_types::{ActionId, TriggerInstance, TriggerInstanceId};
use gpui::{App, Context, Entity, Pixels, Point, prelude::*};
use std::sync::Arc;

impl MidiScreenView {
    pub(super) fn start_learn(&mut self, cx: &mut Context<Self>) {
        self.capture = Capture::Learn;
        let weak = cx.entity().downgrade();
        self.escape_sub = Some(cx.intercept_keystrokes(move |event, _window, cx| {
            if event.keystroke.key != forge_components::ESCAPE_KEY
                || event.keystroke.modifiers.modified()
            {
                return;
            }
            let handled = weak
                .update(cx, |this, cx| {
                    let armed = this.capture == Capture::Learn;
                    if armed {
                        this.cancel_learn(cx);
                    }
                    armed
                })
                .unwrap_or(false);
            if handled {
                cx.stop_propagation();
            }
        }));
        cx.notify();
    }

    pub(super) fn cancel_learn(&mut self, cx: &mut Context<Self>) {
        self.capture = Capture::Off;
        self.escape_sub = None;
        cx.notify();
    }

    pub(super) fn open_modal(
        &mut self,
        instance_id: Option<TriggerInstanceId>,
        signal: MidiSignal,
        linked_action: Option<ActionId>,
        cx: &mut Context<Self>,
    ) {
        let launch = MappingModalLaunch {
            instance_id,
            signal,
            linked_action,
            devices: self.known().to_vec(),
            input_enabled: self.enabled,
        };
        let action_repo = Arc::clone(&self.action_repo);
        let rt_handle = self.rt_handle.clone();
        let view = cx.new(|cx| MidiMappingModal::new(launch, action_repo, rt_handle, cx));
        let sub = cx.subscribe(&view, Self::on_modal_event);
        self.modal = Some(OpenModal { view, _sub: sub });
        cx.notify();
    }

    pub(super) fn edit_mapping(&mut self, id: TriggerInstanceId, cx: &mut Context<Self>) {
        self.menu_open = None;
        let Some(row) = self.mappings.iter().find(|row| row.id == id) else {
            cx.notify();
            return;
        };
        let signal = row.signal.clone();
        let linked = row.action.as_ref().map(|(action_id, _)| *action_id);
        self.open_modal(Some(id), signal, linked, cx);
    }

    fn on_modal_event(
        &mut self,
        _view: Entity<MidiMappingModal>,
        event: &MidiMappingModalEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            MidiMappingModalEvent::Relearn => {
                self.capture = Capture::Relearn;
                cx.notify();
            }
            MidiMappingModalEvent::Cancel => self.close_modal(cx),
            MidiMappingModalEvent::Delete(id) => {
                let id = *id;
                self.close_modal(cx);
                self.delete(id, cx);
            }
            MidiMappingModalEvent::Save(draft) => {
                let draft = MappingDraft {
                    instance_id: draft.instance_id,
                    signal: draft.signal.clone(),
                    action_id: draft.action_id,
                    name: draft.name.clone(),
                };
                self.close_modal(cx);
                self.persist_mapping(draft, cx);
            }
        }
    }

    fn close_modal(&mut self, cx: &mut Context<Self>) {
        self.modal = None;
        self.capture = Capture::Off;
        self.escape_sub = None;
        cx.notify();
    }

    fn persist_mapping(&mut self, draft: MappingDraft, cx: &mut Context<Self>) {
        let saved_device = draft.signal.device.clone();
        let triggers = Arc::clone(&self.trigger_repo);
        let actions = Arc::clone(&self.action_repo);
        async_bridge::run_async(
            &self.rt_handle,
            async move { save_mapping(&*triggers, &*actions, draft).await },
            move |this, result, cx| {
                if result.is_ok() {
                    this.follow_saved_device(saved_device.as_deref());
                }
                this.apply_mappings(result, cx);
            },
            cx,
        );
    }

    pub(super) fn toggle_menu(
        &mut self,
        id: TriggerInstanceId,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.menu_open = if self.menu_open == Some(id) {
            None
        } else {
            self.menu_click_pos = Some(position);
            Some(id)
        };
        cx.notify();
    }

    pub(super) fn close_menu(&mut self, cx: &mut Context<Self>) {
        self.menu_open = None;
        cx.notify();
    }

    pub(super) fn duplicate(&mut self, id: TriggerInstanceId, cx: &mut Context<Self>) {
        self.menu_open = None;
        cx.notify();
        let Some(copy_name) = self
            .mappings
            .iter()
            .find(|row| row.id == id)
            .map(|row| tr!("triggers_template_copy_name", name = row.name.as_str()))
        else {
            return;
        };
        let triggers = Arc::clone(&self.trigger_repo);
        let actions = Arc::clone(&self.action_repo);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let source = triggers
                    .get(id)
                    .await
                    .map_err(RepoFailure::from)?
                    .ok_or_else(RepoFailure::trigger_gone)?;
                let copy = TriggerInstance {
                    id: TriggerInstanceId::new(),
                    name: copy_name,
                    user_defined: true,
                    ..source
                };
                triggers.save(&copy).await.map_err(RepoFailure::from)?;
                load_mappings(&*triggers, &*actions).await
            },
            |this, result, cx| this.apply_mappings(result, cx),
            cx,
        );
    }

    pub(super) fn delete(&mut self, id: TriggerInstanceId, cx: &mut Context<Self>) {
        self.menu_open = None;
        cx.notify();
        let Some(name) = self
            .mappings
            .iter()
            .find(|row| row.id == id)
            .map(|row| row.name.clone())
        else {
            return;
        };
        let triggers = Arc::clone(&self.trigger_repo);
        let actions = Arc::clone(&self.action_repo);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                triggers.archive(id).await.map_err(RepoFailure::from)?;
                load_mappings(&*triggers, &*actions).await
            },
            move |this, result, cx| {
                let archived = result.is_ok();
                this.apply_mappings(result, cx);
                if archived {
                    this.raise_undo_toast(id, name, cx);
                }
            },
            cx,
        );
    }

    fn raise_undo_toast(&self, id: TriggerInstanceId, name: String, cx: &mut Context<Self>) {
        let view = cx.entity();
        let triggers = Arc::clone(&self.trigger_repo);
        let actions = Arc::clone(&self.action_repo);
        let rt_handle = self.rt_handle.clone();
        cx.push_undo_toast(
            tr!("triggers_toast_deleted", name = name.as_str()),
            move |_window, app: &mut App| {
                let triggers = Arc::clone(&triggers);
                let actions = Arc::clone(&actions);
                async_bridge::run_async_entity(
                    &rt_handle,
                    view.clone(),
                    async move {
                        triggers.restore(id).await.map_err(RepoFailure::from)?;
                        load_mappings(&*triggers, &*actions).await
                    },
                    |this, result, cx| this.apply_mappings(result, cx),
                    app,
                );
            },
        );
    }
}
