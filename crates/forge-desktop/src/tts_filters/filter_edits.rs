use super::add_filter_modal::{AddFilterEvent, AddFilterModal, FilterDraft, ReplaceKind};
use super::filter_options::{
    ModalStage, OutputOpt, OutputPreset, SkipPreset, SkipRule, parse_max_duration,
};
use super::{FilterSet, TtsFiltersView};
use crate::async_bridge::WriteOutcome;
use crate::repo_failure::RepoFailure;
use forge_speak_queue::{
    EmoteTreatment, SpeakCommand, build_config, emote_treatment, repeat_window,
    store_emote_treatment,
};
use forge_storage::{BlocklistMode, FilterRule, FilterRuleKind, SkipRegex, TtsPipelineSettings};
use gpui::prelude::*;
use gpui::{Context, Entity};
use std::sync::Arc;

pub(super) fn set_emote_to_word(settings: &mut TtsPipelineSettings, value: bool) {
    let treatment = if value {
        EmoteTreatment::ToWord
    } else {
        EmoteTreatment::Keep
    };
    store_emote_treatment(settings, treatment);
}

pub(super) fn set_strip_twitch_emotes(settings: &mut TtsPipelineSettings, value: bool) {
    let treatment = match (value, emote_treatment(settings)) {
        (true, _) => EmoteTreatment::StripTwitch,
        (false, EmoteTreatment::StripTwitch) => EmoteTreatment::Keep,
        (false, unchanged) => unchanged,
    };
    store_emote_treatment(settings, treatment);
}

impl TtsFiltersView {
    fn edit(&mut self, cx: &mut Context<Self>, mutate: impl FnOnce(&mut FilterSet)) {
        self.try_apply(cx, mutate);
    }

    fn after_change(&mut self, cx: &mut Context<Self>) {
        self.refresh_preview(cx);
        self.persist(cx);
        cx.notify();
    }

    fn persist(&mut self, cx: &mut Context<Self>) {
        let Some(filters) = self.filters.loaded() else {
            return;
        };
        let config = match build_config(&filters.rules, &filters.settings).require_valid() {
            Ok(config) => config,
            Err(e) => {
                self.save_error = Some(RepoFailure::from(e).text());
                return;
            }
        };
        let FilterSet { rules, settings } = filters.clone();
        self.save_error = None;

        let repo = Arc::clone(&self.repo);
        let pipeline_config = self.pipeline_config.clone();
        let bot_accounts = self.bot_accounts.clone();
        let speak = self.speak.clone();
        let window = repeat_window(&settings);
        self.writer.write(
            async move {
                repo.replace_rules(&rules)
                    .await
                    .map_err(RepoFailure::from)?;
                repo.set_pipeline_settings(&settings)
                    .await
                    .map_err(RepoFailure::from)?;
                bot_accounts.store(settings.bot_accounts);
                if let Some(handle) = pipeline_config {
                    handle.swap(config);
                }
                if let Some(speak) = speak
                    && let Err(e) = speak.send(SpeakCommand::SetRepeatWindow(window)).await
                {
                    tracing::warn!(error = %e, "speak queue did not take the new repeat window");
                }
                Ok::<(), RepoFailure>(())
            },
            Self::on_write_settled,
            cx,
        );
    }

    fn on_write_settled(&mut self, outcome: WriteOutcome, cx: &mut Context<Self>) {
        if let WriteOutcome::Failed(message) = outcome {
            self.save_error = Some(message.text());
            self.reload(cx);
        }
    }

    fn try_apply(&mut self, cx: &mut Context<Self>, mutate: impl FnOnce(&mut FilterSet)) -> bool {
        let Some(filters) = self.filters.loaded_mut() else {
            return false;
        };
        let mut candidate = filters.clone();
        mutate(&mut candidate);
        renumber(&mut candidate.rules);
        match build_config(&candidate.rules, &candidate.settings).require_valid() {
            Ok(_) => {
                *filters = candidate;
                self.after_change(cx);
                true
            }
            Err(e) => {
                self.save_error = Some(RepoFailure::from(e).text());
                cx.notify();
                false
            }
        }
    }

