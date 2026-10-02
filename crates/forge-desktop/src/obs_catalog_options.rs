use std::sync::Arc;

use forge_events::{Event, EventSource};
use forge_obs::{ObsClient, ObsSource};
use forge_registry::FormField;
use forge_runtime::EventBus;
use forge_types::IntegrationId;
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

pub(crate) fn distinct_obs_fields<'a>(
    fields: impl IntoIterator<Item = &'a ObsCatalogField>,
) -> Vec<ObsCatalogField> {
    let mut out: Vec<ObsCatalogField> = Vec::new();
    for field in fields {
        if !out.contains(field) {
            out.push(field.clone());
        }
    }
    out
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
        .get(&IntegrationId::new(OBS_BUILTIN_ID))
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use async_trait::async_trait;
    use forge_obs::{ObsError, SourceInfo};
    use gpui::{AppContext, TestAppContext};

    use super::*;
    use crate::test_support::{StubEventLog, runtime};

    fn select(options_key: &'static str) -> FormField {
        FormField::DynamicSelect {
            key: "target",
            label: "Target",
            options_key,
        }
    }

    fn names(raw: &[&str]) -> Vec<String> {
        raw.iter().map(|name| (*name).to_owned()).collect()
    }

    fn studio() -> ObsCatalogSnapshot {
        ObsCatalogSnapshot {
            scenes: names(&["Main", "Intro"]),
            scene_items: names(&["Camera", "Intro", "Mic", "Camera"]),
            audio_inputs: names(&["Mic", "Desktop Audio"]),
        }
    }

    #[test]
    fn obs_selects_are_picked_up_once_each_even_when_wrapped_optional() {
        let specs = vec![
            select("obs.scene_names"),
            FormField::Optional {
                key: "source",
                label: "Source",
                inner: Box::new(select("obs.source_names")),
            },
            select("obs.scene_names"),
            select("collections.twitch.rewards"),
            FormField::Text {
                key: "note",
                label: "Note",
                placeholder: "",
            },
        ];

        assert_eq!(
            obs_catalog_fields(&specs),
            vec![
                ObsCatalogField {
                    options_key: "obs.scene_names".to_owned(),
                    list: ObsCatalogList::Scenes,
                },
                ObsCatalogField {
                    options_key: "obs.source_names".to_owned(),
                    list: ObsCatalogList::SceneItems,
                },
            ]
        );
    }

    #[test]
    fn each_list_offers_its_names_once_in_first_seen_order() {
        let snapshot = studio();

        for (list, expected) in [
            (ObsCatalogList::Scenes, vec!["Main", "Intro"]),
            (ObsCatalogList::SceneItems, vec!["Camera", "Intro", "Mic"]),
            (ObsCatalogList::AudioInputs, vec!["Mic", "Desktop Audio"]),
            (
                ObsCatalogList::Inputs,
                vec!["Camera", "Mic", "Desktop Audio"],
            ),
        ] {
            assert_eq!(snapshot.names(list), names(&expected), "{list:?}");
        }
    }

    #[test]
    fn a_choice_stores_and_shows_the_obs_name_under_each_fields_own_key() {
        let fields =
            obs_catalog_fields(&[select("obs.audio_inputs"), select("obs.audio_input_names")]);

        let options = obs_catalog_choices(&studio(), &fields);

        let expected = vec![
            ("Mic".to_owned(), "Mic".to_owned()),
            ("Desktop Audio".to_owned(), "Desktop Audio".to_owned()),
        ];
        assert_eq!(
            (
                &options["obs.audio_inputs"],
                &options["obs.audio_input_names"]
            ),
            (&expected, &expected)
        );
    }

    #[test]
    fn without_an_obs_client_every_field_offers_an_empty_list() {
        let fields = obs_catalog_fields(&[select("obs.scene_names"), select("obs.input_names")]);

        let options = runtime().block_on(load_obs_catalog_options(None, fields));

        assert_eq!(
            (
                options.get("obs.scene_names"),
                options.get("obs.input_names")
            ),
            (Some(&Vec::new()), Some(&Vec::new()))
        );
    }

    struct FakeObs {
        scenes: Result<Vec<String>, ()>,
        failing_scene: &'static str,
        audio_inputs: Result<Vec<String>, ()>,
    }

    fn item(name: &str) -> SourceInfo {
        SourceInfo {
            name: name.to_owned(),
            visible: true,
            locked: false,
            audio_db: None,
            kind: None,
        }
    }

    #[async_trait]
    impl ObsSource for FakeObs {
        async fn scenes(&self) -> Result<Vec<String>, ObsError> {
            self.scenes.clone().map_err(|()| ObsError::Timeout)
        }

        async fn current_scene(&self) -> Result<Option<String>, ObsError> {
            Ok(None)
        }

        async fn sources(&self, scene: &str) -> Result<Vec<SourceInfo>, ObsError> {
            if scene == self.failing_scene {
                return Err(ObsError::Timeout);
            }
            Ok(vec![item(&format!("{scene} Camera"))])
        }

        async fn audio_inputs(&self) -> Result<Vec<String>, ObsError> {
            self.audio_inputs.clone().map_err(|()| ObsError::Timeout)
        }

        async fn transitions(&self) -> Result<Vec<String>, ObsError> {
            Ok(Vec::new())
        }

        async fn profiles(&self) -> Result<Vec<String>, ObsError> {
            Ok(Vec::new())
        }

        async fn scene_collections(&self) -> Result<Vec<String>, ObsError> {
            Ok(Vec::new())
        }
    }

    #[test]
    fn a_scene_whose_items_fail_to_list_still_leaves_the_other_scenes_items() {
        let obs = FakeObs {
            scenes: Ok(names(&["Main", "Broken", "Outro"])),
            failing_scene: "Broken",
            audio_inputs: Ok(names(&["Mic"])),
        };

        let snapshot = runtime().block_on(read_snapshot(&obs));

        assert_eq!(
            snapshot.scene_items,
            names(&["Main Camera", "Outro Camera"])
        );
    }

    #[test]
    fn a_failed_scene_list_still_offers_the_audio_inputs() {
        let obs = FakeObs {
            scenes: Err(()),
            failing_scene: "",
            audio_inputs: Ok(names(&["Mic"])),
        };

        let snapshot = runtime().block_on(read_snapshot(&obs));

        assert_eq!(
            snapshot,
            ObsCatalogSnapshot {
                scenes: Vec::new(),
                scene_items: Vec::new(),
                audio_inputs: names(&["Mic"]),
            }
        );
    }

    #[test]
    fn a_failed_audio_input_list_still_offers_the_scenes() {
        let obs = FakeObs {
            scenes: Ok(names(&["Main"])),
            failing_scene: "",
            audio_inputs: Err(()),
        };

        let snapshot = runtime().block_on(read_snapshot(&obs));

        assert_eq!(
            (snapshot.scenes, snapshot.audio_inputs),
            (names(&["Main"]), Vec::new())
        );
    }

    #[test]
    fn only_obs_connection_and_name_list_events_change_the_catalog() {
        for (source, kind, expected) in [
            (EventSource::Obs, "obs.connection.connected", true),
            (EventSource::Obs, "obs.connection.disconnected", true),
            (EventSource::Obs, "obs.scene.created", true),
            (EventSource::Obs, "obs.scene.renamed", true),
            (EventSource::Obs, "obs.source.input_removed", true),
            (EventSource::Obs, "obs.source.scene_item_created", true),
            (EventSource::Obs, "obs.scene.changed", false),
            (EventSource::Obs, "obs.recording.started", false),
            (EventSource::Twitch, "obs.scene.created", false),
            (EventSource::VTube, "obs.connection.connected", false),
        ] {
            let event = Event::new(source, kind, serde_json::json!({}));

            assert_eq!(changes_obs_catalog(&event), expected, "{source:?} {kind}");
        }
    }

    struct Watcher {
        reloads: usize,
        _watch: Task<()>,
    }

    fn count_reload(watcher: &mut Watcher, _: &mut Context<Watcher>) {
        watcher.reloads += 1;
    }

    #[gpui::test]
    fn a_catalog_event_reloads_and_an_unrelated_event_does_not(cx: &mut TestAppContext) {
        let bus = EventBus::new(Arc::new(StubEventLog));
        let watcher = cx.update(|cx| {
            cx.new(|cx| Watcher {
                reloads: 0,
                _watch: watch_obs_catalog(&bus, count_reload, cx),
            })
        });
        cx.run_until_parked();

        bus.publish(Event::new(
            EventSource::Obs,
            "obs.scene.changed",
            serde_json::json!({}),
        ));
        cx.run_until_parked();
        let after_unrelated = cx.update(|cx| watcher.read(cx).reloads);
        bus.publish(Event::new(
            EventSource::Obs,
            "obs.scene.created",
            serde_json::json!({}),
        ));
        cx.run_until_parked();

        assert_eq!(
            (after_unrelated, cx.update(|cx| watcher.read(cx).reloads)),
            (0, 1)
        );
    }
}
