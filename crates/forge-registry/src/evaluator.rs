use forge_events::EventSource;

pub struct EventFilter {
    pub source: Option<EventSource>,
    pub kind_prefix: Option<String>,
}

/// Matches only at a `.` segment boundary: `a.b` matches `a.b` and `a.b.c`, never `a.b_c`; a prefix ending in `.` matches any kind under it.
pub fn kind_matches_prefix(kind: &str, prefix: &str) -> bool {
    match kind.strip_prefix(prefix) {
        Some(rest) => rest.is_empty() || prefix.ends_with('.') || rest.starts_with('.'),
        None => false,
    }
}
