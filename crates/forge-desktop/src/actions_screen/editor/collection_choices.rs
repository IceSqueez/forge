use gpui::{App, Context};

use super::{ActionEditor, AddTriggerStage};
use crate::async_bridge;
use crate::option_choices::{ChoiceOptions, distinct_keys, resolve_options, watch_option_changes};

impl ActionEditor {
    fn option_keys(&self, cx: &App) -> Vec<String> {
        let trigger_fill = match self.modals.add_trigger.as_ref() {
            Some(AddTriggerStage::Fill(host)) => host.form.read(cx).option_keys(),
            _ => Vec::new(),
        };
        distinct_keys(self.choices.option_keys.iter().chain(&trigger_fill))
    }

    pub(super) fn start_option_choices(&mut self, cx: &mut Context<Self>) {
        self.refresh_trigger_fill_choices(cx);
        self.choices._option_watch = watch_option_changes(
            &self.choices.builtins,
            self.option_keys(cx),
            Self::reload_option_choices,
            cx,
        );
        self.reload_option_choices(cx);
    }

    fn reload_option_choices(&mut self, cx: &mut Context<Self>) {
        let keys = self.option_keys(cx);
        if keys.is_empty() {
            return;
        }
        let builtins = self.choices.builtins.clone();
        let ticket = self.choices.option_choices_gen.next();
        async_bridge::run_async(
            &self.deps.rt_handle,
            resolve_options(builtins, keys),
            move |this, options, cx| {
                if this.choices.option_choices_gen.is_current(ticket) {
                    this.apply_option_choices(options, cx);
                }
            },
            cx,
        );
    }

    fn apply_option_choices(&mut self, options: ChoiceOptions, cx: &mut Context<Self>) {
        self.choices.integration_options.extend(options);
        self.choices
            .select_options
            .extend(self.choices.integration_options.clone());
        self.refresh_trigger_fill_choices(cx);
        if let Some(form) = self.modals.sub_form.clone() {
            let merged = self.choices.select_options.clone();
            form.update(cx, |form, cx| form.apply_options(&merged, cx));
        }
        cx.notify();
    }

    fn refresh_trigger_fill_choices(&self, cx: &mut Context<Self>) {
        if let Some(AddTriggerStage::Fill(host)) = self.modals.add_trigger.as_ref() {
            let options = &self.choices.integration_options;
            host.form
                .update(cx, |form, cx| form.refresh_choices(options, cx));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use forge_platform_core::{CollectionItem, CollectionOutcome};
    use forge_storage::MockMediaRepo;
    use gpui::Entity;

    use crate::actions_screen::editor::ActionEditor;
    use crate::actions_screen::editor::tests::{screen_parts, view_with};
    use crate::test_support::{FakeCollections, StubActions, owned_reward, pump_until, runtime};

    use forge_registry::{FormField, SubActionRegistry};

    use crate::option_choices::names_integration_data;

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

    #[test]
    fn every_obs_and_vtube_sub_action_picker_reaches_the_integration_resolver() {
        let reg = integration_sub_actions();
        let mut declared = Vec::new();
        for runner in reg.all() {
            for field in runner.config_fields() {
                dynamic_select_keys(&field, &mut declared, runner.id());
            }
        }

        assert!(!declared.is_empty());
        let internal_or_unserved: Vec<&(String, &str)> = declared
            .iter()
            .filter(|(_, key)| !key.starts_with("forge.") && !names_integration_data(key))
            .filter(|(_, key)| forge_registry::InternalNamespace::from_options_key(key).is_none())
            .collect();
        assert_eq!(internal_or_unserved, Vec::<&(String, &str)>::new());
    }

    const REWARD_OPTIONS: &str = "collections.twitch.rewards";

    fn listing_of(title: &str) -> CollectionOutcome<Vec<CollectionItem>> {
        Ok(vec![owned_reward(title, title)])
    }

    fn offered_titles(
        view: &Entity<ActionEditor>,
        cx: &mut gpui::TestAppContext,
    ) -> Option<Vec<String>> {
        view.read_with(cx, |view, _| {
            view.choices
                .integration_options
                .get(REWARD_OPTIONS)
                .map(|choices| {
                    choices
                        .iter()
                        .map(|option| option.label.clone())
                        .collect::<Vec<_>>()
                })
        })
    }

    #[gpui::test]
    fn a_reload_that_finishes_after_a_newer_one_does_not_replace_its_options(
        cx: &mut gpui::TestAppContext,
    ) {
        let rt = runtime();
        let fake = FakeCollections::listing(Ok(Vec::new()));
        let older = fake.hold_next_listing(listing_of("before the rename"));
        let newer = fake.hold_next_listing(listing_of("after the rename"));
        let mut media = MockMediaRepo::new();
        media.expect_blob_of().returning(|_| Ok(None));
        let host = view_with(
            cx,
            &rt,
            Vec::new(),
            media,
            SubActionRegistry::new(),
            Arc::new(StubActions),
        );
        let view = screen_parts(cx, &host).0;
        let builtins = fake.installed_as("twitch");
        view.update(cx, |view, cx| {
            view.choices.builtins = builtins;
            view.choices.option_keys = vec![REWARD_OPTIONS.to_owned()];
            view.reload_option_choices(cx);
            view.reload_option_choices(cx);
        });
        pump_until(&rt, || fake.list_calls() == 2);

        newer.send(()).expect("the newer listing is waiting");
        pump_until(&rt, || {
            cx.run_until_parked();
            offered_titles(&view, cx).is_some()
        });
        older.send(()).expect("the older listing is waiting");
        pump_until(&rt, || fake.released_listings() == 2);
        cx.run_until_parked();

        assert_eq!(
            offered_titles(&view, cx),
            Some(vec!["after the rename".to_owned()])
        );
    }
}
