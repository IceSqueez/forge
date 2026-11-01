use super::*;
use crate::app::async_bridge;
use crate::shared::repo_failure::RepoFailure;
use forge_components::NAME_FIELD_LIMIT;
use forge_components::PushToast;
use forge_components::focus_inside_modal;
use forge_components::{
    ActivePresentation, ChipGlyph, Density, FONT_XS, FONT_XXS, ForgePalette, InputEvent, ModalSize,
    OverlayPosition, Spacing, TextArea, TextInput, ToastKind, chip, modal, overlay, primary_button,
    secondary_button, spacing, status_dot, toggle, tr,
};
use forge_storage::QueueRepo;
use forge_types::{DEFAULT_QUEUE_ID, ExecutionMode, Queue};
use gpui::{AnyElement, App, ClickEvent, Context, Pixels, Rgba, SharedString, Window, div, px};
use std::sync::Arc;

const DESCRIPTION_HEIGHT: Pixels = px(72.0);
const GROUP_DOT: Pixels = px(8.0);

fn modal_section(
    palette: &ForgePalette,
    label: impl Into<SharedString>,
    control: impl IntoElement,
    density: Density,
    cx: &App,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xxs, density))
        .child(
            div()
                .font_family(cx.mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(label.into()),
        )
        .child(control)
}

pub(super) enum ActionFormEvent {
    Saved(ActionId, Box<ActionWrite>),
    Cancelled,
}

pub(super) struct ActionFormModal {
    base: Option<Action>,
    name: Entity<TextInput>,
    group: Entity<TextInput>,
    description: Entity<TextArea>,
    queues: Vec<(QueueId, SharedString)>,
    selected_queue: usize,
    preselect_queue: Option<QueueId>,
    enabled: bool,
    concurrent: bool,
    bypass_pause: bool,
    random_pick: bool,
    claims_focus: bool,
    _name_sub: Subscription,
}

impl EventEmitter<ActionFormEvent> for ActionFormModal {}

impl Render for ActionFormModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if std::mem::take(&mut self.claims_focus) {
            let name = self.name.clone();
            focus_inside_modal(window, cx, |window, cx| {
                name.update(cx, |field, cx| field.focus(window, cx))
            });
        }
        let palette = cx.palette();
        self.render_form(&palette, cx)
    }
}

