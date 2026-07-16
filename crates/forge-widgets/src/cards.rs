use std::borrow::Cow;

use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding, Shadow};

use crate::icons::{Icon, tabler_icon};
use crate::palette::ForgePalette;
use crate::tokens::{
    BORDER_THIN, FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius, sp, spf,
};

fn card_style(
    bg: iced::Color,
    border_color: iced::Color,
    r: f32,
) -> impl Fn(&iced::Theme) -> container::Style {
    move |_theme: &iced::Theme| container::Style {
        background: Some(iced::Background::Color(bg)),
        border: Border {
            color: border_color,
            width: BORDER_THIN,
            radius: r.into(),
        },
        ..container::Style::default()
    }
}

/// Elevated surface panel matching the design-system `Card`.
///
/// Built via [`card`]. Renders as a bordered container by default and switches
/// to a hover-reactive button once [`Card::on_press`] is set. The child is
/// placed verbatim - no forced column wrapper - so callers own their layout.
pub struct Card<'a, Msg> {
    child: Element<'a, Msg>,
    padding: Padding,
    top_radius: f32,
    bottom_radius: f32,
    width: Length,
    background: Color,
    border_idle: Color,
    border_hover: Color,
    text_color: Color,
    on_press: Option<Msg>,
}

/// Wrap `child` in a standard card surface. Defaults reproduce the baseline
/// chrome: `Spacing::Md` padding, `Radius::Md` on every corner, elevated
/// background, thin regular border, shrink width, no hover reaction.
pub fn card<'a, Msg: 'a>(
    child: impl Into<Element<'a, Msg>>,
    palette: &ForgePalette,
) -> Card<'a, Msg> {
    let r = radius(Radius::Md);
    Card {
        child: child.into(),
        padding: Padding::from(spf(Spacing::Md)),
        top_radius: r,
        bottom_radius: r,
        width: Length::Shrink,
        background: palette.elevated,
        border_idle: palette.border_regular,
        border_hover: palette.border_input,
        text_color: palette.text_primary,
        on_press: None,
    }
}

impl<'a, Msg: 'a> Card<'a, Msg> {
    /// Override inner padding; accepts `0` for a flush, zero-inset surface.
    #[must_use]
    pub fn padding(mut self, padding: impl Into<Padding>) -> Self {
        self.padding = padding.into();
        self
    }

    /// Set the card width (defaults to `Length::Shrink`).
    #[must_use]
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// Override the surface background (defaults to `palette.elevated`).
    #[must_use]
    pub fn background(mut self, color: Color) -> Self {
        self.background = color;
        self
    }

    /// Uniform corner radius on all four corners.
    #[must_use]
    pub fn radius(mut self, r: Radius) -> Self {
        let v = radius(r);
        self.top_radius = v;
        self.bottom_radius = v;
        self
    }

    /// Split the corner radius: `top` rounds the header edge, `bottom` the body
    /// edge - used to butt a header bar flush against the panel below it. Pass
    /// `0.0` for a square edge (feed the token via [`radius`] for rounded ones).
    #[must_use]
    pub fn split_radius(mut self, top: f32, bottom: f32) -> Self {
        self.top_radius = top;
        self.bottom_radius = bottom;
        self
    }

    /// Make the card interactive: a press emits `msg` and hovering lifts the
    /// border to `palette.border_input`.
    #[must_use]
    pub fn on_press(mut self, msg: Msg) -> Self {
        self.on_press = Some(msg);
        self
    }
}

