use gpui::{
    AnyElement, App, ClickEvent, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Pixels, RenderOnce, Rgba, SharedString, StatefulInteractiveElement, Styled,
    Window, div, px,
};

use crate::icons::{Icon, icon};
use crate::palette::{ForgePalette, with_alpha};
use crate::tokens::{BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, FONT_XS, FONT_XXS};

// A chat line is density-neutral: its geometry stays fixed off the shared `Spacing` scale,
// and its pill/badge micro-sizes fall below the token grid, so they are named literals here.
const ROW_GAP: Pixels = px(8.0);
const LINE1_GAP: Pixels = px(6.0);

const ROW_PAD_T: Pixels = px(3.0);
const ROW_PAD_R: Pixels = px(10.0);
const ROW_PAD_B: Pixels = px(3.0);
const ROW_PAD_L: Pixels = px(8.0);
const ROW_RADIUS: Pixels = px(5.0);

const EVENT_PAD_V: Pixels = px(6.0);
const EVENT_PAD_H: Pixels = px(10.0);
const EVENT_RADIUS: Pixels = px(6.0);
const EVENT_MARGIN_V: Pixels = px(2.0);
const STRIPE_W: Pixels = px(2.0);
const EVENT_ICON: Pixels = px(14.0);
const ICON_TOP: Pixels = px(2.0);

const PLATFORM_TILE: Pixels = px(14.0);
const PLATFORM_CORNER: Pixels = px(3.0);
const PLATFORM_GLYPH: Pixels = px(8.0);

const ROLE_BADGE_FONT: Pixels = px(8.5);
const ROLE_BADGE_PAD_H: Pixels = px(5.0);
const ROLE_BADGE_PAD_V: Pixels = px(1.0);
const ROLE_BADGE_RADIUS: Pixels = px(4.0);
const ROLE_BADGE_GAP: Pixels = px(4.0);
const ROLE_BADGE_ML: Pixels = px(4.0);
const ROLE_TINT_ALPHA: f32 = 0.07;
const ROLE_BORDER_ALPHA: f32 = 0.45;

const PILL_FONT: Pixels = px(10.0);
const PILL_PAD_H: Pixels = px(6.0);
const PILL_PAD_V: Pixels = px(1.0);
const PILL_RADIUS: Pixels = px(8.0);
const PILL_GAP: Pixels = px(4.0);
const PILL_ICON: Pixels = px(9.0);

const CMD_PILL_FONT: Pixels = px(11.5);
const CMD_PILL_PAD_H: Pixels = px(5.0);
const CMD_PILL_PAD_V: Pixels = px(1.0);
const CMD_PILL_RADIUS: Pixels = px(3.0);
const CMD_SECOND_INSET: Pixels = px(50.0);

const EVENT_MSG_FONT: Pixels = px(11.5);
const EVENT_MSG_TOP: Pixels = px(1.0);
const TRIGGERED_TOP: Pixels = px(5.0);
const BODY_LINE_GAP: Pixels = px(2.0);

const LINE_BODY: Pixels = px(18.0);
const LINE_TIME: Pixels = px(15.75);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeKind {
    Moderator,
    Vip,
    Subscriber,
    Bot,
    Broadcaster,
    Partner,
    Premium,
    Founder,
    Turbo,
    HypeTrain,
    Bits,
    BitsLeader,
}

pub fn badge_color(kind: BadgeKind, palette: &ForgePalette) -> Rgba {
    match kind {
        BadgeKind::Broadcaster => palette.warning,
        BadgeKind::Moderator => palette.success,
        BadgeKind::Vip => palette.brand,
        BadgeKind::Subscriber => palette.bits,
        BadgeKind::Bot => palette.info,
        BadgeKind::Partner => palette.accent_teal,
        BadgeKind::Premium => palette.accent_pink_light,
        BadgeKind::Founder => palette.accent_pink_light,
        BadgeKind::Turbo => palette.brand,
        BadgeKind::HypeTrain => palette.warning,
        BadgeKind::Bits => palette.bits,
        BadgeKind::BitsLeader => palette.bits,
    }
}

pub fn badge_label(kind: BadgeKind) -> &'static str {
    match kind {
        BadgeKind::Broadcaster => "OWNER",
        BadgeKind::Moderator => "MOD",
        BadgeKind::Vip => "VIP",
        BadgeKind::Subscriber => "SUB",
        BadgeKind::Bot => "BOT",
        BadgeKind::Partner => "PARTNER",
        BadgeKind::Premium => "PRIME",
        BadgeKind::Founder => "FOUNDER",
        BadgeKind::Turbo => "TURBO",
        BadgeKind::HypeTrain => "HYPE",
        BadgeKind::Bits => "BITS",
        BadgeKind::BitsLeader => "BITS LEADER",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Twitch,
    YouTube,
    Kick,
}

