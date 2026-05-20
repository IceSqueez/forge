use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubStatus {
    Pending,
    Active,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct SubscriptionRecord {
    pub kind: String,
    pub version: String,
    pub status: SubStatus,
    pub subscription_id: Option<String>,
}

pub type SubscriptionTracker = Arc<RwLock<Vec<SubscriptionRecord>>>;
