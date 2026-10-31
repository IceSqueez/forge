use forge_components::Density;
use forge_components::FONT_LABEL;
use forge_components::FONT_META;
use forge_components::FONT_XS;
use forge_components::Radius;
use forge_components::Spacing;
use forge_components::focus_inside_modal;
use forge_components::radius;
use forge_components::spacing;
use std::sync::Arc;

use forge_components::{
    BORDER_THIN, FONT_SECTION_LABEL, ForgePalette, Icon, InputEvent, OverlayPosition, SearchState,
    TextInput, body_family, ghost_button_with_icon, icon, modal, mono_family, overlay,
    primary_button, scaled, secondary_button, segment, segmented, tr, with_alpha,
};
use forge_storage::ActionRepo;
use forge_types::{ActionId, TriggerInstanceId};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, Pixels, SharedString, Subscription,
    Window, div, prelude::*, px,
};

use crate::async_bridge::{self, Loadable};
use crate::repo_failure::RepoFailure;
use forge_components::ActivePresentation;
use forge_hotkey::{HotkeyEdge, combo_keys};

const MODAL_W: Pixels = px(520.0);
const BODY_PAD_H: Pixels = px(18.0);
const BODY_MAX_H: Pixels = px(460.0);

const SECTION_LABEL_MB: Pixels = px(8.0);
const RECAPTURE_HEIGHT: Pixels = px(32.0);

const KEYCAP_FS: Pixels = FONT_LABEL;
const KEYCAP_RADIUS: Pixels = px(5.0);
const KEYCAP_PAD_V: Pixels = px(3.0);
const KEYCAP_PAD_H: Pixels = px(8.0);
const KEYCAP_MIN_W: Pixels = px(24.0);

const DISPLAY_PAD_V: Pixels = px(7.0);
const DISPLAY_PAD_H: Pixels = px(11.0);
const DISPLAY_RADIUS: Pixels = px(7.0);
const DISPLAY_GAP: Pixels = px(7.0);
const LISTEN_GLYPH: Pixels = px(12.0);

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

const FOOTER_GAP: Pixels = px(8.0);
const FOOTER_HINT_FS: Pixels = FONT_META;

pub(crate) struct ActionModalLaunch {
    pub instance_id: Option<TriggerInstanceId>,
    pub combo: String,
    pub edge: HotkeyEdge,
    pub locked_edge: Option<HotkeyEdge>,
    pub linked_action: Option<ActionId>,
}

pub(crate) struct BindingDraft {
    pub instance_id: Option<TriggerInstanceId>,
    pub combo: String,
    pub edge: HotkeyEdge,
    pub action_id: ActionId,
}

pub(crate) enum HotkeyActionModalEvent {
    Save(Box<BindingDraft>),
    Recapture,
    Cancel,
}

struct ActionChoice {
    id: ActionId,
    name: String,
}

pub(crate) struct HotkeyActionModal {
    instance_id: Option<TriggerInstanceId>,
    combo: String,
    edge: HotkeyEdge,
    locked_edge: Option<HotkeyEdge>,
    capturing: bool,
    filter: SearchState,
    actions: Loadable<Vec<ActionChoice>>,
    selected_action: Option<ActionId>,
    focus_pending: bool,
    _subs: Vec<Subscription>,
}

impl EventEmitter<HotkeyActionModalEvent> for HotkeyActionModal {}

impl HotkeyActionModal {
    pub(crate) fn new(
        launch: ActionModalLaunch,
        action_repo: Arc<dyn ActionRepo>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let ActionModalLaunch {
            instance_id,
            combo,
            edge,
            locked_edge,
            linked_action,
        } = launch;
        let palette = cx.palette();
        let filter = SearchState::new(cx, palette, tr!("hotkeys_modal_filter_actions"));
        let subs = vec![cx.subscribe(filter.field(), Self::on_filter_event)];

        async_bridge::run_async(
            &rt_handle,
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
            instance_id,
            combo,
            edge,
            locked_edge,
            capturing: false,
            filter,
            actions: Loadable::Loading,
            selected_action: linked_action,
            focus_pending: true,
            _subs: subs,
        }
    }

    pub(crate) fn apply_capture(&mut self, combo: String, cx: &mut Context<Self>) {
        self.capturing = false;
        self.combo = combo;
        cx.notify();
    }

    pub(crate) fn set_edge_lock(
        &mut self,
        locked_edge: Option<HotkeyEdge>,
        cx: &mut Context<Self>,
    ) {
        self.locked_edge = locked_edge;
        if locked_edge == Some(self.edge) {
            self.edge = self.edge.opposite();
        }
        cx.notify();
    }

