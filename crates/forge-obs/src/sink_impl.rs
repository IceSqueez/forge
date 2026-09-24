use std::collections::BTreeMap;
use std::path::Path;

use async_trait::async_trait;
use forge_types::Variant;
use obws::common::MediaAction;
use obws::requests::filters::SetEnabled as FilterSetEnabled;
use obws::requests::inputs::{InputId, SetSettings, Volume};
use obws::requests::scene_items::{Id, SetEnabled, SetLocked};
use obws::requests::scenes::SceneId;
use obws::requests::sources::{SaveScreenshot, SourceId};

use crate::client::ObsClient;
use crate::error::ObsError;
use crate::session::{CONFIG_SWITCH_TIMEOUT, LiveSession};
use crate::sink::ObsSink;

impl ObsClient {
    async fn resolve_scene_item_id(
        &self,
        session: &LiveSession,
        scene: &str,
        source: &str,
    ) -> Result<i64, ObsError> {
        let cached_id = self
            .scene_item_id_cache
            .lock()
            .map_err(|_| ObsError::Protocol("scene item cache poisoned".to_owned()))?
            .get(&(scene.to_owned(), source.to_owned()))
            .copied();

        if let Some(id) = cached_id {
            return Ok(id);
        }

        let id = session
            .request(
                "GetSceneItemId",
                session.obs().scene_items().id(Id {
                    scene: SceneId::Name(scene),
                    source,
                    search_offset: None,
                }),
            )
            .await?;
        self.scene_item_id_cache
            .lock()
            .map_err(|_| ObsError::Protocol("scene item cache poisoned".to_owned()))?
            .insert((scene.to_owned(), source.to_owned()), id);
        Ok(id)
    }

    fn forget_scene_item_ids(&self) {
        if let Ok(mut cache) = self.scene_item_id_cache.lock() {
            cache.clear();
        }
    }
}

#[async_trait]
impl ObsSink for ObsClient {
    async fn set_scene(&self, scene: &str) -> Result<(), ObsError> {
        let session = self.active_session().await?;
        session
            .request(
                "SetCurrentProgramScene",
                session
                    .obs()
                    .scenes()
                    .set_current_program_scene(SceneId::Name(scene)),
            )
            .await
    }

    async fn set_source_visible(
        &self,
        scene: &str,
        source: &str,
        visible: bool,
    ) -> Result<(), ObsError> {
        let session = self.active_session().await?;
        let item_id = self.resolve_scene_item_id(&session, scene, source).await?;

        session
            .request(
                "SetSceneItemEnabled",
                session.obs().scene_items().set_enabled(SetEnabled {
                    scene: SceneId::Name(scene),
                    item_id,
                    enabled: visible,
                }),
            )
            .await
    }

    async fn set_source_locked(
        &self,
        scene: &str,
        source: &str,
        locked: bool,
    ) -> Result<(), ObsError> {
        let session = self.active_session().await?;
        let item_id = self.resolve_scene_item_id(&session, scene, source).await?;

        session
            .request(
                "SetSceneItemLocked",
                session.obs().scene_items().set_locked(SetLocked {
                    scene: SceneId::Name(scene),
                    item_id,
                    locked,
                }),
            )
            .await
    }

    async fn set_input_mute(&self, input: &str, mute: bool) -> Result<(), ObsError> {
        let session = self.active_session().await?;
        session
            .request(
                "SetInputMute",
                session.obs().inputs().set_muted(InputId::Name(input), mute),
            )
            .await
    }

    async fn start_record(&self) -> Result<(), ObsError> {
        let session = self.active_session().await?;
        session
            .request("StartRecord", session.obs().recording().start())
            .await
    }

    async fn stop_record(&self) -> Result<(), ObsError> {
        let session = self.active_session().await?;
        session
            .request("StopRecord", session.obs().recording().stop())
            .await
            .map(|_| ())
    }

    async fn start_stream(&self) -> Result<(), ObsError> {
        let session = self.active_session().await?;
        session
            .request("StartStream", session.obs().streaming().start())
            .await
    }

    async fn stop_stream(&self) -> Result<(), ObsError> {
        let session = self.active_session().await?;
        session
            .request("StopStream", session.obs().streaming().stop())
            .await
    }

    async fn raw_request(
        &self,
        _request_type: &str,
        _payload: &Variant,
    ) -> Result<Variant, ObsError> {
        Err(ObsError::Protocol(
            "raw_request not supported by obws 0.15".to_owned(),
        ))
    }

    async fn set_preview_scene(&self, scene: &str) -> Result<(), ObsError> {
        let session = self.active_session().await?;
        session
            .request(
                "SetCurrentPreviewScene",
                session
                    .obs()
                    .scenes()
                    .set_current_preview_scene(SceneId::Name(scene)),
            )
            .await
    }

