use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, DEFAULT_BODY_FAMILY, Density, FONT_MD, FONT_SM, FONT_XS,
    ForgePalette, Icon, Radius, Spacing, breadcrumb, connection_status_badge, icon, radius,
    spacing,
};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, FontWeight, Pixels, Subscription,
    Window, div, prelude::*, px,
};

use crate::home_stats::Integration;
use crate::presentation::ActivePresentation;
use crate::screen::Screen;
use crate::sidebar::NavRequested;

// Off-scale layout literals. The overview grid is a fixed-rhythm dashboard whose
// paddings, tile size and glyph sizes are hand-tuned to the design, so — mirroring
// the Home hub precedent — they are carried as literals rather than snapped to the
// nearest density-scaled `Spacing` step, which would alter the composition.
const BODY_PAD_V: Pixels = px(22.0);
const BODY_PAD_H: Pixels = px(28.0);

/// Brand-filled identity tile geometry: a 44px rounded square holding the platform
/// initial. The corner (~44 * 0.23) and glyph (44 * 0.5) mirror the shared tile
/// ratio so the overview card and a later detail header would share one shape.
const TILE_SIZE: Pixels = px(44.0);
const TILE_RADIUS: Pixels = px(10.0);
const TILE_GLYPH: Pixels = px(22.0);

/// Card inner padding (the design's `16px 18px`).
const CARD_PAD_V: Pixels = px(16.0);
const CARD_PAD_H: Pixels = px(18.0);

/// Trailing chevron glyph size.
const CHEVRON_SIZE: Pixels = px(16.0);

/// Feature-pill inset (the design's `2px 7px`), off the `Spacing` scale as a fixed
/// caption-chip metric.
const CHIP_PAD_V: Pixels = px(2.0);
const CHIP_PAD_H: Pixels = px(7.0);

/// The three streaming platforms surfaced on the overview: the integration key
/// (brand hue + router destination), display name, tile initial, one-line
/// description and the feature-chip row. Mirrors the design's platform roster.
type PlatformRow = (
    Integration,
    &'static str,
    &'static str,
    &'static str,
    &'static [&'static str],
);

const PLATFORMS: [PlatformRow; 3] = [
    (
        Integration::Twitch,
        "Twitch",
        "T",
        "Chat, EventSub subscriptions, channel points, bits, raids",
        &["IRC chat", "EventSub", "Channel points", "Bits & subs"],
    ),
    (
        Integration::YouTube,
        "YouTube",
        "Y",
        "Live chat, super chats, channel memberships, subscribers",
        &["Live chat", "Super chat", "Memberships"],
    ),
    (
        Integration::Kick,
        "Kick",
        "K",
        "Chat, channel events, subscribers — newer streaming platform",
        &["Chat", "Subs", "Channel events"],
    ),
];

/// Topic-scoped observable cache backing the Platforms and Stream Apps overviews:
/// the connected state of each integration, keyed by [`Integration`] so a single
/// topic serves both the chat platforms and the stream apps. It holds a cached read,
/// never runtime state of its own; the runtime→UI bridge advances it and
/// `cx.notify()`s so the observing overview views repaint.
///
/// Seeded at boot with a representative sample (Twitch connected, the rest not) so
/// the screens render visibly before a connectivity bridge exists; the bridge
/// replaces each entry as the connection stream lands.
pub struct PlatformConnectivity {
    connections: Vec<(Integration, bool)>,
}

impl PlatformConnectivity {
    pub fn seeded() -> Self {
        Self {
            connections: vec![
                (Integration::Twitch, true),
                (Integration::YouTube, false),
                (Integration::Kick, false),
                (Integration::Obs, false),
                (Integration::VTube, false),
            ],
        }
    }

    /// Whether `integ` is currently connected; unknown integrations read as
    /// disconnected. Kept free of `cx` so it stays directly exercisable.
    pub fn is_connected(&self, integ: Integration) -> bool {
        self.connections
            .iter()
            .find(|(i, _)| *i == integ)
            .map(|(_, connected)| *connected)
            .unwrap_or(false)
    }
}

/// The Platforms overview screen view-entity: a breadcrumb header over a scrollable
/// "Streaming platforms" section — a two-column grid of interactive platform cards
/// (brand-tile initial, name + live connection badge, description, feature chips).
///
/// It owns no connectivity data: each card's connected state is read from an
/// injected [`PlatformConnectivity`] topic (a cached runtime read, never the source
/// of truth) and the view repaints when that topic notifies. Pressing a card voices
/// a [`NavRequested`] toward that platform's screen, which the root shell routes —
/// the screen never mutates the router itself.
pub struct PlatformsView {
    connectivity: Entity<PlatformConnectivity>,
    _conn_obs: Subscription,
}

impl PlatformsView {
    pub fn new(connectivity: Entity<PlatformConnectivity>, cx: &mut Context<Self>) -> Self {
        let conn_obs = cx.observe(&connectivity, |_, _, cx| cx.notify());
        Self {
            connectivity,
            _conn_obs: conn_obs,
        }
    }

