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
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_slot_is_empty() {
        let slot: LiveSlot<String> = LiveSlot::new();

        assert!(slot.current().is_none());
        assert!(slot.take().is_none());
    }

    #[test]
    fn install_replaces_the_previous_value() {
        let slot = LiveSlot::new();
        slot.install(Arc::new(1));
        slot.install(Arc::new(2));

        assert_eq!(slot.current().as_deref(), Some(&2));
    }

    #[test]
    fn current_leaves_the_value_in_place() {
        let slot = LiveSlot::new();
        slot.install(Arc::new(1));

        assert!(slot.current().is_some());
        assert!(slot.current().is_some());
    }

    #[test]
    fn take_empties_the_slot_and_hands_back_the_value() {
        let slot = LiveSlot::new();
        slot.install(Arc::new(1));

        assert_eq!(slot.take().as_deref(), Some(&1));
        assert!(slot.current().is_none());
    }

    #[test]
    fn clones_share_one_slot() {
        let slot = LiveSlot::new();
        let other = slot.clone();
        slot.install(Arc::new(1));

        assert_eq!(other.current().as_deref(), Some(&1));
        other.take();
        assert!(slot.current().is_none());
    }

    #[test]
    fn a_slot_holds_a_trait_object() {
        let slot: LiveSlot<dyn std::fmt::Display + Send + Sync> = LiveSlot::new();
        slot.install(Arc::new("live"));

        assert_eq!(
            slot.current().map(|value| value.to_string()).as_deref(),
            Some("live")
        );
    }
}
