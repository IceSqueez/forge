use forge_components::ActivePresentation;
use forge_components::Density;
use forge_components::FONT_LABEL;
use forge_components::FONT_MICRO;
use forge_components::FONT_ROLE_BADGE;
use forge_components::Radius;
use forge_components::Spacing;
use forge_components::radius;
use forge_components::spacing;
use gpui::{
    AnyElement, App, ClickEvent, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Pixels, RenderOnce, Rgba, SharedString, StatefulInteractiveElement, Styled,
    Window, div, px,
};

use forge_components::{BORDER_THIN, FONT_XS, FONT_XXS, body_family, mono_family, scaled};
use forge_components::{ForgePalette, with_alpha};
use forge_components::{Icon, icon};

const ROW_GAP: Pixels = px(8.0);

const ROW_PAD_T: Pixels = px(3.0);
const ROW_PAD_B: Pixels = px(3.0);
const ROW_PAD_L: Pixels = px(8.0);
const ROW_RADIUS: Pixels = px(5.0);

const EVENT_RADIUS: Pixels = radius(Radius::Sm);
const EVENT_MARGIN_V: Pixels = px(2.0);
const STRIPE_W: Pixels = px(2.0);
const EVENT_ICON: Pixels = px(14.0);
const ICON_TOP: Pixels = px(2.0);

const PLATFORM_TILE: Pixels = px(14.0);
const PLATFORM_CORNER: Pixels = px(3.0);
const PLATFORM_GLYPH: Pixels = px(8.0);

const ROLE_BADGE_FONT: Pixels = FONT_ROLE_BADGE;
const ROLE_BADGE_PAD_H: Pixels = px(5.0);
const ROLE_BADGE_PAD_V: Pixels = px(1.0);
const ROLE_BADGE_RADIUS: Pixels = px(4.0);
const ROLE_TINT_ALPHA: f32 = 0.07;
const ROLE_BORDER_ALPHA: f32 = 0.45;

const PILL_FONT: Pixels = FONT_MICRO;
const PILL_PAD_V: Pixels = px(1.0);
const PILL_RADIUS: Pixels = px(8.0);
const PILL_ICON: Pixels = px(9.0);

const CMD_PILL_FONT: Pixels = FONT_LABEL;
const CMD_PILL_PAD_H: Pixels = px(5.0);
const CMD_PILL_PAD_V: Pixels = px(1.0);
const CMD_PILL_RADIUS: Pixels = px(3.0);
const CMD_SECOND_INSET: Pixels = px(50.0);

const EVENT_MSG_FONT: Pixels = FONT_LABEL;
const EVENT_MSG_TOP: Pixels = px(1.0);
const TRIGGERED_TOP: Pixels = px(5.0);
const BODY_LINE_GAP: Pixels = px(2.0);

const LINE_BODY: Pixels = px(18.0);
const LINE_TIME: Pixels = px(15.75);

