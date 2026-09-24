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