impl Platform {
    // Semantic brand/random/info hues, not the raw `platform_*` brand fields, to stay in-theme.
    pub(crate) fn color(self, palette: &ForgePalette) -> Rgba {
        match self {
            Platform::Twitch => palette.brand,
            Platform::YouTube => palette.random,
            Platform::Kick => palette.info,
        }
    }

    pub(crate) fn letter(self) -> &'static str {
        match self {
            Platform::Twitch => "T",
            Platform::YouTube => "Y",
            Platform::Kick => "K",
        }
    }
}

/// Descriptors carry a leading space; the kit renders them verbatim (it owns no i18n).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatBody {
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
pub struct ChatRow {
    pub id: SharedString,
    pub timestamp: SharedString,
    pub platform: Platform,
    pub badges: Vec<BadgeKind>,
    pub username: SharedString,
    pub username_color: Rgba,
    pub body: ChatBody,
    pub moderated: bool,
}

type UsernameClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

#[derive(IntoElement)]
pub struct ChatRowView {
    palette: ForgePalette,
    data: ChatRow,
    click: Option<(ElementId, UsernameClick)>,
}

pub fn chat_row(palette: &ForgePalette, data: ChatRow) -> ChatRowView {
    ChatRowView {
        palette: *palette,
        data,
        click: None,
    }
}

