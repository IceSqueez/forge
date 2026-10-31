use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    AnyElement, App, Bounds, Div, Element, Global, InteractiveElement, Interactivity, IntoElement,
    ParentElement, Pixels, Point, RenderOnce, ScrollHandle, Stateful, StatefulInteractiveElement,
    StyleRefinement, Styled, Window, actions, canvas,
};

use crate::text_input::{FocusNextField, FocusPreviousField};
use crate::tokens::BORDER_THIN;

actions!(forge_focus_reveal, [RevealFocused]);

#[derive(Default)]
struct RevealTarget(Option<Bounds<Pixels>>);

impl Global for RevealTarget {}

fn reveal_target(cx: &App) -> Option<Bounds<Pixels>> {
    cx.try_global::<RevealTarget>().and_then(|target| target.0)
}

fn request_reveal(window: &mut Window, cx: &mut App) {
    window.defer(cx, |window, cx| {
        window.dispatch_action(Box::new(RevealFocused), cx)
    });
}

pub fn reveal_focus_after_tab<E: InteractiveElement>(root: E) -> E {
    root.capture_action(|_: &FocusNextField, window, cx| request_reveal(window, cx))
        .capture_action(|_: &FocusPreviousField, window, cx| request_reveal(window, cx))
        .capture_action(|_: &RevealFocused, _, cx| cx.set_global(RevealTarget(None)))
}

pub fn offset_revealing(
    offset: Point<Pixels>,
    viewport: Bounds<Pixels>,
    target: Bounds<Pixels>,
) -> Point<Pixels> {
    let above = viewport.top() - target.top();
    let below = target.bottom() - viewport.bottom();
    let mut revealed = offset;
    if above > Pixels::ZERO {
        revealed.y += above;
    } else if below > Pixels::ZERO {
        revealed.y -= below.min(-above);
    }
    revealed
}

fn scroll_to_reveal_target(scroll: &ScrollHandle, window: &mut Window, cx: &mut App) {
    cx.propagate();
    let Some(target) = reveal_target(cx) else {
        return;
    };
    let offset = scroll.offset();
    let mut revealed = offset_revealing(offset, scroll.bounds(), target);
    revealed.y = revealed.y.clamp(-scroll.max_offset().y, Pixels::ZERO);
    let moved = revealed - offset;
    if moved.y != Pixels::ZERO {
        scroll.set_offset(revealed);
        cx.set_global(RevealTarget(Some(Bounds::new(
            target.origin + moved,
            target.size,
        ))));
        window.refresh();
    }
}

pub trait ReportsRevealBounds: InteractiveElement + ParentElement + Sized {
    fn reports_reveal_bounds(self) -> Self {
        let painted: Rc<Cell<Option<Bounds<Pixels>>>> = Rc::default();
        let probe = Rc::clone(&painted);
        self.on_action(move |_: &RevealFocused, _, cx| {
            cx.propagate();
            if reveal_target(cx).is_none() {
                cx.set_global(RevealTarget(painted.get()));
            }
        })
        .child(
            canvas(
                move |bounds, _, _| probe.set(Some(bounds.dilate(BORDER_THIN))),
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
    }
}

impl<E: InteractiveElement + ParentElement> ReportsRevealBounds for E {}

pub trait RevealsFocusIn: InteractiveElement + Sized {
    fn reveals_focus_in(self, scroll: &ScrollHandle) -> Self {
        let scroll = scroll.clone();
        self.on_action(move |_: &RevealFocused, window, cx| {
            scroll_to_reveal_target(&scroll, window, cx)
        })
    }
}

impl<E: InteractiveElement> RevealsFocusIn for E {}

#[derive(IntoElement)]
pub struct FocusRevealingScroll {
    scroller: Stateful<Div>,
}

pub trait OverflowRevealingFocus {
    fn overflow_y_revealing_focus(self) -> FocusRevealingScroll;
}

impl OverflowRevealingFocus for Stateful<Div> {
    fn overflow_y_revealing_focus(self) -> FocusRevealingScroll {
        FocusRevealingScroll {
            scroller: self.overflow_y_scroll(),
        }
    }
}

impl RenderOnce for FocusRevealingScroll {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(id) = Element::id(&self.scroller) else {
            return self.scroller.into_any_element();
        };
        let scroll = window
            .use_keyed_state(id, cx, |_, _| ScrollHandle::new())
            .read(cx)
            .clone();
        self.scroller
            .track_scroll(&scroll)
            .reveals_focus_in(&scroll)
            .into_any_element()
    }
}

impl Styled for FocusRevealingScroll {
    fn style(&mut self) -> &mut StyleRefinement {
        self.scroller.style()
    }
}

impl InteractiveElement for FocusRevealingScroll {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.scroller.interactivity()
    }
}

impl StatefulInteractiveElement for FocusRevealingScroll {}

impl ParentElement for FocusRevealingScroll {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.scroller.extend(elements);
    }
}

#[cfg(test)]
mod tests {
    use gpui::{
        AppContext, Context, Entity, Focusable, Modifiers, Render, TestAppContext,
        VisualTestContext, div, point, px, size,
    };

