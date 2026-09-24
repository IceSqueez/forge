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

#[cfg(test)]
mod tests {
    use super::kind_matches_prefix;

    #[test]
    fn a_prefix_matches_its_own_kind_and_every_dotted_descendant() {
        for (kind, prefix) in [
            ("twitch.channel.unban", "twitch.channel.unban"),
            ("obs.scene.changed", "obs.scene"),
            ("obs.scene.item.enable_state_changed", "obs.scene"),
            ("obs.scene.changed", "obs.scene."),
            ("custom.", "custom."),
        ] {
            assert!(
                kind_matches_prefix(kind, prefix),
                "{kind:?} must match {prefix:?}"
            );
        }
    }

    #[test]
    fn a_prefix_never_matches_a_sibling_that_merely_extends_its_last_segment() {
        for (kind, prefix) in [
            (
                "twitch.channel.chat.message_delete",
                "twitch.channel.chat.message",
            ),
            (
                "twitch.channel.unban_request.create",
                "twitch.channel.unban",
            ),
            ("youtube.channel.member_gift", "youtube.channel.member"),
            ("youtube.channel.raided", "youtube.channel.raid"),
            ("youtube.channel.member-x", "youtube.channel.member"),
        ] {
            assert!(
                !kind_matches_prefix(kind, prefix),
                "{kind:?} must not match {prefix:?}"
            );
        }
    }

    #[test]
    fn a_kind_shorter_than_or_unrelated_to_the_prefix_never_matches() {
        for (kind, prefix) in [
            ("twitch.channel", "twitch.channel.unban"),
            ("obs.scene", "obs.scene."),
            ("", "obs.scene"),
            ("twitch.chat.message", "chat.message"),
            ("Obs.scene.changed", "obs.scene"),
        ] {
            assert!(
                !kind_matches_prefix(kind, prefix),
                "{kind:?} must not match {prefix:?}"
            );
        }
    }

    #[test]
    fn an_empty_prefix_matches_only_the_empty_kind() {
        // Why: `None` is the match-all filter; `Some("")` is not a wildcard, so a descriptor
        // declaring it silently never fires.
        assert!(kind_matches_prefix("", ""));
        assert!(!kind_matches_prefix("twitch.channel.unban", ""));
    }
}
