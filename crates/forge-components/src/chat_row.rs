use gpui::{
    AnyElement, App, ClickEvent, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Pixels, RenderOnce, Rgba, SharedString, StatefulInteractiveElement, Styled,
    Window, div, px,
};

use crate::icons::{Icon, icon};
use crate::palette::{ForgePalette, with_alpha};
use crate::tokens::{DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, FONT_SM, FONT_XS, Radius, radius};

// Chat-row geometry. These sit deliberately off the shared `Spacing` scale — a
// chat line is a fixed, density-neutral surface whose stripe, tile, and inset
// are carried as exact literals so the two-line rhythm never drifts with a
// density change. Pill paddings, by contrast, do use the `Spacing` tokens below.
const STRIPE_W: Pixels = px(2.0);
const PAD_V: Pixels = px(8.0);
const PAD_H: Pixels = px(12.0);
/// Gap between the meta row (line 1) and the body row (line 2).
const META_BODY_GAP: Pixels = px(2.0);
/// Leading event-glyph box (star / coin / flag) width.
const ICON_W: Pixels = px(13.0);
/// Gap after the leading event glyph.
const ICON_SPACING: Pixels = px(8.0);
/// Gap between meta-row items (timestamp, platform tile, badges).
const BADGE_SPACING: Pixels = px(6.0);
/// Platform indicator: a rounded square letter tile (not a status dot).
const PLATFORM_TILE: Pixels = px(14.0);
/// Corner of the platform tile — `PLATFORM_TILE * 0.23`.
const PLATFORM_TILE_CORNER: Pixels = px(3.22);
/// Glyph size inside the platform tile — `PLATFORM_TILE * 0.5`.
const PLATFORM_GLYPH: Pixels = px(7.0);
/// Bottom hairline between rows.
const SEPARATOR_H: Pixels = px(0.5);
/// Extra gap before an event row's optional second line.
const BODY_LINE_SPACING: Pixels = px(3.0);
/// Body-row left inset for event rows (`ICON_W + ICON_SPACING`), keeping a
/// second-line message aligned under the username rather than the glyph.
const BODY_INSET: Pixels = px(21.0);
/// Gap between the username and its trailing `": "` separator.
const USERNAME_SEP_GAP: Pixels = px(2.0);

/// Line box for `FONT_XS` text — `FONT_XS * 1.3`.
const LH_XS: Pixels = px(15.6);
/// Line box for `FONT_SM` text — `FONT_SM * 1.3`.
const LH_SM: Pixels = px(18.2);

/// Pill horizontal inset (`Spacing::Xs`, density-neutral) and badge inset
/// (`Spacing::Xxs`). Resolved as literals here to match the source's default
/// density without threading a `Density` through a display-only row.
const PILL_PAD_H: Pixels = px(6.0);
const PILL_PAD_V: Pixels = px(4.0);
const BADGE_PAD_H: Pixels = px(4.0);

/// A chat-participant badge. Each maps to a fixed `ForgePalette` hue in
/// [`badge_color`], so the badge re-tints with the active theme.
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

/// The solid fill hue for a badge pill. VIP intentionally shares `brand` with
/// Bot/Turbo (they never co-occur on one user) but is kept apart from the
/// `warning` yellow the sub-months pill uses and from the badges it co-renders
/// with (Subscriber, Moderator).
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

/// The short uppercase caption drawn inside a badge pill.
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

/// The chat source a row originated from. Drives the platform tile's fill and
/// letter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Twitch,
    YouTube,
    Kick,
}

impl Platform {
    /// The platform tile fill. Uses the semantic `brand`/`random`/`info` hues
    /// (not the raw `platform_*` brand-color fields) so the tile stays inside
    /// the active theme's accent set.
    pub(crate) fn color(self, palette: &ForgePalette) -> Rgba {
        match self {
            Platform::Twitch => palette.brand,
            Platform::YouTube => palette.random,
            Platform::Kick => palette.info,
        }
    }

    /// The single-letter tile glyph.
    pub(crate) fn letter(self) -> &'static str {
        match self {
            Platform::Twitch => "T",
            Platform::YouTube => "Y",
            Platform::Kick => "K",
        }
    }
}

