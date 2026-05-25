use std::borrow::Cow;

use iced::advanced::layout;
use iced::advanced::mouse;
use iced::advanced::renderer;
use iced::advanced::text;
use iced::advanced::widget::Widget;
use iced::advanced::widget::tree::{self, Tree};
use iced::advanced::{Clipboard, Layout, Shell};
use iced::time::Instant;
use iced::{
    Alignment, Background, Border, Color, Element, Event, Font, Length, Pixels, Point, Rectangle,
    Shadow, Size, Vector,
    widget::{Space, container, mouse_area, stack},
    window,
};

use crate::palette::ForgePalette;
use crate::tokens::{BORDER_THIN, FONT_MD, FONT_SM, FontRole, font};

const HEADER_H: f32 = 56.0;
const PAD_H: f32 = 16.0;
const PAD_V: f32 = 12.0;
const CLOSE_HIT_W: f32 = 32.0;
const CLOSE_HIT_H: f32 = 32.0;
const DIVIDER_H: f32 = 1.0;
const MAX_DT_SECS: f32 = 0.032;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SheetPosition {
    Right,
    Left,
}

#[derive(Debug, Clone, Copy)]
pub struct SheetWidth {
    pub initial: f32,
    pub min: f32,
    pub max: f32,
}

impl SheetWidth {
    /// Panics in debug if `min < 200`, `min >= max`, or `initial` is outside `[min, max]`.
    pub fn new(initial: f32, min: f32, max: f32) -> Self {
        debug_assert!(min >= 200.0, "SheetWidth::min must be >= 200px");
        debug_assert!(min < max, "SheetWidth::min must be < max");
        debug_assert!(
            initial >= min && initial <= max,
            "SheetWidth::initial must be in [min, max]"
        );
        Self { initial, min, max }
    }
}

