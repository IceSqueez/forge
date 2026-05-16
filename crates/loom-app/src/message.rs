use crate::Screen;
use loom_widgets::ThemeId;

#[derive(Debug, Clone)]
pub enum Message {
    Navigate(Screen),
    ThemeChanged(ThemeId),
    Noop,
}
