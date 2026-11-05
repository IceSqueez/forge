use super::*;

pub(super) fn ws_error_reason(e: &tokio_tungstenite::tungstenite::Error) -> String {
    match e {
        tokio_tungstenite::tungstenite::Error::Url(_) => "websocket url rejected".to_owned(),
        other => other.to_string(),
    }
}

pub(super) async fn connect_socket(url: &str) -> Result<EventSubSocket, TwitchError> {
    match tokio::time::timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(url)).await {
        Ok(Ok((socket, _))) => Ok(socket),
        Ok(Err(e)) => Err(TwitchError::DialSocket(ws_error_reason(&e))),
        Err(_) => Err(TwitchError::DialConnectTimedOut),
    }
}

pub(super) async fn next_message(
    socket: &mut Option<EventSubSocket>,
) -> Option<Result<Message, tokio_tungstenite::tungstenite::Error>> {
    match socket {
        Some(socket) => socket.next().await,
        None => std::future::pending().await,
    }
}

pub(super) async fn await_successor(
    dial: &mut Option<SuccessorDial>,
) -> Result<(EventSubSocket, String), TwitchError> {
    match dial {
        Some(dial) => dial.await,
        None => std::future::pending().await,
    }
}

pub(super) async fn dial_successor(url: String) -> Result<(EventSubSocket, String), TwitchError> {
    match tokio::time::timeout(SUCCESSOR_TIMEOUT, welcome_on_successor(url)).await {
        Ok(welcomed) => welcomed,
        Err(_) => Err(TwitchError::WelcomeTimedOut),
    }
}

pub(super) async fn welcome_on_successor(
    url: String,
) -> Result<(EventSubSocket, String), TwitchError> {
    let mut socket = connect_socket(&url).await?;

    while let Some(frame) = socket.next().await {
        let Message::Text(text) =
            frame.map_err(|e| TwitchError::DialSocket(ws_error_reason(&e)))?
        else {
            continue;
        };
        let Ok(parsed) = serde_json::from_str::<WsFrame>(&text) else {
            continue;
        };
        if parsed.metadata.message_type != MESSAGE_SESSION_WELCOME {
            debug!(
                message_type = %parsed.metadata.message_type,
                "successor socket frame ahead of session_welcome"
            );
            continue;
        }
        return match parsed.payload.and_then(|payload| payload.session) {
            Some(session) => Ok((socket, session.id)),
            None => Err(TwitchError::WelcomeWithoutSession),
        };
    }

    Err(TwitchError::ClosedBeforeWelcome)
}

pub(super) fn arm_successor(successor: &mut Option<SuccessorDial>, url: String) {
    if successor.is_some() {
        debug!("successor already dialing; repeated reconnect ignored");
    } else {
        *successor = Some(Box::pin(dial_successor(url)));
    }
}

pub(super) async fn await_subscription_pass(pass: &mut Option<SubscriptionPassRun>) -> PassVerdict {
    match pass {
        Some(pass) => pass.await,
        None => std::future::pending().await,
    }
}

pub(super) async fn await_badge_fetch(fetch: &mut Option<BadgeFetchRun>) -> Option<BadgeCatalog> {
    match fetch {
        Some(fetch) => fetch.await,
        None => std::future::pending().await,
    }
}

pub(super) async fn await_topic_retries(retries: &mut Option<TopicRetryRun>) {
    match retries {
        Some(retries) => retries.finished().await,
        None => std::future::pending().await,
    }
}

pub(super) async fn close_socket(mut socket: EventSubSocket) {
    if tokio::time::timeout(CLOSE_TIMEOUT, socket.close(None))
        .await
        .is_err()
    {
        tracing::debug!(timeout = ?CLOSE_TIMEOUT, "eventsub socket close timed out");
    }
}

pub(super) fn close_in_background(socket: EventSubSocket) {
    tokio::spawn(close_socket(socket));
}
