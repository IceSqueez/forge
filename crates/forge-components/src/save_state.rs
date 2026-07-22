use gpui::{AnyElement, IntoElement, SharedString, div, prelude::*, px};

use crate::icons::{Icon, icon};
use crate::palette::ForgePalette;
use crate::tokens::{Density, FONT_SM, Spacing, body_family, spacing};
use crate::tr;

#[derive(Default)]
pub enum SaveState {
    #[default]
    Saved,
    Unsaved,
    Saving,
    Error(SharedString),
}

impl SaveState {
    /// A live error takes display priority; a dirtying edit does not clear it.
    pub fn mark_dirty(&mut self) {
        if !matches!(self, SaveState::Error(_)) {
            *self = SaveState::Unsaved;
        }
    }
}

pub fn save_indicator(state: &SaveState, palette: &ForgePalette) -> AnyElement {
    match state {
        SaveState::Error(message) => div()
            .font_family(body_family())
            .text_size(FONT_SM)
            .text_color(palette.random)
            .child(tr!("widget_save_failed", error = message.as_ref()))
            .into_any_element(),
        SaveState::Saving => div()
            .font_family(body_family())
            .text_size(FONT_SM)
            .text_color(palette.text_faint)
            .child(tr!("widget_save_saving"))
            .into_any_element(),
        SaveState::Saved => div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(icon(Icon::CircleCheck, px(13.0), palette.success))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.success)
                    .child(tr!("widget_save_all_saved")),
            )
            .into_any_element(),
        SaveState::Unsaved => div()
            .font_family(body_family())
            .text_size(FONT_SM)
            .text_color(palette.warning)
            .child(tr!("widget_save_unsaved"))
            .into_any_element(),
    }
}
