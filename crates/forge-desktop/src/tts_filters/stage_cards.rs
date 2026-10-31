use super::TtsFiltersView;
use super::filter_options::{ModalStage, OutputOpt, SkipRule};
use super::preview::is_replacement_kind;
use forge_components::{
    Density, FONT_XS, FONT_XXS, ForgePalette, Icon, Radius, Spacing, body_family, icon,
    mono_family, radio_row, radio_row_label, radius, scaled, spacing, tr,
};
use forge_speak_queue::configured_skip_prefix;
use forge_storage::{BlocklistMode, FilterRule, FilterRuleKind};
use gpui::prelude::*;
use gpui::{AnyElement, ClickEvent, Context, SharedString, div, px};

impl TtsFiltersView {
    pub(super) fn skip_card(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = &self.shown().settings;
        let fixed: [(SkipRule, SharedString); 6] = [
            (
                SkipRule::ContainsUrl,
                tr!("tts_filters_skip_contains_url").into(),
            ),
            (
                SkipRule::BotAccounts,
                tr!("tts_filters_skip_bot_accounts").into(),
            ),
            (
                SkipRule::LongerThan,
                tr!(
                    "tts_filters_skip_longer_than",
                    chars = settings.longer_than_max_chars as i64
                )
                .into(),
            ),
            (
                SkipRule::Repeat,
                tr!(
                    "tts_filters_skip_repeat",
                    window = settings.repeat_of_recent_window as i64
                )
                .into(),
            ),
            (
                SkipRule::EmoteOnly,
                tr!("tts_filters_skip_emote_only").into(),
            ),
            (
                SkipRule::MostlyNonLatin,
                tr!("tts_filters_skip_mostly_non_latin").into(),
            ),
        ];

        let prefix_display = configured_skip_prefix(settings);
        let regex_count = settings.skip_custom_regexes.len();
        let total = fixed.len() + usize::from(prefix_display.is_some()) + regex_count;

        let mut body = div().flex().flex_col();
        let mut i = 0usize;
        for (rule, label) in fixed {
            let on = self.skip_flag(rule);
            let toggle_id = SharedString::from(format!("filt-skip-t-{}", rule.key()));
            let x_id = SharedString::from(format!("filt-skip-x-{}", rule.key()));
            i += 1;
            body = body.child(self.stage_row(
                label,
                on,
                None,
                toggle_id,
                x_id,
                i != total,
                true,
                cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_skip(rule, cx)),
                cx.listener(move |this, _: &ClickEvent, _, cx| this.set_skip(rule, false, cx)),
                palette,
                density,
            ));
        }
        if let Some(prefix) = prefix_display {
            i += 1;
            body = body.child(self.stage_row(
                tr!("tts_filters_skip_prefix", prefix = prefix).into(),
                settings.skip_prefix_enabled,
                None,
                "filt-skip-t-prefix".into(),
                "filt-skip-x-prefix".into(),
                i != total,
                true,
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_skip_prefix(cx)),
                cx.listener(|this, _: &ClickEvent, _, cx| this.clear_skip_prefix(cx)),
                palette,
                density,
            ));
        }
        for (ri, regex) in settings.skip_custom_regexes.iter().enumerate() {
            i += 1;
            let x_id = SharedString::from(format!("filt-skip-regex-x-{ri}"));
            let toggle_id = SharedString::from(format!("filt-skip-regex-t-{ri}"));
            body = body.child(
                self.stage_row(
                    tr!(
                        "tts_filters_skip_regex_row",
                        pattern = regex.pattern.clone()
                    )
                    .into(),
                    regex.enabled,
                    Some(tr!("tts_filters_badge_regex").into()),
                    toggle_id,
                    x_id,
                    i != total,
                    true,
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_skip_regex(ri, cx)),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.remove_skip_regex(ri, cx)),
                    palette,
                    density,
                ),
            );
        }

        let add = self.add_button(
            "filt-skip-add",
            cx.listener(|this, _: &ClickEvent, _, cx| this.open_add_modal(ModalStage::Skip, cx)),
            palette,
            density,
        );

        self.stage_frame(
            1,
            Icon::FilterOff,
            palette.danger,
            tr!("tts_filters_stage_skip_title"),
            Some(add),
            body.into_any_element(),
            palette,
            density,
        )
    }

    pub(super) fn blocklist_card(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = &self.shown().settings;
        let censor_on = settings.blocklist_mode == BlocklistMode::Censor;
        let skip_on = settings.blocklist_mode == BlocklistMode::Suppress;

        let mode_choice = div()
            .flex()
            .flex_col()
            .gap(scaled(px(5.0), density))
            .py(spacing(Spacing::Xs, density))
            .child(
                radio_row(
                    "filt-bl-censor",
                    censor_on,
                    palette.warning,
                    radio_row_label(
                        tr!("tts_filters_blocklist_censor"),
                        Some(tr!("tts_filters_blocklist_censor_meta").into()),
                        censor_on,
                        palette,
                        density,
                    ),
                    palette,
                )
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.set_blocklist_mode(BlocklistMode::Censor, cx)
                })),
            )
            .child(
                radio_row(
                    "filt-bl-skip",
                    skip_on,
                    palette.warning,
                    radio_row_label(
                        tr!("tts_filters_blocklist_skip"),
                        None,
                        skip_on,
                        palette,
                        density,
                    ),
                    palette,
                )
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.set_blocklist_mode(BlocklistMode::Suppress, cx)
                })),
            );

        let body = div()
            .flex()
            .flex_col()
            .child(mode_choice)
            .child(self.blocklist_chips(palette, density, cx));

        let add = self.add_button(
            "filt-bl-add",
            cx.listener(|this, _: &ClickEvent, _, cx| {
                this.open_add_modal(ModalStage::Blocklist, cx)
            }),
            palette,
            density,
        );

        self.stage_frame(
            2,
            Icon::Ban,
            palette.warning,
            tr!("tts_filters_stage_blocklist_title"),
            Some(add),
            body.into_any_element(),
            palette,
            density,
        )
    }

    fn blocklist_chips(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut words: Vec<(usize, usize, String)> = Vec::new();
        for (ri, rule) in self.shown().rules.iter().enumerate() {
            if let FilterRuleKind::Blocklist { words: ws, .. } = &rule.kind {
                for (wi, w) in ws.iter().enumerate() {
                    words.push((ri, wi, w.clone()));
                }
            }
        }

        if words.is_empty() {
            return div()
                .py(spacing(Spacing::Xs, density))
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(palette.text_muted)
                .child(tr!("tts_filters_blocklist_empty"))
                .into_any_element();
        }

        let total = words.len();
        let visible = if self.blocklist_expanded {
            total
        } else {
            total.min(3)
        };

        let mut row = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .py(spacing(Spacing::Xs, density));

        for (ri, wi, word) in words.into_iter().take(visible) {
            let x_id = SharedString::from(format!("filt-bl-word-{ri}-{wi}"));
            row = row.child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xxs, density))
                    .py(spacing(Spacing::Xxs, density))
                    .px(spacing(Spacing::Xs, density))
                    .rounded(radius(Radius::Sm))
                    .bg(palette.surface_overlay)
                    .child(
                        div()
                            .font_family(mono_family())
                            .text_size(FONT_XXS)
                            .text_color(palette.text_primary)
                            .child(word),
                    )
                    .child(
                        div()
                            .id(x_id)
                            .flex()
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.remove_blocklist_word(ri, wi, cx)
                            }))
                            .child(icon(Icon::X, FONT_XXS, palette.text_faint)),
                    ),
            );
        }

        if !self.blocklist_expanded && total > 3 {
            row = row.child(
                div()
                    .id("filt-bl-more")
                    .cursor_pointer()
                    .py(spacing(Spacing::Xxs, density))
                    .px(spacing(Spacing::Xs, density))
                    .rounded(radius(Radius::Sm))
                    .bg(palette.surface_overlay)
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.expand_blocklist(cx)))
                    .child(
                        div()
                            .font_family(mono_family())
                            .text_size(FONT_XXS)
                            .text_color(palette.text_faint)
                            .child(tr!(
                                "tts_filters_blocklist_more",
                                count = i64::try_from(total - 3).unwrap_or(i64::MAX)
                            )),
                    ),
            );
        }

        row.into_any_element()
    }

    pub(super) fn replacements_card(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let reps: Vec<(usize, &FilterRule)> = self
            .shown()
            .rules
            .iter()
            .enumerate()
            .filter(|(_, r)| is_replacement_kind(&r.kind))
            .collect();

        let body: AnyElement = if reps.is_empty() {
            div()
                .py(spacing(Spacing::Xs, density))
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(palette.text_muted)
                .child(tr!("tts_filters_replacements_empty"))
                .into_any_element()
        } else {
            let last = reps.len() - 1;
            let mut col = div().flex().flex_col();
            for (pos, (index, rule)) in reps.into_iter().enumerate() {
                let toggle_id = SharedString::from(format!("filt-rep-t-{index}"));
                let x_id = SharedString::from(format!("filt-rep-x-{index}"));
                col = col.child(self.stage_row(
                    rule_summary(rule).into(),
                    rule.enabled,
                    Some(replacement_meta(rule).into()),
                    toggle_id,
                    x_id,
                    pos != last,
                    true,
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_rule(index, cx)),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.delete_rule(index, cx)),
                    palette,
                    density,
                ));
            }
            col.into_any_element()
        };

        let add = self.add_button(
            "filt-rep-add",
            cx.listener(|this, _: &ClickEvent, _, cx| this.open_add_modal(ModalStage::Replace, cx)),
            palette,
            density,
        );

        self.stage_frame(
            3,
            Icon::Replace,
            palette.info,
            tr!("tts_filters_stage_replacements_title"),
            Some(add),
            body,
            palette,
            density,
        )
    }

    pub(super) fn output_card(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = &self.shown().settings;
        let rows: [(OutputOpt, SharedString, SharedString, bool); 6] = [
            (
                OutputOpt::ReadName,
                tr!("tts_filters_output_read_name").into(),
                tr!("tts_filters_output_read_name_meta").into(),
                settings.output_read_display_name_first,
            ),
            (
                OutputOpt::EmoteWord,
                tr!("tts_filters_output_emote").into(),
                tr!("tts_filters_output_emote_meta").into(),
                self.output_flag(OutputOpt::EmoteWord),
            ),
            (
                OutputOpt::Sanitize,
                tr!("tts_filters_output_sanitize").into(),
                tr!("tts_filters_output_sanitize_meta").into(),
                settings.output_sanitize_punctuation,
            ),
            (
                OutputOpt::StripTwitchEmotes,
                tr!("tts_filters_output_strip_twitch_emotes").into(),
                tr!("tts_filters_output_strip_twitch_emotes_meta").into(),
                self.output_flag(OutputOpt::StripTwitchEmotes),
            ),
            (
                OutputOpt::StripRewardEmotes,
                tr!("tts_filters_output_strip_reward_emotes").into(),
                tr!("tts_filters_output_strip_reward_emotes_meta").into(),
                settings.strip_reward_emotes,
            ),
            (
                OutputOpt::StripEmoji,
                tr!("tts_filters_output_strip_emoji").into(),
                tr!("tts_filters_output_strip_emoji_meta").into(),
                settings.strip_emoji,
            ),
        ];

        let max_duration = settings.output_max_duration_secs;
        let language = settings.output_language_aware_voice;
        let total =
            rows.len() + usize::from(max_duration.is_some()) + usize::from(language.is_some());
        let mut body = div().flex().flex_col();
        let mut i = 0usize;
        for (opt, label, meta, on) in rows {
            let toggle_id = SharedString::from(format!("filt-out-t-{}", opt.key()));
            let x_id = SharedString::from(format!("filt-out-x-{}", opt.key()));
            i += 1;
            body = body.child(self.stage_row(
                label,
                on,
                Some(meta),
                toggle_id,
                x_id,
                i != total,
                true,
                cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_output(opt, cx)),
                cx.listener(move |this, _: &ClickEvent, _, cx| this.set_output(opt, false, cx)),
                palette,
                density,
            ));
        }
        if let Some(secs) = max_duration {
            i += 1;
            body = body.child(self.stage_row(
                tr!("tts_filters_preset_output_maxdur").into(),
                settings.output_max_duration_enabled,
                Some(tr!("tts_filters_output_max_duration_meta", secs = secs as i64).into()),
                "filt-out-t-maxdur".into(),
                "filt-out-x-maxdur".into(),
                i != total,
                true,
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_output_max_duration(cx)),
                cx.listener(|this, _: &ClickEvent, _, cx| this.clear_output_max_duration(cx)),
                palette,
                density,
            ));
        }
        if let Some(lang_on) = language {
            i += 1;
            body = body.child(self.stage_row(
                tr!("tts_filters_preset_output_lang").into(),
                lang_on,
                Some(tr!("tts_filters_output_lang_meta").into()),
                "filt-out-t-lang".into(),
                "filt-out-x-lang".into(),
                i != total,
                true,
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_output_lang(cx)),
                cx.listener(|this, _: &ClickEvent, _, cx| this.clear_output_lang(cx)),
                palette,
                density,
            ));
        }

        let add = self.add_button(
            "filt-out-add",
            cx.listener(|this, _: &ClickEvent, _, cx| this.open_add_modal(ModalStage::Output, cx)),
            palette,
            density,
        );

        self.stage_frame(
            4,
            Icon::Send,
            palette.success,
            tr!("tts_filters_stage_output_title"),
            Some(add),
            body.into_any_element(),
            palette,
            density,
        )
    }
}

fn rule_summary(rule: &FilterRule) -> String {
    match &rule.kind {
        FilterRuleKind::Literal {
            pattern,
            replacement,
            ..
        }
        | FilterRuleKind::Regex {
            pattern,
            replacement,
        } => format!("{pattern} → {replacement}"),
        FilterRuleKind::Blocklist { words, .. } => words.join(", "),
    }
}

pub(super) fn replacement_meta(rule: &FilterRule) -> String {
    let base = match &rule.kind {
        FilterRuleKind::Regex { .. } => tr!("tts_filters_badge_regex"),
        FilterRuleKind::Literal {
            match_inside_words: true,
            ..
        } => tr!("tts_filters_badge_text_inside_words"),
        _ => tr!("tts_filters_badge_text"),
    };
    if rule.name.trim().is_empty() {
        base
    } else {
        format!("{base} ({})", rule.name.trim())
    }
}