    async fn set_current_scene_transition(&self, name: &str) -> Result<(), ObsError> {
        let session = self.active_session().await?;
        session
            .request(
                "SetCurrentSceneTransition",
                session.obs().transitions().set_current(name),
            )
            .await
    }

    async fn set_input_volume_db(&self, input: &str, db: f64) -> Result<(), ObsError> {
        let session = self.active_session().await?;
        session
            .request(
                "SetInputVolume",
                session
                    .obs()
                    .inputs()
                    .set_volume(InputId::Name(input), Volume::Db(db as f32)),
            )
            .await?;

        // An input's volume is global in OBS, so the same name can appear as a scene item
        // in several scenes; write the confirmed level back into every scene's copy.
        if let Ok(mut catalog) = self.catalog_state.write() {
            for sources in catalog.sources.values_mut() {
                for info in sources.iter_mut() {
                    if info.name == input {
                        info.audio_db = Some(db as f32);
                    }
                }
            }
        }

        Ok(())
    }

    async fn set_input_settings(
        &self,
        input: &str,
        settings: &Variant,
        overlay: bool,
    ) -> Result<(), ObsError> {
        let json_settings = settings.to_json();
        let session = self.active_session().await?;
        session
            .request(
                "SetInputSettings",
                session.obs().inputs().set_settings(SetSettings {
                    input: InputId::Name(input),
                    settings: &json_settings,
                    overlay: Some(overlay),
                }),
            )
            .await
    }

    async fn pause_record(&self) -> Result<(), ObsError> {
        let session = self.active_session().await?;
        session
            .request("PauseRecord", session.obs().recording().pause())
            .await
    }

    async fn resume_record(&self) -> Result<(), ObsError> {
        let session = self.active_session().await?;
        session
            .request("ResumeRecord", session.obs().recording().resume())
            .await
    }

    async fn toggle_record_pause(&self) -> Result<(), ObsError> {
        let session = self.active_session().await?;
        session
            .request(
                "ToggleRecordPause",
                session.obs().recording().toggle_pause(),
            )
            .await
            .map(|_| ())
    }

    async fn send_stream_caption(&self, text: &str) -> Result<(), ObsError> {
        let session = self.active_session().await?;
        session
            .request(
                "SendStreamCaption",
                session.obs().streaming().send_caption(text),
            )
            .await
    }

    async fn start_replay_buffer(&self) -> Result<(), ObsError> {
        let session = self.active_session().await?;
        session
            .request("StartReplayBuffer", session.obs().replay_buffer().start())
            .await
    }

    async fn stop_replay_buffer(&self) -> Result<(), ObsError> {
        let session = self.active_session().await?;
        session
            .request("StopReplayBuffer", session.obs().replay_buffer().stop())
            .await
    }

    async fn save_replay_buffer(&self) -> Result<(), ObsError> {
        let session = self.active_session().await?;
        session
            .request("SaveReplayBuffer", session.obs().replay_buffer().save())
            .await
    }

    async fn set_studio_mode(&self, enabled: bool) -> Result<(), ObsError> {
        let session = self.active_session().await?;
        session
            .request(
                "SetStudioModeEnabled",
                session.obs().ui().set_studio_mode_enabled(enabled),
            )
            .await
    }

    async fn trigger_studio_transition(&self) -> Result<(), ObsError> {
        let session = self.active_session().await?;
        session
            .request(
                "TriggerStudioModeTransition",
                session.obs().transitions().trigger(),
            )
            .await
    }

    async fn get_scene_list(&self) -> Result<Variant, ObsError> {
        let session = self.active_session().await?;
        let scenes = session
            .request("GetSceneList", session.obs().scenes().list())
            .await?;
        let all_names: Vec<Variant> = scenes
            .scenes
            .iter()
            .map(|s| Variant::String(s.id.name.clone()))
            .collect();
        let current = scenes
            .current_program_scene
            .map(|id| id.name)
            .unwrap_or_default();
        let mut obj = BTreeMap::new();
        obj.insert("all_names".to_owned(), Variant::Array(all_names));
        obj.insert("current".to_owned(), Variant::String(current));
        Ok(Variant::Object(obj))
    }

    async fn get_input_list(&self) -> Result<Variant, ObsError> {
        let session = self.active_session().await?;
        let inputs = session
            .request("GetInputList", session.obs().inputs().list(None))
            .await?;
        let all_names: Vec<Variant> = inputs
            .iter()
            .map(|i| Variant::String(i.id.name.clone()))
            .collect();
        let mut obj = BTreeMap::new();
        obj.insert("all_names".to_owned(), Variant::Array(all_names));
        Ok(Variant::Object(obj))
    }

    async fn get_record_status(&self) -> Result<Variant, ObsError> {
        let session = self.active_session().await?;
        let status = session
            .request("GetRecordStatus", session.obs().recording().status())
            .await?;
        let mut obj = BTreeMap::new();
        obj.insert("is_active".to_owned(), Variant::Bool(status.active));
        obj.insert("is_paused".to_owned(), Variant::Bool(status.paused));
        obj.insert(
            "duration_ms".to_owned(),
            Variant::Int(status.duration.whole_milliseconds() as i64),
        );
        Ok(Variant::Object(obj))
    }

