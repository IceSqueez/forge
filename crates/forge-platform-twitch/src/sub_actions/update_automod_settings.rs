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
// levels, ALL eight must be present in the body — Twitch rejects a partial set.
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

        // All nine selects left at "unchanged" → nothing to change. Skip the
        // network call entirely; an empty PUT body would be rejected and a
        // full echo would waste a rate-limit token (Helix 800/min budget).
        if overall.is_none() && categories.iter().all(Option::is_none) {
            return SubActionOutcome::Success;
        }

        // Twitch forbids mixing overall_level with the per-category fields. When
        // the user sets overall_level we send ONLY that — overall wins and any
        // individual selects are intentionally ignored.
        let body = if let Some(level) = overall {
            serde_json::json!({ OVERALL_KEY: level })
        } else {
            // Individual mode: PUT requires all eight categories. A category left
            // at "unchanged" keeps its current value, so we GET the live settings
            // first and merge the user's overrides over them.
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

        // moderator_id == broadcaster_id == self: the broadcaster manages their
        // own AutoMod settings, so both query params carry the same id.
        let request = HelixRequest::new(HelixMethod::Put, PATH)
            .query("broadcaster_id", user_id.clone())
            .query("moderator_id", user_id)
            .body(body);

        match self.transport.execute(request).await {
            Ok(_) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        }
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
                    return Err(RegistryError::UnknownKindId(format!(
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
