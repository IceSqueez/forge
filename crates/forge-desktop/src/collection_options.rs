use std::collections::HashMap;

use forge_platform_core::{
    BuiltinId, CollectionId, CollectionItem, CollectionItemAccess, CollectionRevisions,
    RevisionWait,
};
use forge_registry::FormField;
use gpui::{Context, Task};

use crate::integrations::BuiltinRegistry;

const OPTIONS_PREFIX: &str = "collections.";
const MANAGEABLE_SUFFIX: &str = ".manageable";

pub(crate) type ChoiceOptions = HashMap<String, Vec<(String, String)>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CollectionSource {
    pub(crate) options_key: String,
    pub(crate) builtin: BuiltinId,
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
            builtin: BuiltinId::new(builtin),
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
    let mut listed: Vec<((BuiltinId, CollectionId), Vec<CollectionItem>)> = Vec::new();
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
    let mut watched: Vec<&BuiltinId> = Vec::new();
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
