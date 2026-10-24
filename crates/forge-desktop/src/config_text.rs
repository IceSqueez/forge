use forge_types::{Variant, display_scalar};

const LIST_SEPARATOR: &str = "\n";

pub(crate) fn editable_text(value: &Variant) -> String {
    match value {
        Variant::Array(items) if items.iter().all(is_scalar) => items
            .iter()
            .map(display_scalar)
            .collect::<Vec<_>>()
            .join(LIST_SEPARATOR),
        Variant::Array(_) | Variant::Object(_) => {
            let json = value.to_plain_json();
            serde_json::to_string_pretty(&json).unwrap_or_else(|_| json.to_string())
        }
        scalar => scalar.to_string(),
    }
}

pub(crate) fn editable_line(value: &Variant) -> String {
    if is_scalar(value) {
        value.to_string()
    } else {
        value.to_plain_json().to_string()
    }
}

fn is_scalar(value: &Variant) -> bool {
    !matches!(value, Variant::Array(_) | Variant::Object(_))
}
