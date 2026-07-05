use std::borrow::Cow;

use iced::advanced::layout;
use iced::advanced::mouse;
use iced::advanced::renderer;
use iced::advanced::svg;
use iced::advanced::text;
use iced::advanced::widget::Widget;
use iced::advanced::widget::tree::{self, Tree};
use iced::advanced::{Clipboard, Layout, Shell};
use iced::time::Instant;
#[cfg(test)]
use iced::widget::Space;
use iced::{
    Border, Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Shadow, Size, Vector,
    window,
};

use crate::icons::Icon;
use crate::palette::ForgePalette;
use crate::tokens::{BORDER_THIN, FONT_MD, FONT_SM, FontRole, Radius, font, radius};

const HEADER_H: f32 = 56.0;
const HEADER_TILE: f32 = 28.0;
const HEADER_TILE_GAP: f32 = 10.0;
const HEADER_TILE_ICON: f32 = 15.0;
const PAD_H: f32 = 16.0;
const PAD_V: f32 = 12.0;
const CLOSE_HIT_W: f32 = 32.0;
const CLOSE_HIT_H: f32 = 32.0;
const DIVIDER_H: f32 = 1.0;
const MAX_DT_SECS: f32 = 0.032;
const RESIZE_VISUAL_W: f32 = 2.0;
const RESIZE_HIT_W: f32 = 8.0;

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
    pub header_icon: Option<(Icon, Color)>,
    pub on_close: Option<Message>,
    pub on_resize: Option<Box<dyn Fn(f32) -> Message + 'a>>,
    pub resizable: bool,
    pub sheet_key: Option<&'a str>,
    pub palette: &'a ForgePalette,
}

pub struct SideSheet<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer>
where
    Renderer: iced::advanced::Renderer
        + iced::advanced::text::Renderer<Font = Font>
        + iced::advanced::svg::Renderer,
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
    is_resizing: bool,
    resize_drag_origin: Option<(f32, f32)>,
    is_hovering_resize_handle: bool,
    needs_layout_invalidation: bool,
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
    Renderer: iced::advanced::Renderer
        + iced::advanced::text::Renderer<Font = Font>
        + iced::advanced::svg::Renderer
        + 'a,
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
                header_icon: None,
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

    pub fn header_icon(mut self, icon: Icon, tint: Color) -> Self {
        self.config.header_icon = Some((icon, tint));
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

    fn resize_hit_zone(&self, sheet_rect: Rectangle) -> Rectangle {
        let center_x = match self.config.position {
            SheetPosition::Right => sheet_rect.x + RESIZE_VISUAL_W / 2.0,
            SheetPosition::Left => sheet_rect.x + sheet_rect.width - RESIZE_VISUAL_W / 2.0,
        };
        Rectangle {
            x: center_x - RESIZE_HIT_W / 2.0,
            y: sheet_rect.y,
            width: RESIZE_HIT_W,
            height: sheet_rect.height,
        }
    }

    fn resize_visual_rect(&self, sheet_rect: Rectangle) -> Rectangle {
        let x = match self.config.position {
            SheetPosition::Right => sheet_rect.x,
            SheetPosition::Left => sheet_rect.x + sheet_rect.width - RESIZE_VISUAL_W,
        };
        Rectangle {
            x,
            y: sheet_rect.y,
            width: RESIZE_VISUAL_W,
            height: sheet_rect.height,
        }
    }
}