    use super::*;
    use crate::modal::modal;
    use crate::overlay::overlay;
    use crate::palette::FORGE_DEFAULT;
    use crate::test_support::install_presentation;
    use crate::text_input::{TextInput, bind_text_input_keys};

    const SCROLLED: f32 = -50.0;
    const SIDEWAYS: f32 = -7.0;
    const VIEWPORT_TOP: f32 = 100.0;
    const VIEWPORT_HEIGHT: f32 = 200.0;
    const VIEWPORT_WIDTH: f32 = 300.0;
    const ROW_HEIGHT: f32 = 30.0;

    fn viewport() -> Bounds<Pixels> {
        Bounds::new(
            point(px(0.0), px(VIEWPORT_TOP)),
            size(px(VIEWPORT_WIDTH), px(VIEWPORT_HEIGHT)),
        )
    }

    fn revealed_y(top: f32, height: f32) -> Pixels {
        let target = Bounds::new(
            point(px(0.0), px(top)),
            size(px(VIEWPORT_WIDTH), px(height)),
        );
        offset_revealing(point(px(0.0), px(SCROLLED)), viewport(), target).y
    }

    #[test]
    fn a_stepped_row_scrolls_just_far_enough_into_the_viewport() {
        for (top, expected) in [
            (150.0, -50.0),
            (100.0, -50.0),
            (270.0, -50.0),
            (99.0, -49.0),
            (271.0, -51.0),
            (70.0, -20.0),
            (280.0, -60.0),
            (0.0, 50.0),
        ] {
            assert_eq!(revealed_y(top, ROW_HEIGHT), px(expected), "row at {top}");
        }
    }

    #[test]
    fn a_target_taller_than_the_viewport_aligns_to_its_top() {
        for (top, height, expected) in [
            (150.0, 400.0, -100.0),
            (100.0, 400.0, -50.0),
            (101.0, 400.0, -51.0),
            (99.0, 400.0, -49.0),
            (40.0, 400.0, 10.0),
            (100.0, 201.0, -50.0),
            (150.0, 201.0, -100.0),
            (150.0, 200.0, -100.0),
            (100.0, 200.0, -50.0),
        ] {
            assert_eq!(
                revealed_y(top, height),
                px(expected),
                "target {height} tall at {top}"
            );
        }
    }

    #[test]
    fn revealing_leaves_the_horizontal_offset_alone() {
        let out_of_view_sideways = Bounds::new(
            point(px(VIEWPORT_WIDTH * 2.0), px(0.0)),
            size(px(VIEWPORT_WIDTH), px(ROW_HEIGHT)),
        );

        let revealed = offset_revealing(
            point(px(SIDEWAYS), px(SCROLLED)),
            viewport(),
            out_of_view_sideways,
        );

        assert_eq!(revealed.x, px(SIDEWAYS));
    }

    const FORM_ROW: f32 = 60.0;
    const FORM_VIEWPORT: f32 = 100.0;
    const SCROLLER: &str = "reveal-test-scroller";
    const OUTER_SCROLLER: &str = "reveal-test-outer-scroller";
    const FIELDS: [&str; 4] = [
        "reveal-test-field-0",
        "reveal-test-field-1",
        "reveal-test-field-2",
        "reveal-test-field-3",
    ];

    #[derive(Clone, Copy)]
    enum Layout {
        Scroller,
        Nested,
        Modal,
    }

    struct Form {
        fields: Vec<Entity<TextInput>>,
        layout: Layout,
    }

    impl Form {
        fn rows(&self, gap: f32) -> Vec<AnyElement> {
            self.fields
                .iter()
                .zip(FIELDS)
                .map(|(field, selector)| {
                    div()
                        .flex_none()
                        .pb(px(gap))
                        .child(
                            div()
                                .debug_selector(|| selector.to_owned())
                                .child(field.clone()),
                        )
                        .into_any_element()
                })
                .collect()
        }

        fn scroller(&self, id: &'static str) -> FocusRevealingScroll {
            div()
                .id(id)
                .h(px(FORM_VIEWPORT))
                .overflow_y_revealing_focus()
        }
    }

