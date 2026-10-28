pub(crate) fn rows(response: &serde_json::Value) -> impl Iterator<Item = &serde_json::Value> {
    response["data"].as_array().into_iter().flatten()
}

pub(crate) fn first_row(response: &serde_json::Value) -> Option<&serde_json::Value> {
    response["data"].as_array().and_then(|rows| rows.first())
}
