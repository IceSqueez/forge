use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    AnyElement, App, Bounds, Div, Element, Global, InteractiveElement, Interactivity, IntoElement,
    ParentElement, Pixels, Point, RenderOnce, ScrollHandle, Stateful, StatefulInteractiveElement,
    StyleRefinement, Styled, Window, actions, canvas,
};

use crate::text_input::{FocusNextField, FocusPreviousField};

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
            canvas(move |bounds, _, _| probe.set(Some(bounds)), |_, _, _, _| {})
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
