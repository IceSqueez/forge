use forge_components::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, FONT_XS, FONT_XXS, ForgePalette, Radius,
    Spacing, icon_inherit, radius, spacing,
};
use gpui::{Context, Entity, FocusHandle, MouseButton, MouseDownEvent, Window, div, prelude::*};

use crate::actions::{
    GoActions, GoChat, GoHome, GoPlatforms, GoSettings, GoTriggers, SHELL_CONTEXT,
};
use crate::presentation::{ActivePresentation, Presentation};
use crate::runtime_status::RuntimeStatus;
use crate::screen::Screen;
use crate::screen_stub::ScreenStub;

/// Root shell view-entity. Holds the router discriminant, the single
/// active-screen child entity, the topic entity it observes, and its own focus
/// handle — four top-level fields. It owns no screen-internal or domain state;
/// the routed screen is a separate view-entity swapped on navigation.
pub struct AppShell {
    screen: Screen,
    content: Entity<ScreenStub>,
    status: Entity<RuntimeStatus>,
    focus: FocusHandle,
}

impl AppShell {
    pub fn new(status: Entity<RuntimeStatus>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let screen = Screen::Home;
        let content = cx.new(|_| ScreenStub::new(screen));
        let focus = cx.focus_handle();

        // Repaint when the observed topic entity advances (bridge → notify) and
        // when the presentation global (theme / density) is replaced.
        cx.observe(&status, |_, _, cx| cx.notify()).detach();
        cx.observe_global::<Presentation>(|_, cx| cx.notify())
            .detach();

        window.focus(&focus);

        Self {
            screen,
            content,
            status,
            focus,
        }
    }

    fn navigate(&mut self, screen: Screen, cx: &mut Context<Self>) {
        if self.screen == screen {
            return;
        }
        self.screen = screen;
        self.content = cx.new(|_| ScreenStub::new(screen));
        cx.notify();
    }

    fn go_home(&mut self, _: &GoHome, _: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Screen::Home, cx);
    }

    fn go_chat(&mut self, _: &GoChat, _: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Screen::Chat, cx);
    }

    fn go_actions(&mut self, _: &GoActions, _: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Screen::Actions, cx);
    }

    fn go_triggers(&mut self, _: &GoTriggers, _: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Screen::Triggers, cx);
    }

    fn go_platforms(&mut self, _: &GoPlatforms, _: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Screen::Platforms, cx);
    }

    fn go_settings(&mut self, _: &GoSettings, _: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Screen::Settings, cx);
    }

    fn nav_item(
        &self,
        target: Screen,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let density = cx.density();
        let active = target == self.screen;
        let (fg, bg) = if active {
            (palette.text_primary, palette.elevated)
        } else {
            (palette.text_muted, palette.base)
        };

        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .py(spacing(Spacing::Xs, density))
            .rounded(radius(Radius::Sm))
            .bg(bg)
            .text_color(fg)
            .child(icon_inherit(target.icon(), FONT_XS))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .child(target.title()),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _: &MouseDownEvent, _, cx| this.navigate(target, cx)),
            )
    }
}

impl Render for AppShell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();
        let uptime = self.status.read(cx).uptime_secs();

        let mut nav_items = Vec::with_capacity(Screen::SEED_ROSTER.len());
        for &target in Screen::SEED_ROSTER.iter() {
            nav_items.push(self.nav_item(target, &palette, cx));
        }

        let nav = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .children(nav_items);

        let uptime_cell = div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(format!("uptime {uptime}s"));

        let top_bar = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Md, density))
            .py(spacing(Spacing::Sm, density))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.shell)
            .child(nav)
            .child(uptime_cell);

        div()
            .key_context(SHELL_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::go_home))
            .on_action(cx.listener(Self::go_chat))
            .on_action(cx.listener(Self::go_actions))
            .on_action(cx.listener(Self::go_triggers))
            .on_action(cx.listener(Self::go_platforms))
            .on_action(cx.listener(Self::go_settings))
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(top_bar)
            .child(div().w_full().flex_1().child(self.content.clone()))
    }
}
