use super::filter_options::{
    MAX_DURATION_MAX, MAX_DURATION_MIN, ModalStage, OUTPUT_PRESETS, OutputPreset, SKIP_PRESETS,
    SkipPreset, parse_max_duration,
};
use forge_components::ActivePresentation;
use forge_components::{
    BORDER_THIN, Density, FONT_XS, FONT_XXS, ForgePalette, Icon, InputEvent, ModalSize,
    OverlayPosition, Radius, Spacing, TextArea, TextInput, body_family, field_label, ghost_button,
    modal, mono_family, overlay, primary_button_with_icon, radio_row, radio_row_label, radius,
    scaled, spacing, toggle, tr,
};
use forge_storage::BlocklistMode;
use gpui::prelude::*;
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, EventEmitter, Pixels, Rgba, SharedString,
    Subscription, Window, div, px,
};

const MODAL_W: Pixels = px(480.0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReplaceKind {
    Text,
    Regex,
}

pub(super) enum FilterDraft {
    Skip {
        preset: SkipPreset,
        param: String,
    },
    Output {
        preset: OutputPreset,
        param: String,
    },
    Blocklist {
        words: Vec<String>,
        mode: BlocklistMode,
    },
    Replace {
        kind: ReplaceKind,
        from: String,
        to: String,
        match_inside_words: bool,
    },
}

pub(super) enum AddFilterEvent {
    Submit(FilterDraft),
    Cancel,
}

pub(super) struct AddFilterModal {
    pub(super) stage: ModalStage,
    skip_preset: SkipPreset,
    output_preset: OutputPreset,
    pub(super) param: Entity<TextInput>,
    blocklist_words: Entity<TextArea>,
    pub(super) blocklist_mode: BlocklistMode,
    replace_kind: ReplaceKind,
    pub(super) replace_from: Entity<TextInput>,
    replace_to: Entity<TextInput>,
    replace_inside_words: bool,
    _param_sub: Subscription,
}

impl EventEmitter<AddFilterEvent> for AddFilterModal {}

impl AddFilterModal {
    pub(super) fn new(
        stage: ModalStage,
        blocklist_mode: BlocklistMode,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let param = cx.new(|cx| TextInput::new(SharedString::default(), cx).with_palette(palette));
        let param_sub = cx.subscribe(&param, |this, _input, event: &InputEvent, cx| {
            if let InputEvent::Changed(_) = event {
                this.revalidate_param(cx);
                cx.notify();
            }
        });
        let blocklist_words = cx.new(|cx| {
            TextArea::new(tr!("tts_filters_modal_blocklist_words_placeholder"), cx)
                .with_palette(palette)
                .on_surface()
                .with_height(px(62.0))
        });
        let replace_from = cx.new(|cx| {
            TextInput::new(tr!("tts_filters_modal_replace_find_placeholder"), cx)
                .with_palette(palette)
        });
        let replace_to = cx.new(|cx| {
            TextInput::new(
                tr!("tts_filters_modal_replace_replace_text_placeholder"),
                cx,
            )
            .with_palette(palette)
        });
        AddFilterModal {
            stage,
            skip_preset: SkipPreset::Url,
            output_preset: OutputPreset::Name,
            param,
            blocklist_words,
            blocklist_mode,
            replace_kind: ReplaceKind::Text,
            replace_from,
            replace_to,
            replace_inside_words: false,
            _param_sub: param_sub,
        }
    }

    fn set_skip_preset(&mut self, preset: SkipPreset, cx: &mut Context<Self>) {
        self.skip_preset = preset;
        let placeholder = preset.placeholder();
        self.param.update(cx, |field, cx| {
            field.set_placeholder(placeholder, cx);
            field.set_content("", cx);
            field.set_invalid(false, cx);
        });
        cx.notify();
    }

    fn set_output_preset(&mut self, preset: OutputPreset, cx: &mut Context<Self>) {
        if preset == self.output_preset {
            return;
        }
        self.output_preset = preset;
        let placeholder = preset.placeholder();
        let content = preset.default_param();
        self.param.update(cx, |field, cx| {
            field.set_placeholder(placeholder, cx);
            field.set_content(content, cx);
            field.set_invalid(false, cx);
        });
        cx.notify();
    }

    pub(super) fn set_blocklist_mode(&mut self, mode: BlocklistMode, cx: &mut Context<Self>) {
        self.blocklist_mode = mode;
        cx.notify();
    }

    fn set_replace_kind(&mut self, kind: ReplaceKind, cx: &mut Context<Self>) {
        self.replace_kind = kind;
        cx.notify();
    }

    pub(super) fn toggle_replace_inside_words(&mut self, cx: &mut Context<Self>) {
        self.replace_inside_words = !self.replace_inside_words;
        cx.notify();
    }

    fn param_invalid(&self, cx: &App) -> bool {
        let raw = self.param.read(cx).content();
        match self.stage {
            ModalStage::Skip => match self.skip_preset {
                SkipPreset::Prefix | SkipPreset::Regex => raw.trim().is_empty(),
                SkipPreset::Length => raw.trim().parse::<u32>().is_err(),
                _ => false,
            },
            ModalStage::Output => match self.output_preset {
                OutputPreset::MaxDur => parse_max_duration(raw).is_none(),
                _ => false,
            },
            ModalStage::Blocklist | ModalStage::Replace => false,
        }
    }

    fn revalidate_param(&mut self, cx: &mut Context<Self>) {
        let invalid = self.param_invalid(cx);
        self.param
            .update(cx, |field, cx| field.set_invalid(invalid, cx));
    }

    fn is_valid(&self, cx: &App) -> bool {
        match self.stage {
            ModalStage::Skip => !self.param_invalid(cx),
            ModalStage::Output => !self.param_invalid(cx),
            ModalStage::Blocklist => !self.blocklist_words.read(cx).content().trim().is_empty(),
            ModalStage::Replace => !self.replace_from.read(cx).content().trim().is_empty(),
        }
    }

    pub(super) fn submit(&mut self, cx: &mut Context<Self>) {
        if !self.is_valid(cx) {
            return;
        }
        let draft = match self.stage {
            ModalStage::Skip => FilterDraft::Skip {
                preset: self.skip_preset,
                param: self.param.read(cx).content().trim().to_owned(),
            },
            ModalStage::Output => FilterDraft::Output {
                preset: self.output_preset,
                param: self.param.read(cx).content().trim().to_owned(),
            },
            ModalStage::Blocklist => FilterDraft::Blocklist {
                words: self
                    .blocklist_words
                    .read(cx)
                    .content()
                    .split([',', '\n'])
                    .map(str::trim)
                    .filter(|w| !w.is_empty())
                    .map(str::to_owned)
                    .collect(),
                mode: self.blocklist_mode,
            },
            ModalStage::Replace => FilterDraft::Replace {
                kind: self.replace_kind,
                from: self.replace_from.read(cx).content().trim().to_owned(),
                to: self.replace_to.read(cx).content().trim().to_owned(),
                match_inside_words: self.replace_inside_words,
            },
        };
        cx.emit(AddFilterEvent::Submit(draft));
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(AddFilterEvent::Cancel);
    }

    fn modal_meta(
        stage: ModalStage,
    ) -> (Icon, fn(&ForgePalette) -> Rgba, SharedString, SharedString) {
        match stage {
            ModalStage::Skip => (
                Icon::FilterOff,
                (|p: &ForgePalette| p.danger) as fn(&ForgePalette) -> Rgba,
                tr!("tts_filters_modal_skip_title").into(),
                tr!("tts_filters_modal_skip_subtitle").into(),
            ),
            ModalStage::Blocklist => (
                Icon::Ban,
                (|p: &ForgePalette| p.warning) as fn(&ForgePalette) -> Rgba,
                tr!("tts_filters_modal_blocklist_title").into(),
                tr!("tts_filters_modal_blocklist_subtitle").into(),
            ),
            ModalStage::Replace => (
                Icon::Replace,
                (|p: &ForgePalette| p.info) as fn(&ForgePalette) -> Rgba,
                tr!("tts_filters_modal_replace_title").into(),
                tr!("tts_filters_modal_replace_subtitle").into(),
            ),
            ModalStage::Output => (
                Icon::Send,
                (|p: &ForgePalette| p.success) as fn(&ForgePalette) -> Rgba,
                tr!("tts_filters_modal_output_title").into(),
                tr!("tts_filters_modal_output_subtitle").into(),
            ),
        }
    }

    fn render_skip_body(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let color = palette.danger;
        let mut list = div().flex().flex_col().gap(scaled(px(5.0), density));
        for preset in SKIP_PRESETS {
            let selected = self.skip_preset == preset;
            let id = SharedString::from(format!("filt-modal-skip-{}", preset.key()));
            list =
                list.child(
                    radio_row(
                        id,
                        selected,
                        color,
                        radio_row_label(preset.label(), None, selected, palette, density),
                        palette,
                    )
                    .on_click(cx.listener(
                        move |this, _: &ClickEvent, _, cx| this.set_skip_preset(preset, cx),
                    )),
                );
        }

        let mut body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(field_label(
                palette,
                tr!("tts_filters_modal_condition_label"),
                list,
            ));

        if let Some(param_label) = self.skip_preset.param_label() {
            body = body.child(field_label(palette, param_label, self.param.clone()));
        }

        body.into_any_element()
    }

    fn render_output_body(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let color = palette.success;
        let mut list = div().flex().flex_col().gap(scaled(px(5.0), density));
        for preset in OUTPUT_PRESETS {
            let selected = self.output_preset == preset;
            let id = SharedString::from(format!("filt-modal-output-{}", preset.key()));
            list = list.child(
                radio_row(
                    id,
                    selected,
                    color,
                    radio_row_label(
                        preset.label(),
                        Some(preset.hint()),
                        selected,
                        palette,
                        density,
                    ),
                    palette,
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.set_output_preset(preset, cx)
                })),
            );
        }

        let mut body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(field_label(
                palette,
                tr!("tts_filters_modal_condition_label"),
                list,
            ));

        if let Some(param_label) = self.output_preset.param_label() {
            body = body
                .child(field_label(palette, param_label, self.param.clone()))
                .child(
                    div()
                        .font_family(body_family())
                        .text_size(FONT_XXS)
                        .text_color(palette.text_faint)
                        .child(tr!(
                            "tts_filters_preset_output_maxdur_range",
                            min = MAX_DURATION_MIN as i64,
                            max = MAX_DURATION_MAX as i64
                        )),
                );
        }

        body.into_any_element()
    }

    fn render_blocklist_body(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let note = div()
            .font_family(body_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(tr!("tts_filters_modal_blocklist_note"));

        let censor_selected = self.blocklist_mode == BlocklistMode::Censor;
        let skip_selected = self.blocklist_mode == BlocklistMode::Suppress;
        let mode_list = div()
            .flex()
            .flex_col()
            .gap(scaled(px(5.0), density))
            .child(
                radio_row(
                    "filt-modal-bl-censor",
                    censor_selected,
                    palette.warning,
                    radio_row_label(
                        tr!("tts_filters_modal_blocklist_censor_row"),
                        Some(tr!("tts_filters_modal_blocklist_censor_row_hint").into()),
                        censor_selected,
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
                    "filt-modal-bl-skip",
                    skip_selected,
                    palette.warning,
                    radio_row_label(
                        tr!("tts_filters_modal_blocklist_skip_row"),
                        Some(tr!("tts_filters_modal_blocklist_skip_row_hint").into()),
                        skip_selected,
                        palette,
                        density,
                    ),
                    palette,
                )
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.set_blocklist_mode(BlocklistMode::Suppress, cx)
                })),
            );

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(field_label(
                palette,
                tr!("tts_filters_modal_blocklist_words_label"),
                self.blocklist_words.clone(),
            ))
            .child(note)
            .child(field_label(
                palette,
                tr!("tts_filters_modal_blocklist_when_matched_label"),
                mode_list,
            ))
            .into_any_element()
    }

    fn render_replace_body(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_regex = self.replace_kind == ReplaceKind::Regex;
        let mut tabs = div()
            .flex()
            .p(px(2.0))
            .gap(px(2.0))
            .rounded(radius(Radius::Sm))
            .bg(palette.shell)
            .border(BORDER_THIN)
            .border_color(palette.border_regular);
        for (kind, label) in [
            (ReplaceKind::Text, tr!("tts_filters_modal_replace_text_tab")),
            (
                ReplaceKind::Regex,
                tr!("tts_filters_modal_replace_regex_tab"),
            ),
        ] {
            let active = self.replace_kind == kind;
            let id = SharedString::from(format!("filt-modal-replace-tab-{}", tab_key(kind)));
            let fg = if active {
                palette.shell
            } else {
                palette.text_secondary
            };
            let mut chip = div()
                .id(id)
                .py(scaled(px(5.0), density))
                .px(scaled(px(14.0), density))
                .rounded(radius(Radius::Sm))
                .cursor_pointer()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(fg)
                .on_click(
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.set_replace_kind(kind, cx)),
                )
                .child(label);
            chip = if active { chip.bg(palette.info) } else { chip };
            tabs = tabs.child(chip);
        }

        let find_label = if is_regex {
            tr!("tts_filters_modal_replace_match_label")
        } else {
            tr!("tts_filters_modal_replace_find_label")
        };
        let note = div()
            .font_family(body_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(tr!("tts_filters_modal_replace_note"));

        let mut body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(tabs)
            .child(field_label(palette, find_label, self.replace_from.clone()))
            .child(field_label(
                palette,
                tr!("tts_filters_modal_replace_replace_label"),
                self.replace_to.clone(),
            ));
        if !is_regex {
            body = body.child(self.inside_words_row(palette, density, cx));
        }
        body.child(note).into_any_element()
    }

    fn inside_words_row(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(
                toggle(self.replace_inside_words, palette)
                    .on_color(palette.info)
                    .on_click(
                        "filt-modal-replace-inside-words",
                        cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.toggle_replace_inside_words(cx)
                        }),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_primary)
                            .child(tr!("tts_filters_modal_replace_inside_words_label")),
                    )
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_XXS)
                            .text_color(palette.text_faint)
                            .child(tr!("tts_filters_modal_replace_inside_words_hint")),
                    ),
            )
            .into_any_element()
    }
}

