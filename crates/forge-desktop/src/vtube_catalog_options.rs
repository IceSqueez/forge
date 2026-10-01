use std::sync::Arc;

use forge_events::{Event, EventSource};
use forge_platform_core::BuiltinId;
use forge_registry::FormField;
use forge_runtime::EventBus;
use forge_vtube::{VTubeCatalog, VTubeChoice, VTubeClient};
use gpui::{Context, Task};

use crate::async_bridge;
use crate::collection_options::ChoiceOptions;
use crate::integrations::BuiltinRegistry;

const VTUBE_BUILTIN_ID: &str = "vtube";
const VTUBE_CONNECTION_KIND: &str = "vtube.connection.changed";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VTubeCatalogList {
    Models,
    Hotkeys,
    Expressions,
}

impl VTubeCatalogList {
    pub(crate) fn parse(options_key: &str) -> Option<Self> {
        match options_key {
            "vtube.model_ids" => Some(Self::Models),
            "vtube.hotkey_ids" => Some(Self::Hotkeys),
            "vtube.expression_files" => Some(Self::Expressions),
            _ => None,
        }
    }

    fn choices(self, catalog: &VTubeCatalog) -> &[VTubeChoice] {
        match self {
            Self::Models => &catalog.models,
            Self::Hotkeys => &catalog.hotkeys,
            Self::Expressions => &catalog.expressions,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VTubeCatalogField {
    pub(crate) options_key: String,
    pub(crate) list: VTubeCatalogList,
}

pub(crate) fn vtube_catalog_fields(specs: &[FormField]) -> Vec<VTubeCatalogField> {
    let mut out = Vec::new();
    for spec in specs {
        push_catalog_field(spec, &mut out);
    }
    out
}

fn push_catalog_field(spec: &FormField, out: &mut Vec<VTubeCatalogField>) {
    match spec {
        FormField::DynamicSelect { options_key, .. } => {
            if let Some(list) = VTubeCatalogList::parse(options_key)
                && !out.iter().any(|field| field.options_key == *options_key)
            {
                out.push(VTubeCatalogField {
                    options_key: (*options_key).to_owned(),
                    list,
                });
            }
        }
        FormField::Optional { inner, .. } => push_catalog_field(inner, out),
        _ => {}
    }
}

pub(crate) fn vtube_catalog_choices(
    catalog: &VTubeCatalog,
    fields: &[VTubeCatalogField],
) -> ChoiceOptions {
    fields
        .iter()
        .map(|field| {
            let choices = field
                .list
                .choices(catalog)
                .iter()
                .map(|choice| (choice.value.clone(), choice.label.clone()))
                .collect();
            (field.options_key.clone(), choices)
        })
        .collect()
}

pub(crate) fn live_vtube_client(builtins: &BuiltinRegistry) -> Option<Arc<VTubeClient>> {
    builtins
        .get(&BuiltinId::new(VTUBE_BUILTIN_ID))
        .and_then(|object| object.vtube_client)
}

pub(crate) fn current_vtube_catalog_options(
    client: Option<&VTubeClient>,
    fields: &[VTubeCatalogField],
) -> ChoiceOptions {
    let catalog = match client {
        Some(client) if client.connection_state().is_connected() => client.catalog(),
        _ => VTubeCatalog::default(),
    };
    vtube_catalog_choices(&catalog, fields)
}

pub(crate) fn changes_vtube_connection(event: &Event) -> bool {
    event.source == EventSource::VTube && event.kind == VTUBE_CONNECTION_KIND
}

pub(crate) fn watch_vtube_catalog<V: 'static>(
    bus: &Arc<EventBus>,
    client: Option<Arc<VTubeClient>>,
    on_change: fn(&mut V, &mut Context<V>),
    cx: &mut Context<V>,
) -> Vec<Task<()>> {
    let mut sub = bus.subscribe();
    let connection_task = cx.spawn(async move |this, cx| {
        while let async_bridge::EventBatch::Ready(batch) =
            async_bridge::recv_event_batch(&mut sub).await
        {
            if !batch.iter().any(changes_vtube_connection) {
                continue;
            }
            if this.update(cx, on_change).is_err() {
                break;
            }
        }
    });
    let mut tasks = vec![connection_task];
    if let Some(client) = client {
        let mut changes = client.catalog_changes();
        tasks.push(cx.spawn(async move |this, cx| {
            while changes.changed().await {
                if this.update(cx, on_change).is_err() {
                    break;
                }
            }
        }));
    }
    tasks
}
