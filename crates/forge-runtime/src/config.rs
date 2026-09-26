#[derive(Debug, Clone)]
pub struct Config {
    /// A child chain past this bound fails the step instead of overflowing the stack.
    pub max_nesting_depth: u32,
    pub condition_op_limit: u64,
    /// Deliberately tighter than a full-script budget: re-evaluated on every poll.
    pub condition_wall_time_ms: u64,
    pub max_cooldown_entries: usize,
    pub bus_ring_retention: usize,
    pub bus_observer_capacity: usize,
    pub critical_priority_capacity: usize,
    pub critical_bulk_capacity: usize,
    pub persist_batch_max_rows: usize,
    /// How long a persisting consumer keeps collecting after its first pending row before it
    /// commits; zero commits whatever is queued at once.
    pub persist_batch_linger_ms: u64,
    pub run_history_capacity: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_nesting_depth: 32,
            condition_op_limit: 10_000,
            condition_wall_time_ms: 50,
            max_cooldown_entries: 4096,
            bus_ring_retention: 131_072,
            bus_observer_capacity: 65_536,
            critical_priority_capacity: 16_384,
            critical_bulk_capacity: 65_536,
            persist_batch_max_rows: 2_048,
            persist_batch_linger_ms: 0,
            run_history_capacity: 65_536,
        }
    }
}
