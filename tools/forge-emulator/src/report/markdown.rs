use std::fmt::Write;

use time::format_description::well_known::Rfc3339;

use super::actual::request_line;
use super::bug::BugEntry;
use super::document::{JSON_FILE, RunReport};
use super::expected::source_name;
use super::text::{cell, clip, code, compact, elapsed, fence, plural};
use crate::run::{
    ActionReport, Evidence, ExpectationOutcome, ForgeEvidence, GapKind, JournaledEvent,
    LedgerExcerpt, ScenarioVerdict, StepOutcome, StepStatus, Verdict,
};

const LISTED: usize = 5;
const PAYLOAD_CHARS: usize = 200;
const LINE_CHARS: usize = 400;
const TAIL_LINES: usize = 20;

pub(crate) fn render(report: &RunReport) -> String {
    let mut out = String::new();
    header(&mut out, report);
    repro(&mut out, report);
    steps(&mut out, &report.outcome.steps);
    match report.outcome.verdict {
        ScenarioVerdict::Passed => {}
        ScenarioVerdict::Interrupted => {
            out.push_str(
                "## Bugs\n\nThe run was interrupted, so nothing was judged a bug. Rerun it to completion.\n\n",
            );
        }
        ScenarioVerdict::Failed => {
            bugs(&mut out, report);
            if let Some(forge) = &report.outcome.forge {
                forge_output(&mut out, forge);
            }
        }
    }
    out
}

fn header(out: &mut String, report: &RunReport) {
    let outcome = &report.outcome;
    let verdict = match outcome.verdict {
        ScenarioVerdict::Passed => "passed".to_owned(),
        ScenarioVerdict::Failed => format!("FAILED ({})", plural(report.bugs.len() as u64, "bug")),
        ScenarioVerdict::Interrupted => "interrupted".to_owned(),
    };
    let started = report
        .run
        .started_at
        .format(&Rfc3339)
        .unwrap_or_else(|_| report.run.started_at.unix_timestamp().to_string());
    let _ = writeln!(out, "# Scenario {}: {verdict}\n", code(&outcome.name));
    let _ = writeln!(out, "{}\n", quote_block(&report.scenario.purpose));
    out.push_str("| Run | |\n| :--- | :--- |\n");
    let mut row = |label: &str, value: String| {
        let _ = writeln!(out, "| {label} | {} |", cell(&value));
    };
    row("Verdict", verdict);
    row("Started", started);
    row("Duration", elapsed(report.run.duration_ms));
    match &outcome.forge {
        Some(forge) => {
            row("forge", forge_identity(forge));
            row("forge exit", forge_exit(forge));
            if let Some(error) = &forge.teardown_error {
                row("Teardown error", error.clone());
            }
            row("forge logs", code(&forge.log_dir.to_string_lossy()));
        }
        None => row("forge", "never became ready".to_owned()),
    }
    row(
        "forge binary",
        code(&report.run.forge_binary.to_string_lossy()),
    );
    row(
        "Scenario file",
        code(&report.run.scenario_file.to_string_lossy()),
    );
    row("Run root", code(&report.run.run_root.to_string_lossy()));
    row("Machine-readable", code(JSON_FILE));
    out.push('\n');
}

fn forge_identity(forge: &ForgeEvidence) -> String {
    let version = forge.version.as_ref().map_or_else(
        || "version unknown".to_owned(),
        |v| format!("version {}", code(v)),
    );
    format!(
        "{version}, pid {}, {}",
        forge.pid,
        plural(u64::from(forge.attempts), "launch attempt")
    )
}

fn forge_exit(forge: &ForgeEvidence) -> String {
    let mut text = match &forge.exit {
        Some(exit) => code(&exit.status),
        None => "unknown".to_owned(),
    };
    if forge.exit.as_ref().is_some_and(|exit| exit.forced) {
        text.push_str(", killed after the shutdown grace period");
    }
    if forge.exited_during_run {
        text.push_str(", exited on its own during the run");
    }
    text
}

fn repro(out: &mut String, report: &RunReport) {
    out.push_str("## Repro\n\n");
    out.push_str(&fence("sh", &report.repro_command()));
    let _ = writeln!(
        out,
        "\nThe scenario file {} is the whole reproduction: attach it to the issue.\n",
        code(&report.run.scenario_file.to_string_lossy())
    );
}

