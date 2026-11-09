use forge_components::{
    ActivePresentation, BORDER_THIN, Button, Density, FONT_XS, FONT_XXS, ForgePalette, Icon,
    ModalSize, OverlayPosition, Radius, Spacing, TINT_STRONG_FILL_ALPHA, badge, choice_trigger,
    dropdown, empty_state, field_column, ghost_button_with_icon, icon, modal, overlay,
    primary_button, radius, scaled, secondary_button, section_label, segment, segmented, spacing,
    tr, with_alpha,
};
use forge_types::ActionId;
use gpui::{AnyElement, App, ClickEvent, Context, Pixels, Rgba, SharedString, div, prelude::*, px};

use super::form::{Popover, TargetMode, WiringForm, WordingField};
use super::plan::{ActionRecord, TriggerRecord, WiringDraft};
use super::{ConfirmStage, EventWiringView, Preview};

const CARD_PAD_V: Pixels = px(14.0);
const CARD_GAP: Pixels = px(14.0);
const ROW_LABEL_GAP: Pixels = px(8.0);
const RECORDS_PAD: Pixels = px(12.0);
const RECORD_LABEL_W: Pixels = px(64.0);
const NOTE_GLYPH: Pixels = px(12.0);
const TAG_FS: Pixels = FONT_XXS;

#[derive(Clone, Copy)]
enum FooterAction {
    Create { enabled: bool },
    Open(ActionId),
}

