use super::*;
use crate::shared::config_form::{
    CollectedValues, ConfigField, GateStates, InputFlavor, flag_integer_input, flag_variable_input,
    gated_field_values,
};

impl EditSubActionForm {
    pub(in crate::automation::actions_screen) fn submit(&mut self, cx: &mut Context<Self>) {
        self.clear_rejection(cx);
        if self.flag_invalid_entries(cx) {
            return;
        }
        let target = self.target;
        let kind_id = self.kind_id.clone();
        let continue_on_error = self.continue_on_error;
        let label = resolve_step_label(self.name_input.read(cx).content(), &self.kind_label);
        let condition = normalize_condition(self.condition_input.read(cx).content());
        let CollectedValues {
            values: overrides,
            cleared,
        } = gated_field_values(&self.fields, cx);

        cx.emit(SubFormEvent::Commit(SubFormCommit {
            target,
            kind_id,
            overrides,
            cleared,
            continue_on_error,
            condition,
            label,
        }));
    }

    fn flag_invalid_entries(&self, cx: &mut Context<Self>) -> bool {
        let gates = GateStates::of(&self.fields);
        let mut has_invalid = false;
        for field in &self.fields {
            let open = gates.open(field.gate());
            match field {
                ConfigField::UnitAmount { .. } => {
                    has_invalid |= open && field.flag_unit_amount_bound(cx);
                }
                ConfigField::Input { flavor, input, .. } => match flavor {
                    InputFlavor::VariableName if open => {
                        has_invalid |= flag_variable_input(input, cx);
                    }
                    InputFlavor::Integer(_) | InputFlavor::OneBased(_) if open => {
                        if let Some(range) = flavor.shown_bounds() {
                            has_invalid |= flag_integer_input(input, &range, cx);
                        }
                    }
                    InputFlavor::VariableName
                    | InputFlavor::Integer(_)
                    | InputFlavor::OneBased(_) => {
                        input.update(cx, |input, cx| input.set_invalid(false, cx));
                    }
                    InputFlavor::Text | InputFlavor::File | InputFlavor::DateTime => {}
                },
                _ => {}
            }
        }
        has_invalid
    }
}

const CONDITION_KEYWORD: &str = "if";
const CONDITION_PREFIX: &str = "if ";

fn normalize_condition(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed == CONDITION_KEYWORD {
        return None;
    }
    let trimmed = trimmed
        .strip_prefix(CONDITION_PREFIX)
        .unwrap_or(trimmed)
        .trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

fn resolve_step_label(raw: &str, kind_label: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == kind_label {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_condition, resolve_step_label};

    #[test]
    fn a_condition_loses_surrounding_space_and_one_leading_if_keyword() {
        for (raw, expected) in [
            ("if x", Some("x")),
            ("  if   x > 1  ", Some("x > 1")),
            ("iffy", Some("iffy")),
            ("if if x", Some("if x")),
            ("x", Some("x")),
            ("", None),
            ("   ", None),
            ("if", None),
            ("if ", None),
            ("  if  ", None),
        ] {
            assert_eq!(normalize_condition(raw).as_deref(), expected, "{raw:?}");
        }
    }

    #[test]
    fn a_step_label_equal_to_the_kind_label_or_blank_is_dropped() {
        for (raw, expected) in [
            ("", None),
            ("   ", None),
            ("Send chat", None),
            ("  Send chat  ", None),
            ("send chat", Some("send chat")),
            ("  Greet viewers ", Some("Greet viewers")),
        ] {
            assert_eq!(
                resolve_step_label(raw, "Send chat").as_deref(),
                expected,
                "{raw:?}"
            );
        }
    }
}
