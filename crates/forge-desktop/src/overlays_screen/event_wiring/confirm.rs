use forge_components::{
    BORDER_THIN, Button, Density, FONT_XS, FONT_XXS, ForgePalette, Icon, ModalSize,
    OverlayPosition, Radius, Spacing, body_family, empty_state, ghost_button_with_icon, modal,
    mono_family, overlay, primary_button, radius, secondary_button, section_label, spacing, tr,
};
use forge_types::ActionId;
use gpui::{AnyElement, ClickEvent, Context, FontWeight, Pixels, div, prelude::*, px};

use super::plan::WiringDraft;
use super::{ConfirmStage, ConfirmState, EventWiringView};

const CARD_PAD_V: Pixels = px(14.0);
const CARD_PAD_H: Pixels = px(16.0);
const CARD_GAP: Pixels = px(14.0);
const ROW_GAP: Pixels = px(6.0);
const ROW_LABEL_GAP: Pixels = px(8.0);
const WORDING_PAD: Pixels = px(12.0);
const WORDING_GAP: Pixels = px(4.0);
const WORDING_LEAD_FS: Pixels = px(15.0);

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
                self.render_footer(Some(FooterAction::Create { enabled: false }), palette, cx),
            ),
            ConfirmState::Refused(message) => (
                empty_state(message.clone(), palette)
                    .glyph(Icon::AlertTriangle)
                    .into_any_element(),
                self.render_footer(Some(FooterAction::Create { enabled: false }), palette, cx),
            ),
            ConfirmState::AlreadyWired {
                action_id,
                action_name,
            } => (
                render_already_wired(action_name, palette),
                self.render_footer(Some(FooterAction::Open(*action_id)), palette, cx),
            ),
            ConfirmState::Ready(draft) => (
                render_draft(draft, palette),
                self.render_footer(
                    Some(FooterAction::Create {
                        enabled: !stage.writing,
                    }),
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
                .px(CARD_PAD_H)
                .py(CARD_PAD_V)
                .flex()
                .flex_col()
                .gap(CARD_GAP)
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
        action: Option<FooterAction>,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let back = ghost_button_with_icon(Icon::ArrowBackUp, tr!("overlays_wire_back"), palette)
            .on_click(
                "overlays-wire-back",
                cx.listener(|this, _: &ClickEvent, window, cx| this.back_to_picker(window, cx)),
            );
        let cancel = secondary_button(tr!("common_cancel"), palette).on_click(
            "overlays-wire-cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.close(cx)),
        );

        let primary: Option<Button> = match action {
            Some(FooterAction::Create { enabled }) => Some(
                primary_button(tr!("overlays_wire_create"), palette)
                    .disabled(!enabled)
                    .on_click(
                        "overlays-wire-create",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.confirm(cx)),
                    ),
            ),
            Some(FooterAction::Open(action_id)) => Some(
                primary_button(tr!("overlays_wire_open_action"), palette).on_click(
                    "overlays-wire-open",
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.open_action(action_id, cx)),
                ),
            ),
            None => None,
        };

        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(back)
            .child(div().flex_1())
            .child(cancel)
            .children(primary)
            .into_any_element()
    }
}

fn render_draft(draft: &WiringDraft, palette: &ForgePalette) -> AnyElement {
    let records = div()
        .flex()
        .flex_col()
        .gap(ROW_GAP)
        .child(section_label(
            tr!("overlays_wire_section_creates").to_uppercase(),
            palette,
        ))
        .child(record_row(
            tr!("overlays_wire_record_action"),
            &draft.action_name,
            palette,
        ))
        .child(record_row(
            tr!("overlays_wire_record_trigger"),
            &draft.trigger_name,
            palette,
        ))
        .child(record_row(
            tr!("overlays_wire_record_queue"),
            &draft.queue_name,
            palette,
        ));

    div()
        .flex()
        .flex_col()
        .gap(CARD_GAP)
        .child(records)
        .child(
            div()
                .flex()
                .flex_col()
                .gap(ROW_GAP)
                .child(section_label(
                    tr!("overlays_wire_section_wording").to_uppercase(),
                    palette,
                ))
                .child(render_wording(&draft.wording, palette)),
        )
        .into_any_element()
}

fn render_wording(lines: &[String], palette: &ForgePalette) -> AnyElement {
    let frame = div()
        .w_full()
        .p(WORDING_PAD)
        .flex()
        .flex_col()
        .gap(WORDING_GAP)
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

fn render_already_wired(action_name: &str, palette: &ForgePalette) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(ROW_GAP)
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
        ))
        .into_any_element()
}

fn record_row(label: String, value: &str, palette: &ForgePalette) -> AnyElement {
    let shown = if value.is_empty() { "-" } else { value };
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(ROW_LABEL_GAP)
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
