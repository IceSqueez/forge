use forge_components::tr;
use gpui::Context;

use super::TriggersRegistryView;
use super::create::CreateStage;
use crate::async_bridge;
use crate::option_choices::{ChoiceOptions, distinct_keys, resolve_options, watch_option_changes};
use forge_host::integrations::BuiltinRegistry;

impl TriggersRegistryView {
    #[must_use]
    pub(crate) fn with_builtins(mut self, builtins: BuiltinRegistry) -> Self {
        self.builtins = builtins;
        self
    }

    fn option_keys(&self) -> Vec<String> {
        let create = match self.create.as_ref() {
            Some(CreateStage::Fill(form)) => form.choices().option_keys(),
            _ => Vec::new(),
        };
        let detail = self
            .detail
            .as_ref()
            .map(|detail| detail.choices.option_keys())
            .unwrap_or_default();
        distinct_keys(create.iter().chain(&detail))
    }

    pub(super) fn start_option_choices(&mut self, cx: &mut Context<Self>) {
        self.refresh_option_choices();
        self._option_watch = watch_option_changes(
            &self.builtins,
            self.option_keys(),
            Self::reload_option_choices,
            cx,
        );
        self.reload_option_choices(cx);
    }

    fn reload_option_choices(&mut self, cx: &mut Context<Self>) {
        let keys = self.option_keys();
        if keys.is_empty() {
            return;
        }
        let builtins = self.builtins.clone();
        let ticket = self.option_choices_gen.next();
        async_bridge::run_async(
            &self.rt_handle,
            resolve_options(builtins, keys),
            move |this, options, cx| {
                if this.option_choices_gen.is_current(ticket) {
                    this.apply_option_choices(options, cx);
                }
            },
            cx,
        );
    }

    fn apply_option_choices(&mut self, options: ChoiceOptions, cx: &mut Context<Self>) {
        self.integration_options.extend(options);
        self.refresh_option_choices();
        cx.notify();
    }

    fn refresh_option_choices(&mut self) {
        let blank = tr!("config_form_choice_any");
        if let Some(CreateStage::Fill(form)) = self.create.as_mut() {
            form.refresh_choices(&self.integration_options, &blank);
        }
        if let Some(detail) = self.detail.as_mut() {
            detail
                .choices
                .refresh(&mut detail.fields, &self.integration_options, Some(&blank));
        }
    }
}
