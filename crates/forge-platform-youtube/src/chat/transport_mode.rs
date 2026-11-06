use tokio::sync::watch;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChatTransportMode {
    Streaming,
    Polling,
}

#[derive(Clone)]
pub(crate) struct TransportModeSink(watch::Sender<ChatTransportMode>);

impl TransportModeSink {
    pub(crate) fn new(sender: watch::Sender<ChatTransportMode>) -> Self {
        Self(sender)
    }

    pub(crate) fn publish(&self, mode: ChatTransportMode) {
        self.0.send_if_modified(|current| {
            let changed = *current != mode;
            *current = mode;
            if changed {
                match mode {
                    ChatTransportMode::Streaming => {
                        tracing::info!("youtube chat transport: streamed");
                    }
                    ChatTransportMode::Polling => {
                        tracing::info!("youtube chat transport: polling fallback");
                    }
                }
            }
            changed
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ChatTransportMode::{Polling, Streaming};

    #[test]
    fn publishing_wakes_receivers_only_when_the_mode_changes() {
        for (current, published, wakes) in [
            (Streaming, Streaming, false),
            (Polling, Polling, false),
            (Streaming, Polling, true),
            (Polling, Streaming, true),
        ] {
            let (tx, rx) = watch::channel(current);
            let sink = TransportModeSink::new(tx);

            sink.publish(published);

            assert_eq!(
                (rx.has_changed().unwrap(), *rx.borrow()),
                (wakes, published),
                "{current:?} -> {published:?}"
            );
        }
    }
}