fn steps(out: &mut String, steps: &[StepOutcome]) {
    out.push_str("## Steps\n\n| # | Step | Status | Started | Acted | Expectations |\n| ---: | :--- | :--- | ---: | ---: | :--- |\n");
    for step in steps {
        let status = match step.status {
            StepStatus::Passed => "passed",
            StepStatus::Failed => "**FAILED**",
            StepStatus::Interrupted => "interrupted",
            StepStatus::NotRun => "not run",
        };
        let _ = writeln!(
            out,
            "| {} | {} | {status} | {} | {} | {} |",
            step.index,
            code(&step.keyword),
            millis(step.started_ms),
            millis(step.acted_ms),
            expectation_tally(&step.expectations)
        );
    }
    out.push('\n');
}

fn expectation_tally(expectations: &[ExpectationOutcome]) -> String {
    if expectations.is_empty() {
        return "none".to_owned();
    }
    let passed = expectations
        .iter()
        .filter(|e| e.verdict == Verdict::Passed)
        .count();
    let unevaluated = expectations
        .iter()
        .filter(|e| e.verdict == Verdict::NotEvaluated)
        .count();
    if unevaluated == expectations.len() {
        "not evaluated".to_owned()
    } else {
        format!("{passed}/{} passed", expectations.len())
    }
}

fn millis(ms: Option<u64>) -> String {
    ms.map_or_else(|| "-".to_owned(), |ms| format!("+{ms} ms"))
}

fn bugs(out: &mut String, report: &RunReport) {
    out.push_str("## Bugs\n\n");
    for (number, bug) in report.bugs.iter().enumerate() {
        bug_entry(out, number + 1, bug);
        let evidence = evidence_lines(report, bug);
        out.push_str("#### Evidence\n\n");
        for line in evidence {
            let _ = writeln!(out, "- {line}");
        }
        out.push('\n');
    }
}

fn bug_entry(out: &mut String, number: usize, bug: &BugEntry) {
    let _ = writeln!(out, "### Bug {number}: {}\n", bug.title);
    let _ = writeln!(out, "{}\n", sentence(&bug.story));
    let _ = writeln!(out, "**Expected:** {}\n", sentence(&bug.expected));
    let _ = writeln!(out, "**Actual:** {}\n", sentence(&bug.actual));
    for detail in &bug.details {
        let _ = writeln!(out, "- {detail}");
    }
    if !bug.details.is_empty() {
        out.push('\n');
    }
    if let Some(matcher) = &bug.matcher {
        let location = match (bug.step, bug.expectation) {
            (Some(step), Some(expectation)) => format!("step {step}, expectation {expectation}"),
            (Some(step), None) => format!("step {step}, action"),
            _ => "run".to_owned(),
        };
        let _ = writeln!(out, "Matcher ({location}):\n");
        out.push_str(&fence("json", matcher));
        out.push('\n');
    }
}

