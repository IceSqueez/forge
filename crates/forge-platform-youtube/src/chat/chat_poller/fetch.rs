use super::*;

pub(super) enum FetchFailure {
    Cancelled,
    QuotaExhausted,
    ChatEnded,
    Failed(PlatformError),
}

impl From<DataApiError> for FetchFailure {
    fn from(error: DataApiError) -> Self {
        if error.is_cancelled() {
            return Self::Cancelled;
        }
        match error {
            DataApiError::QuotaExhausted => Self::QuotaExhausted,
            other => Self::Failed(other.into_platform_error(MISSING_SCOPE)),
        }
    }
}

pub(super) struct ChatMessagesResponse {
    pub(super) items: Vec<serde_json::Value>,
    pub(super) next_page_token: Option<String>,
    pub(super) polling_interval_millis: u64,
}

impl YouTubeChatPoller {
    pub(super) async fn fetch_live_chat_id(
        &self,
        token: &str,
        cancel: &CancellationToken,
    ) -> Result<Option<(String, String, String)>, FetchFailure> {
        match fetch_active_broadcast(&self.api, token, Some(cancel)).await {
            Ok(found) => Ok(found.map(|broadcast| {
                (
                    broadcast.live_chat_id,
                    broadcast.broadcast_id,
                    broadcast.title,
                )
            })),
            Err(LookupFailure::Api(error)) => Err(error.into()),
            Err(LookupFailure::Malformed(error)) => Err(FetchFailure::Failed(error)),
        }
    }

    pub(super) async fn fetch_chat_messages(
        &self,
        token: &str,
        live_chat_id: &str,
        page_token: Option<&str>,
        cancel: &CancellationToken,
    ) -> Result<ChatMessagesResponse, FetchFailure> {
        let mut request = GatewayRequest::get(MESSAGES_PATH)
            .label(MESSAGES_PATH)
            .bearer(Secret::new(token))
            .query("liveChatId", live_chat_id)
            .query("part", "snippet,authorDetails")
            .query("maxResults", CHAT_MESSAGES_PAGE_SIZE)
            .expect_refusal(ExpectedRefusal::status_with_excerpt(
                LIVE_CHAT_ENDED_STATUS,
                LIVE_CHAT_ENDED_REASON,
            ))
            .cancel_on(cancel.clone());
        if let Some(pt) = page_token {
            request = request.query("pageToken", pt);
        }

        let body: serde_json::Value = match self.api.send(CHAT_POLL_COST, request).await {
            Ok(response) => response
                .json()
                .map_err(|e| FetchFailure::Failed(malformed(e)))?,
            Err(error) if error.forbidden_with_reason(LIVE_CHAT_ENDED_REASON) => {
                return Err(FetchFailure::ChatEnded);
            }
            Err(error) => return Err(error.into()),
        };

        let items = body
            .get("items")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let next_page_token = body
            .get("nextPageToken")
            .and_then(|v| v.as_str())
            .map(ToOwned::to_owned);

        let polling_interval_millis = body
            .get("pollingIntervalMillis")
            .and_then(|v| {
                v.as_u64()
                    .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
            })
            .unwrap_or(DEFAULT_POLLING_INTERVAL_MS);

        Ok(ChatMessagesResponse {
            items,
            next_page_token,
            polling_interval_millis,
        })
    }
}