    async fn get_stream_status(&self) -> Result<Variant, ObsError> {
        let session = self.active_session().await?;
        let status = session
            .request("GetStreamStatus", session.obs().streaming().status())
            .await?;
        let mut obj = BTreeMap::new();
        obj.insert("is_active".to_owned(), Variant::Bool(status.active));
        obj.insert(
            "duration_ms".to_owned(),
            Variant::Int(status.duration.whole_milliseconds() as i64),
        );
        Ok(Variant::Object(obj))
    }

    async fn get_current_scene(&self) -> Result<Option<String>, ObsError> {
        let session = self.active_session().await?;
        let current = session
            .request(
                "GetCurrentProgramScene",
                session.obs().scenes().current_program_scene(),
            )
            .await?;
        Ok(Some(current.id.name))
    }

    async fn get_input_settings(&self, input: &str) -> Result<Variant, ObsError> {
        let session = self.active_session().await?;
        let result = session
            .request(
                "GetInputSettings",
                session
                    .obs()
                    .inputs()
                    .settings::<serde_json::Value>(InputId::Name(input)),
            )
            .await?;
        let settings_variant = serde_json::from_value::<Variant>(result.settings)
            .unwrap_or(Variant::Object(BTreeMap::new()));
        let mut obj = BTreeMap::new();
        obj.insert("settings".to_owned(), settings_variant);
        obj.insert("kind".to_owned(), Variant::String(result.kind));
        Ok(Variant::Object(obj))
    }

    async fn set_source_filter_enabled(
        &self,
        source: &str,
        filter: &str,
        enabled: bool,
    ) -> Result<(), ObsError> {
        let session = self.active_session().await?;
        session
            .request(
                "SetSourceFilterEnabled",
                session.obs().filters().set_enabled(FilterSetEnabled {
                    source: SourceId::Name(source),
                    filter,
                    enabled,
                }),
            )
            .await
    }

    async fn refresh_browser_source(&self, input: &str) -> Result<(), ObsError> {
        let session = self.active_session().await?;
        session
            .request(
                "PressInputPropertiesButton",
                session
                    .obs()
                    .inputs()
                    .press_properties_button(InputId::Name(input), "refreshnocache"),
            )
            .await
    }

    async fn restart_media_input(&self, input: &str) -> Result<(), ObsError> {
        let session = self.active_session().await?;
        session
            .request(
                "TriggerMediaInputAction",
                session
                    .obs()
                    .media_inputs()
                    .trigger_action(InputId::Name(input), MediaAction::Restart),
            )
            .await
    }

    async fn start_virtual_cam(&self) -> Result<(), ObsError> {
        let session = self.active_session().await?;
        session
            .request("StartVirtualCam", session.obs().virtual_cam().start())
            .await
    }

    async fn stop_virtual_cam(&self) -> Result<(), ObsError> {
        let session = self.active_session().await?;
        session
            .request("StopVirtualCam", session.obs().virtual_cam().stop())
            .await
    }

    async fn save_source_screenshot(
        &self,
        source: &str,
        file_path: &str,
        format: &str,
    ) -> Result<(), ObsError> {
        let session = self.active_session().await?;
        session
            .request(
                "SaveSourceScreenshot",
                session.obs().sources().save_screenshot(SaveScreenshot {
                    source: SourceId::Name(source),
                    format,
                    width: None,
                    height: None,
                    compression_quality: None,
                    file_path: Path::new(file_path),
                }),
            )
            .await
    }

    async fn set_record_directory(&self, path: &str) -> Result<(), ObsError> {
        let session = self.active_session().await?;
        session
            .request(
                "SetRecordDirectory",
                session.obs().config().set_record_directory(path),
            )
            .await
    }

    async fn set_current_profile(&self, name: &str) -> Result<(), ObsError> {
        let session = self.active_session().await?;
        session
            .request_within(
                CONFIG_SWITCH_TIMEOUT,
                "SetCurrentProfile",
                session.obs().profiles().set_current(name),
            )
            .await
    }

    async fn set_current_scene_collection(&self, name: &str) -> Result<(), ObsError> {
        let session = self.active_session().await?;
        let switched = session
            .request_within(
                CONFIG_SWITCH_TIMEOUT,
                "SetCurrentSceneCollection",
                session.obs().scene_collections().set_current(name),
            )
            .await;
        self.forget_scene_item_ids();
        switched
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::ObsClient;

    #[tokio::test]
    async fn set_scene_returns_disconnected_when_not_connected() {
        let client = ObsClient::new_for_test("localhost:4455".to_owned());
        let result = client.set_scene("Gameplay").await;
        assert!(matches!(result, Err(ObsError::Disconnected)));
    }
}
