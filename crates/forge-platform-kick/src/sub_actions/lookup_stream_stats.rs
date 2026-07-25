use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_platform_core::PlatformError;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use futures::future::BoxFuture;
use time::OffsetDateTime;

use crate::channel::KickChannel;

const KIND_ID: &str = "kick.lookup.stream_stats";

pub struct LookupStreamStatsRunner {
    client: Arc<KickChannel>,
    token_source: Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
}

impl LookupStreamStatsRunner {
    pub fn new(
        client: Arc<KickChannel>,
        token_source: Arc<
            dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync,
        >,
    ) -> Self {
        Self {
            client,
            token_source,
        }
    }
}

#[async_trait]
impl SubActionRunner for LookupStreamStatsRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Kick
    }

    fn label(&self) -> &str {
        "Stream Stats"
    }

    fn summary(&self) -> &str {
        "Fetches live stream details for your own Kick channel."
    }

    fn search_text(&self) -> &str {
        "kick stream stats viewer count started category live"
    }

    fn icon_name(&self) -> &str {
        "chart-line"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![]
    }

    fn validate_config(&self, _config: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }

    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let result = match (self.token_source)().await {
            Err(e) => Err(format!("token error: {e}")),
            Ok(token) => match self.client.get_channel(&token).await {
                Ok(snapshot) => {
                    let mut stack = ctx.arg_stack.clone();
                    stack = stack.set(
                        "kick.stream.is_live".to_owned(),
                        Variant::Bool(snapshot.is_live),
                    );
                    stack = stack.set(
                        "kick.stream.viewer_count".to_owned(),
                        Variant::Int(snapshot.viewer_count as i64),
                    );
                    stack = stack.set(
                        "kick.stream.started_at".to_owned(),
                        Variant::String(snapshot.started_at),
                    );
                    stack = stack.set(
                        "kick.stream.category_id".to_owned(),
                        Variant::Int(snapshot.category_id as i64),
                    );
                    stack = stack.set(
                        "kick.stream.category_name".to_owned(),
                        Variant::String(snapshot.category_name),
                    );
                    stack = stack.set(
                        "kick.stream.stream_title".to_owned(),
                        Variant::String(snapshot.stream_title),
                    );
                    Ok(stack)
                }
                Err(e) => Err(e.to_string()),
            },
        };

        match result {
            Ok(stack) => (
                SubActionTelemetry {
                    args_in: BTreeMap::new(),
                    produced: BTreeMap::new(),
                    kind: KIND_ID.to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome: SubActionOutcome::Success,
                    index: ctx.index,
                },
                Some(stack),
            ),
            Err(msg) => (
                SubActionTelemetry {
                    args_in: BTreeMap::new(),
                    produced: BTreeMap::new(),
                    kind: KIND_ID.to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome: SubActionOutcome::Failed(msg),
                    index: ctx.index,
                },
                None,
            ),
        }
    }
}
