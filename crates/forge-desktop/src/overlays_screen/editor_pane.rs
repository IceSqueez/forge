use forge_components::{
    BORDER_THIN, FONT_XS, FONT_XXS, ForgePalette, Icon, body_family, empty_state, icon,
    mono_family, tr,
};
use forge_storage::OverlayDefinition;
use gpui::{AnyElement, ClickEvent, Context, FontWeight, Pixels, div, prelude::*, px};

use super::OverlaysView;

const HEAD_PAD_V: Pixels = px(9.0);
const HEAD_PAD_H: Pixels = px(16.0);
const HEAD_GAP: Pixels = px(10.0);
const HEAD_GLYPH: Pixels = px(15.0);
const HEAD_TITLE_FS: Pixels = px(13.0);

const URL_BOX_GAP: Pixels = px(8.0);
const URL_BOX_PAD_V: Pixels = px(5.0);
const URL_BOX_PAD_H: Pixels = px(10.0);
const URL_BOX_RADIUS: Pixels = px(7.0);
const URL_FS: Pixels = px(11.0);
const URL_GLYPH: Pixels = px(12.0);
const COPY_GAP: Pixels = px(3.0);

const NOTICE_GAP: Pixels = px(6.0);
const NOTICE_PAD_V: Pixels = px(8.0);
const NOTICE_PAD_H: Pixels = px(16.0);
const NOTICE_GLYPH: Pixels = px(12.0);

const STAGE_PAD: Pixels = px(20.0);

impl OverlaysView {
    pub(super) fn render_editor_pane(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pane = div()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .flex()
            .flex_col()
            .bg(palette.base);

        let Some(definition) = self.selected_definition() else {
            return pane
                .child(self.render_stage(None, palette))
                .into_any_element();
        };

        let unavailable_notice = (!self.visuals(definition, palette).is_available())
            .then(|| self.render_unavailable_notice(definition, palette));

        pane.child(self.render_selection_header(definition, palette, cx))
            .children(unavailable_notice)
            .child(self.render_stage(Some(definition), palette))
            .into_any_element()
    }

    fn render_selection_header(
        &self,
        definition: &OverlayDefinition,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let visuals = self.visuals(definition, palette);

        div()
            .flex_none()
            .w_full()
            .flex()
            .items_center()
            .gap(HEAD_GAP)
            .px(HEAD_PAD_H)
            .py(HEAD_PAD_V)
            .bg(palette.elevated)
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(icon(visuals.icon, HEAD_GLYPH, visuals.accent))
            .child(
                div()
                    .flex_none()
                    .font_family(body_family())
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(HEAD_TITLE_FS)
                    .text_color(palette.text_primary)
                    .child(definition.display_name.clone()),
            )
            .child(
                div()
                    .flex_none()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(definition.id.as_str().to_owned()),
            )
            .child(div().flex_1().min_w(px(0.0)))
            .child(self.render_url_box(definition, palette, cx))
            .into_any_element()
    }

    fn render_url_box(
        &self,
        definition: &OverlayDefinition,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let box_frame = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(URL_BOX_GAP)
            .px(URL_BOX_PAD_H)
            .py(URL_BOX_PAD_V)
            .rounded(URL_BOX_RADIUS)
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.shell)
            .font_family(mono_family())
            .text_size(URL_FS);

        let Some(url) = self.overlay_url(&definition.id) else {
            return box_frame
                .child(icon(Icon::Link, URL_GLYPH, palette.text_faint))
                .child(
                    div()
                        .font_family(body_family())
                        .text_color(palette.text_faint)
                        .child(tr!("overlays_url_not_served")),
                )
                .into_any_element();
        };

        let id = definition.id.clone();
        box_frame
            .child(icon(Icon::Link, URL_GLYPH, palette.text_faint))
            .child(div().text_color(palette.text_secondary).child(url))
            .child(
                div()
                    .id("overlays-copy-url")
                    .flex()
                    .items_center()
                    .gap(COPY_GAP)
                    .cursor_pointer()
                    .text_color(palette.brand)
                    .on_click(
                        cx.listener(move |this, _: &ClickEvent, _, cx| this.copy_url(&id, cx)),
                    )
                    .child(icon(Icon::Copy, URL_GLYPH, palette.brand))
                    .child(tr!("overlays_url_copy")),
            )
            .into_any_element()
    }

    fn render_unavailable_notice(
        &self,
        definition: &OverlayDefinition,
        palette: &ForgePalette,
    ) -> AnyElement {
        div()
            .flex_none()
            .w_full()
            .flex()
            .items_center()
            .gap(NOTICE_GAP)
            .px(NOTICE_PAD_H)
            .py(NOTICE_PAD_V)
            .bg(palette.shell)
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(icon(Icon::AlertTriangle, NOTICE_GLYPH, palette.warning))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(tr!(
                        "overlays_type_unavailable_notice",
                        kind = definition.kind_id.as_str()
                    )),
            )
            .into_any_element()
    }

    fn render_stage(
        &self,
        selection: Option<&OverlayDefinition>,
        palette: &ForgePalette,
    ) -> AnyElement {
        let (message, glyph) = match selection {
            Some(_) => (tr!("overlays_editor_pending"), Icon::Adjustments),
            None if self.overlays.is_empty() => (tr!("overlays_stage_empty"), Icon::Browser),
            None => (tr!("overlays_stage_select"), Icon::Browser),
        };

        div()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .p(STAGE_PAD)
            .flex()
            .items_center()
            .justify_center()
            .child(empty_state(message, palette).glyph(glyph))
            .into_any_element()
    }
}
