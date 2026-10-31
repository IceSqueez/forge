use gpui::{
    App, BoxShadow, InteractiveElement, KeyBinding, NoAction, Pixels, Rgba,
    StatefulInteractiveElement, Styled, point, px,
};

use crate::text_input::CommitFocusedField;
use crate::tokens::{ENTER_KEY, SPACE_KEY};

const KEY_CONTEXT: &str = "ForgeKeyActivated";
const FOCUS_RING_WIDTH: Pixels = px(2.0);
const TAB_ORDER_BY_PAINT: isize = 0;

pub fn bind_key_activation(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new(ENTER_KEY, NoAction, Some(KEY_CONTEXT)),
        KeyBinding::new(SPACE_KEY, NoAction, Some(KEY_CONTEXT)),
    ]);
}

pub fn with_focus_ring<E: InteractiveElement>(element: E, ring: Rgba) -> E {
    element.focus_visible(move |style| {
        style.shadow(vec![BoxShadow {
            color: ring.into(),
            offset: point(px(0.0), px(0.0)),
            blur_radius: px(0.0),
            spread_radius: FOCUS_RING_WIDTH,
            inset: true,
        }])
    })
}

pub trait KeyActivated: StatefulInteractiveElement + Styled {
    fn key_activated(self, ring: Rgba) -> Self {
        let tab_stop = self
            .key_context(KEY_CONTEXT)
            .tab_index(TAB_ORDER_BY_PAINT)
            .on_any_mouse_down(|_, window, cx| {
                window.prevent_default();
                window.dispatch_action(Box::new(CommitFocusedField), cx);
            });
        with_focus_ring(tab_stop, ring)
    }
}