impl<'a, Msg: Clone + 'a> From<Card<'a, Msg>> for Element<'a, Msg> {
    fn from(card: Card<'a, Msg>) -> Self {
        let Card {
            child,
            padding,
            top_radius,
            bottom_radius,
            width,
            background,
            border_idle,
            border_hover,
            text_color,
            on_press,
        } = card;

        let corners = iced::border::Radius {
            top_left: top_radius,
            top_right: top_radius,
            bottom_right: bottom_radius,
            bottom_left: bottom_radius,
        };

        match on_press {
            Some(msg) => button(child)
                .on_press(msg)
                .padding(padding)
                .width(width)
                .style(move |_theme: &iced::Theme, status| button::Style {
                    background: Some(Background::Color(background)),
                    border: Border {
                        color: if matches!(status, button::Status::Hovered) {
                            border_hover
                        } else {
                            border_idle
                        },
                        width: BORDER_THIN,
                        radius: corners,
                    },
                    text_color,
                    shadow: Shadow::default(),
                    snap: false,
                })
                .into(),
            None => container(child)
                .padding(padding)
                .width(width)
                .style(move |_theme: &iced::Theme| container::Style {
                    background: Some(Background::Color(background)),
                    border: Border {
                        color: border_idle,
                        width: BORDER_THIN,
                        radius: corners,
                    },
                    ..container::Style::default()
                })
                .into(),
        }
    }
}

/// Shared list-row surface: a `leading` visual (status dot or icon), a
/// `title` + optional `meta` line, and an optional `trailing` control cluster
/// (toggle, badge, or a [`crate::row_actions`] / [`crate::menu_button`]
/// overflow menu). Standardizes the row scaffolding - spacing, padding, the
/// selected accent border, and the whole-row hover/press affordance - so list
/// screens stop hand-rolling it.
///
/// Built via [`row_card`]. Interactive rows ([`RowCard::on_press`]) hover to a
/// subtle overlay and, when [`RowCard::selected`], fill elevated with a full
/// accent border. `title`/`meta`/`trailing` accept arbitrary elements so a row
/// can host an inline rename `text_input` or a badge cluster verbatim.
pub struct RowCard<'a, Msg> {
    leading: Option<Element<'a, Msg>>,
    title: Element<'a, Msg>,
    meta: Option<Element<'a, Msg>>,
    trailing: Option<Element<'a, Msg>>,
    selected: bool,
    accent: Color,
    idle_bg: Color,
    selected_bg: Color,
    hover_bg: Color,
    idle_border: Option<Color>,
    border_width: f32,
    corner_radius: f32,
    padding: Padding,
    text_color: Color,
    on_press: Option<Msg>,
}

/// Start a row-card carrying `title`. Defaults: no leading/meta/trailing,
/// transparent idle background, `palette.elevated` selected fill, brand accent
/// border, `Spacing::Xs`/`Spacing::Md` inset, not interactive.
pub fn row_card<'a, Msg: 'a>(
    title: impl Into<Element<'a, Msg>>,
    palette: &ForgePalette,
) -> RowCard<'a, Msg> {
    RowCard {
        leading: None,
        title: title.into(),
        meta: None,
        trailing: None,
        selected: false,
        accent: palette.brand,
        idle_bg: Color::TRANSPARENT,
        selected_bg: palette.elevated,
        hover_bg: palette.surface_overlay,
        idle_border: None,
        border_width: 2.0,
        corner_radius: 0.0,
        padding: Padding {
            top: spf(Spacing::Xs),
            right: spf(Spacing::Md),
            bottom: spf(Spacing::Xs),
            left: spf(Spacing::Md),
        },
        text_color: palette.text_primary,
        on_press: None,
    }
}

impl<'a, Msg: 'a> RowCard<'a, Msg> {
    /// Leading visual placed before the title column (status dot or icon).
    #[must_use]
    pub fn leading(mut self, el: impl Into<Element<'a, Msg>>) -> Self {
        self.leading = Some(el.into());
        self
    }

    /// Secondary line rendered under the title (kind id, summary, path).
    #[must_use]
    pub fn meta(mut self, el: impl Into<Element<'a, Msg>>) -> Self {
        self.meta = Some(el.into());
        self
    }

    /// Trailing control cluster pinned to the row's right edge.
    #[must_use]
    pub fn trailing(mut self, el: impl Into<Element<'a, Msg>>) -> Self {
        self.trailing = Some(el.into());
        self
    }

    /// Mark the row selected: fills the selected background and draws a full
    /// accent border.
    #[must_use]
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Override the accent color used for the selected border (default brand).
    #[must_use]
    pub fn accent(mut self, color: Color) -> Self {
        self.accent = color;
        self
    }

    /// Idle (unselected) background fill; defaults to transparent so the row
    /// blends with its parent surface.
    #[must_use]
    pub fn idle_background(mut self, color: Color) -> Self {
        self.idle_bg = color;
        self
    }

    /// Override inner padding.
    #[must_use]
    pub fn padding(mut self, padding: impl Into<Padding>) -> Self {
        self.padding = padding.into();
        self
    }

    /// Make the row a whole-row press target emitting `msg`.
    #[must_use]
    pub fn on_press(mut self, msg: Msg) -> Self {
        self.on_press = Some(msg);
        self
    }

    /// Give the row a persistent (unselected) border + rounded corners, turning
    /// the flat list-row into a bordered card. The `selected` state still wins,
    /// swapping the border color for the accent.
    #[must_use]
    pub fn bordered(mut self, color: Color, width: f32, radius: f32) -> Self {
        self.idle_border = Some(color);
        self.border_width = width;
        self.corner_radius = radius;
        self
    }
}