fn fill_sheet_text<R>(
    renderer: &mut R,
    content: String,
    size: f32,
    bounds: Size,
    position: Point,
    color: Color,
    viewport: Rectangle,
) where
    R: iced::advanced::text::Renderer<Font = Font>,
{
    renderer.fill_text(
        text::Text {
            content,
            bounds,
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
    Renderer: iced::advanced::Renderer
        + iced::advanced::text::Renderer<Font = Font>
        + iced::advanced::svg::Renderer,
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
                snap: true,
            },
            p.base,
        );

        if let Some(header) = &self.config.header {
            let icon_offset = if self.config.header_icon.is_some() {
                HEADER_TILE + HEADER_TILE_GAP
            } else {
                0.0
            };

            if let Some((icon, tint)) = self.config.header_icon {
                let tile_rect = Rectangle {
                    x: animated_sheet_rect.x + PAD_H,
                    y: bounds.y + (HEADER_H - HEADER_TILE) / 2.0,
                    width: HEADER_TILE,
                    height: HEADER_TILE,
                };
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: tile_rect,
                        border: Border {
                            color: Color::TRANSPARENT,
                            width: 0.0,
                            radius: radius(Radius::Sm).into(),
                        },
                        shadow: Shadow::default(),
                        snap: true,
                    },
                    p.surface_overlay,
                );
                let icon_rect = Rectangle {
                    x: tile_rect.x + (HEADER_TILE - HEADER_TILE_ICON) / 2.0,
                    y: tile_rect.y + (HEADER_TILE - HEADER_TILE_ICON) / 2.0,
                    width: HEADER_TILE_ICON,
                    height: HEADER_TILE_ICON,
                };
                renderer.draw_svg(
                    svg::Svg::new(svg::Handle::from_memory(icon.bytes())).color(tint),
                    icon_rect,
                    *viewport,
                );
            }

            let title_line_h = FONT_MD * 1.4;
            let block_h = if header.subtitle.is_some() {
                title_line_h + 2.0 + FONT_SM * 1.4
            } else {
                title_line_h
            };
            let text_y = bounds.y + (HEADER_H - block_h) / 2.0;
            let text_x = animated_sheet_rect.x + PAD_H + icon_offset;
            let title_avail_w = (sheet_w - PAD_H * 3.0 - CLOSE_HIT_W - icon_offset).max(0.0);

            fill_sheet_text(
                renderer,
                header.title.as_ref().to_owned(),
                FONT_MD,
                Size::new(title_avail_w, FONT_MD * 1.4),
                Point {
                    x: text_x,
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
                    Size::new(title_avail_w, FONT_SM * 1.4),
                    Point {
                        x: text_x,
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
                    Size::INFINITE,
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
                    snap: true,
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

        if self.config.resizable {
            let visual_base = self.resize_visual_rect(base_sheet_rect);
            let handle_rect = Rectangle {
                x: visual_base.x + x_offset,
                ..visual_base
            };
            let handle_color = if state.is_resizing {
                p.brand
            } else if state.is_hovering_resize_handle {
                p.border_active
            } else {
                p.border_input
            };
            renderer.fill_quad(
                renderer::Quad {
                    bounds: handle_rect,
                    border: Border::default(),
                    shadow: Shadow::default(),
                    snap: true,
                },
                handle_color,
            );
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
                if state.needs_layout_invalidation {
                    state.needs_layout_invalidation = false;
                    shell.invalidate_layout();
                }
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
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                let hit_zone = self.resize_hit_zone(sheet_rect);
                let state = tree.state.downcast_mut::<SideSheetState>();
                state.is_hovering_resize_handle =
                    self.config.resizable && progress > 0.95 && hit_zone.contains(*position);
                if state.is_resizing {
                    if let Some((origin_x, origin_width)) = state.resize_drag_origin {
                        let new_width = match self.config.position {
                            SheetPosition::Right => origin_width + (origin_x - position.x),
                            SheetPosition::Left => origin_width + (position.x - origin_x),
                        };
                        state.resized_width =
                            Some(new_width.clamp(self.config.width.min, self.config.width.max));
                        state.needs_layout_invalidation = true;
                        shell.request_redraw();
                    }
                    return;
                }
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
                    if self.config.resizable && progress > 0.95 {
                        let hit_zone = self.resize_hit_zone(sheet_rect);
                        if hit_zone.contains(pos) {
                            let state = tree.state.downcast_mut::<SideSheetState>();
                            let current_w = state
                                .resized_width
                                .unwrap_or(self.config.width.initial)
                                .clamp(self.config.width.min, self.config.width.max);
                            state.is_resizing = true;
                            state.resize_drag_origin = Some((pos.x, current_w));
                            return;
                        }
                    }
                    if !sheet_rect.contains(pos)
                        && let Some(msg) = self.config.on_close.clone()
                    {
                        shell.publish(msg);
                        return;
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let final_width = {
                    let state = tree.state.downcast_mut::<SideSheetState>();
                    if state.is_resizing {
                        let w = state
                            .resized_width
                            .unwrap_or(self.config.width.initial)
                            .clamp(self.config.width.min, self.config.width.max);
                        state.is_resizing = false;
                        state.resize_drag_origin = None;
                        Some(w)
                    } else {
                        None
                    }
                };
                if let Some(w) = final_width {
                    if let Some(cb) = &self.config.on_resize {
                        shell.publish(cb(w));
                    }
                    return;
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

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<iced::advanced::overlay::Element<'b, Message, Theme, Renderer>> {
        let child_layout = layout.children().next()?;
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            child_layout,
            renderer,
            viewport,
            translation,
        )
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
        if self.config.resizable && (state.is_resizing || state.progress > 0.95) {
            if state.is_resizing {
                return mouse::Interaction::ResizingHorizontally;
            }
            let bounds = layout.bounds();
            let sheet_w = self.current_width(state);
            let sheet_rect = self.sheet_rect(bounds, sheet_w);
            let hit_zone = self.resize_hit_zone(sheet_rect);
            if let mouse::Cursor::Available(pos) = cursor
                && hit_zone.contains(pos)
            {
                return mouse::Interaction::ResizingHorizontally;
            }
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
    Renderer: iced::advanced::Renderer
        + iced::advanced::text::Renderer<Font = Font>
        + iced::advanced::svg::Renderer
        + 'a,
{
    fn from(w: SideSheet<'a, Message, Theme, Renderer>) -> Self {
        Element::new(w)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::time::Duration;
    use iced::{Theme, advanced::layout};

    use crate::palette::CATPPUCCIN_MOCHA;

    #[derive(Debug, Clone)]
    enum CloseMsg {
        Close,
        HeaderClose,
        Resized(f32),
    }

    impl PartialEq for CloseMsg {
        fn eq(&self, other: &Self) -> bool {
            match (self, other) {
                (Self::Close, Self::Close) => true,
                (Self::HeaderClose, Self::HeaderClose) => true,
                (Self::Resized(a), Self::Resized(b)) => (a - b).abs() < 0.01,
                _ => false,
            }
        }
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

    fn resizable_open_widget<'a>() -> SideSheet<'a, CloseMsg, Theme, ()> {
        SideSheet::new(Space::new())
            .open(true)
            .resizable(true)
            .palette(&CATPPUCCIN_MOCHA)
    }

    fn make_tree<Msg, R>(widget: &SideSheet<'_, Msg, Theme, R>) -> Tree
    where
        Msg: Clone + 'static,
        R: iced::advanced::Renderer
            + iced::advanced::text::Renderer<Font = Font>
            + iced::advanced::svg::Renderer,
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
        run_update_at(widget, tree, event, mouse::Cursor::Unavailable)
    }

    fn run_update_at(
        widget: &mut SideSheet<'_, CloseMsg, Theme, ()>,
        tree: &mut Tree,
        event: &Event,
        cursor: mouse::Cursor,
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
            cursor,
            &(),
            &mut NullClipboard,
            &mut shell,
            &vp,
        );
        let redraw = shell.redraw_request();
        (messages, redraw)
    }

    fn cursor_at(x: f32, y: f32) -> mouse::Cursor {
        mouse::Cursor::Available(Point::new(x, y))
    }

    fn cursor_moved(x: f32, y: f32) -> Event {
        Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(x, y),
        })
    }

    fn left_pressed() -> Event {
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
    }

    fn left_released() -> Event {
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
    }

    fn mouse_interaction_at(
        widget: &mut SideSheet<'_, CloseMsg, Theme, ()>,
        tree: &mut Tree,
        pos: Point,
    ) -> mouse::Interaction {
        let node = widget.layout(tree, &(), &limits_1280());
        let layout = Layout::new(&node);
        let vp = viewport_rect();
        widget.mouse_interaction(tree, layout, mouse::Cursor::Available(pos), &vp, &())
    }

    fn hit_zone_center() -> Point {
        Point::new(921.0, 400.0)
    }

    fn outside_hit_zone() -> Point {
        Point::new(900.0, 400.0)
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

    #[allow(clippy::unwrap_used)]
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

        let past = Instant::now()
            .checked_sub(Duration::from_millis(100))
            .unwrap();
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

    #[allow(clippy::unwrap_used)]
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
        let past = Instant::now()
            .checked_sub(Duration::from_millis(100))
            .unwrap();
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

    #[allow(clippy::unwrap_used)]
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
        let past = Instant::now()
            .checked_sub(Duration::from_millis(50))
            .unwrap();
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

    #[allow(clippy::unwrap_used)]
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

        let past = Instant::now().checked_sub(Duration::from_secs(10)).unwrap();
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

    #[allow(clippy::unwrap_used)]
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

        let past = Instant::now()
            .checked_sub(Duration::from_millis(50))
            .unwrap();
        tree.state.downcast_mut::<SideSheetState>().last_tick = Some(past);

        let now = Instant::now();
        let (_, redraw) = run_update(&mut widget, &mut tree, &redraw_event(now));

        assert_eq!(
            redraw,
            window::RedrawRequest::NextFrame,
            "must request redraw while progress != target"
        );
    }

    #[test]
    fn hover_flag_follows_cursor_in_hit_zone() {
        let mut widget = resizable_open_widget();
        let mut tree = make_tree(&widget);
        tree.state.downcast_mut::<SideSheetState>().progress = 1.0;

        run_update_at(
            &mut widget,
            &mut tree,
            &cursor_moved(921.0, 400.0),
            cursor_at(921.0, 400.0),
        );
        assert!(
            tree.state
                .downcast_ref::<SideSheetState>()
                .is_hovering_resize_handle
        );

        run_update_at(
            &mut widget,
            &mut tree,
            &cursor_moved(900.0, 400.0),
            cursor_at(900.0, 400.0),
        );
        assert!(
            !tree
                .state
                .downcast_ref::<SideSheetState>()
                .is_hovering_resize_handle
        );
    }

    #[test]
    fn mouse_interaction_resizing_when_in_hit_zone_and_open() {
        let mut widget = resizable_open_widget();
        let mut tree = make_tree(&widget);
        tree.state.downcast_mut::<SideSheetState>().progress = 1.0;

        let interaction = mouse_interaction_at(&mut widget, &mut tree, hit_zone_center());
        assert_eq!(interaction, mouse::Interaction::ResizingHorizontally);
    }

    #[test]
    fn mouse_interaction_default_when_outside_hit_zone_or_closed() {
        let mut widget = resizable_open_widget();
        let mut tree = make_tree(&widget);
        tree.state.downcast_mut::<SideSheetState>().progress = 1.0;

        let outside = mouse_interaction_at(&mut widget, &mut tree, outside_hit_zone());
        assert_eq!(outside, mouse::Interaction::None);

        tree.state.downcast_mut::<SideSheetState>().progress = 0.5;
        let animating = mouse_interaction_at(&mut widget, &mut tree, hit_zone_center());
        assert_eq!(animating, mouse::Interaction::None);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn press_in_hit_zone_starts_resize_drag() {
        let mut widget = resizable_open_widget();
        let mut tree = make_tree(&widget);
        tree.state.downcast_mut::<SideSheetState>().progress = 1.0;

        run_update_at(
            &mut widget,
            &mut tree,
            &left_pressed(),
            cursor_at(921.0, 400.0),
        );

        let state = tree.state.downcast_ref::<SideSheetState>();
        assert!(state.is_resizing);
        assert!(state.resize_drag_origin.is_some());
        let (origin_x, origin_w) = state.resize_drag_origin.unwrap();
        assert!((origin_x - 921.0).abs() < 0.01);
        assert!((origin_w - 360.0).abs() < 0.01);
    }

    #[test]
    fn press_outside_hit_zone_does_not_start_resize() {
        let mut widget = resizable_open_widget().on_close(CloseMsg::Close);
        let mut tree = make_tree(&widget);
        tree.state.downcast_mut::<SideSheetState>().progress = 1.0;

        run_update_at(
            &mut widget,
            &mut tree,
            &left_pressed(),
            cursor_at(900.0, 400.0),
        );

        assert!(!tree.state.downcast_ref::<SideSheetState>().is_resizing);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn cursor_moved_while_resizing_updates_resized_width() {
        let mut widget = resizable_open_widget();
        let mut tree = make_tree(&widget);
        tree.state.downcast_mut::<SideSheetState>().progress = 1.0;

        run_update_at(
            &mut widget,
            &mut tree,
            &left_pressed(),
            cursor_at(921.0, 400.0),
        );

        run_update_at(
            &mut widget,
            &mut tree,
            &cursor_moved(881.0, 400.0),
            cursor_at(881.0, 400.0),
        );

        let w = tree.state.downcast_ref::<SideSheetState>().resized_width;
        assert!(w.is_some());
        assert!(
            (w.unwrap() - 400.0).abs() < 0.01,
            "expected 400, got {:?}",
            w
        );
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn drag_clamps_to_min() {
        let mut widget = resizable_open_widget();
        let mut tree = make_tree(&widget);
        tree.state.downcast_mut::<SideSheetState>().progress = 1.0;

        run_update_at(
            &mut widget,
            &mut tree,
            &left_pressed(),
            cursor_at(921.0, 400.0),
        );
        run_update_at(
            &mut widget,
            &mut tree,
            &cursor_moved(1021.0, 400.0),
            cursor_at(1021.0, 400.0),
        );

        let w = tree
            .state
            .downcast_ref::<SideSheetState>()
            .resized_width
            .unwrap();
        assert!((w - 280.0).abs() < 0.01, "expected min=280, got {w}");
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn drag_clamps_to_max() {
        let mut widget = resizable_open_widget();
        let mut tree = make_tree(&widget);
        tree.state.downcast_mut::<SideSheetState>().progress = 1.0;

        run_update_at(
            &mut widget,
            &mut tree,
            &left_pressed(),
            cursor_at(921.0, 400.0),
        );
        run_update_at(
            &mut widget,
            &mut tree,
            &cursor_moved(720.0, 400.0),
            cursor_at(720.0, 400.0),
        );

        let w = tree
            .state
            .downcast_ref::<SideSheetState>()
            .resized_width
            .unwrap();
        assert!((w - 560.0).abs() < 0.01, "expected max=560, got {w}");
    }

    #[test]
    fn release_while_resizing_emits_on_resize_once() {
        let mut widget = resizable_open_widget().on_resize(CloseMsg::Resized);
        let mut tree = make_tree(&widget);
        tree.state.downcast_mut::<SideSheetState>().progress = 1.0;

        run_update_at(
            &mut widget,
            &mut tree,
            &left_pressed(),
            cursor_at(921.0, 400.0),
        );
        run_update_at(
            &mut widget,
            &mut tree,
            &cursor_moved(881.0, 400.0),
            cursor_at(881.0, 400.0),
        );

        let (msgs, _) = run_update_at(
            &mut widget,
            &mut tree,
            &left_released(),
            cursor_at(881.0, 400.0),
        );

        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0], CloseMsg::Resized(400.0));
    }

    #[test]
    fn release_without_on_resize_callback_clears_state() {
        let mut widget = resizable_open_widget();
        let mut tree = make_tree(&widget);
        tree.state.downcast_mut::<SideSheetState>().progress = 1.0;

        run_update_at(
            &mut widget,
            &mut tree,
            &left_pressed(),
            cursor_at(921.0, 400.0),
        );
        run_update_at(
            &mut widget,
            &mut tree,
            &cursor_moved(881.0, 400.0),
            cursor_at(881.0, 400.0),
        );

        let (msgs, _) = run_update_at(
            &mut widget,
            &mut tree,
            &left_released(),
            cursor_at(881.0, 400.0),
        );

        assert!(msgs.is_empty());
        let state = tree.state.downcast_ref::<SideSheetState>();
        assert!(!state.is_resizing);
        assert!(state.resize_drag_origin.is_none());
    }

    #[test]
    fn release_when_not_resizing_emits_nothing() {
        let mut widget = resizable_open_widget().on_resize(CloseMsg::Resized);
        let mut tree = make_tree(&widget);
        tree.state.downcast_mut::<SideSheetState>().progress = 1.0;

        let (msgs, _) = run_update_at(
            &mut widget,
            &mut tree,
            &left_released(),
            cursor_at(921.0, 400.0),
        );

        assert!(msgs.is_empty());
    }

    #[test]
    fn release_clears_resize_state() {
        let mut widget = resizable_open_widget().on_resize(CloseMsg::Resized);
        let mut tree = make_tree(&widget);
        tree.state.downcast_mut::<SideSheetState>().progress = 1.0;

        run_update_at(
            &mut widget,
            &mut tree,
            &left_pressed(),
            cursor_at(921.0, 400.0),
        );
        assert!(tree.state.downcast_ref::<SideSheetState>().is_resizing);

        run_update_at(
            &mut widget,
            &mut tree,
            &left_released(),
            cursor_at(921.0, 400.0),
        );

        let state = tree.state.downcast_ref::<SideSheetState>();
        assert!(!state.is_resizing);
        assert!(state.resize_drag_origin.is_none());
    }

    #[test]
    fn layout_uses_resized_width_not_initial() {
        let initial_w = 360.0_f32;
        let resized_w = 440.0_f32;

        let mut widget: SideSheet<'_, CloseMsg, Theme, ()> = SideSheet::new(Space::new())
            .open(true)
            .resizable(true)
            .width(SheetWidth::new(initial_w, 280.0, 560.0))
            .palette(&CATPPUCCIN_MOCHA);
        let mut tree = make_tree(&widget);
        tree.state.downcast_mut::<SideSheetState>().resized_width = Some(resized_w);

        let node = widget.layout(&mut tree, &(), &limits_1280());
        let child_x = node.children()[0].bounds().x;

        let expected_sheet_x = 1280.0 - resized_w;
        let expected_child_x = expected_sheet_x + PAD_H;
        let initial_child_x = 1280.0 - initial_w + PAD_H;

        assert!(
            (child_x - expected_child_x).abs() < 0.01,
            "child x={child_x} must reflect resized_w={resized_w}, not initial_w={initial_w} (initial_x={initial_child_x})"
        );
    }
}
