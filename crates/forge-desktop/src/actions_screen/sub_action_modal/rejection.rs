use super::field::SubFormField;
use super::*;
use crate::actions_screen::step_validation::StepRejection;
use forge_components::{ActivePresentation, FONT_XXS, Spacing, spacing};
use gpui::App;

impl EditSubActionForm {
    pub(in crate::actions_screen) fn show_rejection(
        &mut self,
        rejection: StepRejection,
        cx: &mut Context<Self>,
    ) {
        self.mark_rejected_input(&rejection, true, cx);
        self.rejection = Some(rejection);
        cx.notify();
    }

    pub(super) fn clear_rejection(&mut self, cx: &mut Context<Self>) {
        if let Some(rejection) = self.rejection.take() {
            self.mark_rejected_input(&rejection, false, cx);
            cx.notify();
        }
    }

    fn mark_rejected_input(&self, rejection: &StepRejection, invalid: bool, cx: &mut App) {
        let Some(rejected_key) = rejection.field_key.as_deref() else {
            return;
        };
        for field in &self.fields {
            if let SubFormField::Input { key, input, .. }
            | SubFormField::UnitAmount { key, input, .. } = field
                && key == rejected_key
            {
                input.update(cx, |input, cx| input.set_invalid(invalid, cx));
            }
        }
    }
}

pub(super) fn rejection_note(message: String, palette: &ForgePalette, cx: &App) -> AnyElement {
    div()
        .font_family(cx.body_family())
        .text_size(FONT_XXS)
        .text_color(palette.danger)
        .child(message)
        .into_any_element()
}

pub(super) fn with_rejection_note(
    element: AnyElement,
    message: String,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xxs, density))
        .child(element)
        .child(rejection_note(message, palette, cx))
        .into_any_element()
}
