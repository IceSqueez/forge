use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use forge_net::TaskOwner;
use forge_overlay::kinds::chat::KIND_ID as CHAT_KIND_ID;
use forge_overlay::{ChatAvatarSlot, RowAvatar, page_avatar_url};
use forge_platform_core::{AvatarLookup, ViewerAvatarSource};
use forge_storage::OverlayRepo;
use forge_types::{PlatformId, RecoverMutex, RecoverRwLock};

use tokio::time::Instant;

use crate::overlays::overlay_service::OverlayFrameSink;

pub const KNOWN_AVATARS_CAPACITY: usize = 4096;
pub const WAITING_AVATARS_CAPACITY: usize = 512;
pub const AVATAR_BATCH_WINDOW: Duration = Duration::from_millis(150);
pub const AVATAR_BATCH_SPACING: Duration = Duration::from_secs(2);
const LOOKUP_BOUND: Duration = Duration::from_secs(10);
const UNAVAILABLE_PAUSE: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KnownAvatar {
    Image(String),
    NoImage,
    Unknown,
}

#[derive(Default)]
struct AvatarBook {
    known: HashMap<ChatAvatarSlot, Option<String>>,
    arrival: VecDeque<ChatAvatarSlot>,
    waiting: HashMap<PlatformId, BTreeSet<String>>,
    asked: HashSet<ChatAvatarSlot>,
    draining: HashSet<PlatformId>,
    paused_until: HashMap<PlatformId, Instant>,
}

impl AvatarBook {
    fn remember(&mut self, slot: ChatAvatarSlot, url: Option<String>) {
        if self.known.insert(slot.clone(), url).is_none() {
            self.arrival.push_back(slot);
        }
        while self.arrival.len() > KNOWN_AVATARS_CAPACITY {
            if let Some(oldest) = self.arrival.pop_front() {
                self.known.remove(&oldest);
            }
        }
    }

    fn forget_waiting(&mut self, platform: PlatformId) {
        if let Some(ids) = self.waiting.remove(&platform) {
            for user_id in ids {
                self.asked.remove(&ChatAvatarSlot { platform, user_id });
            }
        }
        self.draining.remove(&platform);
    }
}

#[derive(Clone, Default)]
pub struct ChatAvatars {
    sources: Arc<RwLock<HashMap<PlatformId, Arc<dyn ViewerAvatarSource>>>>,
    book: Arc<Mutex<AvatarBook>>,
}

impl ChatAvatars {
    pub fn register(&self, platform: PlatformId, source: Arc<dyn ViewerAvatarSource>) {
        self.sources.write_recover().insert(platform, source);
    }

    fn source(&self, platform: PlatformId) -> Option<Arc<dyn ViewerAvatarSource>> {
        self.sources.read_recover().get(&platform).cloned()
    }

    pub fn known(&self, slot: &ChatAvatarSlot) -> KnownAvatar {
        if let Some(known) = self.book.lock_recover().known.get(slot) {
            return known
                .clone()
                .map_or(KnownAvatar::NoImage, KnownAvatar::Image);
        }
        let remembered = self
            .source(slot.platform)
            .and_then(|source| source.remembered(&slot.user_id))
            .and_then(|raw| page_avatar_url(slot.platform, &raw));
        match remembered {
            Some(url) => {
                self.book
                    .lock_recover()
                    .remember(slot.clone(), Some(url.clone()));
                KnownAvatar::Image(url)
            }
            None => KnownAvatar::Unknown,
        }
    }

    pub fn ask(&self, slot: ChatAvatarSlot) -> bool {
        let mut book = self.book.lock_recover();
        let platform = slot.platform;
        if book
            .paused_until
            .get(&platform)
            .is_some_and(|until| Instant::now() < *until)
            || book.asked.contains(&slot)
            || book.waiting.get(&platform).map_or(0, BTreeSet::len) >= WAITING_AVATARS_CAPACITY
        {
            return false;
        }
        book.waiting
            .entry(platform)
            .or_default()
            .insert(slot.user_id.clone());
        book.asked.insert(slot);
        book.draining.insert(platform)
    }

    pub async fn resolve_batch(&self, platform: PlatformId) -> Option<Vec<RowAvatar>> {
        let source = self.source(platform);
        let user_ids: Vec<String> = {
            let mut book = self.book.lock_recover();
            let Some(source) = &source else {
                book.forget_waiting(platform);
                return None;
            };
            let limit = source.batch_limit().max(1);
            let waiting = book.waiting.entry(platform).or_default();
            let taken: Vec<String> = waiting.iter().take(limit).cloned().collect();
            for user_id in &taken {
                waiting.remove(user_id);
            }
            if taken.is_empty() {
                book.draining.remove(&platform);
                return None;
            }
            taken
        };
        let source = source?;
        let lookup = tokio::time::timeout(LOOKUP_BOUND, source.avatars(&user_ids))
            .await
            .unwrap_or(AvatarLookup::Unavailable);
        let mut book = self.book.lock_recover();
        let mut fills = Vec::new();
        for user_id in user_ids {
            let slot = ChatAvatarSlot { platform, user_id };
            book.asked.remove(&slot);
            let AvatarLookup::Answered(found) = &lookup else {
                continue;
            };
            let url = found
                .get(&slot.user_id)
                .and_then(|raw| page_avatar_url(platform, raw));
            book.remember(slot.clone(), url.clone());
            if let Some(url) = url {
                fills.push(RowAvatar { slot, url });
            }
        }
        if lookup == AvatarLookup::Unavailable {
            tracing::debug!(platform = %platform.as_str(), "viewer avatars unavailable; pausing lookups");
            book.paused_until
                .insert(platform, Instant::now() + UNAVAILABLE_PAUSE);
            book.forget_waiting(platform);
        }
        Some(fills)
    }

    pub(crate) fn spawn_drain(
        &self,
        platform: PlatformId,
        repo: Arc<dyn OverlayRepo>,
        frames: Arc<dyn OverlayFrameSink>,
        tasks: &TaskOwner,
    ) {
        let avatars = self.clone();
        tasks.spawn_until_stopped(async move {
            tokio::time::sleep(AVATAR_BATCH_WINDOW).await;
            while let Some(fills) = avatars.resolve_batch(platform).await {
                if !fills.is_empty() {
                    push_fills(repo.as_ref(), frames.as_ref(), &fills).await;
                }
                tokio::time::sleep(AVATAR_BATCH_SPACING).await;
            }
        });
    }
}

async fn push_fills(repo: &dyn OverlayRepo, frames: &dyn OverlayFrameSink, fills: &[RowAvatar]) {
    let definitions = match repo.list().await {
        Ok(definitions) => definitions,
        Err(error) => {
            tracing::warn!(%error, "overlays unreadable; chat avatars not pushed");
            return;
        }
    };
    for definition in definitions
        .iter()
        .filter(|definition| definition.enabled && definition.kind_id == CHAT_KIND_ID)
    {
        for fill in fills {
            frames.deliver_avatar(&definition.id, fill).await;
        }
    }
}