    pub(crate) fn cancel_capture(&mut self, cx: &mut Context<Self>) {
        if !self.capturing {
            return;
        }
        self.capturing = false;
        cx.notify();
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
            self.cancel(cx);
            return;
        }
        if self.filter.on_changed(event) {
            cx.notify();
        }
    }

    fn select_edge(&mut self, edge: HotkeyEdge, cx: &mut Context<Self>) {
        if self.locked_edge == Some(edge) || self.edge == edge {
            return;
        }
        self.edge = edge;
        cx.notify();
    }

    fn select_action(&mut self, id: ActionId, cx: &mut Context<Self>) {
        self.selected_action = Some(id);
        cx.notify();
    }

    fn recapture(&mut self, cx: &mut Context<Self>) {
        self.capturing = true;
        cx.emit(HotkeyActionModalEvent::Recapture);
        cx.notify();
    }

    fn can_save(&self) -> bool {
        self.selected_action.is_some() && !self.combo.is_empty() && !self.capturing
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let Some(action_id) = self.selected_action.filter(|_| self.can_save()) else {
            return;
        };
        cx.emit(HotkeyActionModalEvent::Save(Box::new(BindingDraft {
            instance_id: self.instance_id,
            combo: self.combo.clone(),
            edge: self.edge,
            action_id,
        })));
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(HotkeyActionModalEvent::Cancel);
    }

    fn render_combo(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let border = if self.capturing {
            palette.success
        } else {
            palette.border_input
        };
        let mut display = div()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(scaled(DISPLAY_GAP, density))
            .py(scaled(DISPLAY_PAD_V, density))
            .px(scaled(DISPLAY_PAD_H, density))
            .rounded(DISPLAY_RADIUS)
            .border(BORDER_THIN)
            .border_color(border)
            .bg(palette.shell);
        if self.capturing {
            display = display
                .child(icon(Icon::Keyboard, LISTEN_GLYPH, palette.success))
                .child(
                    div()
                        .font_family(body_family())
                        .text_size(ROW_FS)
                        .text_color(palette.success)
                        .child(tr!("hotkeys_capture_prompt")),
                );
        } else {
            display = display.child(keycaps(&self.combo, palette, density));
        }

        let recapture =
            ghost_button_with_icon(Icon::Keyboard, tr!("hotkeys_modal_recapture"), palette)
                .height(RECAPTURE_HEIGHT)
                .on_click(
                    "hotkeys-modal-recapture",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.recapture(cx)),
                );

        div()
            .w_full()
            .flex()
            .flex_col()
            .mb(spacing(Spacing::Md, density))
            .child(section_caption(
                &tr!("hotkeys_modal_section_combo"),
                palette,
            ))
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Sm, density))
                    .child(display)
                    .child(div().flex_none().child(recapture)),
            )
            .into_any_element()
    }

    fn edge_hints(&self) -> Vec<String> {
        let mut hints = Vec::new();
        match self.locked_edge {
            Some(HotkeyEdge::Press) => hints.push(tr!("hotkeys_modal_edge_locked_press")),
            Some(HotkeyEdge::Release) => hints.push(tr!("hotkeys_modal_edge_locked_release")),
            None => {}
        }
        if self.edge == HotkeyEdge::Release {
            hints.push(tr!("hotkeys_modal_edge_queue_hint"));
        }
        hints
    }

    fn render_edge(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let selector = segmented(
            vec![
                segment(
                    "hotkeys-modal-edge-press",
                    tr!("hotkeys_modal_edge_press"),
                    self.edge == HotkeyEdge::Press,
                    cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.select_edge(HotkeyEdge::Press, cx)
                    }),
                )
                .disabled(self.locked_edge == Some(HotkeyEdge::Press)),
                segment(
                    "hotkeys-modal-edge-release",
                    tr!("hotkeys_modal_edge_release"),
                    self.edge == HotkeyEdge::Release,
                    cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.select_edge(HotkeyEdge::Release, cx)
                    }),
                )
                .disabled(self.locked_edge == Some(HotkeyEdge::Release)),
            ],
            palette,
        );

        let hints = self.edge_hints().into_iter().map(|hint| {
            div()
                .mt(spacing(Spacing::Xs, density))
                .font_family(body_family())
                .text_size(FOOTER_HINT_FS)
                .text_color(palette.text_faint)
                .child(hint)
        });

        div()
            .w_full()
            .flex()
            .flex_col()
            .mb(spacing(Spacing::Md, density))
            .child(section_caption(&tr!("hotkeys_modal_section_edge"), palette))
            .child(div().flex().items_center().child(selector))
            .children(hints)
            .into_any_element()
    }

    fn render_actions(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let header = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(scaled(FILTER_HEADER_GAP, density))
            .mb(SECTION_LABEL_MB)
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_SECTION_LABEL)
                    .text_color(palette.text_muted)
                    .child(SharedString::from(
                        tr!("hotkeys_modal_section_action").to_uppercase(),
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .max_w(FILTER_MAX_W)
                    .child(self.filter.field().clone()),
            );

        let list = match &self.actions {
            Loadable::Loading => {
                list_message(tr!("hotkeys_modal_actions_loading"), palette, density)
            }
            Loadable::Failed(failure) => {
                let reason = failure.text();
                list_message(
                    tr!("hotkeys_toast_error", message = reason.as_str()),
                    palette,
                    density,
                )
            }
            Loadable::Loaded(actions) if actions.is_empty() => {
                list_message(tr!("hotkeys_modal_actions_none"), palette, density)
            }
            Loadable::Loaded(actions) => {
                let matched: Vec<&ActionChoice> = actions
                    .iter()
                    .filter(|choice| self.filter.matches(&choice.name))
                    .collect();
                if matched.is_empty() {
                    list_message(tr!("hotkeys_modal_actions_empty"), palette, density)
                } else {
                    let rows: Vec<AnyElement> = matched
                        .into_iter()
                        .enumerate()
                        .map(|(index, choice)| self.render_action_row(index, choice, palette, cx))
                        .collect();
                    div().w_full().flex().flex_col().children(rows)
                }
            }
        };

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(header)
            .child(
                div()
                    .id("hotkeys-modal-actions")
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
            .into_any_element()
    }

    fn render_action_row(
        &self,
        index: usize,
        choice: &ActionChoice,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let selected = self.selected_action == Some(choice.id);
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
            .id(("hotkeys-modal-action", index))
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
                    .font_family(body_family())
                    .text_size(ROW_FS)
                    .text_color(ink)
                    .child(choice.name.clone()),
            );
        if selected {
            row = row.child(icon(Icon::Check, ROW_CHECK, palette.brand));
        } else {
            row = row.hover(move |s| s.bg(hover_bg));
        }
        row.into_any_element()
    }

    fn render_footer(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let hint = if self.can_save() {
            tr!("hotkeys_modal_hint_ready")
        } else {
            tr!("hotkeys_modal_hint_pick_action")
        };
        let confirm_label = if self.instance_id.is_some() {
            tr!("hotkeys_modal_save_changes")
        } else {
            tr!("hotkeys_modal_add_binding")
        };

        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(scaled(FILTER_HEADER_GAP, density))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FOOTER_HINT_FS)
                    .text_color(palette.text_faint)
                    .child(hint),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(scaled(FOOTER_GAP, density))
                    .child(secondary_button(tr!("common_cancel"), palette).on_click(
                        "hotkeys-modal-cancel",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
                    ))
                    .child(
                        primary_button(confirm_label, palette)
                            .disabled(!self.can_save())
                            .on_click(
                                "hotkeys-modal-save",
                                cx.listener(|this, _: &ClickEvent, _, cx| this.save(cx)),
                            ),
                    ),
            )
            .into_any_element()
    }
}

