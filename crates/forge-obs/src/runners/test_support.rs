use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_events::{Event, EventPublisher};
use forge_registry::RunContext;
use forge_types::{ArgStack, EventId, Variant};

use crate::{ObsError, ObsSink};

pub(crate) struct MockSink;

#[async_trait]
impl ObsSink for MockSink {
    async fn set_scene(&self, _: &str) -> Result<(), ObsError> {
        Ok(())
    }
    async fn set_source_visible(&self, _: &str, _: &str, _: bool) -> Result<(), ObsError> {
        Ok(())
    }
    async fn set_input_mute(&self, _: &str, _: bool) -> Result<(), ObsError> {
        Ok(())
    }
    async fn start_record(&self) -> Result<(), ObsError> {
        Ok(())
    }
    async fn stop_record(&self) -> Result<(), ObsError> {
        Ok(())
    }
    async fn start_stream(&self) -> Result<(), ObsError> {
        Ok(())
    }
    async fn stop_stream(&self) -> Result<(), ObsError> {
        Ok(())
    }
    async fn raw_request(&self, _: &str, _: &Variant) -> Result<Variant, ObsError> {
        Ok(Variant::Object(BTreeMap::new()))
    }
    async fn set_preview_scene(&self, _: &str) -> Result<(), ObsError> {
        Ok(())
    }
    async fn set_current_scene_transition(&self, _: &str) -> Result<(), ObsError> {
        Ok(())
    }
    async fn set_input_volume_db(&self, _: &str, _: f64) -> Result<(), ObsError> {
        Ok(())
    }
    async fn set_input_settings(&self, _: &str, _: &Variant, _: bool) -> Result<(), ObsError> {
        Ok(())
    }
    async fn pause_record(&self) -> Result<(), ObsError> {
        Ok(())
    }
    async fn resume_record(&self) -> Result<(), ObsError> {
        Ok(())
    }
    async fn toggle_record_pause(&self) -> Result<(), ObsError> {
        Ok(())
    }
    async fn send_stream_caption(&self, _: &str) -> Result<(), ObsError> {
        Ok(())
    }
    async fn start_replay_buffer(&self) -> Result<(), ObsError> {
        Ok(())
    }
    async fn stop_replay_buffer(&self) -> Result<(), ObsError> {
        Ok(())
    }
    async fn save_replay_buffer(&self) -> Result<(), ObsError> {
        Ok(())
    }
    async fn set_studio_mode(&self, _: bool) -> Result<(), ObsError> {
        Ok(())
    }
    async fn trigger_studio_transition(&self) -> Result<(), ObsError> {
        Ok(())
    }
    async fn get_scene_list(&self) -> Result<Variant, ObsError> {
        let mut obj = BTreeMap::new();
        obj.insert(
            "all_names".to_owned(),
            Variant::Array(vec![
                Variant::String("Intro".to_owned()),
                Variant::String("Gameplay".to_owned()),
            ]),
        );
        obj.insert("current".to_owned(), Variant::String("Gameplay".to_owned()));
        Ok(Variant::Object(obj))
    }
    async fn get_input_list(&self) -> Result<Variant, ObsError> {
        let mut obj = BTreeMap::new();
        obj.insert(
            "all_names".to_owned(),
            Variant::Array(vec![
                Variant::String("Mic".to_owned()),
                Variant::String("Desktop Audio".to_owned()),
            ]),
        );
        Ok(Variant::Object(obj))
    }
    async fn get_record_status(&self) -> Result<Variant, ObsError> {
        let mut obj = BTreeMap::new();
        obj.insert("is_active".to_owned(), Variant::Bool(true));
        obj.insert("is_paused".to_owned(), Variant::Bool(false));
        obj.insert("duration_ms".to_owned(), Variant::Int(12_000));
        Ok(Variant::Object(obj))
    }
    async fn get_stream_status(&self) -> Result<Variant, ObsError> {
        let mut obj = BTreeMap::new();
        obj.insert("is_active".to_owned(), Variant::Bool(true));
        obj.insert("duration_ms".to_owned(), Variant::Int(45_000));
        Ok(Variant::Object(obj))
    }
    async fn get_input_settings(&self, _: &str) -> Result<Variant, ObsError> {
        let mut settings = BTreeMap::new();
        settings.insert("text".to_owned(), Variant::String("hello".to_owned()));
        let mut obj = BTreeMap::new();
        obj.insert(
            "kind".to_owned(),
            Variant::String("text_ft2_source_v2".to_owned()),
        );
        obj.insert("settings".to_owned(), Variant::Object(settings));
        Ok(Variant::Object(obj))
    }
    async fn set_source_filter_enabled(&self, _: &str, _: &str, _: bool) -> Result<(), ObsError> {
        Ok(())
    }
    async fn refresh_browser_source(&self, _: &str) -> Result<(), ObsError> {
        Ok(())
    }
    async fn restart_media_input(&self, _: &str) -> Result<(), ObsError> {
        Ok(())
    }
    async fn start_virtual_cam(&self) -> Result<(), ObsError> {
        Ok(())
    }
    async fn stop_virtual_cam(&self) -> Result<(), ObsError> {
        Ok(())
    }
    async fn save_source_screenshot(&self, _: &str, _: &str, _: &str) -> Result<(), ObsError> {
        Ok(())
    }
    async fn set_record_directory(&self, _: &str) -> Result<(), ObsError> {
        Ok(())
    }
    async fn set_current_profile(&self, _: &str) -> Result<(), ObsError> {
        Ok(())
    }
    async fn set_current_scene_collection(&self, _: &str) -> Result<(), ObsError> {
        Ok(())
    }
}

