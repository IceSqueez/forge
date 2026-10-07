use gpui::{Context, Entity, Subscription, prelude::*};

use super::HotkeysScreenView;
use super::capture::Capture;
use crate::actions::ShortcutEntry;
use crate::app_shortcut_modal::{AppShortcutModal, AppShortcutModalEvent};

pub(super) struct OpenAppModal {
    id: &'static str,
    steal: Option<&'static str>,
    pub(super) view: Entity<AppShortcutModal>,
    _sub: Subscription,
}

impl HotkeysScreenView {
    pub(super) fn reset_app_binding(&mut self, id: &'static str, cx: &mut Context<Self>) {
        self.menu_open = None;
        self.shortcuts.reset(id);
        self.persist_shortcuts(cx);
    }

    pub(super) fn unbind_app_binding(&mut self, id: &'static str, cx: &mut Context<Self>) {
        self.menu_open = None;
        self.shortcuts.unbind(id);
        self.persist_shortcuts(cx);
    }

    pub(super) fn toggle_app_binding(&mut self, id: &'static str, cx: &mut Context<Self>) {
        let enabled = !self.shortcuts.is_enabled(id);
        self.shortcuts.set_enabled(id, enabled);
        self.persist_shortcuts(cx);
    }

    pub(super) fn edit_app_binding(
        &mut self,
        entry: &'static ShortcutEntry,
        cx: &mut Context<Self>,
    ) {
        self.menu_open = None;
        let chord = self.shortcuts.chord_of(entry).map(str::to_owned);
        let view = cx.new(|_| AppShortcutModal::new(entry, chord));
        let sub = cx.subscribe(&view, Self::on_app_modal_event);
        self.app_modal = Some(OpenAppModal {
            id: entry.id,
            steal: None,
            view,
            _sub: sub,
        });
        cx.notify();
    }

    pub(super) fn push_app_modal_chord(
        &mut self,
        chord: String,
        steal: Option<&'static str>,
        cx: &mut Context<Self>,
    ) {
        if let Some(open) = &mut self.app_modal {
            open.steal = steal;
        }
        if let Some(open) = &self.app_modal {
            open.view
                .update(cx, |modal, cx| modal.apply_capture(chord, cx));
        }
        cx.notify();
    }

    fn on_app_modal_event(
        &mut self,
        _view: Entity<AppShortcutModal>,
        event: &AppShortcutModalEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.app_modal.as_ref().map(|open| open.id) else {
            return;
        };
        match event {
            AppShortcutModalEvent::Recapture => self.start_capture(Capture::AppModal(id), cx),
            AppShortcutModalEvent::Cancel => self.close_app_modal(cx),
            AppShortcutModalEvent::Save(chord) => {
                let chord = chord.clone();
                let steal = self.app_modal.as_ref().and_then(|open| open.steal);
                self.close_app_modal(cx);
                if let Some(owner) = steal {
                    self.shortcuts.unbind(owner);
                }
                match chord {
                    Some(chord) => self.shortcuts.bind(id, chord),
                    None => self.shortcuts.unbind(id),
                }
                self.persist_shortcuts(cx);
            }
        }
    }

    fn close_app_modal(&mut self, cx: &mut Context<Self>) {
        self.app_modal = None;
        if matches!(self.capture, Capture::AppModal(_)) {
            self.end_capture();
        }
        cx.notify();
    }
}
