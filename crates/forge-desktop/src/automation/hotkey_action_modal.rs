use forge_components::Density;
use forge_components::FONT_LABEL;
use forge_components::FONT_META;
use forge_components::Spacing;
use forge_components::focus_inside_modal;
use forge_components::spacing;
use std::sync::Arc;

use crate::automation::chord_capture::{ChordCapture, chord_capture_field};

use forge_components::OverflowRevealingFocus;
use forge_components::{
    ActivePresentation, BORDER_THIN, FONT_SECTION_LABEL, ForgePalette, Icon, OverlayPosition,
    modal, overlay, primary_button, scaled, secondary_button, segment, segmented, tr,
};
use forge_storage::ActionRepo;
use forge_types::{ActionId, TriggerInstanceId};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, EventEmitter, Pixels, SharedString, Subscription,
    Window, div, prelude::*, px,
};

use crate::automation::action_picker::{ActionPicker, ActionPickerCopy, ActionPickerEvent};
use forge_hotkey::{HotkeyEdge, combo_keys};

const MODAL_W: Pixels = px(520.0);
const BODY_PAD_H: Pixels = px(18.0);
const BODY_MAX_H: Pixels = px(460.0);

const SECTION_LABEL_MB: Pixels = px(8.0);

const KEYCAP_FS: Pixels = FONT_LABEL;
const KEYCAP_RADIUS: Pixels = px(5.0);
const KEYCAP_PAD_V: Pixels = px(3.0);
const KEYCAP_PAD_H: Pixels = px(8.0);
const KEYCAP_MIN_W: Pixels = px(24.0);

const FILTER_HEADER_GAP: Pixels = px(12.0);

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

pub(crate) struct HotkeyActionModal {
    instance_id: Option<TriggerInstanceId>,
    combo: String,
    edge: HotkeyEdge,
    locked_edge: Option<HotkeyEdge>,
    capturing: bool,
    picker: Entity<ActionPicker>,
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
        let picker = cx.new(|cx| {
            ActionPicker::new(
                ActionPickerCopy {
                    id_prefix: "hotkeys-modal",
                    section: tr!("hotkeys_modal_section_action"),
                    filter: tr!("hotkeys_modal_filter_actions"),
                    loading: tr!("hotkeys_modal_actions_loading"),
                    none: tr!("hotkeys_modal_actions_none"),
                    no_match: tr!("hotkeys_modal_actions_empty"),
                    failed: |reason| tr!("hotkeys_toast_error", message = reason),
                },
                linked_action,
                action_repo,
                &rt_handle,
                cx,
            )
        });
        let subs = vec![cx.subscribe(&picker, Self::on_picker_event)];

        Self {
            instance_id,
            combo,
            edge,
            locked_edge,
            capturing: false,
            picker,
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

    fn on_picker_event(
        &mut self,
        _picker: Entity<ActionPicker>,
        event: &ActionPickerEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            ActionPickerEvent::SelectionChanged => cx.notify(),
            ActionPickerEvent::Confirmed => self.save(cx),
            ActionPickerEvent::Cancelled => self.cancel(cx),
        }
    }

    fn select_edge(&mut self, edge: HotkeyEdge, cx: &mut Context<Self>) {
        if self.locked_edge == Some(edge) || self.edge == edge {
            return;
        }
        self.edge = edge;
        cx.notify();
    }

    fn recapture(&mut self, cx: &mut Context<Self>) {
        self.capturing = true;
        cx.emit(HotkeyActionModalEvent::Recapture);
        cx.notify();
    }

    fn selected_action(&self, cx: &App) -> Option<ActionId> {
        self.picker.read(cx).selected()
    }

    fn can_save(&self, cx: &App) -> bool {
        self.selected_action(cx).is_some() && !self.combo.is_empty() && !self.capturing
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let Some(action_id) = self.selected_action(cx).filter(|_| self.can_save(cx)) else {
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
        chord_capture_field(
            ChordCapture {
                capturing: self.capturing,
                caps: Some(&self.combo),
                recapture_id: "hotkeys-modal-recapture",
            },
            cx.listener(|this, _: &ClickEvent, _, cx| this.recapture(cx)),
            palette,
            density,
            cx,
        )
        .mb(spacing(Spacing::Md, density))
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
                .font_family(cx.body_family())
                .text_size(FOOTER_HINT_FS)
                .text_color(palette.text_faint)
                .child(hint)
        });

        div()
            .w_full()
            .flex()
            .flex_col()
            .mb(spacing(Spacing::Md, density))
            .child(section_caption(
                &tr!("hotkeys_modal_section_edge"),
                palette,
                cx,
            ))
            .child(div().flex().items_center().child(selector))
            .children(hints)
            .into_any_element()
    }

    fn render_footer(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let hint = if self.can_save(cx) {
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
                    .font_family(cx.body_family())
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
                            .disabled(!self.can_save(cx))
                            .on_click(
                                "hotkeys-modal-save",
                                cx.listener(|this, _: &ClickEvent, _, cx| this.save(cx)),
                            ),
                    ),
            )
            .into_any_element()
    }
}

pub(crate) fn keycaps(
    combo: &str,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> impl IntoElement {
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
                .font_family(cx.mono_family())
                .text_size(KEYCAP_FS)
                .text_color(palette.text_primary)
                .child(key.to_owned()),
        );
    }
    row
}

