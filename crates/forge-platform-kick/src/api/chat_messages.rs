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

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn sender_on(server: &MockServer) -> KickSendChat {
        KickSendChat::new(crate::test_support::api_at(&server.uri()))
    }

    async fn chat_server() -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat"))
            .respond_with(ResponseTemplate::new(reqwest::StatusCode::OK))
            .mount(&server)
            .await;
        server
    }

    async fn bodies(server: &MockServer) -> Vec<serde_json::Value> {
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .map(|request| serde_json::from_slice(&request.body).unwrap())
            .collect()
    }

    #[tokio::test]
    async fn send_body_author_mode_drives_type_and_broadcaster_id_presence() {
        for (as_bot, expected_type, expects_broadcaster_id) in
            [(false, "user", true), (true, "bot", false)]
        {
            let server = chat_server().await;

            sender_on(&server)
                .send("hi", "tok", 42, as_bot)
                .await
                .unwrap();

            let body = &bodies(&server).await[0];
            assert_eq!(
                (
                    body["type"].as_str(),
                    body.get("broadcaster_user_id").is_some()
                ),
                (Some(expected_type), expects_broadcaster_id),
                "as_bot={as_bot}"
            );
        }
    }

    #[tokio::test]
    async fn reply_to_message_id_is_sent_only_on_replies() {
        let server = chat_server().await;
        let sender = sender_on(&server);

        sender.send("plain", "tok", 42, false).await.unwrap();
        sender
            .send_reply("reply", "tok", 42, "parent-7")
            .await
            .unwrap();

        let sent = bodies(&server).await;
        assert!(sent[0].get("reply_to_message_id").is_none(), "{}", sent[0]);
        assert_eq!(
            (
                &sent[1]["reply_to_message_id"],
                &sent[1]["content"],
                &sent[1]["broadcaster_user_id"]
            ),
            (
                &serde_json::json!("parent-7"),
                &serde_json::json!("reply"),
                &serde_json::json!(42)
            )
        );
    }

    #[tokio::test]
    async fn send_carries_the_token_as_a_bearer_credential() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat"))
            .and(header("authorization", "Bearer tok-123"))
            .respond_with(ResponseTemplate::new(reqwest::StatusCode::OK))
            .expect(1)
            .mount(&server)
            .await;

        let result = sender_on(&server).send("hi", "tok-123", 42, false).await;

        assert!(result.is_ok(), "{result:?}");
    }

    #[tokio::test]
    async fn delete_targets_the_message_id_path() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/chat/msg-77"))
            .respond_with(ResponseTemplate::new(reqwest::StatusCode::NO_CONTENT))
            .expect(1)
            .mount(&server)
            .await;

        let result = sender_on(&server).delete("msg-77", "tok").await;

        assert!(result.is_ok(), "{result:?}");
    }
}
