use forge_components::{ConfirmTone, ForgePalette, OverlayPosition, confirm_modal, overlay, tr};
use gpui::{AnyElement, AnyWindowHandle, App, ClickEvent, IntoElement, SharedString, Window};

use crate::screen::Screen;
use crate::unsaved_work::UnsavedWorkHandle;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeaveTarget {
    Screen(Screen),
    CloseWindow,
}

enum PendingLeave {
    Asking(LeaveTarget),
    Saving(LeaveTarget),
}

pub struct NavigationGuard {
    window: AnyWindowHandle,
    work: Option<UnsavedWorkHandle>,
    pending: Option<PendingLeave>,
}

impl NavigationGuard {
    pub fn new(window: AnyWindowHandle) -> Self {
        Self {
            window,
            work: None,
            pending: None,
        }
    }

    pub fn watch(&mut self, work: Option<UnsavedWorkHandle>) {
        self.work = work;
        self.pending = None;
    }

    pub fn blocks(&self, cx: &App) -> bool {
        self.work
            .as_ref()
            .is_some_and(|work| work.has_unsaved_work(cx))
    }

    pub fn ask(&mut self, target: LeaveTarget) {
        self.pending = Some(PendingLeave::Asking(target));
    }

    pub fn is_asking(&self) -> bool {
        matches!(self.pending, Some(PendingLeave::Asking(_)))
    }

    pub fn work_name(&self, cx: &App) -> Option<SharedString> {
        self.work.as_ref().and_then(|work| work.name(cx))
    }

    pub fn cancel(&mut self) {
        self.pending = None;
    }

    fn take_asked(&mut self) -> Option<LeaveTarget> {
        match self.pending.take() {
            Some(PendingLeave::Asking(target)) => Some(target),
            other => {
                self.pending = other;
                None
            }
        }
    }

    pub fn discard(&mut self, cx: &mut App) -> Option<LeaveTarget> {
        let target = self.take_asked()?;
        if let Some(work) = self.work.clone() {
            work.discard(cx);
        }
        Some(target)
    }

    pub fn save(&mut self, cx: &mut App) -> Option<LeaveTarget> {
        let target = self.take_asked()?;
        let Some(work) = self.work.clone() else {
            return Some(target);
        };
        if work.start_saving(cx) {
            self.pending = Some(PendingLeave::Saving(target));
            return None;
        }
        (!work.has_unsaved_work(cx)).then_some(target)
    }

    pub fn settle(&mut self, cx: &App) -> Option<LeaveTarget> {
        let work = self.work.as_ref()?;
        if !matches!(self.pending, Some(PendingLeave::Saving(_))) || work.is_saving(cx) {
            return None;
        }
        let saved = !work.has_unsaved_work(cx);
        match self.pending.take() {
            Some(PendingLeave::Saving(target)) if saved => Some(target),
            _ => None,
        }
    }

    pub fn close_window(&self, cx: &mut App) {
        let window = self.window;
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, window, _| window.remove_window());
        });
    }
}

pub struct LeaveDialogHandlers<C, S, D, X> {
    pub cancel: C,
    pub save: S,
    pub discard: D,
    pub dismiss: X,
}

pub fn leave_dialog<C, S, D, X>(
    name: Option<SharedString>,
    palette: &ForgePalette,
    handlers: LeaveDialogHandlers<C, S, D, X>,
) -> AnyElement
where
    C: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    S: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    D: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    X: Fn(&mut Window, &mut App) + 'static,
{
    let mut card = confirm_modal(
        tr!("shell_unsaved_leave_title"),
        tr!("shell_unsaved_leave_body"),
        ConfirmTone::Warning,
        palette,
    )
    .on_cancel(
        "shell-unsaved-leave-cancel",
        tr!("shell_unsaved_leave_cancel"),
        handlers.cancel,
    )
    .on_alternate(
        "shell-unsaved-leave-save",
        tr!("shell_unsaved_leave_save"),
        handlers.save,
    )
    .on_confirm(
        "shell-unsaved-leave-discard",
        tr!("shell_unsaved_leave_discard"),
        handlers.discard,
    );
    if let Some(name) = name {
        card = card.item_name(name);
    }

    overlay(card, palette)
        .position(OverlayPosition::Center)
        .on_dismiss("shell-unsaved-leave-dismiss", handlers.dismiss)
        .into_any_element()
}
