use super::{
    BADGE_GLYPH, BADGE_RADIUS, BAR_GAP, CARD_PAD, CARDS_PER_ROW, DESC_FS, HEADER_GAP, MAX_PILLS,
    MODE_PRESETS, PANEL_GLYPH, PILL_RADIUS, QueueRow, QueuesView, RunningEntry, STAT_VALUE_FS,
    elapsed_text, mode_badge_key, mode_caption_key, mode_glyph, mode_ink,
};
use forge_components::{
    ActivePresentation, BORDER_THIN, Density, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon,
    KeyActivated, MenuItem, MenuPlacement, ROW_HOVER_WASH_ALPHA, Radius, Spacing,
    TINT_BORDER_ALPHA, TINT_STRONG_FILL_ALPHA, badge, body_family, card, icon, menu_button,
    menu_divider, menu_item, mono_family, radius, scaled, spacing, spinner, tooltip_builder, tr,
    with_alpha,
};
use forge_runtime::{MAX_PENDING_PER_QUEUE, QueueMode};
use forge_types::DEFAULT_QUEUE_ID;
use gpui::prelude::*;
use gpui::{
    AnyElement, App, ClickEvent, Context, Div, FontWeight, Rgba, SharedString, Stateful, Window,
    div, px,
};
use std::time::Instant;

const NAME_DESC_GAP: gpui::Pixels = px(3.0);
const CARD_BUTTON_GAP: gpui::Pixels = px(5.0);
const METRIC_CAPTION_MB: gpui::Pixels = px(3.0);
const CARD_BUTTON_PAD_V: gpui::Pixels = px(5.0);
const CARD_BUTTON_PAD_H: gpui::Pixels = px(11.0);

impl QueuesView {
    fn queue_card(
        &self,
        index: usize,
        q: &QueueRow,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let not_live = !q.live;

        let border_color = if q.mode == QueueMode::RUNNING {
            palette.border_input
        } else {
            with_alpha(mode_ink(q.mode, palette), 0.35)
        };

        let body = div()
            .h_full()
            .flex()
            .flex_col()
            .child(self.card_header(index, q, not_live, palette, cx))
            .child(self.card_metrics(q, palette, density))
            .child(status_panel(q, self.now, palette, density))
            .child(self.card_buttons(index, q, palette, density, cx));

        card(body, palette)
            .full_width()
            .full_height()
            .padding(scaled(CARD_PAD, density))
            .border_color(border_color)
            .into_any_element()
    }

    fn card_header(
        &self,
        index: usize,
        q: &QueueRow,
        not_live: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let spin_id = SharedString::from(format!("q-badge-spin-{}", q.id));
        let name = div()
            .font_family(mono_family())
            .text_size(FONT_SM)
            .text_color(palette.text_primary)
            .child(q.name.clone());

        let mut name_row = div()
            .flex()
            .items_center()
            .gap(scaled(BAR_GAP, density))
            .child(name)
            .child(status_badge(spin_id, q.mode, palette, density));
        if not_live {
            let id = q.id;
            name_row = name_row.child(not_live_badge(
                ("q-not-live", index),
                palette,
                cx.listener(move |this, _: &ClickEvent, _, cx| this.apply_live(id, cx)),
                density,
            ));
        }

        let desc_text = if q.description.is_empty() {
            SharedString::from("\u{a0}")
        } else {
            SharedString::from(q.description.clone())
        };
        let desc = div()
            .font_family(body_family())
            .text_size(DESC_FS)
            .text_color(palette.text_muted)
            .child(desc_text);

        let left = div()
            .flex()
            .flex_col()
            .gap(scaled(NAME_DESC_GAP, density))
            .child(name_row)
            .child(desc);

        div()
            .flex()
            .items_start()
            .justify_between()
            .mb(scaled(HEADER_GAP, density))
            .child(left)
            .child(self.card_menu(index, q, palette, cx))
            .into_any_element()
    }

