use std::sync::Arc;

use forge_events::{Event, EventSource};
use forge_obs::{ObsClient, ObsSource};
use forge_platform_core::BuiltinId;
use forge_registry::FormField;
use forge_runtime::EventBus;
use gpui::{Context, Task};

use crate::async_bridge;
use crate::collection_options::ChoiceOptions;
use crate::integrations::BuiltinRegistry;

const OBS_BUILTIN_ID: &str = "obs";
const OBS_CONNECTION_PREFIX: &str = "obs.connection.";
const OBS_NAME_LIST_KINDS: [&str; 9] = [
    "obs.scene.list_changed",
    "obs.scene.created",
    "obs.scene.removed",
    "obs.scene.renamed",
    "obs.source.input_created",
    "obs.source.input_removed",
    "obs.source.input_renamed",
    "obs.source.scene_item_created",
    "obs.source.scene_item_removed",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ObsCatalogList {
    Scenes,
    SceneItems,
    Inputs,
    AudioInputs,
}

impl ObsCatalogList {
    pub(crate) fn parse(options_key: &str) -> Option<Self> {
        match options_key {
            "obs.scene_names" => Some(Self::Scenes),
            "obs.source_names" => Some(Self::SceneItems),
            "obs.input_names" => Some(Self::Inputs),
            "obs.audio_inputs" | "obs.audio_input_names" => Some(Self::AudioInputs),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ObsCatalogField {
    pub(crate) options_key: String,
    pub(crate) list: ObsCatalogList,
}

pub(crate) fn obs_catalog_fields(specs: &[FormField]) -> Vec<ObsCatalogField> {
    let mut out = Vec::new();
    for spec in specs {
        push_catalog_field(spec, &mut out);
    }
    out
}

fn push_catalog_field(spec: &FormField, out: &mut Vec<ObsCatalogField>) {
    match spec {
        FormField::DynamicSelect { options_key, .. } => {
            if let Some(list) = ObsCatalogList::parse(options_key)
                && !out.iter().any(|field| field.options_key == *options_key)
            {
                out.push(ObsCatalogField {
                    options_key: (*options_key).to_owned(),
                    list,
                });
            }
        }
        FormField::Optional { inner, .. } => push_catalog_field(inner, out),
        _ => {}
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ObsCatalogSnapshot {
    pub(crate) scenes: Vec<String>,
    pub(crate) scene_items: Vec<String>,
    pub(crate) audio_inputs: Vec<String>,
}

impl ObsCatalogSnapshot {
    pub(crate) fn names(&self, list: ObsCatalogList) -> Vec<String> {
        match list {
            ObsCatalogList::Scenes => distinct(self.scenes.iter()),
            ObsCatalogList::SceneItems => distinct(self.scene_items.iter()),
            ObsCatalogList::AudioInputs => distinct(self.audio_inputs.iter()),
            ObsCatalogList::Inputs => distinct(
                self.scene_items
                    .iter()
                    .filter(|name| !self.scenes.contains(name))
                    .chain(self.audio_inputs.iter()),
            ),
        }
    }
}

fn distinct<'a>(names: impl Iterator<Item = &'a String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for name in names {
        if !out.contains(name) {
            out.push(name.clone());
        }
    }
    out
}

pub(crate) fn obs_catalog_choices(
    snapshot: &ObsCatalogSnapshot,
    fields: &[ObsCatalogField],
) -> ChoiceOptions {
    fields
        .iter()
        .map(|field| {
            let choices = snapshot
                .names(field.list)
                .into_iter()
                .map(|name| (name.clone(), name))
                .collect();
            (field.options_key.clone(), choices)
        })
        .collect()
}

pub(crate) fn live_obs_client(builtins: &BuiltinRegistry) -> Option<Arc<ObsClient>> {
    builtins
        .get(&BuiltinId::new(OBS_BUILTIN_ID))
        .and_then(|object| object.obs_client)
}

pub(crate) async fn load_obs_catalog_options(
    client: Option<Arc<ObsClient>>,
    fields: Vec<ObsCatalogField>,
) -> ChoiceOptions {
    let snapshot = match client {
        Some(client) if client.connection_state().is_connected() => read_snapshot(&*client).await,
        _ => ObsCatalogSnapshot::default(),
    };
    obs_catalog_choices(&snapshot, &fields)
}

async fn read_snapshot(source: &dyn ObsSource) -> ObsCatalogSnapshot {
    let scenes = source.scenes().await.unwrap_or_else(|failure| {
        tracing::debug!(%failure, "OBS scene list unavailable");
        Vec::new()
    });
    let mut scene_items = Vec::new();
    for scene in &scenes {
        if let Ok(items) = source.sources(scene).await {
            scene_items.extend(items.into_iter().map(|item| item.name));
        }
    }
    let audio_inputs = source.audio_inputs().await.unwrap_or_default();
    ObsCatalogSnapshot {
        scenes,
        scene_items,
        audio_inputs,
    }
}

pub(crate) fn changes_obs_catalog(event: &Event) -> bool {
    event.source == EventSource::Obs
        && (event.kind.starts_with(OBS_CONNECTION_PREFIX)
            || OBS_NAME_LIST_KINDS.contains(&event.kind.as_str()))
}

pub(crate) fn watch_obs_catalog<V: 'static>(
    bus: &Arc<EventBus>,
    on_change: fn(&mut V, &mut Context<V>),
    cx: &mut Context<V>,
) -> Task<()> {
    let mut sub = bus.subscribe();
    cx.spawn(async move |this, cx| {
        while let async_bridge::EventBatch::Ready(batch) =
            async_bridge::recv_event_batch(&mut sub).await
        {
            if !batch.iter().any(changes_obs_catalog) {
                continue;
            }
            if this.update(cx, on_change).is_err() {
                break;
            }
        }
    })
}
