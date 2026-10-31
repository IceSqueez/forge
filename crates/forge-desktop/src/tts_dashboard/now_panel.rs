use super::*;

pub(super) fn strip_divider(palette: &ForgePalette, density: Density) -> impl IntoElement {
    div()
        .w(BORDER_THIN)
        .h(STRIP_DIVIDER_H)
        .mx(spacing(Spacing::Xxs, density))
        .bg(palette.border_regular)
}

pub(super) fn eq_bars(animate: bool, color: Rgba) -> impl IntoElement {
    let mut bars = div().flex().items_end().gap(EQ_BAR_GAP).h(EQ_BAR_MAX_H);
    for (i, base) in EQ_BAR_HEIGHTS.iter().enumerate() {
        let base = *base;
        let bar = div().w(EQ_BAR_W).h(px(base)).rounded(px(1.0)).bg(color);
        let child: AnyElement = if animate {
            bar.with_animation(
                ("tts-eq-bar", i),
                Animation::new(Duration::from_secs_f32(0.5 + i as f32 * 0.15))
                    .repeat()
                    .with_easing(bounce(ease_in_out))
                    .with_max_fps(EQ_BAR_FPS),
                move |el, delta| el.h(px(base * (EQ_MIN_SCALE + (1.0 - EQ_MIN_SCALE) * delta))),
            )
            .into_any_element()
        } else {
            bar.into_any_element()
        };
        bars = bars.child(child);
    }
    bars
}

pub(super) fn resolve_now_voice(
    engine_id: &str,
    voice_id: &str,
    voices: &[TtsVoice],
) -> Option<SharedString> {
    if engine_id.is_empty() && voice_id.is_empty() {
        return None;
    }
    let name = voices
        .iter()
        .find(|v| v.id.0 == voice_id)
        .map(|v| v.name.clone())
        .unwrap_or_else(|| voice_id.to_owned());
    Some(format!("{} \u{b7} {}", engine_display_name(engine_id), name).into())
}

pub(super) fn now_speaking_panel(
    now: Option<&NowSpeaking>,
    now_voice: Option<SharedString>,
    last_drop: Option<&str>,
    manual_paused: bool,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let animate = now.is_some() && !manual_paused;
    let bar_color = if animate {
        palette.success
    } else {
        palette.text_faint
    };
    let label = div()
        .flex()
        .items_center()
        .gap(scaled(NOW_LABEL_GAP, density))
        .child(section_label(tr!("tts_dash_now_speaking_header"), palette))
        .child(eq_bars(animate, bar_color));

    let body = match now {
        Some(ns) => {
            let tile = div()
                .w(NOW_TILE)
                .h(NOW_TILE)
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded(NOW_TILE_RADIUS)
                .bg(palette.surface_overlay)
                .child(icon(Icon::Message2Share, NOW_TILE_GLYPH, palette.brand));

            let mut name_row = div()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xs, density))
                .mb(NOW_NAME_MB)
                .child(
                    div()
                        .font_family(body_family())
                        .font_weight(FontWeight::MEDIUM)
                        .text_size(NOW_NAME_FONT)
                        .text_color(hash_accent(&ns.viewer_name, palette))
                        .child(ns.viewer_name.clone()),
                );
            if let Some(voice) = now_voice {
                name_row = name_row.child(badge(
                    palette.surface_overlay,
                    palette.text_muted,
                    voice,
                    true,
                    NOW_PILL_FONT,
                ));
            }

            let message = div()
                .font_family(body_family())
                .text_size(NOW_MSG_FONT)
                .line_height(relative(NOW_MSG_LINE_HEIGHT))
                .text_color(palette.text_primary)
                .child(ns.text.clone());

            let frac = if ns.total_secs > 0 {
                (ns.elapsed_secs as f32 / ns.total_secs as f32).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let progress = div()
                .flex()
                .items_center()
                .gap(scaled(PROGRESS_GAP, density))
                .mt(PROGRESS_MT)
                .child(
                    div()
                        .font_family(mono_family())
                        .text_size(PROGRESS_FONT)
                        .text_color(palette.text_muted)
                        .child(fmt_clock(u64::from(ns.elapsed_secs))),
                )
                .child(
                    div()
                        .flex_1()
                        .h(PROGRESS_BAR_H)
                        .rounded(PROGRESS_BAR_RADIUS)
                        .bg(palette.surface_overlay)
                        .child(
                            div()
                                .h_full()
                                .w(relative(frac))
                                .rounded(PROGRESS_BAR_RADIUS)
                                .bg(palette.success),
                        ),
                )
                .child(
                    div()
                        .font_family(mono_family())
                        .text_size(PROGRESS_FONT)
                        .text_color(palette.text_muted)
                        .child(fmt_clock(u64::from(ns.total_secs))),
                );

            let info = div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .child(name_row)
                .child(message)
                .child(progress);

            div()
                .flex()
                .items_start()
                .gap(spacing(Spacing::Sm, density))
                .child(tile)
                .child(info)
        }
        None => {
            let mut idle = div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xs, density))
                .child(
                    div()
                        .font_family(body_family())
                        .text_size(FONT_SM)
                        .text_color(palette.text_muted)
                        .child(tr!("tts_dash_no_speaking")),
                );
            if let Some(reason) = last_drop {
                idle = idle.child(
                    div()
                        .font_family(mono_family())
                        .text_size(FONT_XXS)
                        .text_color(palette.warning)
                        .child(tr!("tts_dash_last_drop", reason = reason)),
                );
            }
            idle
        }
    };

    div()
        .w_full()
        .flex()
        .flex_col()
        .child(section_heading(label, None, density))
        .child(
            div().mb(scaled(SECTION_GAP, density)).child(
                card(body, palette)
                    .padding(scaled(NOW_CARD_PAD, density))
                    .full_width(),
            ),
        )
        .into_any_element()
}