    fn card_menu(
        &self,
        index: usize,
        q: &QueueRow,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = q.id;
        let is_default = q.id == DEFAULT_QUEUE_ID;
        let menu_open = self.menu_open == Some(id);
        let menu_pos = if menu_open { self.menu_click_pos } else { None };
        let view = cx.entity();

        let pause_resume: MenuItem = if q.mode == QueueMode::RUNNING {
            menu_item(
                ("q-menu-pause", index),
                tr!("queues_menu_pause"),
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.set_mode(id, QueueMode::PAUSED, cx)
                }),
            )
            .icon(Icon::PlayerPause)
            .color(palette.warning)
            .into()
        } else {
            menu_item(
                ("q-menu-resume", index),
                tr!("queues_menu_resume"),
                cx.listener(move |this, _: &ClickEvent, _, cx| this.resume(id, cx)),
            )
            .icon(Icon::PlayerPlay)
            .color(palette.success)
            .into()
        };

        menu_button(Icon::DotsVertical, menu_open, palette)
            .placement(MenuPlacement::BottomRight)
            .open_at(menu_pos)
            .items(vec![
                menu_item(
                    ("q-menu-configure", index),
                    tr!("queues_menu_configure"),
                    cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.open_configure(id, window, cx)
                    }),
                )
                .icon(Icon::Settings)
                .into(),
                menu_divider(),
                pause_resume,
                menu_item(
                    ("q-menu-free", index),
                    tr!("queues_menu_free"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.request_free(id, cx)),
                )
                .icon(Icon::Eraser)
                .into(),
                menu_divider(),
                menu_item(
                    ("q-menu-delete", index),
                    tr!("queues_menu_delete"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.request_delete(id, cx)),
                )
                .icon(Icon::Trash)
                .color(palette.danger)
                .disabled(is_default)
                .into(),
            ])
            .on_toggle(
                ("q-menu-trigger", index),
                cx.listener(move |this, ev: &ClickEvent, _, cx| {
                    this.toggle_menu(id, ev.position(), cx)
                }),
            )
            .on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_menu(cx));
            })
            .into_any_element()
    }

    fn card_metrics(&self, q: &QueueRow, palette: &ForgePalette, density: Density) -> AnyElement {
        let running = q.mode == QueueMode::RUNNING;
        let pending_value_color = if running {
            palette.text_primary
        } else {
            mode_ink(q.mode, palette)
        };
        let pending_hint_color = if running {
            palette.text_faint
        } else {
            mode_ink(q.mode, palette)
        };
        let pending_hint = if q.frozen() {
            SharedString::from(tr!("queues_metric_held"))
        } else if q.in_flight > 0 {
            SharedString::from(tr!("queues_metric_in_flight"))
        } else {
            SharedString::from(tr!("queues_metric_idle"))
        };

        let row = div()
            .flex()
            .flex_row()
            .child(metric_col(
                SharedString::from(tr!("queues_metric_concurrency")),
                q.concurrency.to_string(),
                q.concurrency_label(),
                palette.text_primary,
                palette.text_faint,
                palette,
                density,
            ))
            .child(metric_col(
                SharedString::from(tr!("queues_metric_pending")),
                q.pending.to_string(),
                pending_hint,
                pending_value_color,
                pending_hint_color,
                palette,
                density,
            ))
            .child(metric_col(
                SharedString::from(tr!("queues_metric_actions")),
                q.actions.to_string(),
                SharedString::from(tr!("queues_metric_assigned")),
                palette.text_primary,
                palette.text_faint,
                palette,
                density,
            ));

        div()
            .w_full()
            .pt(spacing(Spacing::Sm, density))
            .mb(spacing(Spacing::Sm, density))
            .border_t(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(row)
            .into_any_element()
    }

    pub(super) fn card_buttons(
        &self,
        index: usize,
        q: &QueueRow,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = q.id;

        let mut mode_row = div()
            .w_full()
            .flex()
            .flex_row()
            .items_stretch()
            .gap(spacing(Spacing::Xs, density));
        for preset in MODE_PRESETS {
            let active = q.mode == preset.mode;
            let ink = if active {
                palette.shell
            } else {
                palette.text_secondary
            };
            let fill = active.then(|| mode_ink(preset.mode, palette));
            let hint = if active {
                tr!("queues_mode_active_tooltip")
            } else {
                tr!(preset.tooltip_key)
            };
            let mode = preset.mode;
            mode_row = mode_row.child(
                card_button(
                    (preset.element_id, index),
                    preset.glyph,
                    SharedString::from(tr!(preset.label_key)),
                    ink,
                    fill,
                    palette,
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_mode(id, mode, cx)),
                    density,
                )
                .tooltip(tooltip_builder(hint, palette)),
            );
        }

        let free = card_button(
            ("q-free", index),
            Icon::Eraser,
            SharedString::from(tr!("queues_free_btn")),
            palette.text_secondary,
            None,
            palette,
            cx.listener(move |this, _: &ClickEvent, _, cx| this.request_free(id, cx)),
            density,
        )
        .tooltip(tooltip_builder(tr!("queues_free_tooltip"), palette));

        let mode_row = mode_row
            .child(div().w(BORDER_THIN).bg(palette.border_regular))
            .child(free);

        let configure = card_button(
            ("q-configure", index),
            Icon::Settings,
            SharedString::from(tr!("queues_configure_btn")),
            palette.text_secondary,
            None,
            palette,
            cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.open_configure(id, window, cx)
            }),
            density,
        );

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(mode_row)
            .child(div().w_full().flex().flex_row().child(configure))
            .into_any_element()
    }

    pub(super) fn queue_grid(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let gap = spacing(Spacing::Sm, density);
        let visible = self.visible_indices();
        let cards: Vec<AnyElement> = visible
            .into_iter()
            .map(|index| self.queue_card(index, &self.queues[index], palette, density, cx))
            .collect();

        let mut grid = div().w_full().flex().flex_col().gap(gap);
        let mut iter = cards.into_iter().peekable();
        while iter.peek().is_some() {
            let mut row = div().w_full().flex().flex_row().gap(gap);
            for _ in 0..CARDS_PER_ROW {
                match iter.next() {
                    Some(card) => row = row.child(div().flex_1().min_w_0().child(card)),
                    None => row = row.child(div().flex_1().min_w_0()),
                }
            }
            grid = grid.child(row);
        }
        grid.into_any_element()
    }
}

