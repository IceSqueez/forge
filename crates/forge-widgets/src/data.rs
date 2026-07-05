use std::cell::Cell;
use std::rc::Rc;

use iced::advanced::widget::tree::{self, Tree};
use iced::advanced::widget::{Operation, Widget};
use iced::advanced::{Clipboard, Layout, Shell, layout, mouse, overlay, renderer};
use iced::widget::button::Status;
use iced::{
    Alignment, Background, Border, Color, Element, Event, Length, Rectangle, Shadow, Size, Vector,
    widget::{Column, Row, Space, button, container, row, rule, text},
};

use forge_types::Variant;
pub use forge_types::VariantKind;

use crate::icons::{Icon, tabler_icon};
use crate::palette::ForgePalette;
use crate::status::badge;
use crate::tokens::{FONT_SM, FONT_XS, FONT_XXS, FontRole, Spacing, font, spf};

pub fn variant_kind_color(kind: VariantKind, palette: &ForgePalette) -> Color {
    match kind {
        VariantKind::Int => palette.info,
        VariantKind::Float => palette.bits,
        VariantKind::Bool => palette.random,
        VariantKind::String => palette.success,
        VariantKind::Datetime => palette.accent_teal,
        VariantKind::Array => palette.brand,
        VariantKind::Object => palette.accent_pink_light,
    }
}

#[derive(Debug, Clone)]
pub struct FooterProps<'a> {
    pub position_info: &'a str,
    pub storage_info: Option<&'a str>,
    pub save_info: Option<&'a str>,
    pub live_indicator: bool,
}

pub fn type_pill<'a, Msg: 'a>(palette: &'a ForgePalette, kind: VariantKind) -> Element<'a, Msg> {
    let bg = palette.surface_overlay;
    let fg = variant_kind_color(kind, palette);
    // Uses the shared `badge()` primitive at the design's smaller badge size
    // (`FONT_XXS`) instead of the former hand-rolled `FONT_XS` container.
    badge(bg, fg, kind.label(), true, FONT_XXS)
}

/// A single `data_table` row: its rendered cells plus an optional hover sink.
/// When a `hover_sink` is provided, the row's whole-width hover state is
/// written into it every time it changes — a companion [`hover_reveal`] cell
/// reads the same `Cell` to fade its contents in on row hover.
pub struct DataRow<'a, Msg> {
    pub cells: Vec<Element<'a, Msg>>,
    pub hover_sink: Option<Rc<Cell<bool>>>,
}

impl<'a, Msg> DataRow<'a, Msg> {
    pub fn new(cells: Vec<Element<'a, Msg>>) -> Self {
        Self {
            cells,
            hover_sink: None,
        }
    }

    pub fn with_hover_sink(cells: Vec<Element<'a, Msg>>, sink: Rc<Cell<bool>>) -> Self {
        Self {
            cells,
            hover_sink: Some(sink),
        }
    }
}

pub fn data_table<'a, Msg: 'a>(
    palette: &'a ForgePalette,
    headers: Vec<&'a str>,
    widths: &[Length],
    rows: Vec<DataRow<'a, Msg>>,
) -> Element<'a, Msg> {
    let border_color = palette.border_regular;
    let shell_bg = palette.shell;
    let header_fg = palette.text_faint;
    let hover_tint = palette.base;

    let rule_style = move |_: &iced::Theme| rule::Style {
        color: border_color,
        radius: 0.0.into(),
        fill_mode: rule::FillMode::Full,
        snap: true,
    };

    let mut header_row = Row::new().spacing(0).align_y(Alignment::Center);
    for (label, &width) in headers.iter().zip(widths.iter()) {
        header_row = header_row.push(
            container(
                text(*label)
                    .size(FONT_XS)
                    .font(font(FontRole::Monospace))
                    .color(header_fg),
            )
            .width(width),
        );
    }

    let header_container = container(header_row)
        .padding([spf(Spacing::Xs), spf(Spacing::Md)])
        .width(Length::Fill)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(shell_bg)),
            ..container::Style::default()
        });

    let mut col = Column::new();
    col = col.push(header_container);
    col = col.push(rule::horizontal(1.0_f32).style(rule_style));

    for row in rows {
        let mut data_row = Row::new().spacing(0).align_y(Alignment::Center);
        for (cell, &width) in row.cells.into_iter().zip(widths.iter()) {
            data_row = data_row.push(container(cell).width(width));
        }
        let data_container = container(data_row)
            .padding([spf(Spacing::Xs), spf(Spacing::Md)])
            .width(Length::Fill);
        col = col.push(hover_row(data_container.into(), hover_tint, row.hover_sink));
        col = col.push(rule::horizontal(1.0_f32).style(rule_style));
    }

    col.width(Length::Fill).into()
}

