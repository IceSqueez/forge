use std::collections::{HashMap, VecDeque};

use crate::{Priority, QueuedOrderEntry, RequestId, SpeakEvent, SpeakRequest};

#[derive(Default)]
pub(crate) struct PendingQueues {
    pub(crate) high: VecDeque<SpeakRequest>,
    pub(crate) normal: VecDeque<SpeakRequest>,
    per_user: HashMap<String, usize>,
}

impl PendingQueues {
    pub(crate) fn len(&self) -> usize {
        self.high.len() + self.normal.len()
    }

    pub(crate) fn user_count(&self, viewer_id: &str) -> usize {
        self.per_user.get(viewer_id).copied().unwrap_or(0)
    }

    pub(crate) fn next(&self) -> Option<&SpeakRequest> {
        self.high.front().or_else(|| self.normal.front())
    }

    pub(crate) fn push(&mut self, request: SpeakRequest) {
        *self.per_user.entry(request.viewer_id.clone()).or_insert(0) += 1;
        match request.priority {
            Priority::High => self.high.push_back(request),
            Priority::Normal => self.normal.push_back(request),
        }
    }

    pub(crate) fn pop_next(&mut self) -> Option<SpeakRequest> {
        let request = self.high.pop_front().or_else(|| self.normal.pop_front())?;
        self.release_user_slot(&request.viewer_id);
        Some(request)
    }

    pub(crate) fn remove(&mut self, request_id: &RequestId) -> Option<SpeakRequest> {
        let request = self.take(request_id)?;
        self.release_user_slot(&request.viewer_id);
        Some(request)
    }

    pub(crate) fn promote(&mut self, request_id: &RequestId) -> bool {
        match self.take(request_id) {
            Some(request) => {
                self.high.push_front(request);
                true
            }
            None => false,
        }
    }

    pub(crate) fn reorder(
        &mut self,
        request_id: &RequestId,
        before: Option<&RequestId>,
    ) -> Option<SpeakRequest> {
        let anchor_in_high = match before {
            Some(anchor_id) if anchor_id == request_id => return None,
            Some(anchor_id) if Self::holds(&self.high, anchor_id) => true,
            Some(anchor_id) if Self::holds(&self.normal, anchor_id) => false,
            Some(_) => return None,
            None => false,
        };
        let was_high = Self::holds(&self.high, request_id);
        let request = self.take(request_id)?;
        let moved = request.clone();
        match before {
            None if was_high => self.high.push_back(request),
            None => self.normal.push_back(request),
            Some(anchor_id) => {
                let queue = if anchor_in_high {
                    &mut self.high
                } else {
                    &mut self.normal
                };
                let position = queue
                    .iter()
                    .position(|queued| &queued.request_id == anchor_id)
                    .unwrap_or(queue.len());
                queue.insert(position, request);
            }
        }
        Some(moved)
    }

    pub(crate) fn drain_all(&mut self) -> Vec<SpeakRequest> {
        self.per_user.clear();
        self.high.drain(..).chain(self.normal.drain(..)).collect()
    }

    pub(crate) fn changed_event(&self) -> SpeakEvent {
        let order: Vec<QueuedOrderEntry> = self
            .high
            .iter()
            .map(|queued| QueuedOrderEntry {
                request_id: queued.request_id.clone(),
                is_high_priority: true,
            })
            .chain(self.normal.iter().map(|queued| QueuedOrderEntry {
                request_id: queued.request_id.clone(),
                is_high_priority: false,
            }))
            .collect();
        SpeakEvent::QueueChanged {
            queue_len: order.len(),
            order,
        }
    }

    fn holds(queue: &VecDeque<SpeakRequest>, request_id: &RequestId) -> bool {
        queue.iter().any(|queued| &queued.request_id == request_id)
    }

    fn take(&mut self, request_id: &RequestId) -> Option<SpeakRequest> {
        if let Some(position) = self
            .high
            .iter()
            .position(|queued| &queued.request_id == request_id)
        {
            return self.high.remove(position);
        }
        let position = self
            .normal
            .iter()
            .position(|queued| &queued.request_id == request_id)?;
        self.normal.remove(position)
    }

    fn release_user_slot(&mut self, viewer_id: &str) {
        if let Some(count) = self.per_user.get_mut(viewer_id) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.per_user.remove(viewer_id);
            }
        }
    }
}
