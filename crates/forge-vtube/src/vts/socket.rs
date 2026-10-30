use std::io::ErrorKind;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{Sink, SinkExt, Stream, StreamExt};
use serde_json::Value;
use tokio_tungstenite::tungstenite::{Error as WsError, Message};

use forge_types::MessageRef;

use crate::error::VTubeError;
use crate::vocabulary::messages;
use crate::vts::protocol::new_request;

pub(crate) trait VtsTransport:
    Sink<Message, Error = WsError> + Stream<Item = Result<Message, WsError>> + Send + Unpin
{
}

impl<T> VtsTransport for T where
    T: Sink<Message, Error = WsError> + Stream<Item = Result<Message, WsError>> + Send + Unpin
{
}

pub(crate) type VtsWs = Box<dyn VtsTransport>;

pub(crate) type TransportLayer = Arc<dyn Fn(VtsWs) -> VtsWs + Send + Sync>;

pub(crate) fn bare_transport() -> TransportLayer {
    Arc::new(|ws| ws)
}

pub(crate) enum Inbound {
    Closed,
    Envelope(Value),
    Skipped,
}

pub(crate) struct DialFailure {
    pub(crate) message: MessageRef,
    pub(crate) detail: String,
}

impl DialFailure {
    fn timed_out() -> Self {
        Self {
            message: messages::unreachable(),
            detail: VTubeError::Timeout.to_string(),
        }
    }

    fn of(error: &WsError) -> Self {
        let message = match error {
            WsError::Io(io_error) if io_error.kind() == ErrorKind::ConnectionRefused => {
                messages::connect_refused()
            }
            WsError::Io(_) => messages::unreachable(),
            _ => messages::connect_failed(),
        };
        Self {
            message,
            detail: error.to_string(),
        }
    }
}

async fn dial(endpoint: &str) -> Result<VtsWs, DialFailure> {
    tokio_tungstenite::connect_async(endpoint)
        .await
        .map(|(ws, _)| Box::new(ws) as VtsWs)
        .map_err(|e| DialFailure::of(&e))
}

pub(crate) async fn dial_within(endpoint: &str, limit: Duration) -> Result<VtsWs, DialFailure> {
    tokio::time::timeout(limit, dial(endpoint))
        .await
        .map_err(|_| DialFailure::timed_out())?
}

pub(crate) async fn send_close(ws: &mut VtsWs) -> Result<(), WsError> {
    ws.send(Message::Close(None)).await
}

pub(crate) async fn send_text(ws: &mut VtsWs, text: String) -> Result<(), VTubeError> {
    ws.send(Message::Text(text.into()))
        .await
        .map_err(|e| VTubeError::Connect(e.to_string()))
}

pub(crate) async fn send_ws_msg<T: serde::Serialize>(
    ws: &mut VtsWs,
    msg: &T,
) -> Result<(), VTubeError> {
    let text = serde_json::to_string(msg).map_err(VTubeError::Json)?;
    send_text(ws, text).await
}

pub(crate) async fn send_ws_msg_within<T: serde::Serialize>(
    ws: &mut VtsWs,
    msg: &T,
    limit: Duration,
) -> Result<(), VTubeError> {
    tokio::time::timeout(limit, send_ws_msg(ws, msg))
        .await
        .map_err(|_| VTubeError::Timeout)?
}

pub(crate) async fn next_inbound(ws: &mut VtsWs) -> Inbound {
    decode_inbound(ws.next().await)
}

fn decode_inbound(frame: Option<Result<Message, WsError>>) -> Inbound {
    match frame {
        None | Some(Err(_)) => Inbound::Closed,
        Some(Ok(Message::Text(text))) => {
            serde_json::from_str::<Value>(&text).map_or(Inbound::Skipped, Inbound::Envelope)
        }
        Some(Ok(_)) => Inbound::Skipped,
    }
}

