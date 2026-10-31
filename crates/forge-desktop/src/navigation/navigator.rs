use std::rc::Rc;

use gpui::App;

use super::screen::Screen;

pub(crate) struct NavRequested(pub Screen);

type OpenScreen = dyn Fn(Screen, &mut App);

#[derive(Clone)]
pub(crate) struct Navigator {
    open: Rc<OpenScreen>,
}

impl Navigator {
    pub(crate) fn new(open: impl Fn(Screen, &mut App) + 'static) -> Self {
        Self {
            open: Rc::new(open),
        }
    }

    pub(crate) fn open(&self, screen: Screen, cx: &mut App) {
        (self.open)(screen, cx);
    }
}
