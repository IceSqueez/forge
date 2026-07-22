use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use super::identity::SelfIdentity;
use crate::helix::{HelixError, HelixMethod, HelixRequest, HelixTransport};

const KIND_ID: &str = "twitch.automod.update_settings";
const PATH: &str = "/helix/moderation/automod/settings";

const UNCHANGED: &str = "unchanged";
const LEVEL_OPTIONS: &[&str] = &[UNCHANGED, "0", "1", "2", "3", "4"];

const OVERALL_KEY: &str = "overall_level";

// The eight per-category filters Twitch accepts. When PUTting individual
// levels, ALL eight must be present in the body - Twitch rejects a partial set.
const CATEGORY_KEYS: &[&str] = &[
    "aggression",
    "bullying",
    "disability",
    "misogyny",
    "race_ethnicity_or_religion",
    "sex_based_terms",
    "sexuality_sex_or_gender",
    "swearing",
];

pub struct UpdateAutomodSettingsRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl UpdateAutomodSettingsRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn apply(&self, config: &SubActionConfig) -> SubActionOutcome {
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };

        let overall = parse_level(config, OVERALL_KEY);
        let categories: Vec<Option<u8>> = CATEGORY_KEYS
            .iter()
            .map(|key| parse_level(config, key))
            .collect();

        // All unchanged: skip the call, an empty PUT body would be rejected.
        if overall.is_none() && categories.iter().all(Option::is_none) {
            return SubActionOutcome::Success;
        }

        // Twitch forbids mixing overall_level with per-category fields; overall wins.
        let body = if let Some(level) = overall {
            serde_json::json!({ OVERALL_KEY: level })
        } else {
            // PUT requires all eight categories; GET current values to fill in unchanged ones.
            let current = match self.fetch_current(&user_id).await {
                Ok(c) => c,
                Err(e) => return SubActionOutcome::Failed(e.to_string()),
            };
            let mut map = serde_json::Map::new();
            for (key, override_level) in CATEGORY_KEYS.iter().zip(categories.iter()) {
                let level = override_level
                    .or_else(|| current_level(&current, key))
                    .unwrap_or(0);
                map.insert((*key).to_owned(), level.into());
            }
            serde_json::Value::Object(map)
        };

        // moderator_id == broadcaster_id == self.
        let request = HelixRequest::new(HelixMethod::Put, PATH)
            .query("broadcaster_id", user_id.clone())
            .query("moderator_id", user_id)
            .body(body);

        SubActionOutcome::from_result(&self.transport.execute(request).await)
    }

    async fn fetch_current(&self, user_id: &str) -> Result<serde_json::Value, HelixError> {
        let request = HelixRequest::new(HelixMethod::Get, PATH)
            .query("broadcaster_id", user_id.to_owned())
            .query("moderator_id", user_id.to_owned());
        self.transport.execute(request).await
    }
}

/// Reads a tri-state level select: `None` for "unchanged" or absent, else the
/// parsed 0..=4 value. Non-conforming strings also yield `None`.
fn parse_level(config: &SubActionConfig, key: &str) -> Option<u8> {
    match config.get(key).and_then(Variant::as_str) {
        Some(s) if s != UNCHANGED => s.parse::<u8>().ok().filter(|n| *n <= 4),
        _ => None,
    }
}

fn current_level(current: &serde_json::Value, key: &str) -> Option<u8> {
    current["data"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|s| s[key].as_u64())
        .map(|n| n as u8)
}

