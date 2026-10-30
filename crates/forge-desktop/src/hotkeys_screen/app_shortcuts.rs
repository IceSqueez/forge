use gpui::{Context, Entity, Subscription, prelude::*};

use super::HotkeysScreenView;
use super::capture::Capture;
use crate::actions::ShortcutEntry;
use crate::app_shortcut_modal::{AppShortcutModal, AppShortcutModalEvent};
use crate::shortcut_overrides::ShortcutOverrides;

pub(super) struct OpenAppModal {
    id: &'static str,
    steal: Option<&'static str>,
    pub(super) view: Entity<AppShortcutModal>,
    _sub: Subscription,
}

impl HotkeysScreenView {
    pub(super) fn reset_app_binding(&mut self, id: &'static str, cx: &mut Context<Self>) {
        self.menu_open = None;
        self.edit_shortcuts(cx, |shortcuts| shortcuts.reset(id));
    }

    pub(super) fn unbind_app_binding(&mut self, id: &'static str, cx: &mut Context<Self>) {
        self.menu_open = None;
        self.edit_shortcuts(cx, |shortcuts| shortcuts.unbind(id));
    }

    pub(super) fn toggle_app_binding(&mut self, id: &'static str, cx: &mut Context<Self>) {
        self.edit_shortcuts(cx, |shortcuts| {
            let enabled = !shortcuts.is_enabled(id);
            shortcuts.set_enabled(id, enabled);
        });
    }

    pub(super) fn edit_shortcuts(
        &mut self,
        cx: &mut Context<Self>,
        mutate: impl FnOnce(&mut ShortcutOverrides),
    ) {
        let Some(shortcuts) = self.shortcuts.loaded_mut() else {
            cx.notify();
            return;
        };
        mutate(shortcuts);
        self.persist_shortcuts(cx);
    }

    pub(super) fn edit_app_binding(
        &mut self,
        entry: &'static ShortcutEntry,
        cx: &mut Context<Self>,
    ) {
        self.menu_open = None;
        let Some(shortcuts) = self.shortcuts.loaded() else {
            cx.notify();
            return;
        };
        let chord = shortcuts.resolved_chord_of(entry);
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
                self.edit_shortcuts(cx, |shortcuts| {
                    if let Some(owner) = steal {
                        shortcuts.unbind(owner);
                    }
                    match chord {
                        Some(chord) => shortcuts.bind(id, chord),
                        None => shortcuts.unbind(id),
                    }
                });
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
