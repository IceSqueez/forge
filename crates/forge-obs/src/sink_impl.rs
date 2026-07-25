use std::collections::BTreeMap;
use std::path::Path;

use async_trait::async_trait;
use forge_types::Variant;
use obws::common::MediaAction;
use obws::requests::filters::SetEnabled as FilterSetEnabled;
use obws::requests::inputs::{InputId, SetSettings, Volume};
use obws::requests::scene_items::{Id, SetEnabled};
use obws::requests::scenes::SceneId;
use obws::requests::sources::{SaveScreenshot, SourceId};

use crate::client::ObsClient;
use crate::error::ObsError;
use crate::sink::ObsSink;

#[async_trait]
impl ObsSink for ObsClient {
    async fn set_scene(&self, scene: &str) -> Result<(), ObsError> {
        let guard = self.inner.read().await;
        let Some(client) = guard.as_ref() else {
            return Err(ObsError::Disconnected);
        };
        client
            .scenes()
            .set_current_program_scene(SceneId::Name(scene))
            .await
            .map_err(|e| map_request_error("SetCurrentProgramScene", e))
    }

    async fn set_source_visible(
        &self,
        scene: &str,
        source: &str,
        visible: bool,
    ) -> Result<(), ObsError> {
        let cached_id = self
            .scene_item_id_cache
            .lock()
            .map_err(|_| ObsError::Protocol("scene item cache poisoned".to_owned()))?
            .get(&(scene.to_owned(), source.to_owned()))
            .copied();

        let guard = self.inner.read().await;
        let Some(client) = guard.as_ref() else {
            return Err(ObsError::Disconnected);
        };

        let item_id = if let Some(id) = cached_id {
            id
        } else {
            let id = client
                .scene_items()
                .id(Id {
                    scene: SceneId::Name(scene),
                    source,
                    search_offset: None,
                })
                .await
                .map_err(|e| map_request_error("GetSceneItemId", e))?;
            self.scene_item_id_cache
                .lock()
                .map_err(|_| ObsError::Protocol("scene item cache poisoned".to_owned()))?
                .insert((scene.to_owned(), source.to_owned()), id);
            id
        };

        client
            .scene_items()
            .set_enabled(SetEnabled {
                scene: SceneId::Name(scene),
                item_id,
                enabled: visible,
            })
            .await
            .map_err(|e| map_request_error("SetSceneItemEnabled", e))
    }

    async fn set_input_mute(&self, input: &str, mute: bool) -> Result<(), ObsError> {
        let guard = self.inner.read().await;
        let Some(client) = guard.as_ref() else {
            return Err(ObsError::Disconnected);
        };
        client
            .inputs()
            .set_muted(InputId::Name(input), mute)
            .await
            .map_err(|e| map_request_error("SetInputMute", e))
    }

    async fn start_record(&self) -> Result<(), ObsError> {
        let guard = self.inner.read().await;
        let Some(client) = guard.as_ref() else {
            return Err(ObsError::Disconnected);
        };
        client
            .recording()
            .start()
            .await
            .map_err(|e| map_request_error("StartRecord", e))
    }

    async fn stop_record(&self) -> Result<(), ObsError> {
        let guard = self.inner.read().await;
        let Some(client) = guard.as_ref() else {
            return Err(ObsError::Disconnected);
        };
        client
            .recording()
            .stop()
            .await
            .map(|_| ())
            .map_err(|e| map_request_error("StopRecord", e))
    }

    async fn start_stream(&self) -> Result<(), ObsError> {
        let guard = self.inner.read().await;
        let Some(client) = guard.as_ref() else {
            return Err(ObsError::Disconnected);
        };
        client
            .streaming()
            .start()
            .await
            .map_err(|e| map_request_error("StartStream", e))
    }

    async fn stop_stream(&self) -> Result<(), ObsError> {
        let guard = self.inner.read().await;
        let Some(client) = guard.as_ref() else {
            return Err(ObsError::Disconnected);
        };
        client
            .streaming()
            .stop()
            .await
            .map_err(|e| map_request_error("StopStream", e))
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
        let guard = self.inner.read().await;
        let Some(client) = guard.as_ref() else {
            return Err(ObsError::Disconnected);
        };
        client
            .scenes()
            .set_current_preview_scene(SceneId::Name(scene))
            .await
            .map_err(|e| map_request_error("SetCurrentPreviewScene", e))
    }

    async fn set_current_scene_transition(&self, name: &str) -> Result<(), ObsError> {
        let guard = self.inner.read().await;
        let Some(client) = guard.as_ref() else {
            return Err(ObsError::Disconnected);
        };
        client
            .transitions()
            .set_current(name)
            .await
            .map_err(|e| map_request_error("SetCurrentSceneTransition", e))
    }