pub(crate) fn keycaps(combo: &str, palette: &ForgePalette, density: Density) -> impl IntoElement {
    let mut row = div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xxs, density));
    for key in combo_keys(combo) {
        row = row.child(
            div()
                .flex_none()
                .min_w(KEYCAP_MIN_W)
                .py(scaled(KEYCAP_PAD_V, density))
                .px(scaled(KEYCAP_PAD_H, density))
                .rounded(KEYCAP_RADIUS)
                .border(BORDER_THIN)
                .border_color(palette.border_regular)
                .bg(palette.shell)
                .text_align(gpui::TextAlign::Center)
                .font_family(mono_family())
                .text_size(KEYCAP_FS)
                .text_color(palette.text_primary)
                .child(key.to_owned()),
        );
    }
    row
}

fn list_message(message: String, palette: &ForgePalette, density: Density) -> gpui::Div {
    div().w_full().flex().flex_col().child(
        div()
            .w_full()
            .py(scaled(EMPTY_PAD_V, density))
            .text_align(gpui::TextAlign::Center)
            .font_family(body_family())
            .text_size(ROW_FS)
            .text_color(palette.text_muted)
            .child(message),
    )
}

fn section_caption(label: &str, palette: &ForgePalette) -> impl IntoElement {
    div()
        .w_full()
        .mb(SECTION_LABEL_MB)
        .font_family(mono_family())
        .text_size(FONT_SECTION_LABEL)
        .text_color(palette.text_muted)
        .child(SharedString::from(label.to_uppercase()))
}

