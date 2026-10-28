use forge_components::ActivePresentation;
use forge_components::Density;
use forge_components::FONT_HERO;
use forge_components::{
    BORDER_THIN, Button, FONT_XS, FONT_XXS, ForgePalette, Icon, ModalSize, OverlayPosition, Radius,
    Spacing, body_family, empty_state, ghost_button_with_icon, modal, mono_family, overlay,
    primary_button, radius, scaled, secondary_button, section_label, spacing, tr,
};
use forge_types::ActionId;
use gpui::{AnyElement, ClickEvent, Context, FontWeight, Pixels, div, prelude::*, px};

use super::plan::WiringDraft;
use super::{ConfirmStage, ConfirmState, EventWiringView};

const CARD_PAD_V: Pixels = px(14.0);
const CARD_GAP: Pixels = px(14.0);
const ROW_LABEL_GAP: Pixels = px(8.0);
const WORDING_PAD: Pixels = px(12.0);
const WORDING_LEAD_FS: Pixels = FONT_HERO;

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

        let (body, footer) = match &stage.state {
            ConfirmState::Checking => (
                empty_state(tr!("overlays_wire_checking"), palette)
                    .loading("overlays-wire-checking")
                    .into_any_element(),
                self.render_footer(FooterAction::Create { enabled: false }, palette, cx),
            ),
            ConfirmState::Refused(message) => (
                empty_state(message.clone(), palette)
                    .glyph(Icon::AlertTriangle)
                    .into_any_element(),
                self.render_footer(FooterAction::Create { enabled: false }, palette, cx),
            ),
            ConfirmState::AlreadyWired {
                action_id,
                action_name,
            } => (
                render_already_wired(action_name, palette, density),
                self.render_footer(FooterAction::Open(*action_id), palette, cx),
            ),
            ConfirmState::Ready(draft) => (
                render_draft(draft, palette, density),
                self.render_footer(
                    FooterAction::Create {
                        enabled: !stage.writing,
                    },
                    palette,
                    cx,
                ),
            ),
        };

        let card = modal(
            tr!(
                "overlays_wire_confirm_title",
                overlay = overlay_name.as_str()
            ),
            div()
                .w_full()
                .px(spacing(Spacing::Md, density))
                .py(scaled(CARD_PAD_V, density))
                .flex()
                .flex_col()
                .gap(scaled(CARD_GAP, density))
                .child(body),
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

fn render_draft(draft: &WiringDraft, palette: &ForgePalette, density: Density) -> AnyElement {
    let records = div()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xs, density))
        .child(section_label(
            tr!("overlays_wire_section_creates").to_uppercase(),
            palette,
        ))
        .child(record_row(
            tr!("overlays_wire_record_action"),
            &draft.action_name,
            palette,
            density,
        ))
        .child(record_row(
            tr!("overlays_wire_record_trigger"),
            &draft.trigger_name,
            palette,
            density,
        ))
        .child(record_row(
            tr!("overlays_wire_record_queue"),
            &draft.queue_name,
            palette,
            density,
        ));

    div()
        .flex()
        .flex_col()
        .gap(scaled(CARD_GAP, density))
        .child(records)
        .child(
            div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xs, density))
                .child(section_label(
                    tr!("overlays_wire_section_wording").to_uppercase(),
                    palette,
                ))
                .child(render_wording(&draft.wording, palette, density)),
        )
        .into_any_element()
}

fn render_wording(lines: &[String], palette: &ForgePalette, density: Density) -> AnyElement {
    let frame = div()
        .w_full()
        .p(scaled(WORDING_PAD, density))
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xxs, density))
        .rounded(radius(Radius::Md))
        .border(BORDER_THIN)
        .border_color(palette.border_regular)
        .bg(palette.shell)
        .font_family(body_family());

    if lines.is_empty() {
        return frame
            .child(
                div()
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(tr!("overlays_wire_wording_empty")),
            )
            .into_any_element();
    }

    let mut column = frame;
    for (index, line) in lines.iter().enumerate() {
        column = column.child(if index == 0 {
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_size(WORDING_LEAD_FS)
                .text_color(palette.text_primary)
                .child(line.clone())
        } else {
            div()
                .text_size(FONT_XS)
                .text_color(palette.text_muted)
                .child(line.clone())
        });
    }
    column.into_any_element()
}

fn render_already_wired(action_name: &str, palette: &ForgePalette, density: Density) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xs, density))
        .child(
            div()
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(palette.text_muted)
                .child(tr!("overlays_wire_already_body")),
        )
        .child(record_row(
            tr!("overlays_wire_record_action"),
            action_name,
            palette,
            density,
        ))
        .into_any_element()
}

fn record_row(label: String, value: &str, palette: &ForgePalette, density: Density) -> AnyElement {
    let shown = if value.is_empty() { "-" } else { value };
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(scaled(ROW_LABEL_GAP, density))
        .child(
            div()
                .flex_none()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(label),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(palette.text_secondary)
                .child(shown.to_owned()),
        )
        .into_any_element()
}
