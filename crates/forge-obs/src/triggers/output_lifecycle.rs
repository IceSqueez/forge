use forge_events::Event;
use forge_types::{ArgStack, Variant};

pub(super) fn output_arg_stack(
    event: &Event,
    active_field: &str,
    state_var: &str,
    active_var: &str,
) -> ArgStack {
    let state = event.kind.rsplit('.').next().unwrap_or_default();
    let stack = ArgStack::new().set(state_var.to_owned(), Variant::String(state.to_owned()));
    match event
        .payload
        .get(active_field)
        .and_then(serde_json::Value::as_bool)
    {
        Some(active) => stack.set(active_var.to_owned(), Variant::Bool(active)),
        None => stack,
    }
}
