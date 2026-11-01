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
use forge_server::{
    ConnectedClientSnapshot, EventFilterSnapshot, ServerSettings, ServerSnapshot,
    resolve_routable_host,
};
use forge_types::Secret;
use gpui::Context;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

const POLL_INTERVAL: Duration = Duration::from_secs(1);
const MAX_THROUGHPUT_SAMPLES: usize = 60;

pub(super) struct ServerPoll {
    pub(super) snapshot: ServerSnapshot,
    pub(super) bind_address: Option<SocketAddr>,
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
    match (filter.source, filter.kind.as_deref()) {
        (None, None) => OwnedSubscriptionChip {
            label: SUBSCRIBE_ALL_LABEL.to_owned(),
            kind: SubscriptionChipKind::All,
        },
        (None, Some(kind)) => OwnedSubscriptionChip {
            label: kind.to_owned(),
            kind: SubscriptionChipKind::Unknown,
        },
        (Some(source), kind) => OwnedSubscriptionChip {
            label: format!("{}.{}", source.as_str(), kind.unwrap_or("*")),
            kind: SubscriptionChipKind::Source(source),
        },
    }
}

impl ServerConsoleView {
    pub(super) fn fetch_token(&self, cx: &mut Context<Self>) {
        let credentials = Arc::clone(&self.services.credentials);
        let server = self.services.server.clone();
        async_bridge::run_async(
            &self.services.rt_handle,
            server_bearer::current_bearer_token(server, credentials),
            |this, result: Result<Option<Secret>, RepoFailure>, cx| match result {
                Ok(Some(token)) => {
                    this.secrets.bearer_token = token;
                    cx.notify();
                }
                Ok(None) => {}
                Err(reason) => server_bearer::token_load_failed(&reason.text(), cx),
            },
            cx,
        );
    }

    pub(super) fn fetch_enabled(&self, cx: &mut Context<Self>) {
        let settings = Arc::clone(&self.services.settings);
        async_bridge::run_async(
            &self.services.rt_handle,
            async move {
                ServerSettings::load(settings.as_ref())
                    .await
                    .map(|loaded| loaded.enabled)
            },
            |this, loaded, cx| match loaded {
                Ok(enabled) => {
                    this.lifecycle.enabled = enabled;
                    cx.notify();
                }
                Err(error) => {
                    tracing::warn!(error = %error, "server enabled setting unreadable");
                }
            },
            cx,
        );
    }

    fn resolve_routable_host_for(&mut self, bind_address: SocketAddr, cx: &mut Context<Self>) {
        self.lifecycle.routable_host_resolved_for = Some(bind_address);
        async_bridge::run_async(
            &self.services.rt_handle,
            async move { resolve_routable_host(bind_address) },
            move |this, routable_host: Option<String>, cx| {
                if this.lifecycle.routable_host_resolved_for == Some(bind_address) {
                    this.lifecycle.routable_host = routable_host;
                    cx.notify();
                }
            },
            cx,
        );
    }

    pub(super) fn start_poll(&self, cx: &mut Context<Self>) {
        let Some(handle) = self.services.server.clone() else {
            return;
        };
        let rt_handle = self.services.rt_handle.clone();
        let mut presence = PresenceGate::of(cx);
        cx.spawn(async move |this, cx| {
            let mut run_state = handle.run_state();
            let mut serving_facts_stale = true;
            loop {
                presence.until_visible().await;
                let run_state_changed = run_state.take_changed();
                let want_serving_facts = serving_facts_stale || run_state_changed;
                serving_facts_stale = false;
                let handle = handle.clone();
                let reply = crate::async_bridge::on_runtime(&rt_handle, async move {
                    let snapshot = handle.snapshot().await;
                    let (bind_address, overlay) = if want_serving_facts {
                        let root = handle.overlay_root().await;
                        (
                            Some(handle.bind_addr().await),
                            Some(scan_overlay_root(root.as_ref()).await),
                        )
                    } else {
                        (None, None)
                    };
                    ServerPoll {
                        snapshot,
                        bind_address,
                        overlay,
                    }
                });
                if let Ok(poll) = reply.await
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

    pub(super) fn apply_poll(&mut self, poll: ServerPoll) -> Option<SocketAddr> {
        let polled_address = poll.bind_address;
        if polled_address.is_some() {
            self.lifecycle.bind_address = polled_address;
        }
        let needs_resolve =
            polled_address.is_some() && self.lifecycle.routable_host_resolved_for != polled_address;

        if self.lifecycle.running {
            let snapshot = &poll.snapshot;
            self.clients.connected = snapshot
                .connected_clients
                .iter()
                .map(client_row_from_snapshot)
                .collect();
            self.clients.recent = count_recent_clients(&snapshot.connected_clients);

            self.traffic.stats = ServerStats {
                events_per_second: snapshot.aggregate_events_per_second,
                http_requests: snapshot.http_requests_total,
                bandwidth_kilobytes_per_second: snapshot.bandwidth.outbound_bytes_per_second as f32
                    / 1000.0,
                total_bytes_sent: snapshot.bandwidth.outbound_bytes_total,
                total_events_out: snapshot.events_out_total,
                dropped_events: snapshot.dropped_events_total,
            };
            self.traffic
                .throughput_samples
                .push(snapshot.aggregate_events_per_second);
            if self.traffic.throughput_samples.len() > MAX_THROUGHPUT_SAMPLES {
                let excess = self.traffic.throughput_samples.len() - MAX_THROUGHPUT_SAMPLES;
                self.traffic.throughput_samples.drain(..excess);
            }
        }

        if let Some(overlay) = poll.overlay {
            self.apply_overlay_listing(overlay);
        }

        polled_address.filter(|_| needs_resolve)
    }
}
