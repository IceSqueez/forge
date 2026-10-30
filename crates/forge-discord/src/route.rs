use forge_net::{GatewayRequest, Method, Secret};

use crate::error::{DiscordError, ValidationFault};

const HTTPS_PREFIX: &str = "https://";
const WEBHOOK_HOSTS: [&str; 2] = ["discord.com", "discordapp.com"];
const API_SEGMENT: &str = "api";
const WEBHOOKS_SEGMENT: &str = "webhooks";
const WAIT_PARAM: &str = "wait";
const WAIT_FOR_MESSAGE: &str = "true";
const PARENT_SEGMENT: &str = "..";
const PERCENT_ESCAPE: char = '%';
const PATH_SEPARATOR: char = '/';
const QUERY_SEPARATOR: char = '?';
const FRAGMENT_SEPARATOR: char = '#';
const SUBDOMAIN_SEPARATOR: char = '.';
const PAIR_SEPARATOR: char = '&';
const KEY_VALUE_SEPARATOR: char = '=';

pub(crate) struct WebhookRoute {
    id: String,
    token: String,
    query: Vec<(String, String)>,
}

impl WebhookRoute {
    pub(crate) fn parse(url: &str) -> Result<Self, DiscordError> {
        let url = url.trim();
        let without_fragment = url
            .split_once(FRAGMENT_SEPARATOR)
            .map_or(url, |(kept, _)| kept);
        let (location, query) = without_fragment
            .split_once(QUERY_SEPARATOR)
            .unwrap_or((without_fragment, ""));
        let (host, route) = location
            .strip_prefix(HTTPS_PREFIX)
            .and_then(|rest| rest.split_once(PATH_SEPARATOR))
            .ok_or_else(rejected_endpoint)?;
        if !is_webhook_host(host) {
            return Err(rejected_endpoint());
        }
        let mut segments = route.split(PATH_SEPARATOR);
        let (Some(API_SEGMENT), Some(WEBHOOKS_SEGMENT), Some(id), Some(token)) = (
            segments.next(),
            segments.next(),
            segments.next(),
            segments.next(),
        ) else {
            return Err(rejected_endpoint());
        };
        if !is_snowflake(id) || !is_plain_segment(token) {
            return Err(rejected_endpoint());
        }
        Ok(Self {
            id: id.to_owned(),
            token: token.to_owned(),
            query: saved_query(query),
        })
    }

    pub(crate) fn execute(&self) -> GatewayRequest {
        self.prepared(GatewayRequest::post(self.path()))
            .query(WAIT_PARAM, WAIT_FOR_MESSAGE)
    }

    pub(crate) fn message(
        &self,
        method: Method,
        message_id: &str,
    ) -> Result<GatewayRequest, DiscordError> {
        if !is_snowflake(message_id) {
            return Err(DiscordError::Validation(
                ValidationFault::MessageIdNotSnowflake,
            ));
        }
        Ok(self.prepared(GatewayRequest::new(
            method,
            format!("{}/messages/{message_id}", self.path()),
        )))
    }

    fn path(&self) -> String {
        format!("{WEBHOOKS_SEGMENT}/{}/{}", self.id, self.token)
    }

    fn prepared(&self, request: GatewayRequest) -> GatewayRequest {
        let masked = request
            .mask(Secret::new(self.token.clone()))
            .mask(Secret::new(self.id.clone()));
        self.query
            .iter()
            .fold(masked, |request, (name, value)| request.query(name, value))
    }
}

fn rejected_endpoint() -> DiscordError {
    DiscordError::Validation(ValidationFault::NotWebhookEndpoint)
}

fn is_webhook_host(host: &str) -> bool {
    WEBHOOK_HOSTS.iter().any(|allowed| {
        host == *allowed
            || host
                .strip_suffix(allowed)
                .and_then(|prefix| prefix.strip_suffix(SUBDOMAIN_SEPARATOR))
                .is_some_and(|label| !label.is_empty() && !label.ends_with(SUBDOMAIN_SEPARATOR))
    })
}

fn is_snowflake(id: &str) -> bool {
    !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) && id.parse::<u64>().is_ok()
}

fn is_plain_segment(segment: &str) -> bool {
    !segment.is_empty()
        && !segment.contains(PARENT_SEGMENT)
        && !segment.contains(PERCENT_ESCAPE)
        && !segment.contains(PATH_SEPARATOR)
}

fn saved_query(query: &str) -> Vec<(String, String)> {
    query
        .split(PAIR_SEPARATOR)
        .filter(|pair| !pair.is_empty())
        .map(|pair| pair.split_once(KEY_VALUE_SEPARATOR).unwrap_or((pair, "")))
        .filter(|(name, _)| *name != WAIT_PARAM)
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LARGEST_SNOWFLAKE: &str = "18446744073709551615";

    #[test]
    fn the_largest_u64_is_still_a_snowflake_for_the_webhook_and_the_message() {
        let route = WebhookRoute::parse(&format!(
            "https://discord.com/api/webhooks/{LARGEST_SNOWFLAKE}/tok"
        ))
        .unwrap();

        assert!(route.message(Method::Delete, LARGEST_SNOWFLAKE).is_ok());
    }
}
