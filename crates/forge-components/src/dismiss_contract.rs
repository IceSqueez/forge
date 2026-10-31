use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    Context, FocusHandle, InteractiveElement, IntoElement, Modifiers, MouseButton, ParentElement,
    Pixels, Render, Styled, TestAppContext, VisualTestContext, Window, div, point, px,
};

use crate::Icon;
use crate::dropdown::{dropdown, dropdown_surface};
use crate::menu::{context_menu, menu_button};
use crate::overlay::{anchored_popover, overlay};
use crate::palette::FORGE_DEFAULT;
use crate::test_support::install_presentation;

const OPENED_NEAR_THE_CORNER: Pixels = px(10.0);
const OUTSIDE_X: Pixels = px(400.0);
const OUTSIDE_Y: Pixels = px(300.0);

#[derive(Clone, Copy, Debug)]
enum Surface {
    Modal,
    Popover,
    Dropdown,
    MenuButton,
    ContextMenu,
}

const SURFACES_WITH_ESCAPE: [Surface; 4] = [
    Surface::Modal,
    Surface::Popover,
    Surface::Dropdown,
    Surface::MenuButton,
];
const EVERY_SURFACE: [Surface; 5] = [
    Surface::Modal,
    Surface::Popover,
    Surface::Dropdown,
    Surface::MenuButton,
    Surface::ContextMenu,
];

struct Host {
    surface: Surface,
    focus: FocusHandle,
    dismissals: Rc<Cell<usize>>,
}

impl Render for Host {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let dismissals = self.dismissals.clone();
        let dismiss = move |_: &mut Window, _: &mut gpui::App| dismissals.set(dismissals.get() + 1);
        let corner = point(OPENED_NEAR_THE_CORNER, OPENED_NEAR_THE_CORNER);
        let surface = match self.surface {
            Surface::Modal => overlay(div().track_focus(&self.focus), &FORGE_DEFAULT)
                .on_dismiss("contract-modal", dismiss)
                .into_any_element(),
            Surface::Popover => anchored_popover(corner, div())
                .on_dismiss(dismiss)
                .dismiss_on_escape(&self.focus)
                .into_any_element(),
            Surface::Dropdown => div()
                .relative()
                .child(
                    dropdown(dropdown_surface(&FORGE_DEFAULT))
                        .on_dismiss(dismiss)
                        .dismiss_on_escape(&self.focus),
                )
                .into_any_element(),
            Surface::MenuButton => menu_button(Icon::Check, true, &FORGE_DEFAULT)
                .on_dismiss(dismiss)
                .dismiss_on_escape(&self.focus)
                .into_any_element(),
            Surface::ContextMenu => context_menu(corner, &FORGE_DEFAULT)
                .on_dismiss(dismiss)
                .into_any_element(),
        };
        div().size_full().child(surface)
    }
}

struct Opened<'a> {
    dismissals: Rc<Cell<usize>>,
    vcx: &'a mut VisualTestContext,
}

fn open(cx: &mut TestAppContext, surface: Surface) -> Opened<'_> {
    install_presentation(cx);
    let dismissals = Rc::new(Cell::new(0));
    let seen = dismissals.clone();
    let (host, vcx) = cx.add_window_view(move |_window, cx| Host {
        surface,
        focus: cx.focus_handle(),
        dismissals: seen,
    });
    vcx.run_until_parked();
    vcx.update(|window, cx| window.focus(&host.read(cx).focus.clone(), cx));
    vcx.run_until_parked();
    Opened { dismissals, vcx }
}

#[gpui::test]
fn an_outside_click_dismisses_every_surface_exactly_once(cx: &mut TestAppContext) {
    for surface in EVERY_SURFACE {
        let opened = open(cx, surface);

        opened
            .vcx
            .simulate_click(point(OUTSIDE_X, OUTSIDE_Y), Modifiers::none());

        assert_eq!(opened.dismissals.get(), 1, "{surface:?}");
    }
}

#[gpui::test]
fn escape_dismisses_every_surface_that_takes_focus_exactly_once(cx: &mut TestAppContext) {
    for surface in SURFACES_WITH_ESCAPE {
        let opened = open(cx, surface);

        opened.vcx.simulate_keystrokes("escape");

        assert_eq!(opened.dismissals.get(), 1, "{surface:?}");
    }
}

#[gpui::test]
fn other_keys_leave_every_surface_open(cx: &mut TestAppContext) {
    for surface in EVERY_SURFACE {
        let opened = open(cx, surface);

        opened.vcx.simulate_keystrokes("a enter down");

        assert_eq!(opened.dismissals.get(), 0, "{surface:?}");
    }
}

#[gpui::test]
fn a_right_click_outside_dismisses_only_the_context_menu(cx: &mut TestAppContext) {
    for (surface, dismissals) in [
        (Surface::ContextMenu, 1),
        (Surface::Popover, 0),
        (Surface::Dropdown, 0),
        (Surface::MenuButton, 0),
    ] {
        let opened = open(cx, surface);
        let outside = point(OUTSIDE_X, OUTSIDE_Y);

        opened
            .vcx
            .simulate_mouse_down(outside, MouseButton::Right, Modifiers::none());
        opened
            .vcx
            .simulate_mouse_up(outside, MouseButton::Right, Modifiers::none());
        opened.vcx.run_until_parked();

        assert_eq!(opened.dismissals.get(), dismissals, "{surface:?}");
    }
}
