use std::sync::{Arc, RwLock};

use forge_types::RecoverRwLock;

#[derive(Clone, Default)]
pub(crate) struct ChannelIdHandle(Arc<RwLock<String>>);

impl ChannelIdHandle {
    pub(crate) fn get(&self) -> String {
        self.0.read_recover().clone()
    }

    pub(crate) fn is_pending(&self) -> bool {
        self.0.read_recover().is_empty()
    }

    pub(crate) fn adopt(&self, channel_id: &str) {
        *self.0.write_recover() = channel_id.to_owned();
    }
}

impl From<String> for ChannelIdHandle {
    fn from(channel_id: String) -> Self {
        Self(Arc::new(RwLock::new(channel_id)))
    }
}
