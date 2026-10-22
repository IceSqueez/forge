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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Mutex;

    use forge_storage::MockMediaRepo;
    use forge_storage::action::MockActionRepo;
    use gpui::{
        Entity, Modifiers, MouseButton, Point, TestAppContext, VisualTestContext, point, px, size,
    };

    use super::super::tests::view_with;
    use super::*;
    use crate::actions_screen::integration_gating::tests::detail;
    use crate::test_support::{error_toasts, pump, runtime, step};

    const FIRST: &str = "test.first";
    const SECOND: &str = "test.second";
    const THIRD: &str = "test.third";
    const ROW_INDEX: usize = 2;
    const ROW_TOP: f32 = 100.0;
    const ROW_WIDTH: f32 = 200.0;
    const ROW_HEIGHT: f32 = 40.0;
    const NUDGE: f32 = 0.5;
    const MAX_CHAIN_LEN: usize = 5;
    const SOURCE_HEIGHT: f32 = 40.0;
    const PRESS_AT: f32 = 10.0;
    const DRAG_TRAVEL: f32 = 20.0;
    const SETTLE_ROUNDS: usize = 3;
    const PROBE_X_FRACTION: f32 = 0.75;
    const PROBE_STEP: f32 = 4.0;

    #[test]
    fn the_pointer_picks_the_gap_on_its_side_of_the_row_center() {
        let row = Bounds::new(
            point(px(0.0), px(ROW_TOP)),
            size(px(ROW_WIDTH), px(ROW_HEIGHT)),
        );
        let center = ROW_TOP + ROW_HEIGHT / 2.0;
        for (pointer_y, slot, case) in [
            (ROW_TOP, ROW_INDEX, "top edge"),
            (center - NUDGE, ROW_INDEX, "just above center"),
            (center, ROW_INDEX + 1, "exact center"),
            (ROW_TOP + ROW_HEIGHT, ROW_INDEX + 1, "bottom edge"),
        ] {
            assert_eq!(
                drop_slot_for_pointer(ROW_INDEX, px(pointer_y), row),
                slot,
                "{case}"
            );
        }
    }

    #[test]
    fn every_move_lands_between_the_neighbours_of_the_chosen_gap() {
        for len in 1..=MAX_CHAIN_LEN {
            for from in 0..len {
                for slot in 0..=len {
                    let Some(to) = moved_step_index(from, slot, len) else {
                        continue;
                    };
                    let mut order: Vec<usize> = (0..len).collect();
                    let dragged = order.remove(from);
                    order.insert(to, dragged);
                    let expected = (
                        (0..slot).rev().find(|&k| k != from),
                        (slot..len).find(|&k| k != from),
                    );
                    let landed = (
                        to.checked_sub(1).map(|k| order[k]),
                        order.get(to + 1).copied(),
                    );
                    assert_eq!(landed, expected, "len {len} from {from} slot {slot}");
                }
            }
        }
    }

    #[test]
    fn only_the_two_gaps_beside_the_dragged_step_leave_the_chain_unchanged() {
        for len in 1..=MAX_CHAIN_LEN {
            for from in 0..len {
                for slot in 0..=len {
                    assert_eq!(
                        moved_step_index(from, slot, len).is_none(),
                        slot == from || slot == from + 1,
                        "len {len} from {from} slot {slot}"
                    );
                }
            }
        }
    }

    #[test]
    fn out_of_range_drags_and_gaps_move_nothing() {
        for (from, slot, len, case) in [
            (3, 0, 3, "dragged index past the end"),
            (0, 4, 3, "gap past the end"),
            (0, 0, 0, "empty chain"),
        ] {
            assert_eq!(moved_step_index(from, slot, len), None, "{case}");
        }
    }

    #[derive(Clone)]
    struct ForeignDrag;

    struct Ghost;

    impl Render for Ghost {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    struct Host {
        screen: Option<Entity<ScreenActionsView>>,
        focus: FocusHandle,
        probe: Option<(ActionId, usize)>,
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let source = div().id("drag-source").w_full().h(px(SOURCE_HEIGHT));
            let source = match self.probe {
                Some((action_id, from)) => source.on_drag(
                    StepDrag {
                        action_id,
                        path: Vec::new(),
                        from,
                    },
                    |_, _, _, cx| cx.new(|_| Ghost),
                ),
                None => source.on_drag(ForeignDrag, |_, _, _, cx| cx.new(|_| Ghost)),
            };
            div()
                .size_full()
                .flex()
                .flex_col()
                .track_focus(&self.focus)
                .child(source)
                .children(self.screen.clone())
        }
    }

    struct Store {
        current: Mutex<Action>,
        saved: Mutex<Vec<Action>>,
        failures_left: Mutex<usize>,
    }

    impl Store {
        fn save(&self, action: &Action) -> Result<(), forge_storage::StorageError> {
            let mut failures = self.failures_left.lock().unwrap();
            if *failures > 0 {
                *failures -= 1;
                return Err(forge_storage::StorageError::NotFound {
                    key: action.id.to_string(),
                });
            }
            self.saved.lock().unwrap().push(action.clone());
            *self.current.lock().unwrap() = action.clone();
            Ok(())
        }
    }

    struct Fixture<'a> {
        view: Entity<ScreenActionsView>,
        host: Entity<Host>,
        store: Arc<Store>,
        rt: tokio::runtime::Runtime,
        vcx: &'a mut VisualTestContext,
    }

    fn chain() -> Vec<SubActionStep> {
        vec![step(FIRST, true), step(SECOND, false), step(THIRD, true)]
    }

    fn order(kinds: [&str; 3]) -> Vec<String> {
        kinds.map(str::to_owned).to_vec()
    }

    fn kinds(action: &Action) -> Vec<String> {
        action
            .sub_actions
            .iter()
            .map(|step| step.kind_id.clone())
            .collect()
    }

    fn mount(cx: &mut TestAppContext, rendered: bool) -> Fixture<'_> {
        mount_failing(cx, rendered, 0)
    }

    fn mount_failing(cx: &mut TestAppContext, rendered: bool, failures: usize) -> Fixture<'_> {
        crate::i18n::install_language(forge_storage::Language::En);
        cx.update(|cx| cx.set_global(forge_components::Toasts::new()));
        let rt = runtime();
        let opened = detail(chain());
        let store = Arc::new(Store {
            current: Mutex::new(opened.action.clone()),
            saved: Mutex::new(Vec::new()),
            failures_left: Mutex::new(failures),
        });
        let mut repo = MockActionRepo::new();
        let listed = Arc::clone(&store);
        repo.expect_list()
            .returning(move || Ok(vec![listed.current.lock().unwrap().clone()]));
        let fetched = Arc::clone(&store);
        repo.expect_get()
            .returning(move |_| Ok(Some(fetched.current.lock().unwrap().clone())));
        let saving = Arc::clone(&store);
        repo.expect_save()
            .returning(move |action| saving.save(action));
        let view = view_with(
            cx,
            &rt,
            Vec::new(),
            MockMediaRepo::new(),
            SubActionRegistry::new(),
            Arc::new(repo),
        );
        for _ in 0..SETTLE_ROUNDS {
            pump(&rt);
            cx.run_until_parked();
        }
        let action_id = opened.action.id;
        view.update(cx, |view, _| {
            view.selected = Some(action_id);
            view.detail = Some(opened);
        });
        let screen = rendered.then(|| view.clone());
        let (host, vcx) = cx.add_window_view(|_, cx| Host {
            screen,
            focus: cx.focus_handle(),
            probe: None,
        });
        Fixture {
            view,
            host,
            store,
            rt,
            vcx,
        }
    }

    impl Fixture<'_> {
        fn settle(&mut self) {
            for _ in 0..SETTLE_ROUNDS {
                pump(&self.rt);
                self.vcx.run_until_parked();
            }
        }

        fn action_id(&mut self) -> ActionId {
            self.view
                .read_with(self.vcx, |view, _| view.detail.as_ref().unwrap().action.id)
        }

        fn payload(&mut self, from: usize) -> StepDrag {
            let action_id = self.action_id();
            let path = self
                .view
                .read_with(self.vcx, |view, _| view.nav_path.clone());
            StepDrag {
                action_id,
                path,
                from,
            }
        }

        fn focus_host(&mut self) {
            let focus = self.host.read_with(self.vcx, |host, _| host.focus.clone());
            self.vcx.update(|window, cx| window.focus(&focus, cx));
        }

        fn host_focused(&mut self) -> bool {
            let focus = self.host.read_with(self.vcx, |host, _| host.focus.clone());
            self.vcx.update(|window, _| focus.is_focused(window))
        }

        fn start_pointer_drag(&mut self) {
            let press: Point<Pixels> = point(px(PRESS_AT), px(PRESS_AT));
            self.vcx
                .simulate_mouse_down(press, MouseButton::Left, Modifiers::none());
            self.vcx.simulate_mouse_move(
                point(px(PRESS_AT + DRAG_TRAVEL), px(PRESS_AT)),
                MouseButton::Left,
                Modifiers::none(),
            );
        }

        fn release_pointer(&mut self) {
            self.vcx.simulate_mouse_up(
                point(px(PRESS_AT + DRAG_TRAVEL), px(PRESS_AT)),
                MouseButton::Left,
                Modifiers::none(),
            );
        }

        fn point_over_first_row(&mut self) -> Point<Pixels> {
            let last = chain().len() - 1;
            let action_id = self.action_id();
            self.host.update(self.vcx, |host, cx| {
                host.probe = Some((action_id, last));
                cx.notify();
            });
            self.settle();
            self.start_pointer_drag();
            self.begin(last);
            let viewport = self.vcx.update(|window, _| window.viewport_size());
            let x = viewport.width * PROBE_X_FRACTION;
            let mut y = px(SOURCE_HEIGHT);
            let mut found = None;
            while found.is_none() && y < viewport.height {
                let at = point(x, y);
                self.vcx
                    .simulate_mouse_move(at, MouseButton::Left, Modifiers::none());
                if self.indicator() == Some(0) {
                    found = Some(at);
                }
                y += px(PROBE_STEP);
            }
            self.vcx.simulate_keystrokes("escape");
            found.unwrap()
        }

        fn begin(&mut self, from: usize) {
            let view = self.view.clone();
            self.vcx.update(|window, cx| {
                view.update(cx, |view, cx| view.begin_step_drag(from, window, cx));
            });
        }

        fn hover(&mut self, slot: usize) {
            self.view
                .update(self.vcx, |view, cx| view.set_step_drop_slot(Some(slot), cx));
        }

        fn drop_payload(&mut self, drag: StepDrag) {
            let view = self.view.clone();
            self.vcx.update(|window, cx| {
                view.update(cx, |view, cx| view.drop_step(&drag, window, cx));
            });
        }

        fn reorder(&mut self, from: usize, slot: usize) {
            let drag = self.payload(from);
            self.begin(from);
            self.hover(slot);
            self.drop_payload(drag);
        }

        fn drag_and_drop(&mut self, from: usize, slot: usize) {
            self.reorder(from, slot);
            self.settle();
        }

        fn saved_orders(&self) -> Vec<Vec<String>> {
            self.store.saved.lock().unwrap().iter().map(kinds).collect()
        }

        fn shown_order(&mut self) -> Vec<String> {
            self.view.read_with(self.vcx, |view, _| {
                kinds(&view.detail.as_ref().unwrap().action)
            })
        }

        fn save_idle(&mut self) -> bool {
            self.view
                .read_with(self.vcx, |view, _| view.action_writes.is_empty())
        }

        fn accepts(&mut self, drag: &StepDrag) -> bool {
            self.view
                .read_with(self.vcx, |view, _| view.accepts_step_drag(drag))
        }

        fn indicator(&mut self) -> Option<usize> {
            self.view
                .read_with(self.vcx, |view, _| view.step_drop_indicator_slot())
        }

        fn session_open(&mut self) -> bool {
            self.view
                .read_with(self.vcx, |view, _| view.step_drag.is_some())
        }

        fn pointer_dragging(&mut self) -> bool {
            self.vcx.update(|_, cx| cx.has_active_drag())
        }
    }

    #[gpui::test]
    fn dropping_into_a_gap_saves_the_chain_once_in_the_new_order(cx: &mut TestAppContext) {
        for (from, slot, expected, case) in [
            (0, 3, [SECOND, THIRD, FIRST], "first step to the last gap"),
            (2, 0, [THIRD, FIRST, SECOND], "last step to the first gap"),
            (0, 2, [SECOND, FIRST, THIRD], "first step one down"),
            (1, 0, [SECOND, FIRST, THIRD], "disabled step to the top"),
        ] {
            let mut fixture = mount(cx, false);

            fixture.drag_and_drop(from, slot);

            assert_eq!(fixture.saved_orders(), vec![order(expected)], "{case}");
        }
    }

    #[gpui::test]
    fn dropping_into_a_gap_beside_the_dragged_step_saves_nothing(cx: &mut TestAppContext) {
        for slot in [1, 2] {
            let mut fixture = mount(cx, false);

            fixture.drag_and_drop(1, slot);

            assert!(fixture.saved_orders().is_empty(), "slot {slot}");
        }
    }

    #[gpui::test]
    fn a_drop_before_any_gap_was_hovered_saves_nothing(cx: &mut TestAppContext) {
        let mut fixture = mount(cx, false);
        let drag = fixture.payload(0);
        fixture.begin(0);

        fixture.drop_payload(drag);
        fixture.settle();

        assert!(fixture.saved_orders().is_empty());
    }

    #[gpui::test]
    fn a_drop_ends_the_drag_session(cx: &mut TestAppContext) {
        let mut fixture = mount(cx, false);

        fixture.drag_and_drop(0, 3);

        assert!(!fixture.session_open());
    }

    #[gpui::test]
    fn only_a_payload_from_the_visible_chain_is_accepted(cx: &mut TestAppContext) {
        let nested = vec![nav::NavFrame {
            step_index: 0,
            chain_key: FIRST.to_owned(),
            case_index: None,
        }];
        for (case, accepted) in [
            ("visible chain", true),
            ("another action", false),
            ("nested chain", false),
            ("index past the chain", false),
            ("no action open", false),
        ] {
            let mut fixture = mount(cx, false);
            let mut drag = fixture.payload(0);
            match case {
                "another action" => drag.action_id = ActionId::new(),
                "nested chain" => drag.path = nested.clone(),
                "index past the chain" => drag.from = chain().len(),
                "no action open" => fixture
                    .view
                    .update(fixture.vcx, |view, _| view.detail = None),
                _ => {}
            }

            assert_eq!(fixture.accepts(&drag), accepted, "{case}");
        }
    }

    #[gpui::test]
    fn no_payload_is_accepted_while_a_dialog_or_the_step_menu_is_open(cx: &mut TestAppContext) {
        for case in ["delete prompt", "step menu"] {
            let mut fixture = mount(cx, false);
            let drag = fixture.payload(0);
            fixture.view.update(fixture.vcx, |view, _| match case {
                "delete prompt" => view.pending_delete.request(ActionId::new()),
                _ => view.step_menu_open = Some(0),
            });

            assert!(!fixture.accepts(&drag), "{case}");
        }
    }

    #[gpui::test]
    fn a_drop_of_a_payload_from_another_action_saves_nothing(cx: &mut TestAppContext) {
        let mut fixture = mount(cx, false);
        let mut drag = fixture.payload(0);
        drag.action_id = ActionId::new();
        fixture.begin(0);
        fixture.hover(3);

        fixture.drop_payload(drag);
        fixture.settle();

        assert!(fixture.saved_orders().is_empty());
    }

    #[gpui::test]
    fn a_drop_after_the_chain_shrank_saves_nothing(cx: &mut TestAppContext) {
        for (from, slot, case) in [
            (2, 0, "dragged step no longer exists"),
            (0, 3, "hovered gap no longer exists"),
        ] {
            let mut fixture = mount(cx, false);
            let drag = fixture.payload(from);
            fixture.begin(from);
            fixture.hover(slot);
            fixture.view.update(fixture.vcx, |view, _| {
                let detail = view.detail.as_mut().unwrap();
                detail.action.sub_actions.truncate(chain().len() - 1);
            });

            fixture.drop_payload(drag);
            fixture.settle();

            assert!(fixture.saved_orders().is_empty(), "{case}");
        }
    }

    #[gpui::test]
    fn the_indicator_shows_only_for_gaps_that_move_the_step(cx: &mut TestAppContext) {
        for (slot, shown) in [(0, Some(0)), (1, None), (2, None), (3, Some(3))] {
            let mut fixture = mount(cx, false);
            fixture.begin(1);

            fixture.hover(slot);

            assert_eq!(fixture.indicator(), shown, "slot {slot}");
        }
    }

    #[gpui::test]
    fn escape_during_a_drag_stops_it_and_ends_the_session(cx: &mut TestAppContext) {
        let mut fixture = mount(cx, true);
        fixture.start_pointer_drag();
        fixture.begin(0);
        fixture.settle();

        fixture.vcx.simulate_keystrokes("escape");

        assert_eq!(
            (fixture.pointer_dragging(), fixture.session_open()),
            (false, false)
        );
    }

    #[gpui::test]
    fn escape_during_a_drag_returns_focus_to_where_it_was(cx: &mut TestAppContext) {
        let mut fixture = mount(cx, true);
        fixture.start_pointer_drag();
        fixture.focus_host();
        fixture.begin(0);
        fixture.settle();

        fixture.vcx.simulate_keystrokes("escape");

        assert!(fixture.host_focused());
    }

    #[gpui::test]
    fn a_render_while_the_pointer_still_drags_keeps_the_session(cx: &mut TestAppContext) {
        let mut fixture = mount(cx, true);
        fixture.start_pointer_drag();
        fixture.begin(0);

        fixture.vcx.update(|window, _| window.refresh());
        fixture.settle();

        assert!(fixture.session_open());
    }

    #[gpui::test]
    fn releasing_the_pointer_outside_the_list_ends_the_session(cx: &mut TestAppContext) {
        let mut fixture = mount(cx, true);
        fixture.start_pointer_drag();
        fixture.begin(0);
        fixture.settle();

        fixture.release_pointer();
        fixture.settle();

        assert!(!fixture.session_open());
    }

    #[gpui::test]
    fn a_press_inside_the_step_list_keeps_the_previous_focus(cx: &mut TestAppContext) {
        let mut fixture = mount(cx, true);
        let inside = fixture.point_over_first_row();
        fixture.settle();
        fixture.focus_host();

        fixture
            .vcx
            .simulate_mouse_down(inside, MouseButton::Left, Modifiers::none());

        assert!(fixture.host_focused());
    }

    #[gpui::test]
    fn escape_after_a_press_inside_the_list_returns_focus_to_where_it_was(cx: &mut TestAppContext) {
        let mut fixture = mount(cx, true);
        let inside = fixture.point_over_first_row();
        fixture.settle();
        fixture.focus_host();
        fixture
            .vcx
            .simulate_mouse_down(inside, MouseButton::Left, Modifiers::none());
        fixture.begin(0);
        fixture.settle();

        fixture.vcx.simulate_keystrokes("escape");

        assert_eq!(
            (fixture.session_open(), fixture.host_focused()),
            (false, true)
        );
    }

    #[gpui::test]
    fn two_reorders_before_the_first_save_lands_show_both_moves(cx: &mut TestAppContext) {
        let mut fixture = mount(cx, false);

        fixture.reorder(0, 3);
        fixture.reorder(0, 3);

        assert_eq!(fixture.shown_order(), order([THIRD, FIRST, SECOND]));
    }

    #[gpui::test]
    fn two_reorders_before_the_first_save_lands_end_saved_and_shown_with_both_moves(
        cx: &mut TestAppContext,
    ) {
        let mut fixture = mount(cx, false);
        fixture.reorder(0, 3);
        fixture.reorder(0, 3);

        fixture.settle();

        let both = order([THIRD, FIRST, SECOND]);
        assert_eq!(
            (
                fixture.saved_orders().last().cloned(),
                fixture.shown_order(),
                fixture.save_idle()
            ),
            (Some(both.clone()), both, true)
        );
    }

    #[gpui::test]
    fn reorders_queued_behind_a_running_save_send_only_the_latest(cx: &mut TestAppContext) {
        let mut fixture = mount(cx, false);
        fixture.reorder(0, 3);
        fixture.reorder(0, 3);
        fixture.reorder(0, 3);

        fixture.settle();

        assert_eq!(
            fixture.saved_orders(),
            vec![order([SECOND, THIRD, FIRST]), order([FIRST, SECOND, THIRD])]
        );
    }

    #[gpui::test]
    fn a_failed_save_with_nothing_queued_shows_the_stored_order_again(cx: &mut TestAppContext) {
        let mut fixture = mount_failing(cx, false, 1);

        fixture.drag_and_drop(0, 3);

        assert_eq!(
            (fixture.shown_order(), fixture.save_idle()),
            (order([FIRST, SECOND, THIRD]), true)
        );
    }

    #[gpui::test]
    fn a_failed_save_raises_an_error_toast(cx: &mut TestAppContext) {
        let mut fixture = mount_failing(cx, false, 1);

        fixture.drag_and_drop(0, 3);

        assert_eq!(fixture.vcx.update(|_, cx| error_toasts(cx)), 1);
    }

    #[gpui::test]
    fn a_failed_save_still_sends_the_reorder_queued_behind_it(cx: &mut TestAppContext) {
        let mut fixture = mount_failing(cx, false, 1);
        fixture.reorder(0, 3);
        fixture.reorder(0, 3);

        fixture.settle();

        let both = order([THIRD, FIRST, SECOND]);
        assert_eq!(
            (fixture.saved_orders(), fixture.shown_order()),
            (vec![both.clone()], both)
        );
    }

    #[gpui::test]
    fn a_reload_landing_during_a_save_keeps_the_unsaved_order(cx: &mut TestAppContext) {
        let mut fixture = mount(cx, false);
        fixture.reorder(0, 3);
        let action_id = fixture.action_id();
        let stale = fixture.view.read_with(fixture.vcx, |view, _| {
            let mut stale = view.detail.clone().unwrap();
            stale.action.sub_actions = chain();
            stale
        });

        fixture.view.update(fixture.vcx, |view, cx| {
            view.apply_detail(action_id, stale, cx)
        });

        assert_eq!(fixture.shown_order(), order([SECOND, THIRD, FIRST]));
    }

    #[gpui::test]
    fn a_reload_with_no_save_running_replaces_the_shown_order(cx: &mut TestAppContext) {
        let mut fixture = mount(cx, false);
        let action_id = fixture.action_id();
        let stored = fixture.view.read_with(fixture.vcx, |view, _| {
            let mut stored = view.detail.clone().unwrap();
            stored.action.sub_actions.reverse();
            stored
        });

        fixture.view.update(fixture.vcx, |view, cx| {
            view.apply_detail(action_id, stored, cx)
        });

        assert_eq!(fixture.shown_order(), order([THIRD, SECOND, FIRST]));
    }
}
