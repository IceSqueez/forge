use std::ops::Range;

use crate::{
    PipelineConfig, PipelineContext, ReplacementRule, StageName, StageOut,
    case_insensitive_replace_logged, run_stage, sanitize_punctuation, spoken_name_prefix,
    stage_output, stage_word_blocklist_logged,
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
            let output = stage_output(text, config, context, prepend_display_name);
            let body_start = if prepend_display_name {
                spoken_prefix_len(context.viewer_name, config)
            } else {
                0
            };
            let carried = realign(marks, text, &output, body_start);
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
    let lower_word = word.to_lowercase();
    blocklist
        .iter()
        .position(|blocked| blocked.to_lowercase() == lower_word)
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
            } => case_insensitive_replace_logged(&current, pattern, replacement, &mut edits),
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

fn carry_through_pass(
    marks: Vec<AppliedReplacement>,
    edits: Vec<PassEdit>,
    origin: ReplacementOrigin,
) -> Vec<AppliedReplacement> {
    let mut consumed_input = 0usize;
    let mut produced_output = 0usize;
    let spans: Vec<(Range<usize>, Range<usize>)> = edits
        .iter()
        .map(|edit| {
            let input_start = edit.output.start - produced_output + consumed_input;
            consumed_input += edit.original.len();
            produced_output += edit.output.len();
            (
                input_start..input_start + edit.original.len(),
                edit.output.clone(),
            )
        })
        .collect();

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
    spans: &[(Range<usize>, Range<usize>)],
) -> Option<AppliedReplacement> {
    let mut removed_before = 0usize;
    let mut inserted_before = 0usize;
    for (input, output) in spans {
        if rewrites(input, &mark.output_range) {
            return None;
        }
        if input.end <= mark.output_range.start {
            removed_before += input.len();
            inserted_before += output.len();
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

fn spoken_prefix_len(viewer_name: &str, config: &PipelineConfig) -> usize {
    let prefix = spoken_name_prefix(viewer_name);
    if config.output.sanitize_punctuation {
        sanitize_punctuation(&prefix).len()
    } else {
        prefix.len()
    }
}

struct AlignedChar {
    source: usize,
    target: Option<Range<usize>>,
}

fn realign(
    marks: &[AppliedReplacement],
    source: &str,
    target: &str,
    body_start: usize,
) -> Vec<AppliedReplacement> {
    let alignment = align_surviving_chars(source, target, body_start);
    marks
        .iter()
        .filter_map(|mark| {
            let mut survivors = alignment
                .iter()
                .filter(|aligned| mark.output_range.contains(&aligned.source))
                .filter_map(|aligned| aligned.target.clone());
            let first = survivors.next()?;
            let end = survivors.next_back().map_or(first.end, |last| last.end);
            Some(AppliedReplacement {
                output_range: first.start..end,
                ..mark.clone()
            })
        })
        .collect()
}

fn align_surviving_chars(source: &str, target: &str, body_start: usize) -> Vec<AlignedChar> {
    let mut remaining = target
        .get(body_start..)
        .unwrap_or_default()
        .char_indices()
        .peekable();
    source
        .char_indices()
        .map(|(source_at, source_char)| {
            let target = match remaining.peek() {
                Some(&(target_at, target_char))
                    if source_char == target_char
                        || (source_char.is_whitespace() && target_char.is_whitespace()) =>
                {
                    remaining.next();
                    let start = body_start + target_at;
                    Some(start..start + target_char.len_utf8())
                }
                _ => None,
            };
            AlignedChar {
                source: source_at,
                target,
            }
        })
        .collect()
}
