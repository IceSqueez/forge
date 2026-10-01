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

pub(crate) fn distinct_vtube_fields<'a>(
    fields: impl IntoIterator<Item = &'a VTubeCatalogField>,
) -> Vec<VTubeCatalogField> {
    let mut out: Vec<VTubeCatalogField> = Vec::new();
    for field in fields {
        if !out.contains(field) {
            out.push(field.clone());
        }
    }
    out
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
    builtins: BuiltinRegistry,
    on_change: fn(&mut V, &mut Context<V>),
    cx: &mut Context<V>,
) -> Task<()> {
    let mut sub = bus.subscribe();
    let mut follow = VTubeCatalogFollow::default();
    follow.track(live_vtube_client(&builtins), on_change, cx);
    cx.spawn(async move |this, cx| {
        while let async_bridge::EventBatch::Ready(batch) =
            async_bridge::recv_event_batch(&mut sub).await
        {
            if !batch.iter().any(changes_vtube_connection) {
                continue;
            }
            let applied = this.update(cx, |view, cx| {
                follow.track(live_vtube_client(&builtins), on_change, cx);
                on_change(view, cx);
            });
            if applied.is_err() {
                break;
            }
        }
    })
}

#[derive(Default)]
struct VTubeCatalogFollow {
    client: Option<Arc<VTubeClient>>,
    _changes: Option<Task<()>>,
}

impl VTubeCatalogFollow {
    fn track<V: 'static>(
        &mut self,
        client: Option<Arc<VTubeClient>>,
        on_change: fn(&mut V, &mut Context<V>),
        cx: &mut Context<V>,
    ) {
        let unchanged = match (&self.client, &client) {
            (Some(watched), Some(live)) => Arc::ptr_eq(watched, live),
            (None, None) => true,
            _ => false,
        };
        if unchanged {
            return;
        }
        self._changes = client.as_ref().map(|client| {
            let mut changes = client.catalog_changes();
            cx.spawn(async move |this, cx| {
                while changes.changed().await {
                    if this.update(cx, on_change).is_err() {
                        break;
                    }
                }
            })
        });
        self.client = client;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use gpui::{AppContext, TestAppContext};

    use super::*;
    use crate::test_support::StubEventLog;

    fn select(options_key: &'static str) -> FormField {
        FormField::DynamicSelect {
            key: "target",
            label: "Target",
            options_key,
        }
    }

    fn choice(value: &str, label: &str) -> VTubeChoice {
        VTubeChoice {
            value: value.to_owned(),
            label: label.to_owned(),
        }
    }

    #[test]
    fn vtube_selects_are_picked_up_once_each_even_when_wrapped_optional() {
        let specs = vec![
            select("vtube.hotkey_ids"),
            FormField::Optional {
                key: "model",
                label: "Model",
                inner: Box::new(select("vtube.model_ids")),
            },
            select("vtube.hotkey_ids"),
            select("obs.scene_names"),
        ];

        let lists: Vec<VTubeCatalogList> = vtube_catalog_fields(&specs)
            .into_iter()
            .map(|field| field.list)
            .collect();

        assert_eq!(
            lists,
            vec![VTubeCatalogList::Hotkeys, VTubeCatalogList::Models]
        );
    }

    #[test]
    fn each_field_offers_its_own_catalog_list_as_value_and_label() {
        let catalog = VTubeCatalog {
            models: vec![choice("m-1", "Avatar")],
            hotkeys: vec![choice("h-1", "Wave")],
            expressions: vec![choice("blush.exp3.json", "Blush")],
        };
        let fields = vtube_catalog_fields(&[
            select("vtube.model_ids"),
            select("vtube.hotkey_ids"),
            select("vtube.expression_files"),
        ]);

        let options = vtube_catalog_choices(&catalog, &fields);

        assert_eq!(
            (
                &options["vtube.model_ids"],
                &options["vtube.hotkey_ids"],
                &options["vtube.expression_files"],
            ),
            (
                &vec![("m-1".to_owned(), "Avatar".to_owned())],
                &vec![("h-1".to_owned(), "Wave".to_owned())],
                &vec![("blush.exp3.json".to_owned(), "Blush".to_owned())],
            )
        );
    }

    #[test]
    fn without_a_vtube_client_every_field_offers_an_empty_list() {
        let fields = vtube_catalog_fields(&[select("vtube.model_ids"), select("vtube.hotkey_ids")]);

        let options = current_vtube_catalog_options(None, &fields);

        assert_eq!(
            (
                options.get("vtube.model_ids"),
                options.get("vtube.hotkey_ids")
            ),
            (Some(&Vec::new()), Some(&Vec::new()))
        );
    }

    #[test]
    fn only_the_vtube_connection_event_changes_the_catalog() {
        for (source, kind, expected) in [
            (EventSource::VTube, "vtube.connection.changed", true),
            (EventSource::VTube, "vtube.model.loaded", false),
            (EventSource::VTube, "vtube.hotkey.triggered", false),
            (EventSource::Obs, "vtube.connection.changed", false),
        ] {
            let event = Event::new(source, kind, serde_json::json!({}));

            assert_eq!(
                changes_vtube_connection(&event),
                expected,
                "{source:?} {kind}"
            );
        }
    }

    struct Watcher {
        reloads: usize,
        _watch: Vec<Task<()>>,
    }

    fn count_reload(watcher: &mut Watcher, _: &mut Context<Watcher>) {
        watcher.reloads += 1;
    }

    #[gpui::test]
    fn a_form_opened_before_the_client_exists_reloads_only_on_connection_changes(
        cx: &mut TestAppContext,
    ) {
        let bus = EventBus::new(Arc::new(StubEventLog));
        let watcher = cx.update(|cx| {
            cx.new(|cx| Watcher {
                reloads: 0,
                _watch: watch_vtube_catalog(&bus, None, count_reload, cx),
            })
        });
        cx.run_until_parked();

        bus.publish(Event::new(
            EventSource::VTube,
            "vtube.model.loaded",
            serde_json::json!({}),
        ));
        cx.run_until_parked();
        let after_model_switch = cx.update(|cx| watcher.read(cx).reloads);
        bus.publish(Event::new(
            EventSource::VTube,
            VTUBE_CONNECTION_KIND,
            serde_json::json!({}),
        ));
        cx.run_until_parked();

        assert_eq!(
            (after_model_switch, cx.update(|cx| watcher.read(cx).reloads)),
            (0, 1)
        );
    }
}
