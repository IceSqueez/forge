use std::sync::Mutex;

use forge_storage::StorageError;
use forge_tts_pipeline::PipelineConfig;
use gpui::TestAppContext;

use super::add_filter_modal::ReplaceKind;
use super::filter_edits::{set_emote_to_word, set_strip_twitch_emotes};
use super::filter_options::{
    ModalStage, OutputOpt, OutputPreset, SkipPreset, SkipRule, parse_max_duration,
};
use super::preview::MarkTint;
use super::preview::SPOKEN_QUOTE;
use super::stage_cards::replacement_meta;
use super::*;
use crate::test_support::{install_presentation, pump, runtime};
use forge_speak_queue::{EmoteTreatment, ReplacementSource, configured_skip_prefix};
use forge_storage::{BlocklistMode, FilterRuleKind, SkipRegex};
use forge_tts_pipeline::{AppliedReplacement, PipelineResult, ReplacementOrigin};
use std::ops::Range;

#[derive(Clone, Copy, Debug)]
enum LoadFault {
    Unreachable,
    UndecodableRuleRow,
    SettingsUnreadable,
}

const STORE_UNREACHABLE: &str = "filters store unreachable";
const STORE_READ_ONLY: &str = "filters store is read-only";

impl LoadFault {
    fn error(self) -> StorageError {
        match self {
            LoadFault::Unreachable | LoadFault::SettingsUnreadable => StorageError::Connection {
                reason: STORE_UNREACHABLE.to_owned(),
            },
            LoadFault::UndecodableRuleRow => StorageError::Serialization(
                serde_json::from_str::<FilterRuleKind>(r#"{"kind":"vanished"}"#).unwrap_err(),
            ),
        }
    }
}

#[derive(Default)]
struct RecordingFilters {
    stored_rules: Mutex<Vec<FilterRule>>,
    replaced: Mutex<Vec<Vec<FilterRule>>>,
    saved: Mutex<Vec<TtsPipelineSettings>>,
    load_fault: Mutex<Option<LoadFault>>,
    load_gate: Mutex<Option<Arc<tokio::sync::Notify>>>,
    write_fails: std::sync::atomic::AtomicBool,
    loads: std::sync::atomic::AtomicUsize,
}

impl RecordingFilters {
    fn storing(rules: Vec<FilterRule>) -> Arc<Self> {
        Arc::new(Self {
            stored_rules: Mutex::new(rules),
            ..Self::default()
        })
    }

    fn failing_loads(self: Arc<Self>, fault: LoadFault) -> Arc<Self> {
        *self.load_fault.lock().unwrap() = Some(fault);
        self
    }

    fn heal(&self) {
        *self.load_fault.lock().unwrap() = None;
    }

    fn writes(&self) -> usize {
        self.replaced.lock().unwrap().len() + self.saved.lock().unwrap().len()
    }

