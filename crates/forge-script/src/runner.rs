use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use forge_events::EventPublisher;
use forge_storage::GlobalsRepo;
use forge_types::{ArgStack, EventId, ScriptId};

use crate::error::ScriptError;
use crate::{Engine, EngineConfig, ForgeApi, build_scope_for_contract, parse_contract};

#[derive(Debug, Clone)]
pub struct RunResult {
    pub script_id: ScriptId,
    pub duration_ms: f64,
    pub error_count: usize,
    pub output_display: String,
}

pub fn content_hash(body: &str) -> String {
    let mut h = DefaultHasher::new();
    body.hash(&mut h);
    format!("{:016x}", h.finish())
}

pub async fn run_inline(
    body: String,
    arg_stack: ArgStack,
    globals: Arc<dyn GlobalsRepo>,
    bus: Arc<dyn EventPublisher>,
    script_id: ScriptId,
) -> Result<RunResult, ScriptError> {
    let contract = parse_contract(&body).map_err(|e| ScriptError::Compile {
        script: body.chars().take(80).collect(),
        reason: e.to_string(),
    })?;
    let mut scope =
        build_scope_for_contract(&contract, &arg_stack).map_err(|e| ScriptError::Runtime {
            script: body.chars().take(80).collect(),
            reason: e.to_string(),
        })?;
    let cfg = EngineConfig::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(cfg.wall_time_ms);
    let api = ForgeApi::new(bus, globals, EventId::new(), deadline);
    let engine = Engine::with_api(cfg, api);
    let start = std::time::Instant::now();
    let result =
        tokio::task::spawn_blocking(move || engine.eval_script_with_scope(&body, &mut scope))
            .await
            .map_err(|e| ScriptError::Runtime {
                script: String::new(),
                reason: e.to_string(),
            })?;
    let duration_ms = start.elapsed().as_secs_f64() * 1000.0;
    let output_display = match result {
        Ok(dyn_val) => {
            if dyn_val.is_unit() {
                "(unit)".to_string()
            } else {
                dyn_val.to_string()
            }
        }
        Err(e) => return Err(e),
    };
    Ok(RunResult {
        script_id,
        duration_ms,
        error_count: 0,
        output_display,
    })
}
