use forge_components::tr;
use gpui::Context;

use super::TriggersRegistryView;
use super::create::CreateStage;
use crate::async_bridge;
use crate::collection_options::{
    ChoiceOptions, CollectionSource, distinct_sources, load_collection_options,
    watch_collection_revisions,
};
use crate::integrations::BuiltinRegistry;

impl TriggersRegistryView {
    #[must_use]
    pub fn with_builtins(mut self, builtins: BuiltinRegistry) -> Self {
        self.builtins = builtins;
        self
    }

    fn collection_sources(&self) -> Vec<CollectionSource> {
        let create = match self.create.as_ref() {
            Some(CreateStage::Fill(form)) => form.choices().fields(),
            _ => &[],
        };
        let detail = self
            .detail
            .as_ref()
            .map(|detail| detail.choices.fields())
            .unwrap_or_default();
        distinct_sources(create.iter().chain(detail))
    }

    pub(super) fn start_collection_options(&mut self, cx: &mut Context<Self>) {
        self.refresh_collection_choices();
        let sources = self.collection_sources();
        self._collection_watch = watch_collection_revisions(
            &self.builtins,
            &sources,
            Self::reload_collection_options,
            cx,
        );
        self.reload_collection_options(cx);
    }

    fn reload_collection_options(&mut self, cx: &mut Context<Self>) {
        let sources = self.collection_sources();
        if sources.is_empty() {
            return;
        }
        let builtins = self.builtins.clone();
        async_bridge::run_async(
            &self.rt_handle,
            load_collection_options(builtins, sources),
            Self::apply_collection_options,
            cx,
        );
    }

    fn apply_collection_options(&mut self, options: ChoiceOptions, cx: &mut Context<Self>) {
        self.collection_options.extend(options);
        self.refresh_collection_choices();
        cx.notify();
    }

    fn refresh_collection_choices(&mut self) {
        let blank = tr!("config_form_choice_any");
        if let Some(CreateStage::Fill(form)) = self.create.as_mut() {
            form.refresh_choices(&self.collection_options, &blank);
        }
        if let Some(detail) = self.detail.as_mut() {
            detail
                .choices
                .refresh(&mut detail.fields, &self.collection_options, Some(&blank));
        }
    }
}