const REPLY_ICON: Pixels = px(11.0);
const REPLY_SNIPPET_MAX: usize = 80;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlatformMark {
    pub(crate) letter: SharedString,
    pub(crate) color: Rgba,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RoleBadge {
    pub(crate) label: SharedString,
    pub(crate) color: Rgba,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ChatBody {
    Message(SharedString),
    Subscription {
        descriptor: SharedString,
        months: Option<u32>,
        message: Option<SharedString>,
        triggered: Option<SharedString>,
    },
    Cheer {
        descriptor: SharedString,
        bits: u64,
        text: SharedString,
    },
    Raid {
        descriptor: SharedString,
        viewers: SharedString,
        triggered: Option<SharedString>,
    },
    Command {
        command: SharedString,
        triggered: Option<SharedString>,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct ChatRow {
    pub(crate) id: SharedString,
    pub(crate) timestamp: SharedString,
    pub(crate) platform: PlatformMark,
    pub(crate) badges: Vec<RoleBadge>,
    pub(crate) username: SharedString,
    pub(crate) username_color: Rgba,
    pub(crate) body: ChatBody,
    pub(crate) moderated: bool,
    pub(crate) reply: Option<(SharedString, SharedString)>,
}

type UsernameClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

#[derive(IntoElement)]
pub(crate) struct ChatRowView {
    palette: ForgePalette,
    data: ChatRow,
    click: Option<(ElementId, UsernameClick)>,
}

pub(crate) fn chat_row(palette: &ForgePalette, data: ChatRow) -> ChatRowView {
    ChatRowView {
        palette: *palette,
        data,
        click: None,
    }
}

impl ChatRowView {
    pub(crate) fn on_username_click(
        mut self,
        id: impl Into<ElementId>,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.click = Some((id.into(), Box::new(handler)));
        self
    }
}

fn time_el(ts: SharedString, palette: &ForgePalette) -> impl IntoElement {
    div()
        .flex_none()
        .whitespace_nowrap()
        .font_family(mono_family())
        .text_size(FONT_XXS)
        .line_height(LINE_TIME)
        .text_color(palette.text_faint)
        .child(ts)
}

fn platform_tile(platform: &PlatformMark, palette: &ForgePalette) -> impl IntoElement {
    div()
        .flex_none()
        .size(PLATFORM_TILE)
        .rounded(PLATFORM_CORNER)
        .bg(platform.color)
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .font_family(body_family())
                .font_weight(FontWeight::SEMIBOLD)
                .text_size(PLATFORM_GLYPH)
                .text_color(palette.shell)
                .child(platform.letter.clone()),
        )
}

fn username_el(
    name: SharedString,
    color: Rgba,
    click: Option<(ElementId, UsernameClick)>,
) -> AnyElement {
    let base = div()
        .flex_none()
        .whitespace_nowrap()
        .font_family(body_family())
        .font_weight(FontWeight::MEDIUM)
        .text_size(FONT_XS)
        .line_height(LINE_BODY)
        .text_color(color)
        .child(name);

    match click {
        Some((id, handler)) => base
            .id(id)
            .cursor_pointer()
            .border_b(BORDER_THIN)
            .border_color(with_alpha(color, 0.0))
            .hover(move |s| s.border_color(color))
            .on_click(handler)
            .into_any_element(),
        None => base.into_any_element(),
    }
}

fn message_el(text: SharedString, color: Rgba, struck: bool) -> impl IntoElement {
    let el = div()
        .flex_1()
        .min_w(px(0.0))
        .font_family(body_family())
        .text_size(FONT_XS)
        .line_height(LINE_BODY)
        .text_color(color);
    if struck {
        el.line_through().child(text)
    } else {
        el.child(text)
    }
}

fn descriptor_el(text: SharedString, palette: &ForgePalette) -> impl IntoElement {
    div()
        .flex_none()
        .whitespace_nowrap()
        .font_family(body_family())
        .text_size(FONT_XS)
        .line_height(LINE_BODY)
        .text_color(palette.text_secondary)
        .child(text)
}

fn command_pill(
    command: SharedString,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement {
    div()
        .flex_1()
        .min_w(px(0.0))
        .py(scaled(CMD_PILL_PAD_V, density))
        .px(scaled(CMD_PILL_PAD_H, density))
        .rounded(CMD_PILL_RADIUS)
        .bg(palette.surface_overlay)
        .font_family(mono_family())
        .text_size(CMD_PILL_FONT)
        .text_color(palette.brand)
        .child(command)
}

fn role_badge(badge: &RoleBadge, density: Density) -> impl IntoElement {
    let c = badge.color;
    div()
        .flex_none()
        .py(scaled(ROLE_BADGE_PAD_V, density))
        .px(scaled(ROLE_BADGE_PAD_H, density))
        .rounded(ROLE_BADGE_RADIUS)
        .border(BORDER_THIN)
        .border_color(with_alpha(c, ROLE_BORDER_ALPHA))
        .bg(with_alpha(c, ROLE_TINT_ALPHA))
        .font_family(mono_family())
        .font_weight(FontWeight::SEMIBOLD)
        .text_size(ROLE_BADGE_FONT)
        .text_color(c)
        .child(badge.label.clone())
}

fn role_badges(badges: &[RoleBadge], density: Density) -> Option<impl IntoElement> {
    if badges.is_empty() {
        return None;
    }
    Some(
        div()
            .flex_none()
            .ml(spacing(Spacing::Xxs, density))
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .children(badges.iter().map(|badge| role_badge(badge, density))),
    )
}

fn pill_badge(
    bg: Rgba,
    text_color: Rgba,
    mono: bool,
    leading: Option<(Icon, Rgba)>,
    label: SharedString,
    density: Density,
) -> impl IntoElement {
    let family = if mono { mono_family() } else { body_family() };
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xxs, density))
        .py(scaled(PILL_PAD_V, density))
        .px(spacing(Spacing::Xs, density))
        .rounded(PILL_RADIUS)
        .bg(bg)
        .children(leading.map(|(glyph, glyph_color)| icon(glyph, PILL_ICON, glyph_color)))
        .child(
            div()
                .font_family(family)
                .font_weight(FontWeight::MEDIUM)
                .text_size(PILL_FONT)
                .text_color(text_color)
                .child(label),
        )
}

fn event_glyph(glyph: Icon, color: Rgba) -> impl IntoElement {
    div()
        .flex_none()
        .mt(ICON_TOP)
        .child(icon(glyph, EVENT_ICON, color))
}

fn event_message_el(text: SharedString, color: Rgba, struck: bool) -> impl IntoElement {
    let el = div()
        .mt(EVENT_MSG_TOP)
        .font_family(body_family())
        .text_size(EVENT_MSG_FONT)
        .line_height(LINE_BODY)
        .text_color(color);
    if struck {
        el.line_through().child(text)
    } else {
        el.child(text)
    }
}

fn triggered_row(text: SharedString, palette: &ForgePalette, density: Density) -> impl IntoElement {
    div().mt(TRIGGERED_TOP).flex().child(pill_badge(
        palette.surface_overlay,
        palette.success,
        false,
        Some((Icon::Bolt, palette.success)),
        text,
        density,
    ))
}

fn reply_snippet(text: &str) -> String {
    let flat = text.replace(['\n', '\r'], " ");
    let trimmed = flat.trim();
    match trimmed.char_indices().nth(REPLY_SNIPPET_MAX) {
        Some((idx, _)) => format!("{}\u{2026}", &trimmed[..idx]),
        None => trimmed.to_owned(),
    }
}

fn reply_line(
    author: SharedString,
    text: SharedString,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xxs, density))
        .child(icon(Icon::ArrowBackUp, REPLY_ICON, palette.text_faint))
        .child(
            div()
                .flex_none()
                .font_family(body_family())
                .text_size(FONT_XXS)
                .line_height(LINE_TIME)
                .text_color(palette.text_muted)
                .child(SharedString::from(format!("@{author}"))),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .whitespace_nowrap()
                .overflow_hidden()
                .font_family(body_family())
                .text_size(FONT_XXS)
                .line_height(LINE_TIME)
                .text_color(palette.text_faint)
                .child(SharedString::from(reply_snippet(&text))),
        )
}