impl Default for SheetWidth {
    fn default() -> Self {
        Self {
            initial: 360.0,
            min: 280.0,
            max: 560.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SheetAnimation {
    pub duration_ms: u32,
    pub easing: Easing,
}

impl Default for SheetAnimation {
    fn default() -> Self {
        Self {
            duration_ms: 200,
            easing: Easing::EaseOutCubic,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Easing {
    Linear,
    EaseInOut,
    EaseOutCubic,
    EaseOutQuart,
}

pub struct SheetHeader<'a, Message> {
    pub title: Cow<'a, str>,
    pub subtitle: Option<Cow<'a, str>>,
    pub on_close: Option<Message>,
}

pub struct SideSheetConfig<'a, Message> {
    pub open: bool,
    pub position: SheetPosition,
    pub width: SheetWidth,
    pub animation: SheetAnimation,
    pub header: Option<SheetHeader<'a, Message>>,
    pub on_close: Option<Message>,
    pub on_resize: Option<Box<dyn Fn(f32) -> Message + 'a>>,
    pub resizable: bool,
    pub sheet_key: Option<&'a str>,
    pub palette: &'a ForgePalette,
}

pub struct SideSheet<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer>
where
    Renderer: iced::advanced::Renderer + iced::advanced::text::Renderer<Font = Font>,
{
    content: Element<'a, Message, Theme, Renderer>,
    config: SideSheetConfig<'a, Message>,
}

#[derive(Default)]
struct SideSheetState {
    progress: f32,
    target: f32,
    last_tick: Option<Instant>,
    resized_width: Option<f32>,
    #[allow(dead_code)]
    is_resizing: bool,
    #[allow(dead_code)]
    resize_drag_origin: Option<(f32, f32)>,
    #[allow(dead_code)]
    is_hovering_resize_handle: bool,
}

fn apply_easing(t: f32, easing: Easing) -> f32 {
    match easing {
        Easing::Linear => t,
        Easing::EaseInOut => {
            if t < 0.5 {
                4.0 * t * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
            }
        }
        Easing::EaseOutCubic => 1.0 - (1.0 - t).powi(3),
        Easing::EaseOutQuart => 1.0 - (1.0 - t).powi(4),
    }
}

impl<'a, Message, Theme, Renderer> SideSheet<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Renderer: iced::advanced::Renderer + iced::advanced::text::Renderer<Font = Font> + 'a,
{
    pub fn new(content: impl Into<Element<'a, Message, Theme, Renderer>>) -> Self
    where
        Theme: 'a,
    {
        Self {
            content: content.into(),
            config: SideSheetConfig {
                open: false,
                position: SheetPosition::Right,
                width: SheetWidth::default(),
                animation: SheetAnimation::default(),
                header: None,
                on_close: None,
                on_resize: None,
                resizable: false,
                sheet_key: None,
                palette: &crate::palette::CATPPUCCIN_MOCHA,
            },
        }
    }

    pub fn open(mut self, is_open: bool) -> Self {
        self.config.open = is_open;
        self
    }

    pub fn position(mut self, pos: SheetPosition) -> Self {
        self.config.position = pos;
        self
    }

    pub fn width(mut self, w: SheetWidth) -> Self {
        self.config.width = w;
        self
    }

    pub fn animation(mut self, a: SheetAnimation) -> Self {
        self.config.animation = a;
        self
    }

    pub fn header(mut self, header: SheetHeader<'a, Message>) -> Self {
        self.config.header = Some(header);
        self
    }

    pub fn on_close(mut self, msg: Message) -> Self {
        self.config.on_close = Some(msg);
        self
    }

    pub fn on_resize<F: Fn(f32) -> Message + 'a>(mut self, f: F) -> Self {
        self.config.on_resize = Some(Box::new(f));
        self
    }

    pub fn resizable(mut self, yes: bool) -> Self {
        self.config.resizable = yes;
        self
    }

    pub fn sheet_key(mut self, key: &'a str) -> Self {
        self.config.sheet_key = Some(key);
        self
    }

    pub fn palette(mut self, palette: &'a ForgePalette) -> Self {
        self.config.palette = palette;
        self
    }

    fn current_width(&self, state: &SideSheetState) -> f32 {
        state
            .resized_width
            .unwrap_or(self.config.width.initial)
            .clamp(self.config.width.min, self.config.width.max)
    }

    fn sheet_rect(&self, bounds: Rectangle, sheet_w: f32) -> Rectangle {
        let x = match self.config.position {
            SheetPosition::Right => bounds.x + bounds.width - sheet_w,
            SheetPosition::Left => bounds.x,
        };
        Rectangle {
            x,
            y: bounds.y,
            width: sheet_w,
            height: bounds.height,
        }
    }

    fn close_btn_rect(&self, sheet_rect: Rectangle) -> Rectangle {
        Rectangle {
            x: sheet_rect.x + sheet_rect.width - PAD_H - CLOSE_HIT_W,
            y: sheet_rect.y + (HEADER_H - CLOSE_HIT_H) / 2.0,
            width: CLOSE_HIT_W,
            height: CLOSE_HIT_H,
        }
    }
}

fn fill_sheet_text<R>(
    renderer: &mut R,
    content: String,
    size: f32,
    position: Point,
    color: Color,
    viewport: Rectangle,
) where
    R: iced::advanced::text::Renderer<Font = Font>,
{
    renderer.fill_text(
        text::Text {
            content,
            bounds: Size::INFINITE,
            size: Pixels(size),
            line_height: text::LineHeight::default(),
            font: font(FontRole::Body),
            align_x: text::Alignment::Default,
            align_y: iced::alignment::Vertical::Top,
            shaping: text::Shaping::default(),
            wrapping: text::Wrapping::None,
        },
        position,
        color,
        viewport,
    );
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for SideSheet<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Renderer: iced::advanced::Renderer + iced::advanced::text::Renderer<Font = Font>,
{
    fn size(&self) -> Size<Length> {
        Size {
            width: Length::Fill,
            height: Length::Fill,
        }
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<SideSheetState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(SideSheetState::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree.state.downcast_ref::<SideSheetState>();
        let avail = limits.max();
        let sheet_w = self.current_width(state);
        let header_h = if self.config.header.is_some() {
            HEADER_H
        } else {
            0.0
        };
        let content_h = (avail.height - header_h - PAD_V * 2.0).max(0.0);
        let content_w = (sheet_w - PAD_H * 2.0).max(0.0);

        let child_limits = layout::Limits::new(Size::ZERO, Size::new(content_w, content_h));
        let child_node =
            self.content
                .as_widget_mut()
                .layout(&mut tree.children[0], renderer, &child_limits);

        let sheet_x = match self.config.position {
            SheetPosition::Right => avail.width - sheet_w,
            SheetPosition::Left => 0.0,
        };
        let child_node = child_node.move_to(Point::new(sheet_x + PAD_H, header_h + PAD_V));

        if !self.config.open && state.progress < 0.001 {
            return layout::Node::with_children(Size::ZERO, vec![child_node]);
        }

        layout::Node::with_children(avail, vec![child_node])
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<SideSheetState>();
        let visual_progress = apply_easing(state.progress, self.config.animation.easing);

        if !self.config.open && state.progress < 0.001 {
            return;
        }

        let p = self.config.palette;
        let bounds = layout.bounds();
        let sheet_w = self.current_width(state);

        let x_offset = match self.config.position {
            SheetPosition::Right => (1.0 - visual_progress) * sheet_w,
            SheetPosition::Left => -(1.0 - visual_progress) * sheet_w,
        };

        let base_sheet_rect = self.sheet_rect(bounds, sheet_w);
        let animated_sheet_rect = Rectangle {
            x: base_sheet_rect.x + x_offset,
            ..base_sheet_rect
        };

        let mut scrim_color = p.scrim;
        scrim_color.a *= visual_progress;
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: Border::default(),
                shadow: Shadow::default(),
                snap: false,
            },
            scrim_color,
        );

        renderer.fill_quad(
            renderer::Quad {
                bounds: animated_sheet_rect,
                border: Border {
                    color: p.border_regular,
                    width: BORDER_THIN,
                    radius: 0.0.into(),
                },
                shadow: Shadow::default(),
                snap: false,
            },
            p.base,
        );

        if let Some(header) = &self.config.header {
            let title_line_h = FONT_MD * 1.4;
            let block_h = if header.subtitle.is_some() {
                title_line_h + 2.0 + FONT_SM * 1.4
            } else {
                title_line_h
            };
            let text_y = bounds.y + (HEADER_H - block_h) / 2.0;

            fill_sheet_text(
                renderer,
                header.title.as_ref().to_owned(),
                FONT_MD,
                Point {
                    x: animated_sheet_rect.x + PAD_H,
                    y: text_y,
                },
                p.text_primary,
                *viewport,
            );

            if let Some(ref sub) = header.subtitle {
                fill_sheet_text(
                    renderer,
                    sub.as_ref().to_owned(),
                    FONT_SM,
                    Point {
                        x: animated_sheet_rect.x + PAD_H,
                        y: text_y + title_line_h + 2.0,
                    },
                    p.text_secondary,
                    *viewport,
                );
            }

            if header.on_close.is_some() {
                let btn = self.close_btn_rect(animated_sheet_rect);
                fill_sheet_text(
                    renderer,
                    "\u{2715}".to_owned(),
                    FONT_MD,
                    Point {
                        x: btn.x + (CLOSE_HIT_W - FONT_MD) / 2.0,
                        y: btn.y + (CLOSE_HIT_H - FONT_MD) / 2.0,
                    },
                    p.text_muted,
                    *viewport,
                );
            }

            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: animated_sheet_rect.x,
                        y: bounds.y + HEADER_H - DIVIDER_H,
                        width: sheet_w,
                        height: DIVIDER_H,
                    },
                    border: Border::default(),
                    shadow: Shadow::default(),
                    snap: false,
                },
                p.border_regular,
            );
        }

        if let Some(child_layout) = layout.children().next() {
            renderer.with_translation(Vector::new(x_offset, 0.0), |renderer| {
                self.content.as_widget().draw(
                    &tree.children[0],
                    renderer,
                    theme,
                    style,
                    child_layout,
                    cursor,
                    viewport,
                );
            });
        }
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        {
            let state = tree.state.downcast_mut::<SideSheetState>();
            state.target = if self.config.open { 1.0 } else { 0.0 };

            if let Event::Window(window::Event::RedrawRequested(now)) = event {
                if state.progress != state.target {
                    let dt = state
                        .last_tick
                        .map(|t| (*now - t).as_secs_f32().min(MAX_DT_SECS))
                        .unwrap_or(0.0);
                    let duration_secs = self.config.animation.duration_ms as f32 / 1000.0;
                    let step = if duration_secs > 0.0 {
                        dt / duration_secs
                    } else {
                        1.0
                    };

                    if state.target > state.progress {
                        state.progress = (state.progress + step).min(1.0);
                    } else {
                        state.progress = (state.progress - step).max(0.0);
                    }

                    if state.progress == state.target {
                        state.last_tick = None;
                        if state.progress < 0.001 {
                            shell.invalidate_layout();
                        }
                    } else {
                        state.last_tick = Some(*now);
                        shell.request_redraw();
                    }
                }
                return;
            }
        }

        let (progress, sheet_w) = {
            let state = tree.state.downcast_ref::<SideSheetState>();
            (state.progress, self.current_width(state))
        };

        if !self.config.open && progress < 0.001 {
            return;
        }

        let bounds = layout.bounds();
        let sheet_rect = self.sheet_rect(bounds, sheet_w);

        match event {
            Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                ..
            }) => {
                if let Some(msg) = self.config.on_close.clone() {
                    shell.publish(msg);
                }
                return;
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let mouse::Cursor::Available(pos) = cursor {
                    if let Some(header) = &self.config.header
                        && let Some(close_msg) = header.on_close.clone()
                        && self.close_btn_rect(sheet_rect).contains(pos)
                    {
                        shell.publish(close_msg);
                        return;
                    }
                    if !sheet_rect.contains(pos)
                        && let Some(msg) = self.config.on_close.clone()
                    {
                        shell.publish(msg);
                        return;
                    }
                }
            }
            _ => {}
        }

