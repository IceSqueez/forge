use iced::advanced::layout;
use iced::advanced::mouse;
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::Widget;
use iced::advanced::widget::tree::{self, Tree};
use iced::advanced::{Clipboard, Layout, Shell};
use iced::{
    Alignment, Background, Border, Color, Element, Event, Length, Padding, Point, Rectangle, Size,
    Vector,
    widget::{button, column, container, row, text},
};

use crate::icons::{Icon, tabler_icon};
use crate::palette::ForgePalette;
use crate::tokens::{Density, FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius, spacing};

pub enum MenuItem<Msg> {
    Item {
        label: String,
        on_press: Msg,
        icon: Option<Icon>,
        shortcut: Option<String>,
        color: Option<Color>,
        disabled: bool,
    },
    Divider,
    Header(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuPlacement {
    BottomLeft,
    BottomRight,
    TopLeft,
    TopRight,
}

pub struct RowAction<Msg> {
    pub icon: Icon,
    pub label: String,
    pub on_press: Msg,
    pub color: Option<Color>,
}

pub fn actionable_count<Msg>(items: &[MenuItem<Msg>]) -> usize {
    items
        .iter()
        .filter(|item| {
            matches!(
                **item,
                MenuItem::Item {
                    disabled: false,
                    ..
                }
            )
        })
        .count()
}

fn divider_el<'a, Msg: 'a>(palette: &'a ForgePalette) -> Element<'a, Msg> {
    let xs = spacing(Spacing::Xs, Density::Cozy) as f32;
    let border_color = palette.border_regular;
    container(
        container(iced::widget::Space::new().width(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fixed(1.0))
            .style(move |_theme: &iced::Theme| container::Style {
                background: Some(Background::Color(border_color)),
                ..container::Style::default()
            }),
    )
    .padding(Padding {
        top: xs / 2.0,
        right: 0.0,
        bottom: xs / 2.0,
        left: 0.0,
    })
    .into()
}

fn header_el<'a, Msg: 'a>(label: String, palette: &'a ForgePalette) -> Element<'a, Msg> {
    let xs = spacing(Spacing::Xs, Density::Cozy) as f32;
    let sm = spacing(Spacing::Sm, Density::Cozy) as f32;
    container(
        text(label)
            .size(FONT_XS)
            .font(font(FontRole::Monospace))
            .color(palette.text_muted),
    )
    .padding(Padding {
        top: xs,
        right: sm,
        bottom: xs / 2.0,
        left: sm,
    })
    .into()
}

fn item_el<'a, Msg: Clone + 'a>(
    label: String,
    on_press: Msg,
    icon: Option<Icon>,
    shortcut: Option<String>,
    item_color: Option<Color>,
    disabled: bool,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let xs = spacing(Spacing::Xs, Density::Cozy) as f32;
    let sm = spacing(Spacing::Sm, Density::Cozy) as f32;

    let text_color = if disabled {
        palette.text_faint
    } else {
        item_color.unwrap_or(palette.text_primary)
    };
    let icon_color = if disabled {
        palette.text_faint
    } else {
        item_color.unwrap_or(palette.text_secondary)
    };
    let faint = palette.text_faint;
    let surface_overlay = palette.surface_overlay;

    let mut children: Vec<Element<'a, Msg>> = Vec::new();

    if let Some(ic) = icon {
        children.push(tabler_icon(ic, FONT_SM, icon_color));
    }

    children.push(
        text(label)
            .size(FONT_SM)
            .font(font(FontRole::Body))
            .color(text_color)
            .width(Length::Fill)
            .into(),
    );

    if let Some(sc) = shortcut {
        children.push(
            text(sc)
                .size(FONT_XS)
                .font(font(FontRole::Monospace))
                .color(faint)
                .into(),
        );
    }

    let content_row = row(children).spacing(sm).align_y(Alignment::Center);

    let inner = container(content_row).padding(Padding {
        top: xs,
        right: sm,
        bottom: xs,
        left: sm,
    });

    let mut btn =
        button(inner)
            .width(Length::Fill)
            .padding(0)
            .style(move |_theme: &iced::Theme, status| button::Style {
                background: if !disabled {
                    match status {
                        button::Status::Hovered | button::Status::Pressed => {
                            Some(Background::Color(surface_overlay))
                        }
                        _ => None,
                    }
                } else {
                    None
                },
                text_color,
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: radius(Radius::Sm).into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            });

    if !disabled {
        btn = btn.on_press(on_press);
    }

    btn.into()
}

fn panel_el<'a, Msg: Clone + 'a>(
    items: Vec<MenuItem<Msg>>,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let xs = spacing(Spacing::Xs, Density::Cozy) as f32;
    let elevated = palette.elevated;
    let border_color = palette.border_input;

    let item_els: Vec<Element<'a, Msg>> = items
        .into_iter()
        .map(|item| match item {
            MenuItem::Divider => divider_el(palette),
            MenuItem::Header(label) => header_el(label, palette),
            MenuItem::Item {
                label,
                on_press,
                icon,
                shortcut,
                color,
                disabled,
            } => item_el(label, on_press, icon, shortcut, color, disabled, palette),
        })
        .collect();

    let col = column(item_els).padding(Padding {
        top: xs,
        right: 0.0,
        bottom: xs,
        left: 0.0,
    });

    container(col)
        .width(Length::Fixed(200.0))
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(elevated)),
            border: Border {
                color: border_color,
                width: 0.5,
                radius: radius(Radius::Md).into(),
            },
            ..container::Style::default()
        })
        .into()
}

pub fn menu_panel<'a, Msg: Clone + 'a>(
    items: Vec<MenuItem<Msg>>,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    panel_el(items, palette)
}

pub fn menu_button_trigger<'a, Msg: Clone + 'a>(
    trigger_icon: Icon,
    open: bool,
    on_toggle: Msg,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let surface_overlay = palette.surface_overlay;
    let faint = palette.text_faint;

    button(
        container(tabler_icon(trigger_icon, FONT_SM, faint))
            .width(Length::Fixed(28.0))
            .height(Length::Fixed(28.0))
            .align_x(Alignment::Center)
            .align_y(Alignment::Center),
    )
    .on_press(on_toggle)
    .padding(0)
    .style(move |_theme: &iced::Theme, status| button::Style {
        background: match status {
            button::Status::Hovered | button::Status::Pressed => {
                Some(Background::Color(surface_overlay))
            }
            _ => {
                if open {
                    Some(Background::Color(surface_overlay))
                } else {
                    None
                }
            }
        },
        text_color: faint,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: radius(Radius::Sm).into(),
        },
        shadow: iced::Shadow::default(),
        snap: false,
    })
    .into()
}

pub fn menu_button<'a, Msg: Clone + 'a>(
    trigger_icon: Icon,
    open: bool,
    on_toggle: Msg,
    on_dismiss: Msg,
    items: Vec<MenuItem<Msg>>,
    placement: MenuPlacement,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let trigger = menu_button_trigger(trigger_icon, open, on_toggle, palette);
    let panel = if open {
        Some(panel_el(items, palette))
    } else {
        None
    };
    MenuButton {
        trigger,
        panel,
        placement,
        on_dismiss,
    }
    .into()
}

struct MenuButton<'a, Msg, Theme = iced::Theme, Renderer = iced::Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    trigger: Element<'a, Msg, Theme, Renderer>,
    panel: Option<Element<'a, Msg, Theme, Renderer>>,
    placement: MenuPlacement,
    on_dismiss: Msg,
}