fn status_badge(
    spin_id: SharedString,
    mode: QueueMode,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let ink = mode_ink(mode, palette);
    let mark: AnyElement = if mode == QueueMode::RUNNING {
        spinner(spin_id, Icon::Loader2, BADGE_GLYPH, ink).into_any_element()
    } else {
        icon(mode_glyph(mode), BADGE_GLYPH, ink).into_any_element()
    };
    let label = SharedString::from(tr!(mode_badge_key(mode)));
    div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xxs, density))
        .py(spacing(Spacing::Xxs, density))
        .px(spacing(Spacing::Xs, density))
        .rounded(BADGE_RADIUS)
        .bg(palette.border_regular)
        .child(mark)
        .child(
            div()
                .font_family(mono_family())
                .text_size(FONT_XS)
                .text_color(ink)
                .child(label),
        )
        .into_any_element()
}

fn metric_col(
    caption: SharedString,
    value: String,
    hint: SharedString,
    value_color: Rgba,
    hint_color: Rgba,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .child(
            div()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_muted)
                .mb(scaled(METRIC_CAPTION_MB, density))
                .child(caption),
        )
        .child(
            div()
                .font_family(mono_family())
                .font_weight(FontWeight::MEDIUM)
                .text_size(STAT_VALUE_FS)
                .text_color(value_color)
                .child(value),
        )
        .child(
            div()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(hint_color)
                .child(hint),
        )
}

fn status_panel(
    q: &QueueRow,
    now: Instant,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let running = running_panel(q, now, palette, density);
    let status = if q.mode != QueueMode::RUNNING {
        Some(mode_panel(q, palette, density))
    } else if running.is_none() || q.overflowed > 0 {
        Some(activity_panel(q, palette, density))
    } else {
        None
    };
    div()
        .w_full()
        .flex()
        .flex_col()
        .children(running)
        .children(status)
        .into_any_element()
}

fn running_panel(
    q: &QueueRow,
    now: Instant,
    palette: &ForgePalette,
    density: Density,
) -> Option<AnyElement> {
    let first = q.running.first()?;
    if !q.blocking || q.running.len() > 1 {
        return Some(concurrent_panel(q, palette, density));
    }
    Some(serial_panel(q, first, now, palette, density))
}

