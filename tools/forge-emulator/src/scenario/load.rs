use std::path::Path;

use super::model::Scenario;
use crate::EmulatorError;

const BYTE_ORDER_MARK: char = '\u{feff}';

pub fn load_scenario(path: &Path) -> Result<Scenario, EmulatorError> {
    let text = std::fs::read_to_string(path).map_err(|e| EmulatorError::ScenarioUnreadable {
        path: path.to_owned(),
        reason: e.to_string(),
    })?;
    parse_scenario(path, &text)
}

/// `path` only labels errors; nothing is read from it.
pub fn parse_scenario(path: &Path, text: &str) -> Result<Scenario, EmulatorError> {
    let text = text.strip_prefix(BYTE_ORDER_MARK).unwrap_or(text);
    let scenario: Scenario =
        serde_json::from_str(text).map_err(|e| EmulatorError::ScenarioSyntax {
            path: path.to_owned(),
            line: e.line(),
            column: e.column(),
            reason: without_position(&e),
        })?;
    let problems = scenario.problems();
    if problems.is_empty() {
        Ok(scenario)
    } else {
        Err(EmulatorError::ScenarioInvalid {
            path: path.to_owned(),
            problems,
        })
    }
}

fn without_position(error: &serde_json::Error) -> String {
    let full = error.to_string();
    let suffix = format!(" at line {} column {}", error.line(), error.column());
    full.strip_suffix(&suffix).unwrap_or(&full).to_owned()
}