pub fn persistence_toggle_inline<'a, Msg: Clone + 'a>(
    palette: &'a ForgePalette,
    persisted: bool,
    on_toggle: Msg,
) -> Element<'a, Msg> {
    let pill_bg = if persisted {
        palette.success
    } else {
        palette.surface_overlay
    };
    let dot_color = if persisted {
        palette.shell
    } else {
        palette.disabled
    };
    let dot_left = if persisted {
        Length::Fill
    } else {
        Length::Fixed(2.0)
    };
    let dot_right = if persisted {
        Length::Fixed(2.0)
    } else {
        Length::Fill
    };

    let dot = container(Space::new())
        .width(10.0)
        .height(10.0)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(dot_color)),
            border: Border {
                radius: 5.0.into(),
                color: Color::TRANSPARENT,
                width: 0.0,
            },
            ..container::Style::default()
        });

    let pill_inner = Row::new()
        .push(Space::new().width(dot_left))
        .push(dot)
        .push(Space::new().width(dot_right))
        .align_y(Alignment::Center);

    let pill = container(pill_inner)
        .width(24.0)
        .height(14.0)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(pill_bg)),
            border: Border {
                radius: 7.0.into(),
                color: Color::TRANSPARENT,
                width: 0.0,
            },
            ..container::Style::default()
        });

    button(pill)
        .on_press(on_toggle)
        .padding(0)
        .style(|_: &iced::Theme, _: Status| button::Style {
            background: None,
            text_color: Color::TRANSPARENT,
            border: Border::default(),
            shadow: Shadow::default(),
            snap: false,
        })
        .into()
}

pub fn value_preview<'a, Msg: 'a>(
    palette: &'a ForgePalette,
    variant: &Variant,
) -> Element<'a, Msg> {
    let mono = font(FontRole::Monospace);
    let is_complex = matches!(variant, Variant::Array(_) | Variant::Object(_));
    let (content, color) = match variant {
        Variant::Int(n) => (n.to_string(), palette.text_primary),
        Variant::Float(f) => (f.to_string(), palette.text_primary),
        Variant::Bool(true) => ("true".to_owned(), palette.success),
        Variant::Bool(false) => ("false".to_owned(), palette.random),
        Variant::String(s) => (format!("\"{}\"", s), palette.text_primary),
        Variant::Array(v) => (format!("[{} items]", v.len()), palette.text_primary),
        Variant::Object(m) => (format!("{{{} keys}}", m.len()), palette.text_primary),
        Variant::Datetime(_) => (variant.to_string(), palette.text_primary),
    };

    let label = text(content).size(FONT_SM).font(mono).color(color);
    if is_complex {
        row![
            label,
            tabler_icon::<Msg>(Icon::ExternalLink, 11.0, palette.text_muted),
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .into()
    } else {
        label.into()
    }
}

pub fn data_screen_footer<'a, Msg: 'a>(
    palette: &'a ForgePalette,
    props: FooterProps<'a>,
) -> Element<'a, Msg> {
    let faint = palette.text_faint;
    let mono = font(FontRole::Monospace);

    let left: Vec<Element<'a, Msg>> = vec![
        text(props.position_info)
            .size(FONT_XS)
            .font(mono)
            .color(faint)
            .into(),
    ];

    let mut right: Vec<Element<'a, Msg>> = Vec::new();

    if let Some(storage) = props.storage_info {
        right.push(text(storage).size(FONT_XS).font(mono).color(faint).into());
    }

    if let Some(save) = props.save_info {
        let mut save_row = Row::new()
            .spacing(spf(Spacing::Xxs))
            .align_y(Alignment::Center);

        if props.live_indicator {
            let dot_color = palette.success;
            let dot =
                container(Space::new())
                    .width(6.0)
                    .height(6.0)
                    .style(move |_: &iced::Theme| container::Style {
                        background: Some(Background::Color(dot_color)),
                        border: Border {
                            radius: 3.0.into(),
                            color: Color::TRANSPARENT,
                            width: 0.0,
                        },
                        ..container::Style::default()
                    });
            save_row = save_row.push(dot);
        }

        save_row = save_row.push(text(save).size(FONT_XS).font(mono).color(faint));

        right.push(save_row.into());
    }

    crate::footer::status_footer(left, right, palette)
}