        if let Some(child_layout) = layout.children().next() {
            self.content.as_widget_mut().update(
                &mut tree.children[0],
                event,
                child_layout,
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
            );
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<SideSheetState>();
        if !self.config.open && state.progress < 0.001 {
            return mouse::Interaction::default();
        }
        if let Some(child_layout) = layout.children().next() {
            return self.content.as_widget().mouse_interaction(
                &tree.children[0],
                child_layout,
                cursor,
                viewport,
                renderer,
            );
        }
        mouse::Interaction::default()
    }
}

impl<'a, Message, Theme, Renderer> From<SideSheet<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + iced::advanced::text::Renderer<Font = Font> + 'a,
{
    fn from(w: SideSheet<'a, Message, Theme, Renderer>) -> Self {
        Element::new(w)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SheetEdge {
    Left,
    Right,
}

pub fn side_sheet<'a, Msg: Clone + 'a>(
    content: Element<'a, Msg>,
    on_dismiss: Msg,
    edge: SheetEdge,
    width: f32,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let p = *palette;

    let backdrop = mouse_area(
        container(Space::new())
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_theme: &iced::Theme| container::Style {
                background: Some(Background::Color(p.scrim)),
                ..container::Style::default()
            }),
    )
    .on_press(on_dismiss);

    let border_color = p.border_input;
    let panel = container(content)
        .width(Length::Fixed(width))
        .height(Length::Fill)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(p.base)),
            border: Border {
                color: border_color,
                width: 0.5,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        });

    let align = match edge {
        SheetEdge::Left => Alignment::Start,
        SheetEdge::Right => Alignment::End,
    };

    let positioned = container(panel)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(align);

    stack![backdrop, positioned].into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::time::Duration;
    use iced::{Theme, advanced::layout};

    use crate::palette::CATPPUCCIN_MOCHA;

    #[derive(Debug, Clone, PartialEq)]
    enum CloseMsg {
        Close,
        HeaderClose,
    }

    fn open_widget<'a>() -> SideSheet<'a, CloseMsg, Theme, ()> {
        SideSheet::new(Space::new())
            .open(true)
            .palette(&CATPPUCCIN_MOCHA)
    }

    fn closed_widget<'a>() -> SideSheet<'a, CloseMsg, Theme, ()> {
        SideSheet::new(Space::new())
            .open(false)
            .palette(&CATPPUCCIN_MOCHA)
    }

    fn make_tree<Msg, R>(widget: &SideSheet<'_, Msg, Theme, R>) -> Tree
    where
        Msg: Clone + 'static,
        R: iced::advanced::Renderer + iced::advanced::text::Renderer<Font = Font>,
    {
        use iced::advanced::widget::Widget as _;
        Tree {
            tag: widget.tag(),
            state: widget.state(),
            children: widget.children(),
        }
    }

    fn limits_1280() -> layout::Limits {
        layout::Limits::new(Size::ZERO, Size::new(1280.0, 800.0))
    }

    struct NullClipboard;
    impl Clipboard for NullClipboard {
        fn read(&self, _kind: iced::advanced::clipboard::Kind) -> Option<String> {
            None
        }
        fn write(&mut self, _kind: iced::advanced::clipboard::Kind, _contents: String) {}
    }

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

    fn redraw_event(now: Instant) -> Event {
        Event::Window(window::Event::RedrawRequested(now))
    }

    fn viewport_rect() -> Rectangle {
        Rectangle::new(Point::ORIGIN, Size::new(1280.0, 800.0))
    }

    fn run_update(
        widget: &mut SideSheet<'_, CloseMsg, Theme, ()>,
        tree: &mut Tree,
        event: &Event,
    ) -> (Vec<CloseMsg>, window::RedrawRequest) {
        let node = widget.layout(tree, &(), &limits_1280());
        let layout = Layout::new(&node);
        let vp = viewport_rect();
        let mut messages: Vec<CloseMsg> = Vec::new();
        let mut shell = Shell::new(&mut messages);
        widget.update(
            tree,
            event,
            layout,
            mouse::Cursor::Unavailable,
            &(),
            &mut NullClipboard,
            &mut shell,
            &vp,
        );
        let redraw = shell.redraw_request();
        (messages, redraw)
    }

    #[test]
    fn sheet_width_new_valid() {
        let w = SheetWidth::new(360.0, 280.0, 560.0);
        assert_eq!(w.initial, 360.0);
        assert_eq!(w.min, 280.0);
        assert_eq!(w.max, 560.0);
    }

    #[test]
    #[should_panic]
    fn sheet_width_new_panics_min_below_floor() {
        SheetWidth::new(300.0, 180.0, 600.0);
    }

    #[test]
    #[should_panic]
    fn sheet_width_new_panics_initial_below_min() {
        SheetWidth::new(200.0, 300.0, 600.0);
    }

    #[test]
    #[should_panic]
    fn sheet_width_new_panics_min_equal_max() {
        SheetWidth::new(400.0, 400.0, 400.0);
    }

    #[test]
    #[should_panic]
    fn sheet_width_new_panics_initial_above_max() {
        SheetWidth::new(700.0, 280.0, 560.0);
    }

    #[test]
    fn closed_sheet_layout_returns_zero_size() {
        let mut widget = closed_widget();
        let mut tree = make_tree(&widget);
        let node = widget.layout(&mut tree, &(), &limits_1280());
        assert_eq!(node.size(), Size::ZERO);
    }

    #[test]
    fn open_sheet_layout_returns_viewport_size() {
        let mut widget = open_widget();
        let mut tree = make_tree(&widget);
        let node = widget.layout(&mut tree, &(), &limits_1280());
        assert_eq!(node.size(), Size::new(1280.0, 800.0));
    }

    #[test]
    fn content_layout_width_stable_across_open_state() {
        let make_content_w = |open: bool| {
            let mut widget: SideSheet<'_, CloseMsg, Theme, ()> = SideSheet::new(Space::new())
                .open(open)
                .width(SheetWidth::new(360.0, 280.0, 560.0))
                .palette(&CATPPUCCIN_MOCHA);
            let mut tree = make_tree(&widget);
            let node = widget.layout(&mut tree, &(), &limits_1280());
            node.children()[0].size().width
        };
        let w_open = make_content_w(true);
        let w_closed = make_content_w(false);
        assert!(
            (w_open - w_closed).abs() < 0.01,
            "content width must be stable: {w_open} vs {w_closed}"
        );
    }

    #[test]
    fn builder_api_compiles_to_element() {
        let p = CATPPUCCIN_MOCHA;
        let _elem: iced::Element<'_, CloseMsg> = SideSheet::new(Space::new())
            .open(false)
            .position(SheetPosition::Right)
            .width(SheetWidth::new(360.0, 280.0, 560.0))
            .animation(SheetAnimation {
                duration_ms: 200,
                easing: Easing::EaseOutCubic,
            })
            .header(SheetHeader {
                title: Cow::Borrowed("Test"),
                subtitle: Some(Cow::Borrowed("Sub")),
                on_close: Some(CloseMsg::HeaderClose),
            })
            .on_close(CloseMsg::Close)
            .resizable(false)
            .sheet_key("test_sheet")
            .palette(&p)
            .into();
    }

    #[test]
    fn esc_with_on_close_publishes_message() {
        use iced::advanced::Shell;

        let mut widget = open_widget().on_close(CloseMsg::Close);
        let mut tree = make_tree(&widget);
        let node = widget.layout(&mut tree, &(), &limits_1280());
        let layout = Layout::new(&node);
        let vp = viewport_rect();

        let mut messages: Vec<CloseMsg> = Vec::new();
        let mut shell = Shell::new(&mut messages);

        widget.update(
            &mut tree,
            &esc_event(),
            layout,
            mouse::Cursor::Unavailable,
            &(),
            &mut NullClipboard,
            &mut shell,
            &vp,
        );

        assert_eq!(messages, vec![CloseMsg::Close]);
    }

    #[test]
    fn esc_without_on_close_publishes_nothing() {
        use iced::advanced::Shell;

        let mut widget = open_widget();
        let mut tree = make_tree(&widget);
        let node = widget.layout(&mut tree, &(), &limits_1280());
        let layout = Layout::new(&node);
        let vp = viewport_rect();

        let mut messages: Vec<CloseMsg> = Vec::new();
        let mut shell = Shell::new(&mut messages);

        widget.update(
            &mut tree,
            &esc_event(),
            layout,
            mouse::Cursor::Unavailable,
            &(),
            &mut NullClipboard,
            &mut shell,
            &vp,
        );

        assert!(messages.is_empty());
    }

    #[test]
    fn backdrop_click_publishes_on_close() {
        use iced::advanced::Shell;

        let mut widget = open_widget().on_close(CloseMsg::Close);
        let mut tree = make_tree(&widget);
        let node = widget.layout(&mut tree, &(), &limits_1280());
        let layout = Layout::new(&node);
        let vp = viewport_rect();

        let backdrop_pos = Point::new(100.0, 400.0);
        let event = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));

        let mut messages: Vec<CloseMsg> = Vec::new();
        let mut shell = Shell::new(&mut messages);

        widget.update(
            &mut tree,
            &event,
            layout,
            mouse::Cursor::Available(backdrop_pos),
            &(),
            &mut NullClipboard,
            &mut shell,
            &vp,
        );

        assert_eq!(messages, vec![CloseMsg::Close]);
    }

    #[test]
    fn header_close_button_publishes_header_on_close() {
        use iced::advanced::Shell;

        let p = CATPPUCCIN_MOCHA;
        let mut widget = SideSheet::new(Space::new())
            .open(true)
            .palette(&p)
            .on_close(CloseMsg::Close)
            .header(SheetHeader {
                title: Cow::Borrowed("Viewers"),
                subtitle: None,
                on_close: Some(CloseMsg::HeaderClose),
            });

        let mut tree = make_tree(&widget);
        let node = widget.layout(&mut tree, &(), &limits_1280());
        let layout = Layout::new(&node);
        let vp = viewport_rect();

        let state = tree.state.downcast_ref::<SideSheetState>();
        let sheet_w = widget.current_width(state);
        let bounds = layout.bounds();
        let sheet_rect = widget.sheet_rect(bounds, sheet_w);
        let btn = widget.close_btn_rect(sheet_rect);
        let btn_center = Point::new(btn.x + btn.width / 2.0, btn.y + btn.height / 2.0);

        let event = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));

        let mut messages: Vec<CloseMsg> = Vec::new();
        let mut shell = Shell::new(&mut messages);

        widget.update(
            &mut tree,
            &event,
            layout,
            mouse::Cursor::Available(btn_center),
            &(),
            &mut NullClipboard,
            &mut shell,
            &vp,
        );

        assert_eq!(messages, vec![CloseMsg::HeaderClose]);
    }

    #[test]
    fn easing_boundary_values() {
        for easing in [
            Easing::Linear,
            Easing::EaseInOut,
            Easing::EaseOutCubic,
            Easing::EaseOutQuart,
        ] {
            assert!(
                (apply_easing(0.0, easing) - 0.0).abs() < 1e-6,
                "{easing:?} must be 0 at t=0"
            );
            assert!(
                (apply_easing(1.0, easing) - 1.0).abs() < 1e-6,
                "{easing:?} must be 1 at t=1"
            );
        }
    }

    #[test]
    fn easing_monotone() {
        for easing in [
            Easing::Linear,
            Easing::EaseInOut,
            Easing::EaseOutCubic,
            Easing::EaseOutQuart,
        ] {
            let mut prev = apply_easing(0.0, easing);
            for i in 1..=20 {
                let t = i as f32 / 20.0;
                let v = apply_easing(t, easing);
                assert!(v >= prev - 1e-6, "{easing:?} must be monotone at t={t}");
                prev = v;
            }
        }
    }

    #[test]
    fn animation_advances_when_open() {
        let mut widget = SideSheet::<CloseMsg, Theme, ()>::new(Space::new())
            .open(true)
            .animation(SheetAnimation {
                duration_ms: 200,
                easing: Easing::Linear,
            })
            .palette(&CATPPUCCIN_MOCHA);
        let mut tree = make_tree(&widget);

        let past = Instant::now() - Duration::from_millis(100);
        tree.state.downcast_mut::<SideSheetState>().last_tick = Some(past);

        let now = Instant::now();
        let (_, redraw) = run_update(&mut widget, &mut tree, &redraw_event(now));

        let progress = tree.state.downcast_ref::<SideSheetState>().progress;
        assert!(progress > 0.0, "progress must have advanced: {progress}");
        assert!(
            progress < 1.0,
            "progress must not be complete yet: {progress}"
        );
        assert_eq!(
            redraw,
            window::RedrawRequest::NextFrame,
            "must request redraw while animating"
        );
    }

    #[test]
    fn animation_retreats_when_closed() {
        let mut widget = SideSheet::<CloseMsg, Theme, ()>::new(Space::new())
            .open(false)
            .animation(SheetAnimation {
                duration_ms: 200,
                easing: Easing::Linear,
            })
            .palette(&CATPPUCCIN_MOCHA);
        let mut tree = make_tree(&widget);

        tree.state.downcast_mut::<SideSheetState>().progress = 1.0;
        let past = Instant::now() - Duration::from_millis(100);
        tree.state.downcast_mut::<SideSheetState>().last_tick = Some(past);

        let now = Instant::now();
        let (_, redraw) = run_update(&mut widget, &mut tree, &redraw_event(now));

        let progress = tree.state.downcast_ref::<SideSheetState>().progress;
        assert!(progress < 1.0, "progress must have retreated: {progress}");
        assert!(
            progress > 0.0,
            "progress must not be complete yet: {progress}"
        );
        assert_eq!(redraw, window::RedrawRequest::NextFrame);
    }

    #[test]
    fn animation_mid_flight_flip() {
        let mut widget = SideSheet::<CloseMsg, Theme, ()>::new(Space::new())
            .open(false)
            .animation(SheetAnimation {
                duration_ms: 200,
                easing: Easing::Linear,
            })
            .palette(&CATPPUCCIN_MOCHA);
        let mut tree = make_tree(&widget);

        tree.state.downcast_mut::<SideSheetState>().progress = 0.5;
        let past = Instant::now() - Duration::from_millis(50);
        tree.state.downcast_mut::<SideSheetState>().last_tick = Some(past);

        let now = Instant::now();
        run_update(&mut widget, &mut tree, &redraw_event(now));

        let progress = tree.state.downcast_ref::<SideSheetState>().progress;
        assert!(
            progress < 0.5,
            "progress must decrease from 0.5 toward 0, got {progress}"
        );
        assert!(progress >= 0.0, "progress must not go below 0: {progress}");
    }

    #[test]
    fn animation_snaps_instantly_when_duration_zero() {
        let mut widget = SideSheet::<CloseMsg, Theme, ()>::new(Space::new())
            .open(true)
            .animation(SheetAnimation {
                duration_ms: 0,
                easing: Easing::Linear,
            })
            .palette(&CATPPUCCIN_MOCHA);
        let mut tree = make_tree(&widget);

        let now = Instant::now();
        run_update(&mut widget, &mut tree, &redraw_event(now));

        let progress = tree.state.downcast_ref::<SideSheetState>().progress;
        assert_eq!(progress, 1.0, "zero-duration must snap to target instantly");
    }

    #[test]
    fn progress_clamped_to_unit_interval() {
        let mut widget = SideSheet::<CloseMsg, Theme, ()>::new(Space::new())
            .open(true)
            .animation(SheetAnimation {
                duration_ms: 1,
                easing: Easing::Linear,
            })
            .palette(&CATPPUCCIN_MOCHA);
        let mut tree = make_tree(&widget);

        let past = Instant::now() - Duration::from_secs(10);
        tree.state.downcast_mut::<SideSheetState>().last_tick = Some(past);

        let now = Instant::now();
        run_update(&mut widget, &mut tree, &redraw_event(now));

        let progress = tree.state.downcast_ref::<SideSheetState>().progress;
        assert!(
            (0.0..=1.0).contains(&progress),
            "progress must be in [0, 1]: {progress}"
        );
    }

    #[test]
    fn content_width_stable_during_animation() {
        let check_content_w = |progress: f32| {
            let mut widget: SideSheet<'_, CloseMsg, Theme, ()> = SideSheet::new(Space::new())
                .open(true)
                .width(SheetWidth::new(360.0, 280.0, 560.0))
                .palette(&CATPPUCCIN_MOCHA);
            let mut tree = make_tree(&widget);
            tree.state.downcast_mut::<SideSheetState>().progress = progress;
            let node = widget.layout(&mut tree, &(), &limits_1280());
            node.children()[0].size().width
        };

        let w_at_0_5 = check_content_w(0.5);
        let w_at_1_0 = check_content_w(1.0);
        let w_at_0_1 = check_content_w(0.1);
        assert!(
            (w_at_0_5 - w_at_1_0).abs() < 0.01,
            "content width unstable: progress=0.5 → {w_at_0_5}, progress=1.0 → {w_at_1_0}"
        );
        assert!(
            (w_at_0_1 - w_at_1_0).abs() < 0.01,
            "content width unstable: progress=0.1 → {w_at_0_1}, progress=1.0 → {w_at_1_0}"
        );
    }

    #[test]
    fn no_render_when_progress_zero_and_closed() {
        let mut widget = closed_widget();
        let mut tree = make_tree(&widget);
        let node = widget.layout(&mut tree, &(), &limits_1280());
        assert_eq!(
            node.size(),
            Size::ZERO,
            "must be invisible when fully closed"
        );
    }

    #[test]
    fn layout_full_size_when_open_with_zero_progress() {
        let mut widget = open_widget();
        let mut tree = make_tree(&widget);
        let node = widget.layout(&mut tree, &(), &limits_1280());
        assert_eq!(
            node.size(),
            Size::new(1280.0, 800.0),
            "open=true must produce full layout even before first animation tick"
        );
    }

    #[test]
    fn no_redraw_requested_when_progress_equals_target() {
        let mut widget = SideSheet::<CloseMsg, Theme, ()>::new(Space::new())
            .open(true)
            .animation(SheetAnimation {
                duration_ms: 200,
                easing: Easing::Linear,
            })
            .palette(&CATPPUCCIN_MOCHA);
        let mut tree = make_tree(&widget);
        tree.state.downcast_mut::<SideSheetState>().progress = 1.0;

        let now = Instant::now();
        let (_, redraw) = run_update(&mut widget, &mut tree, &redraw_event(now));

        assert_ne!(
            redraw,
            window::RedrawRequest::NextFrame,
            "must not request redraw when already at target"
        );
    }

    #[test]
    fn redraw_requested_while_animating() {
        let mut widget = SideSheet::<CloseMsg, Theme, ()>::new(Space::new())
            .open(true)
            .animation(SheetAnimation {
                duration_ms: 200,
                easing: Easing::Linear,
            })
            .palette(&CATPPUCCIN_MOCHA);
        let mut tree = make_tree(&widget);

        let past = Instant::now() - Duration::from_millis(50);
        tree.state.downcast_mut::<SideSheetState>().last_tick = Some(past);

        let now = Instant::now();
        let (_, redraw) = run_update(&mut widget, &mut tree, &redraw_event(now));

        assert_eq!(
            redraw,
            window::RedrawRequest::NextFrame,
            "must request redraw while progress != target"
        );
    }
}
