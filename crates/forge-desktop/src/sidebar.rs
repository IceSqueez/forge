use forge_components::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, FONT_XS, FONT_XXS, ForgePalette, Icon,
    Radius, icon, icon_inherit, radius, status_dot,
};
use forge_platform_core::BuiltinId;
use gpui::{
    AnyElement, ClickEvent, Context, EventEmitter, FontWeight, Pixels, Rgba, SharedString, Window,
    div, prelude::*, px,
};

use crate::presentation::{ActivePresentation, Presentation};
use crate::screen::Screen;

// Fixed sidebar chrome geometry. The nav rail is a fixed-width, density-neutral
// chrome region, so — mirroring the kit footer's precedent — its widths, paddings,
// item insets, glyph sizes, and indicator discs are carried as hand-tuned literals
// rather than snapped to the density-scaled `Spacing` steps, which would alter the
// rail's deliberate rhythm.
const SIDEBAR_WIDTH: Pixels = px(210.0);
const SIDEBAR_PAD_H: Pixels = px(8.0);
const SIDEBAR_PAD_TOP: Pixels = px(12.0);
const SIDEBAR_PAD_BOTTOM: Pixels = px(12.0);
const DIVIDER_PAD_TOP: Pixels = px(8.0);

const ITEM_PAD_H: Pixels = px(10.0);
const ITEM_GAP: Pixels = px(10.0);
const SECTION_ITEM_PAD_V: Pixels = px(7.0);
const FLAT_ITEM_PAD_V: Pixels = px(6.0);
const SECTION_ITEM_MB: Pixels = px(2.0);
const FLAT_ITEM_MB: Pixels = px(1.0);

const SECTION_ICON: Pixels = px(15.0);
const FLAT_ICON: Pixels = px(13.0);
const BRAND_DOT: Pixels = px(8.0);
/// The brand indicator is a soft-cornered square (2px), not a disc — matching the
/// design's rounded-square platform/app marks.
const BRAND_DOT_RADIUS: Pixels = px(2.0);
const STATUS_DOT: Pixels = px(5.0);

const SECTION_LABEL_PAD_TOP: Pixels = px(14.0);
const SECTION_LABEL_PAD_BOTTOM: Pixels = px(6.0);
const MINI_LABEL_PAD_TOP: Pixels = px(8.0);
const MINI_LABEL_PAD_BOTTOM: Pixels = px(3.0);

/// Navigation request emitted when a sidebar item is clicked. The root shell
/// subscribes and drives the router; the sidebar itself never mutates the router —
/// it only voices intent, keeping the active screen single-sourced on the root.
pub struct NavRequested(pub Screen);