pub(crate) async fn recv_next_text(ws: &mut VtsWs) -> Result<Value, VTubeError> {
    loop {
        match ws.next().await {
            None => return Err(VTubeError::Connect("connection closed".to_owned())),
            Some(Err(e)) => return Err(VTubeError::Connect(e.to_string())),
            Some(Ok(Message::Text(text))) => {
                return serde_json::from_str(&text).map_err(VTubeError::Json);
            }
            Some(Ok(_)) => {}
        }
    }
}

async fn recv_reply(ws: &mut VtsWs, request_id: &str) -> Result<Value, VTubeError> {
    loop {
        let msg = recv_next_text(ws).await?;
        if msg["requestID"].as_str() == Some(request_id) {
            return Ok(msg);
        }
    }
}

pub(crate) async fn exchange(
    ws: &mut VtsWs,
    msg_type: &str,
    data: Value,
    limit: Duration,
) -> Result<Value, VTubeError> {
    let req = new_request(msg_type, data);
    let round_trip = async {
        send_ws_msg(ws, &req).await?;
        recv_reply(ws, &req.request_id).await
    };
    tokio::time::timeout(limit, round_trip)
        .await
        .map_err(|_| VTubeError::Timeout)?
}

#[cfg(test)]
pub(crate) mod tests {
    use std::pin::Pin;
    use std::sync::Mutex;
    use std::task::{Context, Poll};

    use serde_json::json;
    use tokio::io::DuplexStream;
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;
    use tokio_tungstenite::WebSocketStream;
    use tokio_tungstenite::tungstenite::protocol::Role;

    use super::*;

    pub(crate) const HANG_GUARD: Duration = Duration::from_secs(30);
    pub(crate) const SHORT_LIMIT: Duration = Duration::from_millis(50);
    const SMALLER_THAN_ANY_FRAME: usize = 1;

    pub(crate) async fn socket_with_a_stalled_write() -> (VtsWs, DuplexStream) {
        let (near, unread_peer) = tokio::io::duplex(SMALLER_THAN_ANY_FRAME);
        let ws = WebSocketStream::from_raw_socket(near, Role::Client, None).await;
        (Box::new(ws), unread_peer)
    }

    #[derive(Clone, Copy)]
    pub(crate) enum WriteFate {
        Stall,
        Fail,
    }

    pub(crate) struct DoomedWrite {
        pub(crate) frame_fragment: &'static str,
        pub(crate) fate: WriteFate,
        pub(crate) reached: oneshot::Sender<()>,
    }

    impl DoomedWrite {
        pub(crate) fn on_the_first_dial(self) -> TransportLayer {
            let armed = Mutex::new(Some(self));
            Arc::new(move |ws| match armed.lock().unwrap().take() {
                Some(doomed) => Box::new(ScriptedWrites {
                    inner: ws,
                    doomed: Some(doomed),
                    blocked_as: None,
                }),
                None => ws,
            })
        }
    }

    struct ScriptedWrites {
        inner: VtsWs,
        doomed: Option<DoomedWrite>,
        blocked_as: Option<WriteFate>,
    }

    impl Sink<Message> for ScriptedWrites {
        type Error = WsError;

        fn poll_ready(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Pin::new(&mut self.inner).poll_ready(cx)
        }

