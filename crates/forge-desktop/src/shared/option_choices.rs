use std::collections::{BTreeMap, HashMap};
use std::pin::pin;
use std::sync::Arc;

use forge_host::integrations::{BuiltinObject, BuiltinRegistry};
use forge_platform_core::{
    BuiltinCollections, BuiltinOptionSource, CollectionId, CollectionItem, CollectionItemAccess,
    CollectionRevisions, OptionListId, RevisionWait,
};
use forge_registry::{FormField, ReferenceNamespace, ReferenceScope};
use forge_runtime::{DONATION_PROVIDER_OPTIONS_KEY, ENGINE_OPTIONS_KEY, VOICE_OPTIONS_PREFIX};
use forge_speech_types::ENGINE_VOICE_OPTIONS_KEY;
use forge_types::IntegrationId;
use futures_util::future::{self, Either, join, join_all};
use futures_util::stream::{self, LocalBoxStream, SelectAll, StreamExt as _};
use gpui::{Context, Task};

const COLLECTION_KEY_PREFIX: &str = "collections";
const MANAGEABLE_SUFFIX: &str = "manageable";

const APP_FILLED_OPTION_KEYS: [&str; 4] = [
    ENGINE_OPTIONS_KEY,
    VOICE_OPTIONS_PREFIX,
    ENGINE_VOICE_OPTIONS_KEY,
    DONATION_PROVIDER_OPTIONS_KEY,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChoiceOption {
    pub(crate) value: String,
    pub(crate) label: String,
}

impl ChoiceOption {
    pub(crate) fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}

pub(crate) type ChoiceOptions = HashMap<String, Vec<ChoiceOption>>;

pub(crate) fn names_integration_data(options_key: &str) -> bool {
    ReferenceNamespace::for_options_key(options_key).scope() == ReferenceScope::External
        && !APP_FILLED_OPTION_KEYS.contains(&options_key)
}

pub(crate) fn integration_option_keys(specs: &[FormField]) -> Vec<String> {
    let mut out = Vec::new();
    for spec in specs {
        push_option_key(spec, &mut out);
    }
    out
}

fn push_option_key(spec: &FormField, out: &mut Vec<String>) {
    match spec {
        FormField::DynamicSelect { options_key, .. } => {
            if names_integration_data(options_key) && !out.iter().any(|key| key == options_key) {
                out.push((*options_key).to_owned());
            }
        }
        FormField::Optional { inner, .. } => push_option_key(inner, out),
        _ => {}
    }
}

pub(crate) fn distinct_keys<'a>(keys: impl IntoIterator<Item = &'a String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for key in keys {
        if !out.contains(key) {
            out.push(key.clone());
        }
    }
    out
}

fn collection_key(integration: &IntegrationId, collection: &CollectionId) -> String {
    format!("{COLLECTION_KEY_PREFIX}.{integration}.{collection}")
}

fn narrowing_of(object: &BuiltinObject, key: &str) -> Option<(CollectionId, bool)> {
    let capability = object.collections.as_ref()?;
    let integration = object.status.id();
    capability.collections().into_iter().find_map(|metadata| {
        let plain = collection_key(integration, &metadata.id);
        if key == plain {
            return Some((metadata.id, false));
        }
        (key == format!("{plain}.{MANAGEABLE_SUFFIX}")).then_some((metadata.id, true))
    })
}

fn declares_list(source: &dyn BuiltinOptionSource, key: &str) -> bool {
    source
        .option_lists()
        .iter()
        .any(|list| list.id.as_str() == key)
}

struct OptionListRead {
    options_key: String,
    source: Arc<dyn BuiltinOptionSource>,
}

impl OptionListRead {
    async fn choices(&self) -> (String, Vec<ChoiceOption>) {
        let entries = self
            .source
            .resolve(&OptionListId::new(&*self.options_key), &BTreeMap::new())
            .await
            .unwrap_or_else(|failure| {
                tracing::debug!(key = %self.options_key, %failure, "options unavailable");
                Vec::new()
            });
        let choices = entries
            .into_iter()
            .map(|entry| ChoiceOption::new(entry.value, entry.label))
            .collect();
        (self.options_key.clone(), choices)
    }
}

