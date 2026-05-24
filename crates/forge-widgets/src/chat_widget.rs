use iced::advanced::layout;
use iced::advanced::mouse;
use iced::advanced::renderer;
use iced::advanced::svg;
use iced::advanced::text::{self, Paragraph as _};
use iced::advanced::widget::Widget;
use iced::advanced::widget::tree::{self, Tree};
use iced::advanced::{Clipboard, Layout, Shell};
use iced::{
    Border, Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Shadow, Size, Theme,
    Vector, alignment,
};

use crate::chat::{BadgeKind, ChatBody, ChatRow, Platform};
use crate::palette::ForgePalette;
use crate::tokens::{FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius, spf};

const STRIPE_W: f32 = 2.0;
const PAD_V: f32 = 8.0;
const PAD_H: f32 = 12.0;
const SPACING_INNER: f32 = 2.0;
const ICON_W: f32 = 13.0;
const ICON_SPACING: f32 = 8.0;
const BADGE_SPACING: f32 = 6.0;
const SEPARATOR_H: f32 = 0.5;
const BODY_LINE_SPACING: f32 = 3.0;

const STAR_SVG_BYTES: &[u8] = include_bytes!("../assets/icons/tabler/star.svg");
const BOLT_SVG_BYTES: &[u8] = include_bytes!("../assets/icons/tabler/bolt.svg");
const FLAG_SVG_BYTES: &[u8] = include_bytes!("../assets/icons/tabler/flag.svg");

fn line_height(font_size: f32) -> f32 {
    font_size * 1.3
}

fn shape_text<P>(
    content: &str,
    size: f32,
    text_font: Font,
    bounds: Size,
    wrapping: text::Wrapping,
) -> P
where
    P: text::Paragraph<Font = Font>,
{
    P::with_text(text::Text {
        content,
        bounds,
        size: Pixels(size),
        line_height: text::LineHeight::default(),
        font: text_font,
        align_x: text::Alignment::Default,
        align_y: alignment::Vertical::Top,
        shaping: text::Shaping::default(),
        wrapping,
    })
}

fn measure_text_width<P: text::Paragraph<Font = Font>>(
    content: &str,
    size: f32,
    text_font: Font,
) -> f32 {
    shape_text::<P>(
        content,
        size,
        text_font,
        Size::INFINITE,
        text::Wrapping::None,
    )
    .min_bounds()
    .width
}

fn badge_color(kind: BadgeKind, palette: ForgePalette) -> Color {
    match kind {
        BadgeKind::Moderator => palette.success,
        BadgeKind::Vip => palette.warning,
        BadgeKind::Bot => palette.brand,
        BadgeKind::Subscriber => palette.info,
        BadgeKind::Broadcaster => palette.random,
    }
}

fn badge_label(kind: BadgeKind) -> &'static str {
    match kind {
        BadgeKind::Moderator => "MOD",
        BadgeKind::Vip => "VIP",
        BadgeKind::Subscriber => "SUB",
        BadgeKind::Bot => "BOT",
        BadgeKind::Broadcaster => "LIVE",
    }
}

fn platform_name(platform: Platform) -> &'static str {
    match platform {
        Platform::Twitch => "Twitch",
        Platform::YouTube => "YouTube",
        Platform::Kick => "Kick",
        Platform::Trovo => "Trovo",
    }
}

fn triggered_label(body: &ChatBody) -> Option<String> {
    match body {
        ChatBody::Subscription {
            triggered_action, ..
        } => triggered_action
            .as_deref()
            .map(|a| format!("Triggered: {a}")),
        ChatBody::Raid {
            triggered_action, ..
        } => triggered_action
            .as_deref()
            .map(|a| format!("Triggered: {a}")),
        ChatBody::Command {
            action_name,
            action_duration_ms,
            ..
        } => match (action_name.as_deref(), *action_duration_ms) {
            (Some(name), Some(ms)) => Some(format!("{name} · {ms}ms")),
            (Some(name), None) => Some(name.to_owned()),
            _ => None,
        },
        _ => None,
    }
}

