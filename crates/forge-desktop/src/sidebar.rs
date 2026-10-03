use forge_components::{
    BORDER_THIN, FONT_XS, FONT_XXS, ForgePalette, Icon, Radius, ResizeEdge, ResizeRange,
    body_family, icon, install_resize, mono_family, pulse_dot, radius, status_dot, tr,
};
use forge_platform_core::{ConnectionAffordance, IntegrationCategory};
use gpui::{
    AnyElement, App, ClickEvent, Context, ElementId, Entity, EventEmitter, FontWeight, Pixels,
    Rgba, SharedString, Window, div, prelude::*, px,
};

use crate::integration_catalog::{core_features, declarations, look_of};
use crate::integration_lifecycle::{CardStatus, IntegrationLifecycle};
use crate::platforms::PlatformConnectivity;
use crate::presentation::{ActivePresentation, Presentation};
use crate::screen::Screen;

const SIDEBAR_WIDTH: Pixels = px(210.0);
const SIDEBAR_MIN: Pixels = px(170.0);
pub const SIDEBAR_MAX: Pixels = px(320.0);
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
const INTEGRATIONS_GAP: Pixels = px(8.0);
const SECTION_BADGE: Pixels = px(10.0);

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

#[derive(Clone, Copy)]
struct NavStatus {
    color: Rgba,
    pulse: bool,
}

enum NavEntry {
    SectionLabel(NavText),
    MiniLabel(NavText),
    Gap(Pixels),
    SectionLeaf {
        icon: Icon,
        label: NavText,
        screen: Screen,
        badge: Option<SharedString>,
    },
    FlatIconLeaf {
        icon: Icon,
        label: NavText,
        screen: Screen,
        status: Option<NavStatus>,
    },
    FlatLink {
        dot: Rgba,
        label: NavText,
        screen: Screen,
        status: Option<NavStatus>,
    },
}

pub struct SidebarNav {
    current: Screen,
    width: Pixels,
    connectivity: Entity<PlatformConnectivity>,
    lifecycle: Entity<IntegrationLifecycle>,
}

impl EventEmitter<NavRequested> for SidebarNav {}

