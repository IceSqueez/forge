use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, DEFAULT_BODY_FAMILY, Density, FONT_MD, FONT_SM, FONT_XS,
    ForgePalette, Icon, Radius, Spacing, breadcrumb, connection_status_badge, icon, radius,
    spacing,
};
use std::collections::HashMap;

use forge_platform_core::{BuiltinId, ConnectionState};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, FontWeight, Pixels, Subscription,
    Window, div, prelude::*, px,
};

use crate::home_stats::Integration;
use crate::integrations::BuiltinObject;
use crate::presentation::ActivePresentation;
use crate::screen::Screen;
use crate::sidebar::NavRequested;

const BODY_PAD_V: Pixels = px(22.0);
const BODY_PAD_H: Pixels = px(28.0);

const TILE_SIZE: Pixels = px(44.0);
const TILE_RADIUS: Pixels = px(10.0);
const TILE_GLYPH: Pixels = px(22.0);

const CARD_PAD_V: Pixels = px(16.0);
const CARD_PAD_H: Pixels = px(18.0);

const CHEVRON_SIZE: Pixels = px(16.0);

const CHIP_PAD_V: Pixels = px(2.0);
const CHIP_PAD_H: Pixels = px(7.0);

/// Fields: integration, display name, tile initial, description, feature chips.
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

const ROSTER: [Integration; 5] = [
    Integration::Twitch,
    Integration::YouTube,
    Integration::Kick,
    Integration::Obs,
    Integration::VTube,
];

pub struct PlatformConnectivity {
    connections: Vec<(Integration, bool)>,
}

impl Default for PlatformConnectivity {
    fn default() -> Self {
        Self::new()
    }
}

impl PlatformConnectivity {
    pub fn new() -> Self {
        Self {
            connections: ROSTER.iter().map(|integ| (*integ, false)).collect(),
        }
    }

    pub fn is_connected(&self, integ: Integration) -> bool {
        self.connections
            .iter()
            .find(|(i, _)| *i == integ)
            .map(|(_, connected)| *connected)
            .unwrap_or(false)
    }

    /// Returns whether the value actually changed.
    pub fn set_connected(&mut self, integ: Integration, connected: bool) -> bool {
        if let Some(entry) = self.connections.iter_mut().find(|(i, _)| *i == integ)
            && entry.1 != connected
        {
            entry.1 = connected;
            return true;
        }
        false
    }

    pub fn connected_count(&self) -> usize {
        self.connections.iter().filter(|(_, ok)| *ok).count()
    }

    pub fn total_count(&self) -> usize {
        self.connections.len()
    }

    pub fn seed_from_builtins(&mut self, builtins: &HashMap<BuiltinId, BuiltinObject>) {
        for (integ, connected) in self.connections.iter_mut() {
            *connected = builtins
                .get(&integ.builtin_id())
                .map(|obj| obj.status.connection() == ConnectionState::Connected)
                .unwrap_or(false);
        }
    }
}

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

/// gpui 0.2.2 has no CSS-grid primitive; the two-column layout is emulated with flex
/// row pairs, a trailing odd card balanced by an equal-flex spacer.
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
