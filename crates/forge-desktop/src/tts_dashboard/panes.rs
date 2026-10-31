use super::*;

impl TtsDashboardView {
    pub(super) fn control_strip(
        &self,
        manual_paused: bool,
        gate_held: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let (pause_label, pause_glyph, btn_bg) = if manual_paused {
            (
                tr!("tts_dash_resume_btn"),
                Icon::PlayerPlay,
                palette.success,
            )
        } else {
            (tr!("tts_dash_pause_btn"), Icon::PlayerPause, palette.danger)
        };
        let pause_btn = div()
            .id("tts-pause")
            .flex()
            .items_center()
            .gap(scaled(BTN_GAP, density))
            .py(scaled(BTN_PAD_V, density))
            .px(scaled(SOLID_BTN_PAD_H, density))
            .rounded(radius(Radius::Sm))
            .bg(btn_bg)
            .cursor_pointer()
            .key_activated(palette.text_primary)
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_pause(cx)))
            .child(icon(pause_glyph, PAUSE_GLYPH, palette.shell))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(BTN_FONT)
                    .text_color(palette.shell)
                    .child(pause_label),
            );

        let skip_btn = self.ghost_strip_button(
            "tts-skip",
            Icon::PlayerSkipForward,
            palette.text_secondary,
            palette.text_primary,
            tr!("tts_dash_skip_btn"),
            palette,
            cx.listener(|this, _: &ClickEvent, _, cx| this.skip(cx)),
            density,
        );
        let stop_btn = self.ghost_strip_button(
            "tts-stop",
            Icon::PlayerStop,
            palette.danger,
            palette.danger,
            tr!("tts_dash_stop_all_btn"),
            palette,
            cx.listener(|this, _: &ClickEvent, _, cx| this.arm_stop_all(cx)),
            density,
        );

        let vol_pct = (self.volume * 100.0).round() as u32;
        let vol_text = div()
            .w(VOL_PCT_W)
            .font_family(mono_family())
            .text_size(VOL_PCT_FONT)
            .text_color(palette.text_muted)
            .child(format!("{vol_pct}%"));
        let vol_slider =
            div()
                .w(VOL_SLIDER_W)
                .child(slider(self.volume, 0.0, 1.0, palette).on_change(
                    "tts-vol",
                    cx.listener(|this, v: &f32, _, cx| this.set_volume(*v, cx)),
                ));
        let volume_row = div()
            .flex()
            .items_center()
            .gap(scaled(VOL_GAP, density))
            .child(icon(Icon::Volume, VOLUME_GLYPH, palette.text_muted))
            .child(vol_slider)
            .child(vol_text);

        let gate_indicator = gate_held.then(|| {
            div()
                .flex()
                .items_center()
                .gap(scaled(STRIP_GAP, density))
                .child(strip_divider(palette, density))
                .child(chip(
                    tr!("tts_dash_voice_gate_held"),
                    ChipGlyph::Icon(Icon::Microphone2, palette.warning),
                    true,
                    palette,
                ))
                .into_any_element()
        });

        let left = div()
            .flex()
            .items_center()
            .gap(scaled(STRIP_GAP, density))
            .child(pause_btn)
            .child(skip_btn)
            .child(stop_btn)
            .child(strip_divider(palette, density))
            .child(volume_row)
            .children(gate_indicator);

        let test_field = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .bg(palette.shell)
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(icon(Icon::TestPipe, TEST_ICON, palette.text_faint))
            .child(div().w(TEST_INPUT_W).child(self.test_input.clone()));
        let speak_btn = div()
            .id("tts-speak")
            .flex()
            .items_center()
            .gap(scaled(BTN_GAP, density))
            .py(scaled(BTN_PAD_V, density))
            .px(scaled(SOLID_BTN_PAD_H, density))
            .rounded(radius(Radius::Sm))
            .bg(palette.brand)
            .cursor_pointer()
            .key_activated(palette.text_primary)
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.speak_test(cx)))
            .child(icon(Icon::PlayerPlayFilled, SPEAK_BTN_GLYPH, palette.shell))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(BTN_FONT)
                    .text_color(palette.shell)
                    .child(tr!("tts_dash_speak_btn")),
            );
        let right = div()
            .flex()
            .items_center()
            .gap(scaled(STRIP_GAP, density))
            .child(test_field)
            .child(speak_btn);

        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .py(scaled(TOOLBAR_PAD_V, density))
            .px(spacing(Spacing::Md, density))
            .bg(palette.elevated)
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(left)
            .child(right)
            .into_any_element()
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "render helper takes each displayed value and handler separately"
    )]
    pub(super) fn ghost_strip_button(
        &self,
        id: &'static str,
        glyph: Icon,
        text_color: Rgba,
        hover_text: Rgba,
        label: impl Into<SharedString>,
        palette: &ForgePalette,
        handler: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
        density: Density,
    ) -> AnyElement {
        let hover_border = palette.border_input;
        div()
            .id(id)
            .flex()
            .items_center()
            .gap(scaled(BTN_GAP, density))
            .py(scaled(BTN_PAD_V, density))
            .px(scaled(GHOST_BTN_PAD_H, density))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .text_color(text_color)
            .cursor_pointer()
            .hover(move |s| s.border_color(hover_border).text_color(hover_text))
            .key_activated(palette.text_primary)
            .on_click(handler)
            .child(icon(glyph, STRIP_BTN_GLYPH, text_color))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(BTN_FONT)
                    .child(label.into()),
            )
            .into_any_element()
    }

    pub(super) fn right_pane(
        &self,
        stats: &SessionStats,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();

        let stat_rows = div()
            .flex()
            .flex_col()
            .child(stat_row(
                tr!("tts_dash_stat_spoken"),
                stats.spoken.to_string(),
                palette.brand,
                palette,
                true,
                density,
            ))
            .child(stat_row(
                tr!("tts_dash_stat_skipped"),
                stats.skipped.to_string(),
                palette.warning,
                palette,
                true,
                density,
            ))
            .child(stat_row(
                tr!("tts_dash_stat_filtered"),
                stats.filtered.to_string(),
                palette.danger,
                palette,
                true,
                density,
            ))
            .child(stat_row(
                tr!("tts_dash_stat_avg_latency"),
                NO_LATENCY_READING.to_owned(),
                palette.success,
                palette,
                false,
                density,
            ));

        let mut content = div()
            .flex()
            .flex_col()
            .child(section_heading(
                section_label(tr!("tts_dash_session_header"), palette),
                None,
                density,
            ))
            .child(
                div().mb(scaled(SECTION_GAP, density)).child(
                    card(stat_rows, palette)
                        .padding_xy(
                            scaled(STAT_CARD_PAD_V, density),
                            scaled(STAT_CARD_PAD_H, density),
                        )
                        .full_width(),
                ),
            )
            .child(section_heading(
                section_label(tr!("tts_dash_engines_header"), palette),
                None,
                density,
            ));

        if self.engines.is_empty() {
            content = content.child(
                div()
                    .font_family(body_family())
                    .text_size(STAT_LABEL_FONT)
                    .text_color(palette.text_muted)
                    .child(tr!("tts_dash_engines_none")),
            );
        }
        let voices = self
            .speak
            .as_ref()
            .map(|h| h.available_voices())
            .unwrap_or_default();
        for eng in &self.engines {
            let voice_count = voices.iter().filter(|v| v.engine_id.0 == eng.id).count();
            let voiceless = voice_count == 0;
            let color = if voiceless {
                palette.warning
            } else {
                palette.success
            };
            let class = engine_class_label(eng.class);
            let meta = if voiceless {
                format!("{class} \u{b7} {}", tr!("tts_dash_engine_no_voices"))
            } else {
                class
            };
            content = content.child(engine_card(eng.name.clone(), meta, color, palette, density));
        }

        let panel = div()
            .w(self.rail_width)
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(palette.shell)
            .border_l(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(
                div()
                    .id("tts-session-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_revealing_focus()
                    .py(scaled(RAIL_PAD_Y, density))
                    .px(scaled(RAIL_PAD_X, density))
                    .child(content),
            );

        install_resize(
            panel,
            TtsRailResizeDrag,
            "tts-rail-resize",
            ResizeEdge::Left,
            ResizeRange {
                min: RAIL_MIN_W,
                max: RAIL_MAX_W,
            },
            palette,
            cx.listener(|this, width: &Pixels, _, cx| this.set_rail_width(*width, cx)),
        )
        .into_any_element()
    }

    pub(super) fn render_stop_confirm(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let card = confirm_modal(
            tr!("tts_dash_stop_all_confirm_name"),
            tr!("tts_dash_stop_all_confirm_hint"),
            ConfirmTone::Destructive,
            palette,
        )
        .esc_hint(tr!("widget_confirm_esc_to_cancel"))
        .on_cancel(
            "tts-stop-cancel",
            tr!("common_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_stop_all(cx)),
        )
        .on_confirm(
            "tts-stop-confirm",
            tr!("tts_dash_stop_all_btn"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_stop_all(cx)),
        );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("tts-stop-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_stop_all(cx));
            })
    }

    pub(super) fn queue_section(
        &self,
        queue: &[QueueItem],
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let count = queue.len();
        let count_badge = badge(
            palette.surface_overlay,
            palette.text_muted,
            count.to_string(),
            false,
            COUNT_BADGE_FONT,
        );
        let title = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(section_label(tr!("tts_dash_queue_header"), palette))
            .child(count_badge);
        let total = (!queue.is_empty()).then(|| {
            let total: u32 = queue.iter().map(|item| item.duration_secs).sum();
            div()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(tr!("tts_dash_queue_total", secs = total))
                .into_any_element()
        });

        let list: AnyElement = if queue.is_empty() {
            empty_state(tr!("tts_dash_queue_empty"), palette)
                .density(density)
                .into_any_element()
        } else {
            let last = count - 1;
            let mut col = div().w_full().flex().flex_col();
            for (index, item) in queue.iter().enumerate() {
                col = col.child(self.queue_item_row(index, index == last, item, palette, cx));
            }
            col.into_any_element()
        };

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(section_heading(title, total, density))
            .child(
                div()
                    .id("tts-queue-drop")
                    .w_full()
                    .on_drop(cx.listener(|this, drag: &QueueRowDrag, _window, cx| {
                        this.reorder_queued(drag.request_id.clone(), None, cx)
                    }))
                    .child(card(list, palette).padding(px(0.0)).full_width()),
            )
            .into_any_element()
    }

    pub(super) fn queue_item_row(
        &self,
        index: usize,
        is_last: bool,
        item: &QueueItem,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let pos = div()
            .w(QUEUE_POS_W)
            .flex_shrink_0()
            .text_center()
            .font_family(mono_family())
            .text_size(QUEUE_INDEX_FONT)
            .text_color(palette.text_faint)
            .child(format!("{}", index + 1));

        let grip = icon(
            Icon::GripVertical,
            QUEUE_GRIP_GLYPH,
            palette.text_extreme_faint,
        );

        let mut name_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(QUEUE_NAME_FONT)
                    .text_color(hash_accent(&item.viewer_name, palette))
                    .child(item.viewer_name.clone()),
            );
        if item.is_high_priority {
            let label = tr!("tts_dash_priority_high");
            name_row = name_row.child(badge(
                palette.warning,
                palette.shell,
                label,
                false,
                QUEUE_BITS_FONT,
            ));
        }
        if !item.engine_voice.is_empty() {
            name_row = name_row.child(
                div()
                    .font_family(mono_family())
                    .text_size(QUEUE_PREVIEW_FONT)
                    .text_color(palette.text_faint)
                    .child(item.engine_voice.clone()),
            );
        }

        let content = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .child(name_row)
            .child(
                div()
                    .truncate()
                    .font_family(body_family())
                    .text_size(QUEUE_MSG_FONT)
                    .text_color(palette.text_muted)
                    .child(item.text.clone()),
            );

        let duration = div()
            .flex_shrink_0()
            .font_family(mono_family())
            .text_size(QUEUE_DUR_FONT)
            .text_color(palette.text_faint)
            .child(format!("~{}", fmt_clock(u64::from(item.duration_secs))));

        let play_id = item.request_id.clone();
        let play_btn = div()
            .id(SharedString::from(format!(
                "tts-q-play-{}",
                item.request_id.0
            )))
            .flex_shrink_0()
            .cursor_pointer()
            .tooltip(tooltip_builder(tr!("tts_dash_play_now"), palette))
            .key_activated(palette.text_primary)
            .on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| this.play_now(play_id.clone(), cx)),
            )
            .child(icon(Icon::PlayerPlay, QUEUE_ACTION_GLYPH, palette.success));

        let remove_id = item.request_id.clone();
        let remove_btn = div()
            .id(SharedString::from(format!(
                "tts-q-remove-{}",
                item.request_id.0
            )))
            .flex_shrink_0()
            .cursor_pointer()
            .tooltip(tooltip_builder(tr!("tts_dash_remove_queued"), palette))
            .key_activated(palette.text_primary)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.remove_queued(remove_id.clone(), cx)
            }))
            .child(icon(Icon::X, QUEUE_ACTION_GLYPH, palette.text_faint));

        let ghost = QueueRowGhost {
            name: item.viewer_name.clone().into(),
            name_color: hash_accent(&item.viewer_name, palette),
            text: item.text.clone().into(),
            text_color: palette.text_muted,
            surface: palette.elevated,
            border: palette.border_regular,
        };
        let view = cx.entity().downgrade();
        let drop_tint = with_alpha(palette.brand, QUEUE_DROP_TINT_ALPHA);
        let brand = palette.brand;
        let hovered_id = item.request_id.clone();
        let dropped_on_id = item.request_id.clone();

        let mut row = div()
            .id(SharedString::from(format!(
                "tts-q-row-{}",
                item.request_id.0
            )))
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .py(scaled(QUEUE_ROW_PAD_V, density))
            .px(scaled(QUEUE_ROW_PAD_H, density))
            .cursor(CursorStyle::OpenHand)
            .on_drag(
                QueueRowDrag {
                    request_id: item.request_id.clone(),
                },
                move |drag, _offset, _window, cx| {
                    let dragged = drag.request_id.clone();
                    view.update(cx, |this, cx| this.begin_row_drag(dragged, cx))
                        .ok();
                    cx.new(|_| ghost.clone())
                },
            )
            .drag_over::<QueueRowDrag>(move |style, drag, _window, _cx| {
                if drag.request_id == hovered_id {
                    return style;
                }
                style
                    .bg(drop_tint)
                    .border_color(brand)
                    .border_t(BORDER_THIN)
                    .border_b(px(0.0))
            })
            .on_drop(cx.listener(move |this, drag: &QueueRowDrag, _window, cx| {
                this.reorder_queued(drag.request_id.clone(), Some(dropped_on_id.clone()), cx)
            }))
            .child(pos)
            .child(grip)
            .child(content)
            .child(duration)
            .child(play_btn)
            .child(remove_btn);
        if !is_last {
            row = row.border_b(BORDER_THIN).border_color(palette.elevated);
        }

        if cx.has_active_drag() && self.dragged_row.as_ref() == Some(&item.request_id) {
            row.opacity(DRAG_SOURCE_OPACITY).into_any_element()
        } else {
            row.into_any_element()
        }
    }
}
