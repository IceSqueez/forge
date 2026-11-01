use std::sync::Arc;

use forge_components::{
    ActivePresentation, BORDER_THIN, Density, FONT_SECTION_LABEL, FONT_XS, ForgePalette, Icon,
    InputEvent, KeyActivated, Radius, RevealsFocusIn, SearchState, Spacing, TextInput, icon,
    radius, scaled, spacing, with_alpha,
};
use forge_storage::ActionRepo;
use forge_types::ActionId;
use gpui::{
    AnyElement, App, ClickEvent, Context, ElementId, Entity, EventEmitter, Pixels, ScrollHandle,
    SharedString, Subscription, Window, div, prelude::*, px,
};

use crate::app::async_bridge::{self, Loadable};
use crate::app::navigable_list::{ListCursor, ListStep, NavigableList, list_keys};
use crate::shared::repo_failure::RepoFailure;

const SECTION_LABEL_MB: Pixels = px(8.0);
const FILTER_MAX_W: Pixels = px(220.0);
const FILTER_HEADER_GAP: Pixels = px(12.0);
const LIST_RADIUS: Pixels = radius(Radius::Md);
const LIST_PAD: Pixels = px(8.0);
const LIST_MAX_H: Pixels = px(230.0);
const ROW_GAP: Pixels = px(8.0);
const ROW_PAD_H: Pixels = px(9.0);
const ROW_RADIUS: Pixels = radius(Radius::Sm);
const ROW_MB: Pixels = px(1.0);
const ROW_DOT: Pixels = px(6.0);
const ROW_FS: Pixels = FONT_XS;
const ROW_CHECK: Pixels = px(12.0);
const EMPTY_PAD_V: Pixels = px(20.0);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ActionChoice {
    pub id: ActionId,
    pub name: String,
}

pub(crate) struct ActionPickerCopy {
    pub id_prefix: &'static str,
    pub section: String,
    pub filter: String,
    pub loading: String,
    pub none: String,
    pub no_match: String,
    pub failed: fn(&str) -> String,
}

pub(crate) enum ActionPickerEvent {
    SelectionChanged,
    Confirmed,
    Cancelled,
}

pub(crate) struct ActionPicker {
    copy: ActionPickerCopy,
    pub(crate) filter: SearchState,
    pub(crate) actions: Loadable<Vec<ActionChoice>>,
    selected: Option<ActionId>,
    list_keys: ListCursor<ActionId>,
    list_scroll: ScrollHandle,
    _filter_sub: Subscription,
}

impl EventEmitter<ActionPickerEvent> for ActionPicker {}

impl ActionPicker {
    pub(crate) fn new(
        copy: ActionPickerCopy,
        selected: Option<ActionId>,
        action_repo: Arc<dyn ActionRepo>,
        rt_handle: &tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let filter = SearchState::new(cx, copy.filter.clone());
        let filter_sub = cx.subscribe(filter.field(), Self::on_filter_event);
        async_bridge::run_async(
            rt_handle,
            async move {
                action_repo
                    .list()
                    .await
                    .map(|actions| {
                        actions
                            .into_iter()
                            .map(|action| ActionChoice {
                                id: action.id,
                                name: action.name,
                            })
                            .collect::<Vec<_>>()
                    })
                    .map_err(RepoFailure::from)
            },
            |this, result, cx| this.apply_actions(result, cx),
            cx,
        );
        Self {
            copy,
            filter,
            actions: Loadable::Loading,
            selected,
            list_keys: ListCursor::new(cx),
            list_scroll: ScrollHandle::new(),
            _filter_sub: filter_sub,
        }
    }

    pub(crate) fn selected(&self) -> Option<ActionId> {
        self.selected
    }

    pub(crate) fn focus_filter(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.filter
            .field()
            .update(cx, |field, cx| field.focus(window, cx));
    }

    fn apply_actions(
        &mut self,
        result: Result<Vec<ActionChoice>, RepoFailure>,
        cx: &mut Context<Self>,
    ) {
        self.actions = match result {
            Ok(mut actions) => {
                actions.sort_by_key(|a| a.name.to_lowercase());
                Loadable::Loaded(actions)
            }
            Err(failure) => Loadable::Failed(failure),
        };
        cx.notify();
    }

