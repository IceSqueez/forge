use std::borrow::Cow;

use iced::advanced::widget::{Operation, tree::Tree};
use iced::advanced::widget::{Widget, tree};
use iced::advanced::{Clipboard, Layout, Shell, layout, mouse, overlay, renderer};
use iced::{
    Alignment, Background, Border, Color, Element, Event, Length, Rectangle, Size, Vector,
    widget::{Space, button, column, container, row, stack, text},
};

use crate::icons::{Icon, tabler_icon};
use crate::palette::ForgePalette;
use crate::tokens::{
    BORDER_THIN, FONT_MD, FONT_XS, FontRole, ModalSize, Radius, Spacing, font, modal_width, radius,
    sp,
};

#[derive(Debug, Clone)]
pub struct ModalProps<'a, Msg> {
    pub title: Cow<'a, str>,
    pub subtitle: Option<Cow<'a, str>>,
    pub icon: Option<Icon>,
    pub icon_tint: Option<Color>,
    pub size: ModalSize,
    pub on_close: Msg,
    pub kbd_hint: Option<Cow<'a, str>>,
    /// Optional submit binding fired on ⌘/Ctrl+Enter while the modal is open.
    pub on_submit: Option<Msg>,
}

impl<'a, Msg> ModalProps<'a, Msg> {
    pub fn new(title: impl Into<Cow<'a, str>>, on_close: Msg) -> Self {
        Self {
            title: title.into(),
            subtitle: None,
            icon: None,
            icon_tint: None,
            size: ModalSize::Md,
            on_close,
            kbd_hint: None,
            on_submit: None,
        }
    }
}

fn icon_tile<'a, Msg: 'a>(palette: &'a ForgePalette, icon: Icon, tint: Color) -> Element<'a, Msg> {
    container(tabler_icon(icon, 15.0, tint))
        .width(28)
        .height(28)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(palette.surface_overlay)),
            border: Border {
                radius: radius(Radius::Sm).into(),
                color: Color::TRANSPARENT,
                width: 0.0,
            },
            ..container::Style::default()
        })
        .into()
}

pub fn modal<'a, Msg: Clone + 'a>(
    palette: &'a ForgePalette,
    props: ModalProps<'a, Msg>,
    body: Element<'a, Msg>,
    footer: Element<'a, Msg>,
) -> Element<'a, Msg> {
    let close_msg = props.on_close.clone();

    let title_el = text(props.title)
        .size(FONT_MD)
        .color(palette.text_primary)
        .font(font(FontRole::Body));

    let title_block: Element<'a, Msg> = if let Some(subtitle) = props.subtitle {
        column![
            title_el,
            text(subtitle)
                .size(FONT_XS)
                .color(palette.text_muted)
                .font(font(FontRole::Body)),
        ]
        .spacing(2)
        .into()
    } else {
        title_el.into()
    };

    let close_btn = button(tabler_icon(Icon::X, FONT_MD, palette.text_faint))
        .on_press(props.on_close.clone())
        .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
        .style(|_theme: &iced::Theme, _status| button::Style {
            background: None,
            border: Border::default(),
            text_color: Color::TRANSPARENT,
            shadow: iced::Shadow::default(),
            snap: false,
        });

    let mut header_items: Vec<Element<'a, Msg>> = Vec::new();
    if let Some(icon) = props.icon {
        let tint = props.icon_tint.unwrap_or(palette.brand);
        header_items.push(icon_tile(palette, icon, tint));
    }
    header_items.push(container(title_block).width(Length::Fill).into());
    header_items.push(close_btn.into());

    let header_row = row(header_items)
        .spacing(10)
        .align_y(Alignment::Center)
        .padding([sp(Spacing::Sm), sp(Spacing::Md)]);

    let header_container =
        container(header_row)
            .width(Length::Fill)
            .style(move |_theme: &iced::Theme| container::Style {
                border: Border {
                    color: palette.border_regular,
                    width: BORDER_THIN,
                    radius: 0.0.into(),
                },
                ..container::Style::default()
            });

    let body_container = container(body).padding([sp(Spacing::Md), sp(Spacing::Md)]);

    let mut footer_col = column![footer].spacing(6);
    if let Some(hint) = props.kbd_hint {
        let hint_el = text(hint)
            .size(FONT_XS)
            .color(palette.text_faint)
            .font(font(FontRole::Monospace));
        footer_col = footer_col.push(hint_el);
    }

    let footer_container = container(footer_col)
        .width(Length::Fill)
        .padding([sp(Spacing::Sm), sp(Spacing::Md)])
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(palette.shell)),
            border: Border {
                color: palette.border_regular,
                width: BORDER_THIN,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        });

    let card_content = column![header_container, body_container, footer_container];

    let card = container(card_content)
        .max_width(modal_width(props.size))
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(palette.elevated)),
            border: Border {
                color: palette.border_regular,
                width: BORDER_THIN,
                radius: radius(Radius::Lg).into(),
            },
            ..container::Style::default()
        });

    let backdrop = button(Space::new().width(Length::Fill).height(Length::Fill))
        .on_press(close_msg)
        .padding(0)
        .style(|_theme: &iced::Theme, _status| button::Style {
            background: Some(Background::Color(Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.6,
            })),
            border: Border::default(),
            text_color: Color::TRANSPARENT,
            shadow: iced::Shadow::default(),
            snap: false,
        });

    let centered_card = container(iced::widget::opaque(card))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center);

    ModalKeys {
        content: stack![backdrop, centered_card].into(),
        on_close: props.on_close,
        on_submit: props.on_submit,
    }
    .into()
}

/// Transparent wrapper that grants a `modal()` dialog keyboard affordance:
/// Escape publishes `on_close`, ⌘/Ctrl+Enter publishes `on_submit` when bound.
/// Every other event is forwarded untouched so nested `text_input` /
/// `text_editor` widgets keep full keyboard control (they do not consume
/// Escape, and only plain Enter, never ⌘/Ctrl+Enter).
struct ModalKeys<'a, Msg, Theme = iced::Theme, Renderer = iced::Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    content: Element<'a, Msg, Theme, Renderer>,
    on_close: Msg,
    on_submit: Option<Msg>,
}

impl<'a, Msg, Theme, Renderer> Widget<Msg, Theme, Renderer> for ModalKeys<'a, Msg, Theme, Renderer>
where
    Msg: Clone + 'a,
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
        if let Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. }) = event {
            use iced::keyboard::key::Named;
            if let iced::keyboard::Key::Named(named) = key {
                match named {
                    Named::Escape => {
                        shell.publish(self.on_close.clone());
                        return;
                    }
                    Named::Enter if modifiers.command() => {
                        if let Some(submit) = &self.on_submit {
                            shell.publish(submit.clone());
                            return;
                        }
                    }
                    _ => {}
                }
            }
        }

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

impl<'a, Msg, Theme, Renderer> From<ModalKeys<'a, Msg, Theme, Renderer>>
    for Element<'a, Msg, Theme, Renderer>
where
    Msg: Clone + 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    fn from(w: ModalKeys<'a, Msg, Theme, Renderer>) -> Self {
        Element::new(w)
    }
}
