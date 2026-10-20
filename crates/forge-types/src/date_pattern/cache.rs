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
