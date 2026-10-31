use super::action_writes::{ActionSettings, ActionWrite};
use super::*;
use crate::async_bridge;
use crate::repo_failure::RepoFailure;
use forge_components::ActivePresentation;
use forge_components::NAME_FIELD_LIMIT;
use forge_components::PushToast;
use forge_components::focus_inside_modal;
use forge_components::{
    ChipGlyph, Density, FONT_XS, FONT_XXS, ForgePalette, InputEvent, ModalSize, OverlayPosition,
    Spacing, TextArea, TextInput, ToastKind, body_family, chip, modal, mono_family, overlay,
    primary_button, secondary_button, spacing, status_dot, toggle, tr,
};
use forge_types::{Action, ActionId, DEFAULT_QUEUE_ID, ExecutionMode, Queue};
use gpui::{AnyElement, ClickEvent, Context, Pixels, Rgba, SharedString, Window, div, px};
use std::sync::Arc;

const DESCRIPTION_HEIGHT: Pixels = px(72.0);

fn modal_section(
    palette: &ForgePalette,
    label: impl Into<SharedString>,
    control: impl IntoElement,
    density: Density,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xxs, density))
        .child(
            div()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(label.into()),
        )
        .child(control)
}

impl ScreenActionsView {
    pub(super) fn open_action_modal(
        &mut self,
        base: Option<Action>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let palette = cx.palette();
        let seed_name = base.as_ref().map(|a| a.name.clone()).unwrap_or_default();
        let seed_group = base
            .as_ref()
            .and_then(|a| a.group.clone())
            .unwrap_or_default();
        let seed_desc = base
            .as_ref()
            .and_then(|a| a.description.clone())
            .unwrap_or_default();
        let name = cx.new(|cx| {
            let mut input =
                TextInput::new(tr!("actions_name_placeholder"), cx).with_palette(palette);
            input.set_content(seed_name, cx);
            input
        });
        let group = cx.new(|cx| {
            let mut input =
                TextInput::new(tr!("actions_group_placeholder"), cx).with_palette(palette);
            input.set_content(seed_group, cx);
            input
        });
        let description = cx.new(|cx| {
            let mut area = TextArea::new(tr!("actions_description_placeholder"), cx)
                .with_palette(palette)
                .with_height(DESCRIPTION_HEIGHT);
            area.set_content(seed_desc, cx);
            area
        });
        focus_inside_modal(window, cx, |window, cx| {
            name.update(cx, |f, cx| f.focus(window, cx))
        });
        let name_sub = cx.subscribe(&name, |_this, _f, _e: &InputEvent, cx| cx.notify());
        let (editing, enabled, concurrent, bypass_pause, random_pick, preselect_queue) = match &base
        {
            Some(a) => (
                Some(a.id),
                a.enabled,
                a.concurrent,
                a.bypass_pause,
                a.execution_mode == ExecutionMode::RandomPick,
                Some(a.queue_id),
            ),
            None => (None, true, false, false, false, None),
        };
        self.modals.action_form = Some(ActionForm {
            editing,
            base,
            name,
            group,
            description,
            queues: Vec::new(),
            selected_queue: 0,
            preselect_queue,
            enabled,
            concurrent,
            bypass_pause,
            random_pick,
            _name_sub: name_sub,
        });
        cx.notify();

        let repo = Arc::clone(&self.deps.queue_repo);
        async_bridge::run_async(
            &self.deps.rt_handle,
            async move { repo.list().await.map_err(RepoFailure::from) },
            |this, result, cx| match result {
                Ok(queues) => this.apply_queue_options(queues, cx),
                Err(failure) => this.on_repo_error(&failure, cx),
            },
            cx,
        );
    }

    fn apply_queue_options(&mut self, queues: Vec<Queue>, cx: &mut Context<Self>) {
        if let Some(form) = self.modals.action_form.as_mut() {
            form.queues = queues
                .into_iter()
                .map(|q| (q.id, SharedString::from(q.name)))
                .collect();
            let target = form.preselect_queue.unwrap_or(DEFAULT_QUEUE_ID);
            form.selected_queue = form
                .queues
                .iter()
                .position(|(qid, _)| *qid == target)
                .unwrap_or(0);
            cx.notify();
        }
    }

