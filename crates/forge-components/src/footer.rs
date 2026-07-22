use gpui::{IntoElement, ParentElement, Pixels, Rgba, SharedString, Styled, div, px};

use crate::icons::{Icon, icon};
use crate::palette::ForgePalette;
use crate::status::status_dot;
use crate::tokens::{BORDER_THIN, Density, FONT_XS, Spacing, mono_family, spacing};

/// Overlays reserve this bottom clearance so they never paint under the bar.
pub const FOOTER_HEIGHT: Pixels = px(24.0);

// Deliberately off the `Spacing` scale: the footer is fixed, density-neutral chrome tuned by hand.
const LEFT_GAP: Pixels = px(8.0);
const VERSION_GAP: Pixels = px(5.0);
const CONN_GAP: Pixels = px(6.0);
const UPTIME_GAP: Pixels = px(5.0);
const RIGHT_GAP: Pixels = px(12.0);

const STATUS_DOT_SIZE: Pixels = px(6.0);

const CLOCK_ICON_SIZE: Pixels = px(10.0);

pub fn split_version_stage(version: &str) -> (&str, Option<&str>) {
    match version.split_once('-') {
        Some((base, stage)) => (base, Some(stage)),
        None => (version, None),
    }
}

fn mono_cell(text: impl Into<SharedString>, color: Rgba, size: Pixels) -> impl IntoElement {
    div()
        .font_family(mono_family())
        .text_size(size)
        .text_color(color)
        .child(text.into())
}

pub fn app_footer(
    app_label: impl Into<SharedString>,
    version: &str,
    connected: u8,
    connected_label: impl Into<SharedString>,
    uptime_label: impl Into<SharedString>,
    palette: &ForgePalette,
) -> impl IntoElement {
    let shell = palette.shell;
    let border_color = palette.border_regular;
    let text_muted = palette.text_muted;
    let text_faint = palette.text_faint;
    let text_secondary = palette.text_secondary;
    let success = palette.success;
    let brand = palette.brand;

    let (version_base, stage_tag) = split_version_stage(version);
    let mut version_group = div()
        .flex()
        .items_center()
        .gap(VERSION_GAP)
        .child(mono_cell(
            SharedString::from(format!("v{version_base}")),
            text_muted,
            FONT_XS,
        ));
    if let Some(stage) = stage_tag {
        version_group = version_group.child(mono_cell(
            SharedString::from(stage.to_owned()),
            brand,
            FONT_XS,
        ));
    }

    let left = div()
        .flex()
        .items_center()
        .gap(LEFT_GAP)
        .child(mono_cell(app_label, text_muted, FONT_XS))
        .child(mono_cell("·", text_faint, FONT_XS))
        .child(version_group);

    let dot_color = if connected == 0 { text_faint } else { success };
    let conn_row = div()
        .flex()
        .items_center()
        .gap(CONN_GAP)
        .child(status_dot(dot_color, STATUS_DOT_SIZE))
        .child(mono_cell(connected_label, text_secondary, FONT_XS));

    let uptime_row = div()
        .flex()
        .items_center()
        .gap(UPTIME_GAP)
        .child(icon(Icon::Clock, CLOCK_ICON_SIZE, text_faint))
        .child(mono_cell(uptime_label, text_secondary, FONT_XS));

    let right = div()
        .flex()
        .items_center()
        .gap(RIGHT_GAP)
        .child(conn_row)
        .child(mono_cell("·", text_faint, FONT_XS))
        .child(uptime_row);

    div()
        .w_full()
        .h(FOOTER_HEIGHT)
        .flex()
        .items_center()
        .justify_between()
        .px(spacing(Spacing::Md, Density::Cozy))
        .border(BORDER_THIN)
        .border_color(border_color)
        .bg(shell)
        .child(left)
        .child(right)
}

#[cfg(test)]
mod tests {
    use super::split_version_stage;

    #[test]
    fn split_version_stage_cuts_base_from_stage_on_first_dash() {
        for (input, expected) in [
            ("0.2.0", ("0.2.0", None)),
            ("0.2.0-beta.2", ("0.2.0", Some("beta.2"))),
            ("1.0.0-rc-1", ("1.0.0", Some("rc-1"))),
            ("1.0-", ("1.0", Some(""))),
            ("", ("", None)),
        ] {
            assert_eq!(split_version_stage(input), expected, "input {input:?}");
        }
    }
}
