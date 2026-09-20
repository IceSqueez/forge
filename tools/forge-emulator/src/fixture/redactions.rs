use std::fmt;

use serde_json::{Map, Value};

use super::report::SeedReport;
use super::spec::Fixture;

pub const REDACTED: &str = "<redacted>";

/// The run's secret values, replaced wherever they occur inside any string.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Redactions {
    /// Longest first, so a secret that contains another is replaced whole.
    secrets: Vec<String>,
}

impl fmt::Debug for Redactions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Redactions({} secret(s))", self.secrets.len())
    }
}

impl Redactions {
    pub fn new(secrets: impl IntoIterator<Item = String>) -> Self {
        let mut secrets: Vec<String> = secrets
            .into_iter()
            .filter(|secret| !secret.is_empty())
            .collect();
        secrets.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
        secrets.dedup();
        Self { secrets }
    }

    /// The fake Twitch access token and, once seeded, the server bearer token and every overlay
    /// page credential.
    pub fn for_run(fixture: &Fixture, seed: Option<&SeedReport>) -> Self {
        let access_token = fixture
            .twitch
            .as_ref()
            .map(|account| account.access_token.clone());
        let bearer = seed.map(|seed| seed.server.bearer_token.clone());
        let overlays = seed
            .into_iter()
            .flat_map(|seed| seed.overlays.iter())
            .map(|overlay| overlay.credential.clone());
        Self::new(access_token.into_iter().chain(bearer).chain(overlays))
    }

    pub fn scrub(&self, text: &str) -> String {
        self.secrets
            .iter()
            .fold(text.to_owned(), |scrubbed, secret| {
                scrubbed.replace(secret.as_str(), REDACTED)
            })
    }

    /// Scrubs every string and object key in place.
    pub fn scrub_value(&self, value: &mut Value) {
        if self.secrets.is_empty() {
            return;
        }
        match value {
            Value::String(text) => *text = self.scrub(text),
            Value::Array(items) => items.iter_mut().for_each(|item| self.scrub_value(item)),
            Value::Object(fields) => {
                let scrubbed: Map<String, Value> = std::mem::take(fields)
                    .into_iter()
                    .map(|(key, mut field)| {
                        self.scrub_value(&mut field);
                        (self.scrub(&key), field)
                    })
                    .collect();
                *fields = scrubbed;
            }
            Value::Null | Value::Bool(_) | Value::Number(_) => {}
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_secret_containing_another_is_replaced_whole() {
        let redactions = Redactions::new(["abc".to_owned(), "abcdef".to_owned()]);
        assert_eq!(
            redactions.scrub("x abcdef y abc"),
            "x <redacted> y <redacted>"
        );
    }

    #[test]
    fn an_empty_secret_is_ignored_rather_than_redacting_between_every_character() {
        let redactions = Redactions::new([String::new()]);
        assert_eq!(redactions.scrub("plain"), "plain");
    }

    #[test]
    fn secrets_inside_nested_strings_and_object_keys_are_scrubbed() {
        let redactions = Redactions::new(["s3cret-token".to_owned()]);
        let mut value = json!({
            "headers": [ "Bearer s3cret-token" ],
            "s3cret-token": { "count": 1, "flag": true, "none": null },
        });
        redactions.scrub_value(&mut value);
        assert_eq!(
            value,
            json!({
                "headers": [ "Bearer <redacted>" ],
                "<redacted>": { "count": 1, "flag": true, "none": null },
            })
        );
    }

    #[test]
    fn debug_output_never_carries_a_secret() {
        let redactions = Redactions::new(["debug-visible-secret".to_owned()]);
        let rendered = format!("{redactions:?}");
        assert!(!rendered.contains("debug-visible-secret"), "{rendered}");
    }
}
