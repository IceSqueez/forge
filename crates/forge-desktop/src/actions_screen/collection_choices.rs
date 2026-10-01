use forge_components::tr;
use gpui::Context;

use super::{AddTriggerStage, ScreenActionsView};
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

    fn obs_catalog_fields(&self) -> Vec<ObsCatalogField> {
        let trigger_fill = match self.add_trigger.as_ref() {
            Some(AddTriggerStage::Fill(form)) => form.choices.obs_fields(),
            _ => Vec::new(),
        };
        distinct_obs_fields(self.sub_form_obs_fields.iter().chain(&trigger_fill))
    }

    fn vtube_catalog_fields(&self) -> Vec<VTubeCatalogField> {
        let trigger_fill = match self.add_trigger.as_ref() {
            Some(AddTriggerStage::Fill(form)) => form.choices.vtube_fields(),
            _ => Vec::new(),
        };
        distinct_vtube_fields(self.sub_form_vtube_fields.iter().chain(&trigger_fill))
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
        if !self.obs_catalog_fields().is_empty() {
            self._collection_watch.push(watch_obs_catalog(
                &self.bus,
                Self::reload_obs_catalog_options,
                cx,
            ));
        }
        if !self.vtube_catalog_fields().is_empty() {
            self._collection_watch.push(watch_vtube_catalog(
                &self.bus,
                self.builtins.clone(),
                Self::reload_vtube_catalog_options,
                cx,
            ));
        }
        self.reload_collection_options(cx);
        self.reload_obs_catalog_options(cx);
        self.reload_vtube_catalog_options(cx);
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use forge_registry::{FormField, SubActionRegistry};

    use crate::collection_options::collection_choice_fields;
    use crate::obs_catalog_options::obs_catalog_fields;
    use crate::vtube_catalog_options::vtube_catalog_fields;

    fn integration_sub_actions() -> SubActionRegistry {
        let mut reg = SubActionRegistry::new();
        forge_obs::register_obs_sub_actions(&mut reg, forge_obs::SwitchableObsSink::new()).unwrap();
        forge_vtube::register_vtube_sub_actions(&mut reg, forge_vtube::SwitchableVTubeSink::new())
            .unwrap();
        reg
    }

    fn dynamic_select_keys(field: &FormField, out: &mut Vec<(String, &'static str)>, id: &str) {
        match field {
            FormField::DynamicSelect { options_key, .. } => out.push((id.to_owned(), options_key)),
            FormField::Optional { inner, .. } => dynamic_select_keys(inner, out, id),
            _ => {}
        }
    }

    fn served(field: &FormField) -> bool {
        let specs = std::slice::from_ref(field);
        !obs_catalog_fields(specs).is_empty()
            || !vtube_catalog_fields(specs).is_empty()
            || !collection_choice_fields(specs).is_empty()
    }

    #[test]
    fn every_obs_and_vtube_sub_action_picker_has_an_options_provider() {
        let reg = integration_sub_actions();
        let mut declared = Vec::new();
        let mut unserved = Vec::new();
        for runner in reg.all() {
            for field in runner.config_fields() {
                let before = declared.len();
                dynamic_select_keys(&field, &mut declared, runner.id());
                if declared.len() > before && !served(&field) {
                    unserved.extend(declared[before..].iter().cloned());
                }
            }
        }

        assert!(!declared.is_empty());
        assert_eq!(unserved, Vec::<(String, &str)>::new());
    }
}