    pub(super) fn skip_flag(&self, rule: SkipRule) -> bool {
        let settings = &self.shown().settings;
        match rule {
            SkipRule::ContainsUrl => settings.skip_contains_url,
            SkipRule::BotAccounts => settings.skip_from_bot_accounts,
            SkipRule::LongerThan => settings.skip_longer_than,
            SkipRule::Repeat => settings.skip_repeat_of_recent,
            SkipRule::EmoteOnly => settings.skip_emote_only,
            SkipRule::MostlyNonLatin => settings.skip_mostly_non_latin,
        }
    }

    pub(super) fn set_skip(&mut self, rule: SkipRule, value: bool, cx: &mut Context<Self>) {
        if self.skip_flag(rule) == value {
            return;
        }
        self.edit(cx, |filters| {
            let settings = &mut filters.settings;
            match rule {
                SkipRule::ContainsUrl => settings.skip_contains_url = value,
                SkipRule::BotAccounts => settings.skip_from_bot_accounts = value,
                SkipRule::LongerThan => settings.skip_longer_than = value,
                SkipRule::Repeat => settings.skip_repeat_of_recent = value,
                SkipRule::EmoteOnly => settings.skip_emote_only = value,
                SkipRule::MostlyNonLatin => settings.skip_mostly_non_latin = value,
            }
        });
    }

    pub(super) fn toggle_skip(&mut self, rule: SkipRule, cx: &mut Context<Self>) {
        let value = !self.skip_flag(rule);
        self.set_skip(rule, value, cx);
    }

    pub(super) fn clear_skip_prefix(&mut self, cx: &mut Context<Self>) {
        let settings = &self.shown().settings;
        if settings.skip_prefix.is_none() && !settings.skip_starts_with_bang {
            return;
        }
        self.edit(cx, |filters| {
            filters.settings.skip_prefix = None;
            filters.settings.skip_prefix_enabled = true;
            filters.settings.skip_starts_with_bang = false;
        });
    }

    pub(super) fn toggle_skip_prefix(&mut self, cx: &mut Context<Self>) {
        self.edit(cx, |filters| {
            let settings = &mut filters.settings;
            settings.skip_prefix_enabled = !settings.skip_prefix_enabled;
        });
    }

    pub(super) fn toggle_skip_regex(&mut self, index: usize, cx: &mut Context<Self>) {
        self.try_apply(cx, |filters| {
            if let Some(regex) = filters.settings.skip_custom_regexes.get_mut(index) {
                regex.enabled = !regex.enabled;
            }
        });
    }

    pub(super) fn remove_skip_regex(&mut self, index: usize, cx: &mut Context<Self>) {
        self.edit(cx, |filters| {
            if index < filters.settings.skip_custom_regexes.len() {
                filters.settings.skip_custom_regexes.remove(index);
            }
        });
    }

    pub(super) fn output_flag(&self, opt: OutputOpt) -> bool {
        let settings = &self.shown().settings;
        match opt {
            OutputOpt::ReadName => settings.output_read_display_name_first,
            OutputOpt::EmoteWord => emote_treatment(settings) == EmoteTreatment::ToWord,
            OutputOpt::Sanitize => settings.output_sanitize_punctuation,
            OutputOpt::StripTwitchEmotes => {
                emote_treatment(settings) == EmoteTreatment::StripTwitch
            }
            OutputOpt::StripRewardEmotes => settings.strip_reward_emotes,
            OutputOpt::StripEmoji => settings.strip_emoji,
        }
    }

    pub(super) fn set_output(&mut self, opt: OutputOpt, value: bool, cx: &mut Context<Self>) {
        if self.output_flag(opt) == value {
            return;
        }
        self.edit(cx, |filters| {
            let settings = &mut filters.settings;
            match opt {
                OutputOpt::ReadName => settings.output_read_display_name_first = value,
                OutputOpt::EmoteWord => set_emote_to_word(settings, value),
                OutputOpt::Sanitize => settings.output_sanitize_punctuation = value,
                OutputOpt::StripTwitchEmotes => set_strip_twitch_emotes(settings, value),
                OutputOpt::StripRewardEmotes => settings.strip_reward_emotes = value,
                OutputOpt::StripEmoji => settings.strip_emoji = value,
            }
        });
    }

    pub(super) fn toggle_output(&mut self, opt: OutputOpt, cx: &mut Context<Self>) {
        let value = !self.output_flag(opt);
        self.set_output(opt, value, cx);
    }

    pub(super) fn toggle_output_max_duration(&mut self, cx: &mut Context<Self>) {
        self.edit(cx, |filters| {
            let settings = &mut filters.settings;
            settings.output_max_duration_enabled = !settings.output_max_duration_enabled;
        });
    }

