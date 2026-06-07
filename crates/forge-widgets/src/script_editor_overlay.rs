use iced::advanced::Layout;
use iced::advanced::layout;
use iced::advanced::mouse;
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::tree::Tree;
use iced::advanced::{Clipboard, Shell};
use iced::{Element, Event, Point, Rectangle, Size};

pub(crate) fn compute_anchor_pixel(
    line: usize,
    col: usize,
    line_height: f32,
    char_width: f32,
) -> (f32, f32) {
    (col as f32 * char_width, line as f32 * line_height)
}

/// `rect` carries the proposed popup rect: `x = anchor_x`, `y = anchor_y + line_height`,
/// `width/height = panel dimensions`. Returns the clamped top-left `Point`.
///
/// Flip invariant: if the panel does not fit below the cursor it is placed above; if it does
/// not fit above either (panel taller than `anchor_y`), it is pinned to `y = 0`.
pub(crate) fn clamp_to_bounds(rect: Rectangle, bounds: Size, anchor_y: f32) -> Point {
    let x = rect.x.max(0.0).min((bounds.width - rect.width).max(0.0));
    let y = if rect.y + rect.height <= bounds.height {
        rect.y
    } else {
        let above_y = anchor_y - rect.height;
        above_y.max(0.0)
    };
    Point::new(x, y)
}

pub(crate) fn is_dismiss_event(
    event: &Event,
    content_bounds: Rectangle,
    cursor: mouse::Cursor,
) -> bool {
    match event {
        Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
            ..
        }) => true,
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
            matches!(cursor, mouse::Cursor::Available(pos) if !content_bounds.contains(pos))
        }
        _ => false,
    }
}

pub struct ScriptEditorOverlay<'a, Msg, Renderer = iced::Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    content: Element<'a, Msg, iced::Theme, Renderer>,
    content_tree: Tree,
    anchor_line: usize,
    anchor_col: usize,
    line_height: f32,
    char_width: f32,
    on_dismiss: Box<dyn Fn() -> Msg + 'a>,
}

impl<'a, Msg, Renderer> ScriptEditorOverlay<'a, Msg, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    pub fn new(
        content: Element<'a, Msg, iced::Theme, Renderer>,
        anchor_line: usize,
        anchor_col: usize,
        line_height: f32,
        char_width: f32,
        on_dismiss: impl Fn() -> Msg + 'a,
    ) -> Self {
        let content_tree = Tree::new(&content);
        Self {
            content,
            content_tree,
            anchor_line,
            anchor_col,
            line_height,
            char_width,
            on_dismiss: Box::new(on_dismiss),
        }
    }
}