pub(crate) fn section_caption(label: &str, palette: &ForgePalette, cx: &App) -> impl IntoElement {
    div()
        .w_full()
        .mb(SECTION_LABEL_MB)
        .font_family(cx.mono_family())
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
                self.picker
                    .update(cx, |picker, cx| picker.focus_filter(window, cx))
            });
        }

        let body = div()
            .id("hotkeys-modal-body")
            .w_full()
            .max_h(BODY_MAX_H)
            .min_h_0()
            .overflow_y_revealing_focus()
            .py(spacing(Spacing::Md, density))
            .px(scaled(BODY_PAD_H, density))
            .flex()
            .flex_col()
            .child(self.render_combo(&palette, cx))
            .child(self.render_edge(&palette, cx))
            .child(self.picker.clone());

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
            .body_scrolls_itself()
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
    use gpui::{Entity, TestAppContext, VisualTestContext};
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;
    use crate::app::async_bridge::Loadable;
    use crate::app::i18n::install_language;
    use crate::test_support::{
        action_running, install_presentation, open_under_root, pump, runtime, settle_focus,
        shown_in_english,
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

    fn keyed<'a>(
        cx: &'a mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        combo: &str,
    ) -> (Entity<HotkeyActionModal>, &'a mut VisualTestContext) {
        install_language(Language::En);
        install_presentation(cx);
        let mut repo = MockActionRepo::new();
        repo.expect_list().returning(|| {
            Ok(["first", "second"]
                .map(|name| action_running(name, Vec::new()))
                .to_vec())
        });
        let launch = ActionModalLaunch {
            instance_id: None,
            combo: combo.to_owned(),
            edge: HotkeyEdge::Press,
            locked_edge: None,
            linked_action: None,
        };
        let handle = rt.handle().clone();
        let (modal, vcx) = open_under_root(cx, |cx| {
            cx.new(|cx| HotkeyActionModal::new(launch, Arc::new(repo), handle, cx))
        });
        pump(rt);
        settle_focus(vcx);
        (modal, vcx)
    }

    fn selected_name(vcx: &mut VisualTestContext, modal: &Entity<HotkeyActionModal>) -> String {
        modal.read_with(vcx, |modal, cx| {
            let picker = modal.picker.read(cx);
            picker
                .actions
                .loaded()
                .and_then(|actions| {
                    actions
                        .iter()
                        .find(|choice| Some(choice.id) == picker.selected())
                })
                .map(|choice| choice.name.clone())
                .unwrap_or_default()
        })
    }

    #[gpui::test]
    fn arrows_in_the_filter_pick_the_action_and_a_second_enter_saves(cx: &mut TestAppContext) {
        let rt = runtime();
        let (modal, vcx) = keyed(cx, &rt, "ctrl+k");
        let saved = Rc::new(Cell::new(0));
        let heard = Rc::clone(&saved);
        vcx.update(|_, cx| {
            cx.subscribe(&modal, move |_, event: &HotkeyActionModalEvent, _| {
                if matches!(event, HotkeyActionModalEvent::Save(_)) {
                    heard.set(heard.get() + 1);
                }
            })
            .detach();
        });

        vcx.simulate_keystrokes("down down enter");
        assert_eq!(selected_name(vcx, &modal), "second");
        assert_eq!(saved.get(), 0);

        vcx.simulate_keystrokes("enter");
        assert_eq!(saved.get(), 1);
    }

    #[gpui::test]
    fn typing_a_space_in_the_filter_still_types_it(cx: &mut TestAppContext) {
        let rt = runtime();
        let (modal, vcx) = keyed(cx, &rt, "ctrl+k");

        vcx.simulate_input("a b");

        let typed = modal.read_with(vcx, |modal, cx| {
            modal
                .picker
                .read(cx)
                .filter
                .field()
                .read(cx)
                .content()
                .to_owned()
        });
        assert_eq!(typed, "a b");
    }

    #[gpui::test]
    fn a_click_on_an_action_moves_the_cursor_to_it(cx: &mut TestAppContext) {
        let rt = runtime();
        let (modal, vcx) = keyed(cx, &rt, "ctrl+k");
        let first = modal.read_with(vcx, |modal, cx| {
            modal.picker.read(cx).matched_action_ids()[0]
        });

        modal.update(vcx, |modal, cx| {
            modal
                .picker
                .update(cx, |picker, cx| picker.select_action(first, cx))
        });
        vcx.simulate_keystrokes("down enter");

        assert_eq!(selected_name(vcx, &modal), "second");
    }

    #[gpui::test]
    fn the_list_is_loading_until_the_repository_answers(cx: &mut TestAppContext) {
        let rt = runtime();
        let modal = opened(cx, &rt, || Ok(Vec::new()));

        let before = modal.read_with(cx, |modal, cx| {
            matches!(modal.picker.read(cx).actions, Loadable::Loading)
        });
        settled(cx, &rt);
        let after = modal.read_with(
            cx,
            |modal, cx| matches!(&modal.picker.read(cx).actions, Loadable::Loaded(listed) if listed.is_empty()),
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

        let names: Vec<String> = modal.read_with(cx, |modal, cx| {
            modal
                .picker
                .read(cx)
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

        let failure = modal.read_with(cx, |modal, cx| modal.picker.read(cx).actions.failure());

        assert_eq!(failure, Some(shown_in_english(outage())));
    }
}
