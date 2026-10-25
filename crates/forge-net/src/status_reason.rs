pub fn canonical_status_reason(status: u16) -> Option<&'static str> {
    reqwest::StatusCode::from_u16(status)
        .ok()
        .and_then(|code| code.canonical_reason())
}
