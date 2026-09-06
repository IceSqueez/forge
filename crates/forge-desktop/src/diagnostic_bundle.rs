use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;

use forge_platform_core::HealthValue;
use forge_runtime::{COMMAND_LINE_TARGET, SCRIPT_LOG_TARGET};
use forge_types::LogLevel;
use forge_types::redaction::STAMP;
use forge_types::run_disclosure::{
    DisclosedOutcome, DisclosedRun, DisclosedStep, DisclosedStepOutcome, DisclosedTrigger,
    DisclosedValue,
};
use time::OffsetDateTime;

use crate::integrations::BuiltinRegistry;
use crate::log_archive;

pub const RUN_HISTORY_LIMIT: u32 = 25;

/// Well inside a 25 MB issue attachment and still openable in an editor on a slow machine.
const TOTAL_BUDGET: usize = 4 * 1024 * 1024;

const SCRIPT_SECTION_BUDGET: usize = 512 * 1024;

/// Headroom for the section framing written around the corpus after its budget is fixed.
const FRAMING_RESERVE: usize = 4096;

const RULE: &str =
    "================================================================================";

pub struct IntegrationFacts {
    pub id: String,
    pub connection: &'static str,
    pub limited: bool,
    pub version: Option<String>,
    pub uptime_secs: Option<u64>,
    pub endpoint_present: bool,
    pub token_expires_in_secs: Option<i64>,
    pub health: Vec<(String, String)>,
    pub quick_actions: usize,
}

pub struct BundleInput {
    pub log_dir: PathBuf,
    pub level: LogLevel,
    pub env_overridden: bool,
    pub integrations: Vec<IntegrationFacts>,
    pub config: BTreeMap<String, String>,
    pub runs: Vec<DisclosedRun>,
}

pub struct Bundle {
    pub text: String,
    pub script_lines: usize,
    pub command_lines: usize,
    pub elided_bytes: u64,
}

/// Reads only `BuiltinStatus` and `BuiltinHealth`, and only their non-naming fields: an account,
/// channel or endpoint value never reaches the returned facts.
pub fn integration_facts(builtins: &BuiltinRegistry) -> Vec<IntegrationFacts> {
    let now = std::time::SystemTime::now();
    let mut facts: Vec<IntegrationFacts> = builtins
        .snapshot()
        .iter()
        .map(|object| IntegrationFacts {
            id: object.status.id().as_str().to_owned(),
            connection: object.status.connection().label(),
            limited: object.status.capability_flags().limited,
            version: object.status.version().map(str::to_owned),
            uptime_secs: object.status.uptime().map(|d| d.as_secs()),
            endpoint_present: object.status.endpoint().is_some(),
            token_expires_in_secs: object.status.token_expiry().map(|at| {
                match at.duration_since(now) {
                    Ok(left) => left.as_secs() as i64,
                    Err(past) => -(past.duration().as_secs() as i64),
                }
            }),
            health: object
                .health
                .metrics()
                .iter()
                .map(|metric| (metric.label.clone(), health_shape(&metric.value)))
                .collect(),
            quick_actions: object.quick.actions().len(),
        })
        .collect();
    facts.sort_by(|a, b| a.id.cmp(&b.id));
    facts
}

/// Numbers and booleans survive; every string is dropped to its variant name, because a health
/// metric's text carries whatever the integration chose to put there.
fn health_shape(value: &HealthValue) -> String {
    match value {
        HealthValue::Status { active, .. } => format!("status(active={active})"),
        HealthValue::Text { .. } => "text".to_owned(),
        HealthValue::Pair { .. } => "pair".to_owned(),
        HealthValue::Ratio { used, total, .. } => format!("ratio({used}/{total})"),
    }
}

pub fn assemble(input: &BundleInput) -> Result<Bundle, String> {
    let mut out = String::new();
    write_header(&mut out, input);
    write_integrations(&mut out, &input.integrations);
    write_config(&mut out, &input.config);
    write_runs(&mut out, &input.runs);

    let room = TOTAL_BUDGET
        .saturating_sub(out.len())
        .saturating_sub(FRAMING_RESERVE);
    let corpus = log_archive::corpus(&input.log_dir, room)?;
    let command_lines = corpus
        .text
        .lines()
        .filter(|line| target_of(line) == Some(COMMAND_LINE_TARGET))
        .count();
    let (script, rest) = split_script_lines(&corpus.text);
    let script_lines = script.lines().count();
    let (script, script_elided) = keep_newest_lines(script, SCRIPT_SECTION_BUDGET);

    write_script_section(&mut out, &script, script_lines, script_elided);
    write_corpus_section(&mut out, &rest, &corpus, script_elided);

    Ok(Bundle {
        text: out,
        script_lines,
        command_lines,
        elided_bytes: corpus.elided_bytes + script_elided,
    })
}

