use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use super::bug::{BugEntry, bug_entries};
use super::markdown::render;
use super::text::shell_word;
use crate::EmulatorError;
use crate::launch::DEFAULT_LOG_DIRECTIVES;
use crate::run::{ScenarioOutcome, ScenarioVerdict};
use crate::scenario::Scenario;

pub const MARKDOWN_FILE: &str = "report.md";
pub const JSON_FILE: &str = "report.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunContext {
    pub scenario_file: PathBuf,
    pub forge_binary: PathBuf,
    pub log_directives: String,
    pub run_root: PathBuf,
    #[serde(with = "time::serde::rfc3339")]
    pub started_at: OffsetDateTime,
    pub duration_ms: u64,
}

/// Everything a report shows, already scrubbed of the run's secrets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunReport {
    pub run: RunContext,
    pub scenario: Scenario,
    pub outcome: ScenarioOutcome,
    pub bugs: Vec<BugEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportFiles {
    pub markdown: PathBuf,
    pub json: PathBuf,
}

impl RunReport {
    /// Every string, scenario and evidence alike, passes through the outcome's redactions.
    pub fn new(
        run: RunContext,
        scenario: &Scenario,
        outcome: &ScenarioOutcome,
    ) -> Result<Self, EmulatorError> {
        let draft = Self {
            run,
            scenario: scenario.clone(),
            outcome: outcome.clone(),
            bugs: bug_entries(scenario, outcome),
        };
        let unbuildable = |e: serde_json::Error| EmulatorError::Output {
            reason: format!("scenario report: {e}"),
        };
        let mut value = serde_json::to_value(&draft).map_err(unbuildable)?;
        outcome.redactions.scrub_value(&mut value);
        serde_json::from_value(value).map_err(unbuildable)
    }

    pub fn to_json(&self) -> Result<String, EmulatorError> {
        serde_json::to_string_pretty(self).map_err(|e| EmulatorError::Output {
            reason: format!("scenario report: {e}"),
        })
    }

    pub fn to_markdown(&self) -> String {
        render(self)
    }

    pub fn verdict_line(&self) -> String {
        let verdict = match self.outcome.verdict {
            ScenarioVerdict::Passed => "passed".to_owned(),
            ScenarioVerdict::Failed => match self.bugs.len() {
                1 => "FAILED, 1 bug".to_owned(),
                count => format!("FAILED, {count} bugs"),
            },
            ScenarioVerdict::Interrupted => "interrupted".to_owned(),
        };
        format!("scenario `{}`: {verdict}", self.outcome.name)
    }

    pub fn repro_command(&self) -> String {
        let mut command = format!(
            "forge-emulator scenario run {} --forge {}",
            shell_word(&self.run.scenario_file.to_string_lossy()),
            shell_word(&self.run.forge_binary.to_string_lossy())
        );
        if self.run.log_directives != DEFAULT_LOG_DIRECTIVES {
            command.push_str(&format!(" --log {}", shell_word(&self.run.log_directives)));
        }
        command
    }
}

/// Writes the Markdown and JSON reports into `dir`, replacing any from an earlier run there.
pub fn write_report(report: &RunReport, dir: &Path) -> Result<ReportFiles, EmulatorError> {
    let files = ReportFiles {
        markdown: dir.join(MARKDOWN_FILE),
        json: dir.join(JSON_FILE),
    };
    write(&files.json, &report.to_json()?)?;
    write(&files.markdown, &report.to_markdown())?;
    Ok(files)
}

fn write(path: &Path, contents: &str) -> Result<(), EmulatorError> {
    std::fs::write(path, contents).map_err(|e| EmulatorError::ReportWrite {
        path: path.to_owned(),
        reason: e.to_string(),
    })
}
