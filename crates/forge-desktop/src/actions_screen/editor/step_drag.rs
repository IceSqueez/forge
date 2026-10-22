use super::*;
use forge_components::{
    BORDER_THIN, DRAG_SOURCE_OPACITY, Density, ESCAPE_KEY, FONT_XS, ForgePalette, Icon, Radius,
    Spacing, body_family, icon, radius, spacing,
};
use gpui::{
    AnyElement, Bounds, ClickEvent, Context, CursorStyle, DragMoveEvent, FocusHandle, FontWeight,
    Half, KeyDownEvent, Pixels, Rgba, SharedString, Window, div,
};

pub(super) struct StepDrag {
    action_id: ActionId,
    path: Vec<nav::NavFrame>,
    from: usize,
}

pub(in crate::actions_screen) struct StepDragSession {
    from: usize,
    slot: Option<usize>,
    restore_focus: Option<FocusHandle>,
}

#[derive(Clone)]
struct StepDragGhost {
    title: SharedString,
    glyph: Icon,
    glyph_color: Rgba,
    text: Rgba,
    surface: Rgba,
    border: Rgba,
}

impl Render for StepDragGhost {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .py(STEP_CARD_PAD_V)
            .px(STEP_CARD_PAD_H)
            .bg(self.surface)
            .border(BORDER_THIN)
            .border_color(self.border)
            .rounded(radius(Radius::Md))
            .child(icon(self.glyph, CARD_GLYPH, self.glyph_color))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(FONT_XS)
                    .text_color(self.text)
                    .child(self.title.clone()),
            )
    }
}

pub(super) fn drop_slot_for_pointer(index: usize, pointer_y: Pixels, row: Bounds<Pixels>) -> usize {
    if pointer_y < row.center().y {
        index
    } else {
        index + 1
    }
}

pub(super) fn moved_step_index(from: usize, slot: usize, len: usize) -> Option<usize> {
    if from >= len || slot > len {
        return None;
    }
    let to = if slot > from { slot - 1 } else { slot };
    (to != from).then_some(to)
}

impl ScreenActionsView {
    fn step_drag_blocked(&self) -> bool {
        self.has_open_modal() || self.step_menu_open.is_some()
    }

    fn accepts_step_drag(&self, drag: &StepDrag) -> bool {
        let Some(detail) = self.detail.as_ref() else {
            return false;
        };
        detail.action.id == drag.action_id
            && self.nav_path == drag.path
            && drag.from < nav::resolve_chain(&detail.action.sub_actions, &self.nav_path).len()
            && !self.step_drag_blocked()
    }

    pub(in crate::actions_screen) fn step_drop_indicator_slot(&self) -> Option<usize> {
        let session = self.step_drag.as_ref()?;
        let slot = session.slot?;
        let len = self.current_chain().len();
        moved_step_index(session.from, slot, len).map(|_| slot)
    }

    pub(in crate::actions_screen) fn is_step_dragged(&self, i: usize) -> bool {
        self.step_drag
            .as_ref()
            .is_some_and(|session| session.from == i)
    }

    fn begin_step_drag(&mut self, from: usize, window: &mut Window, cx: &mut Context<Self>) {
        let restore_focus = window.focused(cx);
        window.focus(&self.step_drag_focus, cx);
        self.step_drag = Some(StepDragSession {
            from,
            slot: None,
            restore_focus,
        });
        cx.notify();
    }

    fn set_step_drop_slot(&mut self, slot: Option<usize>, cx: &mut Context<Self>) {
        let Some(session) = self.step_drag.as_mut() else {
            return;
        };
        if session.slot != slot {
            session.slot = slot;
            cx.notify();
        }
    }

    fn hover_step_row(
        &mut self,
        i: usize,
        event: &DragMoveEvent<StepDrag>,
        cx: &mut Context<Self>,
    ) {
        if !event.bounds.contains(&event.event.position) {
            return;
        }
        let accepted = self.accepts_step_drag(event.drag(cx));
        let slot = accepted.then(|| drop_slot_for_pointer(i, event.event.position.y, event.bounds));
        self.set_step_drop_slot(slot, cx);
    }

    fn hover_step_list(&mut self, event: &DragMoveEvent<StepDrag>, cx: &mut Context<Self>) {
        if !event.bounds.contains(&event.event.position) {
            self.set_step_drop_slot(None, cx);
        }
    }