    async fn set_input_volume_db(&self, input: &str, db: f64) -> Result<(), ObsError> {
        let guard = self.inner.read().await;
        let Some(client) = guard.as_ref() else {
            return Err(ObsError::Disconnected);
        };
        client
            .inputs()
            .set_volume(InputId::Name(input), Volume::Db(db as f32))
            .await
            .map_err(|e| map_request_error("SetInputVolume", e))?;
        drop(guard);

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
        let guard = self.inner.read().await;
        let Some(client) = guard.as_ref() else {
            return Err(ObsError::Disconnected);
        };
        client
            .inputs()
            .set_settings(SetSettings {
                input: InputId::Name(input),
                settings: &json_settings,
                overlay: Some(overlay),
            })
            .await
            .map_err(|e| map_request_error("SetInputSettings", e))
    }

    async fn pause_record(&self) -> Result<(), ObsError> {
        let guard = self.inner.read().await;
        let Some(client) = guard.as_ref() else {
            return Err(ObsError::Disconnected);
        };
        client
            .recording()
            .pause()
            .await
            .map_err(|e| map_request_error("PauseRecord", e))
    }

    async fn resume_record(&self) -> Result<(), ObsError> {
        let guard = self.inner.read().await;
        let Some(client) = guard.as_ref() else {
            return Err(ObsError::Disconnected);
        };
        client
            .recording()
            .resume()
            .await
            .map_err(|e| map_request_error("ResumeRecord", e))
    }

    async fn toggle_record_pause(&self) -> Result<(), ObsError> {
        let guard = self.inner.read().await;
        let Some(client) = guard.as_ref() else {
            return Err(ObsError::Disconnected);
        };
        client
            .recording()
            .toggle_pause()
            .await
            .map(|_| ())
            .map_err(|e| map_request_error("ToggleRecordPause", e))
    }

    async fn send_stream_caption(&self, text: &str) -> Result<(), ObsError> {
        let guard = self.inner.read().await;
        let Some(client) = guard.as_ref() else {
            return Err(ObsError::Disconnected);
        };
        client
            .streaming()
            .send_caption(text)
            .await
            .map_err(|e| map_request_error("SendStreamCaption", e))
    }

    async fn start_replay_buffer(&self) -> Result<(), ObsError> {
        let guard = self.inner.read().await;
        let Some(client) = guard.as_ref() else {
            return Err(ObsError::Disconnected);
        };
        client
            .replay_buffer()
            .start()
            .await
            .map_err(|e| map_request_error("StartReplayBuffer", e))
    }

    async fn stop_replay_buffer(&self) -> Result<(), ObsError> {
        let guard = self.inner.read().await;
        let Some(client) = guard.as_ref() else {
            return Err(ObsError::Disconnected);
        };
        client
            .replay_buffer()
            .stop()
            .await
            .map_err(|e| map_request_error("StopReplayBuffer", e))
    }

    async fn save_replay_buffer(&self) -> Result<(), ObsError> {
        let guard = self.inner.read().await;
        let Some(client) = guard.as_ref() else {
            return Err(ObsError::Disconnected);
        };
        client
            .replay_buffer()
            .save()
            .await
            .map_err(|e| map_request_error("SaveReplayBuffer", e))
    }

    async fn set_studio_mode(&self, enabled: bool) -> Result<(), ObsError> {
        let guard = self.inner.read().await;
        let Some(client) = guard.as_ref() else {
            return Err(ObsError::Disconnected);
        };
        client
            .ui()
            .set_studio_mode_enabled(enabled)
            .await
            .map_err(|e| map_request_error("SetStudioModeEnabled", e))
    }

    async fn trigger_studio_transition(&self) -> Result<(), ObsError> {
        let guard = self.inner.read().await;
        let Some(client) = guard.as_ref() else {
            return Err(ObsError::Disconnected);
        };
        client
            .transitions()
            .trigger()
            .await
            .map_err(|e| map_request_error("TriggerStudioModeTransition", e))
    }

    async fn get_scene_list(&self) -> Result<Variant, ObsError> {
        let guard = self.inner.read().await;
        let Some(client) = guard.as_ref() else {
            return Err(ObsError::Disconnected);
        };
        let scenes = client
            .scenes()
            .list()
            .await
            .map_err(|e| map_request_error("GetSceneList", e))?;
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
        let guard = self.inner.read().await;
        let Some(client) = guard.as_ref() else {
            return Err(ObsError::Disconnected);
        };
        let inputs = client
            .inputs()
            .list(None)
            .await
            .map_err(|e| map_request_error("GetInputList", e))?;
        let all_names: Vec<Variant> = inputs
            .iter()
            .map(|i| Variant::String(i.id.name.clone()))
            .collect();
        let mut obj = BTreeMap::new();
        obj.insert("all_names".to_owned(), Variant::Array(all_names));
        Ok(Variant::Object(obj))
    }