impl<'a, Msg: Clone + 'a> From<RowCard<'a, Msg>> for Element<'a, Msg> {
    fn from(rc: RowCard<'a, Msg>) -> Self {
        let RowCard {
            leading,
            title,
            meta,
            trailing,
            selected,
            accent,
            idle_bg,
            selected_bg,
            hover_bg,
            idle_border,
            border_width,
            corner_radius,
            padding,
            text_color,
            on_press,
        } = rc;

        let idle_border_color = idle_border.unwrap_or(Color::TRANSPARENT);
        let corners: iced::border::Radius = corner_radius.into();

        let mut title_col = iced::widget::column![title].spacing(2.0);
        if let Some(meta) = meta {
            title_col = title_col.push(meta);
        }

        let mut cells: Vec<Element<'a, Msg>> = Vec::with_capacity(3);
        if let Some(leading) = leading {
            cells.push(
                container(leading)
                    .align_y(Alignment::Center)
                    .padding(Padding {
                        top: 0.0,
                        right: spf(Spacing::Xs),
                        bottom: 0.0,
                        left: 0.0,
                    })
                    .into(),
            );
        }
        cells.push(container(title_col).width(Length::Fill).into());
        if let Some(trailing) = trailing {
            cells.push(container(trailing).align_y(Alignment::Center).into());
        }

        let inner = row(cells)
            .spacing(spf(Spacing::Xs))
            .align_y(Alignment::Center);

        match on_press {
            Some(msg) => button(inner)
                .on_press(msg)
                .padding(padding)
                .width(Length::Fill)
                .style(move |_theme: &iced::Theme, status| {
                    let bg = if selected {
                        selected_bg
                    } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                        hover_bg
                    } else {
                        idle_bg
                    };
                    button::Style {
                        background: Some(Background::Color(bg)),
                        border: Border {
                            color: if selected { accent } else { idle_border_color },
                            width: border_width,
                            radius: corners,
                        },
                        text_color,
                        shadow: Shadow::default(),
                        snap: false,
                    }
                })
                .into(),
            None => container(inner)
                .padding(padding)
                .width(Length::Fill)
                .style(move |_theme: &iced::Theme| container::Style {
                    background: Some(Background::Color(if selected {
                        selected_bg
                    } else {
                        idle_bg
                    })),
                    border: Border {
                        color: if selected { accent } else { idle_border_color },
                        width: border_width,
                        radius: corners,
                    },
                    ..container::Style::default()
                })
                .into(),
        }
    }
}

pub fn metric_card<'a, Msg: 'a>(
    label: impl Into<Cow<'a, str>>,
    value: impl Into<Cow<'a, str>>,
    sublabel: Option<impl Into<Cow<'a, str>>>,
    sublabel_color: Option<Color>,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let bg = palette.elevated;
    let border_color = palette.border_regular;
    let label_color = palette.text_muted;
    let value_color = palette.text_primary;

    let label_str: Cow<'a, str> = label.into();
    let value_str: Cow<'a, str> = value.into();

    let mut col = iced::widget::column![
        iced::widget::text(label_str.to_uppercase())
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(label_color),
        iced::widget::text(value_str)
            .size(FONT_SM)
            .color(value_color),
    ]
    .spacing(4);

    if let Some(sub) = sublabel {
        let sub_str: Cow<'a, str> = sub.into();
        col = col.push(
            iced::widget::text(sub_str)
                .font(font(FontRole::Monospace))
                .size(FONT_XS)
                .color(sublabel_color.unwrap_or(palette.text_faint)),
        );
    }

    container(col)
        .padding(sp(Spacing::Md))
        .style(card_style(bg, border_color, radius(Radius::Md)))
        .into()
}

