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
            changed
        });
    }
}