/// One row in the sidebar roster. The grouping (sections, mini-labels, leaf
/// styles) mirrors the shipping app's nav taxonomy; brand-dot colors resolve from
/// the active palette at render time.
enum NavEntry {
    /// Larger heading over a group (`AUDIENCE`, `AUTOMATION`).
    SectionLabel(&'static str),
    /// Tighter uppercase heading over a flat cluster (`PLATFORMS`, `MODULES`).
    MiniLabel(&'static str),
    /// Same tight uppercase heading, but navigable to an overview screen when
    /// clicked (`STREAM APPS` → the stream-apps overview).
    MiniLabelLink { label: &'static str, screen: Screen },
    /// Primary leaf: 15px glyph that recolors to brand when active.
    SectionLeaf {
        icon: Icon,
        label: &'static str,
        screen: Screen,
    },
    /// Module leaf: 13px glyph, no active recolor.
    FlatIconLeaf {
        icon: Icon,
        label: &'static str,
        screen: Screen,
    },
    /// Platform / stream-app leaf: a brand square, a label, and a live-status dot.
    FlatLink {
        dot: Rgba,
        label: &'static str,
        screen: Screen,
        connected: bool,
    },
}

/// Left navigation rail rendered as its own child view-entity. It owns
/// only the active-screen mirror it highlights against; clicks emit
/// [`NavRequested`] for the shell to route, and the shell pushes the confirmed
/// screen back via [`SidebarNav::set_current`]. Palette is read from the
/// presentation `Global`.
pub struct SidebarNav {
    current: Screen,
}

impl EventEmitter<NavRequested> for SidebarNav {}

impl SidebarNav {
    pub fn new(current: Screen, cx: &mut Context<Self>) -> Self {
        cx.observe_global::<Presentation>(|_, cx| cx.notify())
            .detach();
        Self { current }
    }

    /// Confirmed active screen pushed from the root after it routes a request.
    pub fn set_current(&mut self, screen: Screen) {
        self.current = screen;
    }

    fn request(&mut self, screen: Screen, cx: &mut Context<Self>) {
        cx.emit(NavRequested(screen));
    }

    /// Grouped roster (every row except the bottom-pinned Settings). Brand-dot
    /// colors resolve from `palette`; the connection dots are stubbed disconnected
    /// until the platform-health bridge topic lands.
    fn roster(palette: &ForgePalette) -> Vec<NavEntry> {
        vec![
            NavEntry::SectionLeaf {
                icon: Icon::Home,
                label: "Home",
                screen: Screen::Home,
            },
            NavEntry::SectionLabel("Audience"),
            NavEntry::SectionLeaf {
                icon: Icon::MessageCircle,
                label: "Chat",
                screen: Screen::Chat,
            },
            NavEntry::SectionLabel("Automation"),
            NavEntry::SectionLeaf {
                icon: Icon::Bolt,
                label: "Actions",
                screen: Screen::Actions,
            },
            NavEntry::SectionLeaf {
                icon: Icon::TargetArrow,
                label: "Triggers",
                screen: Screen::Triggers,
            },
            NavEntry::SectionLeaf {
                icon: Icon::Notebook,
                label: "Queues",
                screen: Screen::Queues,
            },
            NavEntry::SectionLeaf {
                icon: Icon::Activity,
                label: "Event feed",
                screen: Screen::EventFeed,
            },
            NavEntry::SectionLeaf {
                icon: Icon::Variable,
                label: "Globals",
                screen: Screen::Globals,
            },
            NavEntry::SectionLeaf {
                icon: Icon::FileCode,
                label: "Scripts",
                screen: Screen::Scripts,
            },
            NavEntry::MiniLabel("Platforms"),
            NavEntry::FlatLink {
                dot: palette.brand,
                label: "Twitch",
                screen: Screen::BuiltinDetail(BuiltinId::new("twitch")),
                connected: false,
            },
            NavEntry::FlatLink {
                dot: palette.random,
                label: "YouTube",
                screen: Screen::BuiltinDetail(BuiltinId::new("youtube")),
                connected: false,
            },
            NavEntry::FlatLink {
                dot: palette.info,
                label: "Kick",
                screen: Screen::BuiltinDetail(BuiltinId::new("kick")),
                connected: false,
            },
            NavEntry::MiniLabelLink {
                label: "Stream apps",
                screen: Screen::StreamApps,
            },
            NavEntry::FlatLink {
                dot: palette.success,
                label: "OBS Studio",
                screen: Screen::BuiltinDetail(BuiltinId::new("obs")),
                connected: false,
            },
            NavEntry::FlatLink {
                dot: palette.warning,
                label: "VTube Studio",
                screen: Screen::BuiltinDetail(BuiltinId::new("vtube")),
                connected: false,
            },
            NavEntry::MiniLabel("Modules"),
            NavEntry::FlatIconLeaf {
                icon: Icon::Volume,
                label: "Text-to-Speech",
                screen: Screen::Tts,
            },
            NavEntry::FlatIconLeaf {
                icon: Icon::Music,
                label: "Soundboard",
                screen: Screen::Soundboard,
            },
            NavEntry::FlatIconLeaf {
                icon: Icon::PlugConnected,
                label: "MIDI",
                screen: Screen::Midi,
            },
            NavEntry::FlatIconLeaf {
                icon: Icon::Keyboard,
                label: "Hotkeys",
                screen: Screen::Hotkeys,
            },
            NavEntry::FlatIconLeaf {
                icon: Icon::Send,
                label: "Discord",
                screen: Screen::Discord,
            },
            NavEntry::FlatIconLeaf {
                icon: Icon::Server,
                label: "WebSocket server",
                screen: Screen::Server,
            },
        ]
    }

    fn text_label(label: &'static str) -> AnyElement {
        div()
            .flex_1()
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(FONT_XS)
            .child(label)
            .into_any_element()
    }

    fn section_label(text: &'static str, palette: &ForgePalette) -> AnyElement {
        div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .pt(SECTION_LABEL_PAD_TOP)
            .pb(SECTION_LABEL_PAD_BOTTOM)
            .px(ITEM_PAD_H)
            .child(SharedString::from(text.to_uppercase()))
            .into_any_element()
    }

    fn mini_label(text: &'static str, palette: &ForgePalette) -> AnyElement {
        div()
            .font_family(DEFAULT_MONO_FAMILY)
            .font_weight(FontWeight::MEDIUM)
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .pt(MINI_LABEL_PAD_TOP)
            .pb(MINI_LABEL_PAD_BOTTOM)
            .px(ITEM_PAD_H)
            .child(SharedString::from(text.to_uppercase()))
            .into_any_element()
    }

    /// Navigable variant of [`Self::mini_label`]: the same tight uppercase caption,
    /// but a click voices a [`NavRequested`] toward `screen` (the section's overview)
    /// and a hover lifts the caption ink to signal the affordance.
    fn mini_label_link(
        &self,
        text: &'static str,
        screen: Screen,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hover_ink = palette.text_muted;
        div()
            .id(text)
            .font_family(DEFAULT_MONO_FAMILY)
            .font_weight(FontWeight::MEDIUM)
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .pt(MINI_LABEL_PAD_TOP)
            .pb(MINI_LABEL_PAD_BOTTOM)
            .px(ITEM_PAD_H)
            .cursor_pointer()
            .hover(move |s| s.text_color(hover_ink))
            .on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| this.request(screen.clone(), cx)),
            )
            .child(SharedString::from(text.to_uppercase()))
            .into_any_element()
    }

    /// Shared interactive leaf frame: rounded row, active fill vs hover fill, and a
    /// click that emits the navigation request. `children` are the already-built
    /// leading glyph/dot, label, and any trailing status dot.
    #[allow(clippy::too_many_arguments)]
    fn nav_frame(
        id: &'static str,
        screen: Screen,
        active: bool,
        pad_v: Pixels,
        mb: Pixels,
        fg: Rgba,
        children: Vec<AnyElement>,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let elevated = palette.elevated;
        let mut row = div()
            .id(id)
            .flex()
            .items_center()
            .gap(ITEM_GAP)
            .px(ITEM_PAD_H)
            .py(pad_v)
            .mb(mb)
            .rounded(radius(Radius::Sm))
            .text_color(fg)
            .cursor_pointer()
            .on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| this.request(screen.clone(), cx)),
            )
            .children(children);
        if active {
            row = row.bg(palette.surface_overlay);
        } else {
            row = row.hover(move |style| style.bg(elevated));
        }
        row.into_any_element()
    }

    /// Builds a primary leaf (Home / automation group / Settings): 15px glyph that
    /// recolors to brand when active.
    fn section_leaf(
        &self,
        ic: Icon,
        label: &'static str,
        screen: Screen,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = self.current == screen;
        let (fg, glyph) = if active {
            (
                palette.text_primary,
                icon(ic, SECTION_ICON, palette.brand).into_any_element(),
            )
        } else {
            (
                palette.text_secondary,
                icon_inherit(ic, SECTION_ICON).into_any_element(),
            )
        };
        Self::nav_frame(
            label,
            screen,
            active,
            SECTION_ITEM_PAD_V,
            SECTION_ITEM_MB,
            fg,
            vec![glyph, Self::text_label(label)],
            palette,
            cx,
        )
    }

    fn render_entry(
        &self,
        entry: NavEntry,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match entry {
            NavEntry::SectionLabel(text) => Self::section_label(text, palette),
            NavEntry::MiniLabel(text) => Self::mini_label(text, palette),
            NavEntry::MiniLabelLink { label, screen } => {
                self.mini_label_link(label, screen, palette, cx)
            }
            NavEntry::SectionLeaf {
                icon: ic,
                label,
                screen,
            } => self.section_leaf(ic, label, screen, palette, cx),
            NavEntry::FlatIconLeaf {
                icon: ic,
                label,
                screen,
            } => {
                let active = self.current == screen;
                let fg = if active {
                    palette.text_primary
                } else {
                    palette.text_secondary
                };
                Self::nav_frame(
                    label,
                    screen,
                    active,
                    FLAT_ITEM_PAD_V,
                    FLAT_ITEM_MB,
                    fg,
                    vec![
                        icon_inherit(ic, FLAT_ICON).into_any_element(),
                        Self::text_label(label),
                    ],
                    palette,
                    cx,
                )
            }
            NavEntry::FlatLink {
                dot,
                label,
                screen,
                connected,
            } => {
                let active = self.current == screen;
                let fg = if active {
                    palette.text_primary
                } else {
                    palette.text_secondary
                };
                let square = div()
                    .flex_none()
                    .size(BRAND_DOT)
                    .rounded(BRAND_DOT_RADIUS)
                    .bg(dot)
                    .into_any_element();
                let status_color = if connected {
                    palette.success
                } else {
                    palette.text_extreme_faint
                };
                Self::nav_frame(
                    label,
                    screen,
                    active,
                    FLAT_ITEM_PAD_V,
                    FLAT_ITEM_MB,
                    fg,
                    vec![
                        square,
                        Self::text_label(label),
                        status_dot(status_color, STATUS_DOT).into_any_element(),
                    ],
                    palette,
                    cx,
                )
            }
        }
    }
}

impl Render for SidebarNav {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();

        let mut items: Vec<AnyElement> = Vec::new();
        for entry in Self::roster(&palette) {
            items.push(self.render_entry(entry, &palette, cx));
        }

        let settings =
            self.section_leaf(Icon::Settings, "Settings", Screen::Settings, &palette, cx);

        div()
            .flex()
            .flex_col()
            .w(SIDEBAR_WIDTH)
            .h_full()
            .flex_none()
            .bg(palette.shell)
            .border_r(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(
                div()
                    .id("sidebar-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .px(SIDEBAR_PAD_H)
                    .pt(SIDEBAR_PAD_TOP)
                    .children(items),
            )
            .child(
                div()
                    .flex_none()
                    .px(SIDEBAR_PAD_H)
                    .pt(DIVIDER_PAD_TOP)
                    .pb(SIDEBAR_PAD_BOTTOM)
                    .border_t(BORDER_THIN)
                    .border_color(palette.border_regular)
                    .child(settings),
            )
    }
}
