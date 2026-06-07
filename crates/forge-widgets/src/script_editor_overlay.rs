use iced::advanced::Layout;
use iced::advanced::layout;
use iced::advanced::mouse;
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::tree::Tree;
use iced::{Element, Point, Rectangle, Size};

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
    ) -> Self {
        let content_tree = Tree::new(&content);
        Self {
            content,
            content_tree,
            anchor_line,
            anchor_col,
            line_height,
            char_width,
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
    use iced::{Point, Rectangle, Size};

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
}
