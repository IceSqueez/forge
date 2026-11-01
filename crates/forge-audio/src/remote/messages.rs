use forge_types::{MessageCatalog, MessageCode, MessageRef, UpstreamDetail};

use crate::remote::RemoteFailure;

const NAMESPACE: &str = "audio";

const REMOTE_PAGE_REFUSED: MessageCode = MessageCode::from_static(NAMESPACE, "remote_page_refused");
const REMOTE_CLIP_NOT_OFFERED: MessageCode =
    MessageCode::from_static(NAMESPACE, "remote_clip_not_offered");
const REMOTE_NOT_DELIVERED: MessageCode =
    MessageCode::from_static(NAMESPACE, "remote_not_delivered");
const REMOTE_NOT_PENDING: MessageCode = MessageCode::from_static(NAMESPACE, "remote_not_pending");
const REMOTE_NEVER_FETCHED: MessageCode =
    MessageCode::from_static(NAMESPACE, "remote_never_fetched");
const REMOTE_NO_VERDICT: MessageCode = MessageCode::from_static(NAMESPACE, "remote_no_verdict");
const REMOTE_WITHDRAWN: MessageCode = MessageCode::from_static(NAMESPACE, "remote_withdrawn");
const REMOTE_SERVER_STOPPED: MessageCode =
    MessageCode::from_static(NAMESPACE, "remote_server_stopped");

pub const AUDIO_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: NAMESPACE,
    codes: &[
        REMOTE_PAGE_REFUSED,
        REMOTE_CLIP_NOT_OFFERED,
        REMOTE_NOT_DELIVERED,
        REMOTE_NOT_PENDING,
        REMOTE_NEVER_FETCHED,
        REMOTE_NO_VERDICT,
        REMOTE_WITHDRAWN,
        REMOTE_SERVER_STOPPED,
    ],
    en_ftl: include_str!("../../locales/en/messages.ftl"),
    uk_ftl: include_str!("../../locales/uk/messages.ftl"),
};

impl RemoteFailure {
    pub fn message(&self) -> MessageRef {
        match self {
            Self::PageRefused(reason) if reason.is_empty() => MessageRef::new(REMOTE_PAGE_REFUSED),
            Self::PageRefused(reason) => MessageRef::new(REMOTE_PAGE_REFUSED)
                .with_detail(UpstreamDetail::from_redacted(reason.as_str())),
            Self::ClipNotOffered => MessageRef::new(REMOTE_CLIP_NOT_OFFERED),
            Self::NotDelivered => MessageRef::new(REMOTE_NOT_DELIVERED),
            Self::NotPending => MessageRef::new(REMOTE_NOT_PENDING),
            Self::NeverFetched => MessageRef::new(REMOTE_NEVER_FETCHED),
            Self::NoVerdict => MessageRef::new(REMOTE_NO_VERDICT),
            Self::Withdrawn => MessageRef::new(REMOTE_WITHDRAWN),
            Self::ServerStopped => MessageRef::new(REMOTE_SERVER_STOPPED),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    fn every_failure() -> [RemoteFailure; 8] {
        [
            RemoteFailure::PageRefused("autoplay blocked".into()),
            RemoteFailure::ClipNotOffered,
            RemoteFailure::NotDelivered,
            RemoteFailure::NotPending,
            RemoteFailure::NeverFetched,
            RemoteFailure::NoVerdict,
            RemoteFailure::Withdrawn,
            RemoteFailure::ServerStopped,
        ]
    }

    #[test]
    fn every_failure_has_its_own_catalogued_code() {
        let produced: BTreeSet<MessageCode> = every_failure()
            .iter()
            .map(|failure| failure.message().code().clone())
            .collect();
        let catalogued: BTreeSet<MessageCode> = AUDIO_MESSAGES.codes.iter().cloned().collect();

        assert_eq!(produced, catalogued);
        assert!(AUDIO_MESSAGES.check().is_ok());
    }

    #[test]
    fn only_a_page_refusal_carries_upstream_detail() {
        for failure in every_failure() {
            let detail = failure.message().detail().map(|d| d.as_str().to_owned());
            let expected = matches!(failure, RemoteFailure::PageRefused(_))
                .then(|| "autoplay blocked".to_owned());

            assert_eq!(detail, expected, "{failure:?}");
        }
        assert!(
            RemoteFailure::PageRefused(String::new())
                .message()
                .detail()
                .is_none()
        );
    }
}
