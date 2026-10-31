use super::TtsFiltersView;
use forge_components::tr;
use forge_speak_queue::{
    EmoteTokenSet, Priority, ReplacementSource, RequestId, SpeakCommand, SpeakRequest, build_config,
};
use forge_storage::FilterRuleKind;
use forge_tts_pipeline::{AppliedReplacement, PipelineResult, ReplacementOrigin, StageOutcome};
use gpui::Context;
use std::ops::Range;

pub(super) const SPOKEN_QUOTE: &str = "\"";

pub(super) struct CachedPreview {
    pub(super) stages: Vec<StageOutcome>,
    pub(super) result: PipelineResult,
    pub(super) replacement_sources: Vec<ReplacementSource>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MarkTint {
    ReplacementRow(usize),
    UrlMode,
    Blocklist,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PreviewMark {
    pub(super) range: Range<usize>,
    pub(super) tint: MarkTint,
}

impl TtsFiltersView {
    pub(super) fn refresh_preview(&mut self, cx: &mut Context<Self>) {
        let input = self.preview_input.read(cx).content().to_owned();
        if input.is_empty() {
            self.cached_preview = None;
            return;
        }
        let shown = self.shown();
        let (config, replacement_sources) =
            build_config(&shown.rules, &shown.settings).dropping_invalid();
        let speaker_name = tr!("tts_filters_preview_speaker_name");
        let context = forge_tts_pipeline::PipelineContext {
            viewer_name: &speaker_name,
            recent_messages: &[],
            speech_language: None,
        };
        let (result, stages) = forge_tts_pipeline::preview(&input, &config, &context);
        self.cached_preview = Some(CachedPreview {
            stages,
            result,
            replacement_sources,
        });
    }

    fn replacement_ordinal(&self, row: usize) -> usize {
        self.shown()
            .rules
            .iter()
            .take(row)
            .filter(|rule| is_replacement_kind(&rule.kind))
            .count()
    }

    pub(super) fn mark_tint(
        &self,
        origin: ReplacementOrigin,
        sources: &[ReplacementSource],
    ) -> Option<MarkTint> {
        match origin {
            ReplacementOrigin::BlockedWordIndex(_) => Some(MarkTint::Blocklist),
            ReplacementOrigin::ReplacementRuleIndex(index) => match sources.get(index)? {
                ReplacementSource::UrlMode => Some(MarkTint::UrlMode),
                ReplacementSource::FilterRule(row) => {
                    Some(MarkTint::ReplacementRow(self.replacement_ordinal(*row)))
                }
            },
        }
    }

    pub(super) fn preview_marks(
        &self,
        text: &str,
        applied: &[AppliedReplacement],
        sources: &[ReplacementSource],
    ) -> Vec<PreviewMark> {
        let mut marks = Vec::with_capacity(applied.len());
        let mut covered_until = 0;
        for replacement in applied {
            let range = replacement.output_range.clone();
            let in_text = range.start < range.end
                && range.start >= covered_until
                && range.end <= text.len()
                && text.is_char_boundary(range.start)
                && text.is_char_boundary(range.end);
            if !in_text {
                continue;
            }
            let Some(tint) = self.mark_tint(replacement.origin, sources) else {
                continue;
            };
            covered_until = range.end;
            marks.push(PreviewMark { range, tint });
        }
        marks
    }

    pub(super) fn spoken_marks(&self, preview: &CachedPreview, spoken: &str) -> Vec<PreviewMark> {
        let Some(last) = preview.stages.last() else {
            return Vec::new();
        };
        if last.output != spoken {
            return Vec::new();
        }
        let shift = SPOKEN_QUOTE.len();
        self.preview_marks(spoken, &last.replacements, &preview.replacement_sources)
            .into_iter()
            .map(|mark| PreviewMark {
                range: mark.range.start + shift..mark.range.end + shift,
                tint: mark.tint,
            })
            .collect()
    }

    pub(super) fn speak_preview(&self, cx: &mut Context<Self>) {
        let text = self.preview_input.read(cx).content().trim().to_owned();
        if text.is_empty() {
            return;
        }
        let Some(handle) = self.speak.clone() else {
            return;
        };
        let speaker_name = tr!("tts_filters_preview_speaker_name");
        self.rt_handle.spawn(async move {
            let request = SpeakRequest {
                request_id: RequestId::new(),
                viewer_id: String::new(),
                viewer_name: speaker_name,
                text,
                priority: Priority::Normal,
                engine_override: None,
                voice_override: None,
                source_event_id: None,
                is_reward: false,
                target: None,
                message_emotes: EmoteTokenSet::default(),
            };
            if let Err(e) = handle.send(SpeakCommand::Enqueue(request)).await {
                tracing::warn!(error = %e, "tts filters preview speak failed");
            }
        });
    }
}

pub(super) fn is_replacement_kind(kind: &FilterRuleKind) -> bool {
    matches!(
        kind,
        FilterRuleKind::Literal { .. } | FilterRuleKind::Regex { .. }
    )
}