impl Render for AddFilterModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();
        let (glyph, color_fn, title, subtitle) = Self::modal_meta(self.stage);
        let color = color_fn(&palette);

        let body: AnyElement = match self.stage {
            ModalStage::Skip => self.render_skip_body(&palette, density, cx),
            ModalStage::Output => self.render_output_body(&palette, density, cx),
            ModalStage::Blocklist => self.render_blocklist_body(&palette, density, cx),
            ModalStage::Replace => self.render_replace_body(&palette, density, cx),
        };

        let valid = self.is_valid(cx);
        let status_text = if valid {
            tr!("tts_filters_modal_footer_valid")
        } else {
            tr!("tts_filters_modal_footer_invalid")
        };
        let submit_label = if self.stage == ModalStage::Blocklist {
            tr!("tts_filters_modal_add_words")
        } else {
            tr!("tts_filters_modal_add_rule")
        };

        let footer = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(status_text),
            )
            .child(
                div()
                    .flex()
                    .gap(spacing(Spacing::Xs, density))
                    .child(
                        ghost_button(tr!("tts_filters_modal_cancel"), &palette).on_click(
                            "filt-modal-cancel",
                            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
                        ),
                    )
                    .child(
                        primary_button_with_icon(Icon::Plus, submit_label, &palette)
                            .disabled(!valid)
                            .on_click(
                                "filt-modal-submit",
                                cx.listener(|this, _: &ClickEvent, _, cx| this.submit(cx)),
                            ),
                    ),
            );

        let card = modal(title, body, &palette)
            .subtitle(subtitle)
            .header_icon(glyph, color)
            .size(ModalSize::Md)
            .width(MODAL_W)
            .footer(footer)
            .on_close(
                "filt-modal-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
            );

        let view = cx.entity();
        overlay(card, &palette)
            .position(OverlayPosition::Center)
            .on_dismiss("filt-modal-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel(cx));
            })
            .into_any_element()
    }
}

fn tab_key(kind: ReplaceKind) -> &'static str {
    match kind {
        ReplaceKind::Text => "text",
        ReplaceKind::Regex => "regex",
    }
}