impl Render for HotkeyActionModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let density = cx.density();
        let palette = cx.palette();
        if self.focus_pending {
            self.focus_pending = false;
            focus_inside_modal(window, cx, |window, cx| {
                self.filter
                    .field()
                    .update(cx, |field, cx| field.focus(window, cx))
            });
        }

        let body = div()
            .id("hotkeys-modal-body")
            .w_full()
            .max_h(BODY_MAX_H)
            .overflow_y_scroll()
            .py(spacing(Spacing::Md, density))
            .px(scaled(BODY_PAD_H, density))
            .flex()
            .flex_col()
            .child(self.render_combo(&palette, cx))
            .child(self.render_edge(&palette, cx))
            .child(self.render_actions(&palette, cx));

        let (title, subtitle) = if self.instance_id.is_some() {
            (tr!("hotkeys_modal_title_edit"), self.combo.clone())
        } else {
            (
                tr!("hotkeys_modal_title_add"),
                tr!("hotkeys_modal_subtitle_captured"),
            )
        };

        let card = modal(title, body, &palette)
            .header_icon(Icon::Keyboard, palette.success)
            .subtitle(subtitle)
            .width(MODAL_W)
            .flush_body()
            .footer(self.render_footer(&palette, cx))
            .on_close(
                "hotkeys-modal-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
            );

        let view = cx.entity();
        div().absolute().top_0().left_0().size_full().child(
            overlay(card, &palette)
                .position(OverlayPosition::Center)
                .on_dismiss("hotkeys-modal-scrim", move |_window, cx| {
                    view.update(cx, |this, cx| this.cancel(cx));
                }),
        )
    }
}

#[cfg(test)]
mod tests {
    use forge_storage::MockActionRepo;
    use forge_storage::{Language, StorageError};
    use gpui::{Entity, TestAppContext};

    use super::*;
    use crate::i18n::install_language;
    use crate::test_support::{
        action_running, install_presentation, pump, runtime, shown_in_english,
    };

    fn outage() -> StorageError {
        StorageError::Connection {
            reason: "disk gone".to_owned(),
        }
    }

    fn opened(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        listed: fn() -> Result<Vec<forge_types::Action>, StorageError>,
    ) -> Entity<HotkeyActionModal> {
        install_language(Language::En);
        install_presentation(cx);
        let mut repo = MockActionRepo::new();
        repo.expect_list().returning(listed);
        cx.update(|cx| {
            cx.new(|cx| {
                HotkeyActionModal::new(
                    ActionModalLaunch {
                        instance_id: None,
                        combo: String::new(),
                        edge: HotkeyEdge::Press,
                        locked_edge: None,
                        linked_action: None,
                    },
                    Arc::new(repo),
                    rt.handle().clone(),
                    cx,
                )
            })
        })
    }

    fn settled(cx: &mut TestAppContext, rt: &tokio::runtime::Runtime) {
        pump(rt);
        cx.run_until_parked();
    }

    #[gpui::test]
    fn the_list_is_loading_until_the_repository_answers(cx: &mut TestAppContext) {
        let rt = runtime();
        let modal = opened(cx, &rt, || Ok(Vec::new()));

        let before = modal.read_with(cx, |modal, _| matches!(modal.actions, Loadable::Loading));
        settled(cx, &rt);
        let after = modal.read_with(
            cx,
            |modal, _| matches!(&modal.actions, Loadable::Loaded(listed) if listed.is_empty()),
        );

        assert_eq!((before, after), (true, true));
    }

    #[gpui::test]
    fn loaded_actions_are_listed_ignoring_case(cx: &mut TestAppContext) {
        let rt = runtime();
        let modal = opened(cx, &rt, || {
            Ok(["beta", "Alpha", "Gamma"]
                .map(|name| action_running(name, Vec::new()))
                .to_vec())
        });
        settled(cx, &rt);

        let names: Vec<String> = modal.read_with(cx, |modal, _| {
            modal
                .actions
                .loaded()
                .expect("the list loaded")
                .iter()
                .map(|choice| choice.name.clone())
                .collect()
        });

        assert_eq!(names, ["Alpha", "beta", "Gamma"]);
    }

    #[gpui::test]
    fn a_repository_failure_keeps_the_text_shown_to_the_user(cx: &mut TestAppContext) {
        let rt = runtime();
        let modal = opened(cx, &rt, || Err(outage()));
        settled(cx, &rt);

        let failure = modal.read_with(cx, |modal, _| modal.actions.failure());

        assert_eq!(failure, Some(shown_in_english(outage())));
    }
}
