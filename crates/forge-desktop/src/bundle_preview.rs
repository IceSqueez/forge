use forge_components::{
    Density, FONT_XS, ForgePalette, Icon, Spacing, body_family, icon, spacing, tr,
};
use forge_types::LogLevel;
use forge_types::redaction::STAMP;
use gpui::{Pixels, SharedString, div, prelude::*, px};

use crate::diagnostic_bundle::{Bundle, RUN_HISTORY_LIMIT};

const PREVIEW_GLYPH: Pixels = px(15.0);
const BYTES_PER_KIB: u64 = 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StatementTone {
    Plain,
    Alert,
}

pub struct Statement {
    pub text: SharedString,
    pub tone: StatementTone,
}

fn plain(text: impl Into<SharedString>) -> Statement {
    Statement {
        text: text.into(),
        tone: StatementTone::Plain,
    }
}

fn alert(text: impl Into<SharedString>) -> Statement {
    Statement {
        text: text.into(),
        tone: StatementTone::Alert,
    }
}

fn kib(bytes: u64) -> i64 {
    bytes.div_ceil(BYTES_PER_KIB) as i64
}

pub fn export_statement(bundle: &Bundle, level: &LogLevel) -> Vec<Statement> {
    let mut notes = vec![
        plain(tr!(
            "settings_diagnostics_export_preview_includes",
            runs = i64::from(RUN_HISTORY_LIMIT)
        )),
        plain(tr!(
            "settings_diagnostics_export_preview_excludes",
            stamp = STAMP
        )),
        plain(tr!("settings_diagnostics_export_preview_lengths")),
    ];

    notes.push(if bundle.script_lines == 0 {
        plain(tr!("settings_diagnostics_export_preview_script_none"))
    } else {
        alert(tr!(
            "settings_diagnostics_export_preview_script",
            count = bundle.script_lines as i64
        ))
    });

    if bundle.command_lines > 0 {
        notes.push(alert(tr!(
            "settings_diagnostics_export_preview_trace",
            count = bundle.command_lines as i64
        )));
    } else if *level == LogLevel::Trace {
        notes.push(alert(tr!("settings_diagnostics_export_preview_trace_none")));
    }

    notes.push(plain(tr!(
        "settings_diagnostics_export_preview_size",
        size = kib(bundle.text.len() as u64)
    )));
    if bundle.elided_bytes > 0 {
        notes.push(plain(tr!(
            "settings_diagnostics_export_preview_elided",
            size = kib(bundle.elided_bytes)
        )));
    }
    notes
}

pub fn statement_row(
    note: Statement,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement + use<> {
    let (glyph, tint, text_color) = match note.tone {
        StatementTone::Plain => (Icon::Check, palette.text_faint, palette.text_secondary),
        StatementTone::Alert => (Icon::AlertTriangle, palette.warning, palette.text_primary),
    };
    div()
        .flex()
        .items_start()
        .gap(spacing(Spacing::Xs, density))
        .child(div().flex_none().child(icon(glyph, PREVIEW_GLYPH, tint)))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(text_color)
                .child(note.text),
        )
}
