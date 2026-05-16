use crate::Screen;
use loom_events::Event;
use loom_widgets::ThemeId;

#[derive(Debug, Clone)]
pub enum Message {
    Navigate(Screen),
    ThemeChanged(ThemeId),
    BusEvent(Event),
    Noop,
}
