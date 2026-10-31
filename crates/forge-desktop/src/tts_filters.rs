mod add_filter_modal;
mod filter_edits;
mod filter_options;
mod pipeline_column;
mod preview;
mod preview_column;
mod stage_cards;
#[cfg(test)]
mod tests;

use self::add_filter_modal::AddFilterModal;
use self::filter_edits::renumber;
use self::preview::CachedPreview;
use crate::async_bridge;
use crate::async_bridge::{Loadable, SnapshotWriter};
use crate::repo_failure::RepoFailure;
use forge_components::ActivePresentation;
use forge_components::{FONT_XXS, ForgePalette, InputEvent, TextArea, mono_family, tr};
use forge_speak_queue::{PipelineConfigHandle, SpeakQueueHandle};
use forge_storage::{FilterRule, TtsFiltersRepo, TtsPipelineSettings};
use forge_types::Shared;
use gpui::prelude::*;
use gpui::{Context, Entity, SharedString, Subscription, Window, div, px};
use std::sync::{Arc, LazyLock};

#[derive(Clone, Default)]
struct FilterSet {
    rules: Vec<FilterRule>,
    settings: TtsPipelineSettings,
}

static BLANK_FILTERS: LazyLock<FilterSet> = LazyLock::new(FilterSet::default);

pub(crate) struct TtsFiltersView {
    repo: Arc<dyn TtsFiltersRepo>,
    pipeline_config: Option<PipelineConfigHandle>,
    bot_accounts: Shared<Vec<String>>,
    speak: Option<SpeakQueueHandle>,
    rt_handle: tokio::runtime::Handle,
    filters: Loadable<FilterSet>,
    writer: SnapshotWriter,
    add_modal: Option<Entity<AddFilterModal>>,
    _add_sub: Option<Subscription>,
    save_error: Option<String>,
    blocklist_expanded: bool,
    preview_input: Entity<TextArea>,
    cached_preview: Option<CachedPreview>,
    _preview_sub: Subscription,
}

impl TtsFiltersView {
    pub(crate) fn new(
        repo: Arc<dyn TtsFiltersRepo>,
        pipeline_config: Option<PipelineConfigHandle>,
        bot_accounts: Shared<Vec<String>>,
        speak: Option<SpeakQueueHandle>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();

        let preview_input = cx.new(|cx| {
            let mut input = TextArea::new(tr!("tts_filters_preview_input_placeholder"), cx)
                .with_palette(palette)
                .on_surface()
                .with_height(px(56.0));
            input.set_content("hey @koval check this out https://example.com POGGERS", cx);
            input
        });
        let preview_sub = cx.subscribe(&preview_input, |this, _input, event: &InputEvent, cx| {
            if let InputEvent::Changed(_) = event {
                this.refresh_preview(cx);
                cx.notify();
            }
        });

        let mut view = Self {
            repo,
            pipeline_config,
            bot_accounts,
            speak,
            writer: SnapshotWriter::new(&rt_handle),
            rt_handle,
            filters: Loadable::Loading,
            add_modal: None,
            _add_sub: None,
            save_error: None,
            blocklist_expanded: false,
            preview_input,
            cached_preview: None,
            _preview_sub: preview_sub,
        };
        view.refresh_preview(cx);
        view.reload(cx);
        view
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        self.filters = Loadable::Loading;
        let repo = Arc::clone(&self.repo);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let rules = repo.list_rules().await.map_err(RepoFailure::from)?;
                let settings = repo
                    .get_pipeline_settings()
                    .await
                    .map_err(RepoFailure::from)?;
                Ok::<_, RepoFailure>((rules, settings))
            },
            |this, result, cx| this.apply_loaded(result, cx),
            cx,
        );
        cx.notify();
    }

    fn apply_loaded(
        &mut self,
        result: Result<(Vec<FilterRule>, TtsPipelineSettings), RepoFailure>,
        cx: &mut Context<Self>,
    ) {
        if let Err(message) = &result {
            tracing::warn!(error = %message, "tts filters load failed");
        }
        self.filters = Loadable::settled(result.map(|(mut rules, settings)| {
            rules.sort_by_key(|r| r.position);
            renumber(&mut rules);
            FilterSet { rules, settings }
        }));
        self.refresh_preview(cx);
        cx.notify();
    }

    fn shown(&self) -> &FilterSet {
        self.filters.loaded().unwrap_or(&BLANK_FILTERS)
    }

    fn retry_load(&mut self, cx: &mut Context<Self>) {
        self.save_error = None;
        self.reload(cx);
    }
}

impl Render for TtsFiltersView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let pipeline = self.pipeline_column(&palette, density, cx);
        let preview = self.preview_column(&palette, density, cx);
        let modal_overlay = self.add_modal.clone();

        div()
            .size_full()
            .flex()
            .flex_row()
            .bg(palette.base)
            .child(pipeline)
            .child(preview)
            .children(modal_overlay)
    }
}

fn mono_caption(label: impl Into<SharedString>, palette: &ForgePalette) -> impl IntoElement {
    div()
        .font_family(mono_family())
        .text_size(FONT_XXS)
        .text_color(palette.text_muted)
        .child(label.into())
}