    fn on_filter_event(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, InputEvent::Cancelled) {
            cx.emit(ActionPickerEvent::Cancelled);
            return;
        }
        if matches!(event, InputEvent::Submitted(_)) {
            self.pick_cursor_or_confirm(cx);
            return;
        }
        if self.filter.on_changed(event) {
            cx.notify();
        }
    }

    pub(crate) fn select_action(&mut self, id: ActionId, cx: &mut Context<Self>) {
        self.selected = Some(id);
        self.list_keys.move_to(id);
        cx.emit(ActionPickerEvent::SelectionChanged);
        cx.notify();
    }

    pub(crate) fn matched_action_ids(&self) -> Vec<ActionId> {
        match &self.actions {
            Loadable::Loaded(actions) => actions
                .iter()
                .filter(|choice| self.filter.matches(&choice.name))
                .map(|choice| choice.id)
                .collect(),
            Loadable::Loading | Loadable::Failed(_) => Vec::new(),
        }
    }

    fn pick_cursor_or_confirm(&mut self, cx: &mut Context<Self>) {
        match self.list_keys.activation_target(&self.matched_action_ids()) {
            Some(id) if self.selected != Some(id) => self.select_action(id, cx),
            _ => cx.emit(ActionPickerEvent::Confirmed),
        }
    }

    fn render_list(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        match &self.actions {
            Loadable::Loading => list_message(self.copy.loading.clone(), palette, density, cx),
            Loadable::Failed(failure) => {
                let reason = failure.text();
                list_message((self.copy.failed)(&reason), palette, density, cx)
            }
            Loadable::Loaded(actions) if actions.is_empty() => {
                list_message(self.copy.none.clone(), palette, density, cx)
            }
            Loadable::Loaded(actions) => {
                let matched: Vec<&ActionChoice> = actions
                    .iter()
                    .filter(|choice| self.filter.matches(&choice.name))
                    .collect();
                if matched.is_empty() {
                    list_message(self.copy.no_match.clone(), palette, density, cx)
                } else {
                    let rows: Vec<AnyElement> = matched
                        .into_iter()
                        .enumerate()
                        .map(|(index, choice)| self.render_row(index, choice, palette, cx))
                        .collect();
                    div().w_full().flex().flex_col().children(rows)
                }
            }
        }
    }

    fn render_row(
        &self,
        index: usize,
        choice: &ActionChoice,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let selected = self.selected == Some(choice.id);
        let (background, border, ink) = if selected {
            (palette.surface_overlay, palette.brand, palette.text_primary)
        } else {
            (
                with_alpha(palette.shell, 0.0),
                with_alpha(palette.shell, 0.0),
                palette.text_secondary,
            )
        };
        let id = choice.id;
        let hover_bg = palette.base;
        let mut row = div()
            .id(ElementId::Name(
                format!("{}-action-{index}", self.copy.id_prefix).into(),
            ))
            .relative()
            .w_full()
            .flex()
            .items_center()
            .gap(scaled(ROW_GAP, density))
            .mb(ROW_MB)
            .py(spacing(Spacing::Xs, density))
            .px(scaled(ROW_PAD_H, density))
            .rounded(ROW_RADIUS)
            .border(BORDER_THIN)
            .border_color(border)
            .bg(background)
            .cursor_pointer()
            .key_activated(palette.text_primary)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select_action(id, cx)))
            .child(
                div()
                    .flex_none()
                    .size(ROW_DOT)
                    .rounded_full()
                    .bg(palette.brand),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(cx.body_family())
                    .text_size(ROW_FS)
                    .text_color(ink)
                    .child(choice.name.clone()),
            );
        if self.list_keys.is_at(&id) {
            row = row.child(self.list_keys.row_anchor(Some(&self.list_scroll)));
            if !selected {
                row = row.bg(hover_bg);
            }
        }
        if selected {
            row = row.child(icon(Icon::Check, ROW_CHECK, palette.brand));
        } else {
            row = row.hover(move |s| s.bg(hover_bg));
        }
        row.into_any_element()
    }
}

impl NavigableList for ActionPicker {
    fn step_list(&mut self, step: ListStep, cx: &mut Context<Self>) {
        let rows = self.matched_action_ids();
        let selected = self.selected;
        if self
            .list_keys
            .step(&rows, selected.as_ref(), step)
            .is_some()
        {
            cx.notify();
        }
    }

    fn activate_list(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.pick_cursor_or_confirm(cx);
    }
}

impl Render for ActionPicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let density = cx.density();
        let palette = cx.palette();
        let header = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(scaled(FILTER_HEADER_GAP, density))
            .mb(SECTION_LABEL_MB)
            .child(
                div()
                    .font_family(cx.mono_family())
                    .text_size(FONT_SECTION_LABEL)
                    .text_color(palette.text_muted)
                    .child(SharedString::from(self.copy.section.to_uppercase())),
            )
            .child(
                div()
                    .flex_1()
                    .max_w(FILTER_MAX_W)
                    .child(self.filter.field().clone()),
            );
        let list = self.render_list(&palette, density, cx);
        list_keys(div(), cx)
            .w_full()
            .flex()
            .flex_col()
            .child(header)
            .child(
                div()
                    .id(ElementId::Name(
                        format!("{}-actions", self.copy.id_prefix).into(),
                    ))
                    .track_scroll(&self.list_scroll)
                    .reveals_focus_in(&self.list_scroll)
                    .w_full()
                    .max_h(LIST_MAX_H)
                    .overflow_y_scroll()
                    .p(scaled(LIST_PAD, density))
                    .rounded(LIST_RADIUS)
                    .border(BORDER_THIN)
                    .border_color(palette.border_regular)
                    .bg(palette.shell)
                    .child(list),
            )
    }
}

fn list_message(message: String, palette: &ForgePalette, density: Density, cx: &App) -> gpui::Div {
    div().w_full().flex().flex_col().child(
        div()
            .w_full()
            .py(scaled(EMPTY_PAD_V, density))
            .text_align(gpui::TextAlign::Center)
            .font_family(cx.body_family())
            .text_size(ROW_FS)
            .text_color(palette.text_muted)
            .child(message),
    )
}
