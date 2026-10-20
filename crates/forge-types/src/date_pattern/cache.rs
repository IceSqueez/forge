use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, LazyLock, PoisonError, RwLock};

use super::{DatePattern, DatePatternError};

const PATTERN_CACHE_CAPACITY: usize = 256;

pub(super) type ParsedPattern = Arc<Result<DatePattern, DatePatternError>>;

pub(super) struct CachedPattern {
    pub(super) parsed: ParsedPattern,
    pub(super) newly_parsed: bool,
}

#[derive(Default)]
struct PatternCache {
    entries: HashMap<String, ParsedPattern>,
    insertion_order: VecDeque<String>,
}

impl PatternCache {
    fn insert(&mut self, pattern: &str, parsed: ParsedPattern) {
        if self.entries.len() >= PATTERN_CACHE_CAPACITY
            && let Some(oldest) = self.insertion_order.pop_front()
        {
            self.entries.remove(&oldest);
        }
        self.entries.insert(pattern.to_owned(), parsed);
        self.insertion_order.push_back(pattern.to_owned());
    }
}

static PATTERN_CACHE: LazyLock<RwLock<PatternCache>> = LazyLock::new(RwLock::default);

pub(super) fn cached_pattern(pattern: &str) -> CachedPattern {
    if let Some(parsed) = PATTERN_CACHE
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .entries
        .get(pattern)
    {
        return CachedPattern {
            parsed: Arc::clone(parsed),
            newly_parsed: false,
        };
    }
    let parsed = Arc::new(DatePattern::parse(pattern));
    let mut cache = PATTERN_CACHE
        .write()
        .unwrap_or_else(PoisonError::into_inner);
    if let Some(existing) = cache.entries.get(pattern) {
        return CachedPattern {
            parsed: Arc::clone(existing),
            newly_parsed: false,
        };
    }
    cache.insert(pattern, Arc::clone(&parsed));
    CachedPattern {
        parsed,
        newly_parsed: true,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn filled_to_capacity() -> PatternCache {
        let mut cache = PatternCache::default();
        for index in 0..PATTERN_CACHE_CAPACITY {
            let pattern = format!("'{index}' HH");
            cache.insert(&pattern, Arc::new(DatePattern::parse(&pattern)));
        }
        cache
    }

    #[test]
    fn cached_pattern_reports_newly_parsed_only_on_the_first_lookup() {
        let pattern = "'first lookup probe' HH:mm";
        assert!(cached_pattern(pattern).newly_parsed);
        assert!(!cached_pattern(pattern).newly_parsed);
    }

    #[test]
    fn cached_pattern_keeps_an_invalid_pattern_and_returns_the_same_error() {
        let pattern = "'invalid probe' qqq";
        let first = cached_pattern(pattern);
        let second = cached_pattern(pattern);
        assert!(first.parsed.is_err());
        assert!(!second.newly_parsed);
        assert!(Arc::ptr_eq(&first.parsed, &second.parsed));
    }

    #[test]
    fn pattern_cache_at_capacity_keeps_every_entry() {
        let cache = filled_to_capacity();
        assert_eq!(cache.entries.len(), PATTERN_CACHE_CAPACITY);
        assert!(cache.entries.contains_key("'0' HH"));
    }

    #[test]
    fn pattern_cache_over_capacity_evicts_in_insertion_order() {
        let mut cache = filled_to_capacity();
        for pattern in ["'extra one' HH", "'extra two' HH"] {
            cache.insert(pattern, Arc::new(DatePattern::parse(pattern)));
        }
        assert_eq!(cache.entries.len(), PATTERN_CACHE_CAPACITY);
        assert!(!cache.entries.contains_key("'0' HH"));
        assert!(!cache.entries.contains_key("'1' HH"));
        assert!(cache.entries.contains_key("'2' HH"));
        assert!(cache.entries.contains_key("'extra two' HH"));
    }
}
