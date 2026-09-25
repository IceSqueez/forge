use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use super::profile::StressProfile;
use super::runner::{Sample, StressOutcome};
use super::tracker::{Leg, percentile};
use crate::EmulatorError;

pub struct StressFiles {
    pub markdown: PathBuf,
    pub samples: PathBuf,
}

pub fn write_stress_report(
    profile: &StressProfile,
    outcome: &StressOutcome,
    run_root: &Path,
    forge_binary: &Path,
) -> Result<StressFiles, EmulatorError> {
    let markdown = run_root.join("stress-report.md");
    let samples = run_root.join("samples.jsonl");
    let failed = |path: &Path, e: std::io::Error| EmulatorError::ReportWrite {
        path: path.to_owned(),
        reason: e.to_string(),
    };
    std::fs::write(&markdown, render(profile, outcome, forge_binary))
        .map_err(|e| failed(&markdown, e))?;
    let mut lines = String::new();
    for sample in &outcome.samples {
        if let Ok(line) = serde_json::to_string(sample) {
            lines.push_str(&line);
            lines.push('\n');
        }
    }
    std::fs::write(&samples, lines).map_err(|e| failed(&samples, e))?;
    Ok(StressFiles { markdown, samples })
}

fn render(profile: &StressProfile, outcome: &StressOutcome, forge_binary: &Path) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Stress run: {}\n", profile.name);
    let _ = writeln!(out, "{}\n", profile.purpose);
    let _ = writeln!(
        out,
        "- forge: `{}` ({})\n- data dir: `{}`\n- senders: {}\n- forge exited during run: {}\n- shutdown: {}\n",
        forge_binary.display(),
        outcome.version.as_deref().unwrap_or("version unknown"),
        outcome.data_dir.display(),
        profile.senders,
        outcome.forge_exited,
        match (outcome.shutdown_ms, outcome.shutdown_forced) {
            (Some(ms), Some(true)) => format!("{ms} ms, killed after the grace period"),
            (Some(ms), _) => format!("{ms} ms, clean"),
            _ => "failed".to_owned(),
        }
    );

    let _ = writeln!(out, "## Phases\n");
    let _ = writeln!(
        out,
        "| # | phase | secs | sent / owed | observer dropped | verdict | peak RSS MiB | peak anon MiB | CPU avg / max % | UI thread avg / max % | threads max | fds max | DB MiB at end | load avg |"
    );
    let _ = writeln!(
        out,
        "|---:|---|---:|---:|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|"
    );
    for (index, result) in outcome.phases.iter().enumerate() {
        let samples: Vec<&Sample> = outcome
            .samples
            .iter()
            .filter(|s| s.phase == index)
            .collect();
        let stats = PhaseStats::of(&samples);
        let _ = writeln!(
            out,
            "| {index} | {} | {} | {} / {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            result.phase.label,
            result.phase.secs,
            result.sent,
            result.owed,
            result.observer_dropped,
            result.degraded.as_deref().unwrap_or("ok"),
            mib(stats.peak_rss_kib.map(|k| k * 1024)),
            mib(stats.peak_anon_kib.map(|k| k * 1024)),
            pair(stats.cpu_avg, stats.cpu_max),
            pair(stats.ui_avg, stats.ui_max),
            opt(stats.threads_max),
            opt(stats.fds_max),
            mib(stats.db_end),
            stats
                .load_avg
                .map_or_else(|| "-".to_owned(), |load| format!("{load:.1}")),
        );
    }

    let _ = writeln!(
        out,
        "\n## Latency from injection (ms, p50 / p95 / p99, count)\n"
    );
    let _ = writeln!(out, "| # | phase | leg | action | p50 | p95 | p99 | n |");
    let _ = writeln!(out, "|---:|---|---|---|---:|---:|---:|---:|");
    for (index, result) in outcome.phases.iter().enumerate() {
        let mut row = |leg: Leg, label: String, action: Option<usize>| {
            if let Some(values) = outcome.tracker.latencies(index, action, leg) {
                let _ = writeln!(
                    out,
                    "| {index} | {} | {label} | {} | {} | {} | {} | {} |",
                    result.phase.label,
                    action
                        .and_then(|a| outcome.actions.get(a))
                        .map_or("-", String::as_str),
                    opt(percentile(values, 50.0)),
                    opt(percentile(values, 95.0)),
                    opt(percentile(values, 99.0)),
                    values.len()
                );
            }
        };
        row(Leg::Ingest, "twitch event".to_owned(), None);
        for action in 0..outcome.actions.len() {
            row(Leg::Start, "action.start".to_owned(), Some(action));
            row(Leg::Done, "action.done".to_owned(), Some(action));
        }
        for (page, name) in outcome.pages.iter().enumerate() {
            row(Leg::Frame(page), format!("frame on `{name}`"), None);
        }
        row(Leg::Reply, "chat reply at Helix".to_owned(), None);
    }

    let _ = writeln!(out, "\n## Actions at the end of each phase\n");
    let _ = writeln!(
        out,
        "| # | phase | action | queue | started | done | skipped | unstarted | in flight |"
    );
    let _ = writeln!(out, "|---:|---|---|---|---:|---:|---:|---:|---:|");
    for (index, result) in outcome.phases.iter().enumerate() {
        let Some(last) = outcome.samples.iter().rfind(|s| s.phase == index) else {
            continue;
        };
        for (name, [started, done, skipped, unstarted, in_flight]) in &last.actions {
            let _ = writeln!(
                out,
                "| {index} | {} | {name} | {} | {started} | {done} | {skipped} | {unstarted} | {in_flight} |",
                result.phase.label,
                profile.queue_of(name),
            );
        }
    }

    let _ = writeln!(out, "\n## Skips by reason (whole run)\n");
    for (index, name) in outcome.actions.iter().enumerate() {
        let counts = outcome.tracker.counts(index);
        if !counts.skipped.is_empty() || counts.failed > 0 {
            let _ = writeln!(
                out,
                "- `{name}`: failed {}, skipped {:?}",
                counts.failed, counts.skipped
            );
        }
    }

    let _ = writeln!(out, "\n## Table rows at the end of each phase\n");
    for (index, result) in outcome.phases.iter().enumerate() {
        let busy: Vec<String> = result
            .tables
            .iter()
            .filter(|(_, rows)| **rows > 0)
            .map(|(name, rows)| format!("{name} {rows}"))
            .collect();
        let _ = writeln!(out, "- {index} {}: {}", result.phase.label, busy.join(", "));
    }

    let _ = writeln!(
        out,
        "\n## Observer\n\n- twitch events seen: {}\n- events the server dropped for the observer: {}\n- uncorrelated observations: {}\n- frames per page: {:?}\n- frames forge dropped per page: {:?}\n- chat replies seen: {}\n- other kinds: {:?}\n",
        outcome.tracker.twitch_events,
        outcome.tracker.observer_dropped,
        outcome.tracker.uncorrelated,
        outcome.tracker.frames,
        outcome.tracker.page_dropped,
        outcome.tracker.replies,
        outcome.tracker.other_kinds
    );

    let _ = writeln!(out, "## WARN / ERROR log lines (whole run)\n");
    let mut warnings: Vec<(&String, &u64)> = outcome.warnings_total.iter().collect();
    warnings.sort_by(|a, b| b.1.cmp(a.1));
    for (key, count) in warnings {
        let _ = writeln!(out, "- {count} x `{key}`");
    }
    out
}

