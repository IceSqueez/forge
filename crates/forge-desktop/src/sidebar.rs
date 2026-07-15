use forge_components::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, FONT_XS, FONT_XXS, ForgePalette, Icon,
    Radius, icon, icon_inherit, radius, status_dot,
};
use forge_platform_core::BuiltinId;
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, FontWeight, Pixels, Rgba, SharedString,
    Window, div, prelude::*, px,
};

use crate::home_stats::Integration;
use crate::platforms::PlatformConnectivity;
use crate::presentation::{ActivePresentation, Presentation};
use crate::screen::Screen;

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
const BRAND_DOT_RADIUS: Pixels = px(2.0);
const STATUS_DOT: Pixels = px(5.0);

const SECTION_LABEL_PAD_TOP: Pixels = px(14.0);
const SECTION_LABEL_PAD_BOTTOM: Pixels = px(6.0);
const MINI_LABEL_PAD_TOP: Pixels = px(8.0);
const MINI_LABEL_PAD_BOTTOM: Pixels = px(3.0);

pub struct NavRequested(pub Screen);

enum NavEntry {
    SectionLabel(&'static str),
    MiniLabel(&'static str),
    MiniLabelLink {
        label: &'static str,
        screen: Screen,
    },
    SectionLeaf {
        icon: Icon,
        label: &'static str,
        screen: Screen,
    },
    FlatIconLeaf {
        icon: Icon,
        label: &'static str,
        screen: Screen,
    },
    FlatLink {
        dot: Rgba,
        label: &'static str,
        screen: Screen,
        integ: Integration,
    },
}

pub struct SidebarNav {
    current: Screen,
    connectivity: Entity<PlatformConnectivity>,
}

impl EventEmitter<NavRequested> for SidebarNav {}

impl SidebarNav {
    pub fn new(
        current: Screen,
        connectivity: Entity<PlatformConnectivity>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe_global::<Presentation>(|_, cx| cx.notify())
            .detach();
        cx.observe(&connectivity, |_, _, cx| cx.notify()).detach();
        Self {
            current,
            connectivity,
        }
    }

    pub fn set_current(&mut self, screen: Screen) {
        self.current = screen;
    }

    fn request(&mut self, screen: Screen, cx: &mut Context<Self>) {
        cx.emit(NavRequested(screen));
    }

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
                integ: Integration::Twitch,
            },
            NavEntry::FlatLink {
                dot: palette.random,
                label: "YouTube",
                screen: Screen::BuiltinDetail(BuiltinId::new("youtube")),
                integ: Integration::YouTube,
            },
            NavEntry::FlatLink {
                dot: palette.info,
                label: "Kick",
                screen: Screen::BuiltinDetail(BuiltinId::new("kick")),
                integ: Integration::Kick,
            },
            NavEntry::MiniLabelLink {
                label: "Stream apps",
                screen: Screen::StreamApps,
            },
            NavEntry::FlatLink {
                dot: palette.success,
                label: "OBS Studio",
                screen: Screen::BuiltinDetail(BuiltinId::new("obs")),
                integ: Integration::Obs,
            },
            NavEntry::FlatLink {
                dot: palette.warning,
                label: "VTube Studio",
                screen: Screen::BuiltinDetail(BuiltinId::new("vtube")),
                integ: Integration::VTube,
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
                screen: Screen::BuiltinDetail(BuiltinId::new("midi")),
            },
            NavEntry::FlatIconLeaf {
                icon: Icon::Keyboard,
                label: "Hotkeys",
                screen: Screen::BuiltinDetail(BuiltinId::new("hotkey")),
            },
            NavEntry::FlatIconLeaf {
                icon: Icon::Send,
                label: "Discord",
                screen: Screen::BuiltinDetail(BuiltinId::new("discord")),
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
                integ,
            } => {
                let connected = self.connectivity.read(cx).is_connected(integ);
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
