use forge_types::ActionId;
use tokio::sync::mpsc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionFailureNotice {
    pub action_id: ActionId,
    pub action_name: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationIntent {
    ActionFailed(ActionFailureNotice),
    ActionFailuresFolded { count: u32 },
    ConnectionLossCue { platform_ids: Vec<String> },
}

pub struct NotificationIntents(mpsc::Receiver<NotificationIntent>);

impl NotificationIntents {
    pub(super) fn new(receiver: mpsc::Receiver<NotificationIntent>) -> Self {
        Self(receiver)
    }

    pub async fn next(&mut self) -> Option<NotificationIntent> {
        self.0.recv().await
    }

    pub fn try_next(&mut self) -> Option<NotificationIntent> {
        self.0.try_recv().ok()
    }
}
