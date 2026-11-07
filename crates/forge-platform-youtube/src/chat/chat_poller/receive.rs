use super::*;

pub(crate) struct ChatSession<'a> {
    pub(crate) live_chat_id: &'a str,
    pub(crate) broadcast_id: &'a str,
    pub(crate) checked_chat: &'a tokio::sync::watch::Receiver<CheckedChat>,
    pub(crate) dedup: &'a mut DedupSet,
    pub(crate) health: &'a mut PollHealthTracker,
    pub(crate) is_live: &'a mut bool,
    pub(crate) cancel: &'a CancellationToken,
}

impl ChatSession<'_> {
    pub(crate) fn broadcast_moved_on(&self) -> bool {
        !matches!(
            &*self.checked_chat.borrow(),
            CheckedChat::Open { live_chat_id } if live_chat_id == self.live_chat_id
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReceiveExit {
    Stop,
    BroadcastEnded,
    Resolve,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PageEffect {
    Continue { delivered_fresh: bool },
    ChatEnded,
    BusClosed,
}

pub(crate) trait Receive: Send {
    fn receive(
        &mut self,
        poller: &YouTubeChatPoller,
        session: ChatSession<'_>,
    ) -> impl Future<Output = ReceiveExit> + Send;
}

pub(crate) struct PollingReceive;

impl Receive for PollingReceive {
    async fn receive(
        &mut self,
        poller: &YouTubeChatPoller,
        mut session: ChatSession<'_>,
    ) -> ReceiveExit {
        poller.poll_until_resolve(&mut session).await
    }
}

impl YouTubeChatPoller {
    pub(crate) fn timings(&self) -> ChatPollTimings {
        self.timings
    }

    pub(crate) async fn handle_page(
        &self,
        response: &ChatMessagesResponse,
        session: &mut ChatSession<'_>,
    ) -> PageEffect {
        let mut chat_ended = false;
        let mut delivered_fresh = false;
        for item in &response.items {
            if item_type(item) == CHAT_ENDED_EVENT_TYPE {
                chat_ended = true;
                if let Some(event) =
                    self.chat_ended_event(item, session.broadcast_id, session.dedup)
                {
                    *session.is_live = false;
                    if self.bus_sender.send(event).is_err() {
                        return PageEffect::BusClosed;
                    }
                }
                continue;
            }
            if let Some(avatars) = &self.avatars {
                avatars.record(item);
            }
            if let Some(event) = self.build_event(item, session.dedup) {
                delivered_fresh = true;
                if item_type(item) == USER_BANNED_EVENT_TYPE {
                    self.record_observed_ban(item).await;
                }
                if self.bus_sender.send(event).is_err() {
                    return PageEffect::BusClosed;
                }
            }
        }
        if chat_ended {
            PageEffect::ChatEnded
        } else {
            PageEffect::Continue { delivered_fresh }
        }
    }

    async fn poll_until_resolve(&self, session: &mut ChatSession<'_>) -> ReceiveExit {
        let cancel = session.cancel;
        let mut next_page_token: Option<String> = None;
        loop {
            if cancel.is_cancelled() {
                return ReceiveExit::Stop;
            }

            if session.broadcast_moved_on() {
                return ReceiveExit::Resolve;
            }

            let fetched_token = tokio::select! {
                biased;
                () = cancel.cancelled() => return ReceiveExit::Stop,
                fetched = self.access_tokens.current() => fetched,
            };
            let token = match fetched_token {
                Ok(t) => t,
                Err(e) => {
                    session.health.failed();
                    tracing::warn!(
                        "access token fetch failed during chat poll: {}",
                        crate::api::error_shape::redact_platform_error(&e)
                    );
                    tokio::select! {
                        () = tokio::time::sleep(self.poll_floor().await) => {}
                        () = cancel.cancelled() => return ReceiveExit::Stop,
                    }
                    continue;
                }
            };

            let fetched = self
                .fetch_chat_messages(
                    &token,
                    session.live_chat_id,
                    next_page_token.as_deref(),
                    cancel,
                )
                .await;
            let floor = self.poll_floor().await;
            let response = match fetched {
                Ok(r) => {
                    session.health.succeeded();
                    r
                }
                Err(FetchFailure::Cancelled) => return ReceiveExit::Stop,
                Err(FetchFailure::ChatEnded) => {
                    session.health.succeeded();
                    return ReceiveExit::BroadcastEnded;
                }
                Err(FetchFailure::QuotaExhausted) => {
                    tracing::warn!(
                        "quota exhausted before chat poll: {}",
                        PlatformError::QuotaExhausted
                    );
                    tokio::select! {
                        () = tokio::time::sleep(self.timings.broadcast_cadence) => {}
                        () = cancel.cancelled() => return ReceiveExit::Stop,
                    }
                    return ReceiveExit::Resolve;
                }
                Err(FetchFailure::Failed(e)) => {
                    session.health.failed();
                    tracing::warn!(
                        "chat messages fetch failed: {}",
                        crate::api::error_shape::redact_platform_error(&e)
                    );
                    tokio::select! {
                        () = tokio::time::sleep(floor) => {}
                        () = cancel.cancelled() => return ReceiveExit::Stop,
                    }
                    continue;
                }
            };

            match self.handle_page(&response, session).await {
                PageEffect::BusClosed => return ReceiveExit::Stop,
                PageEffect::ChatEnded => return ReceiveExit::BroadcastEnded,
                PageEffect::Continue { .. } => {}
            }

            next_page_token = response.next_page_token;

            let computed_sleep = sleep_duration(
                response.polling_interval_millis,
                u64::try_from(floor.as_millis()).unwrap_or(u64::MAX),
            );

            tokio::select! {
                () = tokio::time::sleep(computed_sleep) => {}
                () = cancel.cancelled() => return ReceiveExit::Stop,
            }
        }
    }
}
