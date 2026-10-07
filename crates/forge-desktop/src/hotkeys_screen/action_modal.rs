use forge_types::{ActionId, TriggerInstanceId};
use gpui::{Context, Entity, Subscription, prelude::*};

use super::HotkeysScreenView;
use super::capture::Capture;
use crate::hotkey_action_modal::{
    ActionModalLaunch, BindingDraft, HotkeyActionModal, HotkeyActionModalEvent,
};
use crate::hotkey_bindings::HotkeyEdge;

pub(super) struct OpenModal {
    pub(super) view: Entity<HotkeyActionModal>,
    editing: Option<TriggerInstanceId>,
    _sub: Subscription,
}

impl HotkeysScreenView {
    pub(super) fn open_add_modal(&mut self, combo: String, cx: &mut Context<Self>) {
        let locked = self.locked_edge_for(&combo);
        let edge = locked.map_or(HotkeyEdge::Press, HotkeyEdge::opposite);
        self.open_modal(None, combo, edge, locked, None, cx);
    }

    fn open_modal(
        &mut self,
        instance_id: Option<TriggerInstanceId>,
        combo: String,
        edge: HotkeyEdge,
        locked_edge: Option<HotkeyEdge>,
        linked_action: Option<ActionId>,
        cx: &mut Context<Self>,
    ) {
        let launch = ActionModalLaunch {
            instance_id,
            combo,
            edge,
            locked_edge,
            linked_action,
        };
        let action_repo = self.backend.action_repo();
        let rt_handle = self.rt_handle.clone();
        let view = cx.new(|cx| HotkeyActionModal::new(launch, action_repo, rt_handle, cx));
        let sub = cx.subscribe(&view, Self::on_modal_event);
        self.modal = Some(OpenModal {
            view,
            editing: instance_id,
            _sub: sub,
        });
        cx.notify();
    }

    fn on_modal_event(
        &mut self,
        _view: Entity<HotkeyActionModal>,
        event: &HotkeyActionModalEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            HotkeyActionModalEvent::Recapture => {
                let editing = self.modal.as_ref().and_then(|open| open.editing);
                self.start_capture(Capture::Modal(editing), cx);
            }
            HotkeyActionModalEvent::Cancel => self.close_modal(cx),
            HotkeyActionModalEvent::Save(draft) => {
                let draft = BindingDraft {
                    instance_id: draft.instance_id,
                    combo: draft.combo.clone(),
                    edge: draft.edge,
                    action_id: draft.action_id,
                };
                self.close_modal(cx);
                self.persist_draft(draft, cx);
            }
        }
    }

    fn close_modal(&mut self, cx: &mut Context<Self>) {
        self.modal = None;
        self.end_capture();
        cx.notify();
    }

    pub(super) fn edit_half(&mut self, instance_id: TriggerInstanceId, cx: &mut Context<Self>) {
        self.menu_open = None;
        let Some(row) = self.row_of_instance(instance_id) else {
            cx.notify();
            return;
        };
        let combo = row.combo.clone();
        let Some((edge, half)) = row
            .halves()
            .find(|(_, half)| half.instance_id == instance_id)
        else {
            cx.notify();
            return;
        };
        let linked = half.action.as_ref().map(|(id, _)| *id);
        let locked = row.free_edge().is_none().then(|| edge.opposite());
        self.open_modal(Some(instance_id), combo, edge, locked, linked, cx);
    }

    pub(super) fn add_half(&mut self, key: TriggerInstanceId, cx: &mut Context<Self>) {
        self.menu_open = None;
        let Some(row) = self.row_by_key(key) else {
            cx.notify();
            return;
        };
        let Some(edge) = row.free_edge() else {
            cx.notify();
            return;
        };
        let combo = row.combo.clone();
        self.open_modal(None, combo, edge, Some(edge.opposite()), None, cx);
    }
}