fn simple_text(content: String, size: f32, text_font: Font) -> text::Text<String, Font> {
    text::Text {
        content,
        bounds: Size::INFINITE,
        size: Pixels(size),
        line_height: text::LineHeight::default(),
        font: text_font,
        align_x: text::Alignment::Default,
        align_y: alignment::Vertical::Top,
        shaping: text::Shaping::default(),
        wrapping: text::Wrapping::None,
    }
}

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

#[derive(Default)]
struct ChatRowState<P: Default> {
    paragraphs: ChatRowParagraphs<P>,
    username_bounds: Rectangle,
    hovered: bool,
}

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
    R: iced::advanced::Renderer + text::Renderer<Font = Font> + svg::Renderer,
    R::Paragraph: Default,
{
    fn size(&self) -> Size<Length> {
        Size {
            width: Length::Fill,
            height: Length::Shrink,
        }
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<ChatRowState<R::Paragraph>>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(ChatRowState::<R::Paragraph>::default())
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &R, limits: &layout::Limits) -> layout::Node {
        let state = tree.state.downcast_mut::<ChatRowState<R::Paragraph>>();
        let max_w = limits.max().width;
        let content_w = max_w - STRIPE_W - PAD_H * 2.0;

        state.paragraphs.timestamp = shape_text::<R::Paragraph>(
            &self.data.timestamp,
            FONT_XS,
            font(FontRole::Monospace),
            Size::INFINITE,
            text::Wrapping::None,
        );
        state.paragraphs.platform = shape_text::<R::Paragraph>(
            platform_name(self.data.platform),
            FONT_XS,
            font(FontRole::Body),
            Size::INFINITE,
            text::Wrapping::None,
        );
        state.paragraphs.badges = self
            .data
            .badges
            .iter()
            .map(|&kind| {
                shape_text::<R::Paragraph>(
                    badge_label(kind),
                    FONT_XS,
                    font(FontRole::Body),
                    Size::INFINITE,
                    text::Wrapping::None,
                )
            })
            .collect();

        let triggered_text = triggered_label(&self.data.body);
        state.paragraphs.triggered = triggered_text.as_deref().map(|label| {
            shape_text::<R::Paragraph>(
                label,
                FONT_XS,
                font(FontRole::Body),
                Size::INFINITE,
                text::Wrapping::None,
            )
        });

        let top_row_h = line_height(FONT_XS);

        state.paragraphs.primary_body = shape_text::<R::Paragraph>(
            &self.data.username,
            FONT_SM,
            font(FontRole::Body),
            Size::INFINITE,
            text::Wrapping::None,
        );
        let uname_w = state.paragraphs.primary_body.min_bounds().width;

        let body_h = match &self.data.body {
            ChatBody::Message(msg) => {
                let sep_w = measure_text_width::<R::Paragraph>(": ", FONT_SM, font(FontRole::Body));
                let wrap_w = (content_w - uname_w - sep_w).max(1.0);
                state.paragraphs.secondary_body = Some(shape_text::<R::Paragraph>(
                    msg,
                    FONT_SM,
                    font(FontRole::Body),
                    Size::new(wrap_w, f32::INFINITY),
                    text::Wrapping::Word,
                ));
                state
                    .paragraphs
                    .secondary_body
                    .as_ref()
                    .map_or(line_height(FONT_SM), |p| {
                        p.min_bounds().height.max(line_height(FONT_SM))
                    })
            }
            ChatBody::Subscription { message, .. } => {
                let icon_offset = ICON_W + ICON_SPACING;
                let wrap_w = (content_w - icon_offset - uname_w).max(1.0);
                state.paragraphs.secondary_body = message.as_deref().map(|m| {
                    shape_text::<R::Paragraph>(
                        m,
                        FONT_SM,
                        font(FontRole::Body),
                        Size::new(wrap_w, f32::INFINITY),
                        text::Wrapping::Word,
                    )
                });
                line_height(FONT_SM)
                    + state.paragraphs.secondary_body.as_ref().map_or(0.0, |p| {
                        BODY_LINE_SPACING + p.min_bounds().height.max(line_height(FONT_SM))
                    })
            }
            ChatBody::Cheer {
                text: cheer_text, ..
            } => {
                let icon_offset = ICON_W + ICON_SPACING;
                let wrap_w = (content_w - icon_offset - uname_w).max(1.0);
                state.paragraphs.secondary_body = Some(shape_text::<R::Paragraph>(
                    cheer_text,
                    FONT_SM,
                    font(FontRole::Body),
                    Size::new(wrap_w, f32::INFINITY),
                    text::Wrapping::Word,
                ));
                line_height(FONT_SM)
                    + BODY_LINE_SPACING
                    + state
                        .paragraphs
                        .secondary_body
                        .as_ref()
                        .map_or(line_height(FONT_SM), |p| {
                            p.min_bounds().height.max(line_height(FONT_SM))
                        })
            }
            ChatBody::Raid { .. } | ChatBody::Command { .. } => {
                state.paragraphs.secondary_body = None;
                line_height(FONT_SM)
            }
        };

        state.username_bounds = Rectangle {
            x: STRIPE_W + PAD_H,
            y: PAD_V + top_row_h + SPACING_INNER,
            width: uname_w + 4.0,
            height: line_height(FONT_SM),
        };

        let total_h = PAD_V * 2.0 + top_row_h + SPACING_INNER + body_h + SEPARATOR_H;
        layout::Node::new(Size::new(max_w, total_h))
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut R,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<ChatRowState<R::Paragraph>>();
        let bounds = layout.bounds();
        let origin = bounds.position();
        let total_h = bounds.height;

        let (stripe_color, body_bg) = match &self.data.body {
            ChatBody::Message(_) | ChatBody::Command { .. } => {
                (Color::TRANSPARENT, Color::TRANSPARENT)
            }
            ChatBody::Subscription { .. } => (self.palette.brand, self.palette.elevated),
            ChatBody::Cheer { .. } => (self.palette.warning, self.palette.elevated),
            ChatBody::Raid { .. } => (self.palette.random, self.palette.elevated),
        };

        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle {
                    x: origin.x,
                    y: origin.y,
                    width: STRIPE_W,
                    height: total_h - SEPARATOR_H,
                },
                border: Border::default(),
                shadow: Shadow::default(),
                snap: false,
            },
            stripe_color,
        );

        if body_bg != Color::TRANSPARENT {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: origin.x + STRIPE_W,
                        y: origin.y,
                        width: bounds.width - STRIPE_W,
                        height: total_h - SEPARATOR_H,
                    },
                    border: Border {
                        radius: iced::border::left(0.0)
                            .top_right(radius(Radius::Sm))
                            .bottom_right(radius(Radius::Sm)),
                        color: Color::TRANSPARENT,
                        width: 0.0,
                    },
                    shadow: Shadow::default(),
                    snap: false,
                },
                body_bg,
            );
        }

        let top_row_h = line_height(FONT_XS);
        let top_row_y = origin.y + PAD_V;
        let content_x = origin.x + STRIPE_W + PAD_H;
        let mut cursor_x = content_x;

        renderer.fill_paragraph(
            &state.paragraphs.timestamp,
            Point {
                x: cursor_x,
                y: top_row_y,
            },
            self.palette.text_faint,
            *viewport,
        );
        cursor_x += state.paragraphs.timestamp.min_bounds().width + BADGE_SPACING;

        {
            let plat_text_w = state.paragraphs.platform.min_bounds().width;
            let plat_pad = spf(Spacing::Xs);
            let plat_w = plat_text_w + plat_pad * 2.0;
            let plat_color = self.data.platform.color(&self.palette);
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: cursor_x,
                        y: top_row_y,
                        width: plat_w,
                        height: top_row_h,
                    },
                    border: Border {
                        radius: radius(Radius::Sm).into(),
                        color: Color::TRANSPARENT,
                        width: 0.0,
                    },
                    shadow: Shadow::default(),
                    snap: false,
                },
                plat_color,
            );
            renderer.fill_paragraph(
                &state.paragraphs.platform,
                Point {
                    x: cursor_x + plat_pad,
                    y: top_row_y,
                },
                self.palette.shell,
                *viewport,
            );
            cursor_x += plat_w + BADGE_SPACING;
        }

        for (i, badge_para) in state.paragraphs.badges.iter().enumerate() {
            let kind = self.data.badges[i];
            let color = badge_color(kind, self.palette);
            let bg = Color { a: 0.18, ..color };
            let pad = spf(Spacing::Xxs);
            let pill_w = badge_para.min_bounds().width + pad * 2.0;
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: cursor_x,
                        y: top_row_y,
                        width: pill_w,
                        height: top_row_h,
                    },
                    border: Border {
                        radius: radius(Radius::Sm).into(),
                        color: Color::TRANSPARENT,
                        width: 0.0,
                    },
                    shadow: Shadow::default(),
                    snap: false,
                },
                bg,
            );
            renderer.fill_paragraph(
                badge_para,
                Point {
                    x: cursor_x + pad,
                    y: top_row_y,
                },
                color,
                *viewport,
            );
            cursor_x += pill_w + BADGE_SPACING;
        }

        if let Some(triggered_para) = &state.paragraphs.triggered {
            let pad_h = spf(Spacing::Xs);
            let pad_v = spf(Spacing::Xxs);
            let trigg_text_w = triggered_para.min_bounds().width;
            let trigg_w = trigg_text_w + pad_h * 2.0;
            let trigg_h = top_row_h + pad_v * 2.0;
            let trigg_x = (origin.x + bounds.width - PAD_H - trigg_w).max(cursor_x);
            let trigg_y = top_row_y - pad_v;
            let trigg_bg = Color {
                a: 0.20,
                ..self.palette.success
            };
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: trigg_x,
                        y: trigg_y,
                        width: trigg_w,
                        height: trigg_h,
                    },
                    border: Border {
                        radius: radius(Radius::Sm).into(),
                        color: Color::TRANSPARENT,
                        width: 0.0,
                    },
                    shadow: Shadow::default(),
                    snap: false,
                },
                trigg_bg,
            );
            renderer.fill_paragraph(
                triggered_para,
                Point {
                    x: trigg_x + pad_h,
                    y: top_row_y,
                },
                self.palette.success,
                *viewport,
            );
        }

        let body_y = origin.y + PAD_V + top_row_h + SPACING_INNER;
        let body_line_h = line_height(FONT_SM);

        match &self.data.body {
            ChatBody::Message(_) => {
                renderer.fill_paragraph(
                    &state.paragraphs.primary_body,
                    Point {
                        x: content_x,
                        y: body_y,
                    },
                    self.data.username_color,
                    *viewport,
                );
                let uname_w = state.paragraphs.primary_body.min_bounds().width;
                let sep_x = content_x + uname_w + 2.0;
                let sep_w = measure_text_width::<R::Paragraph>(": ", FONT_SM, font(FontRole::Body));
                renderer.fill_text(
                    simple_text(": ".to_owned(), FONT_SM, font(FontRole::Body)),
                    Point {
                        x: sep_x,
                        y: body_y,
                    },
                    self.palette.text_secondary,
                    *viewport,
                );
                if let Some(secondary) = &state.paragraphs.secondary_body {
                    renderer.fill_paragraph(
                        secondary,
                        Point {
                            x: sep_x + sep_w,
                            y: body_y,
                        },
                        self.palette.text_primary,
                        *viewport,
                    );
                }
            }
            ChatBody::Subscription { tier, months, .. } => {
                renderer.draw_svg(
                    svg::Svg::new(svg::Handle::from_memory(STAR_SVG_BYTES))
                        .color(self.palette.brand),
                    Rectangle {
                        x: content_x,
                        y: body_y,
                        width: ICON_W,
                        height: ICON_W,
                    },
                    *viewport,
                );
                let text_x = content_x + ICON_W + ICON_SPACING;
                renderer.fill_paragraph(
                    &state.paragraphs.primary_body,
                    Point {
                        x: text_x,
                        y: body_y,
                    },
                    self.data.username_color,
                    *viewport,
                );
                let uname_w = state.paragraphs.primary_body.min_bounds().width;
                let tier_label = format!(" subscribed (Tier {tier})");
                let tier_w =
                    measure_text_width::<R::Paragraph>(&tier_label, FONT_SM, font(FontRole::Body));
                renderer.fill_text(
                    simple_text(tier_label, FONT_SM, font(FontRole::Body)),
                    Point {
                        x: text_x + uname_w,
                        y: body_y,
                    },
                    self.palette.text_secondary,
                    *viewport,
                );
                if let Some(mo) = months {
                    let mo_label = format!("{mo} mo");
                    let mo_text_w = measure_text_width::<R::Paragraph>(
                        &mo_label,
                        FONT_XS,
                        font(FontRole::Body),
                    );
                    let mo_pad = spf(Spacing::Xxs);
                    let mo_bg = Color {
                        a: 0.15,
                        ..self.palette.warning
                    };
                    let mo_x = text_x + uname_w + tier_w + BADGE_SPACING;
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: Rectangle {
                                x: mo_x,
                                y: body_y,
                                width: mo_text_w + mo_pad * 2.0,
                                height: body_line_h,
                            },
                            border: Border {
                                radius: radius(Radius::Sm).into(),
                                color: Color::TRANSPARENT,
                                width: 0.0,
                            },
                            shadow: Shadow::default(),
                            snap: false,
                        },
                        mo_bg,
                    );
                    renderer.fill_text(
                        simple_text(mo_label, FONT_XS, font(FontRole::Body)),
                        Point {
                            x: mo_x + mo_pad,
                            y: body_y,
                        },
                        self.palette.warning,
                        *viewport,
                    );
                }
                if let Some(secondary) = &state.paragraphs.secondary_body {
                    renderer.fill_paragraph(
                        secondary,
                        Point {
                            x: text_x,
                            y: body_y + body_line_h + BODY_LINE_SPACING,
                        },
                        self.palette.text_muted,
                        *viewport,
                    );
                }
            }
            ChatBody::Cheer { bits, .. } => {
                renderer.draw_svg(
                    svg::Svg::new(svg::Handle::from_memory(BOLT_SVG_BYTES))
                        .color(self.palette.warning),
                    Rectangle {
                        x: content_x,
                        y: body_y,
                        width: ICON_W,
                        height: ICON_W,
                    },
                    *viewport,
                );
                let text_x = content_x + ICON_W + ICON_SPACING;
                renderer.fill_paragraph(
                    &state.paragraphs.primary_body,
                    Point {
                        x: text_x,
                        y: body_y,
                    },
                    self.data.username_color,
                    *viewport,
                );
                let uname_w = state.paragraphs.primary_body.min_bounds().width;
                let cheered_label = " cheered";
                let cheered_w = measure_text_width::<R::Paragraph>(
                    cheered_label,
                    FONT_SM,
                    font(FontRole::Body),
                );
                renderer.fill_text(
                    simple_text(cheered_label.to_owned(), FONT_SM, font(FontRole::Body)),
                    Point {
                        x: text_x + uname_w,
                        y: body_y,
                    },
                    self.palette.text_secondary,
                    *viewport,
                );
                let bits_label = format!("{bits} bits");
                let bits_text_w =
                    measure_text_width::<R::Paragraph>(&bits_label, FONT_XS, font(FontRole::Body));
                let bits_pad = spf(Spacing::Xxs);
                let bits_bg = Color {
                    a: 0.20,
                    ..self.palette.warning
                };
                let bits_x = text_x + uname_w + cheered_w + BADGE_SPACING;
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: Rectangle {
                            x: bits_x,
                            y: body_y,
                            width: bits_text_w + bits_pad * 2.0,
                            height: body_line_h,
                        },
                        border: Border {
                            radius: radius(Radius::Sm).into(),
                            color: Color::TRANSPARENT,
                            width: 0.0,
                        },
                        shadow: Shadow::default(),
                        snap: false,
                    },
                    bits_bg,
                );
                renderer.fill_text(
                    simple_text(bits_label, FONT_XS, font(FontRole::Body)),
                    Point {
                        x: bits_x + bits_pad,
                        y: body_y,
                    },
                    self.palette.warning,
                    *viewport,
                );
                if let Some(secondary) = &state.paragraphs.secondary_body {
                    renderer.fill_paragraph(
                        secondary,
                        Point {
                            x: text_x,
                            y: body_y + body_line_h + BODY_LINE_SPACING,
                        },
                        self.palette.text_primary,
                        *viewport,
                    );
                }
            }
            ChatBody::Raid { viewers, .. } => {
                renderer.draw_svg(
                    svg::Svg::new(svg::Handle::from_memory(FLAG_SVG_BYTES))
                        .color(self.palette.random),
                    Rectangle {
                        x: content_x,
                        y: body_y,
                        width: ICON_W,
                        height: ICON_W,
                    },
                    *viewport,
                );
                let text_x = content_x + ICON_W + ICON_SPACING;
                renderer.fill_paragraph(
                    &state.paragraphs.primary_body,
                    Point {
                        x: text_x,
                        y: body_y,
                    },
                    self.data.username_color,
                    *viewport,
                );
                let uname_w = state.paragraphs.primary_body.min_bounds().width;
                let raiding_label = " is raiding with";
                let raiding_w = measure_text_width::<R::Paragraph>(
                    raiding_label,
                    FONT_SM,
                    font(FontRole::Body),
                );
                renderer.fill_text(
                    simple_text(raiding_label.to_owned(), FONT_SM, font(FontRole::Body)),
                    Point {
                        x: text_x + uname_w,
                        y: body_y,
                    },
                    self.palette.text_secondary,
                    *viewport,
                );
                let viewers_label = format!("{viewers} viewers");
                let viewers_text_w = measure_text_width::<R::Paragraph>(
                    &viewers_label,
                    FONT_XS,
                    font(FontRole::Body),
                );
                let v_pad = spf(Spacing::Xxs);
                let v_bg = Color {
                    a: 0.20,
                    ..self.palette.random
                };
                let v_x = text_x + uname_w + raiding_w + BADGE_SPACING;
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: Rectangle {
                            x: v_x,
                            y: body_y,
                            width: viewers_text_w + v_pad * 2.0,
                            height: body_line_h,
                        },
                        border: Border {
                            radius: radius(Radius::Sm).into(),
                            color: Color::TRANSPARENT,
                            width: 0.0,
                        },
                        shadow: Shadow::default(),
                        snap: false,
                    },
                    v_bg,
                );
                renderer.fill_text(
                    simple_text(viewers_label, FONT_XS, font(FontRole::Body)),
                    Point {
                        x: v_x + v_pad,
                        y: body_y,
                    },
                    self.palette.random,
                    *viewport,
                );
            }
            ChatBody::Command { command, .. } => {
                renderer.fill_paragraph(
                    &state.paragraphs.primary_body,
                    Point {
                        x: content_x,
                        y: body_y,
                    },
                    self.data.username_color,
                    *viewport,
                );
                let uname_w = state.paragraphs.primary_body.min_bounds().width;
                let sep_x = content_x + uname_w + 2.0;
                let sep_w = measure_text_width::<R::Paragraph>(": ", FONT_SM, font(FontRole::Body));
                renderer.fill_text(
                    simple_text(": ".to_owned(), FONT_SM, font(FontRole::Body)),
                    Point {
                        x: sep_x,
                        y: body_y,
                    },
                    self.palette.text_secondary,
                    *viewport,
                );
                let cmd_label = command.clone();
                let cmd_text_w = measure_text_width::<R::Paragraph>(
                    &cmd_label,
                    FONT_XS,
                    font(FontRole::Monospace),
                );
                let cmd_pad = spf(Spacing::Xxs);
                let cmd_bg = Color {
                    a: 0.25,
                    ..self.palette.surface_overlay
                };
                let cmd_x = sep_x + sep_w;
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: Rectangle {
                            x: cmd_x,
                            y: body_y,
                            width: cmd_text_w + cmd_pad * 2.0,
                            height: body_line_h,
                        },
                        border: Border {
                            radius: radius(Radius::Sm).into(),
                            color: Color::TRANSPARENT,
                            width: 0.0,
                        },
                        shadow: Shadow::default(),
                        snap: false,
                    },
                    cmd_bg,
                );
                renderer.fill_text(
                    simple_text(cmd_label, FONT_XS, font(FontRole::Monospace)),
                    Point {
                        x: cmd_x + cmd_pad,
                        y: body_y,
                    },
                    self.palette.brand,
                    *viewport,
                );
            }
        }

        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle {
                    x: origin.x,
                    y: origin.y + total_h - SEPARATOR_H,
                    width: bounds.width,
                    height: SEPARATOR_H,
                },
                border: Border::default(),
                shadow: Shadow::default(),
                snap: false,
            },
            self.palette.border_regular,
        );
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _renderer: &R,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Msg>,
        _viewport: &Rectangle,
    ) {
        let Some(on_click) = self.on_user_click else {
            return;
        };
        let state = tree.state.downcast_mut::<ChatRowState<R::Paragraph>>();
        let origin = layout.bounds().position();
        let abs_username = state.username_bounds + Vector::new(origin.x, origin.y);

        match event {
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                state.hovered = abs_username.contains(*position);
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if state.hovered => {
                shell.publish(on_click(self.data.username.clone()));
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &R,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<ChatRowState<R::Paragraph>>();
        if state.hovered && self.on_user_click.is_some() {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::{ChatBody, Platform};
    use crate::palette::CATPPUCCIN_MOCHA;

    fn make_row(body: ChatBody) -> ChatRow {
        ChatRow {
            seq: 0,
            timestamp: "12:00:00".into(),
            platform: Platform::Twitch,
            badges: vec![],
            username: "test".into(),
            username_color: iced::Color::WHITE,
            body,
        }
    }

    fn layout_with_null_renderer(body: ChatBody) -> layout::Node {
        use iced::advanced::Widget as _;
        let mut widget: ChatRowWidget<()> =
            ChatRowWidget::new(CATPPUCCIN_MOCHA, make_row(body), None);
        let r: &() = &();
        let mut tree = Tree {
            tag: <ChatRowWidget<()> as Widget<(), Theme, ()>>::tag(&widget),
            state: <ChatRowWidget<()> as Widget<(), Theme, ()>>::state(&widget),
            children: vec![],
        };
        let limits = layout::Limits::new(Size::ZERO, Size::new(400.0, f32::INFINITY));
        widget.layout(&mut tree, r, &limits)
    }

    #[test]
    fn chat_row_widget_can_be_constructed_and_converted_to_element() {
        let widget: ChatRowWidget<()> = ChatRowWidget::new(
            CATPPUCCIN_MOCHA,
            make_row(ChatBody::Message("hello".into())),
            None,
        );
        let _element: iced::Element<'_, ()> = widget.into();
    }

    #[test]
    fn layout_message_returns_positive_height() {
        let node = layout_with_null_renderer(ChatBody::Message("hello world".into()));
        assert!(node.size().height > 0.0);
    }

    #[test]
    fn layout_subscription_returns_positive_height() {
        let node = layout_with_null_renderer(ChatBody::Subscription {
            tier: 1,
            months: Some(3),
            message: Some("hi".into()),
            triggered_action: None,
        });
        assert!(node.size().height > 0.0);
    }

    #[test]
    fn layout_cheer_returns_positive_height() {
        let node = layout_with_null_renderer(ChatBody::Cheer {
            bits: 100,
            text: "go!".into(),
        });
        assert!(node.size().height > 0.0);
    }

    #[test]
    fn layout_raid_returns_positive_height() {
        let node = layout_with_null_renderer(ChatBody::Raid {
            viewers: 50,
            triggered_action: None,
        });
        assert!(node.size().height > 0.0);
    }

    #[test]
    fn layout_command_returns_positive_height() {
        let node = layout_with_null_renderer(ChatBody::Command {
            command: "!ping".into(),
            action_name: Some("Ping".into()),
            action_duration_ms: Some(12),
        });
        assert!(node.size().height > 0.0);
    }
}
