use forge_components::{ChipGlyph, Density, ForgePalette, InputEvent, TextInput, chip, tr};
use gpui::{AnyElement, ClickEvent, Context, Entity, Pixels, div, prelude::*, px};

use super::SoundboardView;
use super::categories::{category_color, category_label};
use super::pad::GRID_GAP;

const SEARCH_WIDTH: Pixels = px(220.0);

impl SoundboardView {
    pub(super) fn on_search_event(
        &mut self,
        _input: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if self.search.on_changed(event) {
            cx.notify();
        }
    }

    fn set_category_filter(&mut self, filter: Option<String>, cx: &mut Context<Self>) {
        self.category_filter = filter;
        cx.notify();
    }

    pub(super) fn filtered_indices(&self) -> Vec<usize> {
        self.clips
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                self.category_filter
                    .as_ref()
                    .is_none_or(|f| &c.category == f)
            })
            .filter(|(_, c)| self.search.matches(&c.name))
            .map(|(i, _)| i)
            .collect()
    }

    pub(super) fn render_subheader_left(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut chips = div().flex().items_center().gap(px(4.0)).child(
            chip(
                tr!("soundboard_category_all", count = self.clips.len() as i64),
                ChipGlyph::None,
                self.category_filter.is_none(),
                palette,
            )
            .density(density)
            .on_click(
                "sb-cat-all",
                cx.listener(|this, _: &ClickEvent, _, cx| this.set_category_filter(None, cx)),
            ),
        );
        for (idx, cat) in self.categories_present().into_iter().enumerate() {
            let active = self.category_filter.as_deref() == Some(cat.as_str());
            let color = category_color(&cat, palette);
            let for_click = cat.clone();
            chips = chips.child(
                chip(category_label(&cat), ChipGlyph::Dot(color), active, palette)
                    .density(density)
                    .on_click(
                        ("sb-cat", idx),
                        cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.set_category_filter(Some(for_click.clone()), cx)
                        }),
                    ),
            );
        }

        div()
            .flex()
            .items_center()
            .gap(GRID_GAP)
            .child(div().w(SEARCH_WIDTH).child(self.search.field().clone()))
            .child(chips)
            .into_any_element()
    }
}
