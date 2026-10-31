use super::state::BuiltinPorts;
use super::{IntegrationDetail, VIEWER_DELTA_WINDOW_MINUTES};
use crate::async_bridge;
use crate::runtime_event::{self, RuntimeEvent};
use crate::window_presence::PresenceGate;
use forge_platform_core::{
    BuiltinHealth, CollectionRevisions, ConnectionState, HealthDelta, HealthMetricId, HealthText,
    HealthValue, RevisionWait,
};
use forge_runtime::EventBus;
use futures_util::{Stream, StreamExt as _, stream};
use gpui::{Context, Task};
use std::sync::Arc;
use std::time::{Duration, Instant};

const SECONDS_PER_MINUTE: u64 = 60;

const VIEWER_DELTA_WINDOW: Duration =
    Duration::from_secs(VIEWER_DELTA_WINDOW_MINUTES * SECONDS_PER_MINUTE);

const VIEWER_RING_CAP: usize = 256;
const VIEWER_RING_SLACK: Duration = Duration::from_secs(60);
const TOKEN_COUNTDOWN_REPAINT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DetailChange {
    Content,
    Status,
    QuickActions,
}

fn change_stream(
    revisions: CollectionRevisions,
    change: DetailChange,
) -> impl Stream<Item = DetailChange> {
    stream::unfold(revisions, move |mut revisions| async move {
        (revisions.changed().await == RevisionWait::Changed).then_some((change, revisions))
    })
}

fn merged_changes(ports: &BuiltinPorts) -> impl Stream<Item = DetailChange> + use<> {
    let handles = [
        (ports.content.content_revisions(), DetailChange::Content),
        (ports.status.status_revisions(), DetailChange::Status),
        (ports.quick.revisions(), DetailChange::QuickActions),
    ];
    stream::select_all(handles.into_iter().filter_map(|(revisions, change)| {
        revisions.map(|revisions| change_stream(revisions, change).boxed())
    }))
}

fn viewer_reading(value: &HealthValue) -> Option<u64> {
    match value {
        HealthValue::Text {
            primary: HealthText::Count(count),
            ..
        } if *count > 0 => Some(*count),
        _ => None,
    }
}

impl IntegrationDetail {
    pub(super) fn apply_health_delta(&mut self, delta: HealthDelta, cx: &mut Context<Self>) {
        let Some(metric) = self
            .shown
            .health_metrics
            .iter_mut()
            .find(|metric| metric.id == delta.metric)
        else {
            tracing::warn!(metric = %delta.metric, "health delta for an undeclared metric");
            return;
        };
        metric.value = delta.new_value;
        if delta.metric == HealthMetricId::VIEWERS {
            self.sample_viewers();
        }
        self.shown.connection = self.ports.status.connection();
        self.shown.header_actions = self.ports.status.header_actions();
        self.sync_token_countdown(cx);
        self.refresh_content(cx);
    }

    fn refresh_content(&mut self, cx: &mut Context<Self>) {
        self.shown.sections = self.ports.content.sections();
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

    pub(super) fn spawn_change_bridge(ports: &BuiltinPorts, cx: &mut Context<Self>) -> Task<()> {
        let mut changes = merged_changes(ports);
        cx.spawn(async move |this, cx| {
            while let Some(change) = changes.next().await {
                if this
                    .update(cx, |this, cx| this.apply_change(change, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
    }

    pub(super) fn apply_change(&mut self, change: DetailChange, cx: &mut Context<Self>) {
        match change {
            DetailChange::Content => self.refresh_content(cx),
            DetailChange::Status | DetailChange::QuickActions => self.reload(cx),
        }
    }

    pub(super) fn sync_token_countdown(&mut self, cx: &mut Context<Self>) {
        if !self.token_countdown_shown() {
            self.background._token_countdown_repaint = None;
            return;
        }
        if self.background._token_countdown_repaint.is_none() {
            self.background._token_countdown_repaint = Some(Self::spawn_token_countdown(cx));
        }
    }

    fn token_countdown_shown(&self) -> bool {
        self.shown.connection == ConnectionState::Connected && self.token_remaining().is_some()
    }

    fn spawn_token_countdown(cx: &mut Context<Self>) -> Task<()> {
        let mut presence = PresenceGate::of(cx);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(TOKEN_COUNTDOWN_REPAINT)
                    .await;
                presence.until_visible().await;
                let alive = this.update(cx, |this, cx| {
                    cx.notify();
                    this.sync_token_countdown(cx);
                });
                if alive.is_err() {
                    break;
                }
            }
        })
    }

    pub(super) fn spawn_reauth_watch(bus: &Arc<EventBus>, cx: &mut Context<Self>) -> Task<()> {
        let mut sub = bus.subscribe();
        cx.spawn(async move |this, cx| {
            while let async_bridge::EventBatch::Ready(batch) =
                async_bridge::recv_event_batch(&mut sub).await
            {
                let reauth = batch.iter().any(|event| {
                    matches!(
                        runtime_event::decode(event),
                        Some(RuntimeEvent::ReauthRequired(_))
                    )
                });
                if !reauth {
                    continue;
                }
                let alive = this.update(cx, |this, cx| {
                    this.shown.twitch_reauth_required = true;
                    cx.notify();
                });
                if alive.is_err() {
                    break;
                }
            }
        })
    }

    pub(super) fn sample_viewers(&mut self) {
        let reading = self
            .shown
            .health_metrics
            .iter()
            .find(|metric| metric.id == HealthMetricId::VIEWERS)
            .and_then(|metric| viewer_reading(&metric.value));
        if let Some(count) = reading {
            self.push_viewer_sample(count, Instant::now());
        }
    }

    pub(super) fn push_viewer_sample(&mut self, count: u64, now: Instant) {
        self.live.viewer_samples.push_back((now, count));
        while self.live.viewer_samples.len() > VIEWER_RING_CAP {
            self.live.viewer_samples.pop_front();
        }
        let cutoff = now.checked_sub(VIEWER_DELTA_WINDOW + VIEWER_RING_SLACK);
        if let Some(cutoff) = cutoff {
            while self
                .live
                .viewer_samples
                .front()
                .is_some_and(|(t, _)| *t < cutoff)
            {
                self.live.viewer_samples.pop_front();
            }
        }
    }

    pub(super) fn viewer_delta(&self) -> Option<i64> {
        let (newest_t, newest) = self.live.viewer_samples.back().copied()?;
        let oldest = self.live.viewer_samples.front().copied()?;
        let target = newest_t.checked_sub(VIEWER_DELTA_WINDOW)?;
        let baseline = self
            .live
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
