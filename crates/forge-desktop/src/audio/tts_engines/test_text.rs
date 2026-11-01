use std::sync::Arc;

use forge_components::{
    ActivePresentation, Density, FONT_XXS, ForgePalette, Spacing, TextArea, error_row_with_retry,
    section_label, spacing, tr,
};
use forge_storage::StorageError;
use gpui::{ClickEvent, Context, Entity, Pixels, div, prelude::*, px};

use super::TtsEnginesView;
use crate::app::async_bridge::{self, ErrorSink, Loadable};
use crate::audio::voice_test_text::{
    VOICE_TEST_TEXT_MAX_CHARS, default_voice_test_text, stored_voice_test_text,
    voice_test_text_too_long,
};

const TEST_TEXT_H: Pixels = px(56.0);

pub(super) fn new_input(cx: &mut Context<TtsEnginesView>) -> Entity<TextArea> {
    let default_text = default_voice_test_text();
    cx.new(|cx| {
        let mut input = TextArea::new(default_text.clone(), cx).with_height(TEST_TEXT_H);
        input.set_content(default_text, cx);
        input
    })
}

impl TtsEnginesView {
    pub(super) fn load_test_text(&mut self, cx: &mut Context<Self>) {
        self.test_text_load = Loadable::Loading;
        let settings = Arc::clone(&self.settings);
        async_bridge::run_async(
            &self.rt_handle,
            async move { forge_storage::voice_test_text(settings.as_ref()).await },
            |this, result: Result<Option<String>, StorageError>, cx| {
                if let Err(error) = &result {
                    tracing::warn!(%error, "failed to load the voice test text");
                }
                this.test_text_load = Loadable::settled(result.map(|stored| {
                    if let Some(stored) = stored {
                        this.test_text
                            .update(cx, |input, cx| input.set_content(stored, cx));
                    }
                }));
                cx.notify();
            },
            cx,
        );
        cx.notify();
    }

    pub(super) fn save_test_text(&mut self, cx: &mut Context<Self>) {
        cx.notify();
        if !self.test_text_load.is_loaded() {
            return;
        }
        let stored = stored_voice_test_text(
            self.test_text.read(cx).content(),
            &default_voice_test_text(),
        );
        let settings = Arc::clone(&self.settings);
        self.test_text_debounce.schedule_reporting(
            &self.rt_handle,
            tr!("tts_voice_test_save_failed"),
            async move { forge_storage::set_voice_test_text(settings.as_ref(), stored).await },
            ErrorSink::Toast,
            cx,
        );
    }

    pub(super) fn test_text_section(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let failure = self.test_text_load.failure().map(|reason| {
            error_row_with_retry(
                "tts-engines-test-text-retry",
                tr!("tts_voice_test_load_failed", reason = reason),
                palette,
                cx.listener(|this, _: &ClickEvent, _, cx| this.load_test_text(cx)),
                density,
                cx,
            )
        });
        let too_long = voice_test_text_too_long(self.test_text.read(cx).content());
        let note = div()
            .font_family(cx.body_family())
            .text_size(FONT_XXS)
            .when_some(too_long.then_some(()), |note, ()| {
                note.text_color(palette.warning).child(tr!(
                    "tts_voice_test_too_long",
                    limit = i64::try_from(VOICE_TEST_TEXT_MAX_CHARS).unwrap_or(i64::MAX)
                ))
            })
            .when(!too_long, |note| {
                note.text_color(palette.text_faint)
                    .child(tr!("tts_voice_test_hint"))
            });

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .mb(spacing(Spacing::Md, density))
            .child(section_label(tr!("tts_voice_test_label"), palette, cx))
            .children(failure)
            .child(self.test_text.clone())
            .child(note)
    }
}