struct CollectionListing {
    integration: IntegrationId,
    collection: CollectionId,
    capability: Arc<dyn BuiltinCollections>,
}

impl CollectionListing {
    async fn items(&self) -> Vec<CollectionItem> {
        self.capability
            .list(&self.collection)
            .await
            .unwrap_or_else(|failure| {
                tracing::debug!(
                    integration = %self.integration,
                    collection = %self.collection,
                    %failure,
                    "collection options unavailable"
                );
                Vec::new()
            })
    }
}

struct CollectionKey {
    options_key: String,
    listing: usize,
    manageable_only: bool,
}

#[derive(Default)]
struct ResolvePlan {
    lists: Vec<OptionListRead>,
    listings: Vec<CollectionListing>,
    collection_keys: Vec<CollectionKey>,
    unclaimed: Vec<String>,
}

impl ResolvePlan {
    fn for_keys(objects: &[BuiltinObject], keys: Vec<String>) -> Self {
        let mut plan = Self::default();
        for key in keys {
            plan.claim(objects, key);
        }
        plan
    }

    fn claim(&mut self, objects: &[BuiltinObject], key: String) {
        let list_source = objects
            .iter()
            .filter_map(|object| object.option_source.as_ref())
            .find(|source| declares_list(source.as_ref(), &key));
        if let Some(source) = list_source {
            self.lists.push(OptionListRead {
                options_key: key,
                source: Arc::clone(source),
            });
            return;
        }
        let collection = objects.iter().find_map(|object| {
            let (collection, manageable_only) = narrowing_of(object, &key)?;
            Some((object, collection, manageable_only))
        });
        match collection {
            Some((object, collection, manageable_only)) => {
                let listing = self.listing_index(object, collection);
                self.collection_keys.push(CollectionKey {
                    options_key: key,
                    listing,
                    manageable_only,
                });
            }
            None => self.unclaimed.push(key),
        }
    }

    fn listing_index(&mut self, object: &BuiltinObject, collection: CollectionId) -> usize {
        let integration = object.status.id();
        let known = self.listings.iter().position(|listing| {
            listing.integration == *integration && listing.collection == collection
        });
        if let Some(index) = known {
            return index;
        }
        if let Some(capability) = &object.collections {
            self.listings.push(CollectionListing {
                integration: integration.clone(),
                collection,
                capability: Arc::clone(capability),
            });
        }
        self.listings.len().saturating_sub(1)
    }
}

pub(crate) fn collection_choices(
    items: &[CollectionItem],
    manageable_only: bool,
) -> Vec<ChoiceOption> {
    items
        .iter()
        .filter(|item| !manageable_only || item.access == CollectionItemAccess::Manageable)
        .map(|item| ChoiceOption::new(item.id.as_str(), item.title.clone()))
        .collect()
}

pub(crate) async fn resolve_options(builtins: BuiltinRegistry, keys: Vec<String>) -> ChoiceOptions {
    let plan = ResolvePlan::for_keys(&builtins.snapshot(), keys);
    let (read_lists, listed) = join(
        join_all(plan.lists.iter().map(OptionListRead::choices)),
        join_all(plan.listings.iter().map(CollectionListing::items)),
    )
    .await;
    let mut options: ChoiceOptions = read_lists.into_iter().collect();
    options.extend(plan.unclaimed.into_iter().map(|key| (key, Vec::new())));
    for key in plan.collection_keys {
        let items = listed
            .get(key.listing)
            .map(Vec::as_slice)
            .unwrap_or_default();
        options.insert(
            key.options_key,
            collection_choices(items, key.manageable_only),
        );
    }
    options
}

type RevisionSignals = SelectAll<LocalBoxStream<'static, ()>>;

fn revision_signal(revisions: CollectionRevisions) -> LocalBoxStream<'static, ()> {
    stream::unfold(revisions, |mut revisions| async move {
        (revisions.changed().await == RevisionWait::Changed).then_some(((), revisions))
    })
    .boxed_local()
}

