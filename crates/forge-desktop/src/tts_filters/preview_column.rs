use super::preview::SPOKEN_QUOTE;
use super::preview::{MarkTint, PreviewMark};
use super::{TtsFiltersView, mono_caption};
use forge_components::{
    BORDER_THIN, Density, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon, Radius, Spacing,
    body_family, card, icon, mono_family, radius, spacing, tr,
};
use forge_tts_pipeline::{PipelineResult, SkipReason, StageAction, StageName, StageOutcome};
use gpui::prelude::*;
use gpui::{
    AnyElement, ClickEvent, Context, HighlightStyle, Hsla, Pixels, Rgba, SharedString, StyledText,
    div, px,
};

const PREVIEW_W: Pixels = px(320.0);

impl TtsFiltersView {
    pub(super) fn preview_column(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let gap_md = spacing(Spacing::Sm, density);

        let header = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(icon(Icon::Eye, FONT_SM, palette.brand))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(tr!("tts_filters_preview_header")),
            );

        let input_block = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(mono_caption(
                tr!("tts_filters_preview_input_label"),
                palette,
            ))
            .child(self.preview_input.clone());

        let mut stages_section = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(mono_caption(
                tr!("tts_filters_preview_output_label"),
                palette,
            ));

        if let Some(preview) = &self.cached_preview {
            for (i, outcome) in preview.stages.iter().enumerate() {
                let marks = self.preview_marks(
                    &outcome.output,
                    &outcome.replacements,
                    &preview.replacement_sources,
                );
                stages_section = stages_section.child(preview_stage_card(
                    u32::try_from(i + 1).unwrap_or(u32::MAX),
                    stage_name_label(outcome.stage),
                    outcome,
                    &marks,
                    palette,
                    density,
                ));
            }
            let spoken_marks = match &preview.result {
                PipelineResult::Speak(spoken) => self.spoken_marks(preview, spoken),
                PipelineResult::Skip { .. } => Vec::new(),
            };
            stages_section = stages_section.child(final_output_card(
                u32::try_from(preview.stages.len() + 1).unwrap_or(u32::MAX),
                &preview.result,
                &spoken_marks,
                palette,
                density,
            ));
        } else {
            stages_section = stages_section.child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(tr!("tts_filters_preview_empty")),
            );
        }

        let speak_btn = div()
            .id("filt-speak-preview")
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Xs, density))
            .rounded(radius(Radius::Sm))
            .bg(palette.brand)
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.speak_preview(cx)))
            .child(icon(Icon::PlayerPlayFilled, FONT_XS, palette.shell))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.shell)
                    .child(tr!("tts_filters_speak_preview_btn")),
            );

        let inner = div()
            .flex()
            .flex_col()
            .gap(gap_md)
            .child(header)
            .child(input_block)
            .child(stages_section)
            .child(speak_btn);

        div()
            .w(PREVIEW_W)
            .flex_none()
            .h_full()
            .bg(palette.shell)
            .border_l(BORDER_THIN)
            .border_color(palette.border_regular)
            .flex()
            .flex_col()
            .child(
                div()
                    .id("filt-preview")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .p(spacing(Spacing::Md, density))
                    .child(inner),
            )
            .into_any_element()
    }
}

fn preview_stage_card(
    n: u32,
    name: String,
    outcome: &StageOutcome,
    marks: &[PreviewMark],
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let body: AnyElement = match &outcome.action {
        StageAction::PassedThrough => div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.success)
                    .child("\u{2713}"),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(tr!("tts_filters_stage_pass")),
            )
            .into_any_element(),
        StageAction::Transformed => div()
            .font_family(body_family())
            .text_size(FONT_XS)
            .text_color(palette.text_primary)
            .child(marked_text(
                outcome.output.clone(),
                marks,
                Some(palette.surface_overlay),
                palette,
            ))
            .into_any_element(),
        StageAction::Skipped { reason } => div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.danger)
                    .child("\u{d7}"),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.danger)
                    .child(format!(
                        "{}: {}",
                        tr!("tts_filters_stage_skipped"),
                        skip_reason_label(reason)
                    )),
            )
            .into_any_element(),
    };

    stage_card_frame(format!("{n} \u{b7} {name}"), body, palette, density)
}

fn final_output_card(
    n: u32,
    result: &PipelineResult,
    marks: &[PreviewMark],
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let (text, color) = match result {
        PipelineResult::Speak(spoken) => (
            format!("{SPOKEN_QUOTE}{spoken}{SPOKEN_QUOTE}"),
            palette.text_primary,
        ),
        PipelineResult::Skip { reason } => (
            format!(
                "{}: {}",
                tr!("tts_filters_stage_skipped"),
                skip_reason_label(reason)
            ),
            palette.danger,
        ),
    };

    stage_card_frame(
        format!("{} \u{b7} {}", n, tr!("tts_filters_preview_final_label")),
        div()
            .font_family(body_family())
            .text_size(FONT_XS)
            .text_color(color)
            .child(marked_text(text, marks, None, palette))
            .into_any_element(),
        palette,
        density,
    )
}

fn stage_card_frame(
    label: String,
    body: AnyElement,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    card(
        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(label),
            )
            .child(body),
        palette,
    )
    .radius(Radius::Md)
    .padding_xy(spacing(Spacing::Xs, density), spacing(Spacing::Sm, density))
    .full_width()
    .into_any_element()
}

fn mark_color(tint: MarkTint, palette: &ForgePalette) -> Rgba {
    match tint {
        MarkTint::Blocklist => palette.danger,
        MarkTint::UrlMode => palette.info,
        MarkTint::ReplacementRow(ordinal) => {
            let rotation = [
                palette.success,
                palette.warning,
                palette.brand,
                palette.accent_peach,
                palette.accent_pink_light,
                palette.accent_teal,
            ];
            rotation[ordinal % rotation.len()]
        }
    }
}

fn marked_text(
    text: String,
    marks: &[PreviewMark],
    background: Option<Rgba>,
    palette: &ForgePalette,
) -> StyledText {
    let highlights = marks
        .iter()
        .map(|mark| {
            (
                mark.range.clone(),
                HighlightStyle {
                    color: Some(Hsla::from(mark_color(mark.tint, palette))),
                    background_color: background.map(Hsla::from),
                    ..HighlightStyle::default()
                },
            )
        })
        .collect::<Vec<_>>();
    StyledText::new(SharedString::from(text)).with_highlights(highlights)
}

fn stage_name_label(stage: StageName) -> String {
    match stage {
        StageName::SkipRules => tr!("tts_filters_stage_name_skip_rules"),
        StageName::WordBlocklist => tr!("tts_filters_stage_name_blocklist"),
        StageName::TextReplacements => tr!("tts_filters_stage_name_replacements"),
        StageName::Output => tr!("tts_filters_stage_name_output"),
    }
}

fn skip_reason_label(reason: &SkipReason) -> String {
    match reason {
        SkipReason::MatchedSkipRule(_) => tr!("tts_filters_skip_reason_rule"),
        SkipReason::BlockedByWordFilter => tr!("tts_filters_skip_reason_blocked"),
        SkipReason::EmptyAfterProcessing => tr!("tts_filters_skip_reason_empty"),
    }
}
