use gpui::{IntoElement, ParentElement, Pixels, Rgba, SharedString, Styled, div, px};

use crate::icons::{Icon, icon};
use crate::palette::ForgePalette;
use crate::status::status_dot;
use crate::tokens::{BORDER_THIN, DEFAULT_MONO_FAMILY, Density, FONT_XS, Spacing, spacing};

/// Fixed drawn height of [`app_footer`]. Exported so overlays (e.g. a toast
/// viewport) can reserve exactly this much bottom clearance and never paint under
/// the bar.
pub const FOOTER_HEIGHT: Pixels = px(24.0);

/// Inter-cell gaps carried as literals. The source pins each of these off the
/// shared `Spacing` scale — the footer is a fixed, density-neutral chrome strip,
/// so its cell rhythm is tuned by hand (an 8px cluster gap, a tight 5px pairing
/// inside the version and uptime cells, a 12px gap between the two right-hand
/// groups) rather than snapped to the nearest token step.
const LEFT_GAP: Pixels = px(8.0);
const VERSION_GAP: Pixels = px(5.0);
const CONN_GAP: Pixels = px(6.0);
const UPTIME_GAP: Pixels = px(5.0);
const RIGHT_GAP: Pixels = px(12.0);

/// Connection status dot diameter, matching the source's fixed 6px disc.
const STATUS_DOT_SIZE: Pixels = px(6.0);

/// Leading clock glyph size for the uptime cell. Pinned at a literal 10px, a step
/// below the surrounding `FONT_XS` text so the icon reads lighter than the label.
const CLOCK_ICON_SIZE: Pixels = px(10.0);

/// Split a version string into a base and an optional prerelease stage tag.
///
/// A prerelease version splits on the first `-`: `"0.2.0-beta.2"` yields
/// `("0.2.0", Some("beta.2"))`. A plain release version has no stage tag:
/// `"1.0.0"` yields `("1.0.0", None)`. The footer inks the base muted and the
/// stage tag in the brand accent.
pub fn split_version_stage(version: &str) -> (&str, Option<&str>) {
    match version.split_once('-') {
        Some((base, stage)) => (base, Some(stage)),
        None => (version, None),
    }
}

/// A single monospace text cell at a given ink and size. The whole footer renders
/// in the monospace family, so every label routes through here.
fn mono_cell(text: impl Into<SharedString>, color: Rgba, size: Pixels) -> impl IntoElement {
    div()
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(size)
        .text_color(color)
        .child(text.into())
}

/// The application status bar pinned to the bottom of the shell: an app-name and
/// version cluster on the left, a connection indicator and uptime readout on the
/// right, split apart by the free space between them.
///
/// The bar fills a `shell` background under a thin `border_regular` rule with
/// square corners, at the fixed [`FOOTER_HEIGHT`]. Every label renders in the
/// monospace family at `FONT_XS`. Left cluster: the app label (`text_muted`), a
/// `·` divider (`text_faint`), then the version — its base inked `text_muted` and,
/// for a prerelease, a stage tag inked in the brand accent (see
/// [`split_version_stage`]). Right cluster: a status dot (the brand `success` hue
/// when anything is connected, `text_faint` when nothing is) beside the connection
/// label (`text_secondary`), a `·` divider (`text_faint`), then a clock glyph
/// (`text_faint`) beside the uptime label (`text_secondary`).
///
/// The kit carries no localization of its own, so the caller passes already-
/// localized strings for the app, connection, and uptime labels; the numeric
/// `connected` count is kept because it drives the dot's connected/idle color.
/// Colors resolve from `palette` up front.
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

    // The contract that matters: split on the FIRST `-`, not the last and not
    // on `.`. A clean release yields no stage tag (drives the footer's muted-only
    // rendering); a prerelease yields the stage tag verbatim (drives brand-accent
    // coloring). The multi-dash case (`rc-1`) is the load-bearing one — a
    // `rsplit_once('-')` mis-impl would yield `("1.0.0-rc", Some("1"))` and fail.
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
