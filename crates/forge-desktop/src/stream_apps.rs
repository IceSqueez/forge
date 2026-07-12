use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, DEFAULT_BODY_FAMILY, Density, FONT_MD, FONT_SM, ForgePalette,
    Icon, Radius, Spacing, breadcrumb, connection_status_badge, icon, radius, spacing,
};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, Pixels, Subscription, Window, div,
    prelude::*, px,
};

use crate::home_stats::Integration;
use crate::platforms::PlatformConnectivity;
use crate::presentation::ActivePresentation;
use crate::screen::Screen;
use crate::sidebar::NavRequested;

// Off-scale layout literals, carried as fixed-rhythm dashboard metrics exactly as
// the Platforms overview does: paddings, tile size and glyph sizes are hand-tuned to
// the design rather than snapped to the nearest density-scaled `Spacing` step.
const BODY_PAD_V: Pixels = px(22.0);
const BODY_PAD_H: Pixels = px(28.0);

/// Glyph identity tile geometry: a 44px rounded square holding the app's line icon
/// over a soft `surface_overlay` fill (a muted plate, unlike the brand-filled
/// platform tile), with the glyph tinted the app's brand hue.
const TILE_SIZE: Pixels = px(44.0);
const TILE_RADIUS: Pixels = px(10.0);
const TILE_GLYPH: Pixels = px(22.0);

/// Card inner padding (the design's `16px 18px`).
const CARD_PAD_V: Pixels = px(16.0);
const CARD_PAD_H: Pixels = px(18.0);

/// Trailing chevron glyph size.
const CHEVRON_SIZE: Pixels = px(16.0);

/// The two stream apps surfaced on the overview: the integration key (brand hue +
/// router destination), display name, tile glyph and one-line description. Mirrors
/// the design's stream-app roster.
type AppRow = (Integration, &'static str, Icon, &'static str);

const APPS: [AppRow; 2] = [
    (
        Integration::Obs,
        "OBS Studio",
        Icon::Broadcast,
        "Scenes, sources, recording control, replay buffers — full obs-websocket API",
    ),
    (
        Integration::VTube,
        "VTube Studio",
        Icon::MoodSmile,
        "Vtuber avatar control: hotkeys, expressions, item triggers",
    ),
];

/// The Stream Apps overview screen view-entity: a breadcrumb header over a scrollable
/// "Stream apps" section — a two-column grid of interactive app cards (glyph tile,
/// name + live connection badge, description).
///
/// It owns no connectivity data: each card's connected state is read from the shared
/// [`PlatformConnectivity`] topic (a cached runtime read, never the source of truth,
/// keyed by [`Integration`] so it carries the stream apps alongside the chat
/// platforms) and the view repaints when that topic notifies. Pressing a card voices
/// a [`NavRequested`] toward that app's integration detail, which the root shell
/// routes — the screen never mutates the router itself.
pub struct StreamAppsView {
    connectivity: Entity<PlatformConnectivity>,
    _conn_obs: Subscription,
}

impl StreamAppsView {
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
    fn app_card(
        &self,
        integ: Integration,
        name: &'static str,
        glyph: Icon,
        desc: &'static str,
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
            .bg(palette.surface_overlay)
            .child(icon(glyph, TILE_GLYPH, integ.dot_color(palette)));

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

        let info = div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(title_row)
            .child(desc_el);

        let hover_border = palette.border_input;
        let target = integ.screen();
        div()
            .id(("stream-app-card", integ as usize))
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

impl EventEmitter<NavRequested> for StreamAppsView {}

impl Render for StreamAppsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        // Snapshot the connected flags up front, ending the immutable borrow on the
        // topic before the per-card `cx.listener` closures are built.
        let connectivity = self.connectivity.read(cx);
        let connected: Vec<bool> = APPS
            .iter()
            .map(|(integ, ..)| connectivity.is_connected(*integ))
            .collect();

        let mut cards: Vec<AnyElement> = Vec::with_capacity(APPS.len());
        for (idx, entry) in APPS.iter().enumerate() {
            let (integ, name, glyph, desc) = *entry;
            cards.push(self.app_card(
                integ,
                name,
                glyph,
                desc,
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
                    .child("Stream apps"),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_muted)
                    .child("Local apps Forge talks to over WebSocket. Connect to control them from actions."),
            );

        let body = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .child(section_header)
            .child(app_grid(cards, density));

        let header = breadcrumb(vec![BreadcrumbCrumb::leaf("Stream apps")], &palette);

        let scroll = div()
            .id("stream-apps-scroll")
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

/// Lays the app cards into a two-column grid: each card fills half its row, and a
/// trailing odd card is balanced by an equal-flex spacer so it keeps its half-width
/// (mirroring the design's `repeat(2, 1fr)` grid, which gpui 0.2.2 has no native
/// equivalent for — a flex row pair is the port).
fn app_grid(cards: Vec<AnyElement>, density: Density) -> impl IntoElement {
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
