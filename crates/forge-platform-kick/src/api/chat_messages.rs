use forge_net::{Gateway, GatewayRequest};
use forge_platform_core::PlatformError;

use crate::gateway::{RefusalReasons, authorized, json_body, platform_error};

const CHAT_PATH: &str = "/chat";
const SEND_LABEL: &str = "/chat";
const DELETE_LABEL: &str = "/chat/{message_id}";

const SEND_REFUSALS: RefusalReasons = RefusalReasons {
    unauthorized: "send-chat token rejected (401)",
    forbidden: None,
};

const DELETE_REFUSALS: RefusalReasons = RefusalReasons {
    unauthorized: "delete-chat-message token rejected (401)",
    forbidden: Some(
        "delete-chat-message forbidden (403); check moderation:chat_message:manage scope",
    ),
};

pub struct KickSendChat {
    api: Gateway,
}

impl KickSendChat {
    pub fn new(api: Gateway) -> Self {
        Self { api }
    }

    pub async fn send(
        &self,
        content: &str,
        token: &str,
        broadcaster_user_id: u64,
        as_bot: bool,
    ) -> Result<(), PlatformError> {
        self.post(content, token, broadcaster_user_id, as_bot, None)
            .await
    }

    pub async fn send_reply(
        &self,
        content: &str,
        token: &str,
        broadcaster_user_id: u64,
        reply_to_message_id: &str,
    ) -> Result<(), PlatformError> {
        self.post(
            content,
            token,
            broadcaster_user_id,
            false,
            Some(reply_to_message_id),
        )
        .await
    }

    async fn post(
        &self,
        content: &str,
        token: &str,
        broadcaster_user_id: u64,
        as_bot: bool,
        reply_to_message_id: Option<&str>,
    ) -> Result<(), PlatformError> {
        let mut body = serde_json::json!({
            "content": content,
            "type": if as_bot { "bot" } else { "user" },
        });
        if !as_bot {
            body["broadcaster_user_id"] = serde_json::json!(broadcaster_user_id);
        }
        if let Some(parent) = reply_to_message_id {
            body["reply_to_message_id"] = serde_json::json!(parent);
        }

        let request =
            authorized(GatewayRequest::post(CHAT_PATH), token, SEND_LABEL).body(json_body(&body)?);
        self.api
            .send(request)
            .await
            .map(drop)
            .map_err(|failure| platform_error(failure, SEND_REFUSALS))
    }

    pub async fn delete(&self, message_id: &str, token: &str) -> Result<(), PlatformError> {
        let request = authorized(
            GatewayRequest::delete(format!("{CHAT_PATH}/{message_id}")),
            token,
            DELETE_LABEL,
        );
        self.api
            .send(request)
            .await
            .map(drop)
            .map_err(|failure| platform_error(failure, DELETE_REFUSALS))
    }
}
