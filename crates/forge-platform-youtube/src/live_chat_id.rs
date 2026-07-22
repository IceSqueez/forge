use std::sync::Arc;

/// `std::sync::Mutex`: the critical section is a single clone, never held across an `await`.
#[derive(Debug, Clone, Default)]
pub struct LiveChatIdHandle {
    inner: Arc<std::sync::Mutex<Option<String>>>,
}

impl LiveChatIdHandle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, id: Option<String>) {
        if let Ok(mut guard) = self.inner.lock() {
            *guard = id;
        }
    }

    pub fn get(&self) -> Option<String> {
        self.inner.lock().ok().and_then(|g| g.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clone_shares_inner_state() {
        let h1 = LiveChatIdHandle::new();
        let h2 = h1.clone();
        h1.set(Some("shared".to_owned()));
        assert_eq!(h2.get().as_deref(), Some("shared"));
    }
}
