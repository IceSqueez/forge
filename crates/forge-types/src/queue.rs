use crate::ids::{DEFAULT_QUEUE_ID, QueueId};
use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Queue {
    pub id: QueueId,
    pub name: String,
    pub description: String,
    pub concurrency: NonZeroU32,
}

impl Queue {
    pub fn is_serial(&self) -> bool {
        self.concurrency == NonZeroU32::MIN
    }

    pub fn is_default(&self) -> bool {
        self.id == DEFAULT_QUEUE_ID
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn queue_json(concurrency: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "id": QueueId::new(),
            "name": "q",
            "description": "",
            "concurrency": concurrency,
        })
    }

    #[test]
    fn a_stored_queue_keeps_any_positive_concurrency() {
        for concurrency in [1, 2, u32::MAX] {
            let queue: Queue = serde_json::from_value(queue_json(concurrency.into())).unwrap();
            assert_eq!(queue.concurrency.get(), concurrency);
        }
    }

    #[test]
    fn a_stored_queue_with_no_slots_is_refused() {
        let error = serde_json::from_value::<Queue>(queue_json(0.into())).unwrap_err();
        assert!(
            error.to_string().contains("nonzero"),
            "unexpected refusal: {error}"
        );
    }

    #[test]
    fn a_stored_concurrency_outside_u32_is_refused() {
        for concurrency in [serde_json::json!(-1), serde_json::json!(u64::MAX)] {
            assert!(
                serde_json::from_value::<Queue>(queue_json(concurrency.clone())).is_err(),
                "{concurrency}"
            );
        }
    }
}
