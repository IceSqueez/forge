use std::sync::Arc;
use std::time::Duration;

use forge_types::RecoverRwLock;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use crate::catalog_revisions::CatalogBaseline;
use crate::catalog_sync::{CatalogFeeds, reconcile_catalog_topology};
use crate::client::SessionSlot;
use crate::error::ObsError;
use crate::session::with_deadline;
use crate::timing::ObsTiming;

pub(crate) const STATS_POLL_INTERVAL: Duration = Duration::from_secs(2);
pub(crate) const STATS_POLL_REQUEST_TIMEOUT: Duration = STATS_POLL_INTERVAL;
const UNANSWERED_STATS_POLLS_BEFORE_SUSPECT: u32 = 3;

pub(crate) fn spawn_stats_poll(
    inner: SessionSlot,
    resync_nudge: Arc<Notify>,
    feeds: CatalogFeeds,
    timing: ObsTiming,
) -> JoinHandle<()> {
    let CatalogFeeds {
        mirror,
        catalog_revisions,
        health_state,
        health_tx,
        stream_output,
    } = feeds;
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(timing.stats_poll_interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut unanswered_polls: u32 = 0;
        loop {
            let nudged = tokio::select! {
                _ = ticker.tick() => false,
                () = resync_nudge.notified() => true,
            };

            let session = {
                let guard = inner.read().await;
                let Some(session) = guard.as_ref() else {
                    continue;
                };
                session.clone()
            };

            if !nudged {
                let general = session.obs().general();
                let streaming = session.obs().streaming();
                let recording = session.obs().recording();
                let (stats, stream_status, record_status) = tokio::join!(
                    with_deadline(
                        timing.stats_poll_request_timeout,
                        "GetStats",
                        general.stats()
                    ),
                    with_deadline(
                        timing.stats_poll_request_timeout,
                        "GetStreamStatus",
                        streaming.status()
                    ),
                    with_deadline(
                        timing.stats_poll_request_timeout,
                        "GetRecordStatus",
                        recording.status()
                    ),
                );

                let unanswered = matches!(stats, Err(ObsError::Timeout))
                    || matches!(stream_status, Err(ObsError::Timeout))
                    || matches!(record_status, Err(ObsError::Timeout));
                if unanswered {
                    unanswered_polls += 1;
                    if unanswered_polls >= UNANSWERED_STATS_POLLS_BEFORE_SUSPECT {
                        unanswered_polls = 0;
                        session.mark_suspect();
                    }
                } else {
                    unanswered_polls = 0;
                }

                let mut deltas = Vec::new();
                if let Ok(stats) = stats {
                    let mut snapshot = health_state.write_recover();
                    deltas.extend(crate::events::apply_stats_update(&stats, &mut snapshot));
                }
                if let Ok(status) = stream_status {
                    let mut snapshot = health_state.write_recover();
                    deltas.extend(crate::events::apply_stream_status_update(
                        &status,
                        &mut snapshot,
                    ));
                }
                if let Ok(status) = record_status {
                    let mut snapshot = health_state.write_recover();
                    deltas.extend(crate::events::apply_record_status_update(
                        &status,
                        &mut snapshot,
                    ));
                }
                for delta in deltas {
                    #[expect(
                        clippy::let_underscore_must_use,
                        reason = "having no subscribers is a normal state"
                    )]
                    let _ = health_tx.send(delta);
                }
                stream_output.refresh();
            }

            if nudged {
                let baseline = CatalogBaseline::of(&mirror);
                if let Err(e) = reconcile_catalog_topology(&session, &mirror).await {
                    tracing::debug!(error = %e, "OBS catalog reconciliation interrupted");
                }
                catalog_revisions.announce_difference(&mirror, &baseline);
            }
        }
    })
}