fn mode_panel(q: &QueueRow, palette: &ForgePalette, density: Density) -> AnyElement {
    let ink = mode_ink(q.mode, palette);
    status_strip(
        icon(mode_glyph(q.mode), PANEL_GLYPH, ink).into_any_element(),
        SharedString::from(tr!(mode_caption_key(q.mode))),
        palette.text_primary,
        Some(ink),
        q,
        palette,
        density,
    )
}

fn activity_panel(q: &QueueRow, palette: &ForgePalette, density: Density) -> AnyElement {
    if q.in_flight == 0 {
        return status_strip(
            icon(Icon::CircleDashed, PANEL_GLYPH, palette.text_faint).into_any_element(),
            SharedString::from(tr!("queues_no_actions_running")),
            palette.text_faint,
            None,
            q,
            palette,
            density,
        );
    }
    let spin_id = SharedString::from(format!("q-strip-spin-{}", q.id));
    status_strip(
        spinner(spin_id, Icon::Loader2, PANEL_GLYPH, palette.brand).into_any_element(),
        SharedString::from(tr!("queues_running_count", count = q.in_flight)),
        palette.text_primary,
        None,
        q,
        palette,
        density,
    )
}

fn status_strip(
    mark: AnyElement,
    caption: SharedString,
    caption_ink: Rgba,
    tint: Option<Rgba>,
    q: &QueueRow,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let counts = div()
        .font_family(mono_family())
        .text_size(FONT_XXS)
        .text_color(palette.text_faint)
        .child(SharedString::from(tr!(
            "queues_strip_counts",
            pending = q.pending,
            in_flight = q.in_flight
        )));

    let mut strip = div()
        .w_full()
        .flex()
        .items_center()
        .gap(scaled(BAR_GAP, density))
        .py(spacing(Spacing::Xs, density))
        .px(spacing(Spacing::Sm, density))
        .mb(spacing(Spacing::Sm, density))
        .rounded(radius(Radius::Sm))
        .child(mark)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(caption_ink)
                .child(caption),
        );
    if q.overflowed > 0 {
        strip = strip.child(overflow_badge(q.overflowed, palette, density));
    }
    strip = strip.child(counts);

    match tint {
        Some(ink) => strip
            .bg(with_alpha(ink, ROW_HOVER_WASH_ALPHA))
            .border(BORDER_THIN)
            .border_color(with_alpha(ink, 0.20)),
        None => strip.bg(palette.shell),
    }
    .into_any_element()
}

fn overflow_badge(overflowed: u64, palette: &ForgePalette, density: Density) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xxs, density))
        .py(spacing(Spacing::Xxs, density))
        .px(spacing(Spacing::Xs, density))
        .rounded(BADGE_RADIUS)
        .bg(with_alpha(palette.danger, TINT_STRONG_FILL_ALPHA))
        .border(BORDER_THIN)
        .border_color(with_alpha(palette.danger, TINT_BORDER_ALPHA))
        .child(icon(Icon::AlertTriangle, BADGE_GLYPH, palette.danger))
        .child(
            div()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.danger)
                .child(SharedString::from(tr!(
                    "queues_overflow_badge",
                    count = i64::try_from(overflowed).unwrap_or(i64::MAX),
                    cap = i64::try_from(MAX_PENDING_PER_QUEUE).unwrap_or(i64::MAX)
                ))),
        )
        .into_any_element()
}

fn serial_panel(
    q: &QueueRow,
    entry: &RunningEntry,
    now: Instant,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let elapsed = elapsed_text(now.saturating_duration_since(entry.started_at));
    let spin_id = SharedString::from(format!("q-serial-spin-{}", q.id));
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(scaled(BAR_GAP, density))
        .py(spacing(Spacing::Xs, density))
        .px(spacing(Spacing::Sm, density))
        .mb(spacing(Spacing::Sm, density))
        .rounded(radius(Radius::Sm))
        .bg(palette.shell)
        .child(spinner(spin_id, Icon::Loader2, PANEL_GLYPH, palette.brand))
        .child(
            div()
                .flex_1()
                .font_family(mono_family())
                .text_size(FONT_XS)
                .text_color(palette.text_primary)
                .child(entry.name.clone()),
        )
        .child(
            div()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(SharedString::from(tr!(
                    "queues_running_for",
                    elapsed = elapsed.as_str()
                ))),
        )
        .into_any_element()
}

