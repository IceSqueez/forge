use super::{QueueFilter, QueuesView};
use forge_components::{
    ChipGlyph, Density, ForgePalette, Icon, SEARCH_FIELD_WIDTH, Spacing, chip,
    ghost_button_with_icon, header_stat, header_stats, primary_button_with_icon, spacing, tr,
};
use gpui::prelude::*;
use gpui::{ClickEvent, Context, div};

impl QueuesView {
    pub(super) fn render_stats<'a>(
        &self,
        palette: &'a ForgePalette,
        density: Density,
    ) -> impl IntoElement + use<'a> {
        let paused_count = self.queues.iter().filter(|q| q.frozen()).count();
        let running_count = self.queues.len().saturating_sub(paused_count);

        header_stats(
            vec![
                header_stat(
                    self.queues.len().to_string(),
                    palette.text_primary,
                    tr!("queues_stat_queues"),
                ),
                header_stat(
                    running_count.to_string(),
                    palette.success,
                    tr!("queues_stat_running"),
                ),
                header_stat(
                    paused_count.to_string(),
                    palette.warning,
                    tr!("queues_stat_paused"),
                ),
            ],
            palette,
            density,
        )
    }

    pub(super) fn render_subheader_left(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let mut chips = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density));
        for (id, filter) in QueueFilter::TABS {
            let active = self.status_filter == filter;
            chips = chips.child(
                chip(
                    tr!(filter.label_key()),
                    ChipGlyph::Dot(filter.dot(palette)),
                    active,
                    palette,
                )
                .on_click(
                    id,
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.set_status_filter(filter, cx)
                    }),
                ),
            );
        }

        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(
                div()
                    .w(SEARCH_FIELD_WIDTH)
                    .child(self.search.field().clone()),
            )
            .child(chips)
    }

    pub(super) fn render_subheader_right(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let pause_all =
            ghost_button_with_icon(Icon::PlayerPause, tr!("queues_pause_all_btn"), palette)
                .ink(palette.warning)
                .on_click(
                    "q-pause-all",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.pause_all(cx)),
                );
        let new_queue = primary_button_with_icon(Icon::Plus, tr!("queues_new_queue_btn"), palette)
            .density(density)
            .on_click(
                "q-new",
                cx.listener(|this, _: &ClickEvent, window, cx| this.open_new(window, cx)),
            );

        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(pause_all)
            .child(new_queue)
    }
}
