use std::collections::HashSet;

#[derive(Default)]
pub struct InFlightSteps {
    running: HashSet<(String, String)>,
}

impl InFlightSteps {
    /// False while the same step kind is still running for that row.
    pub fn try_begin(&mut self, row: &str, kind: &str) -> bool {
        self.running.insert((row.to_owned(), kind.to_owned()))
    }

    pub fn finish(&mut self, row: &str, kind: &str) {
        self.running.remove(&(row.to_owned(), kind.to_owned()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_step_on_the_same_row_is_refused_while_it_runs() {
        let mut steps = InFlightSteps::default();

        assert!(steps.try_begin("row-1", "obs.scene.switch"));
        assert!(!steps.try_begin("row-1", "obs.scene.switch"));
    }

    #[test]
    fn a_running_step_does_not_block_another_row_or_another_step_kind() {
        let mut steps = InFlightSteps::default();
        assert!(steps.try_begin("row-1", "obs.scene.switch"));

        assert!(steps.try_begin("row-2", "obs.scene.switch"));
        assert!(steps.try_begin("row-1", "obs.source.toggle"));
    }

    #[test]
    fn finishing_a_step_admits_it_again_on_that_row() {
        let mut steps = InFlightSteps::default();
        steps.try_begin("row-1", "obs.scene.switch");

        steps.finish("row-1", "obs.scene.switch");

        assert!(steps.try_begin("row-1", "obs.scene.switch"));
    }

    #[test]
    fn finishing_a_step_that_is_not_running_leaves_the_running_ones_blocked() {
        let mut steps = InFlightSteps::default();
        steps.try_begin("row-1", "obs.scene.switch");

        steps.finish("row-1", "obs.source.toggle");
        steps.finish("row-2", "obs.scene.switch");
        steps.finish("row-9", "never.started");

        assert!(!steps.try_begin("row-1", "obs.scene.switch"));
    }
}