impl SidebarNav {
    pub fn new(
        current: Screen,
        connectivity: Entity<PlatformConnectivity>,
        lifecycle: Entity<IntegrationLifecycle>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe_global::<Presentation>(|_, cx| cx.notify())
            .detach();
        cx.observe(&connectivity, |_, _, cx| cx.notify()).detach();
        cx.observe(&lifecycle, |_, _, cx| cx.notify()).detach();
        Self {
            current,
            width: SIDEBAR_WIDTH,
            connectivity,
            lifecycle,
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

    fn roster(&self, palette: &ForgePalette, cx: &App) -> Vec<NavEntry> {
        let mut entries = vec![
            NavEntry::SectionLeaf {
                icon: Icon::Home,
                label: NavText::Key("nav_item_home"),
                screen: Screen::Home,
                badge: None,
            },
            NavEntry::SectionLabel(NavText::Key("nav_section_audience")),
            NavEntry::SectionLeaf {
                icon: Icon::MessageCircle,
                label: NavText::Key("nav_item_chat"),
                screen: Screen::Chat,
                badge: None,
            },
            NavEntry::SectionLabel(NavText::Key("nav_section_automation")),
        ];
        for (glyph, key, screen) in [
            (Icon::Bolt, "nav_item_actions", Screen::Actions(None)),
            (
                Icon::TargetArrow,
                "nav_item_triggers",
                Screen::Triggers(None),
            ),
            (Icon::Stack2, "nav_item_queues", Screen::Queues),
            (Icon::Activity, "nav_item_event_feed", Screen::EventFeed),
            (Icon::Variable, "nav_item_globals", Screen::Globals),
            (Icon::Code, "nav_script_editor", Screen::Scripts),
        ] {
            entries.push(NavEntry::SectionLeaf {
                icon: glyph,
                label: NavText::Key(key),
                screen,
                badge: None,
            });
        }
        entries.extend(self.integration_entries(palette, cx));
        entries
    }

    fn integration_entries(&self, palette: &ForgePalette, cx: &App) -> Vec<NavEntry> {
        let lifecycle = self.lifecycle.read(cx);
        let connectivity = self.connectivity.read(cx);
        let available: Vec<_> = declarations()
            .into_iter()
            .filter(|declaration| lifecycle.is_known(&declaration.id))
            .collect();
        let enabled = lifecycle.on_count(available.iter().map(|declaration| &declaration.id));
        let mut entries = vec![
            NavEntry::Gap(INTEGRATIONS_GAP),
            NavEntry::SectionLeaf {
                icon: Icon::Apps,
                label: NavText::Key("nav_item_integrations"),
                screen: Screen::Integrations(None),
                badge: Some(format!("{enabled}/{}", available.len()).into()),
            },
        ];
        for category in IntegrationCategory::DISPLAY_ORDER {
            let mut items: Vec<NavEntry> = Vec::new();
            for declaration in available
                .iter()
                .filter(|declaration| declaration.category == *category)
                .filter(|declaration| lifecycle.is_on(&declaration.id))
            {
                let state = lifecycle.state_of(&declaration.id);
                let connected = connectivity.is_integration_connected(&declaration.id);
                let status = Some(nav_status(
                    &CardStatus::resolve(&state, declaration.connection, connected),
                    declaration.connection,
                    palette,
                ));
                let look = look_of(&declaration.id, palette);
                let label = NavText::Brand(declaration.brand_name);
                let screen = Screen::BuiltinDetail(declaration.id.clone());
                items.push(match look.letter {
                    Some(_) => NavEntry::FlatLink {
                        dot: look.tint,
                        label,
                        screen,
                        status,
                    },
                    None => NavEntry::FlatIconLeaf {
                        icon: look.glyph,
                        label,
                        screen,
                        status,
                    },
                });
            }
            for feature in core_features()
                .into_iter()
                .filter(|feature| feature.category == *category)
            {
                items.push(NavEntry::FlatIconLeaf {
                    icon: feature.glyph,
                    label: NavText::Key(feature.name_key),
                    screen: feature.screen,
                    status: None,
                });
            }
            if !items.is_empty() {
                entries.push(NavEntry::MiniLabel(NavText::Key(category.label_key())));
                entries.extend(items);
            }
        }
        entries
    }

    fn text_label(label: SharedString) -> AnyElement {
        div()
            .flex_1()
            .font_family(body_family())
            .text_size(FONT_XS)
            .child(label)
            .into_any_element()
    }

    fn section_label(text: NavText, palette: &ForgePalette) -> AnyElement {
        div()
            .font_family(mono_family())
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
            .font_family(mono_family())
            .font_weight(FontWeight::MEDIUM)
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .pt(MINI_LABEL_PAD_TOP)
            .pb(MINI_LABEL_PAD_BOTTOM)
            .px(ITEM_PAD_H)
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
        badge: Option<SharedString>,
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
        let mut children = vec![glyph, Self::text_label(label.resolve())];
        if let Some(badge) = badge {
            children.push(
                div()
                    .flex_none()
                    .font_family(mono_family())
                    .text_size(SECTION_BADGE)
                    .text_color(palette.text_faint)
                    .child(badge)
                    .into_any_element(),
            );
        }
        Self::nav_frame(
            label.id(),
            screen,
            active,
            SECTION_ITEM_PAD_V,
            SECTION_ITEM_MB,
            fg,
            children,
            palette,
            cx,
        )
    }

    fn status_marker(label: NavText, status: NavStatus) -> AnyElement {
        if status.pulse {
            pulse_dot(
                ElementId::Name(format!("nav-pulse-{}", label.id()).into()),
                status.color,
                STATUS_DOT,
            )
            .into_any_element()
        } else {
            status_dot(status.color, STATUS_DOT).into_any_element()
        }
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
            NavEntry::Gap(height) => div().flex_none().h(height).into_any_element(),
            NavEntry::SectionLeaf {
                icon: ic,
                label,
                screen,
                badge,
            } => self.section_leaf(ic, label, screen, badge, palette, cx),
            NavEntry::FlatIconLeaf {
                icon: ic,
                label,
                screen,
                status,
            } => {
                let active = self.current.same_nav(&screen);
                let fg = if active {
                    palette.text_primary
                } else {
                    palette.text_secondary
                };
                let mut children = vec![
                    icon(ic, FLAT_ICON, fg).into_any_element(),
                    Self::text_label(label.resolve()),
                ];
                children.extend(status.map(|status| Self::status_marker(label, status)));
                Self::nav_frame(
                    label.id(),
                    screen,
                    active,
                    FLAT_ITEM_PAD_V,
                    FLAT_ITEM_MB,
                    fg,
                    children,
                    palette,
                    cx,
                )
            }
            NavEntry::FlatLink {
                dot,
                label,
                screen,
                status,
            } => {
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
                let mut children = vec![square, Self::text_label(label.resolve())];
                children.extend(status.map(|status| Self::status_marker(label, status)));
                Self::nav_frame(
                    label.id(),
                    screen,
                    active,
                    FLAT_ITEM_PAD_V,
                    FLAT_ITEM_MB,
                    fg,
                    children,
                    palette,
                    cx,
                )
            }
        }
    }
}

fn nav_status(
    status: &CardStatus,
    connection: ConnectionAffordance,
    palette: &ForgePalette,
) -> NavStatus {
    let (color, pulse) = match status {
        CardStatus::Starting | CardStatus::Stopping => (palette.brand, true),
        CardStatus::Failed(_) => (palette.random, false),
        CardStatus::Active | CardStatus::Connected => (palette.success, false),
        CardStatus::NotConnected | CardStatus::Disabled => match connection {
            ConnectionAffordance::Connectable => (palette.text_extreme_faint, false),
            ConnectionAffordance::Connectionless => (palette.success, false),
        },
    };
    NavStatus { color, pulse }
}

impl Render for SidebarNav {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();

