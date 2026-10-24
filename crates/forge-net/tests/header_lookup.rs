use forge_net::HeaderLookup;

#[test]
fn header_names_match_regardless_of_ascii_case() {
    let headers = [("Retry-After", "5"), ("X-RateLimit-Scope", "global")];
    for (query, expected) in [
        ("retry-after", Some("5")),
        ("RETRY-AFTER", Some("5")),
        ("Retry-After", Some("5")),
        ("x-ratelimit-scope", Some("global")),
        ("x-ratelimit-global", None),
        ("retry-afte", None),
    ] {
        assert_eq!(headers.header_value(query), expected, "query {query:?}");
    }
}

#[test]
fn duplicate_header_names_resolve_to_the_first_value() {
    let headers = [
        ("retry-after", "1"),
        ("Retry-After", "2"),
        ("RETRY-AFTER", "3"),
    ];
    assert_eq!(headers.header_value("retry-after"), Some("1"));
}
