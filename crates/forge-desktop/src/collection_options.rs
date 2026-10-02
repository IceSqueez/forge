use std::collections::HashMap;

use forge_platform_core::{
    CollectionId, CollectionItem, CollectionItemAccess, CollectionRevisions, RevisionWait,
};
use forge_registry::FormField;
use forge_types::IntegrationId;
use gpui::{Context, Task};

use crate::integrations::BuiltinRegistry;

const OPTIONS_PREFIX: &str = "collections.";
const MANAGEABLE_SUFFIX: &str = ".manageable";

pub(crate) type ChoiceOptions = HashMap<String, Vec<(String, String)>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CollectionSource {
    pub(crate) options_key: String,
    pub(crate) builtin: IntegrationId,
    pub(crate) collection: CollectionId,
    pub(crate) manageable_only: bool,
}

impl CollectionSource {
    pub(crate) fn parse(options_key: &str) -> Option<Self> {
        let scoped = options_key.strip_prefix(OPTIONS_PREFIX)?;
        let (scoped, manageable_only) = match scoped.strip_suffix(MANAGEABLE_SUFFIX) {
            Some(narrowed) => (narrowed, true),
            None => (scoped, false),
        };
        let (builtin, collection) = scoped.split_once('.')?;
        if builtin.is_empty() || collection.is_empty() || collection.contains('.') {
            return None;
        }
        Some(Self {
            options_key: options_key.to_owned(),
            builtin: IntegrationId::new(builtin),
            collection: CollectionId::new(collection),
            manageable_only,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CollectionChoiceField {
    pub(crate) field_key: String,
    pub(crate) source: CollectionSource,
}

pub(crate) fn collection_choice_fields(specs: &[FormField]) -> Vec<CollectionChoiceField> {
    let mut out = Vec::new();
    for spec in specs {
        push_choice_field(spec, &mut out);
    }
    out
}

fn push_choice_field(spec: &FormField, out: &mut Vec<CollectionChoiceField>) {
    match spec {
        FormField::DynamicSelect {
            key, options_key, ..
        } => {
            if let Some(source) = CollectionSource::parse(options_key) {
                out.push(CollectionChoiceField {
                    field_key: (*key).to_owned(),
                    source,
                });
            }
        }
        FormField::Optional { inner, .. } => push_choice_field(inner, out),
        _ => {}
    }
}

pub(crate) fn distinct_sources<'a>(
    fields: impl IntoIterator<Item = &'a CollectionChoiceField>,
) -> Vec<CollectionSource> {
    let mut sources: Vec<CollectionSource> = Vec::new();
    for field in fields {
        if !sources.contains(&field.source) {
            sources.push(field.source.clone());
        }
    }
    sources
}

pub(crate) fn collection_choices(
    items: &[CollectionItem],
    manageable_only: bool,
) -> Vec<(String, String)> {
    items
        .iter()
        .filter(|item| !manageable_only || item.access == CollectionItemAccess::Manageable)
        .map(|item| (item.id.as_str().to_owned(), item.title.clone()))
        .collect()
}

pub(crate) async fn load_collection_options(
    builtins: BuiltinRegistry,
    sources: Vec<CollectionSource>,
) -> ChoiceOptions {
    let mut listed: Vec<((IntegrationId, CollectionId), Vec<CollectionItem>)> = Vec::new();
    let mut options = ChoiceOptions::new();
    for source in sources {
        let owner = (source.builtin.clone(), source.collection.clone());
        let cached = listed
            .iter()
            .position(|(listed_owner, _)| *listed_owner == owner);
        let index = match cached {
            Some(index) => index,
            None => {
                let items = list_items(&builtins, &source).await;
                listed.push((owner, items));
                listed.len() - 1
            }
        };
        let items = listed
            .get(index)
            .map(|(_, items)| items.as_slice())
            .unwrap_or_default();
        options.insert(
            source.options_key.clone(),
            collection_choices(items, source.manageable_only),
        );
    }
    options
}

async fn list_items(builtins: &BuiltinRegistry, source: &CollectionSource) -> Vec<CollectionItem> {
    let Some(capability) = builtins
        .get(&source.builtin)
        .and_then(|object| object.collections)
    else {
        return Vec::new();
    };
    match capability.list(&source.collection).await {
        Ok(items) => items,
        Err(failure) => {
            tracing::debug!(
                builtin = %source.builtin.as_str(),
                collection = %source.collection,
                %failure,
                "collection options unavailable"
            );
            Vec::new()
        }
    }
}

pub(crate) fn watch_collection_revisions<V: 'static>(
    builtins: &BuiltinRegistry,
    sources: &[CollectionSource],
    on_change: fn(&mut V, &mut Context<V>),
    cx: &mut Context<V>,
) -> Vec<Task<()>> {
    let mut watched: Vec<&IntegrationId> = Vec::new();
    let mut tasks = Vec::new();
    for source in sources {
        if watched.contains(&&source.builtin) {
            continue;
        }
        watched.push(&source.builtin);
        let Some(revisions) = builtins
            .get(&source.builtin)
            .and_then(|object| object.collections)
            .map(|capability| capability.revisions())
        else {
            continue;
        };
        tasks.push(watch_revisions(revisions, on_change, cx));
    }
    tasks
}

fn watch_revisions<V: 'static>(
    mut revisions: CollectionRevisions,
    on_change: fn(&mut V, &mut Context<V>),
    cx: &mut Context<V>,
) -> Task<()> {
    cx.spawn(async move |this, cx| {
        while revisions.changed().await == RevisionWait::Changed {
            if this.update(cx, on_change).is_err() {
                break;
            }
        }
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use forge_platform_core::CollectionFailure;
    use gpui::{AppContext, TestAppContext};

    use super::*;
    use crate::test_support::{FakeCollections, foreign_reward, owned_reward, runtime};

    const REWARD_KEY: &str = "collections.twitch.rewards";
    const OWNED_REWARD_KEY: &str = "collections.twitch.rewards.manageable";

    fn titles(choices: &[(String, String)]) -> Vec<&str> {
        choices.iter().map(|(_, title)| title.as_str()).collect()
    }

    fn mixed_rewards() -> Vec<CollectionItem> {
        vec![
            owned_reward("r-owned", "Hydrate"),
            foreign_reward("r-foreign", "Highlight my message"),
        ]
    }

    #[test]
    fn an_options_key_names_its_builtin_collection_and_whether_only_owned_rows_count() {
        for (key, builtin, collection, manageable_only) in [
            (REWARD_KEY, "twitch", "rewards", false),
            (OWNED_REWARD_KEY, "twitch", "rewards", true),
            ("collections.kick.rewards", "kick", "rewards", false),
        ] {
            let source = CollectionSource::parse(key).unwrap();

            assert_eq!(
                (
                    source.builtin.as_str(),
                    source.collection.to_string(),
                    source.manageable_only,
                    source.options_key.as_str(),
                ),
                (builtin, collection.to_owned(), manageable_only, key),
                "{key}"
            );
        }
    }

    #[test]
    fn a_key_outside_the_collections_namespace_or_malformed_is_not_a_collection_source() {
        for key in [
            "obs.scene_names",
            "action.ids",
            "collections.twitch",
            "collections.twitch.",
            "collections..rewards",
            "collections.twitch.rewards.extra",
            "collections.twitch.manageable",
            "collections.",
            "",
        ] {
            assert_eq!(CollectionSource::parse(key), None, "{key:?}");
        }
    }

    #[test]
    fn only_collection_backed_selects_are_picked_up_even_when_wrapped_optional() {
        let specs = vec![
            FormField::DynamicSelect {
                key: "reward_id",
                label: "Reward",
                options_key: REWARD_KEY,
            },
            FormField::DynamicSelect {
                key: "scene",
                label: "Scene",
                options_key: "obs.scene_names",
            },
            FormField::Text {
                key: "reward_title",
                label: "Title",
                placeholder: "",
            },
            FormField::Optional {
                key: "target",
                label: "Target",
                inner: Box::new(FormField::DynamicSelect {
                    key: "target",
                    label: "Target",
                    options_key: OWNED_REWARD_KEY,
                }),
            },
        ];

        let picked: Vec<(String, bool)> = collection_choice_fields(&specs)
            .into_iter()
            .map(|field| (field.field_key, field.source.manageable_only))
            .collect();

        assert_eq!(
            picked,
            vec![("reward_id".to_owned(), false), ("target".to_owned(), true)]
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
            vec![("r-owned".to_owned(), "Hydrate".to_owned())]
        );
    }

    fn sources(keys: &[&str]) -> Vec<CollectionSource> {
        keys.iter()
            .map(|key| CollectionSource::parse(key).unwrap())
            .collect()
    }

    #[test]
    fn plain_and_owned_keys_over_one_collection_fill_from_a_single_listing() {
        let fake = FakeCollections::listing(Ok(mixed_rewards()));
        let builtins = fake.installed_as("twitch");
        let rt = runtime();

        let options = rt.block_on(load_collection_options(
            builtins,
            sources(&[REWARD_KEY, OWNED_REWARD_KEY]),
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
            let rt = runtime();

            let options = rt.block_on(load_collection_options(
                fake.installed_as("twitch"),
                sources(&[REWARD_KEY]),
            ));

            assert_eq!(
                options.get(REWARD_KEY),
                Some(&Vec::new()),
                "{failure:?} must replace stale rewards with an empty list"
            );
        }
    }

    #[test]
    fn a_builtin_that_is_not_installed_offers_an_empty_list() {
        let fake = FakeCollections::listing(Ok(mixed_rewards()));
        let rt = runtime();

        let options = rt.block_on(load_collection_options(
            fake.installed_as("kick"),
            sources(&[REWARD_KEY]),
        ));

        assert_eq!(options.get(REWARD_KEY), Some(&Vec::new()));
    }

    struct Watcher {
        reloads: usize,
        _watch: Vec<Task<()>>,
    }

    fn count_reload(watcher: &mut Watcher, _: &mut Context<Watcher>) {
        watcher.reloads += 1;
    }

    #[gpui::test]
    fn a_revision_bump_reloads_once_even_when_two_fields_share_the_builtin(
        cx: &mut TestAppContext,
    ) {
        let fake = FakeCollections::listing(Ok(Vec::new()));
        let builtins = fake.installed_as("twitch");
        let watcher = cx.update(|cx| {
            cx.new(|cx| Watcher {
                reloads: 0,
                _watch: watch_collection_revisions(
                    &builtins,
                    &sources(&[REWARD_KEY, OWNED_REWARD_KEY]),
                    count_reload,
                    cx,
                ),
            })
        });
        cx.run_until_parked();

        fake.bump();
        cx.run_until_parked();

        assert_eq!(cx.update(|cx| watcher.read(cx).reloads), 1);
    }
}