impl<'a, Msg, Renderer> overlay::Overlay<Msg, iced::Theme, Renderer>
    for ScriptEditorOverlay<'a, Msg, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        let limits = layout::Limits::new(Size::ZERO, Size::INFINITE);
        let content_node =
            self.content
                .as_widget_mut()
                .layout(&mut self.content_tree, renderer, &limits);
        let panel_sz = content_node.size();
        let (anchor_x, anchor_y) = compute_anchor_pixel(
            self.anchor_line,
            self.anchor_col,
            self.line_height,
            self.char_width,
        );
        let position = clamp_to_bounds(
            Rectangle {
                x: anchor_x,
                y: anchor_y + self.line_height,
                width: panel_sz.width,
                height: panel_sz.height,
            },
            bounds,
            anchor_y,
        );
        content_node.move_to(position)
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        let bounds = layout.bounds();
        self.content.as_widget().draw(
            &self.content_tree,
            renderer,
            theme,
            style,
            layout,
            cursor,
            &bounds,
        );
    }

    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Msg>,
    ) {
        let content_bounds = layout.bounds();
        if is_dismiss_event(event, content_bounds, cursor) {
            shell.publish((self.on_dismiss)());
            shell.capture_event();
            return;
        }
        self.content.as_widget_mut().update(
            &mut self.content_tree,
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            &content_bounds,
        );
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let bounds = layout.bounds();
        self.content.as_widget().mouse_interaction(
            &self.content_tree,
            layout,
            cursor,
            &bounds,
            renderer,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::advanced::overlay::Overlay as _;
    use iced::advanced::{Layout, Shell};
    use iced::{Point, Rectangle, Size};

    fn esc_event() -> Event {
        Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
            modified_key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
            physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::Escape),
            location: iced::keyboard::Location::Standard,
            modifiers: iced::keyboard::Modifiers::empty(),
            text: None,
            repeat: false,
        })
    }

    fn left_click() -> Event {
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
    }

    fn panel_bounds() -> Rectangle {
        Rectangle {
            x: 100.0,
            y: 100.0,
            width: 200.0,
            height: 150.0,
        }
    }

    #[test]
    fn compute_anchor_pixel_basic() {
        let (x, y) = compute_anchor_pixel(2, 5, 16.0, 8.0);
        assert_eq!(x, 40.0);
        assert_eq!(y, 32.0);
    }

    #[test]
    fn clamp_shifts_left_when_right_overflow() {
        let rect = Rectangle {
            x: 700.0,
            y: 30.0,
            width: 200.0,
            height: 120.0,
        };
        let p = clamp_to_bounds(rect, Size::new(800.0, 600.0), 14.0);
        assert_eq!(p, Point::new(600.0, 30.0));
    }

    #[test]
    fn clamp_flips_above_when_bottom_overflow() {
        let rect = Rectangle {
            x: 100.0,
            y: 566.0,
            width: 200.0,
            height: 120.0,
        };
        let p = clamp_to_bounds(rect, Size::new(800.0, 600.0), 550.0);
        assert_eq!(p, Point::new(100.0, 430.0));
    }

    #[test]
    fn clamp_pins_to_top_when_both_overflow() {
        let rect = Rectangle {
            x: 100.0,
            y: 66.0,
            width: 200.0,
            height: 650.0,
        };
        let p = clamp_to_bounds(rect, Size::new(800.0, 600.0), 50.0);
        assert_eq!(p, Point::new(100.0, 0.0));
    }

    #[test]
    fn esc_is_dismiss_event() {
        assert!(is_dismiss_event(
            &esc_event(),
            panel_bounds(),
            mouse::Cursor::Available(Point::new(0.0, 0.0)),
        ));
    }

    #[test]
    fn left_click_outside_panel_is_dismiss() {
        assert!(is_dismiss_event(
            &left_click(),
            panel_bounds(),
            mouse::Cursor::Available(Point::new(500.0, 500.0)),
        ));
    }

    #[test]
    fn left_click_inside_panel_is_not_dismiss() {
        assert!(!is_dismiss_event(
            &left_click(),
            panel_bounds(),
            mouse::Cursor::Available(Point::new(150.0, 150.0)),
        ));
    }

    #[test]
    fn other_event_is_not_dismiss() {
        let event = Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(0.0, 0.0),
        });
        assert!(!is_dismiss_event(
            &event,
            panel_bounds(),
            mouse::Cursor::Available(Point::new(0.0, 0.0)),
        ));
    }

    #[test]
    fn unavailable_cursor_click_is_not_dismiss() {
        assert!(!is_dismiss_event(
            &left_click(),
            panel_bounds(),
            mouse::Cursor::Unavailable,
        ));
    }

    struct NullClipboard;
    impl iced::advanced::Clipboard for NullClipboard {
        fn read(&self, _kind: iced::advanced::clipboard::Kind) -> Option<String> {
            None
        }
        fn write(&mut self, _kind: iced::advanced::clipboard::Kind, _contents: String) {}
    }

    #[test]
    fn esc_publishes_dismiss_and_captures() {
        let mut ov: ScriptEditorOverlay<'static, u32, ()> =
            ScriptEditorOverlay::new(iced::widget::Space::new().into(), 0, 0, 16.0, 8.0, || 99u32);
        let node = ov.layout(&(), Size::new(800.0, 600.0));
        let layout = Layout::new(&node);
        let mut messages: Vec<u32> = Vec::new();
        let mut shell = Shell::new(&mut messages);
        ov.update(
            &esc_event(),
            layout,
            mouse::Cursor::Available(Point::new(0.0, 0.0)),
            &(),
            &mut NullClipboard,
            &mut shell,
        );
        let captured = shell.is_event_captured();
        drop(shell);
        assert_eq!(messages, vec![99u32]);
        assert!(captured);
    }

    #[test]
    fn outside_click_publishes_dismiss_and_captures() {
        let mut ov: ScriptEditorOverlay<'static, u32, ()> =
            ScriptEditorOverlay::new(iced::widget::Space::new().into(), 0, 0, 16.0, 8.0, || 99u32);
        let node = ov.layout(&(), Size::new(800.0, 600.0));
        let layout = Layout::new(&node);
        let mut messages: Vec<u32> = Vec::new();
        let mut shell = Shell::new(&mut messages);
        ov.update(
            &left_click(),
            layout,
            mouse::Cursor::Available(Point::new(500.0, 500.0)),
            &(),
            &mut NullClipboard,
            &mut shell,
        );
        let captured = shell.is_event_captured();
        drop(shell);
        assert_eq!(messages, vec![99u32]);
        assert!(captured);
    }

    #[test]
    fn inside_click_does_not_dismiss() {
        let mut ov: ScriptEditorOverlay<'static, u32, ()> = ScriptEditorOverlay::new(
            iced::widget::Space::new()
                .width(iced::Length::Fixed(200.0))
                .height(iced::Length::Fixed(150.0))
                .into(),
            0,
            0,
            16.0,
            8.0,
            || 99u32,
        );
        let node = ov.layout(&(), Size::new(800.0, 600.0));
        let panel_b = node.bounds();
        let layout = Layout::new(&node);
        let mut messages: Vec<u32> = Vec::new();
        let mut shell = Shell::new(&mut messages);
        ov.update(
            &left_click(),
            layout,
            mouse::Cursor::Available(Point::new(
                panel_b.x + panel_b.width / 2.0,
                panel_b.y + panel_b.height / 2.0,
            )),
            &(),
            &mut NullClipboard,
            &mut shell,
        );
        assert!(messages.is_empty());
    }
}