impl<E: StatefulInteractiveElement + Styled> KeyActivated for E {}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use gpui::prelude::FluentBuilder as _;
    use gpui::{
        AppContext, Context, Entity, FocusHandle, Focusable, IntoElement, KeyUpEvent, Keystroke,
        Modifiers, MouseButton, ParentElement, Render, SharedString, TestAppContext,
        VisualTestContext, Window, actions, div,
    };

    use super::*;
    use crate::breadcrumb::{BreadcrumbCrumb, breadcrumb};
    use crate::buttons::primary_button;
    use crate::cards::{pad_tile, row_card};
    use crate::chip::{ChipGlyph, chip};
    use crate::modal::modal;
    use crate::overlay::{focus_inside_modal, overlay};
    use crate::palette::FORGE_DEFAULT;
    use crate::test_support::install_presentation;
    use crate::text_input::{
        FocusNextField, FocusPreviousField, InputEvent, TextInput, bind_text_input_keys,
    };
    use crate::toggle::toggle;

    actions!(key_activation_test, [Activated]);

    const LIST_CONTEXT: &str = "KeyActivationList";

    #[derive(Default)]
    struct Hits {
        button: Cell<usize>,
        chip: Cell<usize>,
        row: Cell<usize>,
        list: Cell<usize>,
        inside: Cell<usize>,
    }

    fn bump(counter: &Cell<usize>) {
        counter.set(counter.get() + 1);
    }

    struct Host {
        focus: FocusHandle,
        hits: Rc<Hits>,
        disabled: bool,
        modal: bool,
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let palette = &FORGE_DEFAULT;
            let (on_button, on_chip) = (self.hits.clone(), self.hits.clone());
            let (on_row, on_list) = (self.hits.clone(), self.hits.clone());
            let on_inside = self.hits.clone();
            div()
                .size_full()
                .track_focus(&self.focus)
                .on_action(|_: &FocusNextField, window, cx| window.focus_next(cx))
                .on_action(|_: &FocusPreviousField, window, cx| window.focus_prev(cx))
                .child(
                    div()
                        .key_context(LIST_CONTEXT)
                        .on_action(move |_: &Activated, _, _| bump(&on_list.list))
                        .w(px(200.0))
                        .child(
                            primary_button("Go", palette)
                                .disabled(self.disabled)
                                .on_click("go", move |_, _, _| bump(&on_button.button)),
                        )
                        .child(
                            chip("All", ChipGlyph::None, false, palette)
                                .on_click("all", move |_, _, _| bump(&on_chip.chip)),
                        )
                        .child(
                            row_card(SharedString::from("Row"), palette)
                                .on_click("row", move |_, _, _| bump(&on_row.row)),
                        ),
                )
                .children(self.modal.then(|| {
                    overlay(
                        primary_button("Inside", palette)
                            .on_click("inside", move |_, _, _| bump(&on_inside.inside)),
                        palette,
                    )
                    .on_dismiss("sheet", |_, _| {})
                }))
        }
    }

    fn mount(
        cx: &mut TestAppContext,
        disabled: bool,
        modal: bool,
    ) -> (Rc<Hits>, Entity<Host>, &mut VisualTestContext) {
        cx.update(|cx| {
            bind_key_activation(cx);
            cx.bind_keys([
                KeyBinding::new("tab", FocusNextField, None),
                KeyBinding::new("shift-tab", FocusPreviousField, None),
                KeyBinding::new(ENTER_KEY, Activated, Some(LIST_CONTEXT)),
            ]);
        });
        install_presentation(cx);
        let hits = Rc::new(Hits::default());
        let seen = hits.clone();
        let (host, vcx) = cx.add_window_view(move |window, cx| {
            let focus = cx.focus_handle();
            window.focus(&focus, cx);
            Host {
                focus,
                hits: seen,
                disabled,
                modal,
            }
        });
        vcx.update(|window, _| window.activate_window());
        vcx.run_until_parked();
        (hits, host, vcx)
    }

    fn press(vcx: &mut VisualTestContext, keys: &str) {
        for key in keys.split_whitespace() {
            vcx.simulate_keystrokes(key);
            vcx.simulate_event(KeyUpEvent {
                keystroke: Keystroke::parse(key).expect("a valid keystroke"),
            });
            vcx.run_until_parked();
        }
    }

    #[gpui::test]
    fn tab_reaches_each_clickable_and_enter_or_space_fires_only_that_one(cx: &mut TestAppContext) {
        let (hits, _, vcx) = mount(cx, false, false);

        press(vcx, "tab enter");
        assert_eq!(hits.button.get(), 1);
        press(vcx, "tab space");
        assert_eq!(hits.chip.get(), 1);
        press(vcx, "tab enter");
        assert_eq!(hits.row.get(), 1);
        press(vcx, "shift-tab shift-tab enter");

        assert_eq!(
            (hits.button.get(), hits.chip.get(), hits.row.get()),
            (2, 1, 1)
        );
    }

    #[gpui::test]
    fn enter_on_a_clickable_inside_a_list_does_not_activate_the_list(cx: &mut TestAppContext) {
        let (hits, _, vcx) = mount(cx, false, false);

        press(vcx, "tab enter");

        assert_eq!((hits.button.get(), hits.list.get()), (1, 0));
    }

    #[gpui::test]
    fn a_disabled_button_is_skipped_by_tab(cx: &mut TestAppContext) {
        let (hits, _, vcx) = mount(cx, true, false);

        press(vcx, "tab enter");

        assert_eq!((hits.button.get(), hits.chip.get()), (0, 1));
    }

    #[gpui::test]
    fn tab_never_leaves_an_open_overlay(cx: &mut TestAppContext) {
        let (hits, _, vcx) = mount(cx, false, true);
        press(vcx, "tab");

        for _ in 0..4 {
            press(vcx, "tab");
        }
        press(vcx, "enter");

        assert_eq!(hits.inside.get(), 1);
        assert_eq!(
            (hits.button.get(), hits.chip.get(), hits.row.get()),
            (0, 0, 0)
        );
    }

    const SURFACE_COUNT: usize = 4;

    struct Surfaces {
        focus: FocusHandle,
        hits: Rc<[Cell<usize>; SURFACE_COUNT]>,
    }

    impl Render for Surfaces {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let palette = &FORGE_DEFAULT;
            let hit = |slot: usize| {
                let hits = self.hits.clone();
                move |_: &gpui::ClickEvent, _: &mut Window, _: &mut App| bump(&hits[slot])
            };
            div()
                .size_full()
                .track_focus(&self.focus)
                .on_action(|_: &FocusNextField, window, cx| window.focus_next(cx))
                .child(pad_tile("pad", "P", "Pad", palette).on_click(hit(0)))
                .child(breadcrumb(
                    vec![BreadcrumbCrumb::link("Parent", "crumb", hit(1))],
                    palette,
                ))
                .child(toggle(false, palette).on_click("switch", hit(2)))
                .child(modal("Title", "Body", palette).on_close("close", hit(3)))
        }
    }

    #[gpui::test]
    fn tab_then_enter_fires_each_kit_surface_in_paint_order(cx: &mut TestAppContext) {
        cx.update(|cx| {
            bind_key_activation(cx);
            cx.bind_keys([KeyBinding::new("tab", FocusNextField, None)]);
        });
        install_presentation(cx);
        let hits: Rc<[Cell<usize>; SURFACE_COUNT]> = Rc::default();
        let seen = hits.clone();
        let (_, vcx) = cx.add_window_view(move |window, cx| {
            let focus = cx.focus_handle();
            window.focus(&focus, cx);
            Surfaces { focus, hits: seen }
        });
        vcx.update(|window, _| window.activate_window());
        vcx.run_until_parked();

        for slot in 0..SURFACE_COUNT {
            press(vcx, "tab enter");

            let fired: Vec<usize> = hits.iter().map(Cell::get).collect();
            let expected: Vec<usize> = (0..SURFACE_COUNT)
                .map(|at| usize::from(at <= slot))
                .collect();
            assert_eq!(fired, expected);
        }
    }

    const STOPS_BEYOND_THE_WALK_LIMIT: usize = 600;

    struct Crowded {
        focus: FocusHandle,
        field: Entity<TextInput>,
        open: bool,
        outside: Rc<Cell<usize>>,
        inside: Rc<Cell<usize>>,
    }

    impl Render for Crowded {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let palette = &FORGE_DEFAULT;
            let inside = self.inside.clone();
            div()
                .size_full()
                .track_focus(&self.focus)
                .on_action(|_: &FocusNextField, window, cx| window.focus_next(cx))
                .children((0..STOPS_BEYOND_THE_WALK_LIMIT).map(|index| {
                    let outside = self.outside.clone();
                    primary_button("Behind", palette)
                        .on_click(("behind", index), move |_, _, _| bump(&outside))
                }))
                .when(self.open, |root| {
                    root.child(
                        overlay(
                            div().child(self.field.clone()).child(
                                primary_button("Inside", palette)
                                    .on_click("inside", move |_, _, _| bump(&inside)),
                            ),
                            palette,
                        )
                        .on_dismiss("crowded-sheet", |_, _| {}),
                    )
                })
        }
    }

    #[gpui::test]
    fn tab_stays_in_an_overlay_over_more_stops_than_the_walk_limit(cx: &mut TestAppContext) {
        cx.update(|cx| {
            bind_key_activation(cx);
            bind_text_input_keys(cx);
            cx.bind_keys([KeyBinding::new("tab", FocusNextField, None)]);
        });
        install_presentation(cx);
        let outside = Rc::new(Cell::new(0));
        let inside = Rc::new(Cell::new(0));
        let (seen_outside, seen_inside) = (outside.clone(), inside.clone());
        let (crowded, vcx) = cx.add_window_view(move |window, cx| {
            let focus = cx.focus_handle();
            window.focus(&focus, cx);
            Crowded {
                focus,
                field: cx.new(|cx| TextInput::new("crowded-field", cx)),
                open: false,
                outside: seen_outside,
                inside: seen_inside,
            }
        });
        vcx.update(|window, _| window.activate_window());
        vcx.run_until_parked();
        vcx.update(|window, cx| {
            let field = crowded.read(cx).field.clone();
            focus_inside_modal(window, cx, |window, cx| {
                field.update(cx, |field, cx| field.focus(window, cx));
            });
            crowded.update(cx, |crowded, cx| {
                crowded.open = true;
                cx.notify();
            });
        });
        vcx.run_until_parked();

        for _ in 0..4 {
            press(vcx, "tab enter");
        }

        assert_eq!(outside.get(), 0);
        assert!(inside.get() > 0);
    }

    actions!(key_activation_test, [StepList]);

    struct Toolbar {
        list: FocusHandle,
        field: Entity<TextInput>,
        sheet_field: Entity<TextInput>,
        opened: bool,
        clicks: usize,
        steps: usize,
    }

    impl Render for Toolbar {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let palette = &FORGE_DEFAULT;
            div()
                .size_full()
                .child(primary_button("New", palette).on_click(
                    "toolbar-new",
                    cx.listener(|this, _, window, cx| {
                        this.clicks += 1;
                        this.opened = true;
                        let sheet_field = this.sheet_field.clone();
                        focus_inside_modal(window, cx, |window, cx| {
                            sheet_field.update(cx, |field, cx| field.focus(window, cx));
                        });
                        cx.notify();
                    }),
                ))
                .child(self.field.clone())
                .child(
                    div()
                        .id("toolbar-list")
                        .key_context(LIST_CONTEXT)
                        .track_focus(&self.list)
                        .on_action(cx.listener(|this, _: &StepList, _, cx| {
                            this.steps += 1;
                            cx.notify();
                        }))
                        .h(px(40.0)),
                )
                .when(self.opened, |root| {
                    let closing = cx.entity();
                    root.child(overlay(self.sheet_field.clone(), palette).on_dismiss(
                        "toolbar-sheet",
                        move |_, cx| {
                            closing.update(cx, |this, cx| {
                                this.opened = false;
                                cx.notify();
                            });
                        },
                    ))
                })
        }
    }

    fn mount_toolbar(cx: &mut TestAppContext) -> (Entity<Toolbar>, &mut VisualTestContext) {
        cx.update(|cx| {
            bind_key_activation(cx);
            bind_text_input_keys(cx);
            cx.bind_keys([KeyBinding::new("down", StepList, Some(LIST_CONTEXT))]);
        });
        install_presentation(cx);
        let (host, vcx) = cx.add_window_view(|window, cx| {
            let list = cx.focus_handle();
            window.focus(&list, cx);
            Toolbar {
                list,
                field: cx.new(|cx| TextInput::new("field", cx)),
                sheet_field: cx.new(|cx| TextInput::new("sheet", cx)),
                opened: false,
                clicks: 0,
                steps: 0,
            }
        });
        vcx.update(|window, _| window.activate_window());
        vcx.run_until_parked();
        (host, vcx)
    }

    fn click_new(vcx: &mut VisualTestContext) {
        let center = vcx
            .debug_bounds("toolbar-new")
            .expect("the button is painted")
            .center();
        vcx.simulate_click(center, Modifiers::none());
        vcx.run_until_parked();
    }

    fn list_focused(host: &Entity<Toolbar>, vcx: &mut VisualTestContext) -> bool {
        let list = host.read_with(vcx, |host, _| host.list.clone());
        vcx.update(|window, _| list.is_focused(window))
    }

    #[gpui::test]
    fn a_click_leaves_focus_on_the_list_and_arrows_still_reach_it(cx: &mut TestAppContext) {
        let (host, vcx) = mount_toolbar(cx);

        click_new(vcx);
        press(vcx, "escape");
        press(vcx, "down");

        assert!(list_focused(&host, vcx));
        assert_eq!(
            host.read_with(vcx, |host, _| (host.clicks, host.opened, host.steps)),
            (1, false, 1)
        );
    }

    #[gpui::test]
    fn a_press_does_not_focus_the_button_even_before_the_click_lands(cx: &mut TestAppContext) {
        let (host, vcx) = mount_toolbar(cx);
        let center = vcx
            .debug_bounds("toolbar-new")
            .expect("the button is painted")
            .center();

        vcx.simulate_mouse_down(center, MouseButton::Left, Modifiers::none());
        vcx.run_until_parked();
        let list_kept_focus = list_focused(&host, vcx);
        vcx.simulate_mouse_up(center, MouseButton::Left, Modifiers::none());

        assert!(list_kept_focus);
    }

    #[gpui::test]
    fn enter_after_a_click_does_not_repeat_it(cx: &mut TestAppContext) {
        let (host, vcx) = mount_toolbar(cx);

        click_new(vcx);
        press(vcx, "escape");
        press(vcx, "enter space");

        assert_eq!(host.read_with(vcx, |host, _| host.clicks), 1);
    }

    #[gpui::test]
    fn a_press_commits_the_focused_field_without_taking_its_focus(cx: &mut TestAppContext) {
        let (host, vcx) = mount_toolbar(cx);
        let field = host.read_with(vcx, |host, _| host.field.clone());
        vcx.update(|window, cx| field.update(cx, |field, cx| field.focus(window, cx)));
        vcx.run_until_parked();
        let commits = Rc::new(Cell::new(0));
        let seen = commits.clone();
        vcx.update(|_, cx| {
            cx.subscribe(&field, move |_, event: &InputEvent, _| {
                if matches!(event, InputEvent::Blurred(_)) {
                    bump(&seen);
                }
            })
            .detach();
        });
        vcx.simulate_input("typed");
        vcx.run_until_parked();

        let center = vcx
            .debug_bounds("toolbar-new")
            .expect("the button is painted")
            .center();
        vcx.simulate_mouse_down(center, MouseButton::Left, Modifiers::none());
        vcx.run_until_parked();

        let field_focused =
            vcx.update(|window, cx| field.read(cx).focus_handle(cx).is_focused(window));
        assert_eq!((commits.get(), field_focused), (1, true));
    }
}