fn evidence_lines(report: &RunReport, bug: &BugEntry) -> Vec<String> {
    let step = bug
        .step
        .and_then(|index| report.outcome.steps.iter().find(|s| s.index == index));
    let mut lines = match (step, bug.expectation) {
        (Some(step), Some(index)) => step
            .expectations
            .iter()
            .find(|e| e.index == index)
            .map(|e| expectation_evidence(&e.evidence))
            .unwrap_or_default(),
        (Some(step), None) => match &step.action {
            Some(ActionReport::Failed {
                ledger: Some(ledger),
                ..
            }) => ledger_lines(ledger),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    };
    if report.outcome.forge.is_some() {
        lines.push("forge stderr and log tails: see [forge output](#forge-output)".to_owned());
    }
    lines
}

fn expectation_evidence(evidence: &Evidence) -> Vec<String> {
    match evidence {
        Evidence::None => Vec::new(),
        Evidence::Events(events) => {
            let mut lines = vec![format!("matching events in the window: {}", events.matched)];
            listed(&mut lines, "matched", &events.samples, event_line);
            listed(&mut lines, "late", &events.late, event_line);
            listed(&mut lines, "near miss", &events.near_misses, |miss| {
                event_line(&miss.event)
            });
            listed(&mut lines, "gap", &events.gaps, |gap| {
                format!("+{} ms {}", gap.arrived_ms, compact_gap(&gap.kind))
            });
            lines
        }
        Evidence::Causation(causation) => {
            let mut lines = Vec::new();
            if let Some(effect) = &causation.effect {
                lines.push(format!("effect: {}", event_line(effect)));
            }
            if let Some(cause) = &causation.cause {
                lines.push(format!("expected cause: {}", event_line(cause)));
            }
            listed(&mut lines, "ancestor", &causation.chain, event_line);
            lines
        }
        Evidence::Ledger(ledger) => ledger_lines(ledger),
        Evidence::Log(log) => {
            let mut lines = Vec::new();
            if let Some(record) = &log.matched {
                lines.push(format!(
                    "matched log line: {}",
                    code(&clip(&record.line, LINE_CHARS))
                ));
            }
            listed(&mut lines, "log line", &log.near_misses, |record| {
                code(&clip(&record.line, LINE_CHARS))
            });
            if !log.files.is_empty() {
                let files: Vec<String> = log
                    .files
                    .iter()
                    .map(|file| code(&file.to_string_lossy()))
                    .collect();
                lines.push(format!("log files read: {}", files.join(", ")));
            }
            lines
        }
    }
}

fn ledger_lines(ledger: &LedgerExcerpt) -> Vec<String> {
    let mut lines = Vec::new();
    listed(&mut lines, "session", &ledger.sessions, |session| {
        let mut line = format!(
            "{} opened {} ({})",
            code(&session.id),
            session.connected_at,
            if session.live { "live" } else { "closed" }
        );
        if let Some(from) = &session.reconnected_from {
            line.push_str(&format!(", reconnected from {}", code(from)));
        }
        line
    });
    listed(
        &mut lines,
        "subscription",
        &ledger.subscriptions,
        |subscription| {
            format!(
                "{} version {} on session {}",
                code(&subscription.subscription_type),
                code(&subscription.version),
                code(&subscription.session_id)
            )
        },
    );
    listed(&mut lines, "request", &ledger.requests, request_line);
    if lines.is_empty() {
        lines.push("the fake Twitch recorded nothing relevant".to_owned());
    }
    lines
}

fn listed<T>(lines: &mut Vec<String>, label: &str, items: &[T], line: impl Fn(&T) -> String) {
    for item in items.iter().take(LISTED) {
        lines.push(format!("{label}: {}", line(item)));
    }
    if items.len() > LISTED {
        lines.push(format!(
            "{label}: {} more in {}",
            items.len() - LISTED,
            code(JSON_FILE)
        ));
    }
}

fn event_line(event: &JournaledEvent) -> String {
    let caused = event
        .event
        .caused_by
        .map(|cause| format!(" caused by {}", code(&cause.to_string())))
        .unwrap_or_default();
    format!(
        "+{} ms {} {} {}{caused} {}",
        event.arrived_ms,
        source_name(event.event.source),
        code(&event.event.kind),
        code(&event.event.id.to_string()),
        code(&compact(&event.event.payload, PAYLOAD_CHARS))
    )
}

fn compact_gap(kind: &GapKind) -> String {
    match kind {
        GapKind::Dropped(count) => {
            format!("server dropped {}", plural(*count, "event"))
        }
        GapKind::Undecodable { frame, reason } => format!(
            "undecodable frame ({reason}) {}",
            code(&clip(frame, LINE_CHARS))
        ),
    }
}

fn forge_output(out: &mut String, forge: &ForgeEvidence) {
    out.push_str("## forge output\n\n");
    tail(out, "stderr", &forge.stderr_tail);
    tail(out, "log file", &forge.log_tail);
}

fn tail(out: &mut String, label: &str, lines: &[String]) {
    let skipped = lines.len().saturating_sub(TAIL_LINES);
    let _ = writeln!(out, "### {label}\n");
    if lines.is_empty() {
        out.push_str("Nothing captured.\n\n");
        return;
    }
    if skipped > 0 {
        let _ = writeln!(
            out,
            "Last {TAIL_LINES} lines; {} earlier in {}.\n",
            plural(skipped as u64, "line"),
            code(JSON_FILE)
        );
    }
    let body: Vec<String> = lines[skipped..]
        .iter()
        .map(|line| clip(line, LINE_CHARS))
        .collect();
    out.push_str(&fence("text", &body.join("\n")));
    out.push('\n');
}

fn quote_block(text: &str) -> String {
    text.lines()
        .map(|line| format!("> {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn sentence(text: &str) -> String {
    let text = text.trim_end();
    if text.ends_with(['.', '!', '?']) {
        text.to_owned()
    } else {
        format!("{text}.")
    }
}