/// The file layer writes `<timestamp> <LEVEL> <target>: <message>`, so the third field is the
/// target; matching on position rather than on a substring keeps a message that quotes a target
/// name from being counted as one.
fn target_of(line: &str) -> Option<&str> {
    let mut fields = line.split_whitespace();
    fields.next()?;
    fields.next()?;
    fields.next()?.strip_suffix(':')
}

fn split_script_lines(corpus: &str) -> (String, String) {
    let mut script = String::new();
    let mut rest = String::new();
    for line in corpus.lines() {
        let sink = if target_of(line) == Some(SCRIPT_LOG_TARGET) {
            &mut script
        } else {
            &mut rest
        };
        sink.push_str(line);
        sink.push('\n');
    }
    (script, rest)
}

fn keep_newest_lines(text: String, budget: usize) -> (String, u64) {
    if text.len() <= budget {
        return (text, 0);
    }
    let from = text.len() - budget;
    let start = match text[from..].find('\n') {
        Some(offset) => from + offset + 1,
        None => text.len(),
    };
    (text[start..].to_owned(), start as u64)
}

fn write_header(out: &mut String, input: &BundleInput) {
    let level_source = if input.env_overridden {
        "RUST_LOG"
    } else {
        "settings"
    };
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let _ = write!(
        out,
        "{RULE}\nforge diagnostic bundle\n{RULE}\n\
         version    : {}\n\
         os / arch  : {} / {}\n\
         build      : {profile}\n\
         log level  : {} (from {level_source})\n\
         generated  : {}\n\
         redaction  : a value shown as `{STAMP} len=N>` or `{STAMP}>` was withheld on purpose - \
         it is a redaction, not a truncated or failed read.\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        forge_storage::log_level_as_str(&input.level),
        stamp_time(OffsetDateTime::now_utc()),
    );
}

fn write_integrations(out: &mut String, facts: &[IntegrationFacts]) {
    section(out, "integrations");
    out.push_str(
        "(state only - no account name, channel name, endpoint or host value appears here)\n",
    );
    if facts.is_empty() {
        out.push_str("none installed\n");
        return;
    }
    for fact in facts {
        let _ = writeln!(
            out,
            "{:<12} state={:<13} capability={:<8} version={:<10} uptime={:<10} endpoint={:<6} token={:<16} quick_actions={}",
            fact.id,
            fact.connection,
            if fact.limited { "limited" } else { "full" },
            fact.version.as_deref().unwrap_or("-"),
            fact.uptime_secs
                .map_or_else(|| "-".to_owned(), |s| format!("{s}s")),
            if fact.endpoint_present {
                "set"
            } else {
                "unset"
            },
            fact.token_expires_in_secs
                .map_or_else(|| "none".to_owned(), |s| format!("expires_in={s}s")),
            fact.quick_actions,
        );
        if !fact.health.is_empty() {
            let metrics: Vec<String> = fact
                .health
                .iter()
                .map(|(label, shape)| format!("{label}={shape}"))
                .collect();
            let _ = writeln!(out, "  health: {}", metrics.join(" | "));
        }
    }
}

fn write_config(out: &mut String, config: &BTreeMap<String, String>) {
    section(out, "configuration");
    out.push_str(
        "(default-deny: a key with no disclosure class is absent entirely; `set` / `unset` means \
         the value exists but cannot be shown)\n",
    );
    if config.is_empty() {
        out.push_str("nothing disclosable\n");
        return;
    }
    for (key, value) in config {
        let _ = writeln!(out, "{key:<40} = {value}");
    }
}

