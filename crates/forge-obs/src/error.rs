#[derive(Debug, thiserror::Error)]
pub enum ObsError {
    #[error("connection failed: {0}")]
    Connect(String),

    #[error("authentication rejected")]
    Authentication,

    #[error("not connected to OBS")]
    Disconnected,

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
        matches!(self, Self::Timeout | Self::Disconnected)
    }
}

pub(crate) fn map_request_error(request_type: &str, e: obws::error::Error) -> ObsError {
    match e {
        obws::error::Error::Timeout => ObsError::Timeout,
        obws::error::Error::Disconnected
        | obws::error::Error::Send(_)
        | obws::error::Error::ReceiveMessage(_) => ObsError::Disconnected,
        obws::error::Error::Api {
            code: obws::responses::StatusCode::StudioModeNotActive,
            ..
        } => ObsError::StudioModeInactive,
        obws::error::Error::Api { code, message } => ObsError::Request {
            request_type: request_type.to_owned(),
            message: match message {
                Some(detail) => format!("{code:?}: {detail}"),
                None => format!("{code:?}"),
            },
        },
        _ => ObsError::Request {
            request_type: request_type.to_owned(),
            message: e.to_string(),
        },
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
    fn api_errors_keep_their_code_and_obs_message_except_studio_mode_which_gets_a_reason() {
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
                "request failed: SetCurrentPreviewScene - ResourceNotFound: No source was found",
            ),
            (
                None,
                "request failed: SetCurrentPreviewScene - ResourceNotFound",
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