impl EventWiringView {
    pub(super) fn render_confirm(
        &self,
        stage: &ConfirmStage,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let overlay_name = self
            .overlay
            .as_ref()
            .map(|definition| definition.display_name.clone())
            .unwrap_or_default();

        let footer = match &stage.preview {
            Preview::AlreadyWired { action_id } => {
                self.render_footer(FooterAction::Open(*action_id), palette, cx)
            }
            Preview::Ready(_) => self.render_footer(
                FooterAction::Create {
                    enabled: !stage.writing && !stage.planning,
                },
                palette,
                cx,
            ),
            Preview::Checking | Preview::Prompt(_) | Preview::Refused(_) => {
                self.render_footer(FooterAction::Create { enabled: false }, palette, cx)
            }
        };

        let body = div()
            .w_full()
            .px(spacing(Spacing::Md, density))
            .py(scaled(CARD_PAD_V, density))
            .flex()
            .flex_col()
            .gap(scaled(CARD_GAP, density))
            .child(self.render_target(&stage.form, palette, density, cx))
            .child(self.render_wording(&stage.form, palette, density, cx))
            .child(self.render_preview(stage, &overlay_name, palette, density, cx));

        let card = modal(
            tr!(
                "overlays_wire_confirm_title",
                overlay = overlay_name.as_str()
            ),
            body,
            palette,
        )
        .header_icon(Icon::Bolt, palette.brand)
        .subtitle(stage.trigger_label.clone())
        .size(ModalSize::Md)
        .flush_body()
        .footer(footer)
        .on_close(
            "overlays-wire-close",
            cx.listener(|this, _: &ClickEvent, _, cx| this.close(cx)),
        );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .busy(stage.writing)
            .on_dismiss("overlays-wire-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.close(cx));
            })
            .into_any_element()
    }

    fn render_target(
        &self,
        form: &WiringForm,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let modes = segmented(
            vec![
                segment(
                    "overlays-wire-mode-new",
                    tr!("overlays_wire_mode_new"),
                    form.mode == TargetMode::New,
                    cx.listener(|this, _: &ClickEvent, _, cx| this.set_mode(TargetMode::New, cx)),
                ),
                segment(
                    "overlays-wire-mode-existing",
                    tr!("overlays_wire_mode_existing"),
                    form.mode == TargetMode::Existing,
                    cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.set_mode(TargetMode::Existing, cx)
                    }),
                ),
            ],
            palette,
        );

        let mut section = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(section_label(
                tr!("overlays_wire_section_action").to_uppercase(),
                palette,
                cx,
            ))
            .child(modes);

        section = match form.mode {
            TargetMode::New => {
                let queue = form
                    .queue_label()
                    .unwrap_or_else(|| tr!("overlays_wire_queue_none"));
                let queue_trigger = choice_trigger(
                    "overlays-wire-queue",
                    queue.into(),
                    true,
                    palette,
                    cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.open_queue_picker(window, cx)
                    }),
                    density,
                    cx,
                );
                let queue_popover = match &form.popover {
                    Some(Popover::Queue { picker, .. }) => Some(picker.clone().into_any_element()),
                    _ => None,
                };
                section
                    .child(field_column(
                        palette,
                        &tr!("overlays_wire_field_name"),
                        None,
                        None,
                        form.name.clone().into_any_element(),
                        density,
                        cx,
                    ))
                    .child(field_column(
                        palette,
                        &tr!("overlays_wire_field_queue"),
                        Some(&tr!("overlays_wire_queue_hint")),
                        None,
                        self.with_popover(queue_trigger, queue_popover, cx),
                        density,
                        cx,
                    ))
            }
            TargetMode::Existing => {
                let picked = form.existing_label();
                let has_pick = picked.is_some();
                let action_trigger = choice_trigger(
                    "overlays-wire-existing",
                    picked
                        .unwrap_or_else(|| tr!("overlays_wire_action_placeholder"))
                        .into(),
                    has_pick,
                    palette,
                    cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.open_action_picker(window, cx)
                    }),
                    density,
                    cx,
                );
                let action_popover = match &form.popover {
                    Some(Popover::Action { picker, .. }) => Some(picker.clone().into_any_element()),
                    _ => None,
                };
                section.child(field_column(
                    palette,
                    &tr!("overlays_wire_field_action"),
                    Some(&tr!("overlays_wire_existing_hint")),
                    None,
                    self.with_popover(action_trigger, action_popover, cx),
                    density,
                    cx,
                ))
            }
        };
        section.into_any_element()
    }

    fn render_wording(
        &self,
        form: &WiringForm,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut section = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(section_label(
                tr!("overlays_wire_section_wording").to_uppercase(),
                palette,
                cx,
            ));
        if form.wording.is_empty() {
            return section
                .child(
                    div()
                        .font_family(cx.body_family())
                        .text_size(FONT_XS)
                        .text_color(palette.text_faint)
                        .child(tr!("overlays_wire_wording_empty")),
                )
                .into_any_element();
        }
        for field in &form.wording {
            let control = self.render_wording_control(field, form, palette, density, cx);
            section = section.child(field_column(
                palette,
                &field.label,
                None,
                None,
                control,
                density,
                cx,
            ));
        }
        section.into_any_element()
    }

    fn render_wording_control(
        &self,
        field: &WordingField,
        form: &WiringForm,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = field.key.clone();
        let button =
            ghost_button_with_icon(Icon::Variable, tr!("actions_var_picker_open"), palette)
                .ink(palette.warning)
                .on_click(
                    SharedString::from(format!("overlays-wire-variable-{}", field.key)),
                    cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.open_variable_picker(key.clone(), window, cx)
                    }),
                );
        let row = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(div().flex_1().min_w(px(0.0)).child(field.input.clone()))
            .child(button)
            .into_any_element();
        let popover = match &form.popover {
            Some(Popover::Variable { key, picker, .. }) if *key == field.key => {
                Some(picker.clone().into_any_element())
            }
            _ => None,
        };
        self.with_popover(row, popover, cx)
    }

    fn with_popover(
        &self,
        anchor: AnyElement,
        popover: Option<AnyElement>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let popover = popover.map(|content| {
            let view = cx.entity();
            dropdown(content).on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_popover(cx));
            })
        });
        div()
            .relative()
            .w_full()
            .child(anchor)
            .children(popover)
            .into_any_element()
    }

    fn render_preview(
        &self,
        stage: &ConfirmStage,
        overlay_name: &str,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let heading = section_label(
            tr!("overlays_wire_section_preview").to_uppercase(),
            palette,
            cx,
        );
        let content = match &stage.preview {
            Preview::Checking => empty_state(tr!("overlays_wire_checking"), palette)
                .loading("overlays-wire-checking")
                .density(density.tighter())
                .into_any_element(),
            Preview::Prompt(message) => note_row(
                Icon::InfoCircle,
                palette.text_faint,
                message,
                palette.text_muted,
                density,
                cx,
            ),
            Preview::Refused(message) => note_row(
                Icon::AlertTriangle,
                palette.warning,
                message,
                palette.warning,
                density,
                cx,
            ),
            Preview::AlreadyWired { action_id } => {
                render_already_wired(&self.action_name(*action_id), palette, density, cx)
            }
            Preview::Ready(draft) => {
                render_records(draft, &self.step_value(overlay_name), palette, density, cx)
            }
        };
        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(heading)
            .child(content)
            .into_any_element()
    }

    fn step_value(&self, overlay_name: &str) -> String {
        let step = self
            .handles
            .sub_actions
            .get(forge_runtime::OVERLAY_SEND_KIND_ID)
            .map_or_else(
                || forge_runtime::OVERLAY_SEND_KIND_ID.to_owned(),
                crate::shared::kind_copy::step_name,
            );
        tr!(
            "overlays_wire_record_step_value",
            step = step.as_str(),
            overlay = overlay_name
        )
    }

    fn render_footer(
        &self,
        action: FooterAction,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let back = ghost_button_with_icon(Icon::ArrowBackUp, tr!("overlays_wire_back"), palette)
            .on_click(
                "overlays-wire-back",
                cx.listener(|this, _: &ClickEvent, window, cx| this.back_to_picker(window, cx)),
            );
        let cancel = secondary_button(tr!("common_cancel"), palette).on_click(
            "overlays-wire-cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.close(cx)),
        );

        let primary: Button = match action {
            FooterAction::Create { enabled } => {
                primary_button(tr!("overlays_wire_create"), palette)
                    .disabled(!enabled)
                    .on_click(
                        "overlays-wire-create",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.confirm(cx)),
                    )
            }
            FooterAction::Open(action_id) => {
                primary_button(tr!("overlays_wire_open_action"), palette).on_click(
                    "overlays-wire-open",
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.open_action(action_id, cx)),
                )
            }
        };

        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(back)
            .child(div().flex_1())
            .child(cancel)
            .child(primary)
            .into_any_element()
    }
}