fn revision_signals(objects: &[BuiltinObject], keys: &[String]) -> RevisionSignals {
    let mut signals = Vec::new();
    for object in objects {
        if let Some(source) = &object.option_source
            && keys.iter().any(|key| declares_list(source.as_ref(), key))
        {
            signals.push(revision_signal(source.revisions()));
        }
        if let Some(capability) = &object.collections
            && keys.iter().any(|key| narrowing_of(object, key).is_some())
        {
            signals.push(revision_signal(capability.revisions()));
        }
    }
    stream::select_all(signals)
}

async fn next_revision(signals: &mut RevisionSignals) {
    if signals.next().await.is_none() {
        future::pending::<()>().await;
    }
}

pub(crate) fn watch_option_changes<V: 'static>(
    builtins: &BuiltinRegistry,
    keys: Vec<String>,
    on_change: fn(&mut V, &mut Context<V>),
    cx: &mut Context<V>,
) -> Option<Task<()>> {
    if keys.is_empty() {
        return None;
    }
    let builtins = builtins.clone();
    let mut registry = builtins.changes();
    Some(cx.spawn(async move |this, cx| {
        loop {
            let mut signals = revision_signals(&builtins.snapshot(), &keys);
            loop {
                let replaced = pin!(registry.changed());
                let bumped = pin!(next_revision(&mut signals));
                match future::select(replaced, bumped).await {
                    Either::Left((true, _)) => break,
                    Either::Left((false, _)) => return,
                    Either::Right(((), _)) => {
                        if this.update(cx, on_change).is_err() {
                            return;
                        }
                    }
                }
            }
            if this.update(cx, on_change).is_err() {
                return;
            }
        }
    }))
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use forge_platform_core::{
        CollectionFailure, CollectionOutcome, CollectionRevisionSignal, OptionEntry,
        OptionListMetadata, QuickActionFieldValue,
    };
    use gpui::{AppContext, TestAppContext};

    use super::*;
    use crate::integrations::unavailable_builtin::unavailable_builtin;
    use crate::test_support::{FakeCollections, foreign_reward, owned_reward, runtime};

    const REWARD_KEY: &str = "collections.twitch.rewards";
    const OWNED_REWARD_KEY: &str = "collections.twitch.rewards.manageable";
    const SCENES_KEY: &str = "obs.scene_names";

    struct FakeLists {
        key: &'static str,
        resolved: CollectionOutcome<Vec<OptionEntry>>,
        signal: CollectionRevisionSignal,
    }

    impl FakeLists {
        fn offering(key: &'static str, names: &[&str]) -> Arc<Self> {
            let entries = names
                .iter()
                .map(|name| OptionEntry::new(*name, format!("{name} label")))
                .collect();
            Self::resolving(key, Ok(entries))
        }

        fn resolving(
            key: &'static str,
            resolved: CollectionOutcome<Vec<OptionEntry>>,
        ) -> Arc<Self> {
            Arc::new(Self {
                key,
                resolved,
                signal: CollectionRevisionSignal::new(),
            })
        }

        fn object(self: &Arc<Self>, integration: &str) -> BuiltinObject {
            BuiltinObject {
                option_source: Some(Arc::clone(self) as Arc<dyn BuiltinOptionSource>),
                ..unavailable_builtin(&IntegrationId::new(integration))
            }
        }
    }

    #[async_trait]
    impl BuiltinOptionSource for FakeLists {
        fn option_lists(&self) -> Vec<OptionListMetadata> {
            vec![OptionListMetadata::new(OptionListId::new(self.key))]
        }

        fn revisions(&self) -> CollectionRevisions {
            self.signal.subscribe()
        }

        async fn resolve(
            &self,
            _: &OptionListId,
            _: &BTreeMap<String, QuickActionFieldValue>,
        ) -> CollectionOutcome<Vec<OptionEntry>> {
            self.resolved.clone()
        }
    }

    fn installed(objects: Vec<BuiltinObject>) -> BuiltinRegistry {
        let registry = BuiltinRegistry::default();
        for object in objects {
            registry.install(object);
        }
        registry
    }

    fn keys(raw: &[&str]) -> Vec<String> {
        raw.iter().map(|key| (*key).to_owned()).collect()
    }

    fn select(options_key: &'static str) -> FormField {
        FormField::DynamicSelect {
            key: "target",
            label: "Target",
            options_key,
        }
    }

    fn titles(choices: &[ChoiceOption]) -> Vec<&str> {
        choices.iter().map(|option| option.label.as_str()).collect()
    }

    fn mixed_rewards() -> Vec<CollectionItem> {
        vec![
            owned_reward("r-owned", "Hydrate"),
            foreign_reward("r-foreign", "Highlight my message"),
        ]
    }

    #[test]
    fn integration_keys_are_picked_up_once_each_even_when_wrapped_optional() {
        let specs = vec![
            select(SCENES_KEY),
            FormField::Optional {
                key: "target",
                label: "Target",
                inner: Box::new(select(OWNED_REWARD_KEY)),
            },
            select(SCENES_KEY),
            FormField::Text {
                key: "note",
                label: "Note",
                placeholder: "",
            },
        ];

        assert_eq!(
            integration_option_keys(&specs),
            keys(&[SCENES_KEY, OWNED_REWARD_KEY])
        );
    }

    #[test]
    fn keys_the_app_fills_itself_never_reach_the_integration_resolver() {
        let specs: Vec<FormField> = [
            forge_registry::ACTION_OPTIONS_KEY,
            ENGINE_OPTIONS_KEY,
            ENGINE_VOICE_OPTIONS_KEY,
            VOICE_OPTIONS_PREFIX,
            DONATION_PROVIDER_OPTIONS_KEY,
        ]
        .into_iter()
        .map(select)
        .collect();

        assert_eq!(integration_option_keys(&specs), Vec::<String>::new());
    }

    #[test]
    fn a_list_key_resolves_through_whichever_integration_declares_it() {
        let lists = FakeLists::offering(SCENES_KEY, &["Main", "Intro"]);
        let builtins = installed(vec![
            FakeLists::offering("other.list", &["Unrelated"]).object("other"),
            lists.object("not-named-after-the-key"),
        ]);

        let options = runtime().block_on(resolve_options(builtins, keys(&[SCENES_KEY])));

        assert_eq!(
            options[SCENES_KEY],
            [
                ChoiceOption::new("Main", "Main label"),
                ChoiceOption::new("Intro", "Intro label"),
            ]
        );
    }

    #[test]
    fn a_key_nobody_serves_right_now_offers_an_empty_list_instead_of_staying_unset() {
        for builtins in [
            BuiltinRegistry::default(),
            installed(vec![
                FakeLists::resolving(SCENES_KEY, Err(CollectionFailure::NotConnected))
                    .object("obs"),
            ]),
            installed(vec![
                FakeLists::resolving(SCENES_KEY, Err(CollectionFailure::Transport)).object("obs"),
            ]),
        ] {
            let options = runtime().block_on(resolve_options(builtins, keys(&[SCENES_KEY])));

            assert_eq!(options.get(SCENES_KEY), Some(&Vec::new()));
        }
    }

    #[test]
    fn a_key_that_fails_leaves_the_other_keys_of_the_form_filled() {
        let audio_key = "obs.audio_input_names";
        let rewards = FakeCollections::listing(Ok(mixed_rewards()));
        let builtins = rewards.installed_as("twitch");
        builtins.install(
            FakeLists::resolving(SCENES_KEY, Err(CollectionFailure::Transport)).object("obs"),
        );
        builtins.install(FakeLists::offering(audio_key, &["Mic"]).object("audio"));

        let options = runtime().block_on(resolve_options(
            builtins,
            keys(&[SCENES_KEY, audio_key, OWNED_REWARD_KEY]),
        ));

        assert_eq!(
            (
                titles(&options[SCENES_KEY]),
                titles(&options[audio_key]),
                titles(&options[OWNED_REWARD_KEY]),
            ),
            (vec![], vec!["Mic label"], vec!["Hydrate"])
        );
    }

    #[test]
    fn a_failed_listing_leaves_a_sibling_integrations_collection_filled() {
        let failing = FakeCollections::listing(Err(CollectionFailure::Transport));
        let builtins = failing.installed_as("twitch");
        let kick_key = "collections.kick.rewards";
        let kick = FakeCollections::listing(Ok(mixed_rewards()));
        for object in kick.installed_as("kick").snapshot() {
            builtins.install(object);
        }

        let options = runtime().block_on(resolve_options(
            builtins,
            keys(&[REWARD_KEY, kick_key, "collections.kick.rewards.manageable"]),
        ));

        assert_eq!(
            (
                titles(&options[REWARD_KEY]),
                titles(&options[kick_key]),
                titles(&options["collections.kick.rewards.manageable"]),
            ),
            (
                vec![],
                vec!["Hydrate", "Highlight my message"],
                vec!["Hydrate"],
            )
        );
    }

    #[test]
    fn owned_only_choices_drop_rewards_forge_cannot_modify_while_plain_choices_keep_all() {
        let rewards = mixed_rewards();

        assert_eq!(
            (
                titles(&collection_choices(&rewards, true)),
                titles(&collection_choices(&rewards, false)),
            ),
            (vec!["Hydrate"], vec!["Hydrate", "Highlight my message"])
        );
    }

    #[test]
    fn a_choice_stores_the_reward_id_and_shows_its_title() {
        assert_eq!(
            collection_choices(&[owned_reward("r-owned", "Hydrate")], false),
            vec![ChoiceOption::new("r-owned", "Hydrate")]
        );
    }

    #[test]
    fn plain_and_owned_keys_over_one_collection_fill_from_a_single_listing() {
        let fake = FakeCollections::listing(Ok(mixed_rewards()));
        let builtins = fake.installed_as("twitch");

        let options = runtime().block_on(resolve_options(
            builtins,
            keys(&[REWARD_KEY, OWNED_REWARD_KEY]),
        ));

        assert_eq!(
            (
                titles(&options[REWARD_KEY]),
                titles(&options[OWNED_REWARD_KEY]),
                fake.list_calls(),
            ),
            (vec!["Hydrate", "Highlight my message"], vec!["Hydrate"], 1)
        );
    }

    #[test]
    fn a_failed_listing_clears_the_key_instead_of_leaving_it_unset() {
        for failure in [
            CollectionFailure::NotConnected,
            CollectionFailure::NotEligible,
            CollectionFailure::Transport,
        ] {
            let fake = FakeCollections::listing(Err(failure.clone()));

            let options = runtime().block_on(resolve_options(
                fake.installed_as("twitch"),
                keys(&[REWARD_KEY]),
            ));

            assert_eq!(
                options.get(REWARD_KEY),
                Some(&Vec::new()),
                "{failure:?} must replace stale rewards with an empty list"
            );
        }
    }

    #[test]
    fn a_collection_key_of_another_integration_or_a_malformed_one_offers_an_empty_list() {
        for key in [
            REWARD_KEY,
            "collections.kick",
            "collections.kick.rewards.extra",
            "collections.kick.manageable",
            "collections..rewards",
        ] {
            let fake = FakeCollections::listing(Ok(mixed_rewards()));

            let options =
                runtime().block_on(resolve_options(fake.installed_as("kick"), keys(&[key])));

            assert_eq!(options.get(key), Some(&Vec::new()), "{key:?}");
        }
    }

    struct Watcher {
        reloads: usize,
        _watch: Option<Task<()>>,
    }

    fn count_reload(watcher: &mut Watcher, _: &mut Context<Watcher>) {
        watcher.reloads += 1;
    }

    fn watching(
        cx: &mut TestAppContext,
        builtins: &BuiltinRegistry,
        watched: &[&str],
    ) -> gpui::Entity<Watcher> {
        let watcher = cx.update(|cx| {
            cx.new(|cx| Watcher {
                reloads: 0,
                _watch: watch_option_changes(builtins, keys(watched), count_reload, cx),
            })
        });
        cx.run_until_parked();
        watcher
    }

    fn reloads(cx: &mut TestAppContext, watcher: &gpui::Entity<Watcher>) -> usize {
        cx.run_until_parked();
        cx.update(|cx| watcher.read(cx).reloads)
    }

    #[gpui::test]
    fn a_revision_bump_reloads_once_even_when_two_keys_share_the_collection(
        cx: &mut TestAppContext,
    ) {
        let fake = FakeCollections::listing(Ok(Vec::new()));
        let builtins = fake.installed_as("twitch");
        let watcher = watching(cx, &builtins, &[REWARD_KEY, OWNED_REWARD_KEY]);

        fake.bump();

        assert_eq!(reloads(cx, &watcher), 1);
    }

    #[gpui::test]
    fn a_form_without_integration_keys_watches_nothing(cx: &mut TestAppContext) {
        let lists = FakeLists::offering(SCENES_KEY, &[]);
        let builtins = installed(vec![lists.object("obs")]);
        let watcher = watching(cx, &builtins, &[]);

        lists.signal.bump();
        builtins.install(lists.object("obs"));

        assert_eq!(reloads(cx, &watcher), 0);
    }

    #[gpui::test]
    fn a_form_opened_before_the_integration_exists_follows_the_object_once_installed(
        cx: &mut TestAppContext,
    ) {
        let builtins = BuiltinRegistry::default();
        let watcher = watching(cx, &builtins, &[SCENES_KEY]);
        let lists = FakeLists::offering(SCENES_KEY, &[]);

        builtins.install(lists.object("obs"));
        let after_install = reloads(cx, &watcher);
        lists.signal.bump();

        assert_eq!((after_install, reloads(cx, &watcher)), (1, 2));
    }

    #[gpui::test]
    fn a_replaced_object_is_followed_and_the_retired_one_is_not(cx: &mut TestAppContext) {
        let retired = FakeLists::offering(SCENES_KEY, &[]);
        let builtins = installed(vec![retired.object("obs")]);
        let watcher = watching(cx, &builtins, &[SCENES_KEY]);
        let replacement = FakeLists::offering(SCENES_KEY, &[]);

        builtins.install(replacement.object("obs"));
        let after_swap = reloads(cx, &watcher);
        retired.signal.bump();
        let after_retired_bump = reloads(cx, &watcher);
        replacement.signal.bump();

        assert_eq!(
            (after_swap, after_retired_bump, reloads(cx, &watcher)),
            (1, 1, 2)
        );
    }

    #[gpui::test]
    fn a_removed_integration_reloads_the_form_and_stops_signalling_it(cx: &mut TestAppContext) {
        let lists = FakeLists::offering(SCENES_KEY, &[]);
        let object = lists.object("obs");
        let id = object.status.id().clone();
        let builtins = installed(vec![object]);
        let watcher = watching(cx, &builtins, &[SCENES_KEY]);

        builtins.remove(&id);
        let after_removal = reloads(cx, &watcher);
        lists.signal.bump();

        assert_eq!((after_removal, reloads(cx, &watcher)), (1, 1));
    }

    #[gpui::test]
    fn a_collection_bump_the_form_does_not_use_reloads_nothing(cx: &mut TestAppContext) {
        let rewards = FakeCollections::listing(Ok(Vec::new()));
        let builtins = rewards.installed_as("twitch");
        builtins.install(FakeLists::offering(SCENES_KEY, &[]).object("obs"));
        let watcher = watching(cx, &builtins, &[SCENES_KEY]);

        rewards.bump();

        assert_eq!(reloads(cx, &watcher), 0);
    }

    #[gpui::test]
    fn a_bump_of_an_integration_the_form_does_not_use_reloads_nothing(cx: &mut TestAppContext) {
        let unrelated = FakeLists::offering("other.list", &[]);
        let used = FakeLists::offering(SCENES_KEY, &[]);
        let builtins = installed(vec![unrelated.object("other"), used.object("obs")]);
        let watcher = watching(cx, &builtins, &[SCENES_KEY]);

        unrelated.signal.bump();

        assert_eq!(reloads(cx, &watcher), 0);
    }
}
