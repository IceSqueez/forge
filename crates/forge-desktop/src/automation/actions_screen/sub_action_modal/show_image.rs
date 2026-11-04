use super::*;
use crate::shared::config_form::set_picked_value;
use crate::tools::overlays_screen::icon_choice::{IconImage, icon_field_row};
use crate::tools::overlays_screen::icon_picker_host::IconPickResult;
use forge_components::{ActivePresentation, FONT_XXS, Spacing, spacing};
use forge_overlay::SHOW_IMAGE;
use forge_overlay::config::IMAGE;

#[derive(Default)]
pub(super) struct ShowImageChoice {
    images: Option<Vec<IconImage>>,
    note: Option<String>,
}

impl EditSubActionForm {
    pub(in crate::automation::actions_screen) fn offer_show_images(
        &mut self,
        images: Vec<IconImage>,
        cx: &mut Context<Self>,
    ) {
        self.show_image.images = Some(images);
        cx.notify();
    }

    pub(in crate::automation::actions_screen) fn begin_show_image_import(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.show_image.note = Some(tr!("overlays_icon_importing"));
        cx.notify();
    }

    pub(in crate::automation::actions_screen) fn cancel_show_image_import(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.show_image.note = None;
        cx.notify();
    }

    pub(in crate::automation::actions_screen) fn settle_show_image(
        &mut self,
        result: IconPickResult,
        cx: &mut Context<Self>,
    ) {
        match result {
            IconPickResult::Chosen(value) => {
                set_picked_value(&mut self.fields, SHOW_IMAGE, &value, cx);
                self.show_image.note = None;
            }
            IconPickResult::Refused(message) => self.show_image.note = Some(message),
        }
        cx.notify();
    }

    fn request_show_image(&mut self, cx: &mut Context<Self>) {
        self.show_image.note = None;
        cx.emit(SubFormEvent::PickShowImage);
        cx.notify();
    }

    pub(super) fn show_image_item(
        &self,
        key: &str,
        label: &str,
        selected: &str,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if key != SHOW_IMAGE {
            return None;
        }
        let images = self.show_image.images.as_deref()?;
        let density = cx.density();
        let hover_border = palette.brand;
        let row = icon_field_row(IMAGE, selected, images, palette.brand, palette, density, cx)
            .id(SharedString::from(format!("actions-sub-image-{key}")))
            .cursor_pointer()
            .hover(move |style| style.border_color(hover_border))
            .key_activated(palette.text_primary)
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.request_show_image(cx);
            }));
        let note_color = palette.text_faint;
        let note = self.show_image.note.clone().map(|note| {
            div()
                .font_family(cx.body_family())
                .text_size(FONT_XXS)
                .text_color(note_color)
                .child(note)
        });
        Some(
            div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xxs, density))
                .child(
                    div()
                        .font_family(cx.mono_family())
                        .text_size(FONT_XXS)
                        .text_color(palette.text_muted)
                        .child(label.to_owned()),
                )
                .child(row)
                .children(note)
                .into_any_element(),
        )
    }
}
