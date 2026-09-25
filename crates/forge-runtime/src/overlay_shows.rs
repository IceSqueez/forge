use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use forge_storage::OverlayId;
use tokio::sync::oneshot;

use crate::overlay_service::{OverlayDelivery, OverlayFrameSink};

/// A memory guard on shows waiting behind the one on screen, not a pacing limit: a backlog below
/// it is always shown in full, however late.
pub const SHOW_QUEUE_CAPACITY: usize = 64;

/// No show holds its overlay longer than this, whatever duration it asked for.
pub const SHOW_CEILING: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShowEnd {
    Shown(OverlayDelivery),
    Cleared,
    Withdrawn,
}

#[derive(Debug)]
pub struct ShowTicket(oneshot::Receiver<ShowEnd>);

impl ShowTicket {
    /// Resolves once the show has left the screen or left the queue without being shown.
    pub async fn finished(self) -> ShowEnd {
        self.0.await.unwrap_or(ShowEnd::Withdrawn)
    }
}

pub(crate) struct Show {
    pub(crate) content: serde_json::Value,
    pub(crate) duration_ms: Option<u64>,
    pub(crate) window: Duration,
}

pub(crate) struct QueueFull;

struct Pending {
    show: Show,
    done: oneshot::Sender<ShowEnd>,
}

#[derive(Default)]
struct Lane {
    pending: VecDeque<Pending>,
    running: bool,
}

/// One presenter task per overlay with shows queued; it exits when its queue drains, so idle
/// overlays cost nothing. The map lock is never held across an await.
pub(crate) struct ShowSequencer {
    frames: Option<Arc<dyn OverlayFrameSink>>,
    lanes: Mutex<HashMap<OverlayId, Lane>>,
}

impl ShowSequencer {
    pub(crate) fn new(frames: Option<Arc<dyn OverlayFrameSink>>) -> Self {
        Self {
            frames,
            lanes: Mutex::default(),
        }
    }

    pub(crate) fn enqueue(
        self: &Arc<Self>,
        id: &OverlayId,
        show: Show,
    ) -> Result<ShowTicket, QueueFull> {
        let (done, ticket) = oneshot::channel();
        let start_presenter = {
            let mut lanes = self.lanes.lock().unwrap_or_else(PoisonError::into_inner);
            let lane = lanes.entry(id.clone()).or_default();
            if lane.pending.len() >= SHOW_QUEUE_CAPACITY {
                return Err(QueueFull);
            }
            lane.pending.push_back(Pending { show, done });
            !std::mem::replace(&mut lane.running, true)
        };
        if start_presenter {
            tokio::spawn(Arc::clone(self).present_all(id.clone()));
        }
        Ok(ShowTicket(ticket))
    }

    pub(crate) fn depth(&self, id: &OverlayId) -> usize {
        self.lanes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .map_or(0, |lane| lane.pending.len())
    }

    /// Drops every show still waiting for `id`, ending each with `end`; the one on screen runs
    /// out its window.
    pub(crate) fn drop_pending(&self, id: &OverlayId, end: ShowEnd) -> usize {
        let dropped: Vec<Pending> = {
            let mut lanes = self.lanes.lock().unwrap_or_else(PoisonError::into_inner);
            lanes
                .get_mut(id)
                .map(|lane| lane.pending.drain(..).collect())
                .unwrap_or_default()
        };
        let count = dropped.len();
        for pending in dropped {
            let _ = pending.done.send(end);
        }
        count
    }

    async fn present_all(self: Arc<Self>, id: OverlayId) {
        while let Some(next) = self.next(&id) {
            let delivery = self.present(&id, &next.show).await;
            tokio::time::sleep(next.show.window.min(SHOW_CEILING)).await;
            let _ = next.done.send(ShowEnd::Shown(delivery));
        }
    }

    async fn present(&self, id: &OverlayId, show: &Show) -> OverlayDelivery {
        let Some(frames) = &self.frames else {
            return OverlayDelivery::NoPage;
        };
        frames
            .deliver_content(id, show.content.clone(), show.duration_ms)
            .await
            .outcome()
    }

    fn next(&self, id: &OverlayId) -> Option<Pending> {
        let mut lanes = self.lanes.lock().unwrap_or_else(PoisonError::into_inner);
        let lane = lanes.get_mut(id)?;
        let next = lane.pending.pop_front();
        if next.is_none() {
            lanes.remove(id);
        }
        next
    }
}
