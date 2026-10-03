use std::collections::HashSet;
use std::sync::Mutex;

use forge_events::Event;
use forge_storage::{StorageError, ViewerPlatform, ViewerRepo};
use forge_types::ChatViewer;

use crate::viewer_tracker::viewer_platform;

pub struct FirstChatLedger {
    seen: Mutex<HashSet<(ViewerPlatform, String)>>,
}

impl FirstChatLedger {
    pub async fn load(repo: &dyn ViewerRepo) -> Result<Self, StorageError> {
        let seen = repo
            .list()
            .await?
            .into_iter()
            .map(|viewer| (viewer.platform, viewer.viewer_id))
            .collect();
        Ok(Self {
            seen: Mutex::new(seen),
        })
    }

    pub(crate) fn stamp(&self, event: &mut Event) {
        if event.replay {
            return;
        }
        let Some(platform) = viewer_platform(event.source) else {
            return;
        };
        let Some(mut viewer) = ChatViewer::read(&event.payload) else {
            return;
        };
        let first = self
            .seen
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert((platform, viewer.id.clone()));
        if viewer.first_message != first {
            viewer.first_message = first;
            viewer.attach(&mut event.payload);
        }
    }
}