        fn start_send(mut self: Pin<&mut Self>, item: Message) -> Result<(), Self::Error> {
            let is_doomed = self.doomed.as_ref().is_some_and(|doomed| {
                matches!(&item, Message::Text(text) if text.contains(doomed.frame_fragment))
            });
            if !is_doomed {
                return Pin::new(&mut self.inner).start_send(item);
            }
            if let Some(doomed) = self.doomed.take() {
                self.blocked_as = Some(doomed.fate);
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "a scenario may not wait for the write to be reached"
                )]
                let _ = doomed.reached.send(());
            }
            Ok(())
        }

        fn poll_flush(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            match self.blocked_as {
                None => Pin::new(&mut self.inner).poll_flush(cx),
                Some(WriteFate::Stall) => Poll::Pending,
                Some(WriteFate::Fail) => Poll::Ready(Err(WsError::ConnectionClosed)),
            }
        }

        fn poll_close(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Pin::new(&mut self.inner).poll_close(cx)
        }
    }

    impl Stream for ScriptedWrites {
        type Item = Result<Message, WsError>;

        fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            Pin::new(&mut self.inner).poll_next(cx)
        }
    }

    #[test]
    fn a_json_text_frame_is_delivered_as_an_envelope() {
        let frame = Some(Ok(Message::Text(
            r#"{"messageType":"ItemEvent","data":{}}"#.into(),
        )));

        let Inbound::Envelope(value) = decode_inbound(frame) else {
            panic!("a JSON text frame must decode to an envelope");
        };
        assert_eq!(value, json!({ "messageType": "ItemEvent", "data": {} }));
    }

    #[test]
    fn frames_that_carry_no_json_text_are_skipped() {
        for frame in [
            Message::Text("not json".into()),
            Message::Text("".into()),
            Message::Binary(vec![b'{', b'}'].into()),
            Message::Ping(Vec::new().into()),
            Message::Pong(Vec::new().into()),
        ] {
            let label = format!("{frame:?}");

            assert!(
                matches!(decode_inbound(Some(Ok(frame))), Inbound::Skipped),
                "{label}"
            );
        }
    }

    #[test]
    fn the_end_of_the_stream_or_a_transport_error_closes_the_session() {
        for frame in [None, Some(Err(WsError::ConnectionClosed))] {
            assert!(matches!(decode_inbound(frame), Inbound::Closed));
        }
    }

    #[test]
    fn a_dial_error_maps_to_the_code_for_its_cause() {
        for (label, error, expected) in [
            (
                "connection refused",
                WsError::Io(std::io::Error::from(ErrorKind::ConnectionRefused)),
                "connect_refused",
            ),
            (
                "any other io failure",
                WsError::Io(std::io::Error::from(ErrorKind::TimedOut)),
                "unreachable",
            ),
            (
                "a non-io failure",
                WsError::ConnectionClosed,
                "connect_failed",
            ),
        ] {
            let failure = DialFailure::of(&error);

            assert_eq!(failure.message.code().name(), expected, "{label}");
        }
    }

    #[tokio::test]
    async fn dialing_a_closed_port_fails_as_connect_refused_with_the_transport_detail() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);

        let Err(failure) = dial_within(&format!("ws://{addr}"), HANG_GUARD).await else {
            panic!("dialing a closed port succeeded");
        };

        assert_eq!(failure.message.code().name(), "connect_refused");
        assert!(!failure.detail.is_empty());
    }

    #[tokio::test]
    async fn a_peer_silent_on_the_upgrade_gives_up_as_unreachable_at_the_limit() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("ws://{}", listener.local_addr().unwrap());

        let Err(failure) = dial_within(&endpoint, SHORT_LIMIT).await else {
            panic!("the dial completed an upgrade nobody answered");
        };

        assert_eq!(failure.message.code().name(), "unreachable");
        assert_eq!(failure.detail, VTubeError::Timeout.to_string());
    }

    #[tokio::test(start_paused = true)]
    async fn a_request_whose_write_stalls_times_out() {
        let (mut ws, _peer) = socket_with_a_stalled_write().await;

        let outcome = tokio::time::timeout(
            HANG_GUARD,
            exchange(&mut ws, "AnyRequest", json!({}), SHORT_LIMIT),
        )
        .await
        .expect("the stalled write outlived its limit");

        assert!(matches!(outcome, Err(VTubeError::Timeout)), "{outcome:?}");
    }

    #[tokio::test(start_paused = true)]
    async fn a_message_whose_write_stalls_times_out() {
        let (mut ws, _peer) = socket_with_a_stalled_write().await;

        let outcome = tokio::time::timeout(
            HANG_GUARD,
            send_ws_msg_within(&mut ws, &json!({}), SHORT_LIMIT),
        )
        .await
        .expect("the stalled write outlived its limit");

        assert!(matches!(outcome, Err(VTubeError::Timeout)), "{outcome:?}");
    }
}
