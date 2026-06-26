#[derive(Debug, Clone)]
pub struct Config {
    /// Nesting levels permitted below an action's top-level chain; entering a
    /// child chain past this bound fails the step instead of overflowing the stack.
    pub max_nesting_depth: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_nesting_depth: 32,
        }
    }
}
