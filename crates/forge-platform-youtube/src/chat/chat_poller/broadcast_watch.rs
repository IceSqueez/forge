use super::*;

pub(super) struct BroadcastWatch {
    pub(super) last_seen_title: Option<String>,
    pub(super) next_check: tokio::time::Instant,
    consecutive_failures: u32,
}

impl BroadcastWatch {
    pub(super) fn new() -> Self {
        Self {
            last_seen_title: None,
            next_check: tokio::time::Instant::now(),
            consecutive_failures: 0,
        }
    }
}

impl YouTubeChatPoller {
    pub(super) fn report_viewers(&self, report: ViewerReport) {
        if let Some(reports) = &self.viewer_reports {
            reports.send_replace(report);
        }
    }

    pub(super) fn title_changed(&self, watch: &mut BroadcastWatch, current_title: String) -> bool {
        let changed = watch
            .last_seen_title
            .as_deref()
            .filter(|previous| *previous != current_title)
            .map(|previous| {
                Event::new(
                    EventSource::YouTube,
                    event_kinds::STREAM_TITLE_CHANGED,
                    serde_json::json!({
                        (stream_fields::TITLE): {
                            (stream_fields::OLD): previous,
                            (stream_fields::NEW): current_title,
                        },
                    }),
                )
            });
        watch.last_seen_title = Some(current_title);
        match changed {
            Some(event) => self.bus_sender.send(event).is_ok(),
            None => true,
        }
    }

    pub(super) async fn watch_broadcast_until_receive_stops(
        &self,
        watch: &mut BroadcastWatch,
        broadcast_id: &str,
        checked_chat: &tokio::sync::watch::Sender<CheckedChat>,
        cancel: &CancellationToken,
    ) {
        self.watch_broadcast(watch, broadcast_id, checked_chat, cancel)
            .await;
        if cancel.is_cancelled() {
            std::future::pending::<()>().await;
        }
    }

    async fn watch_broadcast(
        &self,
        watch: &mut BroadcastWatch,
        broadcast_id: &str,
        checked_chat: &tokio::sync::watch::Sender<CheckedChat>,
        cancel: &CancellationToken,
    ) {
        loop {
            tokio::select! {
                biased;
                () = cancel.cancelled() => return,
                () = tokio::time::sleep_until(watch.next_check) => {}
            }
            watch.next_check = tokio::time::Instant::now() + self.timings.broadcast_cadence;

            let fetched_token = tokio::select! {
                biased;
                () = cancel.cancelled() => return,
                fetched = self.access_tokens.current() => fetched,
            };
            let checked = match fetched_token {
                Ok(token) => check_broadcast(&self.api, &token, broadcast_id, cancel)
                    .await
                    .map_err(|failure| match failure {
                        CheckFailure::Api(error) => FetchFailure::from(error),
                        CheckFailure::Malformed(error) => FetchFailure::Failed(error),
                    }),
                Err(error) => Err(FetchFailure::Failed(error)),
            };

            match checked {
                Ok(snapshot) => {
                    watch.consecutive_failures = 0;
                    self.report_viewers(snapshot.viewers);
                    if let Some(title) = snapshot.title
                        && !self.title_changed(watch, title)
                    {
                        return;
                    }
                    checked_chat.send_replace(snapshot.chat);
                }
                Err(FetchFailure::Cancelled) => return,
                Err(FetchFailure::QuotaExhausted) => {
                    tracing::warn!(
                        "quota exhausted before broadcast check: {}",
                        PlatformError::QuotaExhausted
                    );
                    self.note_check_failure(watch);
                }
                Err(FetchFailure::ChatEnded) => self.note_check_failure(watch),
                Err(FetchFailure::Failed(error)) => {
                    tracing::warn!(
                        "broadcast check failed: {}",
                        crate::api::error_shape::redact_platform_error(&error)
                    );
                    self.note_check_failure(watch);
                }
            }
        }
    }

    fn note_check_failure(&self, watch: &mut BroadcastWatch) {
        watch.consecutive_failures = watch.consecutive_failures.saturating_add(1);
        if watch.consecutive_failures >= CONSECUTIVE_POLL_FAILURES_FOR_LOSS {
            self.report_viewers(ViewerReport::Absent);
        }
    }
}
