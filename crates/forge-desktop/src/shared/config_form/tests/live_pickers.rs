use std::sync::Arc;

use forge_host::integrations::{BuiltinRegistry, ObsInstallSeed, VTubeInstallSeed};
use forge_obs::SwitchableObsSink;
use forge_storage::CredentialsRepo;
use forge_storage::test_support::MemoryCredentials;
use forge_vtube::{SwitchableVTubeSink, VTubeClient, VTubeConfig};
use futures_util::{SinkExt as _, StreamExt as _};
use gpui::TestAppContext;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::runtime::Runtime;
use tokio_tungstenite::tungstenite::Message;

use super::{build, collected, config, dynamic_select};
use crate::shared::config_form::{CollectionChoices, ConfigField, FieldConfig};
use crate::shared::option_choices::{ChoiceOption, resolve_options};

#[path = "../../../../../forge-obs/tests/common/mod.rs"]
mod fake_obs;

use fake_obs::{
    Recorder, Reply, SCENE_UUID, Seen, completes_every_handshake, connect_to, settle_until_async,
    spawn_fake_obs, standard_reply,
};

const SAVED_KEY: &str = "target";
const OBS_SCENES_KEY: &str = "obs.scene_names";
const VTUBE_HOTKEYS_KEY: &str = "vtube.hotkey_ids";

#[derive(Debug, Clone, Copy)]
enum LetGo {
    Clear,
    DisconnectLive,
}

fn current_thread_runtime() -> Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

struct Picker {
    _host: gpui::Entity<super::Host>,
    fields: Vec<ConfigField>,
    choices: CollectionChoices,
    builtins: BuiltinRegistry,
    saved: FieldConfig,
}

impl Picker {
    fn open(
        cx: &mut TestAppContext,
        options_key: &'static str,
        saved_value: &str,
        builtins: &BuiltinRegistry,
    ) -> Self {
        let spec = dynamic_select(options_key);
        let saved = config(&[(SAVED_KEY, forge_types::Variant::String(saved_value.into()))]);
        let (host, fields) = build(cx, &spec, &Default::default(), &saved, "");
        Self {
            _host: host,
            fields,
            choices: CollectionChoices::for_specs(&[spec]),
            builtins: builtins.clone(),
            saved,
        }
    }

    fn reload(&mut self, rt: &Runtime) -> Vec<ChoiceOption> {
        let keys = self.choices.option_keys();
        let options = rt.block_on(resolve_options(self.builtins.clone(), keys));
        self.choices.refresh(&mut self.fields, &options, None);
        match self.fields.as_slice() {
            [ConfigField::Choice { options, .. }] => options.clone(),
            _ => panic!("the select did not fold into a single choice"),
        }
    }

    fn reload_until_offered(&mut self, rt: &Runtime) -> Vec<ChoiceOption> {
        let keys = self.choices.option_keys();
        let builtins = &self.builtins;
        rt.block_on(settle_until_async(
            "the integration offered its names",
            || {
                let offered = resolve_options(builtins.clone(), keys.clone());
                async move { offered.await.values().all(|list| !list.is_empty()) }
            },
        ));
        self.reload(rt)
    }

    fn collected(&self, cx: &mut TestAppContext) -> FieldConfig {
        collected(cx, &self.fields, &FieldConfig::new())
    }
}

fn pair(value: &str, label: &str) -> ChoiceOption {
    ChoiceOption::new(value, label)
}

fn studio_with_two_scenes(seen: Seen<'_>) -> Reply {
    match seen.request_type {
        "GetSceneList" => Reply::Answer(json!({
            "scenes": [
                { "sceneName": "Main", "sceneUuid": SCENE_UUID, "sceneIndex": 0 },
                { "sceneName": "Intro", "sceneUuid": SCENE_UUID, "sceneIndex": 1 },
            ],
        })),
        other => standard_reply(other),
    }
}

