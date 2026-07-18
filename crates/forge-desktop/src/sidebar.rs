use forge_components::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, FONT_XS, FONT_XXS, ForgePalette, Icon,
    Radius, ResizeEdge, ResizeRange, icon, install_resize, radius, status_dot, tr,
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
const SIDEBAR_MIN: Pixels = px(170.0);
const SIDEBAR_MAX: Pixels = px(320.0);
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

struct SidebarResizeDrag;

#[derive(Clone, Copy)]
enum NavText {
    Key(&'static str),
    Brand(&'static str),
}

impl NavText {
    fn id(self) -> &'static str {
        match self {
            NavText::Key(s) | NavText::Brand(s) => s,
        }
    }

    fn resolve(self) -> SharedString {
        match self {
            NavText::Key(key) => tr!(key).into(),
            NavText::Brand(name) => SharedString::from(name),
        }
    }
}

enum NavEntry {
    SectionLabel(NavText),
    MiniLabel(NavText),
    MiniLabelLink {
        label: NavText,
        screen: Screen,
    },
    SectionLeaf {
        icon: Icon,
        label: NavText,
        screen: Screen,
    },
    FlatIconLeaf {
        icon: Icon,
        label: NavText,
        screen: Screen,
    },
    FlatLink {
        dot: Rgba,
        label: NavText,
        screen: Screen,
        integ: Integration,
    },
}

pub struct SidebarNav {
    current: Screen,
    width: Pixels,
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
            width: SIDEBAR_WIDTH,
            connectivity,
        }
    }

    pub fn set_current(&mut self, screen: Screen) {
        self.current = screen;
    }

    fn set_width(&mut self, width: Pixels, cx: &mut Context<Self>) {
        if self.width != width {
            self.width = width;
            cx.notify();
        }
    }

    fn request(&mut self, screen: Screen, cx: &mut Context<Self>) {
        cx.emit(NavRequested(screen));
    }

    fn roster(palette: &ForgePalette) -> Vec<NavEntry> {
        vec![
            NavEntry::SectionLeaf {
                icon: Icon::Home,
                label: NavText::Key("nav_item_home"),
                screen: Screen::Home,
            },
            NavEntry::SectionLabel(NavText::Key("nav_section_audience")),
            NavEntry::SectionLeaf {
                icon: Icon::MessageCircle,
                label: NavText::Key("nav_item_chat"),
                screen: Screen::Chat,
            },
            NavEntry::SectionLabel(NavText::Key("nav_section_automation")),
            NavEntry::SectionLeaf {
                icon: Icon::Bolt,
                label: NavText::Key("nav_item_actions"),
                screen: Screen::Actions,
            },
            NavEntry::SectionLeaf {
                icon: Icon::TargetArrow,
                label: NavText::Key("nav_item_triggers"),
                screen: Screen::Triggers(None),
            },
            NavEntry::SectionLeaf {
                icon: Icon::Stack2,
                label: NavText::Key("nav_item_queues"),
                screen: Screen::Queues,
            },
            NavEntry::SectionLeaf {
                icon: Icon::Activity,
                label: NavText::Key("nav_item_event_feed"),
                screen: Screen::EventFeed,
            },
            NavEntry::SectionLeaf {
                icon: Icon::Variable,
                label: NavText::Key("nav_item_globals"),
                screen: Screen::Globals,
            },
            NavEntry::SectionLeaf {
                icon: Icon::Code,
                label: NavText::Key("nav_script_editor"),
                screen: Screen::Scripts,
            },
            NavEntry::MiniLabel(NavText::Key("nav_item_platforms")),
            NavEntry::FlatLink {
                dot: palette.brand,
                label: NavText::Brand("Twitch"),
                screen: Screen::BuiltinDetail(BuiltinId::new("twitch")),
                integ: Integration::Twitch,
            },
            NavEntry::FlatLink {
                dot: palette.random,
                label: NavText::Brand("YouTube"),
                screen: Screen::BuiltinDetail(BuiltinId::new("youtube")),
                integ: Integration::YouTube,
            },
            NavEntry::FlatLink {
                dot: palette.info,
                label: NavText::Brand("Kick"),
                screen: Screen::BuiltinDetail(BuiltinId::new("kick")),
                integ: Integration::Kick,
            },
            NavEntry::MiniLabelLink {
                label: NavText::Key("nav_item_stream_apps"),
                screen: Screen::StreamApps,
            },
            NavEntry::FlatLink {
                dot: palette.success,
                label: NavText::Brand("OBS Studio"),
                screen: Screen::BuiltinDetail(BuiltinId::new("obs")),
                integ: Integration::Obs,
            },
            NavEntry::FlatLink {
                dot: palette.warning,
                label: NavText::Brand("VTube Studio"),
                screen: Screen::BuiltinDetail(BuiltinId::new("vtube")),
                integ: Integration::VTube,
            },
            NavEntry::MiniLabel(NavText::Key("nav_section_builtin")),
            NavEntry::FlatIconLeaf {
                icon: Icon::Message2Share,
                label: NavText::Key("nav_item_tts"),
                screen: Screen::Tts,
            },
            NavEntry::FlatIconLeaf {
                icon: Icon::Music,
                label: NavText::Key("nav_item_soundboard"),
                screen: Screen::Soundboard,
            },
            NavEntry::FlatIconLeaf {
                icon: Icon::Piano,
                label: NavText::Brand("MIDI"),
                screen: Screen::BuiltinDetail(BuiltinId::new("midi")),
            },
            NavEntry::FlatIconLeaf {
                icon: Icon::Keyboard,
                label: NavText::Key("nav_item_hotkey"),
                screen: Screen::BuiltinDetail(BuiltinId::new("hotkey")),
            },
            NavEntry::FlatIconLeaf {
                icon: Icon::BrandDiscord,
                label: NavText::Brand("Discord"),
                screen: Screen::BuiltinDetail(BuiltinId::new("discord")),
            },
            NavEntry::FlatIconLeaf {
                icon: Icon::Network,
                label: NavText::Key("nav_item_ws_server"),
                screen: Screen::Server,
            },
        ]
    }

    fn text_label(label: SharedString) -> AnyElement {
        div()
            .flex_1()
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(FONT_XS)
            .child(label)
            .into_any_element()
    }

    fn section_label(text: NavText, palette: &ForgePalette) -> AnyElement {
        div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .pt(SECTION_LABEL_PAD_TOP)
            .pb(SECTION_LABEL_PAD_BOTTOM)
            .px(ITEM_PAD_H)
            .child(SharedString::from(text.resolve().to_uppercase()))
            .into_any_element()
    }

    fn mini_label(text: NavText, palette: &ForgePalette) -> AnyElement {
        div()
            .font_family(DEFAULT_MONO_FAMILY)
            .font_weight(FontWeight::MEDIUM)
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .pt(MINI_LABEL_PAD_TOP)
            .pb(MINI_LABEL_PAD_BOTTOM)
            .px(ITEM_PAD_H)
            .child(SharedString::from(text.resolve().to_uppercase()))
            .into_any_element()
    }

    fn mini_label_link(
        &self,
        text: NavText,
        screen: Screen,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hover_ink = palette.text_muted;
        div()
            .id(text.id())
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
            .child(SharedString::from(text.resolve().to_uppercase()))
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
        label: NavText,
        screen: Screen,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = self.current.same_nav(&screen);
        let (fg, glyph) = if active {
            (
                palette.text_primary,
                icon(ic, SECTION_ICON, palette.brand).into_any_element(),
            )
        } else {
            (
                palette.text_secondary,
                icon(ic, SECTION_ICON, palette.text_secondary).into_any_element(),
            )
        };
        Self::nav_frame(
            label.id(),
            screen,
            active,
            SECTION_ITEM_PAD_V,
            SECTION_ITEM_MB,
            fg,
            vec![glyph, Self::text_label(label.resolve())],
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
                let active = self.current.same_nav(&screen);
                let fg = if active {
                    palette.text_primary
                } else {
                    palette.text_secondary
                };
                Self::nav_frame(
                    label.id(),
                    screen,
                    active,
                    FLAT_ITEM_PAD_V,
                    FLAT_ITEM_MB,
                    fg,
                    vec![
                        icon(ic, FLAT_ICON, fg).into_any_element(),
                        Self::text_label(label.resolve()),
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
                let active = self.current.same_nav(&screen);
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
                    label.id(),
                    screen,
                    active,
                    FLAT_ITEM_PAD_V,
                    FLAT_ITEM_MB,
                    fg,
                    vec![
                        square,
                        Self::text_label(label.resolve()),
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

        let settings = self.section_leaf(
            Icon::Settings,
            NavText::Key("nav_item_settings"),
            Screen::Settings,
            &palette,
            cx,
        );

        let panel = div()
            .flex()
            .flex_col()
            .w(self.width)
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
            );

        install_resize(
            panel,
            SidebarResizeDrag,
            "sidebar-resize",
            ResizeEdge::Right,
            ResizeRange {
                min: SIDEBAR_MIN,
                max: SIDEBAR_MAX,
            },
            &palette,
            cx.listener(|this, width: &Pixels, _, cx| this.set_width(*width, cx)),
        )
    }
}