pub(super) fn section_heading(
    label: impl IntoElement,
    right: Option<AnyElement>,
    density: Density,
) -> impl IntoElement {
    div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .mb(spacing(Spacing::Sm, density))
        .child(label)
        .children(right)
}

pub(super) fn stat_row(
    label: impl Into<SharedString>,
    value: String,
    value_color: Rgba,
    palette: &ForgePalette,
    border_bottom: bool,
    density: Density,
) -> impl IntoElement {
    let mut row = div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .py(spacing(Spacing::Xs, density))
        .child(
            div()
                .flex_1()
                .font_family(body_family())
                .text_size(STAT_LABEL_FONT)
                .text_color(palette.text_muted)
                .child(label.into()),
        )
        .child(
            div()
                .font_family(body_family())
                .font_weight(FontWeight::MEDIUM)
                .text_size(STAT_VALUE_FONT)
                .text_color(value_color)
                .child(value),
        );
    if border_bottom {
        row = row
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular);
    }
    row
}

pub(super) fn engine_card(
    name: impl Into<SharedString>,
    meta: impl Into<SharedString>,
    status_color: Rgba,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement {
    let name = name.into();
    let meta = meta.into();
    let body = div()
        .flex()
        .flex_col()
        .gap(ENGINE_NAME_MB)
        .child(
            div()
                .flex()
                .items_center()
                .child(
                    div()
                        .flex_1()
                        .font_family(body_family())
                        .font_weight(FontWeight::MEDIUM)
                        .text_size(ENGINE_NAME_FONT)
                        .text_color(palette.text_primary)
                        .child(name),
                )
                .child(status_dot(status_color, ENGINE_DOT)),
        )
        .child(
            div()
                .font_family(mono_family())
                .text_size(ENGINE_META_FONT)
                .text_color(palette.text_faint)
                .child(meta),
        );
    div().mb(spacing(Spacing::Xs, density)).child(
        card(body, palette)
            .padding_xy(scaled(ENGINE_PAD_V, density), scaled(ENGINE_PAD_H, density))
            .full_width(),
    )
}
