use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use fluent::{FluentArgs, FluentBundle, FluentResource, FluentValue};

thread_local! {
    static BUNDLE: RefCell<Option<Rc<FluentBundle<FluentResource>>>> = const { RefCell::new(None) };

    /// Tracks keys already warned about in this thread to emit each warning once.
    static WARNED_KEYS: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

/// Replaces the active bundle for this thread. Subsequent `tr!` calls on this thread see the new
/// bundle. Other threads keep their previous bundle (caller is responsible for re-installing
/// per-thread if those threads also render UI — for iced's single-threaded render loop this is
/// called once on the main thread).
pub fn install_bundle(bundle: Rc<FluentBundle<FluentResource>>) {
    BUNDLE.with(|cell| {
        *cell.borrow_mut() = Some(bundle);
    });
    WARNED_KEYS.with(|cell| {
        cell.borrow_mut().clear();
    });
}

/// Missing key → returns the raw key string (debug builds also emit `tracing::warn!` once per
/// key). Dotted keys (`common.cancel`) auto-map to Fluent underscore IDs (`common_cancel`) —
/// Fluent reserves `.` for attribute access.
pub fn tr_lookup(key: &str, args: Option<&FluentArgs<'_>>) -> String {
    let fluent_id: std::borrow::Cow<'_, str> = if key.contains('.') {
        std::borrow::Cow::Owned(key.replace('.', "_"))
    } else {
        std::borrow::Cow::Borrowed(key)
    };

    BUNDLE.with(|cell| {
        let borrow = cell.borrow();
        let Some(bundle) = borrow.as_ref() else {
            warn_missing_once(key);
            return key.to_owned();
        };

        let Some(msg) = bundle.get_message(fluent_id.as_ref()) else {
            warn_missing_once(key);
            return key.to_owned();
        };

        let Some(pattern) = msg.value() else {
            warn_missing_once(key);
            return key.to_owned();
        };

        let mut errors = vec![];
        let formatted = bundle.format_pattern(pattern, args, &mut errors);
        if !errors.is_empty() {
            #[cfg(debug_assertions)]
            tracing::warn!(key, "fluent format errors: {:?}", errors);
        }
        formatted.into_owned()
    })
}

#[allow(dead_code)]
fn warn_missing_once(key: &str) {
    #[cfg(debug_assertions)]
    {
        WARNED_KEYS.with(|cell| {
            let mut warned = cell.borrow_mut();
            if !warned.contains(key) {
                warned.insert(key.to_owned());
                tracing::warn!(key, "tr!: key not found in active bundle");
            }
        });
    }
}

pub struct ArgsBuilder<'a>(FluentArgs<'a>);

impl<'a> Default for ArgsBuilder<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> ArgsBuilder<'a> {
    pub fn new() -> Self {
        Self(FluentArgs::new())
    }

    pub fn set(mut self, key: &'a str, value: impl Into<FluentValue<'a>>) -> Self {
        self.0.set(key, value);
        self
    }

    pub fn build(self) -> FluentArgs<'a> {
        self.0
    }
}

/// Translates a Fluent message key using the active thread-local bundle.
///
/// Two forms:
/// - `tr!("key")` — simple lookup with no arguments.
/// - `tr!("key", arg_name = value, ...)` — lookup with named Fluent arguments.
#[macro_export]
macro_rules! tr {
    ($key:expr) => {
        $crate::locale::tr_lookup($key, None)
    };
    ($key:expr, $($name:ident = $val:expr),+ $(,)?) => {{
        let args = $crate::locale::ArgsBuilder::new()
            $(.set(stringify!($name), $val))+
            .build();
        $crate::locale::tr_lookup($key, Some(&args))
    }};
}