pub fn stat_row<'a, Msg: 'a>(
    label: impl Into<Cow<'a, str>>,
    value: impl Into<Cow<'a, str>>,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let label_color = palette.text_muted;
    let value_color = palette.text_primary;

    let label_str: Cow<'a, str> = label.into();
    let value_str: Cow<'a, str> = value.into();

    iced::widget::row![
        iced::widget::text(label_str)
            .size(FONT_SM)
            .color(label_color),
        iced::widget::Space::new().width(iced::Length::Fill),
        iced::widget::text(value_str)
            .size(FONT_SM)
            .color(value_color),
    ]
    .align_y(iced::alignment::Vertical::Center)
    .into()
}

pub struct BigJumpCardProps<Msg> {
    pub icon: Icon,
    pub icon_color: iced::Color,
    pub section_label: String,
    pub title: String,
    pub stat: String,
    pub stat_label: String,
    pub hint: String,
    pub on_press: Msg,
    pub warn: bool,
}

pub fn big_jump_card<'a, Msg: Clone + 'a>(
    props: BigJumpCardProps<Msg>,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let surface_overlay = palette.surface_overlay;
    let text_faint = palette.text_faint;
    let text_muted = palette.text_muted;
    let text_primary = palette.text_primary;
    let icon_color = props.icon_color;
    let warning = palette.warning;
    let border_regular = palette.border_regular;
    let border_input = palette.border_input;
    let elevated = palette.elevated;

    let icon_box = container(tabler_icon(props.icon, 18.0, icon_color))
        .width(34.0)
        .height(34.0)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(surface_overlay)),
            border: Border {
                radius: radius(Radius::Md).into(),
                color: iced::Color::TRANSPARENT,
                width: 0.0,
            },
            ..container::Style::default()
        });

    let label_col = column![
        text(props.section_label)
            .size(FONT_XS)
            .color(text_faint)
            .font(font(FontRole::Monospace)),
        text(props.title).size(FONT_SM).color(text_primary),
    ]
    .spacing(1.0);

    let warn_el: Element<'a, Msg> = if props.warn {
        tabler_icon(Icon::AlertTriangle, 14.0, warning)
    } else {
        iced::widget::Space::new().into()
    };

    let top_row = row![icon_box, label_col, warn_el]
        .spacing(10.0)
        .align_y(Alignment::Center);

    let mono_font = font(FontRole::Monospace);

    let stat_row = row![
        text(props.stat)
            .size(24.0)
            .color(icon_color)
            .font(mono_font),
        text(props.stat_label).size(FONT_SM).color(text_muted),
    ]
    .spacing(8.0)
    .align_y(Alignment::Center);

    let hint_row = row![
        text(props.hint)
            .size(FONT_XS)
            .color(text_faint)
            .width(Length::Fill),
        tabler_icon(Icon::ArrowRight, 12.0, text_faint),
    ]
    .spacing(4.0)
    .align_y(Alignment::Center);

    let content = column![top_row, stat_row, hint_row].spacing(10.0);

    button(content)
        .on_press(props.on_press)
        .padding(iced::Padding {
            top: spf(Spacing::Md),
            right: spf(Spacing::Md),
            bottom: spf(Spacing::Md),
            left: spf(Spacing::Md),
        })
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme, status| button::Style {
            background: Some(Background::Color(elevated)),
            border: Border {
                color: if matches!(status, button::Status::Hovered) {
                    border_input
                } else {
                    border_regular
                },
                width: 0.5,
                radius: radius(Radius::Md).into(),
            },
            text_color: text_primary,
            shadow: Shadow::default(),
            snap: false,
        })
        .into()
}
