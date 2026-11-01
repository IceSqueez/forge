use forge_components::FONT_XXS;
use forge_components::Spacing;
use forge_components::spacing;
use forge_components::{
    ActivePresentation, BORDER_THIN, Density, ForgePalette, fmt_bytes, scaled, status_dot, tr,
};
use forge_types::ClipId;
use gpui::{AnyElement, Context, EventEmitter, Pixels, SharedString, Window, div, prelude::*, px};
use std::sync::Arc;

use super::SoundboardHandles;
use crate::app::async_bridge;

pub(super) const FOOTER_FS: Pixels = FONT_XXS;
const FOOTER_DOT: Pixels = px(6.0);
pub(super) const FOOTER_PAD_Y: Pixels = px(7.0);
pub(super) const FOOTER_PAD_X: Pixels = px(14.0);

#[derive(Default, PartialEq, Eq)]
pub(super) struct FooterFacts {
    pub(super) sounds: usize,
    pub(super) categories: usize,
    pub(super) unadopted: Vec<ClipId>,
    pub(super) output_ready: bool,
}

pub(super) enum FooterEvent {
    AdoptionFinished,
}

pub(super) struct SoundboardFooter {
    pub(super) facts: FooterFacts,
    total_size: Option<u64>,
    pub(super) adopting_all: bool,
    pub(super) adopt_summary: Option<SharedString>,
    pub(super) handles: SoundboardHandles,
}

impl EventEmitter<FooterEvent> for SoundboardFooter {}

impl SoundboardFooter {
    pub(super) fn new(handles: SoundboardHandles) -> Self {
        Self {
            facts: FooterFacts::default(),
            total_size: None,
            adopting_all: false,
            adopt_summary: None,
            handles,
        }
    }

    pub(super) fn set_facts(&mut self, facts: FooterFacts, cx: &mut Context<Self>) {
        if self.facts != facts {
            self.facts = facts;
            cx.notify();
        }
    }

    pub(super) fn clips_reloaded(&mut self, cx: &mut Context<Self>) {
        self.adopt_summary = None;
        self.refresh_library_size(cx);
        cx.notify();
    }

    pub(super) fn refresh_library_size(&self, cx: &mut Context<Self>) {
        let library = Arc::clone(&self.handles.library);
        async_bridge::run_async(
            &self.handles.rt_handle,
            async move { library.total_bytes().await },
            |this, result, cx| {
                this.total_size = match result {
                    Ok(total) => Some(total),
                    Err(error) => {
                        tracing::warn!(error = %error, "media library size unavailable");
                        None
                    }
                };
                cx.notify();
            },
            cx,
        );
    }

    fn render_counts(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> AnyElement {
        let category_count = self.facts.categories;
        let size_label = self
            .total_size
            .map(fmt_bytes)
            .unwrap_or_else(|| "-".to_owned());
        let ready = self.facts.output_ready;
        let (dot, status_text) = if ready {
            (palette.success, tr!("soundboard_output_ready"))
        } else {
            (palette.warning, tr!("soundboard_output_missing"))
        };
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .py(scaled(FOOTER_PAD_Y, density))
            .px(scaled(FOOTER_PAD_X, density))
            .child(
                div()
                    .font_family(cx.mono_family())
                    .text_size(FOOTER_FS)
                    .text_color(palette.text_faint)
                    .child(tr!(
                        "soundboard_footer_left",
                        sounds = i64::try_from(self.facts.sounds).unwrap_or(i64::MAX),
                        categories = i64::try_from(category_count).unwrap_or(i64::MAX),
                        size = size_label.as_str()
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(status_dot(dot, FOOTER_DOT))
                    .child(
                        div()
                            .font_family(cx.mono_family())
                            .text_size(FOOTER_FS)
                            .text_color(palette.text_faint)
                            .child(status_text),
                    ),
            )
            .into_any_element()
    }
}

impl Render for SoundboardFooter {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();
        div()
            .w_full()
            .flex()
            .flex_col()
            .border_t(BORDER_THIN)
            .border_color(palette.surface_overlay)
            .bg(palette.shell)
            .children(self.render_adoption_strip(&palette, density, cx))
            .child(self.render_counts(&palette, density, cx))
    }
}