        let mut items: Vec<AnyElement> = Vec::new();
        for entry in self.roster(&palette, cx) {
            items.push(self.render_entry(entry, &palette, cx));
        }

        let settings = self.section_leaf(
            Icon::Settings,
            NavText::Key("nav_item_settings"),
            Screen::Settings(None),
            None,
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

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use forge_components::ThemeId;
    use forge_types::IntegrationId;
    use gpui::TestAppContext;

    use super::*;
    use crate::integration_catalog::declaration_of;
    use crate::integration_supervisor::{LifecycleState, LifecycleStates};

    enum Row {
        Label(&'static str),
        Integration(IntegrationId),
        Other,
    }

    fn entries(cx: &mut TestAppContext, states: LifecycleStates) -> Vec<NavEntry> {
        let nav = cx.update(|cx| {
            let connectivity = cx.new(|_| PlatformConnectivity::new());
            let lifecycle = cx.new(|_| IntegrationLifecycle::new(states));
            cx.new(|cx| SidebarNav::new(Screen::Home, connectivity, lifecycle, cx))
        });
        let palette = ThemeId::ForgeDefault.palette();
        nav.read_with(cx, |nav, cx| nav.integration_entries(&palette, cx))
    }

    fn integration_rows(cx: &mut TestAppContext, states: LifecycleStates) -> Vec<Row> {
        entries(cx, states)
            .into_iter()
            .map(|entry| match entry {
                NavEntry::MiniLabel(text) => Row::Label(text.id()),
                NavEntry::FlatLink {
                    screen: Screen::BuiltinDetail(id),
                    ..
                }
                | NavEntry::FlatIconLeaf {
                    screen: Screen::BuiltinDetail(id),
                    ..
                } => Row::Integration(id),
                _ => Row::Other,
            })
            .collect()
    }

    fn hub_badge(cx: &mut TestAppContext, states: LifecycleStates) -> Option<SharedString> {
        entries(cx, states)
            .into_iter()
            .find_map(|entry| match entry {
                NavEntry::SectionLeaf {
                    screen: Screen::Integrations(None),
                    badge,
                    ..
                } => badge,
                _ => None,
            })
    }

    fn states(entries: &[(&'static str, LifecycleState)]) -> LifecycleStates {
        entries
            .iter()
            .map(|(id, state)| (IntegrationId::from_static(id), state.clone()))
            .collect()
    }

    fn every_integration(state: LifecycleState) -> LifecycleStates {
        crate::integration_catalog::declarations()
            .into_iter()
            .map(|declaration| (declaration.id, state.clone()))
            .collect()
    }

    fn listed(rows: &[Row]) -> Vec<IntegrationId> {
        rows.iter()
            .filter_map(|row| match row {
                Row::Integration(id) => Some(id.clone()),
                _ => None,
            })
            .collect()
    }

    fn labels(rows: &[Row]) -> Vec<&'static str> {
        rows.iter()
            .filter_map(|row| match row {
                Row::Label(key) => Some(*key),
                _ => None,
            })
            .collect()
    }

    #[gpui::test]
    fn only_integrations_the_user_wants_on_are_listed(cx: &mut TestAppContext) {
        let rows = integration_rows(
            cx,
            states(&[
                ("twitch", LifecycleState::Running),
                ("youtube", LifecycleState::Disabled),
                ("kick", LifecycleState::Failed("offline".to_owned())),
                ("obs", LifecycleState::Stopping),
                ("midi", LifecycleState::Starting),
            ]),
        );

        assert_eq!(
            listed(&rows),
            ["twitch", "kick", "midi"].map(IntegrationId::from_static)
        );
    }

    #[gpui::test]
    fn each_listed_integration_sits_under_its_own_category_label(cx: &mut TestAppContext) {
        let rows = integration_rows(cx, every_integration(LifecycleState::Running));

        let mut current_label = None;
        let mut checked = 0;
        for row in &rows {
            match row {
                Row::Label(key) => current_label = Some(*key),
                Row::Integration(id) => {
                    let declaration = declaration_of(id).expect("only declared integrations");
                    assert_eq!(
                        current_label,
                        Some(declaration.category.label_key()),
                        "{id}"
                    );
                    checked += 1;
                }
                Row::Other => {}
            }
        }
        assert_eq!(checked, crate::integration_catalog::declarations().len());
    }

    #[gpui::test]
    fn a_category_keeps_its_label_only_while_it_holds_an_enabled_integration_or_core_feature(
        cx: &mut TestAppContext,
    ) {
        let core_categories: Vec<&'static str> = IntegrationCategory::DISPLAY_ORDER
            .iter()
            .filter(|category| {
                core_features()
                    .iter()
                    .any(|feature| feature.category == **category)
            })
            .map(|category| category.label_key())
            .collect();
        let with_controls: Vec<&'static str> = IntegrationCategory::DISPLAY_ORDER
            .iter()
            .filter(|category| {
                **category == IntegrationCategory::CONTROLS
                    || core_features()
                        .iter()
                        .any(|feature| feature.category == **category)
            })
            .map(|category| category.label_key())
            .collect();

        for (states, expected, case) in [
            (
                every_integration(LifecycleState::Disabled),
                core_categories,
                "nothing enabled",
            ),
            (
                states(&[
                    ("twitch", LifecycleState::Disabled),
                    ("midi", LifecycleState::Running),
                ]),
                with_controls,
                "one controls integration enabled",
            ),
        ] {
            let rows = integration_rows(cx, states);

            assert_eq!(labels(&rows), expected, "{case}");
        }
    }

    #[gpui::test]
    fn the_hub_link_counts_enabled_out_of_installed_integrations(cx: &mut TestAppContext) {
        let badge = hub_badge(
            cx,
            states(&[
                ("twitch", LifecycleState::Running),
                ("kick", LifecycleState::Failed("offline".to_owned())),
                ("obs", LifecycleState::Disabled),
            ]),
        );

        assert_eq!(badge, Some(SharedString::from("2/3")));
    }
}
