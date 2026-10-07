use std::ops::Range;

use crate::{
    LiteralScope, PipelineConfig, PipelineContext, ReplacementRule, StageName, StageOut,
    equals_ignoring_case, literal_replace_logged, run_stage, stage_output_logged,
    stage_word_blocklist_logged,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedReplacement {
    pub output_range: Range<usize>,
    pub original: String,
    pub replacement: String,
    pub origin: ReplacementOrigin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReplacementOrigin {
    ReplacementRuleIndex(usize),
    BlockedWordIndex(usize),
}

pub(crate) trait EditLog {
    fn record(&mut self, output: Range<usize>, original: &str, replacement: &str);

    fn end_pass(&mut self) {}
}

pub(crate) struct Unlogged;

impl EditLog for Unlogged {
    fn record(&mut self, _output: Range<usize>, _original: &str, _replacement: &str) {}
}

struct PassEdit {
    output: Range<usize>,
    original: String,
    replacement: String,
}

#[derive(Default)]
struct PassEdits(Vec<PassEdit>);

#[derive(Default)]
struct SubPassEdits {
    finished: Vec<Vec<PassEdit>>,
    open: PassEdits,
}

impl EditLog for SubPassEdits {
    fn record(&mut self, output: Range<usize>, original: &str, replacement: &str) {
        self.open.record(output, original, replacement);
    }

    fn end_pass(&mut self) {
        self.finished.push(std::mem::take(&mut self.open.0));
    }
}

impl EditLog for PassEdits {
    fn record(&mut self, output: Range<usize>, original: &str, replacement: &str) {
        self.0.push(PassEdit {
            output,
            original: original.to_owned(),
            replacement: replacement.to_owned(),
        });
    }
}

pub(crate) fn run_stage_traced(
    stage: StageName,
    text: &str,
    config: &PipelineConfig,
    context: &PipelineContext,
    prepend_display_name: bool,
    marks: &[AppliedReplacement],
) -> (StageOut, Vec<AppliedReplacement>) {
    match stage {
        StageName::SkipRules => (
            run_stage(stage, text, config, context, prepend_display_name),
            marks.to_vec(),
        ),
        StageName::WordBlocklist => blocklist_traced(text, config),
        StageName::TextReplacements => {
            let (output, carried) = replacements_traced(text, &config.replacement_rules, marks);
            (StageOut::Ok(output), carried)
        }
        StageName::Output => {
            let mut sub_passes = SubPassEdits::default();
            let output =
                stage_output_logged(text, config, context, prepend_display_name, &mut sub_passes);
            sub_passes.end_pass();
            let carried = sub_passes
                .finished
                .iter()
                .fold(marks.to_vec(), |carried, edits| {
                    shrink_through_pass(carried, edits)
                });
            (StageOut::Ok(output), carried)
        }
    }
}

fn blocklist_traced(text: &str, config: &PipelineConfig) -> (StageOut, Vec<AppliedReplacement>) {
    let mut edits = PassEdits::default();
    match stage_word_blocklist_logged(
        text,
        &config.word_blocklist,
        &config.blocklist_mode,
        &mut edits,
    ) {
        Ok(output) => {
            let marks = edits
                .0
                .into_iter()
                .filter_map(|edit| {
                    let index = blocked_word_index(&config.word_blocklist, &edit.original)?;
                    Some(AppliedReplacement {
                        output_range: edit.output,
                        original: edit.original,
                        replacement: edit.replacement,
                        origin: ReplacementOrigin::BlockedWordIndex(index),
                    })
                })
                .collect();
            (StageOut::Ok(output), marks)
        }
        Err(reason) => (StageOut::Skip(reason), Vec::new()),
    }
}

fn blocked_word_index(blocklist: &[String], word: &str) -> Option<usize> {
    blocklist
        .iter()
        .position(|blocked| equals_ignoring_case(word, blocked))
}

fn replacements_traced(
    text: &str,
    rules: &[ReplacementRule],
    marks: &[AppliedReplacement],
) -> (String, Vec<AppliedReplacement>) {
    let mut current = text.to_owned();
    let mut carried = marks.to_vec();
    for (index, rule) in rules.iter().enumerate() {
        let mut edits = PassEdits::default();
        current = match rule {
            ReplacementRule::Text {
                pattern,
                replacement,
            } => literal_replace_logged(
                &current,
                pattern,
                replacement,
                LiteralScope::AnywhereInText,
                &mut edits,
            ),
            ReplacementRule::WholeWord {
                pattern,
                replacement,
            } => literal_replace_logged(
                &current,
                pattern,
                replacement,
                LiteralScope::WholeWords,
                &mut edits,
            ),
            ReplacementRule::Regex {
                compiled,
                replacement,
            } => regex_replace_logged(&current, compiled, replacement, &mut edits),
        };
        carried = carry_through_pass(
            carried,
            edits.0,
            ReplacementOrigin::ReplacementRuleIndex(index),
        );
    }
    (current, carried)
}

fn regex_replace_logged(
    text: &str,
    compiled: &regex::Regex,
    replacement: &str,
    log: &mut impl EditLog,
) -> String {
    let mut result = String::with_capacity(text.len());
    let mut copied_until = 0usize;
    for captures in compiled.captures_iter(text) {
        let Some(whole) = captures.get(0) else {
            continue;
        };
        result.push_str(&text[copied_until..whole.start()]);
        let replaced_from = result.len();
        captures.expand(replacement, &mut result);
        log.record(
            replaced_from..result.len(),
            whole.as_str(),
            &result[replaced_from..],
        );
        copied_until = whole.end();
    }
    result.push_str(&text[copied_until..]);
    result
}

struct Splice {
    input: Range<usize>,
    output: Range<usize>,
}

fn splices(edits: &[PassEdit]) -> Vec<Splice> {
    let mut consumed_input = 0usize;
    let mut produced_output = 0usize;
    edits
        .iter()
        .map(|edit| {
            let input_start = edit.output.start - produced_output + consumed_input;
            consumed_input += edit.original.len();
            produced_output += edit.output.len();
            Splice {
                input: input_start..input_start + edit.original.len(),
                output: edit.output.clone(),
            }
        })
        .collect()
}

fn carry_through_pass(
    marks: Vec<AppliedReplacement>,
    edits: Vec<PassEdit>,
    origin: ReplacementOrigin,
) -> Vec<AppliedReplacement> {
    let spans = splices(&edits);
    let mut carried: Vec<AppliedReplacement> = marks
        .into_iter()
        .filter_map(|mark| shift_unless_rewritten(mark, &spans))
        .collect();
    carried.extend(edits.into_iter().map(|edit| AppliedReplacement {
        output_range: edit.output,
        original: edit.original,
        replacement: edit.replacement,
        origin,
    }));
    carried.sort_by_key(|mark| (mark.output_range.start, mark.output_range.end));
    carried
}

fn shift_unless_rewritten(
    mut mark: AppliedReplacement,
    spans: &[Splice],
) -> Option<AppliedReplacement> {
    let mut removed_before = 0usize;
    let mut inserted_before = 0usize;
    for span in spans {
        if rewrites(&span.input, &mark.output_range) {
            return None;
        }
        if span.input.end <= mark.output_range.start {
            removed_before += span.input.len();
            inserted_before += span.output.len();
        }
    }
    mark.output_range = mark.output_range.start - removed_before + inserted_before
        ..mark.output_range.end - removed_before + inserted_before;
    Some(mark)
}

fn rewrites(edit: &Range<usize>, mark: &Range<usize>) -> bool {
    let overlaps = edit.start < mark.end && mark.start < edit.end;
    let inserts_inside = edit.is_empty() && mark.start < edit.start && edit.start < mark.end;
    overlaps || inserts_inside
}

fn shrink_through_pass(
    marks: Vec<AppliedReplacement>,
    edits: &[PassEdit],
) -> Vec<AppliedReplacement> {
    let spans = splices(edits);
    marks
        .into_iter()
        .filter_map(|mut mark| {
            let start = surviving_start(mark.output_range.start, &spans);
            let end = surviving_end(mark.output_range.end, &spans);
            (start < end).then(|| {
                mark.output_range = start..end;
                mark
            })
        })
        .collect()
}

fn surviving_start(at: usize, spans: &[Splice]) -> usize {
    let mut removed_before = 0usize;
    let mut inserted_before = 0usize;
    for span in spans {
        if span.input.contains(&at) {
            return span.output.end;
        }
        if span.input.end <= at {
            removed_before += span.input.len();
            inserted_before += span.output.len();
        }
    }
    at - removed_before + inserted_before
}

fn surviving_end(at: usize, spans: &[Splice]) -> usize {
    let mut removed_before = 0usize;
    let mut inserted_before = 0usize;
    for span in spans {
        if span.input.start < at && at < span.input.end {
            return span.output.start;
        }
        if span.input.start < at && span.input.end <= at {
            removed_before += span.input.len();
            inserted_before += span.output.len();
        }
    }
    at - removed_before + inserted_before
}