    fn finish_step_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.step_drag.take()
            && self.step_drag_focus.is_focused(window)
        {
            match session.restore_focus {
                Some(focus) => window.focus(&focus, cx),
                None => window.blur(cx),
            }
        }
        cx.notify();
    }

    fn drop_step(&mut self, drag: &StepDrag, window: &mut Window, cx: &mut Context<Self>) {
        let accepted = self.accepts_step_drag(drag);
        let slot = self.step_drag.as_ref().and_then(|session| session.slot);
        let from = drag.from;
        self.finish_step_drag(window, cx);
        let Some(slot) = slot.filter(|_| accepted) else {
            return;
        };
        let Some(to) = moved_step_index(from, slot, self.current_chain().len()) else {
            return;
        };
        self.persist_chain_mutation(
            move |chain| {
                if from < chain.len() && to < chain.len() {
                    let step = chain.remove(from);
                    chain.insert(to, step);
                }
            },
            cx,
        );
        cx.notify();
    }

    fn cancel_step_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.step_drag.is_none() {
            return;
        }
        cx.stop_active_drag(window);
        self.finish_step_drag(window, cx);
    }

    pub(in crate::actions_screen) fn release_step_drag(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.step_drag.is_some() && !cx.has_active_drag() {
            self.finish_step_drag(window, cx);
        }
    }

    pub(super) fn render_step_grip(
        &self,
        i: usize,
        title: SharedString,
        glyph: Icon,
        glyph_color: Rgba,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let grip = div()
            .id(SharedString::from(format!("actions-step-grip-{i}")))
            .flex()
            .items_center()
            .child(icon(Icon::GripVertical, CARD_GLYPH, palette.text_faint));
        let Some(detail) = self.detail.as_ref() else {
            return grip.into_any_element();
        };
        if self.step_drag_blocked() {
            return grip.into_any_element();
        }
        let payload = StepDrag {
            action_id: detail.action.id,
            path: self.nav_path.clone(),
            from: i,
        };
        let ghost = StepDragGhost {
            title,
            glyph,
            glyph_color,
            text: palette.text_primary,
            surface: palette.elevated,
            border: palette.border_active,
        };
        let view = cx.entity().downgrade();
        grip.cursor(CursorStyle::OpenHand)
            .on_click(|_: &ClickEvent, _, cx| cx.stop_propagation())
            .on_drag(payload, move |drag, _offset, window, cx| {
                let from = drag.from;
                view.update(cx, |this, cx| this.begin_step_drag(from, window, cx))
                    .ok();
                cx.new(|_| ghost.clone())
            })
            .into_any_element()
    }

    pub(super) fn wire_step_row_drop(
        &self,
        row: gpui::Div,
        i: usize,
        total: usize,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let indicator = self.step_drop_indicator_slot().and_then(|slot| {
            let line = div()
                .absolute()
                .left(STEP_COL_W + spacing(Spacing::Xs, Density::Cozy))
                .right_0()
                .h(BORDER_THIN)
                .bg(palette.brand);
            if slot == i {
                Some(line.top(-STEP_GAP.half()))
            } else if slot == total && i + 1 == total {
                Some(line.bottom(-STEP_GAP.half()))
            } else {
                None
            }
        });
        row.relative()
            .when(self.is_step_dragged(i), |el| {
                el.opacity(DRAG_SOURCE_OPACITY)
            })
            .on_drag_move(
                cx.listener(move |this, event: &DragMoveEvent<StepDrag>, _, cx| {
                    this.hover_step_row(i, event, cx)
                }),
            )
            .children(indicator)
    }

    pub(super) fn wire_step_list_drop(
        &self,
        list: gpui::Div,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        list.id("actions-step-list")
            .when(self.step_drag.is_some(), |el| {
                el.track_focus(&self.step_drag_focus)
            })
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key.as_str() == ESCAPE_KEY {
                    this.cancel_step_drag(window, cx);
                }
            }))
            .on_drag_move(cx.listener(|this, event: &DragMoveEvent<StepDrag>, _, cx| {
                this.hover_step_list(event, cx)
            }))
            .on_drop(
                cx.listener(|this, drag: &StepDrag, window, cx| this.drop_step(drag, window, cx)),
            )
    }
}
