use super::clients_section::SUBSCRIBE_ALL_LABEL;
use super::overlay_section::{OverlayListing, scan_overlay_root};
use super::{
    ClientLiveness, OwnedClientRow, OwnedSubscriptionChip, RECENT_CLIENT_WINDOW_SECONDS,
    ServerConsoleView, ServerStats, SubscriptionChipKind,
};
use crate::async_bridge;
use crate::repo_failure::RepoFailure;
use crate::server_bearer;
use crate::window_presence::PresenceGate;
use forge_components::fmt_uptime_short;
use forge_events::EventSource;
use forge_server::{
    ConnectedClientSnapshot, EventFilterSnapshot, ServerSettings, ServerSnapshot,
    resolve_routable_host,
};
use forge_types::Secret;
use gpui::Context;
use std::sync::Arc;
use std::time::Duration;

const POLL_INTERVAL: Duration = Duration::from_secs(1);
const MAX_THROUGHPUT_SAMPLES: usize = 60;

pub(super) struct ServerPoll {
    pub(super) snapshot: ServerSnapshot,
    pub(super) bind_address: Option<String>,
    pub(super) overlay: Option<OverlayListing>,
}

pub(super) fn count_recent_clients(clients: &[ConnectedClientSnapshot]) -> usize {
    clients
        .iter()
        .filter(|c| c.uptime_seconds >= 0 && c.uptime_seconds <= RECENT_CLIENT_WINDOW_SECONDS)
        .count()
}

pub(super) fn client_row_from_snapshot(client: &ConnectedClientSnapshot) -> OwnedClientRow {
    let liveness = if client.events_per_second > 0.0 {
        ClientLiveness::Active
    } else {
        ClientLiveness::Idle
    };
    OwnedClientRow {
        id: client.id,
        key: client.id.to_string(),
        identification: client.identification.clone(),
        origin_label: format!("{} · {}", client.remote_addr, client.client_type),
        liveness,
        subscriptions: client.subscriptions.iter().map(subscription_chip).collect(),
        events_per_second: client.events_per_second,
        uptime_short: fmt_uptime_short(client.uptime_seconds.max(0) as u64),
    }
}

pub(super) fn subscription_chip(filter: &EventFilterSnapshot) -> OwnedSubscriptionChip {
    let source_wildcard = filter.source == "*";
    let kind_wildcard = filter.kind == "*";

    if source_wildcard && kind_wildcard {
        return OwnedSubscriptionChip {
            label: SUBSCRIBE_ALL_LABEL.to_owned(),
            kind: SubscriptionChipKind::All,
        };
    }

    if source_wildcard {
        return OwnedSubscriptionChip {
            label: filter.kind.clone(),
            kind: SubscriptionChipKind::Unknown,
        };
    }

    let source =
        serde_json::from_value::<EventSource>(serde_json::Value::String(filter.source.clone()))
            .ok();
    let source_label = source.map(event_source_label).unwrap_or(&filter.source);
    let label = if kind_wildcard {
        format!("{source_label}.*")
    } else {
        format!("{source_label}.{}", filter.kind)
    };
    OwnedSubscriptionChip {
        label,
        kind: match source {
            Some(source) => SubscriptionChipKind::Source(source),
            None => SubscriptionChipKind::Unknown,
        },
    }
}

fn event_source_label(source: EventSource) -> &'static str {
    match source {
        EventSource::Twitch => "twitch",
        EventSource::YouTube => "youtube",
        EventSource::Kick => "kick",
        EventSource::Core => "core",
        EventSource::Rhai => "rhai",
        EventSource::Http => "http",
        EventSource::Obs => "obs",
        EventSource::VTube => "vtube",
        EventSource::Discord => "discord",
        EventSource::Midi => "midi",
        EventSource::Hotkey => "hotkey",
        EventSource::Timer => "timer",
        EventSource::Server => "server",
        EventSource::Audio => "audio",
        EventSource::Donation => "donation",
    }
}

impl ServerConsoleView {
    pub(super) fn fetch_token(&self, cx: &mut Context<Self>) {
        let credentials = Arc::clone(&self.credentials);
        let server = self.server.clone();
        async_bridge::run_async(
            &self.rt_handle,
            server_bearer::current_bearer_token(server, credentials),
            |this, result: Result<Option<Secret>, RepoFailure>, cx| match result {
                Ok(Some(token)) => {
                    this.bearer_token = token;
                    cx.notify();
                }
                Ok(None) => {}
                Err(reason) => server_bearer::token_load_failed(&reason.text(), cx),
            },
            cx,
        );
    }

