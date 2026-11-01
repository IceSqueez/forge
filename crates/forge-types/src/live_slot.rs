use crate::RecoverRwLock;
use std::sync::{Arc, RwLock};

pub struct LiveSlot<T: ?Sized>(Arc<RwLock<Option<Arc<T>>>>);

impl<T: ?Sized> Clone for LiveSlot<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T: ?Sized> Default for LiveSlot<T> {
    fn default() -> Self {
        Self(Arc::new(RwLock::new(None)))
    }
}

impl<T: ?Sized> LiveSlot<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn install(&self, value: Arc<T>) {
        *self.0.write_recover() = Some(value);
    }

    pub fn take(&self) -> Option<Arc<T>> {
        self.0.write_recover().take()
    }

    pub fn current(&self) -> Option<Arc<T>> {
        self.0.read_recover().clone()
    }
}