impl<'a, Msg, Theme, Renderer> From<MenuButton<'a, Msg, Theme, Renderer>>
    for Element<'a, Msg, Theme, Renderer>
where
    Msg: Clone + 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    fn from(w: MenuButton<'a, Msg, Theme, Renderer>) -> Self {
        Element::new(w)
    }
}

impl<'a, Msg, Theme, Renderer> Widget<Msg, Theme, Renderer> for MenuButton<'a, Msg, Theme, Renderer>
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
        let mut v = vec![Tree::new(&self.trigger)];
        if let Some(p) = &self.panel {
            v.push(Tree::new(p));
        }
        v
    }

    fn diff(&self, tree: &mut Tree) {
        match &self.panel {
            None => tree.diff_children(std::slice::from_ref(&self.trigger)),
            Some(p) => tree.diff_children(&[&self.trigger, p]),
        }
    }

    fn size(&self) -> Size<Length> {
        self.trigger.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.trigger
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
        self.trigger.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
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
        self.trigger.as_widget_mut().update(
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
        self.trigger.as_widget().mouse_interaction(
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
        layout: Layout<'_>,
        _renderer: &Renderer,
        _viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Msg, Theme, Renderer>> {
        let panel = self.panel.as_mut()?;
        let panel_tree = tree.children.get_mut(1)?;
        let trigger_bounds = layout.bounds() + translation;
        Some(overlay::Element::new(Box::new(MenuOverlay {
            panel,
            panel_tree,
            trigger_bounds,
            placement: self.placement,
            on_dismiss: self.on_dismiss.clone(),
        })))
    }
}

struct MenuOverlay<'b, 'a: 'b, Msg, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    panel: &'b mut Element<'a, Msg, Theme, Renderer>,
    panel_tree: &'b mut Tree,
    trigger_bounds: Rectangle,
    placement: MenuPlacement,
    on_dismiss: Msg,
}

impl<Msg, Theme, Renderer> overlay::Overlay<Msg, Theme, Renderer>
    for MenuOverlay<'_, '_, Msg, Theme, Renderer>
where
    Msg: Clone,
    Renderer: iced::advanced::Renderer,
{
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        let limits = layout::Limits::new(Size::ZERO, Size::INFINITE);
        let node = self
            .panel
            .as_widget_mut()
            .layout(self.panel_tree, renderer, &limits);
        let panel_sz = node.size();
        let tb = self.trigger_bounds;
        let (raw_x, raw_y) = match self.placement {
            MenuPlacement::BottomLeft => (tb.x, tb.y + tb.height),
            MenuPlacement::BottomRight => (tb.x + tb.width - panel_sz.width, tb.y + tb.height),
            MenuPlacement::TopLeft => (tb.x, tb.y - panel_sz.height),
            MenuPlacement::TopRight => (tb.x + tb.width - panel_sz.width, tb.y - panel_sz.height),
        };
        let x = raw_x.max(0.0).min((bounds.width - panel_sz.width).max(0.0));
        let y = raw_y
            .max(0.0)
            .min((bounds.height - panel_sz.height).max(0.0));
        node.move_to(Point::new(x, y))
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        let bounds = layout.bounds();
        self.panel.as_widget().draw(
            self.panel_tree,
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
        let panel_bounds = layout.bounds();
        let captured_before_panel = shell.is_event_captured();
        self.panel.as_widget_mut().update(
            self.panel_tree,
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            &panel_bounds,
        );
        if let Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) = event
            && let mouse::Cursor::Available(pos) = cursor
            && !self.trigger_bounds.contains(pos)
        {
            let inside_panel = panel_bounds.contains(pos);
            let panel_captured = !captured_before_panel && shell.is_event_captured();
            if !inside_panel || panel_captured {
                shell.publish(self.on_dismiss.clone());
            }
        }
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let bounds = layout.bounds();
        self.panel
            .as_widget()
            .mouse_interaction(self.panel_tree, layout, cursor, &bounds, renderer)
    }
}

