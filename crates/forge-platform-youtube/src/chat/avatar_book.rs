use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_platform_core::{AvatarLookup, ViewerAvatarSource};
use forge_types::RecoverMutex;

const AUTHOR_DETAILS_KEY: &str = "authorDetails";
const CHANNEL_ID_KEY: &str = "channelId";
const PROFILE_IMAGE_KEY: &str = "profileImageUrl";
const REMEMBERED_AUTHORS: usize = 4096;

#[derive(Default)]
struct Book {
    images: HashMap<String, String>,
    arrival: VecDeque<String>,
}

#[derive(Clone, Default)]
pub(crate) struct AvatarBook(Arc<Mutex<Book>>);

impl AvatarBook {
    pub(crate) fn record(&self, item: &serde_json::Value) {
        let Some(author) = item.get(AUTHOR_DETAILS_KEY) else {
            return;
        };
        let (Some(channel_id), Some(image)) = (
            author
                .get(CHANNEL_ID_KEY)
                .and_then(serde_json::Value::as_str),
            author
                .get(PROFILE_IMAGE_KEY)
                .and_then(serde_json::Value::as_str),
        ) else {
            return;
        };
        if channel_id.is_empty() || image.is_empty() {
            return;
        }
        let mut book = self.0.lock_recover();
        if book
            .images
            .insert(channel_id.to_owned(), image.to_owned())
            .is_none()
        {
            book.arrival.push_back(channel_id.to_owned());
        }
        while book.arrival.len() > REMEMBERED_AUTHORS {
            if let Some(oldest) = book.arrival.pop_front() {
                book.images.remove(&oldest);
            }
        }
    }

    fn image_of(&self, channel_id: &str) -> Option<String> {
        self.0.lock_recover().images.get(channel_id).cloned()
    }
}

#[async_trait]
impl ViewerAvatarSource for AvatarBook {
    fn remembered(&self, viewer_id: &str) -> Option<String> {
        self.image_of(viewer_id)
    }

    fn batch_limit(&self) -> usize {
        REMEMBERED_AUTHORS
    }

    async fn avatars(&self, viewer_ids: &[String]) -> AvatarLookup {
        AvatarLookup::Answered(
            viewer_ids
                .iter()
                .filter_map(|id| Some((id.clone(), self.image_of(id)?)))
                .collect(),
        )
    }
}
