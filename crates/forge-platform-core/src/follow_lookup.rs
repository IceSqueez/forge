use async_trait::async_trait;
use time::OffsetDateTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FollowStatus {
    FollowedSince(OffsetDateTime),
    NotFollowing,
    Hidden,
    Unavailable,
}

#[async_trait]
pub trait FollowLookup: Send + Sync {
    async fn follow_status(&self, viewer_id: &str) -> FollowStatus;
}