    pub(super) fn fetch_enabled(&self, cx: &mut Context<Self>) {
        let settings = Arc::clone(&self.settings);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                ServerSettings::load(settings.as_ref())
                    .await
                    .map(|loaded| loaded.enabled)
            },
            |this, loaded, cx| match loaded {
                Ok(enabled) => {
                    this.enabled = enabled;
                    cx.notify();
                }
                Err(error) => {
                    tracing::warn!(error = %error, "server enabled setting unreadable");
                }
            },
            cx,
        );
    }

    fn resolve_routable_host_for(&mut self, bind_address: String, cx: &mut Context<Self>) {
        self.routable_host_resolved_for = Some(bind_address.clone());
        let target = bind_address.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move { resolve_routable_host(&bind_address) },
            move |this, routable_host: Option<String>, cx| {
                if this.routable_host_resolved_for.as_deref() == Some(target.as_str()) {
                    this.routable_host = routable_host;
                    cx.notify();
                }
            },
            cx,
        );
    }

    pub(super) fn start_poll(&self, cx: &mut Context<Self>) {
        let Some(handle) = self.server.clone() else {
            return;
        };
        let rt_handle = self.rt_handle.clone();
        let mut presence = PresenceGate::of(cx);
        cx.spawn(async move |this, cx| {
            let mut run_state = handle.run_state();
            let mut serving_facts_stale = true;
            loop {
                presence.until_visible().await;
                let run_state_changed = run_state.take_changed();
                let want_serving_facts = serving_facts_stale || run_state_changed;
                serving_facts_stale = false;
                let (tx, rx) = tokio::sync::oneshot::channel::<ServerPoll>();
                let handle = handle.clone();
                rt_handle.spawn(async move {
                    let snapshot = handle.snapshot().await;
                    let (bind_address, overlay) = if want_serving_facts {
                        let root = handle.overlay_root().await;
                        (
                            Some(handle.bind_addr().await.to_string()),
                            Some(scan_overlay_root(root.as_ref()).await),
                        )
                    } else {
                        (None, None)
                    };
                    #[expect(
                        clippy::let_underscore_must_use,
                        reason = "the view may close before the result arrives"
                    )]
                    let _ = tx.send(ServerPoll {
                        snapshot,
                        bind_address,
                        overlay,
                    });
                });
                if let Ok(poll) = rx.await
                    && this
                        .update(cx, |this, cx| {
                            if let Some(bind_address) = this.apply_poll(poll) {
                                this.resolve_routable_host_for(bind_address, cx);
                            }
                            cx.notify();
                        })
                        .is_err()
                {
                    break;
                }
                cx.background_executor().timer(POLL_INTERVAL).await;
            }
        })
        .detach();
    }

    pub(super) fn apply_poll(&mut self, poll: ServerPoll) -> Option<String> {
        let polled_address = poll.bind_address.clone();
        if polled_address.is_some() {
            self.bind_address.clone_from(&polled_address);
        }
        let needs_resolve = polled_address.as_deref().is_some()
            && self.routable_host_resolved_for.as_deref() != polled_address.as_deref();

        if self.running {
            let snapshot = &poll.snapshot;
            self.connected_clients = snapshot
                .connected_clients
                .iter()
                .map(client_row_from_snapshot)
                .collect();
            self.recent_clients = count_recent_clients(&snapshot.connected_clients);

            self.stats = ServerStats {
                events_per_second: snapshot.aggregate_events_per_second,
                http_requests: snapshot.http_requests_total,
                bandwidth_kilobytes_per_second: snapshot.bandwidth.outbound_bytes_per_second as f32
                    / 1000.0,
                total_bytes_sent: snapshot.bandwidth.outbound_bytes_total,
                total_events_out: snapshot.events_out_total,
                dropped_events: snapshot.dropped_events_total,
            };
            self.throughput_samples
                .push(snapshot.aggregate_events_per_second);
            if self.throughput_samples.len() > MAX_THROUGHPUT_SAMPLES {
                let excess = self.throughput_samples.len() - MAX_THROUGHPUT_SAMPLES;
                self.throughput_samples.drain(..excess);
            }
        }

        if let Some(overlay) = poll.overlay {
            self.apply_overlay_listing(overlay);
        }

        polled_address.filter(|_| needs_resolve)
    }
}