impl ActionFormModal {
    pub(super) fn new(
        base: Option<Action>,
        queue_repo: Arc<dyn QueueRepo>,
        rt_handle: &tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
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
            let mut input = TextInput::new(tr!("actions_name_placeholder"), cx);
            input.set_content(seed_name, cx);
            input
        });
        let group = cx.new(|cx| {
            let mut input = TextInput::new(tr!("actions_group_placeholder"), cx);
            input.set_content(seed_group, cx);
            input
        });
        let description = cx.new(|cx| {
            let mut area = TextArea::new(tr!("actions_description_placeholder"), cx)
                .with_height(DESCRIPTION_HEIGHT);
            area.set_content(seed_desc, cx);
            area
        });
        let name_sub = cx.subscribe(&name, |_this, _f, _e: &InputEvent, cx| cx.notify());
        let (enabled, concurrent, bypass_pause, random_pick, preselect_queue) = match &base {
            Some(a) => (
                a.enabled,
                a.concurrent,
                a.bypass_pause,
                a.execution_mode == ExecutionMode::RandomPick,
                Some(a.queue_id),
            ),
            None => (true, false, false, false, None),
        };
        async_bridge::run_async(
            rt_handle,
            async move { queue_repo.list().await.map_err(RepoFailure::from) },
            |this, result, cx| match result {
                Ok(queues) => this.apply_queue_options(queues, cx),
                Err(failure) => on_repo_error(&failure, cx),
            },
            cx,
        );
        Self {
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
            claims_focus: true,
            _name_sub: name_sub,
        }
    }

    pub(super) fn ready(&self, cx: &App) -> bool {
        !self.name.read(cx).content().trim().is_empty() && !self.queues.is_empty()
    }

    fn apply_queue_options(&mut self, queues: Vec<Queue>, cx: &mut Context<Self>) {
        self.queues = queues
            .into_iter()
            .map(|q| (q.id, SharedString::from(q.name)))
            .collect();
        let target = self.preselect_queue.unwrap_or(DEFAULT_QUEUE_ID);
        self.selected_queue = self
            .queues
            .iter()
            .position(|(qid, _)| *qid == target)
            .unwrap_or(0);
        cx.notify();
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(ActionFormEvent::Cancelled);
    }

    fn select_queue(&mut self, index: usize, cx: &mut Context<Self>) {
        self.selected_queue = index;
        cx.notify();
    }

    fn toggle(&mut self, flag: impl FnOnce(&mut Self) -> &mut bool, cx: &mut Context<Self>) {
        let flag = flag(self);
        *flag = !*flag;
        cx.notify();
    }

    pub(super) fn submit(&mut self, cx: &mut Context<Self>) {
        let name = self.name.read(cx).content().trim().to_owned();
        if name.is_empty() {
            return;
        }
        let Some(&(queue_id, _)) = self.queues.get(self.selected_queue) else {
            cx.push_toast(ToastKind::Error, tr!("actions_no_queue"));
            return;
        };
        let group_name = self.group.read(cx).content().trim().to_owned();
        let description = self.description.read(cx).content().trim().to_owned();
        let execution_mode = if self.random_pick {
            ExecutionMode::RandomPick
        } else {
            ExecutionMode::Sequential
        };
        let group = (!group_name.is_empty()).then_some(group_name);
        let description = (!description.is_empty()).then_some(description);
        let (enabled, concurrent, bypass_pause) =
            (self.enabled, self.concurrent, self.bypass_pause);
        let saved = match self.base.as_ref().map(|existing| existing.id) {
            Some(id) => ActionFormEvent::Saved(
                id,
                Box::new(ActionWrite::Settings(ActionSettings {
                    name,
                    group,
                    queue_id,
                    enabled,
                    concurrent,
                    bypass_pause,
                    execution_mode,
                    description,
                })),
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
                ActionFormEvent::Saved(action.id, Box::new(ActionWrite::Create(action)))
            }
        };
        cx.emit(saved);
    }

    fn render_form(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let two_col = div()
            .flex()
            .gap(spacing(Spacing::Sm, density))
            .child(
                div()
                    .flex_1()
                    .child(self.group_section(palette, density, cx)),
            )
            .child(
                div()
                    .flex_1()
                    .child(self.queue_section(palette, density, cx)),
            );

        let desc_section = modal_section(
            palette,
            tr!("actions_modal_section_description"),
            div().child(self.description.clone()),
            density,
            cx,
        )
        .into_any_element();

        let behavior_header = div()
            .font_family(cx.mono_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(SharedString::from(tr!("actions_modal_section_behavior")));

        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(self.name_section(palette, density, cx))
            .child(two_col)
            .child(desc_section)
            .child(behavior_header)
            .children(self.behavior_rows(palette, density, cx));

        let modal_title = if self.base.is_some() {
            tr!("actions_modal_edit_action_title")
        } else {
            tr!("actions_modal_new_action_title")
        };
        let card = modal(modal_title, body, palette)
            .size(ModalSize::Md)
            .footer(self.modal_footer(palette, density, cx))
            .on_close(
                "actions-modal-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
            );

        let view = cx.entity();
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(
                overlay(card, palette)
                    .position(OverlayPosition::Center)
                    .on_dismiss("actions-modal-scrim", move |_window, cx| {
                        view.update(cx, |this, cx| this.cancel(cx));
                    }),
            )
            .into_any_element()
    }

    fn name_section(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name_len = self
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
            .child(div().flex_1().child(self.name.clone()))
            .child(
                div()
                    .font_family(cx.mono_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(format!("{name_len}/{NAME_FIELD_LIMIT}")),
            );
        modal_section(
            palette,
            tr!("actions_modal_section_name"),
            name_row,
            density,
            cx,
        )
        .into_any_element()
    }

    fn group_section(&self, palette: &ForgePalette, density: Density, cx: &App) -> AnyElement {
        let group_field = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(status_dot(palette.brand, GROUP_DOT))
            .child(div().flex_1().child(self.group.clone()));
        modal_section(
            palette,
            tr!("actions_modal_section_group"),
            group_field,
            density,
            cx,
        )
        .into_any_element()
    }

    fn queue_section(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let queue_control: AnyElement = if self.queues.is_empty() {
            div()
                .font_family(cx.body_family())
                .text_size(FONT_XS)
                .text_color(palette.text_faint)
                .child(SharedString::from(tr!("actions_loading_queues")))
                .into_any_element()
        } else {
            let mut queue_chips = div().flex().flex_wrap().gap(spacing(Spacing::Xxs, density));
            for (i, (_, name)) in self.queues.iter().enumerate() {
                queue_chips = queue_chips.child(
                    chip(
                        name.clone(),
                        ChipGlyph::Dot(palette.brand),
                        self.selected_queue == i,
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
            cx,
        )
        .into_any_element()
    }

    fn behavior_rows(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> [AnyElement; 4] {
        [
            Self::modal_toggle_row(
                tr!("actions_modal_enabled_label"),
                tr!("actions_modal_enabled_desc"),
                self.enabled,
                palette.success,
                "actions-modal-enabled",
                cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.toggle(|form| &mut form.enabled, cx)
                }),
                palette,
                density,
                cx,
            ),
            Self::modal_toggle_row(
                tr!("actions_modal_concurrent_label"),
                tr!("actions_modal_concurrent_desc"),
                self.concurrent,
                palette.info,
                "actions-modal-concurrent",
                cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.toggle(|form| &mut form.concurrent, cx)
                }),
                palette,
                density,
                cx,
            ),
            Self::modal_toggle_row(
                tr!("actions_modal_bypass_label"),
                tr!("actions_modal_bypass_desc"),
                self.bypass_pause,
                palette.warning,
                "actions-modal-bypass",
                cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.toggle(|form| &mut form.bypass_pause, cx)
                }),
                palette,
                density,
                cx,
            ),
            Self::modal_toggle_row(
                tr!("actions_modal_random_pick_label"),
                tr!("actions_modal_random_pick_desc"),
                self.random_pick,
                palette.danger,
                "actions-modal-random",
                cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.toggle(|form| &mut form.random_pick, cx)
                }),
                palette,
                density,
                cx,
            ),
        ]
    }

    fn modal_footer(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let valid = self.ready(cx);
        let cancel = secondary_button(tr!("actions_modal_cancel_btn"), palette).on_click(
            "actions-modal-cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
        );
        let submit_label = if self.base.is_some() {
            tr!("actions_modal_save_btn")
        } else {
            tr!("actions_modal_create_btn")
        };
        let create = primary_button(submit_label, palette)
            .disabled(!valid)
            .on_click(
                "actions-modal-create",
                cx.listener(|this, _: &ClickEvent, _, cx| this.submit(cx)),
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
        cx: &App,
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
                            .font_family(cx.body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_primary)
                            .child(label.into()),
                    )
                    .child(
                        div()
                            .font_family(cx.body_family())
                            .text_size(FONT_XXS)
                            .text_color(palette.text_muted)
                            .child(description.into()),
                    ),
            )
            .child(toggle(on, palette).on_color(accent).on_click(id, handler))
            .into_any_element()
    }
}

#[cfg(test)]
impl ActionFormModal {
    pub(super) fn fill(&mut self, name: String, with_queue: bool, cx: &mut Context<Self>) {
        self.name
            .update(cx, |input, cx| input.set_content(name, cx));
        if with_queue {
            self.queues = vec![(QueueId::new(), "Default".into())];
        }
    }

    pub(super) fn editing_id(&self) -> Option<ActionId> {
        self.base.as_ref().map(|action| action.id)
    }

    pub(super) fn name_text(&self, cx: &App) -> String {
        self.name.read(cx).content().to_owned()
    }
}
