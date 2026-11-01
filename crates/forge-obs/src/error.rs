#[derive(Debug, thiserror::Error)]
pub enum ObsError {
    #[error("connection failed: {0}")]
    Connect(String),

    #[error("authentication rejected")]
    Authentication,

    #[error(transparent)]
    Storage(#[from] forge_storage::StorageError),

    #[error("OBS credentials not stored")]
    NotStored,

    #[error("not connected to OBS")]
    NotConnected,

    #[error("request timed out")]
    Timeout,

    #[error("protocol error: {0}")]
    Protocol(String),

    #[error("raw payload serialization: {0}")]
    Payload(#[from] serde_json::Error),

    #[error(
        "Studio Mode is off in OBS - the preview scene exists only in Studio Mode. Turn it on first with the \"Set Studio Mode\" sub-action"
    )]
    StudioModeInactive,

    #[error("request failed: {request_type} - {message}")]
    Request {
        request_type: String,
        message: String,
    },
}

impl ObsError {
    pub(crate) fn is_connection_loss(&self) -> bool {
        matches!(self, Self::Timeout | Self::NotConnected)
    }
}

pub(crate) fn map_request_error(request_type: &str, e: obws::error::Error) -> ObsError {
    match e {
        obws::error::Error::Timeout => ObsError::Timeout,
        obws::error::Error::Disconnected
        | obws::error::Error::Send(_)
        | obws::error::Error::ReceiveMessage(_) => ObsError::NotConnected,
        obws::error::Error::Api {
            code: obws::responses::StatusCode::StudioModeNotActive,
            ..
        } => ObsError::StudioModeInactive,
        obws::error::Error::Api { code, message } => ObsError::Request {
            request_type: request_type.to_owned(),
            message: message.unwrap_or_else(|| rejection_reason(code).to_owned()),
        },
        _ => ObsError::Request {
            request_type: request_type.to_owned(),
            message: e.to_string(),
        },
    }
}

fn rejection_reason(code: obws::responses::StatusCode) -> &'static str {
    use obws::responses::StatusCode;
    match code {
        StatusCode::NotReady => "OBS is not ready yet",
        StatusCode::OutputRunning => "the output is already running",
        StatusCode::OutputNotRunning => "the output is not running",
        StatusCode::OutputPaused => "the output is already paused",
        StatusCode::OutputNotPaused => "the output is not paused",
        StatusCode::OutputDisabled => "the output is disabled in OBS",
        StatusCode::StudioModeActive => "Studio Mode is on",
        StatusCode::ResourceNotFound => "not found in OBS",
        StatusCode::ResourceAlreadyExists => "already exists in OBS",
        StatusCode::InvalidResourceType | StatusCode::InvalidInputKind => {
            "OBS does not accept this kind of item here"
        }
        StatusCode::InvalidResourceState => "the item is in the wrong state for this request",
        StatusCode::ResourceNotConfigurable => "this item cannot be changed",
        StatusCode::MissingRequestField
        | StatusCode::MissingRequestData
        | StatusCode::InvalidRequestField
        | StatusCode::InvalidRequestFieldType
        | StatusCode::RequestFieldOutOfRange
        | StatusCode::RequestFieldEmpty
        | StatusCode::TooManyRequestFields => "OBS rejected a value in the request",
        _ => "OBS rejected the request",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_errors_split_into_connection_loss_and_obs_rejections() {
        for (error, expect_loss) in [
            (obws::error::Error::Timeout, true),
            (obws::error::Error::Disconnected, true),
            (
                obws::error::Error::Api {
                    code: obws::responses::StatusCode::ResourceNotFound,
                    message: Some("No source was found".to_owned()),
                },
                false,
            ),
        ] {
            let mapped = map_request_error("GetSceneItemId", error);
            assert_eq!(mapped.is_connection_loss(), expect_loss, "{mapped:?}");
            if !expect_loss {
                assert!(
                    matches!(&mapped, ObsError::Request { request_type, .. } if request_type == "GetSceneItemId"),
                    "a rejection lost its request type: {mapped:?}"
                );
            }
        }
    }

    #[test]
    fn api_errors_show_the_obs_message_or_a_readable_reason_for_the_code() {
        let api = |code, message: Option<&str>| obws::error::Error::Api {
            code,
            message: message.map(str::to_owned),
        };
        let studio = map_request_error(
            "SetCurrentPreviewScene",
            api(
                obws::responses::StatusCode::StudioModeNotActive,
                Some("Studio mode is not active."),
            ),
        );
        assert_eq!(
            studio.to_string(),
            "Studio Mode is off in OBS - the preview scene exists only in Studio Mode. Turn it on first with the \"Set Studio Mode\" sub-action"
        );

        for (message, expected) in [
            (
                Some("No source was found"),
                "request failed: SetCurrentPreviewScene - No source was found",
            ),
            (
                None,
                "request failed: SetCurrentPreviewScene - not found in OBS",
            ),
        ] {
            let mapped = map_request_error(
                "SetCurrentPreviewScene",
                api(obws::responses::StatusCode::ResourceNotFound, message),
            );
            assert_eq!(mapped.to_string(), expected);
        }
    }
}