fn write_runs(out: &mut String, runs: &[DisclosedRun]) {
    section(out, "run history");
    out.push_str(
        "(newest first; every string is its length only - no interpolated value appears here)\n",
    );
    if runs.is_empty() {
        out.push_str("no runs recorded\n");
        return;
    }
    for run in runs {
        let duration = run
            .completed_at
            .map(|at| (at - run.started_at).whole_milliseconds());
        let _ = writeln!(
            out,
            "\nrun action={} trigger={} started={} duration={} outcome={}",
            run.action_id,
            trigger_shape(&run.trigger),
            stamp_time(run.started_at),
            duration.map_or_else(|| "-".to_owned(), |ms| format!("{ms}ms")),
            outcome_shape(&run.outcome),
        );
        if !run.arguments.is_empty() {
            let _ = writeln!(out, "  args: {}", values_shape(&run.arguments));
        }
        for step in &run.steps {
            write_step(out, step);
        }
    }
}

fn write_step(out: &mut String, step: &DisclosedStep) {
    let _ = writeln!(
        out,
        "  step {:<4} {:<28} {:>7}ms {}",
        step.index
            .map_or_else(|| "-".to_owned(), |index| index.to_string()),
        step.kind,
        step.duration_ms,
        step_outcome_shape(&step.outcome),
    );
    if !step.args_in.is_empty() {
        let _ = writeln!(out, "    in  {{ {} }}", values_shape(&step.args_in));
    }
    if !step.produced.is_empty() {
        let _ = writeln!(out, "    out {{ {} }}", values_shape(&step.produced));
    }
}

fn values_shape(values: &BTreeMap<String, DisclosedValue>) -> String {
    values
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("  ")
}

fn trigger_shape(trigger: &DisclosedTrigger) -> String {
    match trigger {
        DisclosedTrigger::Event {
            event_id,
            trigger_kind,
        } => format!(
            "event({event_id}, kind={})",
            trigger_kind.as_deref().unwrap_or("-")
        ),
        DisclosedTrigger::QuickAction {
            builtin_id,
            label_chars,
        } => format!("quick_action({builtin_id}, label {STAMP} len={label_chars}>)"),
    }
}

fn outcome_shape(outcome: &DisclosedOutcome) -> String {
    match outcome {
        DisclosedOutcome::Success => "success".to_owned(),
        DisclosedOutcome::Cancelled => "cancelled".to_owned(),
        DisclosedOutcome::Failed { reason_chars } => {
            format!("failed(reason {STAMP} len={reason_chars}>)")
        }
    }
}

fn step_outcome_shape(outcome: &DisclosedStepOutcome) -> String {
    match outcome {
        DisclosedStepOutcome::Success => "success".to_owned(),
        DisclosedStepOutcome::Failed { reason_chars } => {
            format!("failed(reason {STAMP} len={reason_chars}>)")
        }
        DisclosedStepOutcome::Skipped { reason_chars } => {
            format!("skipped(reason {STAMP} len={reason_chars}>)")
        }
    }
}

fn write_script_section(out: &mut String, script: &str, total_lines: usize, elided: u64) {
    section(out, &format!("user script log ({total_lines} lines)"));
    out.push_str(
        "(written by the log step in your own actions and scripts - forge did not choose this \
         text; it is grouped here so you can read it before publishing)\n",
    );
    if elided > 0 {
        let _ = writeln!(
            out,
            "({elided} older bytes of this section did not fit and were left out)"
        );
    }
    if script.is_empty() {
        out.push_str("no script log lines\n");
        return;
    }
    out.push_str(script);
}

fn write_corpus_section(
    out: &mut String,
    rest: &str,
    corpus: &log_archive::Corpus,
    script_elided: u64,
) {
    let elided = corpus.elided_bytes + script_elided;
    section(
        out,
        &format!(
            "log ({} files, {} older bytes left out)",
            corpus.files_kept, elided
        ),
    );
    out.push_str("(script log lines were lifted into the section above)\n");
    if rest.is_empty() {
        out.push_str("no log lines\n");
        return;
    }
    out.push_str(rest);
}

fn section(out: &mut String, title: &str) {
    let _ = write!(out, "\n{RULE}\n-- {title}\n{RULE}\n");
}

fn stamp_time(at: OffsetDateTime) -> String {
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        at.year(),
        u8::from(at.month()),
        at.day(),
        at.hour(),
        at.minute(),
        at.second(),
    )
}
