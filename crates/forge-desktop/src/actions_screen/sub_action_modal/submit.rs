use super::field::{InputFlavor, SubFormField, field_gate, field_keys, field_values};
use super::*;
use forge_components::int_entry_invalid;
use forge_types::{Variant, normalize_var_name};

impl EditSubActionForm {
    pub(in crate::actions_screen) fn submit(&mut self, cx: &mut Context<Self>) {
        self.clear_rejection(cx);
        let bool_vals: HashMap<String, bool> = self
            .fields
            .iter()
            .filter_map(|f| match f {
                SubFormField::Bool { key, value, .. } => Some((key.clone(), *value)),
                _ => None,
            })
            .collect();
        let gate_on = |gate: Option<&String>| {
            gate.map(|g| bool_vals.get(g).copied().unwrap_or(false))
                .unwrap_or(true)
        };
        let mut has_invalid = false;
        for field in &self.fields {
            if let SubFormField::UnitAmount { gate, .. } = field {
                has_invalid |= gate_on(gate.as_ref()) && field.flag_unit_amount_bound(cx);
                continue;
            }
            let SubFormField::Input {
                flavor,
                gate,
                input,
                ..
            } = field
            else {
                continue;
            };
            let text = input.read(cx).content().to_owned();
            let invalid = match flavor {
                InputFlavor::VariableName => {
                    gate_on(gate.as_ref())
                        && !text.trim().is_empty()
                        && normalize_var_name(&text).is_none()
                }
                InputFlavor::Integer(_) | InputFlavor::OneBased(_) => {
                    gate_on(gate.as_ref())
                        && flavor.shown_bounds().is_some_and(|range| {
                            int_entry_invalid(&text, *range.start(), *range.end(), false)
                        })
                }
                InputFlavor::Text | InputFlavor::File | InputFlavor::DateTime => continue,
            };
            input.update(cx, |input, cx| input.set_invalid(invalid, cx));
            has_invalid |= invalid;
        }
        if has_invalid {
            return;
        }
        let target = self.target;
        let kind_id = self.kind_id.clone();
        let continue_on_error = self.continue_on_error;
        let label = resolve_step_label(self.name_input.read(cx).content(), &self.kind_label);
        let condition = normalize_condition(self.condition_input.read(cx).content());
        let overrides: Vec<(String, Variant)> = self
            .fields
            .iter()
            .filter(|field| gate_on(field_gate(field)))
            .flat_map(|field| field_values(field, cx))
            .collect();
        let dropped_gates: Vec<&String> = self
            .fields
            .iter()
            .filter_map(|field| match field {
                SubFormField::Bool {
                    key,
                    gate,
                    value: false,
                    presence_gate: true,
                    ..
                } if gate_on(gate.as_ref()) => Some(key),
                _ => None,
            })
            .collect();
        let mut cleared: Vec<String> = self
            .fields
            .iter()
            .filter(|field| field_gate(field).is_some_and(|gate| dropped_gates.contains(&gate)))
            .flat_map(field_keys)
            .map(str::to_owned)
            .collect();
        cleared.extend(dropped_gates.into_iter().cloned());
        cleared.sort();
        cleared.dedup();

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