    fn cancel_action_modal(&mut self, cx: &mut Context<Self>) {
        self.modals.action_form = None;
        cx.notify();
    }

    fn select_queue(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(form) = self.modals.action_form.as_mut() {
            form.selected_queue = index;
            cx.notify();
        }
    }

    fn toggle_modal_enabled(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = self.modals.action_form.as_mut() {
            form.enabled = !form.enabled;
            cx.notify();
        }
    }

    fn toggle_modal_concurrent(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = self.modals.action_form.as_mut() {
            form.concurrent = !form.concurrent;
            cx.notify();
        }
    }

    fn toggle_modal_bypass(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = self.modals.action_form.as_mut() {
            form.bypass_pause = !form.bypass_pause;
            cx.notify();
        }
    }

    fn toggle_modal_random(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = self.modals.action_form.as_mut() {
            form.random_pick = !form.random_pick;
            cx.notify();
        }
    }

    pub(super) fn submit_action_modal(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self.modals.action_form.as_ref() else {
            return;
        };
        let name = form.name.read(cx).content().trim().to_owned();
        if name.is_empty() {
            return;
        }
        let Some(&(queue_id, _)) = form.queues.get(form.selected_queue) else {
            cx.push_toast(ToastKind::Error, tr!("actions_no_queue"));
            return;
        };
        let group_name = form.group.read(cx).content().trim().to_owned();
        let description = form.description.read(cx).content().trim().to_owned();
        let execution_mode = if form.random_pick {
            ExecutionMode::RandomPick
        } else {
            ExecutionMode::Sequential
        };
        let group = (!group_name.is_empty()).then_some(group_name);
        let description = (!description.is_empty()).then_some(description);
        let write_target = form.base.as_ref().map(|existing| existing.id);
        let (enabled, concurrent, bypass_pause) =
            (form.enabled, form.concurrent, form.bypass_pause);
        self.modals.action_form = None;
        match write_target {
            Some(id) => self.write_action(
                id,
                ActionWrite::Settings(ActionSettings {
                    name,
                    group,
                    queue_id,
                    enabled,
                    concurrent,
                    bypass_pause,
                    execution_mode,
                    description,
                }),
                cx,
            ),
            None => {
                let action = Action {
                    id: ActionId::new(),
                    name,
                    group,
                    queue_id,
                    enabled,
                    concurrent,
                    bypass_pause,
                    execution_mode,
                    description,
                    sub_actions: Vec::new(),
                };
                self.write_action(action.id, ActionWrite::Create(action), cx);
            }
        }
    }

    pub(super) fn render_action_modal(
        &self,
        form: &ActionForm,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let two_col = div()
            .flex()
            .gap(spacing(Spacing::Sm, density))
            .child(
                div()
                    .flex_1()
                    .child(Self::group_section(form, palette, density)),
            )
            .child(
                div()
                    .flex_1()
                    .child(Self::queue_section(form, palette, density, cx)),
            );

        let desc_section = modal_section(
            palette,
            tr!("actions_modal_section_description"),
            div().child(form.description.clone()),
            density,
        );

        let behavior_header = div()
            .font_family(mono_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(SharedString::from(tr!("actions_modal_section_behavior")));

        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(Self::name_section(form, palette, density, cx))
            .child(two_col)
            .child(desc_section)
            .child(behavior_header)
            .children(Self::behavior_rows(form, palette, density, cx));

        let modal_title = if form.editing.is_some() {
            tr!("actions_modal_edit_action_title")
        } else {
            tr!("actions_modal_new_action_title")
        };
        let card = modal(modal_title, body, palette)
            .size(ModalSize::Md)
            .footer(Self::modal_footer(form, palette, density, cx))
            .on_close(
                "actions-modal-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_action_modal(cx)),
            );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("actions-modal-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_action_modal(cx));
            })
            .into_any_element()
    }

