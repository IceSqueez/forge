use std::rc::Rc;

use gpui::{App, Context, Entity, Window};

use crate::screen::Screen;

pub trait ScreenShortcuts: Sized + 'static {
    fn save_shortcut(&mut self, _cx: &mut Context<Self>) {}

    fn run_shortcut(&mut self, _cx: &mut Context<Self>) {}

    fn new_action_shortcut(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {}
}

trait ErasedScreenShortcuts {
    fn save(&self, cx: &mut App);
    fn run(&self, cx: &mut App);
    fn new_action(&self, window: &mut Window, cx: &mut App);
}

impl<V: ScreenShortcuts> ErasedScreenShortcuts for Entity<V> {
    fn save(&self, cx: &mut App) {
        self.update(cx, |view, cx| view.save_shortcut(cx));
    }

    fn run(&self, cx: &mut App) {
        self.update(cx, |view, cx| view.run_shortcut(cx));
    }

    fn new_action(&self, window: &mut Window, cx: &mut App) {
        self.update(cx, |view, cx| view.new_action_shortcut(window, cx));
    }
}

#[derive(Clone)]
pub struct ScreenShortcutsHandle(Rc<dyn ErasedScreenShortcuts>);

impl ScreenShortcutsHandle {
    pub fn new<V: ScreenShortcuts>(view: Entity<V>) -> Self {
        Self(Rc::new(view))
    }

    pub fn save(&self, cx: &mut App) {
        self.0.save(cx);
    }

    pub fn run(&self, cx: &mut App) {
        self.0.run(cx);
    }

    pub fn new_action(&self, window: &mut Window, cx: &mut App) {
        self.0.new_action(window, cx);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToggleReturn {
    pub toggled: Screen,
    pub back_to: Screen,
}

pub fn toggle_destination(
    current: &Screen,
    target: &Screen,
    recorded: Option<&ToggleReturn>,
) -> Option<Screen> {
    if current != target {
        return Some(target.clone());
    }
    recorded
        .filter(|recorded| &recorded.toggled == target)
        .map(|recorded| recorded.back_to.clone())
}