fn standard_row(
    data: &ChatRow,
    click: Option<(ElementId, UsernameClick)>,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement {
    let text = match &data.body {
        ChatBody::Message(t) => t.clone(),
        _ => SharedString::default(),
    };
    let color = if data.moderated {
        palette.text_muted
    } else {
        palette.text_secondary
    };
    let reply = data
        .reply
        .clone()
        .map(|(author, parent)| reply_line(author, parent, palette, density));
    let main = div()
        .flex()
        .items_center()
        .gap(scaled(ROW_GAP, density))
        .child(time_el(data.timestamp.clone(), palette))
        .child(platform_tile(&data.platform, palette))
        .child(username_el(
            data.username.clone(),
            data.username_color,
            click,
        ))
        .child(message_el(text, color, data.moderated))
        .children(role_badges(&data.badges, density));
    div()
        .id(data.id.clone())
        .flex()
        .flex_col()
        .gap(scaled(BODY_LINE_GAP, density))
        .pt(scaled(ROW_PAD_T, density))
        .pr(spacing(Spacing::Sm, density))
        .pb(scaled(ROW_PAD_B, density))
        .pl(scaled(ROW_PAD_L, density))
        .rounded(ROW_RADIUS)
        .hover(|s| s.bg(palette.elevated))
        .children(reply)
        .child(main)
}

fn command_row(
    data: &ChatRow,
    click: Option<(ElementId, UsernameClick)>,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement {
    let (command, triggered) = match &data.body {
        ChatBody::Command { command, triggered } => (command.clone(), triggered.clone()),
        _ => (SharedString::default(), None),
    };
    let main = div()
        .flex()
        .items_center()
        .gap(scaled(ROW_GAP, density))
        .child(time_el(data.timestamp.clone(), palette))
        .child(platform_tile(&data.platform, palette))
        .child(username_el(
            data.username.clone(),
            data.username_color,
            click,
        ))
        .child(command_pill(command, palette, density))
        .children(role_badges(&data.badges, density));
    let second = triggered.map(|t| {
        div().pl(CMD_SECOND_INSET).flex().child(pill_badge(
            palette.elevated,
            palette.success,
            false,
            Some((Icon::ArrowRight, palette.success)),
            t,
            density,
        ))
    });
    div()
        .id(data.id.clone())
        .flex()
        .flex_col()
        .gap(scaled(BODY_LINE_GAP, density))
        .pt(scaled(ROW_PAD_T, density))
        .pr(spacing(Spacing::Sm, density))
        .pb(scaled(ROW_PAD_B, density))
        .pl(scaled(ROW_PAD_L, density))
        .rounded(ROW_RADIUS)
        .hover(|s| s.bg(palette.elevated))
        .child(main)
        .children(second)
}

#[expect(
    clippy::too_many_arguments,
    reason = "render helper takes each displayed value and handler separately"
)]
fn event_row(
    timestamp: SharedString,
    stripe: Rgba,
    glyph: Icon,
    username: AnyElement,
    descriptor: SharedString,
    count: Option<AnyElement>,
    message: Option<(SharedString, Rgba, bool)>,
    triggered: Option<SharedString>,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement {
    let line1 = div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, density))
        .child(username)
        .child(descriptor_el(descriptor, palette))
        .children(count);
    let column = div()
        .flex_1()
        .flex()
        .flex_col()
        .child(line1)
        .children(message.map(|(text, color, struck)| event_message_el(text, color, struck)))
        .children(triggered.map(|t| triggered_row(t, palette, density)));
    div()
        .flex()
        .items_start()
        .gap(scaled(ROW_GAP, density))
        .my(scaled(EVENT_MARGIN_V, density))
        .py(spacing(Spacing::Xs, density))
        .px(spacing(Spacing::Sm, density))
        .bg(palette.elevated)
        .border_l(STRIPE_W)
        .border_color(stripe)
        .rounded_r(EVENT_RADIUS)
        .child(
            div()
                .flex_none()
                .mt(ICON_TOP)
                .child(time_el(timestamp, palette)),
        )
        .child(event_glyph(glyph, stripe))
        .child(column)
}

