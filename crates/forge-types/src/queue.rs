use crate::ids::QueueId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Queue {
    pub id: QueueId,
    pub name: String,
    pub description: String,
    pub blocking: bool,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn queue_serde_roundtrip() {
        let q = Queue {
            id: QueueId::new(),
            name: "Default".to_string(),
            description: "Catch-all".to_string(),
            blocking: false,
        };
        let json = serde_json::to_string(&q).unwrap();
        let back: Queue = serde_json::from_str(&json).unwrap();
        assert_eq!(q, back);
    }

    #[test]
    fn queue_blocking_flag_persists() {
        let q = Queue {
            id: QueueId::new(),
            name: "Slow".to_string(),
            description: String::new(),
            blocking: true,
        };
        let json = serde_json::to_string(&q).unwrap();
        let back: Queue = serde_json::from_str(&json).unwrap();
        assert!(back.blocking);
    }
}