#[async_trait]
impl SubActionRunner for UpdateAutomodSettingsRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Moderation
    }

    fn label(&self) -> &str {
        "Update AutoMod Settings"
    }

    fn summary(&self) -> &str {
        "Sets the overall AutoMod level or individual filter category levels."
    }

    fn search_text(&self) -> &str {
        "twitch automod settings level filter aggression bullying swearing moderation"
    }

    fn icon_name(&self) -> &str {
        "shield-check"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut config = BTreeMap::new();
        config.insert(
            OVERALL_KEY.to_owned(),
            Variant::String(UNCHANGED.to_owned()),
        );
        for key in CATEGORY_KEYS {
            config.insert((*key).to_owned(), Variant::String(UNCHANGED.to_owned()));
        }
        config
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Select {
                key: OVERALL_KEY,
                label: "Overall Level (overrides individual categories)",
                options: LEVEL_OPTIONS,
            },
            FormField::Select {
                key: "aggression",
                label: "Aggression",
                options: LEVEL_OPTIONS,
            },
            FormField::Select {
                key: "bullying",
                label: "Bullying",
                options: LEVEL_OPTIONS,
            },
            FormField::Select {
                key: "disability",
                label: "Disability",
                options: LEVEL_OPTIONS,
            },
            FormField::Select {
                key: "misogyny",
                label: "Misogyny",
                options: LEVEL_OPTIONS,
            },
            FormField::Select {
                key: "race_ethnicity_or_religion",
                label: "Race, Ethnicity, or Religion",
                options: LEVEL_OPTIONS,
            },
            FormField::Select {
                key: "sex_based_terms",
                label: "Sex-Based Terms",
                options: LEVEL_OPTIONS,
            },
            FormField::Select {
                key: "sexuality_sex_or_gender",
                label: "Sexuality, Sex, or Gender",
                options: LEVEL_OPTIONS,
            },
            FormField::Select {
                key: "swearing",
                label: "Swearing",
                options: LEVEL_OPTIONS,
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        let mut keys = vec![OVERALL_KEY];
        keys.extend_from_slice(CATEGORY_KEYS);
        for key in keys {
            match config.get(key) {
                None => {}
                Some(Variant::String(s)) if LEVEL_OPTIONS.contains(&s.as_str()) => {}
                _ => {
                    return Err(RegistryError::InvalidConfig(format!(
                        "{KIND_ID}: '{key}' must be one of: unchanged, 0, 1, 2, 3, 4"
                    )));
                }
            }
        }
        Ok(())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let outcome = self.apply(config).await;

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: KIND_ID.to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use forge_types::{ArgStack, SubActionOutcome};

    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, TOKEN_SENTINEL, make_ctx,
    };

    fn runner(transport: Arc<MockTransport>) -> UpdateAutomodSettingsRunner {
        UpdateAutomodSettingsRunner::new(
            transport as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        )
    }

    fn config_with(overrides: &[(&str, &str)]) -> SubActionConfig {
        let r = UpdateAutomodSettingsRunner::new(
            Arc::new(MockTransport::returning(Ok(serde_json::Value::Null)))
                as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        let mut config = r.default_config();
        for (key, value) in overrides {
            config.insert((*key).to_owned(), Variant::String((*value).to_owned()));
        }
        config
    }

    fn has_query(req: &HelixRequest, key: &str, value: &str) -> bool {
        req.query.contains(&(key.to_owned(), value.to_owned()))
    }

    #[tokio::test]
    async fn all_unchanged_succeeds_without_any_helix_call() {
        let transport = Arc::new(MockTransport::returning(Ok(serde_json::Value::Null)));
        let runner = runner(Arc::clone(&transport));
        let config = config_with(&[]); // every select at "unchanged"
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);

        let (telemetry, _) = runner.execute(&config, &ctx).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(
            transport.call_count(),
            0,
            "an all-unchanged config must not spend a rate-limit token"
        );
    }

    #[tokio::test]
    async fn overall_level_sends_single_put_with_numeric_overall_and_no_categories() {
        let transport = Arc::new(MockTransport::returning(Ok(serde_json::Value::Null)));
        let runner = runner(Arc::clone(&transport));
        let config = config_with(&[("overall_level", "2")]);
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);

        let (telemetry, _) = runner.execute(&config, &ctx).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(
            transport.call_count(),
            1,
            "overall mode is a single PUT - no GET merge"
        );
        let req = transport.request(0);
        assert_eq!(req.method, HelixMethod::Put);
        assert_eq!(req.path, "/helix/moderation/automod/settings");
        assert!(has_query(&req, "broadcaster_id", SELF_USER_ID));
        assert!(has_query(&req, "moderator_id", SELF_USER_ID));
        assert_eq!(
            req.body,
            Some(serde_json::json!({ "overall_level": 2 })),
            "body must carry overall_level as a JSON number and nothing else"
        );
    }

    #[tokio::test]
    async fn overall_level_wins_over_individual_categories() {
        let transport = Arc::new(MockTransport::returning(Ok(serde_json::Value::Null)));
        let runner = runner(Arc::clone(&transport));
        let config = config_with(&[("overall_level", "4"), ("swearing", "1")]);
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);

        let (telemetry, _) = runner.execute(&config, &ctx).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(
            transport.call_count(),
            1,
            "overall-wins must not trigger the GET-merge path"
        );
        let body = transport.request(0).body.unwrap();
        assert_eq!(body["overall_level"], serde_json::json!(4));
        for key in CATEGORY_KEYS {
            assert!(
                body.get(*key).is_none(),
                "category key {key} must be absent when overall_level wins"
            );
        }
    }

    #[tokio::test]
    async fn individual_mode_merges_overrides_over_fetched_current_levels() {
        let get_response = serde_json::json!({
            "data": [{
                "aggression": 0,
                "bullying": 2,
                "disability": 0,
                "misogyny": 0,
                "race_ethnicity_or_religion": 4,
                "sex_based_terms": 0,
                "sexuality_sex_or_gender": 0,
                "swearing": 0,
            }]
        });
        let transport = Arc::new(MockTransport::returning_sequence(vec![
            Ok(get_response),
            Ok(serde_json::Value::Null),
        ]));
        let runner = runner(Arc::clone(&transport));
        let config = config_with(&[("aggression", "3"), ("swearing", "1")]);
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);

        let (telemetry, _) = runner.execute(&config, &ctx).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(transport.call_count(), 2, "individual mode is GET-then-PUT");

        let get_req = transport.request(0);
        assert_eq!(get_req.method, HelixMethod::Get);
        assert_eq!(get_req.path, "/helix/moderation/automod/settings");
        assert!(has_query(&get_req, "broadcaster_id", SELF_USER_ID));
        assert!(has_query(&get_req, "moderator_id", SELF_USER_ID));

        let put_req = transport.request(1);
        assert_eq!(put_req.method, HelixMethod::Put);
        let body = put_req.body.unwrap();
        assert_eq!(body["aggression"], serde_json::json!(3), "override");
        assert_eq!(body["swearing"], serde_json::json!(1), "override");
        assert_eq!(body["bullying"], serde_json::json!(2), "from GET current");
        assert_eq!(
            body["race_ethnicity_or_religion"],
            serde_json::json!(4),
            "from GET current"
        );
        assert_eq!(body["disability"], serde_json::json!(0), "from GET current");
        assert_eq!(body["misogyny"], serde_json::json!(0), "from GET current");
        assert_eq!(
            body["sex_based_terms"],
            serde_json::json!(0),
            "from GET current"
        );
        assert_eq!(
            body["sexuality_sex_or_gender"],
            serde_json::json!(0),
            "from GET current"
        );
        assert!(
            body.get("overall_level").is_none(),
            "individual-mode PUT must omit overall_level"
        );
    }

    #[tokio::test]
    async fn individual_mode_get_failure_fails_without_issuing_put() {
        let transport = Arc::new(MockTransport::returning_sequence(vec![Err(
            HelixError::Http {
                status: 500,
                body: "boom".to_owned(),
            },
        )]));
        let runner = runner(Arc::clone(&transport));
        let config = config_with(&[("aggression", "3")]);
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);

        let (telemetry, _) = runner.execute(&config, &ctx).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(
            transport.call_count(),
            1,
            "a failed GET must abort before the PUT"
        );
    }

    #[test]
    fn validate_config_accepts_unchanged_and_levels_zero_to_four() {
        let runner = runner(Arc::new(MockTransport::returning(Ok(
            serde_json::Value::Null,
        ))));
        for value in ["unchanged", "0", "1", "2", "3", "4"] {
            let mut config = runner.default_config();
            for key in std::iter::once(OVERALL_KEY).chain(CATEGORY_KEYS.iter().copied()) {
                config.insert(key.to_owned(), Variant::String(value.to_owned()));
            }
            assert!(
                runner.validate_config(&config).is_ok(),
                "{value} must be accepted on every select"
            );
        }
    }

    #[test]
    fn validate_config_rejects_out_of_range_or_non_numeric_levels() {
        let runner = runner(Arc::new(MockTransport::returning(Ok(
            serde_json::Value::Null,
        ))));
        for (key, bad) in [
            ("overall_level", "5"),
            ("aggression", "high"),
            ("swearing", "-1"),
            ("bullying", ""),
        ] {
            let mut config = runner.default_config();
            config.insert(key.to_owned(), Variant::String(bad.to_owned()));
            assert!(
                runner.validate_config(&config).is_err(),
                "{key}={bad:?} must be rejected"
            );
        }
    }

    #[tokio::test]
    async fn failure_outcome_does_not_leak_token() {
        let transport = Arc::new(MockTransport::returning(Err(HelixError::Http {
            status: 500,
            body: "server error".to_owned(),
        })));
        let runner = runner(Arc::clone(&transport));
        let config = config_with(&[("overall_level", "1")]);
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);

        let (telemetry, _) = runner.execute(&config, &ctx).await;

        match telemetry.outcome {
            SubActionOutcome::Failed(msg) => assert!(
                !msg.contains(TOKEN_SENTINEL),
                "failure message must not leak the token: {msg}"
            ),
            other => panic!("expected Failed, got {other:?}"),
        }
    }
}
