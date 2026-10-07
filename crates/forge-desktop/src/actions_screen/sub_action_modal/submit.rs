use super::field::{SubFormField, field_gate, field_values, is_var_key};
use super::*;
use crate::quick_action_field_rows::int_entry_invalid;
use forge_types::{Variant, normalize_var_name};

impl EditSubActionForm {
    pub(super) fn submit(&mut self, cx: &mut Context<Self>) {
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
                key,
                integer,
                gate,
                input,
                ..
            } = field
            else {
                continue;
            };
            let text = input.read(cx).content().to_owned();
            let invalid = if is_var_key(key) {
                !text.trim().is_empty() && normalize_var_name(&text).is_none()
            } else if let Some(range) = integer {
                gate_on(gate.as_ref())
                    && int_entry_invalid(&text, *range.start(), *range.end(), false)
            } else {
                continue;
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

        cx.emit(SubFormEvent::Commit(SubFormCommit {
            target,
            kind_id,
            overrides,
            continue_on_error,
            condition,
            label,
        }));
    }
}

fn normalize_condition(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let trimmed = trimmed.strip_prefix("if ").unwrap_or(trimmed).trim();
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
