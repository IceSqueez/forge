/// Integration tests for `forge_speak_queue::filters`.
///
/// Each test exercises observable behaviour - either the `PipelineResult` produced
/// by `forge_tts_pipeline::process(text, &config)` or the `FilterMappingError`
/// variant/fields returned by `build_config_strict`. No tautological struct-field
/// assertions; no derive/literal re-checks.
#[allow(clippy::unwrap_used, clippy::panic)]
mod filters {
    use forge_speak_queue::{PipelineConfigHandle, build_config_lenient, build_config_strict};
    use forge_storage::{
        BlocklistMode as StorageBlocklistMode, FilterRule, FilterRuleKind, TtsPipelineSettings,
        UrlMode as StorageUrlMode,
    };
    use forge_tts_pipeline::{PipelineContext, PipelineResult, SkipReason, process};

    // -------------------------------------------------------------------------
    // Helpers
    // -------------------------------------------------------------------------

    fn ctx() -> PipelineContext<'static> {
        PipelineContext {
            viewer_name: "viewer",
            recent_messages: &[],
        }
    }

    fn default_settings() -> TtsPipelineSettings {
        TtsPipelineSettings::default()
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

    // -------------------------------------------------------------------------
    // Literal mapping
    // -------------------------------------------------------------------------

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
    fn empty_pattern_literal_is_noop() {
        // An empty literal pattern must never panic and must leave text unchanged.
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
        // disabled rule → text is unchanged
        assert_eq!(result, PipelineResult::Speak("lol".into()));
    }

    // -------------------------------------------------------------------------
    // Regex mapping
    // -------------------------------------------------------------------------

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
        // Rule 1: replace "hello" → "hi"
        // Rule 2: replace "hi" → "hey"
        // If order is preserved the final output is "hey world".
        // If reversed, Rule 2 fires on original "hello" which doesn't match "hi"
        // and we'd get "hi world" instead.
        let rules = [
            FilterRule {
                id: "r1".into(),
                name: "step1".into(),
                enabled: true,
                position: 0,
                kind: FilterRuleKind::Literal {
                    pattern: "hello".into(),
                    replacement: "hi".into(),
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
                },
            },
        ];
        let config = build_config_strict(&rules, &default_settings()).unwrap();
        let result = process("hello world", &config, &ctx());
        assert_eq!(result, PipelineResult::Speak("hey world".into()));
    }

    // -------------------------------------------------------------------------
    // Invalid regex - strict posture
    // -------------------------------------------------------------------------

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
        // The error must surface the source pattern string, not an internal
        // compiled-regex representation that could expose implementation details.
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
        // The display must identify rule 0, name, and pattern.
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

    // -------------------------------------------------------------------------
    // Invalid regex - lenient posture
    // -------------------------------------------------------------------------

    #[test]
    fn lenient_skips_bad_regex_and_still_applies_surrounding_valid_rules() {
        // Three rules: valid → invalid → valid.
        // Lenient posture must drop the middle rule and apply rule 1 + rule 3.
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
        // Both valid rules applied; broken rule skipped without error.
        assert_eq!(result, PipelineResult::Speak("hi earth".into()));
    }

    // -------------------------------------------------------------------------
    // Blocklist mapping
    // -------------------------------------------------------------------------

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
        // Two blocklist rules: first is Censor, second is Suppress.
        // "Last mode wins" rule - final result must be a Skip, not a censor.
        let rules = [
            blocklist_rule("bl1", &["word1"], StorageBlocklistMode::Censor),
            blocklist_rule("bl2", &["word2"], StorageBlocklistMode::Suppress),
        ];
        let config = build_config_strict(&rules, &default_settings()).unwrap();
        // word2 is in the blocklist; mode is Suppress (last rule wins)
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

    // -------------------------------------------------------------------------
    // UrlMode mapping
    // -------------------------------------------------------------------------

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
    fn url_mode_replace_substitutes_label_in_output() {
        let mut settings = default_settings();
        settings.url_mode = StorageUrlMode::Replace;
        let config = build_config_strict(&[], &settings).unwrap();
        let result = process("check https://example.com now", &config, &ctx());
        // The substitute label is "link" (hard-coded in the mapper).
        assert_eq!(result, PipelineResult::Speak("check link now".into()));
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

    // -------------------------------------------------------------------------
    // max_length mapping
    //
    // `max_length` no longer drives truncation (the `LengthCapper` stage is
    // retired). The migration maps it onto the new skip-based `longer_than`
    // condition: `None` ("unlimited") disables the condition entirely; `Some(n)`
    // enables it at threshold `n`, so a message longer than `n` is now Skipped
    // rather than truncated with an ellipsis.
    // -------------------------------------------------------------------------

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
                reason: SkipReason::MatchedSkipRule("message exceeds max length")
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
        // Exactly at the limit - not "longer than", so it passes through unchanged.
        assert_eq!(result, PipelineResult::Speak("hello".into()));
    }

    // -------------------------------------------------------------------------
    // PipelineConfigHandle - hot-reload semantics
    // -------------------------------------------------------------------------

    #[test]
    fn handle_load_returns_seeded_config() {
        let mut settings = default_settings();
        settings.max_length = Some(7);
        let config = build_config_strict(&[], &settings).unwrap();
        let handle = PipelineConfigHandle::new(config);
        let loaded = handle.load();
        // Use the loaded config to drive process() - observable via the skip.
        let result = process("hello world!", &loaded, &ctx());
        assert_eq!(
            result,
            PipelineResult::Skip {
                reason: SkipReason::MatchedSkipRule("message exceeds max length")
            },
            "loaded config must carry the seeded max_length=7"
        );
    }

    #[test]
    fn handle_swap_replaces_active_config_observable_via_process() {
        // Seed with max_length=5 (skips "hello world!") then swap to max_length=500
        // (passes through). After swap, load() must return the new config.
        let mut settings_v1 = default_settings();
        settings_v1.max_length = Some(5);
        let config_v1 = build_config_strict(&[], &settings_v1).unwrap();
        let handle = PipelineConfigHandle::new(config_v1);

        // v1: skips
        let loaded_v1 = handle.load();
        assert!(
            matches!(
                process("hello world!", &loaded_v1, &ctx()),
                PipelineResult::Skip { .. }
            ),
            "pre-swap config (max=5) must skip 'hello world!'"
        );

        // Swap to v2 (no skip for short text)
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
        // Clone shares the same inner Arc<RwLock<…>>. A swap on the original
        // must be visible through the clone.
        let mut settings_a = default_settings();
        settings_a.max_length = Some(3);
        let config_a = build_config_strict(&[], &settings_a).unwrap();
        let handle_original = PipelineConfigHandle::new(config_a);
        let handle_clone = handle_original.clone();

        // Both see max_length=3 initially
        let pre = handle_clone.load();
        assert!(
            matches!(process("hello", &pre, &ctx()), PipelineResult::Skip { .. }),
            "clone must see the original seeded config (max=3, skips 'hello')"
        );

        // Swap through the original
        let mut settings_b = default_settings();
        settings_b.max_length = Some(500);
        let config_b = build_config_strict(&[], &settings_b).unwrap();
        handle_original.swap(config_b);

        // Clone must now see the swapped config
        let post = handle_clone.load();
        let result_post = process("hello", &post, &ctx());
        assert_eq!(
            result_post,
            PipelineResult::Speak("hello".into()),
            "clone must observe swap made through original handle (shared Arc)"
        );
    }
}