fn concurrent_panel(q: &QueueRow, palette: &ForgePalette, density: Density) -> AnyElement {
    let shown = q.running.len().min(MAX_PILLS);
    let overflow = q.running.len().saturating_sub(MAX_PILLS);

    let mut pills = div().flex().flex_wrap().gap(spacing(Spacing::Xxs, density));
    for entry in &q.running[..shown] {
        pills = pills.child(running_pill(entry.name.clone(), palette, density));
    }
    if overflow > 0 {
        pills = pills.child(running_pill(
            tr!(
                "queues_overflow_more",
                count = i64::try_from(overflow).unwrap_or(i64::MAX)
            ),
            palette,
            density,
        ));
    }

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xxs, density))
        .py(spacing(Spacing::Xs, density))
        .px(spacing(Spacing::Sm, density))
        .mb(spacing(Spacing::Sm, density))
        .rounded(radius(Radius::Sm))
        .bg(palette.shell)
        .child(
            div()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(SharedString::from(tr!("queues_running_now_header"))),
        )
        .child(pills)
        .into_any_element()
}

pub(crate) fn running_pill(
    label: impl Into<SharedString>,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement {
    badge(
        palette.border_regular,
        palette.text_primary,
        label,
        true,
        FONT_XS,
    )
    .weight(FontWeight::NORMAL)
    .padding_xy(
        spacing(Spacing::Xxs, density),
        spacing(Spacing::Xs, density),
    )
    .radius(PILL_RADIUS)
}

#[expect(
    clippy::too_many_arguments,
    reason = "render helper takes each displayed value and handler separately"
)]
fn card_button(
    id: impl Into<gpui::ElementId>,
    glyph: Icon,
    label: SharedString,
    ink: Rgba,
    fill: Option<Rgba>,
    palette: &ForgePalette,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    density: Density,
) -> Stateful<Div> {
    let mut btn = div()
        .id(id.into())
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .gap(scaled(CARD_BUTTON_GAP, density))
        .py(scaled(CARD_BUTTON_PAD_V, density))
        .px(scaled(CARD_BUTTON_PAD_H, density))
        .rounded(radius(Radius::Sm))
        .cursor_pointer()
        .key_activated(palette.text_primary)
        .on_click(handler)
        .child(icon(glyph, PANEL_GLYPH, ink))
        .child(
            div()
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(ink)
                .child(label),
        );
    match fill {
        Some(bg) => btn = btn.bg(bg),
        None => {
            let hover_border = palette.border_input;
            btn = btn
                .border(BORDER_THIN)
                .border_color(palette.border_regular)
                .hover(move |s| s.border_color(hover_border));
        }
    }
    btn
}

fn not_live_badge(
    id: impl Into<gpui::ElementId>,
    palette: &ForgePalette,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    density: Density,
) -> AnyElement {
    div()
        .id(id.into())
        .cursor_pointer()
        .key_activated(palette.text_primary)
        .on_click(handler)
        .tooltip(tooltip_builder(tr!("queues_not_live_tooltip"), palette))
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xxs, density))
        .py(spacing(Spacing::Xxs, density))
        .px(spacing(Spacing::Xs, density))
        .rounded(BADGE_RADIUS)
        .bg(with_alpha(palette.warning, TINT_STRONG_FILL_ALPHA))
        .border(BORDER_THIN)
        .border_color(with_alpha(palette.warning, TINT_BORDER_ALPHA))
        .child(icon(Icon::AlertTriangle, BADGE_GLYPH, palette.warning))
        .child(
            div()
                .font_family(mono_family())
                .text_size(FONT_XS)
                .text_color(palette.warning)
                .child(SharedString::from(tr!("queues_not_live_badge"))),
        )
        .into_any_element()
}
