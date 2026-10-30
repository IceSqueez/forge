mod filters {
    use forge_speak_queue::{
        EmoteTokenSet, EmoteTreatment, PipelineConfigHandle, ReplacementSource, build_config,
        configured_skip_prefix, effective_skip_prefix, emote_treatment, repeat_window,
        store_emote_treatment,
    };
    use forge_storage::{
        BlocklistMode as StorageBlocklistMode, FilterRule, FilterRuleKind, SkipRegex,
        TtsPipelineSettings, UrlMode as StorageUrlMode,
    };
    use forge_tts_pipeline::{
        PipelineConfig, PipelineContext, PipelineResult, SkipReason, SkipRule, process,
    };

    fn ctx() -> PipelineContext<'static> {
        PipelineContext {
            viewer_name: "viewer",
            recent_messages: &[],
            speech_language: None,
        }
    }

    fn default_settings() -> TtsPipelineSettings {
        TtsPipelineSettings::default()
    }

    fn build_config_strict(
        rules: &[FilterRule],
        settings: &TtsPipelineSettings,
    ) -> Result<PipelineConfig, forge_speak_queue::FilterMappingError> {
        build_config(rules, settings).require_valid()
    }

    fn build_config_lenient(
        rules: &[FilterRule],
        settings: &TtsPipelineSettings,
    ) -> PipelineConfig {
        build_config(rules, settings).dropping_invalid().0
    }

    fn literal_rule(name: &str, pattern: &str, replacement: &str, enabled: bool) -> FilterRule {
        FilterRule {
            id: format!("rule-{name}"),
            name: name.to_owned(),
            enabled,
            position: 0,
            kind: FilterRuleKind::Literal {
                pattern: pattern.to_owned(),
                replacement: replacement.to_owned(),
                match_inside_words: false,
            },
        }
    }

    fn regex_rule(name: &str, pattern: &str, replacement: &str) -> FilterRule {
        FilterRule {
            id: format!("rule-{name}"),
            name: name.to_owned(),
            enabled: true,
            position: 0,
            kind: FilterRuleKind::Regex {
                pattern: pattern.to_owned(),
                replacement: replacement.to_owned(),
            },
        }
    }

    fn blocklist_rule(name: &str, words: &[&str], mode: StorageBlocklistMode) -> FilterRule {
        FilterRule {
            id: format!("rule-{name}"),
            name: name.to_owned(),
            enabled: true,
            position: 0,
            kind: FilterRuleKind::Blocklist {
                words: words.iter().map(|s| s.to_string()).collect(),
                mode,
            },
        }
    }

    #[test]
    fn enabled_literal_rule_rewrites_matching_text() {
        let rules = [literal_rule("lol", "lol", "(laugh)", true)];
        let config = build_config_strict(&rules, &default_settings()).unwrap();
        let result = process("LOL that was funny lol!", &config, &ctx());
        assert_eq!(
            result,
            PipelineResult::Speak("(laugh) that was funny (laugh)!".into())
        );
    }

    #[test]
    fn literal_inside_words_flag_picks_substring_or_whole_word_matching() {
        for (match_inside_words, expected) in [(true, "POXERS X!"), (false, "POGGERS X!")] {
            let rules = [FilterRule {
                kind: FilterRuleKind::Literal {
                    pattern: "gg".into(),
                    replacement: "X".into(),
                    match_inside_words,
                },
                ..literal_rule("gg", "", "", true)
            }];
            for (builder, config) in [
                (
                    "strict",
                    build_config_strict(&rules, &default_settings()).unwrap(),
                ),
                ("lenient", build_config_lenient(&rules, &default_settings())),
            ] {
                assert_eq!(
                    process("POGGERS GG!", &config, &ctx()),
                    PipelineResult::Speak(expected.into()),
                    "{builder}, match_inside_words = {match_inside_words}"
                );
            }
        }
    }

    #[test]
    fn empty_pattern_literal_is_noop() {
        let rules = [literal_rule("empty", "", "anything", true)];
        let config = build_config_strict(&rules, &default_settings()).unwrap();
        let result = process("hello world", &config, &ctx());
        assert_eq!(result, PipelineResult::Speak("hello world".into()));
    }

    #[test]
    fn disabled_literal_rule_does_not_apply() {
        let rules = [literal_rule("lol", "lol", "(laugh)", false)];
        let config = build_config_strict(&rules, &default_settings()).unwrap();
        let result = process("lol", &config, &ctx());
        assert_eq!(result, PipelineResult::Speak("lol".into()));
    }

    #[test]
    fn enabled_regex_rule_rewrites_via_process() {
        let rules = [regex_rule("digits", r"\d+", "#")];
        let config = build_config_strict(&rules, &default_settings()).unwrap();
        let result = process("I have 42 cats and 7 dogs", &config, &ctx());
        assert_eq!(
            result,
            PipelineResult::Speak("I have # cats and # dogs".into())
        );
    }

    #[test]
    fn rule_order_preserved_second_rule_operates_on_first_rule_output() {
        let rules = [
            FilterRule {
                id: "r1".into(),
                name: "step1".into(),
                enabled: true,
                position: 0,
                kind: FilterRuleKind::Literal {
                    pattern: "hello".into(),
                    replacement: "hi".into(),
                    match_inside_words: false,
                },
            },
            FilterRule {
                id: "r2".into(),
                name: "step2".into(),
                enabled: true,
                position: 1,
                kind: FilterRuleKind::Literal {
                    pattern: "hi".into(),
                    replacement: "hey".into(),
                    match_inside_words: false,
                },
            },
        ];
        let config = build_config_strict(&rules, &default_settings()).unwrap();
        let result = process("hello world", &config, &ctx());
        assert_eq!(result, PipelineResult::Speak("hey world".into()));
    }

    #[test]
    fn strict_returns_invalid_regex_error_for_bad_pattern() {
        let rules = [FilterRule {
            id: "bad".into(),
            name: "bad-rule".into(),
            enabled: true,
            position: 0,
            kind: FilterRuleKind::Regex {
                pattern: "((unclosed".into(),
                replacement: "x".into(),
            },
        }];
        let err = build_config_strict(&rules, &default_settings()).unwrap_err();
        match err {
            forge_speak_queue::FilterMappingError::InvalidRegex {
                index,
                name,
                pattern,
                ..
            } => {
                assert_eq!(index, 0, "error must carry the offending rule index");
                assert_eq!(name, "bad-rule", "error must carry the rule name");
                assert_eq!(
                    pattern, "((unclosed",
                    "error must carry the offending pattern verbatim"
                );
            }
            other => panic!("expected InvalidRegex, got {other:?}"),
        }
    }

    #[test]
    fn strict_error_display_does_not_contain_compiled_regex_object() {
        let rules = [FilterRule {
            id: "bad2".into(),
            name: "my-rule".into(),
            enabled: true,
            position: 0,
            kind: FilterRuleKind::Regex {
                pattern: "[invalid".into(),
                replacement: "y".into(),
            },
        }];
        let err = build_config_strict(&rules, &default_settings()).unwrap_err();
        let display = err.to_string();
        assert!(display.contains("0"), "index 0 missing from error display");
        assert!(
            display.contains("my-rule"),
            "rule name missing from error display"
        );
        assert!(
            display.contains("[invalid"),
            "pattern missing from error display"
        );
    }

    #[test]
    fn lenient_skips_bad_regex_and_still_applies_surrounding_valid_rules() {
        let rules = [
            literal_rule("first", "hello", "hi", true),
            FilterRule {
                id: "bad".into(),
                name: "broken".into(),
                enabled: true,
                position: 1,
                kind: FilterRuleKind::Regex {
                    pattern: "((bad".into(),
                    replacement: "x".into(),
                },
            },
            literal_rule("third", "world", "earth", true),
        ];
        let config = build_config_lenient(&rules, &default_settings());
        let result = process("hello world", &config, &ctx());
        assert_eq!(result, PipelineResult::Speak("hi earth".into()));
    }

    #[test]
    fn blocklist_censor_mode_replaces_blocked_word_in_output() {
        let rules = [blocklist_rule(
            "bad-words",
            &["slur"],
            StorageBlocklistMode::Censor,
        )];
        let config = build_config_strict(&rules, &default_settings()).unwrap();
        let result = process("don't say slur here", &config, &ctx());
        assert_eq!(
            result,
            PipelineResult::Speak("don't say [beep] here".into())
        );
    }

    #[test]
    fn blocklist_suppress_mode_skips_entire_message() {
        let rules = [blocklist_rule(
            "bad-words",
            &["slur"],
            StorageBlocklistMode::Suppress,
        )];
        let config = build_config_strict(&rules, &default_settings()).unwrap();
        let result = process("this contains slur inside", &config, &ctx());
        assert!(
            matches!(
                result,
                PipelineResult::Skip {
                    reason: SkipReason::BlockedByWordFilter
                }
            ),
            "expected skip for blocked word in suppress mode, got {result:?}"
        );
    }

    #[test]
    fn multiple_blocklist_rules_last_mode_wins() {
        let rules = [
            blocklist_rule("bl1", &["word1"], StorageBlocklistMode::Censor),
            blocklist_rule("bl2", &["word2"], StorageBlocklistMode::Suppress),
        ];
        let config = build_config_strict(&rules, &default_settings()).unwrap();
        let result = process("message with word2", &config, &ctx());
        assert!(
            matches!(
                result,
                PipelineResult::Skip {
                    reason: SkipReason::BlockedByWordFilter
                }
            ),
            "last blocklist rule's mode (Suppress) must override earlier Censor"
        );
    }

    #[test]
    fn url_mode_speak_passes_url_through() {
        let mut settings = default_settings();
        settings.url_mode = StorageUrlMode::Speak;
        let config = build_config_strict(&[], &settings).unwrap();
        let result = process("check https://example.com", &config, &ctx());
        assert_eq!(
            result,
            PipelineResult::Speak("check https://example.com".into())
        );
    }

    #[test]
    fn url_mode_replace_substitutes_the_link_word_of_the_voice_language() {
        let mut settings = default_settings();
        settings.url_mode = StorageUrlMode::Replace;
        let config = build_config(&[], &settings).require_valid().unwrap();
        let spoken = |locale: &str| {
            let context = PipelineContext {
                speech_language: forge_speech_types::LanguageCode::from_locale(locale),
                ..ctx()
            };
            process("check https://example.com now", &config, &context)
        };
        assert_eq!(
            (spoken("en-US"), spoken("uk-UA")),
            (
                PipelineResult::Speak("check link now".into()),
                PipelineResult::Speak("check посилання now".into()),
            )
        );
    }

    #[test]
    fn url_mode_suppress_skips_message_containing_url() {
        let mut settings = default_settings();
        settings.url_mode = StorageUrlMode::Suppress;
        let config = build_config_strict(&[], &settings).unwrap();
        let result = process("visit http://spam.biz for prizes", &config, &ctx());
        assert!(
            matches!(result, PipelineResult::Skip { .. }),
            "expected skip for message with URL in Suppress mode"
        );
    }

    #[test]
    fn url_mode_suppress_passes_message_without_url() {
        let mut settings = default_settings();
        settings.url_mode = StorageUrlMode::Suppress;
        let config = build_config_strict(&[], &settings).unwrap();
        let result = process("no url here, safe message", &config, &ctx());
        assert!(
            matches!(result, PipelineResult::Speak(_)),
            "URL-free message must not be skipped under Suppress mode"
        );
    }

    #[test]
    fn max_length_none_disables_the_longer_than_skip_condition() {
        let mut settings = default_settings();
        settings.max_length = None;
        let config = build_config_strict(&[], &settings).unwrap();
        let long: String = "a".repeat(501);
        let result = process(&long, &config, &ctx());
        assert_eq!(
            result,
            PipelineResult::Speak(long),
            "max_length=None must disable longer_than, not fall back to an implicit cap"
        );
    }

    #[test]
    fn max_length_some_n_skips_messages_longer_than_n() {
        let mut settings = default_settings();
        settings.max_length = Some(10);
        let config = build_config_strict(&[], &settings).unwrap();
        let result = process("hello world this is too long", &config, &ctx());
        assert_eq!(
            result,
            PipelineResult::Skip {
                reason: SkipReason::MatchedSkipRule(SkipRule::ExceedsMaxLength)
            },
            "text longer than the migrated max_length must now be Skipped, not truncated"
        );
    }

    #[test]
    fn max_length_exactly_at_boundary_is_not_skipped() {
        let mut settings = default_settings();
        settings.max_length = Some(5);
        let config = build_config_strict(&[], &settings).unwrap();
        let result = process("hello", &config, &ctx());
        assert_eq!(result, PipelineResult::Speak("hello".into()));
    }

    #[test]
    fn hot_reload_of_strip_emoji_changes_what_the_live_config_speaks() {
        let off = TtsPipelineSettings {
            strip_emoji: false,
            ..default_settings()
        };
        let on = TtsPipelineSettings {
            strip_emoji: true,
            ..default_settings()
        };
        let handle = PipelineConfigHandle::new(build_config_lenient(&[], &off));
        let spoken =
            |handle: &PipelineConfigHandle| process("nice\u{1F389}", &handle.load(), &ctx());

        let before = spoken(&handle);
        handle.swap(build_config_lenient(&[], &on));
        let after = spoken(&handle);

        assert_eq!(
            (before, after),
            (
                PipelineResult::Speak("nice\u{1F389}".into()),
                PipelineResult::Speak("nice".into())
            )
        );
    }

    #[test]
    fn handle_load_returns_seeded_config() {
        let mut settings = default_settings();
        settings.max_length = Some(7);
        let config = build_config_strict(&[], &settings).unwrap();
        let handle = PipelineConfigHandle::new(config);
        let loaded = handle.load();
        let result = process("hello world!", &loaded, &ctx());
        assert_eq!(
            result,
            PipelineResult::Skip {
                reason: SkipReason::MatchedSkipRule(SkipRule::ExceedsMaxLength)
            },
            "loaded config must carry the seeded max_length=7"
        );
    }

    #[test]
    fn handle_swap_replaces_active_config_observable_via_process() {
        let mut settings_v1 = default_settings();
        settings_v1.max_length = Some(5);
        let config_v1 = build_config_strict(&[], &settings_v1).unwrap();
        let handle = PipelineConfigHandle::new(config_v1);

        let loaded_v1 = handle.load();
        assert!(
            matches!(
                process("hello world!", &loaded_v1, &ctx()),
                PipelineResult::Skip { .. }
            ),
            "pre-swap config (max=5) must skip 'hello world!'"
        );

        let mut settings_v2 = default_settings();
        settings_v2.max_length = Some(500);
        let config_v2 = build_config_strict(&[], &settings_v2).unwrap();
        handle.swap(config_v2);

        let loaded_v2 = handle.load();
        let result_v2 = process("hello world!", &loaded_v2, &ctx());
        assert_eq!(
            result_v2,
            PipelineResult::Speak("hello world!".into()),
            "after swap, load() must return the new config (no skip)"
        );
    }

    #[test]
    fn cloned_handle_observes_swap_made_through_original() {
        let mut settings_a = default_settings();
        settings_a.max_length = Some(3);
        let config_a = build_config_strict(&[], &settings_a).unwrap();
        let handle_original = PipelineConfigHandle::new(config_a);
        let handle_clone = handle_original.clone();

        let pre = handle_clone.load();
        assert!(
            matches!(process("hello", &pre, &ctx()), PipelineResult::Skip { .. }),
            "clone must see the original seeded config (max=3, skips 'hello')"
        );

        let mut settings_b = default_settings();
        settings_b.max_length = Some(500);
        let config_b = build_config_strict(&[], &settings_b).unwrap();
        handle_original.swap(config_b);

        let post = handle_clone.load();
        let result_post = process("hello", &post, &ctx());
        assert_eq!(
            result_post,
            PipelineResult::Speak("hello".into()),
            "clone must observe swap made through original handle (shared Arc)"
        );
    }
    #[test]
    fn lenient_sources_name_the_row_behind_each_live_replacement_rule() {
        let rules = [
            literal_rule("off", "a", "b", false),
            blocklist_rule("words", &["gtfo"], StorageBlocklistMode::Censor),
            regex_rule("broken", "(unclosed", "x"),
            literal_rule("empty", "", "x", true),
            regex_rule("digits", r"\d+", "#"),
            literal_rule("thanks", "thanks", "дякую", true),
        ];
        for (url_mode, expected) in [
            (
                StorageUrlMode::Replace,
                vec![
                    ReplacementSource::UrlMode,
                    ReplacementSource::FilterRule(4),
                    ReplacementSource::FilterRule(5),
                ],
            ),
            (
                StorageUrlMode::Speak,
                vec![
                    ReplacementSource::FilterRule(4),
                    ReplacementSource::FilterRule(5),
                ],
            ),
        ] {
            let mut settings = default_settings();
            settings.url_mode = url_mode;
            let (config, sources) = build_config(&rules, &settings).dropping_invalid();
            assert_eq!(sources, expected, "{url_mode:?}");
            assert_eq!(
                config.replacement_rules.len(),
                sources.len(),
                "{url_mode:?}: one source per replacement rule"
            );
        }
    }

    fn skip_regex_settings(patterns: &[&str]) -> TtsPipelineSettings {
        TtsPipelineSettings {
            skip_custom_regexes: patterns
                .iter()
                .map(|p| forge_storage::SkipRegex::enabled(*p))
                .collect(),
            ..default_settings()
        }
    }

    fn rules_with_bad_regexes_at_one_and_three() -> [FilterRule; 4] {
        [
            literal_rule("first", "hello", "hi", true),
            regex_rule("broken-one", "((one", "x"),
            literal_rule("third", "world", "earth", true),
            regex_rule("broken-three", "[three", "x"),
        ]
    }

    #[test]
    fn strict_build_returns_the_first_invalid_filter_rule_in_rule_order() {
        let rules = rules_with_bad_regexes_at_one_and_three();
        let settings = skip_regex_settings(&["(skip"]);

        let err = build_config_strict(&rules, &settings).unwrap_err();

        assert!(
            matches!(
                &err,
                forge_speak_queue::FilterMappingError::InvalidRegex { index: 1, name, .. }
                    if name == "broken-one"
            ),
            "{err:?}"
        );
    }

    #[test]
    fn strict_build_returns_the_first_invalid_skip_regex_when_every_rule_is_valid() {
        let settings = skip_regex_settings(&["^ok$", "(bad", "[worse"]);

        let err = build_config_strict(&[], &settings).unwrap_err();

        assert!(
            matches!(
                &err,
                forge_speak_queue::FilterMappingError::InvalidSkipRegex { index: 1, pattern, .. }
                    if pattern == "(bad"
            ),
            "{err:?}"
        );
    }

    fn switched_regex(pattern: &str, enabled: bool) -> SkipRegex {
        SkipRegex {
            pattern: pattern.to_owned(),
            enabled,
        }
    }

    type SwitchedSettings = fn(bool) -> TtsPipelineSettings;

    #[test]
    fn a_skip_row_filters_only_while_its_switch_is_on() {
        let rows: [(&str, &str, SwitchedSettings); 3] = [
            ("regex", "skip me", |on| TtsPipelineSettings {
                skip_custom_regexes: vec![switched_regex("^skip me$", on)],
                ..default_settings()
            }),
            ("prefix", "#skip me", |on| TtsPipelineSettings {
                skip_prefix: Some("#".to_owned()),
                skip_prefix_enabled: on,
                ..default_settings()
            }),
            ("bang", "!skip me", |on| TtsPipelineSettings {
                skip_starts_with_bang: true,
                skip_prefix_enabled: on,
                ..default_settings()
            }),
        ];

        for (row, message, settings) in rows {
            let filters = [true, false].map(|on| {
                let config = build_config_lenient(&[], &settings(on));
                matches!(
                    process(message, &config, &ctx()),
                    PipelineResult::Skip { .. }
                )
            });
            assert_eq!(filters, [true, false], "{row}");
        }
    }

    #[test]
    fn a_duration_cap_applies_only_while_its_switch_is_on() {
        let caps = [true, false].map(|on| {
            let settings = TtsPipelineSettings {
                output_max_duration_secs: Some(30),
                output_max_duration_enabled: on,
                ..default_settings()
            };
            build_config_lenient(&[], &settings)
                .output
                .max_duration_secs
        });

        assert_eq!(caps, [Some(30), None]);
    }

    #[test]
    fn a_switched_off_invalid_skip_regex_is_not_rejected() {
        let settings = TtsPipelineSettings {
            skip_custom_regexes: vec![switched_regex("(bad", false)],
            ..default_settings()
        };

        let built = build_config_strict(&[], &settings);

        assert!(built.is_ok(), "{:?}", built.err());
    }

    #[test]
    fn an_invalid_skip_regex_is_reported_by_its_stored_index_past_switched_off_rows() {
        let settings = TtsPipelineSettings {
            skip_custom_regexes: vec![
                switched_regex("(off", false),
                switched_regex("^ok$", true),
                switched_regex("^off$", false),
                switched_regex("(bad", true),
            ],
            ..default_settings()
        };

        let err = build_config_strict(&[], &settings).unwrap_err();

        assert!(
            matches!(
                &err,
                forge_speak_queue::FilterMappingError::InvalidSkipRegex { index: 3, pattern, .. }
                    if pattern == "(bad"
            ),
            "{err:?}"
        );
    }

    #[test]
    fn a_switched_off_prefix_stays_configured_but_takes_no_effect() {
        for (skip_prefix, bang, expected) in [
            (Some("#"), false, "#"),
            (None, true, "!"),
            (Some(""), true, "!"),
            (Some("#"), true, "#"),
        ] {
            let settings = TtsPipelineSettings {
                skip_prefix: skip_prefix.map(str::to_owned),
                skip_starts_with_bang: bang,
                skip_prefix_enabled: false,
                ..default_settings()
            };

            assert_eq!(
                (
                    configured_skip_prefix(&settings).as_deref(),
                    effective_skip_prefix(&settings),
                ),
                (Some(expected), None),
                "{skip_prefix:?} {bang}"
            );
        }
    }

    #[test]
    fn lenient_build_drops_every_invalid_regex_and_keeps_the_valid_ones() {
        let rules = rules_with_bad_regexes_at_one_and_three();
        let settings = skip_regex_settings(&["(bad", "^skip me$"]);

        let config = build_config_lenient(&rules, &settings);

        assert_eq!(
            (
                process("hello world", &config, &ctx()),
                process("skip me", &config, &ctx()),
            ),
            (
                PipelineResult::Speak("hi earth".into()),
                PipelineResult::Skip {
                    reason: SkipReason::MatchedSkipRule(SkipRule::CustomRegex),
                },
            )
        );
    }

    #[test]
    fn lenient_build_reports_each_dropped_regex_once() {
        let rules = rules_with_bad_regexes_at_one_and_three();
        let settings = skip_regex_settings(&["(bad", "^fine$", "[worse"]);
        let reported = warning_patterns_while(|| {
            build_config(&rules, &settings).dropping_invalid();
        });

        assert_eq!(reported, vec!["((one", "[three", "(bad", "[worse"]);
    }

    fn warning_patterns_while(run: impl FnOnce()) -> Vec<String> {
        static INTEREST_FLOOR: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        INTEREST_FLOOR.get_or_init(|| {
            drop(tracing::subscriber::set_global_default(
                PatternCollector::default(),
            ));
        });
        let collector = PatternCollector::default();
        let patterns = std::sync::Arc::clone(&collector.patterns);
        tracing::subscriber::with_default(collector, || {
            tracing::callsite::rebuild_interest_cache();
            run();
        });
        patterns.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    #[derive(Default)]
    struct PatternCollector {
        patterns: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    }

    struct PatternField<'a>(&'a mut Option<String>);

    impl tracing::field::Visit for PatternField<'_> {
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            if field.name() == "pattern" {
                *self.0 = Some(format!("{value:?}"));
            }
        }
    }

    impl tracing::Subscriber for PatternCollector {
        fn register_callsite(
            &self,
            _: &'static tracing::Metadata<'static>,
        ) -> tracing::subscriber::Interest {
            tracing::subscriber::Interest::sometimes()
        }
        fn max_level_hint(&self) -> Option<tracing::level_filters::LevelFilter> {
            Some(tracing::level_filters::LevelFilter::WARN)
        }
        fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
            *metadata.level() == tracing::Level::WARN
        }
        fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
            tracing::span::Id::from_u64(1)
        }
        fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
        fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
        fn event(&self, event: &tracing::Event<'_>) {
            let mut pattern = None;
            event.record(&mut PatternField(&mut pattern));
            if let Some(pattern) = pattern {
                self.patterns
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(pattern);
            }
        }
        fn enter(&self, _: &tracing::span::Id) {}
        fn exit(&self, _: &tracing::span::Id) {}
    }

    #[test]
    fn emote_treatment_reads_to_word_over_strip_over_keep() {
        for (to_word, strip, expected) in [
            (false, false, EmoteTreatment::Keep),
            (false, true, EmoteTreatment::StripTwitch),
            (true, false, EmoteTreatment::ToWord),
            (true, true, EmoteTreatment::ToWord),
        ] {
            let settings = TtsPipelineSettings {
                output_emote_to_word: to_word,
                strip_twitch_emotes: strip,
                ..default_settings()
            };
            assert_eq!(
                emote_treatment(&settings),
                expected,
                "to_word = {to_word}, strip = {strip}"
            );
        }
    }

    #[test]
    fn stored_emote_treatment_reads_back_from_any_prior_flag_state() {
        for prior in [(false, false), (false, true), (true, false), (true, true)] {
            for treatment in [
                EmoteTreatment::Keep,
                EmoteTreatment::StripTwitch,
                EmoteTreatment::ToWord,
            ] {
                let mut settings = TtsPipelineSettings {
                    output_emote_to_word: prior.0,
                    strip_twitch_emotes: prior.1,
                    ..default_settings()
                };
                store_emote_treatment(&mut settings, treatment);
                assert_eq!(
                    emote_treatment(&settings),
                    treatment,
                    "prior flags {prior:?}"
                );
            }
        }
    }

    #[test]
    fn effective_skip_prefix_prefers_a_non_empty_custom_prefix_then_the_bang_toggle() {
        for (prefix, bang, expected) in [
            (None, false, None),
            (None, true, Some("!")),
            (Some(""), false, None),
            (Some(""), true, Some("!")),
            (Some("?"), false, Some("?")),
            (Some("?"), true, Some("?")),
        ] {
            let settings = TtsPipelineSettings {
                skip_prefix: prefix.map(str::to_owned),
                skip_starts_with_bang: bang,
                ..default_settings()
            };
            assert_eq!(
                effective_skip_prefix(&settings).as_deref(),
                expected,
                "prefix = {prefix:?}, bang = {bang}"
            );
        }
    }

    #[test]
    fn repeat_window_raises_zero_to_one_and_keeps_positive_values() {
        for (stored, expected) in [(0, 1), (1, 1), (2, 2), (100, 100)] {
            let settings = TtsPipelineSettings {
                repeat_of_recent_window: stored,
                ..default_settings()
            };
            assert_eq!(
                repeat_window(&settings).get(),
                expected,
                "stored = {stored}"
            );
        }
    }

    #[test]
    fn spoken_text_for_sampled_settings_and_messages_matches_the_pinned_table() {
        const MESSAGES: [&str; 10] = [
            "hello chat",
            "check https://example.com now",
            "Kappa nice :pog: play",
            "party \u{1F389} time",
            "!command arg",
            "?ask thing",
            "repeat me",
            "this message is definitely long",
            "Kappa",
            "wow!!! ok??",
        ];
        type Case = (
            &'static str,
            fn(&mut TtsPipelineSettings),
            [Option<&'static str>; 10],
        );
        let cases: [Case; 19] = [
            (
                "defaults",
                |_| {},
                [
                    Some("hello chat"),
                    Some("check https://example.com now"),
                    Some("nice :pog: play"),
                    Some("party \u{1F389} time"),
                    Some("!command arg"),
                    Some("?ask thing"),
                    Some("repeat me"),
                    Some("this message is definitely long"),
                    None,
                    Some("wow!!! ok??"),
                ],
            ),
            (
                "url replaced by the link word",
                |s| s.url_mode = StorageUrlMode::Replace,
                [
                    Some("hello chat"),
                    Some("check link now"),
                    Some("nice :pog: play"),
                    Some("party \u{1F389} time"),
                    Some("!command arg"),
                    Some("?ask thing"),
                    Some("repeat me"),
                    Some("this message is definitely long"),
                    None,
                    Some("wow!!! ok??"),
                ],
            ),
            (
                "url suppressed",
                |s| s.url_mode = StorageUrlMode::Suppress,
                [
                    Some("hello chat"),
                    None,
                    Some("nice :pog: play"),
                    Some("party \u{1F389} time"),
                    Some("!command arg"),
                    Some("?ask thing"),
                    Some("repeat me"),
                    Some("this message is definitely long"),
                    None,
                    Some("wow!!! ok??"),
                ],
            ),
            (
                "twitch emotes kept",
                |s| s.strip_twitch_emotes = false,
                [
                    Some("hello chat"),
                    Some("check https://example.com now"),
                    Some("Kappa nice :pog: play"),
                    Some("party \u{1F389} time"),
                    Some("!command arg"),
                    Some("?ask thing"),
                    Some("repeat me"),
                    Some("this message is definitely long"),
                    Some("Kappa"),
                    Some("wow!!! ok??"),
                ],
            ),
            (
                "emote to word wins over strip",
                |s| s.output_emote_to_word = true,
                [
                    Some("hello chat"),
                    Some("check https://example.com now"),
                    Some("Kappa nice pog play"),
                    Some("party \u{1F389} time"),
                    Some("!command arg"),
                    Some("?ask thing"),
                    Some("repeat me"),
                    Some("this message is definitely long"),
                    Some("Kappa"),
                    Some("wow!!! ok??"),
                ],
            ),
            (
                "emote to word alone",
                |s| {
                    s.output_emote_to_word = true;
                    s.strip_twitch_emotes = false;
                },
                [
                    Some("hello chat"),
                    Some("check https://example.com now"),
                    Some("Kappa nice pog play"),
                    Some("party \u{1F389} time"),
                    Some("!command arg"),
                    Some("?ask thing"),
                    Some("repeat me"),
                    Some("this message is definitely long"),
                    Some("Kappa"),
                    Some("wow!!! ok??"),
                ],
            ),
            (
                "emoji stripped",
                |s| s.strip_emoji = true,
                [
                    Some("hello chat"),
                    Some("check https://example.com now"),
                    Some("nice :pog: play"),
                    Some("party  time"),
                    Some("!command arg"),
                    Some("?ask thing"),
                    Some("repeat me"),
                    Some("this message is definitely long"),
                    None,
                    Some("wow!!! ok??"),
                ],
            ),
            (
                "max length",
                |s| s.max_length = Some(12),
                [
                    Some("hello chat"),
                    None,
                    None,
                    Some("party \u{1F389} time"),
                    Some("!command arg"),
                    Some("?ask thing"),
                    Some("repeat me"),
                    None,
                    None,
                    Some("wow!!! ok??"),
                ],
            ),
            (
                "legacy longer-than toggle",
                |s| {
                    s.skip_longer_than = true;
                    s.longer_than_max_chars = 12;
                },
                [
                    Some("hello chat"),
                    None,
                    None,
                    Some("party \u{1F389} time"),
                    Some("!command arg"),
                    Some("?ask thing"),
                    Some("repeat me"),
                    None,
                    None,
                    Some("wow!!! ok??"),
                ],
            ),
            (
                "max length wins over the legacy toggle",
                |s| {
                    s.max_length = Some(40);
                    s.skip_longer_than = true;
                    s.longer_than_max_chars = 12;
                },
                [
                    Some("hello chat"),
                    Some("check https://example.com now"),
                    Some("nice :pog: play"),
                    Some("party \u{1F389} time"),
                    Some("!command arg"),
                    Some("?ask thing"),
                    Some("repeat me"),
                    Some("this message is definitely long"),
                    None,
                    Some("wow!!! ok??"),
                ],
            ),
            (
                "bang commands skipped",
                |s| s.skip_starts_with_bang = true,
                [
                    Some("hello chat"),
                    Some("check https://example.com now"),
                    Some("nice :pog: play"),
                    Some("party \u{1F389} time"),
                    None,
                    Some("?ask thing"),
                    Some("repeat me"),
                    Some("this message is definitely long"),
                    None,
                    Some("wow!!! ok??"),
                ],
            ),
            (
                "custom prefix wins over bang",
                |s| {
                    s.skip_prefix = Some("?".into());
                    s.skip_starts_with_bang = true;
                },
                [
                    Some("hello chat"),
                    Some("check https://example.com now"),
                    Some("nice :pog: play"),
                    Some("party \u{1F389} time"),
                    Some("!command arg"),
                    None,
                    Some("repeat me"),
                    Some("this message is definitely long"),
                    None,
                    Some("wow!!! ok??"),
                ],
            ),
            (
                "empty prefix falls back to bang",
                |s| {
                    s.skip_prefix = Some(String::new());
                    s.skip_starts_with_bang = true;
                },
                [
                    Some("hello chat"),
                    Some("check https://example.com now"),
                    Some("nice :pog: play"),
                    Some("party \u{1F389} time"),
                    None,
                    Some("?ask thing"),
                    Some("repeat me"),
                    Some("this message is definitely long"),
                    None,
                    Some("wow!!! ok??"),
                ],
            ),
            (
                "custom prefix without bang",
                |s| s.skip_prefix = Some("?".into()),
                [
                    Some("hello chat"),
                    Some("check https://example.com now"),
                    Some("nice :pog: play"),
                    Some("party \u{1F389} time"),
                    Some("!command arg"),
                    None,
                    Some("repeat me"),
                    Some("this message is definitely long"),
                    None,
                    Some("wow!!! ok??"),
                ],
            ),
            (
                "repeat of recent skipped",
                |s| s.skip_repeat_of_recent = true,
                [
                    Some("hello chat"),
                    Some("check https://example.com now"),
                    Some("nice :pog: play"),
                    Some("party \u{1F389} time"),
                    Some("!command arg"),
                    Some("?ask thing"),
                    None,
                    Some("this message is definitely long"),
                    None,
                    Some("wow!!! ok??"),
                ],
            ),
            (
                "emote-only skipped",
                |s| s.skip_emote_only = true,
                [
                    Some("hello chat"),
                    Some("check https://example.com now"),
                    Some("nice :pog: play"),
                    Some("party \u{1F389} time"),
                    Some("!command arg"),
                    Some("?ask thing"),
                    Some("repeat me"),
                    Some("this message is definitely long"),
                    None,
                    Some("wow!!! ok??"),
                ],
            ),
            (
                "punctuation sanitized",
                |s| s.output_sanitize_punctuation = true,
                [
                    Some("hello chat"),
                    Some("check https:/example.com now"),
                    Some("nice :pog: play"),
                    Some("party \u{1F389} time"),
                    Some("!command arg"),
                    Some("?ask thing"),
                    Some("repeat me"),
                    Some("this message is definitely long"),
                    None,
                    Some("wow! ok?"),
                ],
            ),
            (
                "url skip with replace mode",
                |s| {
                    s.skip_contains_url = true;
                    s.url_mode = StorageUrlMode::Replace;
                },
                [
                    Some("hello chat"),
                    None,
                    Some("nice :pog: play"),
                    Some("party \u{1F389} time"),
                    Some("!command arg"),
                    Some("?ask thing"),
                    Some("repeat me"),
                    Some("this message is definitely long"),
                    None,
                    Some("wow!!! ok??"),
                ],
            ),
            (
                "name first with link and emoji",
                |s| {
                    s.output_read_display_name_first = true;
                    s.url_mode = StorageUrlMode::Replace;
                    s.strip_emoji = true;
                },
                [
                    Some("koval_dev says: hello chat"),
                    Some("koval_dev says: check link now"),
                    Some("koval_dev says: nice :pog: play"),
                    Some("koval_dev says: party  time"),
                    Some("koval_dev says: !command arg"),
                    Some("koval_dev says: ?ask thing"),
                    Some("koval_dev says: repeat me"),
                    Some("koval_dev says: this message is definitely long"),
                    None,
                    Some("koval_dev says: wow!!! ok??"),
                ],
            ),
        ];
        let recent = vec!["repeat me".to_owned()];
        let context = PipelineContext {
            viewer_name: "koval_dev",
            recent_messages: &recent,
            speech_language: None,
        };
        let emotes: EmoteTokenSet = ["Kappa".to_owned()].into_iter().collect();
        for (case, mutate, expected) in cases {
            let mut settings = default_settings();
            mutate(&mut settings);
            let config = build_config_strict(&[], &settings)
                .unwrap()
                .with_message_emotes(&emotes);
            for (message, spoken) in MESSAGES.into_iter().zip(expected) {
                let actual = match process(message, &config, &context) {
                    PipelineResult::Speak(text) => Some(text),
                    PipelineResult::Skip { .. } => None,
                };
                assert_eq!(actual.as_deref(), spoken, "{case}: {message:?}");
            }
        }
    }
}