fn render_records(
    draft: &WiringDraft,
    step_value: &str,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    let (action_tag, action_value) = match &draft.action {
        ActionRecord::Create { name, queue } => (
            Tag::New,
            tr!(
                "overlays_wire_record_action_new",
                name = name.as_str(),
                queue = queue.as_str()
            ),
        ),
        ActionRecord::Extend { name } => (Tag::Changed, name.clone()),
    };
    let step_tag = if draft.plan.added_step().is_some() {
        Tag::Added
    } else {
        Tag::Kept
    };
    let (trigger_tag, trigger_value) = match &draft.trigger {
        TriggerRecord::Create(name) => (Tag::New, name.clone()),
        TriggerRecord::Reuse(name) => (Tag::Kept, name.clone()),
    };

    div()
        .w_full()
        .p(scaled(RECORDS_PAD, density))
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xs, density))
        .rounded(radius(Radius::Md))
        .border(BORDER_THIN)
        .border_color(palette.border_regular)
        .bg(palette.shell)
        .child(record_row(
            tr!("overlays_wire_record_action"),
            Some(action_tag),
            &action_value,
            palette,
            density,
            cx,
        ))
        .child(record_row(
            tr!("overlays_wire_record_trigger"),
            Some(trigger_tag),
            &trigger_value,
            palette,
            density,
            cx,
        ))
        .child(record_row(
            tr!("overlays_wire_record_step"),
            Some(step_tag),
            step_value,
            palette,
            density,
            cx,
        ))
        .into_any_element()
}

#[derive(Clone, Copy)]
enum Tag {
    New,
    Added,
    Changed,
    Kept,
}

impl Tag {
    fn text(self) -> String {
        match self {
            Self::New => tr!("overlays_wire_tag_new"),
            Self::Added => tr!("overlays_wire_tag_added"),
            Self::Changed => tr!("overlays_wire_tag_changed"),
            Self::Kept => tr!("overlays_wire_tag_kept"),
        }
    }

    fn tint(self, palette: &ForgePalette) -> Rgba {
        match self {
            Self::New | Self::Added => palette.success,
            Self::Changed => palette.info,
            Self::Kept => palette.text_faint,
        }
    }
}

fn render_already_wired(
    action_name: &str,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xs, density))
        .child(
            div()
                .font_family(cx.body_family())
                .text_size(FONT_XS)
                .text_color(palette.text_muted)
                .child(tr!("overlays_wire_already_body")),
        )
        .child(record_row(
            tr!("overlays_wire_record_action"),
            None,
            action_name,
            palette,
            density,
            cx,
        ))
        .into_any_element()
}

fn note_row(
    glyph: Icon,
    tint: Rgba,
    message: &str,
    text_color: Rgba,
    density: Density,
    cx: &App,
) -> AnyElement {
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, density))
        .child(icon(glyph, NOTE_GLYPH, tint))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .font_family(cx.body_family())
                .text_size(FONT_XS)
                .text_color(text_color)
                .child(message.to_owned()),
        )
        .into_any_element()
}

fn record_row(
    label: String,
    tag: Option<Tag>,
    value: &str,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    let shown = if value.is_empty() { "-" } else { value };
    let tag = tag.map(|tag| {
        let tint = tag.tint(palette);
        badge(
            with_alpha(tint, TINT_STRONG_FILL_ALPHA),
            tint,
            tag.text().to_uppercase(),
            true,
            TAG_FS,
        )
    });
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(scaled(ROW_LABEL_GAP, density))
        .child(
            div()
                .flex_none()
                .w(RECORD_LABEL_W)
                .font_family(cx.mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(label),
        )
        .children(tag)
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .font_family(cx.body_family())
                .text_size(FONT_XS)
                .text_color(palette.text_secondary)
                .child(shown.to_owned()),
        )
        .into_any_element()
}