    pub(super) fn clear_output_max_duration(&mut self, cx: &mut Context<Self>) {
        if self.shown().settings.output_max_duration_secs.is_none() {
            return;
        }
        self.edit(cx, |filters| {
            filters.settings.output_max_duration_secs = None;
            filters.settings.output_max_duration_enabled = true;
        });
    }

    pub(super) fn toggle_output_lang(&mut self, cx: &mut Context<Self>) {
        self.edit(cx, |filters| {
            let language = &mut filters.settings.output_language_aware_voice;
            *language = language.map(|enabled| !enabled);
        });
    }

    pub(super) fn clear_output_lang(&mut self, cx: &mut Context<Self>) {
        if self.shown().settings.output_language_aware_voice.is_none() {
            return;
        }
        self.edit(cx, |filters| {
            filters.settings.output_language_aware_voice = None
        });
    }

    pub(super) fn set_blocklist_mode(&mut self, mode: BlocklistMode, cx: &mut Context<Self>) {
        let shown = self.shown();
        let every_rule_matches = shown.rules.iter().all(|rule| match &rule.kind {
            FilterRuleKind::Blocklist { mode: held, .. } => *held == mode,
            _ => true,
        });
        if shown.settings.blocklist_mode == mode && every_rule_matches {
            return;
        }
        self.edit(cx, |filters| {
            filters.settings.blocklist_mode = mode;
            for rule in filters.rules.iter_mut() {
                if let FilterRuleKind::Blocklist { mode: m, .. } = &mut rule.kind {
                    *m = mode;
                }
            }
        });
    }

    pub(super) fn remove_blocklist_word(
        &mut self,
        rule_index: usize,
        word_index: usize,
        cx: &mut Context<Self>,
    ) {
        self.edit(cx, |filters| {
            let mut drop_rule = false;
            if let Some(rule) = filters.rules.get_mut(rule_index)
                && let FilterRuleKind::Blocklist { words, .. } = &mut rule.kind
            {
                if word_index < words.len() {
                    words.remove(word_index);
                }
                drop_rule = words.is_empty();
            }
            if drop_rule && rule_index < filters.rules.len() {
                filters.rules.remove(rule_index);
            }
        });
    }

    pub(super) fn expand_blocklist(&mut self, cx: &mut Context<Self>) {
        self.blocklist_expanded = true;
        cx.notify();
    }

    pub(super) fn toggle_rule(&mut self, index: usize, cx: &mut Context<Self>) {
        self.edit(cx, |filters| {
            if let Some(rule) = filters.rules.get_mut(index) {
                rule.enabled = !rule.enabled;
            }
        });
    }

    pub(super) fn delete_rule(&mut self, index: usize, cx: &mut Context<Self>) {
        self.edit(cx, |filters| {
            if index < filters.rules.len() {
                filters.rules.remove(index);
            }
        });
    }

    pub(super) fn open_add_modal(&mut self, stage: ModalStage, cx: &mut Context<Self>) {
        let Some(filters) = self.filters.loaded() else {
            return;
        };
        let mode = filters.settings.blocklist_mode;
        let modal = cx.new(|cx| AddFilterModal::new(stage, mode, cx));
        self._add_sub = Some(cx.subscribe(&modal, Self::on_add_event));
        self.add_modal = Some(modal);
        self.save_error = None;
        cx.notify();
    }

    fn close_add_modal(&mut self, cx: &mut Context<Self>) {
        self.add_modal = None;
        self._add_sub = None;
        self.save_error = None;
        cx.notify();
    }