    fn name_section(
        form: &ActionForm,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name_len = form
            .name
            .read(cx)
            .content()
            .chars()
            .count()
            .min(NAME_FIELD_LIMIT);
        let name_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(div().flex_1().child(form.name.clone()))
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(format!("{name_len}/{NAME_FIELD_LIMIT}")),
            );
        modal_section(
            palette,
            tr!("actions_modal_section_name"),
            name_row,
            density,
        )
        .into_any_element()
    }

    fn group_section(form: &ActionForm, palette: &ForgePalette, density: Density) -> AnyElement {
        let group_field = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(status_dot(palette.brand, GROUP_DOT))
            .child(div().flex_1().child(form.group.clone()));
        modal_section(
            palette,
            tr!("actions_modal_section_group"),
            group_field,
            density,
        )
        .into_any_element()
    }

    fn queue_section(
        form: &ActionForm,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let queue_control: AnyElement = if form.queues.is_empty() {
            div()
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(palette.text_faint)
                .child(SharedString::from(tr!("actions_loading_queues")))
                .into_any_element()
        } else {
            let mut queue_chips = div().flex().flex_wrap().gap(spacing(Spacing::Xxs, density));
            for (i, (_, name)) in form.queues.iter().enumerate() {
                queue_chips = queue_chips.child(
                    chip(
                        name.clone(),
                        ChipGlyph::Dot(palette.brand),
                        form.selected_queue == i,
                        palette,
                    )
                    .on_click(
                        SharedString::from(format!("actions-queue-{i}")),
                        cx.listener(move |this, _: &ClickEvent, _, cx| this.select_queue(i, cx)),
                    ),
                );
            }
            queue_chips.into_any_element()
        };
        modal_section(
            palette,
            tr!("actions_modal_section_queue"),
            queue_control,
            density,
        )
        .into_any_element()
    }

    fn behavior_rows(
        form: &ActionForm,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> [AnyElement; 4] {
        [
            Self::modal_toggle_row(
                tr!("actions_modal_enabled_label"),
                tr!("actions_modal_enabled_desc"),
                form.enabled,
                palette.success,
                "actions-modal-enabled",
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_modal_enabled(cx)),
                palette,
                density,
            ),
            Self::modal_toggle_row(
                tr!("actions_modal_concurrent_label"),
                tr!("actions_modal_concurrent_desc"),
                form.concurrent,
                palette.info,
                "actions-modal-concurrent",
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_modal_concurrent(cx)),
                palette,
                density,
            ),
            Self::modal_toggle_row(
                tr!("actions_modal_bypass_label"),
                tr!("actions_modal_bypass_desc"),
                form.bypass_pause,
                palette.warning,
                "actions-modal-bypass",
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_modal_bypass(cx)),
                palette,
                density,
            ),
            Self::modal_toggle_row(
                tr!("actions_modal_random_pick_label"),
                tr!("actions_modal_random_pick_desc"),
                form.random_pick,
                palette.danger,
                "actions-modal-random",
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_modal_random(cx)),
                palette,
                density,
            ),
        ]
    }

    fn modal_footer(
        form: &ActionForm,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let valid = form.ready(cx);
        let cancel = secondary_button(tr!("actions_modal_cancel_btn"), palette).on_click(
            "actions-modal-cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_action_modal(cx)),
        );
        let submit_label = if form.editing.is_some() {
            tr!("actions_modal_save_btn")
        } else {
            tr!("actions_modal_create_btn")
        };
        let create = primary_button(submit_label, palette)
            .disabled(!valid)
            .on_click(
                "actions-modal-create",
                cx.listener(|this, _: &ClickEvent, _, cx| this.submit_action_modal(cx)),
            );
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_end()
            .gap(spacing(Spacing::Xs, density))
            .child(cancel)
            .child(create)
            .into_any_element()
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "each argument is one input of the toggle row plus the active density"
    )]
    fn modal_toggle_row(
        label: impl Into<SharedString>,
        description: impl Into<SharedString>,
        on: bool,
        accent: Rgba,
        id: &'static str,
        handler: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(spacing(Spacing::Sm, density))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_primary)
                            .child(label.into()),
                    )
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_XXS)
                            .text_color(palette.text_muted)
                            .child(description.into()),
                    ),
            )
            .child(toggle(on, palette).on_color(accent).on_click(id, handler))
            .into_any_element()
    }
}
