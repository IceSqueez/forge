use std::collections::HashMap;

use async_trait::async_trait;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AvatarLookup {
    Answered(HashMap<String, String>),
    Unavailable,
}

#[async_trait]
pub trait ViewerAvatarSource: Send + Sync {
    fn remembered(&self, _viewer_id: &str) -> Option<String> {
        None
    }

    fn batch_limit(&self) -> usize;

    async fn avatars(&self, viewer_ids: &[String]) -> AvatarLookup;
}
