use super::*;
use crate::shared::option_choices::ChoiceOption;
use forge_components::{ActivePresentation, scaled};

impl EditSubActionForm {
    pub(super) fn unit_amount_control(
        &self,
        unit_picker_key: &str,
        units: &[ChoiceOption],
        unit: &str,
        input: &Entity<TextInput>,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let unit_select = self.render_select_trigger(
            unit_picker_key,
            units,
            unit,
            self.open_picker_for(unit_picker_key),
            palette,
            cx,
        );
        div()
            .flex()
            .gap(scaled(GRID_COL_GAP, density))
            .child(div().flex_1().child(input.clone()))
            .child(div().flex_1().child(unit_select))
            .into_any_element()
    }
}