/// Wraps a row so it paints a subtle background tint whenever the pointer is
/// over any part of it, and (optionally) publishes that hover state into a
/// shared `Cell` for a [`hover_reveal`] cell to consume. Hover changes only
/// request a redraw — no `Message` round-trip — so every `data_table` row gets
/// the tint for free without per-screen state.
pub fn hover_row<'a, Msg: 'a>(
    content: Element<'a, Msg>,
    tint: Color,
    sink: Option<Rc<Cell<bool>>>,
) -> Element<'a, Msg> {
    HoverRow {
        content,
        tint,
        sink,
    }
    .into()
}

/// Cell wrapper that only draws (and only accepts interaction with) its content
/// while the shared `flag` is set — the row's [`hover_row`] flips the flag on
/// hover, so the wrapped controls appear on row hover and hide otherwise.
pub fn hover_reveal<'a, Msg: 'a>(
    content: Element<'a, Msg>,
    flag: Rc<Cell<bool>>,
) -> Element<'a, Msg> {
    HoverReveal { content, flag }.into()
}

#[derive(Default)]
struct HoverFlag {
    hovered: bool,
}

struct HoverRow<'a, Msg, Theme = iced::Theme, Renderer = iced::Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    content: Element<'a, Msg, Theme, Renderer>,
    tint: Color,
    sink: Option<Rc<Cell<bool>>>,
}

impl<'a, Msg, Theme, Renderer> Widget<Msg, Theme, Renderer> for HoverRow<'a, Msg, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<HoverFlag>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(HoverFlag::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
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
        if tree.state.downcast_ref::<HoverFlag>().hovered {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: layout.bounds(),
                    border: Border::default(),
                    shadow: Shadow::default(),
                    snap: false,
                },
                Background::Color(self.tint),
            );
        }
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Msg>,
        viewport: &Rectangle,
    ) {
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        let is_over = cursor.is_over(layout.bounds());
        let state = tree.state.downcast_mut::<HoverFlag>();
        if state.hovered != is_over {
            state.hovered = is_over;
            if let Some(sink) = &self.sink {
                sink.set(is_over);
            }
            shell.request_redraw();
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
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Msg, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Msg, Theme, Renderer> From<HoverRow<'a, Msg, Theme, Renderer>>
    for Element<'a, Msg, Theme, Renderer>
where
    Msg: 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    fn from(w: HoverRow<'a, Msg, Theme, Renderer>) -> Self {
        Element::new(w)
    }
}

struct HoverReveal<'a, Msg, Theme = iced::Theme, Renderer = iced::Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    content: Element<'a, Msg, Theme, Renderer>,
    flag: Rc<Cell<bool>>,
}

impl<'a, Msg, Theme, Renderer> Widget<Msg, Theme, Renderer>
    for HoverReveal<'a, Msg, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::stateless()
    }

    fn state(&self) -> tree::State {
        tree::State::None
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
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
        // Hidden until the row is hovered — layout still reserves the cell so
        // sibling columns never shift when the controls fade in.
        if self.flag.get() {
            self.content.as_widget().draw(
                &tree.children[0],
                renderer,
                theme,
                style,
                layout,
                cursor,
                viewport,
            );
        }
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Msg>,
        viewport: &Rectangle,
    ) {
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        if self.flag.get() {
            self.content.as_widget().mouse_interaction(
                &tree.children[0],
                layout,
                cursor,
                viewport,
                renderer,
            )
        } else {
            mouse::Interaction::default()
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Msg, Theme, Renderer>> {
        if !self.flag.get() {
            return None;
        }
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Msg, Theme, Renderer> From<HoverReveal<'a, Msg, Theme, Renderer>>
    for Element<'a, Msg, Theme, Renderer>
where
    Msg: 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    fn from(w: HoverReveal<'a, Msg, Theme, Renderer>) -> Self {
        Element::new(w)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA;

    #[test]
    fn variant_kind_colors_are_distinct() {
        let p = CATPPUCCIN_MOCHA;
        let kinds = [
            VariantKind::Int,
            VariantKind::Float,
            VariantKind::Bool,
            VariantKind::String,
            VariantKind::Datetime,
            VariantKind::Array,
            VariantKind::Object,
        ];
        let colors: Vec<Color> = kinds.iter().map(|k| variant_kind_color(*k, &p)).collect();
        for i in 0..colors.len() {
            for j in (i + 1)..colors.len() {
                assert_ne!(
                    colors[i].r, colors[j].r,
                    "VariantKind index {i} and {j} share identical red channel"
                );
            }
        }
    }
}