    /// Voices a navigation intent to the root shell. No `self` state changes, so no
    /// `cx.notify()` — the shell owns the routing side effect.
    fn go(&mut self, screen: Screen, cx: &mut Context<Self>) {
        cx.emit(NavRequested(screen));
    }

    #[allow(clippy::too_many_arguments)]
    fn platform_card(
        &self,
        integ: Integration,
        name: &'static str,
        letter: &'static str,
        desc: &'static str,
        features: &'static [&'static str],
        connected: bool,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tile = div()
            .flex_none()
            .size(TILE_SIZE)
            .flex()
            .items_center()
            .justify_center()
            .rounded(TILE_RADIUS)
            .bg(integ.dot_color(palette))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(TILE_GLYPH)
                    .text_color(palette.shell)
                    .child(letter),
            );

        let badge_label = if connected {
            "Connected"
        } else {
            "Not connected"
        };
        let title_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(name),
            )
            .child(connection_status_badge(connected, badge_label, palette));

        let desc_el = div()
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(FONT_SM)
            .text_color(palette.text_muted)
            .child(desc);

        let mut info = div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(title_row)
            .child(desc_el);
        if !features.is_empty() {
            let mut chip_row = div().flex().flex_wrap().gap(spacing(Spacing::Xxs, density));
            for feature in features {
                chip_row = chip_row.child(feature_chip(feature, palette));
            }
            info = info.child(chip_row);
        }

        let hover_border = palette.border_input;
        let target = integ.screen();
        div()
            .id(("platform-card", integ as usize))
            .flex_1()
            .flex()
            .flex_row()
            .items_start()
            .gap(spacing(Spacing::Sm, density))
            .py(CARD_PAD_V)
            .px(CARD_PAD_H)
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.elevated)
            .cursor_pointer()
            .hover(move |s| s.border_color(hover_border))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.go(target.clone(), cx)))
            .child(tile)
            .child(info)
            .child(icon(Icon::ChevronRight, CHEVRON_SIZE, palette.text_faint))
            .into_any_element()
    }
}

impl EventEmitter<NavRequested> for PlatformsView {}

impl Render for PlatformsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        // Snapshot the connected flags up front, ending the immutable borrow on the
        // topic before the per-card `cx.listener` closures are built.
        let connectivity = self.connectivity.read(cx);
        let connected: Vec<bool> = PLATFORMS
            .iter()
            .map(|(integ, ..)| connectivity.is_connected(*integ))
            .collect();

        let mut cards: Vec<AnyElement> = Vec::with_capacity(PLATFORMS.len());
        for (idx, entry) in PLATFORMS.iter().enumerate() {
            let (integ, name, letter, desc, features) = *entry;
            cards.push(self.platform_card(
                integ,
                name,
                letter,
                desc,
                features,
                connected[idx],
                &palette,
                density,
                cx,
            ));
        }

        let section_header = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_MD)
                    .text_color(palette.text_primary)
                    .child("Streaming platforms"),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_muted)
                    .child("Connect once, Forge listens to all chats and events in one place."),
            );

        let body = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .child(section_header)
            .child(platform_grid(cards, density));

        let header = breadcrumb(vec![BreadcrumbCrumb::leaf("Platforms")], &palette);

        let scroll = div()
            .id("platforms-scroll")
            .flex_1()
            .overflow_y_scroll()
            .bg(palette.base)
            .child(
                div()
                    .w_full()
                    .pt(BODY_PAD_V)
                    .pb(BODY_PAD_V)
                    .px(BODY_PAD_H)
                    .child(body),
            );

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(header)
            .child(scroll)
    }
}

/// A single feature pill: a small `shell`-filled, `Radius::Sm` rounded chip inking
/// `text_secondary` at `FONT_XS`. Distinct from the kit's filter [`chip`] (a
/// pill-radius, dot-leading pressable), so it is carried locally as a one-off view
/// fragment — matching the design's feature tags exactly.
fn feature_chip(label: &'static str, palette: &ForgePalette) -> impl IntoElement {
    div()
        .py(CHIP_PAD_V)
        .px(CHIP_PAD_H)
        .rounded(radius(Radius::Sm))
        .bg(palette.shell)
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_secondary)
                .child(label),
        )
}

/// Lays the platform cards into a two-column grid: each card fills half its row, and
/// a trailing odd card is balanced by an equal-flex spacer so it keeps its
/// half-width (mirroring the design's `repeat(2, 1fr)` grid, which gpui 0.2.2 has no
/// native equivalent for — a flex row pair is the port).
fn platform_grid(cards: Vec<AnyElement>, density: Density) -> impl IntoElement {
    let gap = spacing(Spacing::Sm, density);
    let mut grid = div().w_full().flex().flex_col().gap(gap);
    let mut iter = cards.into_iter();
    while let Some(first) = iter.next() {
        let mut row = div().w_full().flex().flex_row().gap(gap).child(first);
        match iter.next() {
            Some(second) => row = row.child(second),
            None => row = row.child(div().flex_1()),
        }
        grid = grid.child(row);
    }
    grid
}
