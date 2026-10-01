use std::sync::Arc;

use forge_components::tr;
use forge_runtime::EventBus;
use gpui::Context;

use super::TriggersRegistryView;
use super::create::CreateStage;
use crate::async_bridge;
use crate::collection_options::{
    ChoiceOptions, CollectionSource, distinct_sources, load_collection_options,
    watch_collection_revisions,
};
use crate::integrations::BuiltinRegistry;
use crate::obs_catalog_options::{
    ObsCatalogField, distinct_obs_fields, live_obs_client, load_obs_catalog_options,
    watch_obs_catalog,
};
use crate::vtube_catalog_options::{
    VTubeCatalogField, current_vtube_catalog_options, distinct_vtube_fields, live_vtube_client,
    watch_vtube_catalog,
};

impl TriggersRegistryView {
    #[must_use]
    pub fn with_builtins(mut self, builtins: BuiltinRegistry) -> Self {
        self.builtins = builtins;
        self
    }

    #[must_use]
    pub fn with_event_bus(mut self, bus: Arc<EventBus>) -> Self {
        self.bus = Some(bus);
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

    fn obs_catalog_fields(&self) -> Vec<ObsCatalogField> {
        let create = match self.create.as_ref() {
            Some(CreateStage::Fill(form)) => form.choices().obs_fields(),
            _ => Vec::new(),
        };
        let detail = self
            .detail
            .as_ref()
            .map(|detail| detail.choices.obs_fields())
            .unwrap_or_default();
        distinct_obs_fields(create.iter().chain(&detail))
    }

    fn vtube_catalog_fields(&self) -> Vec<VTubeCatalogField> {
        let create = match self.create.as_ref() {
            Some(CreateStage::Fill(form)) => form.choices().vtube_fields(),
            _ => Vec::new(),
        };
        let detail = self
            .detail
            .as_ref()
            .map(|detail| detail.choices.vtube_fields())
            .unwrap_or_default();
        distinct_vtube_fields(create.iter().chain(&detail))
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
        if let Some(bus) = self.bus.clone() {
            if !self.obs_catalog_fields().is_empty() {
                self._collection_watch.push(watch_obs_catalog(
                    &bus,
                    Self::reload_obs_catalog_options,
                    cx,
                ));
            }
            if !self.vtube_catalog_fields().is_empty() {
                self._collection_watch.push(watch_vtube_catalog(
                    &bus,
                    self.builtins.clone(),
                    Self::reload_vtube_catalog_options,
                    cx,
                ));
            }
        }
        self.reload_collection_options(cx);
        self.reload_obs_catalog_options(cx);
        self.reload_vtube_catalog_options(cx);
    }

    fn reload_obs_catalog_options(&mut self, cx: &mut Context<Self>) {
        let fields = self.obs_catalog_fields();
        if fields.is_empty() {
            return;
        }
        let client = live_obs_client(&self.builtins);
        async_bridge::run_async(
            &self.rt_handle,
            load_obs_catalog_options(client, fields),
            Self::apply_collection_options,
            cx,
        );
    }

    fn reload_vtube_catalog_options(&mut self, cx: &mut Context<Self>) {
        let fields = self.vtube_catalog_fields();
        if fields.is_empty() {
            return;
        }
        let client = live_vtube_client(&self.builtins);
        let options = current_vtube_catalog_options(client.as_deref(), &fields);
        self.apply_collection_options(options, cx);
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