#[derive(Default)]
struct PhaseStats {
    peak_rss_kib: Option<u64>,
    peak_anon_kib: Option<u64>,
    cpu_avg: Option<f64>,
    cpu_max: Option<f64>,
    ui_avg: Option<f64>,
    ui_max: Option<f64>,
    threads_max: Option<u64>,
    fds_max: Option<u64>,
    db_end: Option<u64>,
    load_avg: Option<f64>,
}

impl PhaseStats {
    fn of(samples: &[&Sample]) -> Self {
        let max_u = |f: fn(&Sample) -> Option<u64>| samples.iter().filter_map(|s| f(s)).max();
        let floats = |f: fn(&Sample) -> Option<f64>| -> Vec<f64> {
            samples.iter().filter_map(|s| f(s)).collect()
        };
        let avg = |values: &[f64]| {
            (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
        };
        let max = |values: &[f64]| values.iter().copied().reduce(f64::max);
        let cpu = floats(|s| s.cpu_percent);
        let ui = floats(|s| s.main_thread_percent);
        let load = floats(|s| s.load_average);
        Self {
            peak_rss_kib: max_u(|s| s.process.rss_kib),
            peak_anon_kib: max_u(|s| s.process.anonymous_kib),
            cpu_avg: avg(&cpu),
            cpu_max: max(&cpu),
            ui_avg: avg(&ui),
            ui_max: max(&ui),
            threads_max: max_u(|s| s.process.threads),
            fds_max: max_u(|s| s.process.fds),
            db_end: samples.last().and_then(|s| s.database_bytes),
            load_avg: avg(&load),
        }
    }
}

fn opt<T: std::fmt::Display>(value: Option<T>) -> String {
    value.map_or_else(|| "-".to_owned(), |v| v.to_string())
}

fn mib(bytes: Option<u64>) -> String {
    bytes.map_or_else(
        || "-".to_owned(),
        |b| format!("{:.1}", b as f64 / 1_048_576.0),
    )
}

fn pair(avg: Option<f64>, max: Option<f64>) -> String {
    match (avg, max) {
        (Some(avg), Some(max)) => format!("{avg:.0} / {max:.0}"),
        _ => "-".to_owned(),
    }
}