pub fn row_actions<'a, Msg: Clone + 'a>(
    actions: Vec<RowAction<Msg>>,
    hovered: bool,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let xs = spacing(Spacing::Xs, Density::Cozy) as f32;
    let surface_overlay = palette.surface_overlay;
    let primary = palette.text_primary;

    let btns: Vec<Element<'a, Msg>> = actions
        .into_iter()
        .map(|action| {
            let default_color = if hovered {
                action.color.unwrap_or(palette.text_secondary)
            } else {
                palette.text_faint
            };
            let hover_color = action.color.unwrap_or(primary);

            button(
                container(tabler_icon(action.icon, FONT_SM, default_color)).padding(Padding {
                    top: xs,
                    right: xs,
                    bottom: xs,
                    left: xs,
                }),
            )
            .on_press(action.on_press)
            .padding(0)
            .style(move |_theme: &iced::Theme, status| button::Style {
                background: match status {
                    button::Status::Hovered | button::Status::Pressed => {
                        Some(Background::Color(surface_overlay))
                    }
                    _ => None,
                },
                text_color: match status {
                    button::Status::Hovered | button::Status::Pressed => hover_color,
                    _ => default_color,
                },
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: radius(Radius::Sm).into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            })
            .into()
        })
        .collect();

    row(btns).spacing(xs).align_y(Alignment::Center).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icons::Icon;
    use crate::palette::CATPPUCCIN_MOCHA;
    use iced::advanced::{Layout, Shell};

    #[test]
    fn menu_item_divider_is_constructable() {
        let _d: MenuItem<()> = MenuItem::Divider;
    }

    #[test]
    fn menu_item_header_carries_label() {
        let h: MenuItem<()> = MenuItem::Header("Section".to_string());
        assert!(matches!(h, MenuItem::Header(ref s) if s == "Section"));
    }

    #[test]
    fn menu_item_enabled_item_matches_disabled_false() {
        let i: MenuItem<u32> = MenuItem::Item {
            label: "Rename".to_string(),
            on_press: 1,
            icon: Some(Icon::InfoCircle),
            shortcut: None,
            color: None,
            disabled: false,
        };
        assert!(matches!(
            i,
            MenuItem::Item {
                disabled: false,
                ..
            }
        ));
    }

    #[test]
    fn menu_item_disabled_item_matches_disabled_true() {
        let i: MenuItem<u32> = MenuItem::Item {
            label: "Delete".to_string(),
            on_press: 2,
            icon: None,
            shortcut: Some("Del".to_string()),
            color: None,
            disabled: true,
        };
        assert!(matches!(i, MenuItem::Item { disabled: true, .. }));
    }

    #[test]
    fn menu_placement_all_variants_are_distinct() {
        assert_ne!(MenuPlacement::BottomLeft, MenuPlacement::BottomRight);
        assert_ne!(MenuPlacement::TopLeft, MenuPlacement::TopRight);
        assert_ne!(MenuPlacement::BottomLeft, MenuPlacement::TopLeft);
        assert_ne!(MenuPlacement::BottomRight, MenuPlacement::TopRight);
    }

    #[test]
    fn actionable_count_excludes_dividers_headers_and_disabled() {
        let items: Vec<MenuItem<u32>> = vec![
            MenuItem::Header("Section".to_string()),
            MenuItem::Item {
                label: "a".to_string(),
                on_press: 1,
                icon: None,
                shortcut: None,
                color: None,
                disabled: false,
            },
            MenuItem::Divider,
            MenuItem::Item {
                label: "b".to_string(),
                on_press: 2,
                icon: None,
                shortcut: None,
                color: None,
                disabled: true,
            },
            MenuItem::Item {
                label: "c".to_string(),
                on_press: 3,
                icon: None,
                shortcut: None,
                color: None,
                disabled: false,
            },
        ];
        assert_eq!(actionable_count(&items), 2);
    }

    #[test]
    fn actionable_count_empty_is_zero() {
        let items: Vec<MenuItem<()>> = vec![];
        assert_eq!(actionable_count(&items), 0);
    }

    #[test]
    fn actionable_count_all_non_items_is_zero() {
        let items: Vec<MenuItem<()>> = vec![
            MenuItem::Divider,
            MenuItem::Header("x".to_string()),
            MenuItem::Divider,
        ];
        assert_eq!(actionable_count(&items), 0);
    }

    #[test]
    fn menu_button_closed_has_one_child() {
        let elem: Element<u32> = menu_button(
            Icon::DotsVertical,
            false,
            0u32,
            1u32,
            vec![],
            MenuPlacement::BottomRight,
            &CATPPUCCIN_MOCHA,
        );
        assert_eq!(elem.as_widget().children().len(), 1);
    }

    #[test]
    fn menu_button_open_has_two_children() {
        let elem: Element<u32> = menu_button(
            Icon::DotsVertical,
            true,
            0u32,
            1u32,
            vec![],
            MenuPlacement::BottomRight,
            &CATPPUCCIN_MOCHA,
        );
        assert_eq!(elem.as_widget().children().len(), 2);
    }

    struct NullClipboard;
    impl Clipboard for NullClipboard {
        fn read(&self, _kind: iced::advanced::clipboard::Kind) -> Option<String> {
            None
        }
        fn write(&mut self, _kind: iced::advanced::clipboard::Kind, _contents: String) {}
    }

    fn space_tree() -> Tree {
        let s = iced::widget::Space::new();
        Tree {
            tag: Widget::<u32, iced::Theme, ()>::tag(&s),
            state: Widget::<u32, iced::Theme, ()>::state(&s),
            children: Widget::<u32, iced::Theme, ()>::children(&s),
        }
    }

    fn click_event() -> Event {
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
    }

    fn trigger_rect() -> Rectangle {
        Rectangle {
            x: 100.0,
            y: 100.0,
            width: 28.0,
            height: 28.0,
        }
    }

    #[test]
    fn outside_click_publishes_dismiss() {
        use iced::advanced::overlay::Overlay as _;

        let mut panel: Element<'static, u32, iced::Theme, ()> = iced::widget::Space::new().into();
        let mut pt = space_tree();
        let mut ov = MenuOverlay {
            panel: &mut panel,
            panel_tree: &mut pt,
            trigger_bounds: trigger_rect(),
            placement: MenuPlacement::BottomRight,
            on_dismiss: 99u32,
        };

        let node = ov.layout(&(), Size::new(1280.0, 800.0));
        let layout = Layout::new(&node);

        let mut messages: Vec<u32> = Vec::new();
        let mut shell = Shell::new(&mut messages);

        ov.update(
            &click_event(),
            layout,
            mouse::Cursor::Available(Point::new(500.0, 500.0)),
            &(),
            &mut NullClipboard,
            &mut shell,
        );

        assert_eq!(messages, vec![99u32]);
    }

    #[test]
    fn inside_panel_click_does_not_publish_dismiss() {
        use iced::advanced::overlay::Overlay as _;

        let mut panel: Element<'static, u32, iced::Theme, ()> =
            iced::widget::container(iced::widget::Space::new())
                .width(Length::Fixed(200.0))
                .height(Length::Fixed(160.0))
                .into();
        let mut pt = space_tree();
        let tr = trigger_rect();
        let mut ov = MenuOverlay {
            panel: &mut panel,
            panel_tree: &mut pt,
            trigger_bounds: tr,
            placement: MenuPlacement::BottomRight,
            on_dismiss: 99u32,
        };

        let node = ov.layout(&(), Size::new(1280.0, 800.0));
        let panel_b = node.bounds();
        let layout = Layout::new(&node);

        let mut messages: Vec<u32> = Vec::new();
        let mut shell = Shell::new(&mut messages);

        ov.update(
            &click_event(),
            layout,
            mouse::Cursor::Available(Point::new(
                panel_b.x + panel_b.width / 2.0,
                panel_b.y + panel_b.height / 2.0,
            )),
            &(),
            &mut NullClipboard,
            &mut shell,
        );

        assert!(messages.is_empty(), "click inside panel must not dismiss");
    }

    #[test]
    fn trigger_click_does_not_publish_dismiss() {
        use iced::advanced::overlay::Overlay as _;

        let mut panel: Element<'static, u32, iced::Theme, ()> = iced::widget::Space::new().into();
        let mut pt = space_tree();
        let tr = trigger_rect();
        let mut ov = MenuOverlay {
            panel: &mut panel,
            panel_tree: &mut pt,
            trigger_bounds: tr,
            placement: MenuPlacement::BottomRight,
            on_dismiss: 99u32,
        };

        let node = ov.layout(&(), Size::new(1280.0, 800.0));
        let layout = Layout::new(&node);

        let mut messages: Vec<u32> = Vec::new();
        let mut shell = Shell::new(&mut messages);

        ov.update(
            &click_event(),
            layout,
            mouse::Cursor::Available(Point::new(tr.x + 5.0, tr.y + 5.0)),
            &(),
            &mut NullClipboard,
            &mut shell,
        );

        assert!(messages.is_empty());
    }
}
