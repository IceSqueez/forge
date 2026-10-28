use std::rc::Rc;

use gpui::App;

use super::screen::Screen;

pub struct NavRequested(pub Screen);

type OpenScreen = dyn Fn(Screen, &mut App);

#[derive(Clone)]
pub struct Navigator {
    open: Rc<OpenScreen>,
}

impl Navigator {
    pub fn new(open: impl Fn(Screen, &mut App) + 'static) -> Self {
        Self {
            open: Rc::new(open),
        }
    }

    pub fn open(&self, screen: Screen, cx: &mut App) {
        (self.open)(screen, cx);
    }
}
