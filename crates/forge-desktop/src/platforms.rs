use forge_components::{
    BreadcrumbCrumb, Density, FONT_MD, FONT_SM, FONT_XS, ForgePalette, Radius, Spacing,
    avatar_tile, body_family, connection_status_badge, nav_card, page_frame, radius, spacing, tr,
};

use forge_platform_core::ConnectionState;
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, Pixels, SharedString, Subscription,
    Window, div, prelude::*, px,
};

use crate::home_stats::Integration;
use crate::integrations::BuiltinRegistry;
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

const CHIP_PAD_V: Pixels = px(2.0);
const CHIP_PAD_H: Pixels = px(7.0);

type PlatformRow = (Integration, &'static str, &'static str);

const PLATFORMS: [PlatformRow; 3] = [
    (Integration::Twitch, "Twitch", "T"),
    (Integration::YouTube, "YouTube", "Y"),
    (Integration::Kick, "Kick", "K"),
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

    pub fn seed_from_builtins(&mut self, builtins: &BuiltinRegistry) {
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
        desc: String,
        features: Vec<SharedString>,
        connected: bool,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tile = avatar_tile(letter, integ.dot_color(palette), palette)
            .size(TILE_SIZE)
            .corner(TILE_RADIUS)
            .font(TILE_GLYPH);

        let badge_label = if connected {
            tr!("platforms_status_connected")
        } else {
            tr!("platforms_status_not_connected")
        };
        let title_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(name),
            )
            .child(connection_status_badge(connected, badge_label, palette));

        let desc_el = div()
            .font_family(body_family())
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

        let target = integ.screen();
        div()
            .flex_1()
            .child(
                nav_card(tile, info, palette)
                    .density(density)
                    .padding_xy(CARD_PAD_V, CARD_PAD_H)
                    .on_click(
                        ("platform-card", integ as usize),
                        cx.listener(move |this, _: &ClickEvent, _, cx| this.go(target.clone(), cx)),
                    ),
            )
            .into_any_element()
    }

    fn platform_desc(integ: Integration) -> String {
        match integ {
            Integration::Twitch => tr!("platforms_twitch_desc"),
            Integration::YouTube => tr!("platforms_youtube_desc"),
            Integration::Kick => tr!("platforms_kick_desc"),
            _ => String::new(),
        }
    }

    fn platform_features(integ: Integration) -> Vec<SharedString> {
        match integ {
            Integration::Twitch => vec![
                tr!("platforms_feature_irc_chat").into(),
                SharedString::from("EventSub"),
                tr!("platforms_feature_channel_points").into(),
                tr!("platforms_feature_bits_subs").into(),
            ],
            Integration::YouTube => vec![
                tr!("platforms_feature_live_chat").into(),
                tr!("platforms_feature_super_chat").into(),
                tr!("platforms_feature_memberships").into(),
            ],
            Integration::Kick => vec![
                tr!("platforms_feature_chat").into(),
                tr!("platforms_feature_subs").into(),
                tr!("platforms_feature_channel_events").into(),
            ],
            _ => Vec::new(),
        }
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
            let (integ, name, letter) = *entry;
            cards.push(self.platform_card(
                integ,
                name,
                letter,
                Self::platform_desc(integ),
                Self::platform_features(integ),
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
                    .font_family(body_family())
                    .text_size(FONT_MD)
                    .text_color(palette.text_primary)
                    .child(tr!("platforms_title")),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.text_muted)
                    .child(tr!("platforms_subtitle")),
            );

        let body = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .child(section_header)
            .child(platform_grid(cards, density));

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

        page_frame(
            vec![BreadcrumbCrumb::leaf(tr!("platforms_breadcrumb"))],
            &palette,
        )
        .body(scroll)
    }
}

fn feature_chip(label: impl Into<SharedString>, palette: &ForgePalette) -> impl IntoElement {
    let label = label.into();
    div()
        .py(CHIP_PAD_V)
        .px(CHIP_PAD_H)
        .rounded(radius(Radius::Sm))
        .bg(palette.shell)
        .child(
            div()
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(palette.text_secondary)
                .child(label),
        )
}

/// gpui 0.2.2 has no CSS-grid primitive; the two-column layout is emulated with flex row pairs, a trailing odd card balanced by an equal-flex spacer.
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