    impl Render for Form {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let content = match self.layout {
                Layout::Scroller => div()
                    .debug_selector(|| SCROLLER.to_owned())
                    .child(self.scroller(SCROLLER).children(self.rows(FORM_ROW))),
                Layout::Nested => {
                    let inner = self.scroller(SCROLLER).children(self.rows(FORM_ROW));
                    div().debug_selector(|| OUTER_SCROLLER.to_owned()).child(
                        self.scroller(OUTER_SCROLLER)
                            .child(div().flex_none().h(px(FORM_VIEWPORT)))
                            .child(div().flex_none().child(inner))
                            .child(div().flex_none().h(px(FORM_VIEWPORT))),
                    )
                }
                Layout::Modal => div().child(
                    overlay(
                        modal(
                            "Form",
                            div().children(self.rows(FORM_ROW * 30.0)),
                            &FORGE_DEFAULT,
                        ),
                        &FORGE_DEFAULT,
                    )
                    .on_dismiss("reveal-test-modal", |_, _| {}),
                ),
            };
            reveal_focus_after_tab(div()).size_full().child(content)
        }
    }

    fn assert_flush(edge: Pixels, viewport_edge: Pixels) {
        assert!(
            (edge - viewport_edge).abs() <= BORDER_THIN,
            "{edge:?} is not flush with {viewport_edge:?}"
        );
    }

    struct Rig<'a> {
        fields: Vec<Entity<TextInput>>,
        vcx: &'a mut VisualTestContext,
    }

    fn mount(cx: &mut TestAppContext, layout: Layout) -> Rig<'_> {
        cx.update(bind_text_input_keys);
        install_presentation(cx);
        let (form, vcx) = cx.add_window_view(move |_, cx| Form {
            fields: FIELDS
                .iter()
                .map(|_| cx.new(|cx| TextInput::new("field", cx)))
                .collect(),
            layout,
        });
        vcx.update(|window, _| window.activate_window());
        vcx.run_until_parked();
        let fields = form.read_with(vcx, |form, _| form.fields.clone());
        let mut rig = Rig { fields, vcx };
        rig.focus_field(0);
        rig
    }

    impl Rig<'_> {
        fn focus_field(&mut self, index: usize) {
            let field = self.fields[index].clone();
            self.vcx
                .update(|window, cx| field.update(cx, |field, cx| field.focus(window, cx)));
            self.vcx.run_until_parked();
        }

        fn press(&mut self, key: &str) {
            self.vcx.simulate_keystrokes(key);
            self.vcx.run_until_parked();
        }

        fn is_focused(&mut self, index: usize) -> bool {
            let field = self.fields[index].clone();
            self.vcx
                .update(|window, cx| field.read(cx).focus_handle(cx).is_focused(window))
        }

        fn bounds(&mut self, selector: &'static str) -> Bounds<Pixels> {
            self.vcx.debug_bounds(selector).expect("the element paints")
        }
    }

    #[gpui::test]
    fn tab_scrolls_a_field_below_the_fold_just_far_enough(cx: &mut TestAppContext) {
        let mut rig = mount(cx, Layout::Scroller);

        rig.press("tab");

        assert!(rig.is_focused(1));
        let viewport = rig.bounds(SCROLLER);
        assert_flush(rig.bounds(FIELDS[1]).bottom(), viewport.bottom());
        assert!(rig.bounds(FIELDS[1]).bottom() <= viewport.bottom());
    }

    #[gpui::test]
    fn each_tab_reveals_the_field_it_landed_on(cx: &mut TestAppContext) {
        let mut rig = mount(cx, Layout::Scroller);

        rig.press("tab");
        rig.press("tab");

        let viewport = rig.bounds(SCROLLER);
        assert_flush(rig.bounds(FIELDS[2]).bottom(), viewport.bottom());
    }

    #[gpui::test]
    fn shift_tab_scrolls_back_up_to_the_previous_field(cx: &mut TestAppContext) {
        let mut rig = mount(cx, Layout::Scroller);
        rig.press("tab");

        rig.press("shift-tab");

        assert!(rig.is_focused(0));
        let viewport = rig.bounds(SCROLLER);
        assert_flush(rig.bounds(FIELDS[0]).top(), viewport.top());
        assert!(rig.bounds(FIELDS[0]).top() >= viewport.top());
    }

    #[gpui::test]
    fn a_click_on_a_half_visible_field_focuses_it_without_scrolling(cx: &mut TestAppContext) {
        let mut rig = mount(cx, Layout::Scroller);
        let before = rig.bounds(FIELDS[1]);
        assert!(before.bottom() > rig.bounds(SCROLLER).bottom());

        let visible_part = before.origin + point(px(4.0), px(4.0));
        rig.vcx.simulate_click(visible_part, Modifiers::none());
        rig.vcx.run_until_parked();

        assert!(rig.is_focused(1));
        assert_eq!(rig.bounds(FIELDS[1]), before);
    }

    #[gpui::test]
    fn programmatic_focus_does_not_scroll(cx: &mut TestAppContext) {
        let mut rig = mount(cx, Layout::Scroller);
        let before = rig.bounds(FIELDS[2]);

        rig.focus_field(2);

        assert_eq!(rig.bounds(FIELDS[2]), before);
    }

    #[gpui::test]
    fn nested_scrollers_each_scroll_only_their_share(cx: &mut TestAppContext) {
        let mut rig = mount(cx, Layout::Nested);

        rig.press("tab");

        let outer = rig.bounds(OUTER_SCROLLER);
        assert_flush(rig.bounds(FIELDS[1]).bottom(), outer.bottom());
    }

    #[gpui::test]
    fn tab_inside_a_modal_body_reveals_the_next_field(cx: &mut TestAppContext) {
        let mut rig = mount(cx, Layout::Modal);
        let window_height = rig.vcx.update(|window, _| window.viewport_size().height);
        assert!(rig.bounds(FIELDS[1]).bottom() > window_height);

        rig.press("tab");

        assert!(rig.is_focused(1));
        assert!(rig.bounds(FIELDS[1]).bottom() <= window_height);
    }
}
