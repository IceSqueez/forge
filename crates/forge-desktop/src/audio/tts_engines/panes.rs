use super::*;
use forge_components::ActivePresentation;
use gpui::App;

impl TtsEnginesView {
    pub(super) fn engine_list(
        &self,
        catalog: &[TtsVoice],
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> AnyElement {
        let header = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .px(spacing(Spacing::Xs, density))
            .pb(spacing(Spacing::Sm, density))
            .child(
                div()
                    .font_family(cx.mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(tr!("tts_engines_header_prefix")),
            )
            .child(
                div()
                    .text_size(FONT_MICRO)
                    .text_color(palette.text_faint)
                    .child(self.engines.len().to_string()),
            );

        let mut entries = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density));
        for (index, engine) in self.engines.iter().enumerate() {
            let count = engine_voice_count(catalog, &engine.id);
            let disabled = self
                .disabled
                .loaded()
                .is_some_and(|set| set.contains(&engine.id));
            entries = entries
                .child(self.engine_entry(index, engine, count, disabled, palette, density, cx));
        }

        let failure = self.disabled.failure().map(|reason| {
            div()
                .w_full()
                .pb(spacing(Spacing::Sm, density))
                .child(error_row_with_retry(
                    "tts-engines-disabled-retry",
                    tr!("tts_engines_disabled_load_failed", reason = reason),
                    palette,
                    cx.listener(|this, _: &ClickEvent, _, cx| this.load_disabled(cx)),
                    density,
                    cx,
                ))
        });
        let column = div()
            .w_full()
            .flex()
            .flex_col()
            .child(header)
            .children(failure)
            .child(entries)
            .child(self.add_engine_button(palette, density, cx))
            .child(self.add_picker(palette, density, cx));

        div()
            .id("tts-engines-list")
            .w(RAIL_W)
            .flex_shrink_0()
            .h_full()
            .bg(palette.shell)
            .border_r(BORDER_THIN)
            .border_color(palette.border_regular)
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Xs, density))
            .overflow_y_revealing_focus()
            .child(column)
            .into_any_element()
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "render helper takes each displayed value and handler separately"
    )]
    pub(super) fn engine_entry(
        &self,
        index: usize,
        engine: &RosterEngine,
        voice_count: usize,
        disabled: bool,
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> AnyElement {
        let selected = self.selected == Selection::Engine(index);
        let name_color = if disabled {
            palette.text_faint
        } else if selected {
            palette.text_primary
        } else {
            palette.text_secondary
        };
        let dot_color = if disabled {
            palette.text_faint
        } else {
            engine_status_color(engine.class, palette)
        };

        let identity = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .font_family(cx.body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_XS)
                    .text_color(name_color)
                    .child(engine.name.clone()),
            )
            .child(
                div()
                    .font_family(cx.mono_family())
                    .text_size(FONT_MICRO)
                    .text_color(palette.text_faint)
                    .child(tr!(
                        "tts_engines_rail_sub",
                        kind = engine_class_label(engine.class),
                        count = i64::try_from(voice_count).unwrap_or(i64::MAX)
                    )),
            );

        div()
            .id((gpui::ElementId::from("tts-engine"), engine.id.clone()))
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .when(selected, |d| d.bg(palette.surface_overlay))
            .cursor_pointer()
            .key_activated(palette.text_primary)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select_engine(index, cx)))
            .child(status_dot(dot_color, STATUS_DOT))
            .child(identity)
            .into_any_element()
    }

    pub(super) fn add_engine_button(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> AnyElement {
        div()
            .id("tts-add-engine")
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .gap(spacing(Spacing::Xxs, density))
            .mt(spacing(Spacing::Sm, density))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_dashed()
            .border_color(palette.border_regular)
            .cursor_pointer()
            .key_activated(palette.text_primary)
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_add_picker(cx)))
            .child(icon(Icon::Plus, PLUS_GLYPH, palette.brand))
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_LABEL)
                    .text_color(palette.brand)
                    .child(tr!("tts_engines_add_engine")),
            )
            .into_any_element()
    }

    pub(super) fn add_picker(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> AnyElement {
        if !self.add_open {
            return div().into_any_element();
        }
        let configured = self.configured_cloud_kinds();
        let mut list = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .mt(spacing(Spacing::Xxs, density));
        let mut any = false;
        for kind in CloudEngineKind::ALL {
            if configured.contains(kind.key()) {
                continue;
            }
            any = true;
            list = list.child(
                div()
                    .id(SharedString::from(format!("tts-add-pick-{}", kind.key())))
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .py(spacing(Spacing::Xs, density))
                    .px(spacing(Spacing::Sm, density))
                    .rounded(radius(Radius::Sm))
                    .cursor_pointer()
                    .hover(|s| s.bg(palette.surface_overlay))
                    .key_activated(palette.text_primary)
                    .on_click(
                        cx.listener(move |this, _: &ClickEvent, _, cx| this.choose_add(kind, cx)),
                    )
                    .child(icon(Icon::Cloud, PLUS_GLYPH, palette.brand))
                    .child(
                        div()
                            .font_family(cx.body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_secondary)
                            .child(kind.display_name()),
                    ),
            );
        }
        if !any {
            list = list.child(
                div()
                    .px(spacing(Spacing::Sm, density))
                    .py(spacing(Spacing::Xs, density))
                    .font_family(cx.body_family())
                    .text_size(FONT_META)
                    .text_color(palette.text_faint)
                    .child(tr!("tts_engines_add_none_left")),
            );
        }
        list.into_any_element()
    }

    pub(super) fn detail_pane(
        &self,
        catalog: &[TtsVoice],
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> AnyElement {
        let inner: AnyElement = match self.selected {
            Selection::Engine(index) => match self.engines.get(index) {
                Some(engine) => self.engine_detail(engine, catalog, palette, density, cx),
                None => self.detail_hint(palette, cx),
            },
            Selection::Adding(kind) => self.adding_detail(kind, palette, density, cx),
            Selection::None => self.detail_hint(palette, cx),
        };

        div()
            .id("tts-engine-detail-scroll")
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .bg(palette.base)
            .py(spacing(Spacing::Md, density))
            .px(spacing(Spacing::Lg, density))
            .overflow_y_revealing_focus()
            .child(inner)
            .into_any_element()
    }

    pub(super) fn detail_hint(&self, palette: &ForgePalette, cx: &App) -> AnyElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(tr!("tts_engines_select_hint")),
            )
            .into_any_element()
    }

    pub(super) fn engine_detail(
        &self,
        engine: &RosterEngine,
        catalog: &[TtsVoice],
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> AnyElement {
        let voices: Vec<&TtsVoice> = catalog
            .iter()
            .filter(|v| v.engine_id.0 == engine.id)
            .collect();
        let is_cloud = CloudEngineKind::from_engine_id(&engine.id).is_some();
        let region = self.regions.get(&engine.id).map(|s| s.as_str());

        let mut col = div().w_full().flex().flex_col().child(self.detail_header(
            &engine.name,
            engine.class,
            region,
            voices.len(),
            Some(&engine.id),
            palette,
            density,
            cx,
        ));
        if is_cloud {
            col = col.child(self.cloud.clone());
        }
        col.child(self.params_section(palette, density, cx))
            .child(self.voices_section(engine, &voices, palette, density, cx))
            .into_any_element()
    }

    pub(super) fn adding_detail(
        &self,
        kind: CloudEngineKind,
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .flex()
            .flex_col()
            .child(self.detail_header(
                kind.display_name(),
                EngineClass::Cloud,
                None,
                0,
                None,
                palette,
                density,
                cx,
            ))
            .child(self.cloud.clone())
            .into_any_element()
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "render helper takes each displayed value and handler separately"
    )]
    pub(super) fn detail_header(
        &self,
        name: &str,
        class: EngineClass,
        region: Option<&str>,
        voice_count: usize,
        engine_id: Option<&str>,
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> AnyElement {
        let tile = div()
            .w(TILE)
            .h(TILE)
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(radius(Radius::Md))
            .bg(palette.surface_overlay)
            .child(icon(engine_glyph(class), TILE_GLYPH, palette.brand));

        let kind = engine_class_label(class);
        let subtitle = match region {
            Some(region) => tr!(
                "tts_engines_detail_sub_region",
                kind = kind,
                region = region,
                count = i64::try_from(voice_count).unwrap_or(i64::MAX)
            ),
            None => tr!(
                "tts_engines_detail_sub",
                kind = kind,
                count = i64::try_from(voice_count).unwrap_or(i64::MAX)
            ),
        };

        let identity = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(cx.body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_MD)
                    .text_color(palette.text_primary)
                    .child(name.to_owned()),
            )
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_LABEL)
                    .text_color(palette.text_muted)
                    .child(subtitle),
            );

        let right: AnyElement = match engine_id {
            Some(id) => match self.disabled.loaded() {
                Some(disabled) => {
                    let id_owned = id.to_owned();
                    toggle(!disabled.contains(id), palette)
                        .on_click(
                            SharedString::from(format!("engine-toggle-{id}")),
                            cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.toggle_engine(id_owned.clone(), cx)
                            }),
                        )
                        .into_any_element()
                }
                None => div()
                    .opacity(DISABLED_OPACITY)
                    .child(toggle(false, palette))
                    .into_any_element(),
            },
            None => div().into_any_element(),
        };

        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .mb(spacing(Spacing::Md, density))
            .child(tile)
            .child(identity)
            .child(right)
            .into_any_element()
    }

    pub(super) fn voices_section(
        &self,
        engine: &RosterEngine,
        voices: &[&TtsVoice],
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> AnyElement {
        let header = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(section_label(
                tr!("tts_engines_voices_header_prefix"),
                palette,
                cx,
            ))
            .child(
                div()
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(tr!(
                        "tts_engines_voices_available",
                        count = i64::try_from(voices.len()).unwrap_or(i64::MAX)
                    )),
            );

        let body: AnyElement = if voices.is_empty() {
            empty_state(tr!("tts_engines_voices_empty"), palette)
                .density(density)
                .into_any_element()
        } else {
            let mut grid = div()
                .w_full()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xs, density));
            for row in voices.chunks(GRID_COLS) {
                let mut line = div().w_full().flex().gap(spacing(Spacing::Xs, density));
                for voice in row {
                    line = line.child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .child(self.voice_cell(&engine.id, voice, palette, density, cx)),
                    );
                }
                for _ in row.len()..GRID_COLS {
                    line = line.child(div().flex_1());
                }
                grid = grid.child(line);
            }
            grid.into_any_element()
        };

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(self.test_text_section(palette, density, cx))
            .child(header)
            .child(body)
            .into_any_element()
    }

    pub(super) fn voice_cell(
        &self,
        engine_id: &str,
        voice: &TtsVoice,
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> AnyElement {
        let avatar = avatar_tile(
            voice_initial(&voice.name),
            hash_accent(&voice.name, palette),
            palette,
        )
        .size(AVATAR)
        .font(FONT_META);

        let body = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(cx.body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(voice.name.clone()),
            )
            .child(
                div()
                    .font_family(cx.mono_family())
                    .text_size(FONT_MICRO)
                    .text_color(palette.text_faint)
                    .child(format!(
                        "{} · {}",
                        voice.locale,
                        voice_descriptor_label(voice)
                    )),
            );

        let engine_id = engine_id.to_owned();
        let voice_id = voice.id.0.clone();
        let play = div()
            .id(SharedString::from(format!("tts-voice-play-{voice_id}")))
            .flex_shrink_0()
            .cursor_pointer()
            .key_activated(palette.text_primary)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.preview_voice(engine_id.clone(), voice_id.clone(), cx)
            }))
            .child(icon(Icon::PlayerPlay, PLAY_GLYPH, palette.success));

        let row = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(avatar)
            .child(body)
            .child(play);

        div()
            .child(
                card(row, palette)
                    .padding_xy(spacing(Spacing::Sm, density), spacing(Spacing::Sm, density))
                    .full_width(),
            )
            .into_any_element()
    }

    pub(super) fn params_section(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let params = self.params.loaded().copied().unwrap_or_default();
        let failure = self.params.failure().map(|reason| {
            div()
                .w_full()
                .pb(spacing(Spacing::Sm, density))
                .child(error_row_with_retry(
                    "tts-engines-params-retry",
                    tr!("tts_engines_params_load_failed", reason = reason),
                    palette,
                    cx.listener(|this, _: &ClickEvent, _, cx| this.retry_params(cx)),
                    density,
                    cx,
                ))
        });
        let rows = div()
            .w_full()
            .flex()
            .flex_col()
            .when(!self.params.is_loaded(), |d| d.opacity(DISABLED_OPACITY))
            .child(param_row(
                tr!("tts_engines_param_pitch"),
                format!("{:+.0} st", params.pitch_semitones),
                params.pitch_semitones,
                PITCH_MIN_SEMITONES,
                PITCH_MAX_SEMITONES,
                "tts-engines-pitch",
                cx.listener(|this, v: &f32, _, cx| this.set_pitch(*v, cx)),
                palette,
                density,
                cx,
            ))
            .child(param_row(
                tr!("tts_engines_param_speed"),
                format!("{:.1}x", params.rate_multiplier),
                params.rate_multiplier,
                SPEED_MIN,
                SPEED_MAX,
                "tts-engines-speed",
                cx.listener(|this, v: &f32, _, cx| this.set_speed(*v, cx)),
                palette,
                density,
                cx,
            ))
            .child(param_row(
                tr!("tts_engines_param_volume"),
                format!("{}%", (params.gain * 100.0).round() as i64),
                params.gain,
                GAIN_MIN,
                GAIN_MAX,
                "tts-engines-volume",
                cx.listener(|this, v: &f32, _, cx| this.set_engine_volume(*v, cx)),
                palette,
                density,
                cx,
            ));

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .mb(spacing(Spacing::Md, density))
            .child(section_label(
                tr!("tts_engines_section_params"),
                palette,
                cx,
            ))
            .children(failure)
            .child(
                card(rows, palette)
                    .radius(Radius::Md)
                    .padding(spacing(Spacing::Md, density))
                    .full_width(),
            )
    }
}