/// The kind of chat line, carrying its already-translated display strings. The
/// kit owns no i18n, so descriptors ("subscribed", "cheered", "raiding with"),
/// the raid viewer caption, and any triggered-action label are composed by the
/// caller and passed in; numeric pills whose format is non-translated (`N mo`,
/// `N bits`) are formatted in-row from the raw counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatBody {
    /// A plain message: `username: text`, with the body wrapping under the
    /// message start.
    Message(SharedString),
    /// A subscription notice on an accented row.
    Subscription {
        /// Translated descriptor, e.g. `" subscribed at tier 1"` (leading space
        /// included by the caller).
        descriptor: SharedString,
        /// Tenure in months; rendered as an `N mo` pill when present.
        months: Option<u32>,
        /// Optional accompanying message on the second line.
        message: Option<SharedString>,
        /// Optional right-aligned "triggered action" pill.
        triggered: Option<SharedString>,
    },
    /// A bits cheer on an accented row.
    Cheer {
        /// Translated descriptor, e.g. `" cheered"`.
        descriptor: SharedString,
        /// Bit count; rendered as an `N bits` pill.
        bits: u64,
        /// The cheer message shown on the second line.
        text: SharedString,
    },
    /// A raid notice on an accented row.
    Raid {
        /// Translated descriptor, e.g. `" raiding with"`.
        descriptor: SharedString,
        /// Translated viewer-count caption, e.g. `"512 viewers"`.
        viewers: SharedString,
        /// Optional right-aligned "triggered action" pill.
        triggered: Option<SharedString>,
    },
    /// A command line: `username: !command` with the command in a mono pill.
    Command {
        /// The raw command text (monospace pill).
        command: SharedString,
        /// Optional right-aligned pill, e.g. `"greet · 12ms"`.
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

/// One rendered chat line: identity (timestamp, platform, badges, username) plus
/// the typed [`ChatBody`].
#[derive(Debug, Clone)]
pub struct ChatRow {
    pub timestamp: SharedString,
    pub platform: Platform,
    pub badges: Vec<BadgeKind>,
    pub username: SharedString,
    /// The username's own color (platform- or role-derived by the caller).
    pub username_color: Rgba,
    pub body: ChatBody,
}

/// The left stripe hue and optional row fill for a body kind. Plain messages and
/// commands are unaccented (transparent stripe, no fill); events carry a colored
/// stripe over an `elevated` fill.
fn body_accent(body: &ChatBody, palette: &ForgePalette) -> (Rgba, Option<Rgba>) {
    match body {
        ChatBody::Message(_) | ChatBody::Command { .. } => (with_alpha(palette.brand, 0.0), None),
        ChatBody::Subscription { .. } => (palette.brand, Some(palette.elevated)),
        ChatBody::Cheer { .. } => (palette.warning, Some(palette.elevated)),
        ChatBody::Raid { .. } => (palette.random, Some(palette.elevated)),
    }
}

/// Boxed username-click handler. gpui hands the click event plus the window and
/// app contexts, through which the caller reaches its own entity.
type UsernameClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// A two-line chat row. Line 1 (meta) carries the timestamp, platform tile,
/// badges, and an optional triggered-action pill; line 2 (body) carries the
/// username and message. The username's start-x is fixed regardless of how many
/// badges precede it, because badges live on the meta line above.
#[derive(IntoElement)]
pub struct ChatRowView {
    palette: ForgePalette,
    data: ChatRow,
    click: Option<(ElementId, UsernameClick)>,
}

/// Builds a display-only chat row.
pub fn chat_row(palette: &ForgePalette, data: ChatRow) -> ChatRowView {
    ChatRowView {
        palette: *palette,
        data,
        click: None,
    }
}

impl ChatRowView {
    /// Makes the username pressable. gpui needs a stable [`ElementId`] to promote
    /// it to a stateful clickable element; on hover it gains an underline in its
    /// own color. The handler mutates the caller's entity via the passed `cx`.
    pub fn on_username_click(
        mut self,
        id: impl Into<ElementId>,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.click = Some((id.into(), Box::new(handler)));
        self
    }
}

/// The username element, optionally pressable with a hover underline.
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

/// The `": "` separator between a username and its message.
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

/// A body-text run at `FONT_SM` in the given ink, filling the remaining width and
/// wrapping (continuation lines align under its own start).
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

/// A second-line message, inset under the username of an event row and wrapping
/// within the remaining width.
fn second_line_el(text: SharedString, color: Rgba) -> impl IntoElement {
    div()
        .pl(BODY_INSET)
        .font_family(DEFAULT_BODY_FAMILY)
        .text_size(FONT_SM)
        .line_height(LH_SM)
        .text_color(color)
        .child(text)
}

/// A plain inline descriptor run at `FONT_SM` in `text_secondary`.
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

/// The platform indicator: a rounded square tile filled with the platform hue,
/// carrying a centered semibold letter in the shell color.
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

/// A meta-row badge pill: a solid role-colored fill with a shell-colored caption.
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

/// The right-aligned "triggered action" pill: a translucent success tint with
/// success-colored text.
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

/// A count pill (sub months, cheer bits, raid viewers): a translucent tint of
/// `hue` at `alpha` with `hue`-colored text, `FONT_XS`.
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

/// The monospace command pill: a translucent `surface_overlay` tint with
/// brand-colored mono text.
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

/// The leading glyph plus username cluster shared by the event bodies
/// (subscription / cheer / raid): `[icon] gap [username][descriptor][pill?]`.
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

        // Meta row (line 1): timestamp, platform tile, badges, then the optional
        // right-aligned triggered-action pill.
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

        // Body row (line 2): username + message, shaped per body kind.
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
