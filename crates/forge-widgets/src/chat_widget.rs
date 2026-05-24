use iced::advanced::layout;
use iced::advanced::mouse;
use iced::advanced::renderer;
use iced::advanced::svg;
use iced::advanced::text;
use iced::advanced::widget::Widget;
use iced::advanced::widget::tree::{self, Tree};
use iced::advanced::{Clipboard, Layout, Shell};
use iced::{Element, Event, Length, Rectangle, Size, Theme};

use crate::chat::ChatRow;
use crate::palette::ForgePalette;

#[allow(dead_code)]
pub struct ChatRowWidget<Msg> {
    palette: ForgePalette,
    data: ChatRow,
    on_user_click: Option<fn(String) -> Msg>,
}

impl<Msg: Clone + 'static> ChatRowWidget<Msg> {
    pub fn new(
        palette: ForgePalette,
        data: ChatRow,
        on_user_click: Option<fn(String) -> Msg>,
    ) -> Self {
        Self {
            palette,
            data,
            on_user_click,
        }
    }
}

impl<'a, Msg: Clone + 'static> From<ChatRowWidget<Msg>> for Element<'a, Msg> {
    fn from(w: ChatRowWidget<Msg>) -> Element<'a, Msg> {
        Element::new(w)
    }
}

#[allow(dead_code)]
#[derive(Default)]
struct ChatRowState<P: Default> {
    paragraphs: ChatRowParagraphs<P>,
    username_bounds: Rectangle,
    hovered: bool,
}

#[allow(dead_code)]
#[derive(Default)]
struct ChatRowParagraphs<P: Default> {
    timestamp: P,
    platform: P,
    badges: Vec<P>,
    primary_body: P,
    secondary_body: Option<P>,
    triggered: Option<P>,
}

impl<Msg, R> Widget<Msg, Theme, R> for ChatRowWidget<Msg>
where
    Msg: Clone + 'static,
    R: iced::advanced::Renderer + text::Renderer + svg::Renderer,
    R::Paragraph: Default,
{
    fn size(&self) -> Size<Length> {
        Size {
            width: Length::Fill,
            height: Length::Shrink,
        }
    }

    fn layout(&mut self, _tree: &mut Tree, _renderer: &R, limits: &layout::Limits) -> layout::Node {
        layout::Node::new(Size::new(limits.max().width, 36.0))
    }

    fn draw(
        &self,
        _tree: &Tree,
        _renderer: &mut R,
        _theme: &Theme,
        _style: &renderer::Style,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<ChatRowState<R::Paragraph>>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(ChatRowState::<R::Paragraph>::default())
    }

    fn update(
        &mut self,
        _tree: &mut Tree,
        _event: &Event,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _renderer: &R,
        _clipboard: &mut dyn Clipboard,
        _shell: &mut Shell<'_, Msg>,
        _viewport: &Rectangle,
    ) {
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &R,
    ) -> mouse::Interaction {
        mouse::Interaction::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::{ChatBody, Platform};
    use crate::palette::CATPPUCCIN_MOCHA;

    #[test]
    fn chat_row_widget_can_be_constructed_and_converted_to_element() {
        let palette = CATPPUCCIN_MOCHA;
        let row = ChatRow {
            seq: 0,
            timestamp: "12:00:00".into(),
            platform: Platform::Twitch,
            badges: vec![],
            username: "test".into(),
            username_color: iced::Color::WHITE,
            body: ChatBody::Message("hello".into()),
        };
        let widget: ChatRowWidget<()> = ChatRowWidget::new(palette, row, None);
        let _element: iced::Element<'_, ()> = widget.into();
    }
}