    fn loads(&self) -> usize {
        self.loads.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl TtsFiltersRepo for RecordingFilters {
    async fn list_rules(&self) -> Result<Vec<FilterRule>, StorageError> {
        self.loads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let gate = self.load_gate.lock().unwrap().take();
        if let Some(gate) = gate {
            gate.notified().await;
        }
        let fault = *self.load_fault.lock().unwrap();
        match fault {
            Some(LoadFault::Unreachable | LoadFault::UndecodableRuleRow) => {
                Err(fault.unwrap().error())
            }
            _ => Ok(self.stored_rules.lock().unwrap().clone()),
        }
    }

    async fn replace_rules(&self, rules: &[FilterRule]) -> Result<(), StorageError> {
        if self.write_fails.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(StorageError::Connection {
                reason: STORE_READ_ONLY.to_owned(),
            });
        }
        self.replaced.lock().unwrap().push(rules.to_vec());
        *self.stored_rules.lock().unwrap() = rules.to_vec();
        Ok(())
    }

    async fn get_pipeline_settings(&self) -> Result<TtsPipelineSettings, StorageError> {
        let fault = *self.load_fault.lock().unwrap();
        match fault {
            Some(LoadFault::SettingsUnreadable) => Err(LoadFault::SettingsUnreadable.error()),
            _ => Ok(TtsPipelineSettings::default()),
        }
    }

    async fn set_pipeline_settings(
        &self,
        settings: &TtsPipelineSettings,
    ) -> Result<(), StorageError> {
        self.saved.lock().unwrap().push(settings.clone());
        Ok(())
    }
}

fn initial_config() -> PipelineConfigHandle {
    PipelineConfigHandle::new(
        forge_speak_queue::build_config(&[], &TtsPipelineSettings::default())
            .dropping_invalid()
            .0,
    )
}

fn filters_view(cx: &mut TestAppContext) -> Entity<TtsFiltersView> {
    install_presentation(cx);
    let rt = runtime();
    let view = cx.update(|cx| {
        cx.new(|cx| {
            TtsFiltersView::new(
                Arc::new(RecordingFilters::default()),
                None,
                Shared::new(Vec::new()),
                None,
                rt.handle().clone(),
                cx,
            )
        })
    });
    pump(&rt);
    cx.run_until_parked();
    view
}

fn view_over(
    repo: &Arc<RecordingFilters>,
    rt: &tokio::runtime::Runtime,
    cx: &mut TestAppContext,
) -> Entity<TtsFiltersView> {
    let repo = Arc::clone(repo);
    let view = cx.update(|cx| {
        cx.new(|cx| {
            TtsFiltersView::new(
                repo,
                None,
                Shared::new(Vec::new()),
                None,
                rt.handle().clone(),
                cx,
            )
        })
    });
    settle(rt, cx);
    view
}

const SETTLE_ROUNDS: usize = 3;

fn settle(rt: &tokio::runtime::Runtime, cx: &mut TestAppContext) {
    for _ in 0..SETTLE_ROUNDS {
        pump(rt);
        cx.run_until_parked();
    }
}

fn stored_rules() -> Vec<FilterRule> {
    let mut rules = vec![
        filter_row(true, blocked(&["gtfo"])),
        filter_row(true, literal("thanks", "дякую")),
    ];
    for (position, (rule, id)) in rules.iter_mut().zip(["block", "thanks"]).enumerate() {
        rule.id = id.to_owned();
        rule.position = position as u32;
    }
    rules
}

type Edit = fn(&mut TtsFiltersView, &mut Context<TtsFiltersView>);

fn edit_paths() -> [(&'static str, Edit); 11] {
    [
        ("toggle a skip rule", |v, cx| {
            v.toggle_skip(SkipRule::ContainsUrl, cx)
        }),
        ("toggle an output option", |v, cx| {
            v.toggle_output(OutputOpt::ReadName, cx)
        }),
        ("remove a skip regex", |v, cx| v.remove_skip_regex(0, cx)),
        ("switch the blocklist mode", |v, cx| {
            v.set_blocklist_mode(BlocklistMode::Suppress, cx)
        }),
        ("remove a blocked word", |v, cx| {
            v.remove_blocklist_word(0, 0, cx)
        }),
        ("toggle a rule", |v, cx| v.toggle_rule(0, cx)),
        ("delete a rule", |v, cx| v.delete_rule(0, cx)),
        ("add a skip preset", |v, cx| {
            v.apply_skip_preset(SkipPreset::Url, String::new(), cx);
        }),
        ("add an output preset", |v, cx| {
            v.apply_output_preset(OutputPreset::Name, "", cx);
        }),
        ("add blocked words", |v, cx| {
            v.apply_blocklist_words(vec!["nope".to_owned()], BlocklistMode::Censor, cx);
        }),
        ("add a replacement", |v, cx| {
            v.apply_replacement(
                ReplaceKind::Text,
                "gg".to_owned(),
                "good game".to_owned(),
                false,
                cx,
            );
        }),
    ]
}

#[gpui::test]
fn no_edit_after_a_failed_load_writes_over_the_stored_filters(cx: &mut TestAppContext) {
    install_presentation(cx);
    let rt = runtime();
    for fault in [
        LoadFault::Unreachable,
        LoadFault::UndecodableRuleRow,
        LoadFault::SettingsUnreadable,
    ] {
        for (case, edit) in edit_paths() {
            let shown = crate::test_support::shown_in_english(fault.error());
            let repo = RecordingFilters::storing(stored_rules()).failing_loads(fault);
            let view = view_over(&repo, &rt, cx);

            view.update(cx, edit);
            settle(&rt, cx);

            let failure = view.read_with(cx, |v, _| v.filters.failure());
            assert_eq!(
                (repo.writes(), failure),
                (0, Some(shown)),
                "{fault:?}: {case}"
            );
        }
    }
}

#[gpui::test]
fn no_edit_while_the_load_is_pending_writes_and_the_load_still_lands_intact(
    cx: &mut TestAppContext,
) {
    install_presentation(cx);
    let rt = runtime();
    for (case, edit) in edit_paths() {
        let repo = RecordingFilters::storing(stored_rules());
        let gate = Arc::new(tokio::sync::Notify::new());
        *repo.load_gate.lock().unwrap() = Some(Arc::clone(&gate));
        let view = view_over(&repo, &rt, cx);

        view.update(cx, edit);
        settle(&rt, cx);
        let writes_while_pending = repo.writes();
        gate.notify_one();
        settle(&rt, cx);

        let landed = view.read_with(cx, |v, _| v.filters.loaded().map(|f| f.rules.clone()));
        assert_eq!(
            (writes_while_pending, landed),
            (0, Some(stored_rules())),
            "{case}"
        );
    }
}

#[gpui::test]
fn retry_after_a_failed_load_loads_the_stored_filters_and_edits_persist_again(
    cx: &mut TestAppContext,
) {
    install_presentation(cx);
    let rt = runtime();
    let repo = RecordingFilters::storing(stored_rules()).failing_loads(LoadFault::Unreachable);
    let view = view_over(&repo, &rt, cx);
    repo.heal();

    view.update(cx, |v, cx| v.retry_load(cx));
    settle(&rt, cx);
    view.update(cx, |v, cx| v.toggle_rule(0, cx));
    settle(&rt, cx);

    let mut expected = stored_rules();
    expected[0].enabled = false;
    assert_eq!(repo.replaced.lock().unwrap().clone(), vec![expected]);
}

#[gpui::test]
fn a_failed_write_reports_the_error_and_reloads_what_the_store_holds(cx: &mut TestAppContext) {
    let read_only = crate::test_support::shown_in_english(StorageError::Connection {
        reason: STORE_READ_ONLY.to_owned(),
    });
    install_presentation(cx);
    let rt = runtime();
    let repo = RecordingFilters::storing(stored_rules());
    repo.write_fails
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let view = view_over(&repo, &rt, cx);

    view.update(cx, |v, cx| v.toggle_rule(0, cx));
    settle(&rt, cx);

    let (save_error, shown) = view.read_with(cx, |v, _| {
        (
            v.save_error.clone(),
            v.filters.loaded().map(|f| f.rules.clone()),
        )
    });
    assert_eq!(
        (save_error, repo.loads(), shown),
        (Some(read_only), 2, Some(stored_rules()))
    );
}

fn filter_row(enabled: bool, kind: FilterRuleKind) -> FilterRule {
    FilterRule {
        id: String::new(),
        name: String::new(),
        enabled,
        position: 0,
        kind,
    }
}

fn literal(pattern: &str, replacement: &str) -> FilterRuleKind {
    FilterRuleKind::Literal {
        pattern: pattern.into(),
        replacement: replacement.into(),
        match_inside_words: false,
    }
}

fn blocked(words: &[&str]) -> FilterRuleKind {
    FilterRuleKind::Blocklist {
        words: words.iter().map(|w| (*w).to_owned()).collect(),
        mode: BlocklistMode::Censor,
    }
}

fn applied(range: Range<usize>, origin: ReplacementOrigin) -> AppliedReplacement {
    AppliedReplacement {
        output_range: range,
        original: String::new(),
        replacement: String::new(),
        origin,
    }
}

#[gpui::test]
fn preview_marks_keep_only_ranges_that_fit_the_text_in_order(cx: &mut TestAppContext) {
    let view = filters_view(cx);
    view.update(cx, |v, _| {
        v.filters.loaded_mut().unwrap().rules = vec![filter_row(true, literal("a", "b"))]
    });
    let rule = ReplacementOrigin::ReplacementRuleIndex(0);
    let text = "ok дякую link";
    for (case, marks, kept) in [
        ("in range", vec![applied(14..18, rule)], vec![(14, 18)]),
        ("past the end", vec![applied(14..19, rule)], vec![]),
        ("empty", vec![applied(4..4, rule)], vec![]),
        ("splits a char", vec![applied(4..13, rule)], vec![]),
        (
            "overlaps the previous mark",
            vec![applied(3..13, rule), applied(12..18, rule)],
            vec![(3, 13)],
        ),
        (
            "touches the previous mark",
            vec![applied(3..13, rule), applied(13..18, rule)],
            vec![(3, 13), (13, 18)],
        ),
        (
            "unknown source does not shadow the next mark",
            vec![
                applied(3..13, ReplacementOrigin::ReplacementRuleIndex(7)),
                applied(3..13, rule),
            ],
            vec![(3, 13)],
        ),
    ] {
        let ranges: Vec<(usize, usize)> = view.read_with(cx, |v, _| {
            v.preview_marks(text, &marks, &[ReplacementSource::FilterRule(0)])
                .into_iter()
                .map(|mark| (mark.range.start, mark.range.end))
                .collect()
        });
        assert_eq!(ranges, kept, "{case}");
    }
}

#[gpui::test]
fn mark_tint_colours_by_source_and_by_replacement_row_ordinal(cx: &mut TestAppContext) {
    let view = filters_view(cx);
    view.update(cx, |v, _| {
        v.filters.loaded_mut().unwrap().rules = vec![
            filter_row(false, literal("off", "x")),
            filter_row(true, blocked(&["gtfo"])),
            filter_row(
                true,
                FilterRuleKind::Regex {
                    pattern: r"\d+".into(),
                    replacement: "#".into(),
                },
            ),
            filter_row(true, literal("thanks", "дякую")),
        ];
    });
    let sources = [
        ReplacementSource::UrlMode,
        ReplacementSource::FilterRule(2),
        ReplacementSource::FilterRule(3),
    ];
    for (origin, expected) in [
        (
            ReplacementOrigin::ReplacementRuleIndex(0),
            Some(MarkTint::UrlMode),
        ),
        (
            ReplacementOrigin::BlockedWordIndex(0),
            Some(MarkTint::Blocklist),
        ),
        (
            ReplacementOrigin::ReplacementRuleIndex(1),
            Some(MarkTint::ReplacementRow(1)),
        ),
        (
            ReplacementOrigin::ReplacementRuleIndex(2),
            Some(MarkTint::ReplacementRow(2)),
        ),
        (ReplacementOrigin::ReplacementRuleIndex(3), None),
    ] {
        assert_eq!(
            view.read_with(cx, |v, _| v.mark_tint(origin, &sources)),
            expected,
            "{origin:?}"
        );
    }
}

#[gpui::test]
fn spoken_marks_land_on_the_replacements_inside_the_quoted_output(cx: &mut TestAppContext) {
    crate::i18n::install_language(forge_storage::Language::En);
    let view = filters_view(cx);
    view.update(cx, |v, cx| {
        let filters = v.filters.loaded_mut().unwrap();
        filters.rules = vec![
            filter_row(true, blocked(&["gtfo"])),
            filter_row(true, literal("thanks", "дякую")),
        ];
        filters.settings.url_mode = forge_storage::UrlMode::Replace;
        v.preview_input.update(cx, |input, cx| {
            input.set_content("GG GTFO!!! check https://x.y/z thanks", cx)
        });
        v.refresh_preview(cx);
    });
    let (quoted, marked) = view.read_with(cx, |v, _| {
        let preview = v.cached_preview.as_ref().unwrap();
        let PipelineResult::Speak(spoken) = &preview.result else {
            return (String::new(), Vec::new());
        };
        let quoted = format!("{SPOKEN_QUOTE}{spoken}{SPOKEN_QUOTE}");
        let marked: Vec<(String, MarkTint)> = v
            .spoken_marks(preview, spoken)
            .into_iter()
            .map(|mark| (quoted[mark.range].to_owned(), mark.tint))
            .collect();
        (quoted, marked)
    });
    assert_eq!(
        marked,
        vec![
            ("[beep]".to_owned(), MarkTint::Blocklist),
            ("link".to_owned(), MarkTint::UrlMode),
            ("дякую".to_owned(), MarkTint::ReplacementRow(0)),
        ],
        "{quoted}"
    );
}

#[gpui::test]
fn spoken_marks_are_empty_for_text_other_than_the_final_stage_output(cx: &mut TestAppContext) {
    let view = filters_view(cx);
    view.update(cx, |v, cx| {
        v.filters.loaded_mut().unwrap().rules = vec![filter_row(true, literal("thanks", "дякую"))];
        v.preview_input
            .update(cx, |input, cx| input.set_content("thanks", cx));
        v.refresh_preview(cx);
    });
    let marks = view.read_with(cx, |v, _| {
        let preview = v.cached_preview.as_ref().unwrap();
        v.spoken_marks(preview, "дякую!")
    });
    assert!(marks.is_empty(), "{marks:?}");
}

#[gpui::test]
fn emote_stripping_toggles_persist_and_reach_the_live_pipeline_config(cx: &mut TestAppContext) {
    type Live = fn(&PipelineConfig) -> bool;
    type Saved = fn(&TtsPipelineSettings) -> bool;
    let cases: [(OutputOpt, Saved, Live); 2] = [
        (
            OutputOpt::StripTwitchEmotes,
            |s| s.strip_twitch_emotes,
            |c| c.output.emote_treatment == EmoteTreatment::StripTwitch,
        ),
        (
            OutputOpt::StripRewardEmotes,
            |s| s.strip_reward_emotes,
            |c| c.strip_reward_emotes,
        ),
    ];
    install_presentation(cx);
    let rt = runtime();
    for (opt, saved_flag, live_flag) in cases {
        let repo = RecordingFilters::storing(Vec::new());
        let handle = initial_config();
        let view = cx.update(|cx| {
            cx.new(|cx| {
                TtsFiltersView::new(
                    repo.clone(),
                    Some(handle.clone()),
                    Shared::new(Vec::new()),
                    None,
                    rt.handle().clone(),
                    cx,
                )
            })
        });
        pump(&rt);
        cx.run_until_parked();

        let loaded_on = view.read_with(cx, |v, _| v.output_flag(opt));
        let initial_live = live_flag(&handle.load());
        view.update(cx, |v, cx| v.toggle_output(opt, cx));
        pump(&rt);
        cx.run_until_parked();
        let after_off = (
            view.read_with(cx, |v, _| v.output_flag(opt)),
            repo.saved.lock().unwrap().last().map(saved_flag),
            live_flag(&handle.load()),
        );
        view.update(cx, |v, cx| v.toggle_output(opt, cx));
        pump(&rt);
        cx.run_until_parked();
        let after_on = (
            repo.saved.lock().unwrap().last().map(saved_flag),
            live_flag(&handle.load()),
        );

        assert!(loaded_on && initial_live, "{}", opt.key());
        assert_eq!(after_off, (false, Some(false), false), "{}", opt.key());
        assert_eq!(after_on, (Some(true), true), "{}", opt.key());
    }
}

#[test]
fn emote_to_word_and_strip_twitch_setters_keep_the_pair_exclusive() {
    for (word, strip, to_word) in [
        (true, false, true),
        (false, true, true),
        (false, false, true),
        (true, false, false),
        (false, true, false),
    ] {
        let mut s = TtsPipelineSettings {
            output_emote_to_word: word,
            strip_twitch_emotes: strip,
            ..TtsPipelineSettings::default()
        };
        set_emote_to_word(&mut s, to_word);
        assert_eq!(s.output_emote_to_word, to_word);
        assert!(!s.strip_twitch_emotes, "word={word} strip={strip}");
    }
    for (word, strip, enable) in [
        (true, false, true),
        (false, false, true),
        (false, true, true),
        (true, false, false),
        (false, true, false),
    ] {
        let mut s = TtsPipelineSettings {
            output_emote_to_word: word,
            strip_twitch_emotes: strip,
            ..TtsPipelineSettings::default()
        };
        set_strip_twitch_emotes(&mut s, enable);
        assert_eq!(s.strip_twitch_emotes, enable);
        if enable {
            assert!(!s.output_emote_to_word, "word={word} strip={strip}");
        } else {
            assert_eq!(
                s.output_emote_to_word, word,
                "disabling must not touch the other"
            );
        }
    }
}

#[gpui::test]
fn legacy_row_with_both_emote_flags_shows_strip_twitch_off_and_word_off_clears_both(
    cx: &mut TestAppContext,
) {
    install_presentation(cx);
    let rt = runtime();
    let repo = RecordingFilters::storing(Vec::new());
    let view = view_over(&repo, &rt, cx);
    view.update(cx, |v, _| {
        let settings = &mut v.filters.loaded_mut().unwrap().settings;
        settings.output_emote_to_word = true;
        settings.strip_twitch_emotes = true;
    });
    let shown = view.read_with(cx, |v, _| {
        (
            v.output_flag(OutputOpt::EmoteWord),
            v.output_flag(OutputOpt::StripTwitchEmotes),
        )
    });
    assert_eq!(shown, (true, false));

    view.update(cx, |v, cx| v.toggle_output(OutputOpt::EmoteWord, cx));
    pump(&rt);
    cx.run_until_parked();
    let after = view.read_with(cx, |v, _| {
        (
            v.output_flag(OutputOpt::EmoteWord),
            v.output_flag(OutputOpt::StripTwitchEmotes),
        )
    });
    let saved = repo
        .saved
        .lock()
        .unwrap()
        .last()
        .map(|s| (s.output_emote_to_word, s.strip_twitch_emotes));
    assert_eq!(after, (false, false));
    assert_eq!(saved, Some((false, false)));
}

#[gpui::test]
fn toggling_one_emote_option_on_clears_the_other_in_the_saved_settings(cx: &mut TestAppContext) {
    install_presentation(cx);
    let rt = runtime();
    for (turn_on, other_initially_on) in [
        (OutputOpt::StripTwitchEmotes, OutputOpt::EmoteWord),
        (OutputOpt::EmoteWord, OutputOpt::StripTwitchEmotes),
    ] {
        let repo = RecordingFilters::storing(Vec::new());
        let view = view_over(&repo, &rt, cx);
        view.update(cx, |v, cx| {
            v.set_output(turn_on, false, cx);
            v.set_output(other_initially_on, true, cx);
            v.set_output(turn_on, true, cx);
        });
        pump(&rt);
        cx.run_until_parked();
        let flags = view.read_with(cx, |v, _| {
            (v.output_flag(turn_on), v.output_flag(other_initially_on))
        });
        assert_eq!(flags, (true, false), "{}", turn_on.key());
    }
}

#[gpui::test]
fn emote_output_preset_clears_strip_twitch_emotes(cx: &mut TestAppContext) {
    install_presentation(cx);
    let rt = runtime();
    let repo = RecordingFilters::storing(Vec::new());
    let view = view_over(&repo, &rt, cx);
    view.update(cx, |v, _| {
        let settings = &mut v.filters.loaded_mut().unwrap().settings;
        settings.output_emote_to_word = false;
        settings.strip_twitch_emotes = true;
    });
    let applied = view.update(cx, |v, cx| {
        v.apply_output_preset(OutputPreset::Emote, "", cx)
    });
    pump(&rt);
    cx.run_until_parked();
    let flags = view.read_with(cx, |v, _| {
        (
            v.output_flag(OutputOpt::EmoteWord),
            v.output_flag(OutputOpt::StripTwitchEmotes),
        )
    });
    assert!(applied);
    assert_eq!(flags, (true, false));
}

#[gpui::test]
fn strip_emoji_toggle_starts_off_persists_and_reaches_the_live_pipeline_config(
    cx: &mut TestAppContext,
) {
    install_presentation(cx);
    let rt = runtime();
    let repo = RecordingFilters::storing(Vec::new());
    let handle = initial_config();
    let view = cx.update(|cx| {
        cx.new(|cx| {
            TtsFiltersView::new(
                repo.clone(),
                Some(handle.clone()),
                Shared::new(Vec::new()),
                None,
                rt.handle().clone(),
                cx,
            )
        })
    });
    pump(&rt);
    cx.run_until_parked();
    let initial = (
        view.read_with(cx, |v, _| v.output_flag(OutputOpt::StripEmoji)),
        handle.load().output.strip_emoji,
    );

    view.update(cx, |v, cx| v.toggle_output(OutputOpt::StripEmoji, cx));
    pump(&rt);
    cx.run_until_parked();
    let after_on = (
        view.read_with(cx, |v, _| v.output_flag(OutputOpt::StripEmoji)),
        repo.saved.lock().unwrap().last().map(|s| s.strip_emoji),
        handle.load().output.strip_emoji,
    );
    view.update(cx, |v, cx| v.toggle_output(OutputOpt::StripEmoji, cx));
    pump(&rt);
    cx.run_until_parked();
    let after_off = (
        repo.saved.lock().unwrap().last().map(|s| s.strip_emoji),
        handle.load().output.strip_emoji,
    );

    assert_eq!(initial, (false, false));
    assert_eq!(after_on, (true, Some(true), true));
    assert_eq!(after_off, (Some(false), false));
}

struct SwitchableRow {
    name: &'static str,
    seed: fn(&mut TtsPipelineSettings),
    toggle: Edit,
    remove: Edit,
    state: fn(&TtsPipelineSettings) -> Option<bool>,
}

fn prefix_state(settings: &TtsPipelineSettings) -> Option<bool> {
    configured_skip_prefix(settings).map(|_| settings.skip_prefix_enabled)
}

fn switchable_rows() -> [SwitchableRow; 5] {
    [
        SwitchableRow {
            name: "custom prefix",
            seed: |s| s.skip_prefix = Some("#".to_owned()),
            toggle: |v, cx| v.toggle_skip_prefix(cx),
            remove: |v, cx| v.clear_skip_prefix(cx),
            state: prefix_state,
        },
        SwitchableRow {
            name: "bang prefix",
            seed: |s| s.skip_starts_with_bang = true,
            toggle: |v, cx| v.toggle_skip_prefix(cx),
            remove: |v, cx| v.clear_skip_prefix(cx),
            state: prefix_state,
        },
        SwitchableRow {
            name: "skip regex",
            seed: |s| s.skip_custom_regexes = vec![SkipRegex::enabled("^a$")],
            toggle: |v, cx| v.toggle_skip_regex(0, cx),
            remove: |v, cx| v.remove_skip_regex(0, cx),
            state: |s| s.skip_custom_regexes.first().map(|regex| regex.enabled),
        },
        SwitchableRow {
            name: "max duration",
            seed: |s| s.output_max_duration_secs = Some(30),
            toggle: |v, cx| v.toggle_output_max_duration(cx),
            remove: |v, cx| v.clear_output_max_duration(cx),
            state: |s| {
                s.output_max_duration_secs
                    .map(|_| s.output_max_duration_enabled)
            },
        },
        SwitchableRow {
            name: "language aware voice",
            seed: |s| s.output_language_aware_voice = Some(true),
            toggle: |v, cx| v.toggle_output_lang(cx),
            remove: |v, cx| v.clear_output_lang(cx),
            state: |s| s.output_language_aware_voice,
        },
    ]
}

#[gpui::test]
fn a_row_switch_persists_and_only_its_x_removes_the_row(cx: &mut TestAppContext) {
    install_presentation(cx);
    let rt = runtime();
    for row in switchable_rows() {
        let repo = RecordingFilters::storing(Vec::new());
        let view = view_over(&repo, &rt, cx);
        view.update(cx, |v, _| {
            (row.seed)(&mut v.filters.loaded_mut().unwrap().settings);
        });
        let mut saved = Vec::new();

        for step in [row.toggle, row.toggle, row.toggle, row.remove] {
            view.update(cx, step);
            settle(&rt, cx);
            saved.push(repo.saved.lock().unwrap().last().and_then(row.state));
        }

        assert_eq!(
            saved,
            [Some(false), Some(true), Some(false), None],
            "{}",
            row.name
        );
    }
}

#[gpui::test]
fn removing_or_adding_a_row_over_a_switched_off_one_leaves_the_switch_on(cx: &mut TestAppContext) {
    type Flag = fn(&TtsPipelineSettings) -> bool;
    type Seed = fn(&mut TtsPipelineSettings);
    let prefix_on: Flag = |s| s.skip_prefix_enabled;
    let duration_on: Flag = |s| s.output_max_duration_enabled;
    let seed_prefix: Seed = |s| {
        s.skip_prefix = Some("#".to_owned());
        s.skip_prefix_enabled = false;
    };
    let seed_duration: Seed = |s| {
        s.output_max_duration_secs = Some(30);
        s.output_max_duration_enabled = false;
    };
    let cases: [(&str, Seed, Edit, Flag); 4] = [
        (
            "removed prefix",
            seed_prefix,
            |v, cx| v.clear_skip_prefix(cx),
            prefix_on,
        ),
        (
            "added prefix",
            seed_prefix,
            |v, cx| {
                v.apply_skip_preset(SkipPreset::Prefix, "$".to_owned(), cx);
            },
            prefix_on,
        ),
        (
            "removed duration",
            seed_duration,
            |v, cx| v.clear_output_max_duration(cx),
            duration_on,
        ),
        (
            "added duration",
            seed_duration,
            |v, cx| {
                v.apply_output_preset(OutputPreset::MaxDur, "45", cx);
            },
            duration_on,
        ),
    ];
    install_presentation(cx);
    let rt = runtime();
    for (case, seed, edit, flag) in cases {
        let repo = RecordingFilters::storing(Vec::new());
        let view = view_over(&repo, &rt, cx);
        view.update(cx, |v, _| {
            seed(&mut v.filters.loaded_mut().unwrap().settings)
        });

        view.update(cx, edit);
        settle(&rt, cx);

        assert_eq!(
            repo.saved.lock().unwrap().last().map(flag),
            Some(true),
            "{case}"
        );
    }
}

#[gpui::test]
fn switching_on_an_invalid_regex_is_refused_and_writes_nothing(cx: &mut TestAppContext) {
    install_presentation(cx);
    let rt = runtime();
    let repo = RecordingFilters::storing(Vec::new());
    let view = view_over(&repo, &rt, cx);
    view.update(cx, |v, _| {
        v.filters.loaded_mut().unwrap().settings.skip_custom_regexes = vec![SkipRegex {
            pattern: "(bad".to_owned(),
            enabled: false,
        }];
    });

    view.update(cx, |v, cx| v.toggle_skip_regex(0, cx));
    settle(&rt, cx);

    let (enabled, refused) = view.read_with(cx, |v, _| {
        (
            v.shown().settings.skip_custom_regexes[0].enabled,
            v.save_error.is_some(),
        )
    });
    assert_eq!((enabled, refused, repo.writes()), (false, true, 0));
}

fn stored_with_a_disabled_bad_regex() -> Vec<FilterRule> {
    let mut rules = stored_rules();
    let mut bad = filter_row(
        false,
        FilterRuleKind::Regex {
            pattern: "(bad".to_owned(),
            replacement: String::new(),
        },
    );
    bad.id = "bad".to_owned();
    bad.position = rules.len() as u32;
    rules.push(bad);
    rules
}

#[gpui::test]
fn an_edit_that_breaks_the_filters_is_undone_and_reported(cx: &mut TestAppContext) {
    install_presentation(cx);
    let rt = runtime();
    let repo = RecordingFilters::storing(stored_with_a_disabled_bad_regex());
    let view = view_over(&repo, &rt, cx);
    let bad_row = stored_rules().len();

    view.update(cx, |v, cx| v.toggle_rule(bad_row, cx));
    settle(&rt, cx);

    let (enabled, refused) = view.read_with(cx, |v, _| {
        (v.shown().rules[bad_row].enabled, v.save_error.is_some())
    });
    assert_eq!((enabled, refused, repo.writes()), (false, true, 0));
}

#[gpui::test]
fn a_refused_edit_does_not_block_the_next_valid_one(cx: &mut TestAppContext) {
    install_presentation(cx);
    let rt = runtime();
    let repo = RecordingFilters::storing(stored_with_a_disabled_bad_regex());
    let view = view_over(&repo, &rt, cx);
    let bad_row = stored_rules().len();
    view.update(cx, |v, cx| v.toggle_rule(bad_row, cx));
    settle(&rt, cx);

    view.update(cx, |v, cx| v.toggle_rule(0, cx));
    settle(&rt, cx);

    let (cleared, first_rule_on) = view.read_with(cx, |v, _| {
        (v.save_error.is_none(), v.shown().rules[0].enabled)
    });
    assert_eq!(
        (cleared, first_rule_on, repo.writes() > 0),
        (true, false, true)
    );
}

#[gpui::test]
fn a_replacement_with_a_bad_pattern_is_refused_and_adds_no_row(cx: &mut TestAppContext) {
    install_presentation(cx);
    let rt = runtime();
    let repo = RecordingFilters::storing(stored_rules());
    let view = view_over(&repo, &rt, cx);

    let applied = view.update(cx, |v, cx| {
        v.apply_replacement(
            ReplaceKind::Regex,
            "(bad".to_owned(),
            String::new(),
            false,
            cx,
        )
    });
    settle(&rt, cx);

    let rows = view.read_with(cx, |v, _| v.shown().rules.len());
    assert_eq!(
        (applied, rows, repo.writes()),
        (false, stored_rules().len(), 0)
    );
}

fn blocklist_rule(mode: BlocklistMode) -> FilterRule {
    filter_row(
        true,
        FilterRuleKind::Blocklist {
            words: vec!["gtfo".to_owned()],
            mode,
        },
    )
}

#[gpui::test]
fn picking_the_blocklist_mode_already_held_by_settings_and_rules_writes_nothing(
    cx: &mut TestAppContext,
) {
    install_presentation(cx);
    let rt = runtime();
    let repo = RecordingFilters::storing(vec![blocklist_rule(BlocklistMode::Censor)]);
    let view = view_over(&repo, &rt, cx);

    view.update(cx, |v, cx| v.set_blocklist_mode(BlocklistMode::Censor, cx));
    settle(&rt, cx);

    assert_eq!(repo.writes(), 0);
}

#[gpui::test]
fn picking_the_held_blocklist_mode_heals_a_rule_stored_with_the_other_mode(
    cx: &mut TestAppContext,
) {
    install_presentation(cx);
    let rt = runtime();
    let repo = RecordingFilters::storing(vec![blocklist_rule(BlocklistMode::Suppress)]);
    let view = view_over(&repo, &rt, cx);

    view.update(cx, |v, cx| v.set_blocklist_mode(BlocklistMode::Censor, cx));
    settle(&rt, cx);

    let stored = repo.stored_rules.lock().unwrap().clone();
    assert!(
        matches!(
            stored.as_slice(),
            [FilterRule {
                kind: FilterRuleKind::Blocklist {
                    mode: BlocklistMode::Censor,
                    ..
                },
                ..
            }]
        ),
        "{stored:?}"
    );
}

const SWITCH_SCAN_WINDOW: f32 = 1400.0;
const SWITCH_COLUMN_END: f32 = 60.0;
const SWITCH_COLUMN_STEP: f32 = 12.0;
const SWITCH_ROW_STEP: f32 = 6.0;

#[gpui::test]
fn a_switched_off_bang_row_stays_on_screen_where_a_click_can_switch_it_back(
    cx: &mut TestAppContext,
) {
    install_presentation(cx);
    let rt = runtime();
    let repo = RecordingFilters::storing(Vec::new());
    let handle = rt.handle().clone();
    let (view, vcx) = cx.add_window_view(|_, cx| {
        TtsFiltersView::new(
            repo.clone(),
            None,
            Shared::new(Vec::new()),
            None,
            handle,
            cx,
        )
    });
    for _ in 0..SETTLE_ROUNDS {
        pump(&rt);
        vcx.run_until_parked();
    }
    vcx.simulate_resize(gpui::size(px(SWITCH_SCAN_WINDOW), px(SWITCH_SCAN_WINDOW)));
    view.update(vcx, |v, cx| {
        let settings = &mut v.filters.loaded_mut().unwrap().settings;
        settings.skip_starts_with_bang = true;
        settings.skip_prefix_enabled = false;
        cx.notify();
    });
    vcx.run_until_parked();

    let mut y = 0.0;
    'scan: while y < SWITCH_SCAN_WINDOW {
        let mut x = 0.0;
        while x < SWITCH_COLUMN_END {
            vcx.simulate_click(gpui::point(px(x), px(y)), gpui::Modifiers::none());
            let switched = view.read_with(vcx, |v, _| {
                let settings = &v.shown().settings;
                settings.skip_prefix_enabled || !settings.skip_starts_with_bang
            });
            if switched {
                break 'scan;
            }
            x += SWITCH_COLUMN_STEP;
        }
        y += SWITCH_ROW_STEP;
    }

    let bang = view.read_with(vcx, |v, _| {
        let settings = &v.shown().settings;
        (settings.skip_prefix_enabled, settings.skip_starts_with_bang)
    });
    assert_eq!(bang, (true, true));
}

#[gpui::test]
fn added_text_replacement_keeps_the_modal_inside_words_choice(cx: &mut TestAppContext) {
    for (toggles, expected) in [(0, Some(false)), (1, Some(true)), (2, Some(false))] {
        let view = filters_view(cx);
        view.update(cx, |v, cx| v.open_add_modal(ModalStage::Replace, cx));
        let modal = view.read_with(cx, |v, _| v.add_modal.clone().unwrap());
        modal.update(cx, |m, cx| {
            m.replace_from
                .update(cx, |input, cx| input.set_content("gg", cx));
            for _ in 0..toggles {
                m.toggle_replace_inside_words(cx);
            }
            m.submit(cx);
        });
        cx.run_until_parked();
        let added = view.read_with(cx, |v, _| match v.shown().rules.last().map(|r| &r.kind) {
            Some(FilterRuleKind::Literal {
                match_inside_words, ..
            }) => Some(*match_inside_words),
            _ => None,
        });
        assert_eq!(added, expected, "{toggles} toggles");
    }
}

#[test]
fn literal_badge_names_the_inside_words_scope_only_when_it_is_on() {
    for (match_inside_words, expected) in [
        (true, tr!("tts_filters_badge_text_inside_words")),
        (false, tr!("tts_filters_badge_text")),
    ] {
        let rule = filter_row(
            true,
            FilterRuleKind::Literal {
                pattern: "gg".into(),
                replacement: "X".into(),
                match_inside_words,
            },
        );
        assert_eq!(replacement_meta(&rule), expected, "{match_inside_words}");
    }
}

#[test]
fn parse_max_duration_accepts_whole_seconds_inside_the_inclusive_range() {
    for (raw, expected) in [("1", 1u32), ("600", 600), ("30", 30), ("  45  ", 45)] {
        assert_eq!(parse_max_duration(raw), Some(expected), "input {raw:?}");
    }
}

#[test]
fn parse_max_duration_rejects_out_of_range_and_non_integer_input() {
    for raw in [
        "0", "601", "", "   ", "abc", "-5", "3.5", "30s", "1e2", "٣٠",
    ] {
        assert_eq!(
            parse_max_duration(raw),
            None,
            "input {raw:?} must not produce a cap"
        );
    }
}