    async fn get_record_status(&self) -> Result<Variant, ObsError> {
        let guard = self.inner.read().await;
        let Some(client) = guard.as_ref() else {
            return Err(ObsError::Disconnected);
        };
        let status = client
            .recording()
            .status()
            .await
            .map_err(|e| map_request_error("GetRecordStatus", e))?;
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
        let guard = self.inner.read().await;
        let Some(client) = guard.as_ref() else {
            return Err(ObsError::Disconnected);
        };
        let status = client
            .streaming()
            .status()
            .await
            .map_err(|e| map_request_error("GetStreamStatus", e))?;
        let mut obj = BTreeMap::new();
        obj.insert("is_active".to_owned(), Variant::Bool(status.active));
        obj.insert(
            "duration_ms".to_owned(),
            Variant::Int(status.duration.whole_milliseconds() as i64),
        );
        Ok(Variant::Object(obj))
    }

    async fn get_input_settings(&self, input: &str) -> Result<Variant, ObsError> {
        let guard = self.inner.read().await;
        let Some(client) = guard.as_ref() else {
            return Err(ObsError::Disconnected);
        };
        let result = client
            .inputs()
            .settings::<serde_json::Value>(InputId::Name(input))
            .await
            .map_err(|e| map_request_error("GetInputSettings", e))?;
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
        let guard = self.inner.read().await;
        let Some(client) = guard.as_ref() else {
            return Err(ObsError::Disconnected);
        };
        client
            .filters()
            .set_enabled(FilterSetEnabled {
                source: SourceId::Name(source),
                filter,
                enabled,
            })
            .await
            .map_err(|e| map_request_error("SetSourceFilterEnabled", e))
    }

    async fn refresh_browser_source(&self, input: &str) -> Result<(), ObsError> {
        let guard = self.inner.read().await;
        let Some(client) = guard.as_ref() else {
            return Err(ObsError::Disconnected);
        };
        client
            .inputs()
            .press_properties_button(InputId::Name(input), "refreshnocache")
            .await
            .map_err(|e| map_request_error("PressInputPropertiesButton", e))
    }

    async fn restart_media_input(&self, input: &str) -> Result<(), ObsError> {
        let guard = self.inner.read().await;
        let Some(client) = guard.as_ref() else {
            return Err(ObsError::Disconnected);
        };
        client
            .media_inputs()
            .trigger_action(InputId::Name(input), MediaAction::Restart)
            .await
            .map_err(|e| map_request_error("TriggerMediaInputAction", e))
    }

    async fn start_virtual_cam(&self) -> Result<(), ObsError> {
        let guard = self.inner.read().await;
        let Some(client) = guard.as_ref() else {
            return Err(ObsError::Disconnected);
        };
        client
            .virtual_cam()
            .start()
            .await
            .map_err(|e| map_request_error("StartVirtualCam", e))
    }

    async fn stop_virtual_cam(&self) -> Result<(), ObsError> {
        let guard = self.inner.read().await;
        let Some(client) = guard.as_ref() else {
            return Err(ObsError::Disconnected);
        };
        client
            .virtual_cam()
            .stop()
            .await
            .map_err(|e| map_request_error("StopVirtualCam", e))
    }

    async fn save_source_screenshot(
        &self,
        source: &str,
        file_path: &str,
        format: &str,
    ) -> Result<(), ObsError> {
        let guard = self.inner.read().await;
        let Some(client) = guard.as_ref() else {
            return Err(ObsError::Disconnected);
        };
        client
            .sources()
            .save_screenshot(SaveScreenshot {
                source: SourceId::Name(source),
                format,
                width: None,
                height: None,
                compression_quality: None,
                file_path: Path::new(file_path),
            })
            .await
            .map_err(|e| map_request_error("SaveSourceScreenshot", e))
    }

    async fn set_record_directory(&self, path: &str) -> Result<(), ObsError> {
        let guard = self.inner.read().await;
        let Some(client) = guard.as_ref() else {
            return Err(ObsError::Disconnected);
        };
        client
            .config()
            .set_record_directory(path)
            .await
            .map_err(|e| map_request_error("SetRecordDirectory", e))
    }

    async fn set_current_profile(&self, name: &str) -> Result<(), ObsError> {
        let guard = self.inner.read().await;
        let Some(client) = guard.as_ref() else {
            return Err(ObsError::Disconnected);
        };
        client
            .profiles()
            .set_current(name)
            .await
            .map_err(|e| map_request_error("SetCurrentProfile", e))
    }

    async fn set_current_scene_collection(&self, name: &str) -> Result<(), ObsError> {
        let guard = self.inner.read().await;
        let Some(client) = guard.as_ref() else {
            return Err(ObsError::Disconnected);
        };
        client
            .scene_collections()
            .set_current(name)
            .await
            .map_err(|e| map_request_error("SetCurrentSceneCollection", e))
    }
}

fn map_request_error(request_type: &str, e: obws::error::Error) -> ObsError {
    match e {
        obws::error::Error::Timeout => ObsError::Timeout,
        obws::error::Error::Disconnected => ObsError::Disconnected,
        _ => ObsError::Request {
            request_type: request_type.to_owned(),
            message: e.to_string(),
        },
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
