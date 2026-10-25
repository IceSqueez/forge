pub fn canonical_status_reason(status: u16) -> Option<&'static str> {
    reqwest::StatusCode::from_u16(status)
        .ok()
        .and_then(|code| code.canonical_reason())
}

#[cfg(test)]
mod tests {
    use super::canonical_status_reason;

    #[test]
    fn known_codes_carry_their_reason_and_others_carry_none() {
        for (status, reason) in [
            (404, Some("Not Found")),
            (429, Some("Too Many Requests")),
            (599, None),
            (99, None),
            (1000, None),
            (0, None),
        ] {
            assert_eq!(canonical_status_reason(status), reason, "{status}");
        }
    }
}
