#[derive(Debug, Clone)]
pub struct Config {
    /// Nesting levels permitted below an action's top-level chain; entering a
    /// child chain past this bound fails the step instead of overflowing the stack.
    pub max_nesting_depth: u32,
    pub condition_op_limit: u64,
    /// Per-evaluation wall budget in milliseconds. A predicate condition is
    /// re-evaluated on every `wait_until` / `while` poll, so this is deliberately
    /// tighter than a full-script budget.
    pub condition_wall_time_ms: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_nesting_depth: 32,
            condition_op_limit: 10_000,
            condition_wall_time_ms: 50,
        }
    }
}