#[gpui::test]
fn an_installed_obs_client_fills_the_picker_and_letting_go_empties_it_but_keeps_the_saved_value(
    cx: &mut TestAppContext,
) {
    for let_go in [LetGo::Clear, LetGo::DisconnectLive] {
        let rt = current_thread_runtime();
        let builtins = BuiltinRegistry::default();
        let seed = ObsInstallSeed::new(SwitchableObsSink::new(), builtins.clone());
        let fake = rt.block_on(spawn_fake_obs(
            completes_every_handshake,
            Arc::new(studio_with_two_scenes),
        ));
        let mut picker = Picker::open(cx, OBS_SCENES_KEY, "Intro", &builtins);

        let before_install = picker.reload(&rt);
        let client = rt.block_on(connect_to(&fake, &Recorder::default()));
        seed.install(client);
        let installed = picker.reload_until_offered(&rt);
        match let_go {
            LetGo::Clear => seed.clear(),
            LetGo::DisconnectLive => rt.block_on(seed.disconnect_live()),
        }
        let after = picker.reload(&rt);

        assert_eq!(
            (before_install, installed, after, picker.collected(cx)),
            (
                Vec::new(),
                vec![pair("Main", "Main"), pair("Intro", "Intro")],
                Vec::new(),
                picker.saved.clone(),
            ),
            "{let_go:?}"
        );
    }
}

fn vts_frame(request: &Value, data: Value) -> Value {
    let request_type = request["messageType"].as_str().unwrap_or_default();
    json!({
        "apiName": "VTubeStudioPublicAPI",
        "apiVersion": "1.0",
        "requestID": request["requestID"],
        "messageType": request_type.replace("Request", "Response"),
        "data": data,
    })
}

fn vts_answer(request_type: &str) -> Value {
    match request_type {
        "AuthenticationTokenRequest" => json!({ "authenticationToken": "fake-token" }),
        "AuthenticationRequest" => json!({ "authenticated": true, "reason": "" }),
        "HotkeysInCurrentModelRequest" => json!({
            "availableHotkeys": [
                { "hotkeyID": "h-wave", "name": "Wave" },
                { "hotkeyID": "h-spin", "name": "Spin" },
            ],
        }),
        _ => json!({}),
    }
}

async fn serve_fake_vts(listener: TcpListener) {
    while let Ok((stream, _)) = listener.accept().await {
        let Ok(mut socket) = tokio_tungstenite::accept_async(stream).await else {
            continue;
        };
        tokio::spawn(async move {
            while let Some(Ok(message)) = socket.next().await {
                let Message::Text(text) = message else {
                    continue;
                };
                let request: Value = serde_json::from_str(text.as_str()).unwrap_or_default();
                let request_type = request["messageType"].as_str().unwrap_or_default();
                let reply = vts_frame(&request, vts_answer(request_type));
                if socket.send(Message::text(reply.to_string())).await.is_err() {
                    return;
                }
            }
        });
    }
}

fn connected_vtube_client(rt: &Runtime) -> Arc<VTubeClient> {
    rt.block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("ws://{}", listener.local_addr().unwrap());
        tokio::spawn(serve_fake_vts(listener));
        let creds: Arc<dyn CredentialsRepo> = Arc::new(MemoryCredentials::default());
        Arc::new(VTubeClient::connect(
            VTubeConfig { endpoint },
            Arc::new(Recorder::default()),
            creds,
        ))
    })
}

#[gpui::test]
fn an_installed_vtube_client_fills_the_picker_and_letting_go_empties_it_but_keeps_the_saved_value(
    cx: &mut TestAppContext,
) {
    for let_go in [LetGo::Clear, LetGo::DisconnectLive] {
        let rt = current_thread_runtime();
        let builtins = BuiltinRegistry::default();
        let seed = VTubeInstallSeed::new(SwitchableVTubeSink::new(), builtins.clone());
        let mut picker = Picker::open(cx, VTUBE_HOTKEYS_KEY, "h-spin", &builtins);

        let before_install = picker.reload(&rt);
        seed.install(connected_vtube_client(&rt));
        let installed = picker.reload_until_offered(&rt);
        match let_go {
            LetGo::Clear => seed.clear(),
            LetGo::DisconnectLive => rt.block_on(seed.disconnect_live()),
        }
        let after = picker.reload(&rt);

        assert_eq!(
            (before_install, installed, after, picker.collected(cx)),
            (
                Vec::new(),
                vec![pair("h-wave", "Wave"), pair("h-spin", "Spin")],
                Vec::new(),
                picker.saved.clone(),
            ),
            "{let_go:?}"
        );
    }
}
