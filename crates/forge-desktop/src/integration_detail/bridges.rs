use super::{IntegrationDetail, VIEWER_DELTA_WINDOW_MINUTES};
use crate::async_bridge;
use crate::runtime_event::{self, RuntimeEvent};
use crate::window_presence::PresenceGate;
use forge_platform_core::{BuiltinHealth, HealthDelta, QuickActions, RevisionWait};
use forge_runtime::{EventBus, LiveViewerAggregatorHandle, LiveViewerCount};
use futures_util::StreamExt as _;
use gpui::{Context, Task};
use std::sync::Arc;
use std::time::{Duration, Instant};

const SECONDS_PER_MINUTE: u64 = 60;

const VIEWER_DELTA_WINDOW: Duration =
    Duration::from_secs(VIEWER_DELTA_WINDOW_MINUTES * SECONDS_PER_MINUTE);

const VIEWER_RING_CAP: usize = 256;
const VIEWER_RING_SLACK: Duration = Duration::from_secs(60);
const DETAIL_TICK: Duration = Duration::from_secs(30);

impl IntegrationDetail {
    pub(super) fn apply_health_delta(&mut self, delta: HealthDelta, cx: &mut Context<Self>) {
        let Some(metric) = self
            .health_metrics
            .iter_mut()
            .find(|metric| metric.id == delta.metric)
        else {
            tracing::warn!(metric = %delta.metric, "health delta for an undeclared metric");
            return;
        };
        metric.value = delta.new_value;
        self.connection = self.status.connection();
        self.header_actions = self.status.header_actions();
        self.refresh_content(cx);
    }

    fn refresh_content(&mut self, cx: &mut Context<Self>) {
        self.sections = self.content.sections();
        cx.notify();
    }

    pub(super) fn spawn_health_bridge(
        health: &Arc<dyn BuiltinHealth>,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let mut health_stream = health.stream();
        cx.spawn(async move |this, cx| {
            while let Some(delta) = health_stream.next().await {
                if this
                    .update(cx, |detail, cx| detail.apply_health_delta(delta, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
    }

    pub(super) fn spawn_quick_action_bridge(
        quick: &dyn QuickActions,
        cx: &mut Context<Self>,
    ) -> Option<Task<()>> {
        let mut revisions = quick.revisions()?;
        Some(cx.spawn(async move |this, cx| {
            while revisions.changed().await == RevisionWait::Changed {
                if this.update(cx, |this, cx| this.reload(cx)).is_err() {
                    break;
                }
            }
        }))
    }

    pub(super) fn spawn_obs_content_watch(bus: &Arc<EventBus>, cx: &mut Context<Self>) -> Task<()> {
        let mut sub = bus.subscribe();
        cx.spawn(async move |this, cx| {
            while let async_bridge::EventBatch::Ready(batch) =
                async_bridge::recv_event_batch(&mut sub).await
            {
                if !batch.iter().any(runtime_event::changes_obs_content) {
                    continue;
                }
                if this
                    .update(cx, |this, cx| this.refresh_content(cx))
                    .is_err()
                {
                    break;
                }
            }
        })
    }

    pub(super) fn spawn_eventsub_tally(bus: &Arc<EventBus>, cx: &mut Context<Self>) -> Task<()> {
        let mut sub = bus.subscribe();
        cx.spawn(async move |this, cx| {
            while let async_bridge::EventBatch::Ready(batch) =
                async_bridge::recv_event_batch(&mut sub).await
            {
                let reauth = batch.iter().any(|e| {
                    matches!(
                        runtime_event::decode(e),
                        Some(RuntimeEvent::ReauthRequired(_))
                    )
                });
                let tails: Vec<String> = batch
                    .iter()
                    .filter_map(|e| runtime_event::twitch_topic(e).map(str::to_owned))
                    .collect();
                if tails.is_empty() && !reauth {
                    continue;
                }
                if this
                    .update(cx, |this, cx| {
                        if reauth {
                            this.twitch_reauth_required = true;
                        }
                        this.apply_eventsub_tally(tails, cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
    }

    pub(super) fn spawn_descriptor_reload_watch(
        bus: &Arc<EventBus>,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let mut sub = bus.subscribe();
        cx.spawn(async move |this, cx| {
            while let async_bridge::EventBatch::Ready(batch) =
                async_bridge::recv_event_batch(&mut sub).await
            {
                if !batch.iter().any(runtime_event::rebuilds_descriptors) {
                    continue;
                }
                if this.update(cx, |this, cx| this.reload(cx)).is_err() {
                    break;
                }
            }
        })
    }

    pub(super) fn spawn_viewer_sampler(
        live_viewers: &LiveViewerAggregatorHandle,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let mut stream = live_viewers.subscribe();
        cx.spawn(async move |this, cx| {
            while let Some(count) = stream.next().await {
                let n = match count {
                    LiveViewerCount::Reporting(n) => n,
                    LiveViewerCount::Empty => continue,
                };
                if this
                    .update(cx, |this, cx| this.push_viewer_sample(n, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
    }

    pub(super) fn spawn_detail_ticker(cx: &mut Context<Self>) -> Task<()> {
        let mut presence = PresenceGate::of(cx);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(DETAIL_TICK).await;
                presence.until_visible().await;
                let alive = this.update(cx, |this, cx| {
                    this.sections = this.content.sections();
                    cx.notify();
                });
                if alive.is_err() {
                    break;
                }
            }
        })
    }

    fn apply_eventsub_tally(&mut self, tails: Vec<String>, cx: &mut Context<Self>) {
        for tail in tails {
            *self.eventsub_tally.entry(tail).or_insert(0) += 1;
        }
        cx.notify();
    }

    fn push_viewer_sample(&mut self, count: u64, cx: &mut Context<Self>) {
        let now = Instant::now();
        self.viewer_samples.push_back((now, count));
        while self.viewer_samples.len() > VIEWER_RING_CAP {
            self.viewer_samples.pop_front();
        }
        let cutoff = now.checked_sub(VIEWER_DELTA_WINDOW + VIEWER_RING_SLACK);
        if let Some(cutoff) = cutoff {
            while self
                .viewer_samples
                .front()
                .is_some_and(|(t, _)| *t < cutoff)
            {
                self.viewer_samples.pop_front();
            }
        }
        cx.notify();
    }

    pub(super) fn viewer_delta(&self) -> Option<i64> {
        let (newest_t, newest) = self.viewer_samples.back().copied()?;
        let oldest = self.viewer_samples.front().copied()?;
        let target = newest_t.checked_sub(VIEWER_DELTA_WINDOW)?;
        let baseline = self
            .viewer_samples
            .iter()
            .rev()
            .find(|(t, _)| *t <= target)
            .copied()
            .unwrap_or(oldest);
        Some(
            i64::try_from(newest).unwrap_or(i64::MAX)
                - i64::try_from(baseline.1).unwrap_or(i64::MAX),
        )
    }
}
