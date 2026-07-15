use gpui::{
    AnyElement, App, ClickEvent, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Pixels, RenderOnce, Rgba, SharedString, StatefulInteractiveElement, Styled,
    Window, div, px,
};

use crate::icons::{Icon, icon};
use crate::palette::{ForgePalette, with_alpha};
use crate::tokens::{DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, FONT_SM, FONT_XS, Radius, radius};

// Off the shared `Spacing` scale on purpose: a chat line is density-neutral, so its
// geometry stays fixed. Pill paddings below do use the `Spacing` tokens.
const STRIPE_W: Pixels = px(2.0);
const PAD_V: Pixels = px(8.0);
const PAD_H: Pixels = px(12.0);
const META_BODY_GAP: Pixels = px(2.0);
const ICON_W: Pixels = px(13.0);
const ICON_SPACING: Pixels = px(8.0);
const BADGE_SPACING: Pixels = px(6.0);
const PLATFORM_TILE: Pixels = px(14.0);
const PLATFORM_TILE_CORNER: Pixels = px(3.22);
const PLATFORM_GLYPH: Pixels = px(7.0);
const SEPARATOR_H: Pixels = px(0.5);
const BODY_LINE_SPACING: Pixels = px(3.0);
const BODY_INSET: Pixels = px(21.0);
const USERNAME_SEP_GAP: Pixels = px(2.0);

const LH_XS: Pixels = px(15.6);
const LH_SM: Pixels = px(18.2);

const PILL_PAD_H: Pixels = px(6.0);
const PILL_PAD_V: Pixels = px(4.0);
const BADGE_PAD_H: Pixels = px(4.0);

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

// Vip/Bot/Turbo deliberately share `brand` (they never co-occur on one user).
pub(crate) fn badge_color(kind: BadgeKind, palette: &ForgePalette) -> Rgba {
    match kind {
        BadgeKind::Moderator => palette.success,
        BadgeKind::Vip => palette.brand,
        BadgeKind::Bot => palette.brand,
        BadgeKind::Subscriber => palette.info,
        BadgeKind::Broadcaster => palette.warning,
        BadgeKind::Partner => palette.accent_teal,
        BadgeKind::Premium => palette.accent_pink_light,
        BadgeKind::Founder => palette.disabled,
        BadgeKind::Turbo => palette.brand,
        BadgeKind::HypeTrain => palette.warning,
        BadgeKind::Bits => palette.bits,
        BadgeKind::BitsLeader => palette.bits,
    }
}