/// Records every sink call as `"method(arg, arg)"` so a runner test can assert both WHICH sink
/// method fired and that exactly one fired. `failing()` makes every call return
/// `ObsError::Disconnected` so the telemetry error path is reachable without a live OBS.
pub(crate) struct RecordingSink {
    calls: Mutex<Vec<String>>,
    fail: bool,
}

impl RecordingSink {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            calls: Mutex::new(Vec::new()),
            fail: false,
        })
    }

    pub(crate) fn failing() -> Arc<Self> {
        Arc::new(Self {
            calls: Mutex::new(Vec::new()),
            fail: true,
        })
    }

    pub(crate) fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    fn record(&self, call: String) -> Result<(), ObsError> {
        self.calls
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(call);
        if self.fail {
            Err(ObsError::Disconnected)
        } else {
            Ok(())
        }
    }

    fn record_query(&self, call: String) -> Result<Variant, ObsError> {
        self.record(call)?;
        Ok(Variant::Object(BTreeMap::new()))
    }
}

#[async_trait]
impl ObsSink for RecordingSink {
    async fn set_scene(&self, scene: &str) -> Result<(), ObsError> {
        self.record(format!("set_scene({scene})"))
    }
    async fn set_source_visible(
        &self,
        scene: &str,
        source: &str,
        visible: bool,
    ) -> Result<(), ObsError> {
        self.record(format!("set_source_visible({scene},{source},{visible})"))
    }
    async fn set_input_mute(&self, input: &str, mute: bool) -> Result<(), ObsError> {
        self.record(format!("set_input_mute({input},{mute})"))
    }
    async fn start_record(&self) -> Result<(), ObsError> {
        self.record("start_record".to_owned())
    }
    async fn stop_record(&self) -> Result<(), ObsError> {
        self.record("stop_record".to_owned())
    }
    async fn start_stream(&self) -> Result<(), ObsError> {
        self.record("start_stream".to_owned())
    }
    async fn stop_stream(&self) -> Result<(), ObsError> {
        self.record("stop_stream".to_owned())
    }
    async fn raw_request(&self, request_type: &str, _: &Variant) -> Result<Variant, ObsError> {
        self.record_query(format!("raw_request({request_type})"))
    }
    async fn set_preview_scene(&self, scene: &str) -> Result<(), ObsError> {
        self.record(format!("set_preview_scene({scene})"))
    }
    async fn set_current_scene_transition(&self, name: &str) -> Result<(), ObsError> {
        self.record(format!("set_current_scene_transition({name})"))
    }
    async fn set_input_volume_db(&self, input: &str, db: f64) -> Result<(), ObsError> {
        self.record(format!("set_input_volume_db({input},{db})"))
    }
    async fn set_input_settings(&self, input: &str, _: &Variant, _: bool) -> Result<(), ObsError> {
        self.record(format!("set_input_settings({input})"))
    }
    async fn pause_record(&self) -> Result<(), ObsError> {
        self.record("pause_record".to_owned())
    }
    async fn resume_record(&self) -> Result<(), ObsError> {
        self.record("resume_record".to_owned())
    }
    async fn toggle_record_pause(&self) -> Result<(), ObsError> {
        self.record("toggle_record_pause".to_owned())
    }
    async fn send_stream_caption(&self, text: &str) -> Result<(), ObsError> {
        self.record(format!("send_stream_caption({text})"))
    }
    async fn start_replay_buffer(&self) -> Result<(), ObsError> {
        self.record("start_replay_buffer".to_owned())
    }
    async fn stop_replay_buffer(&self) -> Result<(), ObsError> {
        self.record("stop_replay_buffer".to_owned())
    }
    async fn save_replay_buffer(&self) -> Result<(), ObsError> {
        self.record("save_replay_buffer".to_owned())
    }
    async fn set_studio_mode(&self, enabled: bool) -> Result<(), ObsError> {
        self.record(format!("set_studio_mode({enabled})"))
    }
    async fn trigger_studio_transition(&self) -> Result<(), ObsError> {
        self.record("trigger_studio_transition".to_owned())
    }
    async fn get_scene_list(&self) -> Result<Variant, ObsError> {
        self.record_query("get_scene_list".to_owned())
    }
    async fn get_input_list(&self) -> Result<Variant, ObsError> {
        self.record_query("get_input_list".to_owned())
    }
    async fn get_record_status(&self) -> Result<Variant, ObsError> {
        self.record_query("get_record_status".to_owned())
    }
    async fn get_stream_status(&self) -> Result<Variant, ObsError> {
        self.record_query("get_stream_status".to_owned())
    }
    async fn get_input_settings(&self, input: &str) -> Result<Variant, ObsError> {
        self.record_query(format!("get_input_settings({input})"))
    }
    async fn set_source_filter_enabled(
        &self,
        source: &str,
        filter: &str,
        enabled: bool,
    ) -> Result<(), ObsError> {
        self.record(format!(
            "set_source_filter_enabled({source},{filter},{enabled})"
        ))
    }
    async fn refresh_browser_source(&self, input: &str) -> Result<(), ObsError> {
        self.record(format!("refresh_browser_source({input})"))
    }
    async fn restart_media_input(&self, input: &str) -> Result<(), ObsError> {
        self.record(format!("restart_media_input({input})"))
    }
    async fn start_virtual_cam(&self) -> Result<(), ObsError> {
        self.record("start_virtual_cam".to_owned())
    }
    async fn stop_virtual_cam(&self) -> Result<(), ObsError> {
        self.record("stop_virtual_cam".to_owned())
    }
    async fn save_source_screenshot(
        &self,
        source: &str,
        file_path: &str,
        format: &str,
    ) -> Result<(), ObsError> {
        self.record(format!(
            "save_source_screenshot({source},{file_path},{format})"
        ))
    }
    async fn set_record_directory(&self, path: &str) -> Result<(), ObsError> {
        self.record(format!("set_record_directory({path})"))
    }
    async fn set_current_profile(&self, name: &str) -> Result<(), ObsError> {
        self.record(format!("set_current_profile({name})"))
    }
    async fn set_current_scene_collection(&self, name: &str) -> Result<(), ObsError> {
        self.record(format!("set_current_scene_collection({name})"))
    }
}

pub(crate) struct NoopPublisher;

impl EventPublisher for NoopPublisher {
    fn publish(&self, _: Event) {}
}

pub(crate) fn make_ctx(stack: &ArgStack) -> RunContext<'_> {
    RunContext::leaf(stack, 0, EventId::new(), &NoopPublisher)
}
