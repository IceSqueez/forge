use time::OffsetDateTime;

use super::PatternZone;
use super::cache::cached_pattern;
use super::current_language::current_names_language;
use crate::datetime_text::datetime_to_text;

pub(crate) fn render_inline_pattern(value: OffsetDateTime, pattern: &str) -> String {
    let cached = cached_pattern(pattern);
    match cached.parsed.as_ref() {
        Ok(parsed) => parsed.format(value, PatternZone::Local, current_names_language()),
        Err(error) => {
            if cached.newly_parsed {
                tracing::warn!(
                    pattern,
                    %error,
                    "template date format is invalid; rendering the value in canonical form"
                );
            }
            datetime_to_text(&value)
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tracing::span::{Attributes, Id, Record};
    use tracing::{Event, Level, Metadata, Subscriber};

    use super::*;

    #[derive(Default)]
    struct WarnCounter(Arc<AtomicUsize>);

    impl Subscriber for WarnCounter {
        fn enabled(&self, _: &Metadata<'_>) -> bool {
            true
        }
        fn new_span(&self, _: &Attributes<'_>) -> Id {
            Id::from_u64(1)
        }
        fn record(&self, _: &Id, _: &Record<'_>) {}
        fn record_follows_from(&self, _: &Id, _: &Id) {}
        fn event(&self, event: &Event<'_>) {
            if *event.metadata().level() == Level::WARN {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        fn enter(&self, _: &Id) {}
        fn exit(&self, _: &Id) {}
    }

    #[test]
    fn render_inline_pattern_warns_once_per_invalid_pattern() {
        let counter = WarnCounter::default();
        let warnings = Arc::clone(&counter.0);
        let at = OffsetDateTime::UNIX_EPOCH;
        tracing::subscriber::with_default(counter, || {
            render_inline_pattern(at, "'warn probe' qqq");
            render_inline_pattern(at, "'warn probe' qqq");
        });
        assert_eq!(warnings.load(Ordering::SeqCst), 1);
    }
}