impl RenderOnce for ChatRowView {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let density = cx.density();
        let ChatRowView {
            palette: p,
            data,
            click,
        } = self;

        match &data.body {
            ChatBody::Message(_) => standard_row(&data, click, &p, density).into_any_element(),
            ChatBody::Command { .. } => command_row(&data, click, &p, density).into_any_element(),
            ChatBody::Subscription {
                descriptor,
                months,
                message,
                triggered,
            } => {
                let username = username_el(data.username.clone(), data.username_color, click);
                let count = months.as_ref().map(|m| {
                    pill_badge(
                        p.surface_overlay,
                        p.warning,
                        true,
                        None,
                        SharedString::from(format!("{m} mo")),
                        density,
                    )
                    .into_any_element()
                });
                let msg = message
                    .clone()
                    .map(|text| (text, p.text_muted, data.moderated));
                event_row(
                    data.timestamp.clone(),
                    p.theme_brand,
                    Icon::Star,
                    username,
                    descriptor.clone(),
                    count,
                    msg,
                    triggered.clone(),
                    &p,
                    density,
                )
                .into_any_element()
            }
            ChatBody::Cheer {
                descriptor,
                bits,
                text,
            } => {
                let username = username_el(data.username.clone(), data.username_color, click);
                let count = pill_badge(
                    p.warning,
                    p.shell,
                    false,
                    None,
                    SharedString::from(format!("{bits} bits")),
                    density,
                )
                .into_any_element();
                let color = if data.moderated {
                    p.text_muted
                } else {
                    p.text_primary
                };
                event_row(
                    data.timestamp.clone(),
                    p.warning,
                    Icon::Coin,
                    username,
                    descriptor.clone(),
                    Some(count),
                    Some((text.clone(), color, data.moderated)),
                    None,
                    &p,
                    density,
                )
                .into_any_element()
            }
            ChatBody::Raid {
                descriptor,
                viewers,
                triggered,
            } => {
                let username = username_el(data.username.clone(), p.danger, click);
                let count = pill_badge(p.danger, p.shell, false, None, viewers.clone(), density)
                    .into_any_element();
                event_row(
                    data.timestamp.clone(),
                    p.danger,
                    Icon::Flag,
                    username,
                    descriptor.clone(),
                    Some(count),
                    None,
                    triggered.clone(),
                    &p,
                    density,
                )
                .into_any_element()
            }
        }
    }
}