pub(crate) fn badge_label(kind: BadgeKind) -> &'static str {
    match kind {
        BadgeKind::Moderator => "MOD",
        BadgeKind::Vip => "VIP",
        BadgeKind::Subscriber => "SUB",
        BadgeKind::Bot => "BOT",
        BadgeKind::Broadcaster => "OWN",
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

impl ChatBody {
    fn triggered(&self) -> Option<SharedString> {
        match self {
            ChatBody::Subscription { triggered, .. }
            | ChatBody::Raid { triggered, .. }
            | ChatBody::Command { triggered, .. } => triggered.clone(),
            ChatBody::Message(_) | ChatBody::Cheer { .. } => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChatRow {
    pub timestamp: SharedString,
    pub platform: Platform,
    pub badges: Vec<BadgeKind>,
    pub username: SharedString,
    pub username_color: Rgba,
    pub body: ChatBody,
}

fn body_accent(body: &ChatBody, palette: &ForgePalette) -> (Rgba, Option<Rgba>) {
    match body {
        ChatBody::Message(_) | ChatBody::Command { .. } => (with_alpha(palette.brand, 0.0), None),
        ChatBody::Subscription { .. } => (palette.brand, Some(palette.elevated)),
        ChatBody::Cheer { .. } => (palette.warning, Some(palette.elevated)),
        ChatBody::Raid { .. } => (palette.random, Some(palette.elevated)),
    }
}

type UsernameClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// Two-line row: badges live on the meta line, so the username's start-x is fixed
/// regardless of badge count.
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

fn username_el(
    name: SharedString,
    color: Rgba,
    click: Option<(ElementId, UsernameClick)>,
) -> AnyElement {
    let base = div()
        .flex_none()
        .whitespace_nowrap()
        .font_family(DEFAULT_BODY_FAMILY)
        .text_size(FONT_SM)
        .line_height(LH_SM)
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

fn separator_el(palette: &ForgePalette) -> impl IntoElement {
    div()
        .flex_none()
        .whitespace_nowrap()
        .ml(USERNAME_SEP_GAP)
        .font_family(DEFAULT_BODY_FAMILY)
        .text_size(FONT_SM)
        .line_height(LH_SM)
        .text_color(palette.text_secondary)
        .child(": ")
}

fn message_el(text: SharedString, color: Rgba) -> impl IntoElement {
    div()
        .flex_1()
        .min_w(px(0.0))
        .font_family(DEFAULT_BODY_FAMILY)
        .text_size(FONT_SM)
        .line_height(LH_SM)
        .text_color(color)
        .child(text)
}

fn second_line_el(text: SharedString, color: Rgba) -> impl IntoElement {
    div()
        .pl(BODY_INSET)
        .font_family(DEFAULT_BODY_FAMILY)
        .text_size(FONT_SM)
        .line_height(LH_SM)
        .text_color(color)
        .child(text)
}

fn descriptor_el(text: SharedString, palette: &ForgePalette) -> impl IntoElement {
    div()
        .flex_none()
        .whitespace_nowrap()
        .font_family(DEFAULT_BODY_FAMILY)
        .text_size(FONT_SM)
        .line_height(LH_SM)
        .text_color(palette.text_secondary)
        .child(text)
}

fn platform_tile(platform: Platform, palette: &ForgePalette) -> impl IntoElement {
    div()
        .flex_none()
        .size(PLATFORM_TILE)
        .rounded(PLATFORM_TILE_CORNER)
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

fn badge_pill(kind: BadgeKind, palette: &ForgePalette) -> impl IntoElement {
    div()
        .flex_none()
        .h(LH_XS)
        .px(BADGE_PAD_H)
        .rounded(radius(Radius::Sm))
        .bg(badge_color(kind, palette))
        .flex()
        .items_center()
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.shell)
                .child(badge_label(kind)),
        )
}

fn triggered_pill(text: SharedString, palette: &ForgePalette) -> impl IntoElement {
    div()
        .flex_none()
        .py(PILL_PAD_V)
        .px(PILL_PAD_H)
        .rounded(radius(Radius::Sm))
        .bg(with_alpha(palette.success, 0.20))
        .font_family(DEFAULT_BODY_FAMILY)
        .text_size(FONT_XS)
        .text_color(palette.success)
        .child(text)
}

fn count_pill(text: SharedString, hue: Rgba, alpha: f32) -> impl IntoElement {
    div()
        .flex_none()
        .ml(BADGE_SPACING)
        .h(LH_SM)
        .px(PILL_PAD_H)
        .rounded(radius(Radius::Sm))
        .bg(with_alpha(hue, alpha))
        .flex()
        .items_center()
        .font_family(DEFAULT_BODY_FAMILY)
        .text_size(FONT_XS)
        .text_color(hue)
        .child(text)
}

fn command_pill(command: SharedString, palette: &ForgePalette) -> impl IntoElement {
    div()
        .flex_none()
        .h(LH_SM)
        .px(PILL_PAD_H)
        .rounded(radius(Radius::Sm))
        .bg(with_alpha(palette.surface_overlay, 0.25))
        .flex()
        .items_center()
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(FONT_XS)
        .text_color(palette.brand)
        .child(command)
}

fn event_line(
    glyph: Icon,
    glyph_color: Rgba,
    username: AnyElement,
    descriptor: SharedString,
    pill: Option<AnyElement>,
    palette: &ForgePalette,
) -> impl IntoElement {
    let mut cluster = div()
        .flex()
        .items_center()
        .child(username)
        .child(descriptor_el(descriptor, palette));
    if let Some(pill) = pill {
        cluster = cluster.child(pill);
    }
    div()
        .flex()
        .items_center()
        .gap(ICON_SPACING)
        .child(icon(glyph, ICON_W, glyph_color))
        .child(cluster)
}

impl RenderOnce for ChatRowView {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let ChatRowView {
            palette: p,
            data,
            click,
        } = self;
        let (stripe_color, body_bg) = body_accent(&data.body, &p);
        let triggered = data.body.triggered();
        let username = username_el(data.username, data.username_color, click);

        let left = div()
            .flex()
            .items_center()
            .gap(BADGE_SPACING)
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .line_height(LH_XS)
                    .text_color(p.text_faint)
                    .child(data.timestamp),
            )
            .child(platform_tile(data.platform, &p))
            .children(data.badges.iter().map(|&kind| badge_pill(kind, &p)));

        let mut meta = div()
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .h(LH_XS)
            .child(left);
        if let Some(t) = triggered {
            meta = meta.child(triggered_pill(t, &p));
        }

        let body: AnyElement = match data.body {
            ChatBody::Message(text) => div()
                .flex()
                .items_start()
                .child(username)
                .child(separator_el(&p))
                .child(message_el(text, p.text_primary))
                .into_any_element(),
            ChatBody::Command { command, .. } => div()
                .flex()
                .items_center()
                .child(username)
                .child(separator_el(&p))
                .child(command_pill(command, &p))
                .into_any_element(),
            ChatBody::Subscription {
                descriptor,
                months,
                message,
                ..
            } => {
                let pill = months.map(|m| {
                    count_pill(SharedString::from(format!("{m} mo")), p.warning, 0.15)
                        .into_any_element()
                });
                let line1 = event_line(Icon::Star, p.brand, username, descriptor, pill, &p);
                let mut col = div().flex().flex_col().gap(BODY_LINE_SPACING).child(line1);
                if let Some(msg) = message {
                    col = col.child(second_line_el(msg, p.text_muted));
                }
                col.into_any_element()
            }
            ChatBody::Cheer {
                descriptor,
                bits,
                text,
            } => {
                let pill = count_pill(SharedString::from(format!("{bits} bits")), p.warning, 0.20)
                    .into_any_element();
                let line1 = event_line(Icon::Coin, p.warning, username, descriptor, Some(pill), &p);
                div()
                    .flex()
                    .flex_col()
                    .gap(BODY_LINE_SPACING)
                    .child(line1)
                    .child(second_line_el(text, p.text_primary))
                    .into_any_element()
            }
            ChatBody::Raid {
                descriptor,
                viewers,
                ..
            } => {
                let pill = count_pill(viewers, p.random, 0.20).into_any_element();
                event_line(Icon::Flag, p.random, username, descriptor, Some(pill), &p)
                    .into_any_element()
            }
        };

        let mut content = div()
            .flex()
            .flex_col()
            .w_full()
            .border_l(STRIPE_W)
            .border_color(stripe_color)
            .py(PAD_V)
            .px(PAD_H)
            .gap(META_BODY_GAP)
            .child(meta)
            .child(body);
        if let Some(bg) = body_bg {
            content = content.bg(bg).rounded_r(radius(Radius::Sm));
        }

        div()
            .flex()
            .flex_col()
            .w_full()
            .border_b(SEPARATOR_H)
            .border_color(p.border_regular)
            .child(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA;

    /// Channel-wise colour identity. `Rgba` carries neither `Debug` nor `Eq`, so
    /// this stands in for the `assert_eq!` the hue assertions would otherwise reach
    /// for.
    fn same_rgba(a: Rgba, b: Rgba) -> bool {
        a.r == b.r && a.g == b.g && a.b == b.b && a.a == b.a
    }

    #[test]
    fn badge_maps_each_kind_to_its_hue_field_and_caption() {
        let p = CATPPUCCIN_MOCHA;

        // Guard: the distinct palette fields the badge map draws from are pairwise
        // distinct, so each per-kind hue row below has teeth — a mis-wire to a
        // neighbouring field (VIP onto `warning`, Subscriber onto `success`, ...)
        // resolves to a detectably different colour rather than a silent alias.
        // `brand` (Vip/Bot/Turbo) and `bits` (Bits/BitsLeader) are intentionally
        // shared within their own group and appear once here.
        let distinct = [
            p.success,
            p.brand,
            p.info,
            p.warning,
            p.accent_teal,
            p.accent_pink_light,
            p.disabled,
            p.bits,
        ];
        for i in 0..distinct.len() {
            for j in (i + 1)..distinct.len() {
                assert!(
                    !same_rgba(distinct[i], distinct[j]),
                    "badge hue fields {i} and {j} collide",
                );
            }
        }

        // Captions are deliberate abbreviations (OWN not OWNER, PRIME not PREMIUM,
        // HYPE not HYPE TRAIN) — a future "tidy-up" that expands them is the
        // regression this row pins.
        for (kind, hue, label) in [
            (BadgeKind::Moderator, p.success, "MOD"),
            (BadgeKind::Vip, p.brand, "VIP"),
            (BadgeKind::Subscriber, p.info, "SUB"),
            (BadgeKind::Bot, p.brand, "BOT"),
            (BadgeKind::Broadcaster, p.warning, "OWN"),
            (BadgeKind::Partner, p.accent_teal, "PARTNER"),
            (BadgeKind::Premium, p.accent_pink_light, "PRIME"),
            (BadgeKind::Founder, p.disabled, "FOUNDER"),
            (BadgeKind::Turbo, p.brand, "TURBO"),
            (BadgeKind::HypeTrain, p.warning, "HYPE"),
            (BadgeKind::Bits, p.bits, "BITS"),
            (BadgeKind::BitsLeader, p.bits, "BITS LEADER"),
        ] {
            assert!(same_rgba(badge_color(kind, &p), hue), "hue for {kind:?}");
            assert_eq!(badge_label(kind), label, "label for {kind:?}");
        }
    }

    #[test]
    fn platform_maps_each_source_to_its_tile_hue_and_letter() {
        let p = CATPPUCCIN_MOCHA;

        // Guard: the three tile hues are distinct, so a swapped arm returns a
        // detectably wrong colour rather than the same value on two sources.
        assert!(!same_rgba(p.brand, p.random));
        assert!(!same_rgba(p.random, p.info));
        assert!(!same_rgba(p.brand, p.info));

        for (platform, hue, letter) in [
            (Platform::Twitch, p.brand, "T"),
            (Platform::YouTube, p.random, "Y"),
            (Platform::Kick, p.info, "K"),
        ] {
            assert!(same_rgba(platform.color(&p), hue), "hue for {platform:?}");
            assert_eq!(platform.letter(), letter, "letter for {platform:?}");
        }
    }

    #[test]
    fn triggered_is_carried_only_by_the_pill_bearing_bodies() {
        // `triggered()` fans three variants' optional pill into one accessor and
        // must return `None` for the two bodies that carry no pill field
        // (Message, Cheer). Both the Some-passthrough and the structural `None`
        // are pinned here.
        let cases: [(ChatBody, Option<&str>); 7] = [
            (ChatBody::Message("hi".into()), None),
            (
                ChatBody::Subscription {
                    descriptor: "".into(),
                    months: None,
                    message: None,
                    triggered: Some("greet".into()),
                },
                Some("greet"),
            ),
            (
                ChatBody::Subscription {
                    descriptor: "".into(),
                    months: None,
                    message: None,
                    triggered: None,
                },
                None,
            ),
            (
                ChatBody::Cheer {
                    descriptor: "".into(),
                    bits: 0,
                    text: "".into(),
                },
                None,
            ),
            (
                ChatBody::Raid {
                    descriptor: "".into(),
                    viewers: "".into(),
                    triggered: Some("raid-fx".into()),
                },
                Some("raid-fx"),
            ),
            (
                ChatBody::Command {
                    command: "".into(),
                    triggered: Some("run · 12ms".into()),
                },
                Some("run · 12ms"),
            ),
            (
                ChatBody::Command {
                    command: "".into(),
                    triggered: None,
                },
                None,
            ),
        ];
        for (body, expected) in cases {
            let got = body.triggered();
            let got: Option<&str> = got.as_ref().map(|s| s.as_ref());
            assert_eq!(got, expected, "for {body:?}");
        }
    }
}
