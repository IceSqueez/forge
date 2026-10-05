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

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use forge_components::FORGE_DEFAULT;
    use forge_storage::Language;
    use gpui::{
        AppContext, Context, Entity, Modifiers, ParentElement, Render, Styled, TestAppContext,
        VisualTestContext, div, point, px, size,
    };

    use super::*;
    use crate::i18n::install_language;
    use crate::unsaved_work::UnsavedWork;

    #[derive(Clone, Copy)]
    enum SaveStart {
        InFlight,
        Refused,
        CleanAtOnce,
    }

    struct FakeWork {
        dirty: bool,
        saving: bool,
        save_start: SaveStart,
        discards: usize,
    }

    impl FakeWork {
        fn finish_save(&mut self, stored: bool, cx: &mut Context<Self>) {
            self.saving = false;
            self.dirty = !stored;
            cx.notify();
        }
    }

    impl UnsavedWork for FakeWork {
        fn has_unsaved_work(&self, _: &App) -> bool {
            self.dirty
        }

        fn unsaved_work_name(&self) -> Option<SharedString> {
            Some("greet.rhai".into())
        }

        fn start_saving_unsaved_work(&mut self, _: &mut Context<Self>) -> bool {
            match self.save_start {
                SaveStart::InFlight => self.saving = true,
                SaveStart::Refused => {}
                SaveStart::CleanAtOnce => self.dirty = false,
            }
            self.saving
        }

        fn is_saving_unsaved_work(&self) -> bool {
            self.saving
        }

        fn discard_unsaved_work(&mut self, _: &mut Context<Self>) {
            self.dirty = false;
            self.discards += 1;
        }
    }

    impl Render for FakeWork {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    struct Rig {
        guard: NavigationGuard,
        work: Entity<FakeWork>,
        window: AnyWindowHandle,
    }

    fn rig(cx: &mut TestAppContext, dirty: bool, save_start: SaveStart) -> Rig {
        let window: AnyWindowHandle = cx
            .add_window(|_, _| FakeWork {
                dirty: false,
                saving: false,
                save_start,
                discards: 0,
            })
            .into();
        let work = cx.new(|_| FakeWork {
            dirty,
            saving: false,
            save_start,
            discards: 0,
        });
        let mut guard = NavigationGuard::new(window);
        guard.watch(Some(UnsavedWorkHandle::new(work.clone())));
        Rig {
            guard,
            work,
            window,
        }
    }

    fn asked(cx: &mut TestAppContext, save_start: SaveStart, target: LeaveTarget) -> Rig {
        let mut rig = rig(cx, true, save_start);
        rig.guard.ask(target);
        rig
    }

    fn overlays() -> LeaveTarget {
        LeaveTarget::Screen(Screen::Overlays)
    }

    fn discards(rig: &Rig, cx: &mut TestAppContext) -> usize {
        rig.work.read_with(cx, |work, _| work.discards)
    }

    fn still_dirty(rig: &Rig, cx: &mut TestAppContext) -> bool {
        cx.update(|cx| rig.guard.blocks(cx))
    }

    #[gpui::test]
    fn only_unsaved_work_on_the_watched_screen_blocks_leaving(cx: &mut TestAppContext) {
        let clean = rig(cx, false, SaveStart::InFlight);
        let dirty = rig(cx, true, SaveStart::InFlight);
        let mut unwatched = rig(cx, true, SaveStart::InFlight);
        unwatched.guard.watch(None);

        let blocks = cx.update(|cx| {
            [
                clean.guard.blocks(cx),
                dirty.guard.blocks(cx),
                unwatched.guard.blocks(cx),
            ]
        });

        assert_eq!(blocks, [false, true, false]);
    }

    #[gpui::test]
    fn keep_editing_closes_the_question_and_leaves_the_work_untouched(cx: &mut TestAppContext) {
        let mut rig = asked(cx, SaveStart::InFlight, overlays());

        rig.guard.cancel();
        let late_discard = cx.update(|cx| rig.guard.discard(cx));
        let late_save = cx.update(|cx| rig.guard.save(cx));

        assert_eq!(
            (
                rig.guard.is_asking(),
                late_discard,
                late_save,
                discards(&rig, cx),
                still_dirty(&rig, cx)
            ),
            (false, None, None, 0, true)
        );
    }

    #[gpui::test]
    fn discard_restores_the_work_and_releases_the_asked_target(cx: &mut TestAppContext) {
        for target in [overlays(), LeaveTarget::CloseWindow] {
            let mut rig = asked(cx, SaveStart::InFlight, target.clone());

            let released = cx.update(|cx| rig.guard.discard(cx));

            assert_eq!(
                (
                    released,
                    rig.guard.is_asking(),
                    discards(&rig, cx),
                    still_dirty(&rig, cx)
                ),
                (Some(target.clone()), false, 1, false),
                "{target:?}"
            );
        }
    }

    #[gpui::test]
    fn discard_without_an_open_question_touches_nothing(cx: &mut TestAppContext) {
        let mut rig = rig(cx, true, SaveStart::InFlight);

        let released = cx.update(|cx| rig.guard.discard(cx));

        assert_eq!((released, discards(&rig, cx)), (None, 0));
    }

    #[gpui::test]
    fn save_and_leave_releases_the_target_only_once_the_save_settles_clean(
        cx: &mut TestAppContext,
    ) {
        for target in [overlays(), LeaveTarget::CloseWindow] {
            let mut rig = asked(cx, SaveStart::InFlight, target.clone());

            let on_click = cx.update(|cx| rig.guard.save(cx));
            let while_saving = cx.update(|cx| rig.guard.settle(cx));
            rig.work.update(cx, |work, cx| work.finish_save(true, cx));
            let settled = cx.update(|cx| rig.guard.settle(cx));

            assert_eq!(
                (on_click, rig.guard.is_asking(), while_saving, settled),
                (None, false, None, Some(target.clone())),
                "{target:?}"
            );
        }
    }

    #[gpui::test]
    fn a_failed_save_keeps_the_user_on_the_screen(cx: &mut TestAppContext) {
        let mut rig = asked(cx, SaveStart::InFlight, overlays());

        cx.update(|cx| rig.guard.save(cx));
        rig.work.update(cx, |work, cx| work.finish_save(false, cx));
        let settled = cx.update(|cx| rig.guard.settle(cx));
        rig.work.update(cx, |work, cx| work.finish_save(true, cx));
        let after_a_later_manual_save = cx.update(|cx| rig.guard.settle(cx));

        assert_eq!(
            (settled, after_a_later_manual_save, rig.guard.is_asking()),
            (None, None, false)
        );
    }

    #[gpui::test]
    fn a_save_that_resolves_without_waiting_answers_at_once(cx: &mut TestAppContext) {
        for (save_start, expected) in [
            (SaveStart::Refused, None),
            (SaveStart::CleanAtOnce, Some(overlays())),
        ] {
            let mut rig = asked(cx, save_start, overlays());

            let released = cx.update(|cx| rig.guard.save(cx));
            let settled_later = cx.update(|cx| rig.guard.settle(cx));

            assert_eq!(
                (released, settled_later, rig.guard.is_asking()),
                (expected, None, false)
            );
        }
    }

    #[gpui::test]
    fn view_changes_while_only_asking_do_not_answer_the_question(cx: &mut TestAppContext) {
        let mut rig = asked(cx, SaveStart::InFlight, overlays());

        let settled = cx.update(|cx| rig.guard.settle(cx));

        assert_eq!((settled, rig.guard.is_asking()), (None, true));
    }

    #[gpui::test]
    fn watching_another_screen_forgets_a_save_still_in_flight(cx: &mut TestAppContext) {
        let mut rig = asked(cx, SaveStart::InFlight, overlays());
        cx.update(|cx| rig.guard.save(cx));

        rig.guard
            .watch(Some(UnsavedWorkHandle::new(rig.work.clone())));
        rig.work.update(cx, |work, cx| work.finish_save(true, cx));
        let settled = cx.update(|cx| rig.guard.settle(cx));

        assert_eq!(settled, None);
    }

    #[gpui::test]
    fn closing_removes_the_guarded_window(cx: &mut TestAppContext) {
        let rig = rig(cx, true, SaveStart::InFlight);

        cx.update(|cx| rig.guard.close_window(cx));
        cx.run_until_parked();

        let open = cx.update(|cx| cx.windows().contains(&rig.window));
        assert!(!open);
    }

    const DIALOG_WINDOW_W: f32 = 480.0;
    const DIALOG_WINDOW_H: f32 = 360.0;
    const SCAN_STEP: f32 = 6.0;

    type Heard = Rc<RefCell<Vec<&'static str>>>;

    struct DialogHost {
        heard: Heard,
    }

    fn hear(
        heard: &Heard,
        answer: &'static str,
    ) -> impl Fn(&ClickEvent, &mut Window, &mut App) + use<> {
        let heard = Rc::clone(heard);
        move |_, _, _| heard.borrow_mut().push(answer)
    }

    impl Render for DialogHost {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let dismissed = Rc::clone(&self.heard);
            div().size_full().child(leave_dialog(
                Some("greet.rhai".into()),
                &FORGE_DEFAULT,
                LeaveDialogHandlers {
                    cancel: hear(&self.heard, "keep editing"),
                    save: hear(&self.heard, "save"),
                    discard: hear(&self.heard, "discard"),
                    dismiss: move |_: &mut Window, _: &mut App| {
                        dismissed.borrow_mut().push("dismiss");
                    },
                },
            ))
        }
    }

    fn open_dialog(cx: &mut TestAppContext) -> (Heard, &mut VisualTestContext) {
        install_language(Language::En);
        let heard = Heard::default();
        let host_heard = Rc::clone(&heard);
        let (_host, vcx) = cx.add_window_view(|_, _| DialogHost { heard: host_heard });
        vcx.simulate_resize(size(px(DIALOG_WINDOW_W), px(DIALOG_WINDOW_H)));
        vcx.run_until_parked();
        vcx.update(|window, cx| window.simulate_next_frame(cx));
        vcx.run_until_parked();
        (heard, vcx)
    }

    #[gpui::test]
    fn escape_only_dismisses_the_dialog(cx: &mut TestAppContext) {
        let (heard, vcx) = open_dialog(cx);

        vcx.simulate_keystrokes("escape");

        assert_eq!(*heard.borrow(), ["dismiss"]);
    }

    #[gpui::test]
    fn a_click_outside_the_card_only_dismisses_the_dialog(cx: &mut TestAppContext) {
        let (heard, vcx) = open_dialog(cx);

        vcx.simulate_click(point(px(1.0), px(1.0)), Modifiers::none());

        assert_eq!(*heard.borrow(), ["dismiss"]);
    }

    #[gpui::test]
    fn the_dialog_offers_keep_editing_save_and_discard(cx: &mut TestAppContext) {
        let (heard, vcx) = open_dialog(cx);

        let mut y = 0.0;
        while y < DIALOG_WINDOW_H {
            let mut x = 0.0;
            while x < DIALOG_WINDOW_W {
                vcx.simulate_click(point(px(x), px(y)), Modifiers::none());
                x += SCAN_STEP;
            }
            y += SCAN_STEP;
        }

        let mut first_seen: Vec<&str> = Vec::new();
        for answer in heard.borrow().iter() {
            if *answer != "dismiss" && !first_seen.contains(answer) {
                first_seen.push(answer);
            }
        }
        assert_eq!(first_seen, ["keep editing", "save", "discard"]);
    }
}
