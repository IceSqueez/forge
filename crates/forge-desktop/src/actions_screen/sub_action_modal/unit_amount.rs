use super::field::SubFormField;
use super::*;
use crate::amount_scale::{AmountScale, AmountSeed};
use crate::quick_action_field_rows::int_entry_invalid;
use forge_components::InputEvent;
use forge_types::SubActionConfig;

impl SubFormField {
    pub(super) fn flag_unit_amount_bound(&self, cx: &mut App) -> bool {
        let Self::UnitAmount {
            scale, unit, input, ..
        } = self
        else {
            return false;
        };
        let range = scale.range_for(unit);
        let invalid = int_entry_invalid(
            input.read(cx).content(),
            *range.start(),
            *range.end(),
            false,
        );
        input.update(cx, |input, cx| input.set_invalid(invalid, cx));
        invalid
    }
}

impl EditSubActionForm {
    fn on_unit_amount_input_event(
        &mut self,
        changed: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if !matches!(event, InputEvent::Changed(_)) {
            return;
        }
        if let Some(field) = self.fields.iter().find(
            |field| matches!(field, SubFormField::UnitAmount { input, .. } if *input == changed),
        ) {
            field.flag_unit_amount_bound(cx);
        }
    }

    pub(super) fn unit_amount_control(
        &self,
        unit_picker_key: &str,
        scale: &AmountScale,
        unit: &str,
        input: &Entity<TextInput>,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let unit_select = self.render_select_trigger(
            unit_picker_key,
            &scale.unit_options(),
            unit,
            self.open_picker_for(unit_picker_key),
            palette,
            cx,
        );
        div()
            .flex()
            .gap(GRID_COL_GAP)
            .child(div().flex_1().child(input.clone()))
            .child(div().flex_1().child(unit_select))
            .into_any_element()
    }
}

pub(super) fn build_unit_amount_field(
    key: &str,
    label: &str,
    scale: AmountScale,
    gate: Option<String>,
    config: &SubActionConfig,
    palette: ForgePalette,
    cx: &mut Context<EditSubActionForm>,
) -> SubFormField {
    let AmountSeed { unit, amount: seed } = scale.seed(key, config);
    let range = scale.range_for(&unit);
    let invalid_seed = int_entry_invalid(&seed, *range.start(), *range.end(), false);
    let input = cx.new(|cx| {
        let mut input = TextInput::new("0", cx).with_palette(palette);
        if !seed.is_empty() {
            input.set_content(seed, cx);
        }
        input.set_invalid(invalid_seed, cx);
        input
    });
    let sub = cx.subscribe(&input, EditSubActionForm::on_unit_amount_input_event);
    SubFormField::UnitAmount {
        key: key.to_owned(),
        label: label.to_owned(),
        unit_picker_key: scale.unit_picker_key(key),
        scale,
        unit,
        gate,
        input,
        _sub: sub,
    }
}