    fn on_add_event(
        &mut self,
        _modal: Entity<AddFilterModal>,
        event: &AddFilterEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            AddFilterEvent::Submit(draft) => self.apply_draft(draft, cx),
            AddFilterEvent::Cancel => self.close_add_modal(cx),
        }
    }

    fn apply_draft(&mut self, draft: &FilterDraft, cx: &mut Context<Self>) {
        let applied = match draft {
            FilterDraft::Skip { preset, param } => {
                self.apply_skip_preset(*preset, param.clone(), cx)
            }
            FilterDraft::Output { preset, param } => self.apply_output_preset(*preset, param, cx),
            FilterDraft::Blocklist { words, mode } => {
                self.apply_blocklist_words(words.clone(), *mode, cx)
            }
            FilterDraft::Replace {
                kind,
                from,
                to,
                match_inside_words,
            } => self.apply_replacement(*kind, from.clone(), to.clone(), *match_inside_words, cx),
        };
        if applied {
            self.close_add_modal(cx);
        } else {
            cx.notify();
        }
    }

    pub(super) fn apply_skip_preset(
        &mut self,
        preset: SkipPreset,
        param: String,
        cx: &mut Context<Self>,
    ) -> bool {
        self.try_apply(cx, |filters| match preset {
            SkipPreset::Url => filters.settings.skip_contains_url = true,
            SkipPreset::Prefix => {
                filters.settings.skip_prefix = Some(param);
                filters.settings.skip_prefix_enabled = true;
                filters.settings.skip_starts_with_bang = false;
            }
            SkipPreset::Bots => filters.settings.skip_from_bot_accounts = true,
            SkipPreset::Length => {
                filters.settings.skip_longer_than = true;
                if let Ok(n) = param.parse::<u32>() {
                    filters.settings.longer_than_max_chars = n;
                }
            }
            SkipPreset::Repeat => filters.settings.skip_repeat_of_recent = true,
            SkipPreset::EmoteOnly => filters.settings.skip_emote_only = true,
            SkipPreset::NonLatin => filters.settings.skip_mostly_non_latin = true,
            SkipPreset::Regex => filters
                .settings
                .skip_custom_regexes
                .push(SkipRegex::enabled(param)),
        })
    }

    pub(super) fn apply_output_preset(
        &mut self,
        preset: OutputPreset,
        param: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let max_duration = parse_max_duration(param);
        if preset == OutputPreset::MaxDur && max_duration.is_none() {
            return false;
        }
        self.try_apply(cx, |filters| match preset {
            OutputPreset::Name => filters.settings.output_read_display_name_first = true,
            OutputPreset::Emote => set_emote_to_word(&mut filters.settings, true),
            OutputPreset::Sanitize => filters.settings.output_sanitize_punctuation = true,
            OutputPreset::MaxDur => {
                filters.settings.output_max_duration_secs = max_duration;
                filters.settings.output_max_duration_enabled = true;
            }
            OutputPreset::Lang => filters.settings.output_language_aware_voice = Some(true),
        })
    }

    pub(super) fn apply_blocklist_words(
        &mut self,
        words: Vec<String>,
        mode: BlocklistMode,
        cx: &mut Context<Self>,
    ) -> bool {
        if words.is_empty() {
            return false;
        }
        self.try_apply(cx, |filters| {
            filters.settings.blocklist_mode = mode;
            if let Some(rule) = filters
                .rules
                .iter_mut()
                .find(|r| matches!(r.kind, FilterRuleKind::Blocklist { .. }))
            {
                if let FilterRuleKind::Blocklist {
                    words: existing,
                    mode: m,
                } = &mut rule.kind
                {
                    for word in &words {
                        if !existing.iter().any(|e| e.eq_ignore_ascii_case(word)) {
                            existing.push(word.clone());
                        }
                    }
                    *m = mode;
                }
            } else {
                let position = u32::try_from(filters.rules.len()).unwrap_or(u32::MAX);
                filters.rules.push(FilterRule {
                    id: ulid::Ulid::generate().to_string(),
                    name: String::new(),
                    enabled: true,
                    position,
                    kind: FilterRuleKind::Blocklist { words, mode },
                });
            }
        })
    }

    pub(super) fn apply_replacement(
        &mut self,
        kind: ReplaceKind,
        from: String,
        to: String,
        match_inside_words: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        self.try_apply(cx, |filters| {
            let filter_kind = match kind {
                ReplaceKind::Text => FilterRuleKind::Literal {
                    pattern: from,
                    replacement: to,
                    match_inside_words,
                },
                ReplaceKind::Regex => FilterRuleKind::Regex {
                    pattern: from,
                    replacement: to,
                },
            };
            let position = u32::try_from(filters.rules.len()).unwrap_or(u32::MAX);
            filters.rules.push(FilterRule {
                id: ulid::Ulid::generate().to_string(),
                name: String::new(),
                enabled: true,
                position,
                kind: filter_kind,
            });
        })
    }
}

pub(super) fn renumber(rules: &mut [FilterRule]) {
    for (i, rule) in rules.iter_mut().enumerate() {
        rule.position = u32::try_from(i).unwrap_or(u32::MAX);
    }
}
