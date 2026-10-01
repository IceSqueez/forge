use forge_components::tr;
use gpui::Context;

use super::{AddTriggerStage, ScreenActionsView};
use crate::async_bridge;
use crate::collection_options::{
    ChoiceOptions, CollectionSource, distinct_sources, load_collection_options,
    watch_collection_revisions,
};
use crate::integrations::BuiltinRegistry;
use crate::obs_catalog_options::{live_obs_client, load_obs_catalog_options, watch_obs_catalog};

impl ScreenActionsView {
    #[must_use]
    pub fn with_builtins(mut self, builtins: BuiltinRegistry) -> Self {
        self.builtins = builtins;
        self
    }

    fn collection_sources(&self) -> Vec<CollectionSource> {
        let trigger_fill = match self.add_trigger.as_ref() {
            Some(AddTriggerStage::Fill(form)) => form.choices.fields(),
            _ => &[],
        };
        distinct_sources(self.sub_form_choice_fields.iter().chain(trigger_fill))
    }

    pub(super) fn start_collection_options(&mut self, cx: &mut Context<Self>) {
        self.refresh_trigger_fill_choices();
        let sources = self.collection_sources();
        self._collection_watch = watch_collection_revisions(
            &self.builtins,
            &sources,
            Self::reload_collection_options,
            cx,
        );
        if !self.sub_form_obs_fields.is_empty() {
            self._collection_watch.push(watch_obs_catalog(
                &self.bus,
                Self::reload_obs_catalog_options,
                cx,
            ));
        }
        self.reload_collection_options(cx);
        self.reload_obs_catalog_options(cx);
    }

    fn reload_obs_catalog_options(&mut self, cx: &mut Context<Self>) {
        if self.sub_form_obs_fields.is_empty() {
            return;
        }
        let client = live_obs_client(&self.builtins);
        let fields = self.sub_form_obs_fields.clone();
        async_bridge::run_async(
            &self.rt_handle,
            load_obs_catalog_options(client, fields),
            Self::apply_collection_options,
            cx,
        );
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
        self.select_options.extend(self.collection_options.clone());
        self.refresh_trigger_fill_choices();
        if let Some(form) = self.sub_form.clone() {
            let merged = self.select_options.clone();
            form.update(cx, |form, cx| form.apply_options(&merged, cx));
        }
        cx.notify();
    }

    fn refresh_trigger_fill_choices(&mut self) {
        if let Some(AddTriggerStage::Fill(form)) = self.add_trigger.as_mut() {
            let blank = tr!("config_form_choice_any");
            form.choices
                .refresh(&mut form.fields, &self.collection_options, Some(&blank));
        }
    }
}
