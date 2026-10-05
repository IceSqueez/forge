use std::rc::Rc;

use gpui::{App, Context, Entity, SharedString};

pub trait UnsavedWork: Sized + 'static {
    fn has_unsaved_work(&self, cx: &App) -> bool;

    fn unsaved_work_name(&self) -> Option<SharedString>;

    fn start_saving_unsaved_work(&mut self, cx: &mut Context<Self>) -> bool;

    fn is_saving_unsaved_work(&self) -> bool;

    fn discard_unsaved_work(&mut self, cx: &mut Context<Self>);
}

trait ErasedUnsavedWork {
    fn has_unsaved_work(&self, cx: &App) -> bool;
    fn name(&self, cx: &App) -> Option<SharedString>;
    fn start_saving(&self, cx: &mut App) -> bool;
    fn is_saving(&self, cx: &App) -> bool;
    fn discard(&self, cx: &mut App);
}

impl<V: UnsavedWork> ErasedUnsavedWork for Entity<V> {
    fn has_unsaved_work(&self, cx: &App) -> bool {
        self.read(cx).has_unsaved_work(cx)
    }

    fn name(&self, cx: &App) -> Option<SharedString> {
        self.read(cx).unsaved_work_name()
    }

    fn start_saving(&self, cx: &mut App) -> bool {
        self.update(cx, |view, cx| view.start_saving_unsaved_work(cx))
    }

    fn is_saving(&self, cx: &App) -> bool {
        self.read(cx).is_saving_unsaved_work()
    }

    fn discard(&self, cx: &mut App) {
        self.update(cx, |view, cx| view.discard_unsaved_work(cx));
    }
}

#[derive(Clone)]
pub struct UnsavedWorkHandle(Rc<dyn ErasedUnsavedWork>);

impl UnsavedWorkHandle {
    pub fn new<V: UnsavedWork>(view: Entity<V>) -> Self {
        Self(Rc::new(view))
    }

    pub fn has_unsaved_work(&self, cx: &App) -> bool {
        self.0.has_unsaved_work(cx)
    }

    pub fn name(&self, cx: &App) -> Option<SharedString> {
        self.0.name(cx)
    }

    pub fn start_saving(&self, cx: &mut App) -> bool {
        self.0.start_saving(cx)
    }

    pub fn is_saving(&self, cx: &App) -> bool {
        self.0.is_saving(cx)
    }

    pub fn discard(&self, cx: &mut App) {
        self.0.discard(cx);
    }
}