impl ChatRowView {
    pub fn on_username_click(
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
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(FONT_XXS)
        .line_height(LINE_TIME)
        .text_color(palette.text_faint)
        .child(ts)
}

fn platform_tile(platform: Platform, palette: &ForgePalette) -> impl IntoElement {
    div()
        .flex_none()
        .size(PLATFORM_TILE)
        .rounded(PLATFORM_CORNER)
        .bg(platform.color(palette))
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .font_weight(FontWeight::SEMIBOLD)
                .text_size(PLATFORM_GLYPH)
                .text_color(palette.shell)
                .child(platform.letter()),
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
        .font_family(DEFAULT_BODY_FAMILY)
        .font_weight(FontWeight::MEDIUM)
        .text_size(FONT_XS)
        .line_height(LINE_BODY)
        .text_color(color)
        .child(name);

    match click {
        Some((id, handler)) => base
            .id(id)
            .cursor_pointer()
            .border_b(px(1.0))
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
        .font_family(DEFAULT_BODY_FAMILY)
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
        .font_family(DEFAULT_BODY_FAMILY)
        .text_size(FONT_XS)
        .line_height(LINE_BODY)
        .text_color(palette.text_secondary)
        .child(text)
}

fn command_pill(command: SharedString, palette: &ForgePalette) -> impl IntoElement {
    div()
        .flex_1()
        .min_w(px(0.0))
        .py(CMD_PILL_PAD_V)
        .px(CMD_PILL_PAD_H)
        .rounded(CMD_PILL_RADIUS)
        .bg(palette.surface_overlay)
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(CMD_PILL_FONT)
        .text_color(palette.brand)
        .child(command)
}

fn role_badge(kind: BadgeKind, palette: &ForgePalette) -> impl IntoElement {
    let c = badge_color(kind, palette);
    div()
        .flex_none()
        .py(ROLE_BADGE_PAD_V)
        .px(ROLE_BADGE_PAD_H)
        .rounded(ROLE_BADGE_RADIUS)
        .border(BORDER_THIN)
        .border_color(with_alpha(c, ROLE_BORDER_ALPHA))
        .bg(with_alpha(c, ROLE_TINT_ALPHA))
        .font_family(DEFAULT_MONO_FAMILY)
        .font_weight(FontWeight::SEMIBOLD)
        .text_size(ROLE_BADGE_FONT)
        .text_color(c)
        .child(badge_label(kind))
}

fn role_badges(badges: &[BadgeKind], palette: &ForgePalette) -> Option<impl IntoElement> {
    if badges.is_empty() {
        return None;
    }
    Some(
        div()
            .flex_none()
            .ml(ROLE_BADGE_ML)
            .flex()
            .items_center()
            .gap(ROLE_BADGE_GAP)
            .children(badges.iter().map(|&kind| role_badge(kind, palette))),
    )
}

fn pill_badge(
    bg: Rgba,
    text_color: Rgba,
    mono: bool,
    leading: Option<(Icon, Rgba)>,
    label: SharedString,
) -> impl IntoElement {
    let family = if mono {
        DEFAULT_MONO_FAMILY
    } else {
        DEFAULT_BODY_FAMILY
    };
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(PILL_GAP)
        .py(PILL_PAD_V)
        .px(PILL_PAD_H)
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
        .font_family(DEFAULT_BODY_FAMILY)
        .text_size(EVENT_MSG_FONT)
        .line_height(LINE_BODY)
        .text_color(color);
    if struck {
        el.line_through().child(text)
    } else {
        el.child(text)
    }
}

fn triggered_row(text: SharedString, palette: &ForgePalette) -> impl IntoElement {
    div().mt(TRIGGERED_TOP).flex().child(pill_badge(
        palette.surface_overlay,
        palette.success,
        false,
        Some((Icon::Bolt, palette.success)),
        text,
    ))
}

fn standard_row(
    data: &ChatRow,
    click: Option<(ElementId, UsernameClick)>,
    palette: &ForgePalette,
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
    div()
        .id(data.id.clone())
        .flex()
        .items_center()
        .gap(ROW_GAP)
        .pt(ROW_PAD_T)
        .pr(ROW_PAD_R)
        .pb(ROW_PAD_B)
        .pl(ROW_PAD_L)
        .rounded(ROW_RADIUS)
        .hover(|s| s.bg(palette.elevated))
        .child(time_el(data.timestamp.clone(), palette))
        .child(platform_tile(data.platform, palette))
        .child(username_el(
            data.username.clone(),
            data.username_color,
            click,
        ))
        .child(message_el(text, color, data.moderated))
        .children(role_badges(&data.badges, palette))
}

fn command_row(
    data: &ChatRow,
    click: Option<(ElementId, UsernameClick)>,
    palette: &ForgePalette,
) -> impl IntoElement {
    let (command, triggered) = match &data.body {
        ChatBody::Command { command, triggered } => (command.clone(), triggered.clone()),
        _ => (SharedString::default(), None),
    };
    let main = div()
        .flex()
        .items_center()
        .gap(ROW_GAP)
        .child(time_el(data.timestamp.clone(), palette))
        .child(platform_tile(data.platform, palette))
        .child(username_el(
            data.username.clone(),
            data.username_color,
            click,
        ))
        .child(command_pill(command, palette))
        .children(role_badges(&data.badges, palette));
    let second = triggered.map(|t| {
        div().pl(CMD_SECOND_INSET).flex().child(pill_badge(
            palette.elevated,
            palette.success,
            false,
            Some((Icon::ArrowRight, palette.success)),
            t,
        ))
    });
    div()
        .id(data.id.clone())
        .flex()
        .flex_col()
        .gap(BODY_LINE_GAP)
        .pt(ROW_PAD_T)
        .pr(ROW_PAD_R)
        .pb(ROW_PAD_B)
        .pl(ROW_PAD_L)
        .rounded(ROW_RADIUS)
        .hover(|s| s.bg(palette.elevated))
        .child(main)
        .children(second)
}

#[allow(clippy::too_many_arguments)]
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
) -> impl IntoElement {
    let line1 = div()
        .flex()
        .items_center()
        .gap(LINE1_GAP)
        .child(username)
        .child(descriptor_el(descriptor, palette))
        .children(count);
    let column = div()
        .flex_1()
        .flex()
        .flex_col()
        .child(line1)
        .children(message.map(|(text, color, struck)| event_message_el(text, color, struck)))
        .children(triggered.map(|t| triggered_row(t, palette)));
    div()
        .flex()
        .items_start()
        .gap(ROW_GAP)
        .my(EVENT_MARGIN_V)
        .py(EVENT_PAD_V)
        .px(EVENT_PAD_H)
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
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let ChatRowView {
            palette: p,
            data,
            click,
        } = self;

        match &data.body {
            ChatBody::Message(_) => standard_row(&data, click, &p).into_any_element(),
            ChatBody::Command { .. } => command_row(&data, click, &p).into_any_element(),
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
                    )
                    .into_any_element()
                });
                let msg = message
                    .clone()
                    .map(|text| (text, p.text_muted, data.moderated));
                event_row(
                    data.timestamp.clone(),
                    p.brand,
                    Icon::Star,
                    username,
                    descriptor.clone(),
                    count,
                    msg,
                    triggered.clone(),
                    &p,
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
                )
                .into_any_element()
            }
            ChatBody::Raid {
                descriptor,
                viewers,
                triggered,
            } => {
                let username = username_el(data.username.clone(), p.random, click);
                let count =
                    pill_badge(p.random, p.shell, false, None, viewers.clone()).into_any_element();
                event_row(
                    data.timestamp.clone(),
                    p.random,
                    Icon::Flag,
                    username,
                    descriptor.clone(),
                    Some(count),
                    None,
                    triggered.clone(),
                    &p,
                )
                .into_any_element()
            }
        }
    }
}
