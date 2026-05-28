use forge_types::{ActionId, ClipId, LogLevel, QueueId, SubActionStep, Variant};

pub struct AddActionForm {
    pub name: String,
    pub group: String,
    pub queue_id: Option<QueueId>,
    pub description: String,
    pub enabled: bool,
    pub concurrent: bool,
    pub bypass_pause: bool,
    pub random_pick: bool,
    pub queue_options: Vec<(QueueId, String)>,
    pub selected_queue_name: Option<String>,
    pub error: Option<String>,
    pub saving: bool,
}

impl AddActionForm {
    pub fn new() -> Self {
        Self {
            name: String::new(),
            group: String::new(),
            queue_id: None,
            description: String::new(),
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            random_pick: false,
            queue_options: vec![],
            selected_queue_name: None,
            error: None,
            saving: false,
        }
    }

    pub fn set_queue_options(&mut self, opts: Vec<(QueueId, String)>) {
        let default = opts
            .iter()
            .find(|(_, n)| n.eq_ignore_ascii_case("default"))
            .cloned();
        self.queue_options = opts;
        if let Some((id, name)) = default {
            self.queue_id = Some(id);
            self.selected_queue_name = Some(name);
        }
    }

    pub fn select_queue_by_name(&mut self, name: String) {
        let found = self.queue_options.iter().find(|(_, n)| *n == name);
        if let Some((id, _)) = found {
            self.queue_id = Some(*id);
        }
        self.selected_queue_name = Some(name);
    }

    pub fn is_valid(&self) -> bool {
        !self.name.trim().is_empty() && self.queue_id.is_some()
    }
}

impl Default for AddActionForm {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub enum AddActionMsg {
    OpenRequested,
    QueueOptionsLoaded(Result<Vec<(QueueId, String)>, String>),
    NameChanged(String),
    GroupChanged(String),
    QueueSelected(String),
    DescriptionChanged(String),
    EnabledToggled(bool),
    ConcurrentToggled(bool),
    BypassPauseToggled(bool),
    RandomPickToggled(bool),
    Cancel,
    Submit,
    Saved(Result<ActionId, String>),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SubActionKindChoice {
    #[default]
    SendChat,
    SetGlobal,
    Delay,
    Log,
    PlaySound,
    Speak,
    ReadFile,
    RandomInt,
}

#[derive(Debug, Clone)]
pub struct SubActionConfigForm {
    pub send_chat_message: String,
    pub send_chat_target: String,
    pub set_global_name: String,
    pub set_global_value: String,
    pub delay_ms: String,
    pub log_level: LogLevel,
    pub log_message: String,
    pub play_sound_clip_id: Option<ClipId>,
    pub speak_text: String,
    pub speak_voice_override: String,
    pub read_file_path: String,
    pub read_file_target_var: String,
    pub random_int_min: String,
    pub random_int_max: String,
    pub random_int_target_var: String,
}

impl Default for SubActionConfigForm {
    fn default() -> Self {
        Self {
            send_chat_message: String::new(),
            send_chat_target: "twitch".to_string(),
            set_global_name: String::new(),
            set_global_value: String::new(),
            delay_ms: "500".to_string(),
            log_level: LogLevel::Info,
            log_message: String::new(),
            play_sound_clip_id: None,
            speak_text: String::new(),
            speak_voice_override: String::new(),
            read_file_path: String::new(),
            read_file_target_var: String::new(),
            random_int_min: "1".to_string(),
            random_int_max: "100".to_string(),
            random_int_target_var: String::new(),
        }
    }
}

#[derive(Debug)]
pub struct AddSubActionForm {
    pub for_action_id: ActionId,
    pub kind: SubActionKindChoice,
    pub config: SubActionConfigForm,
    pub available_clips: Vec<(ClipId, String)>,
    pub error: Option<String>,
    pub saving: bool,
    pub editing_index: Option<usize>,
}

impl AddSubActionForm {
    pub fn new(for_action_id: ActionId) -> Self {
        Self {
            for_action_id,
            kind: SubActionKindChoice::SendChat,
            config: SubActionConfigForm::default(),
            available_clips: vec![],
            error: None,
            saving: false,
            editing_index: None,
        }
    }

    pub fn populate_from_step(&mut self, step: &SubActionStep) {
        fn as_str(v: &Variant) -> Option<&str> {
            if let Variant::String(s) = v {
                Some(s.as_str())
            } else {
                None
            }
        }
        fn as_i64(v: &Variant) -> Option<i64> {
            if let Variant::Int(n) = v {
                Some(*n)
            } else {
                None
            }
        }
        match step.kind_id.as_str() {
            "twitch.chat.send_message" => {
                self.kind = SubActionKindChoice::SendChat;
                self.config.send_chat_target = step
                    .config
                    .get("target")
                    .and_then(as_str)
                    .unwrap_or("twitch")
                    .to_owned();
                self.config.send_chat_message = step
                    .config
                    .get("message")
                    .and_then(as_str)
                    .unwrap_or("")
                    .to_owned();
            }
            "core.globals.set" => {
                self.kind = SubActionKindChoice::SetGlobal;
                self.config.set_global_name = step
                    .config
                    .get("name")
                    .and_then(as_str)
                    .unwrap_or("")
                    .to_owned();
                self.config.set_global_value = step
                    .config
                    .get("value")
                    .and_then(as_str)
                    .unwrap_or("")
                    .to_owned();
            }
            "core.logic.wait" => {
                self.kind = SubActionKindChoice::Delay;
                self.config.delay_ms = step
                    .config
                    .get("ms")
                    .and_then(as_i64)
                    .unwrap_or(500)
                    .to_string();
            }
            "core.log.write" => {
                self.kind = SubActionKindChoice::Log;
                let level_str = step.config.get("level").and_then(as_str).unwrap_or("info");
                self.config.log_level = log_level_from_id(level_str);
                self.config.log_message = step
                    .config
                    .get("message")
                    .and_then(as_str)
                    .unwrap_or("")
                    .to_owned();
            }
            "soundboard.sound.play" => {
                self.kind = SubActionKindChoice::PlaySound;
                if let Some(id_str) = step.config.get("clip_id").and_then(as_str) {
                    let quoted = format!("\"{}\"", id_str);
                    if let Ok(id) = serde_json::from_str::<ClipId>(&quoted) {
                        self.config.play_sound_clip_id = Some(id);
                    }
                }
            }
            "tts.speak.text" => {
                self.kind = SubActionKindChoice::Speak;
                self.config.speak_text = step
                    .config
                    .get("text")
                    .and_then(as_str)
                    .unwrap_or("")
                    .to_owned();
                self.config.speak_voice_override = step
                    .config
                    .get("voice_id_override")
                    .and_then(as_str)
                    .unwrap_or("")
                    .to_owned();
            }
            "core.file.read" => {
                self.kind = SubActionKindChoice::ReadFile;
                self.config.read_file_path = step
                    .config
                    .get("path")
                    .and_then(as_str)
                    .unwrap_or("")
                    .to_owned();
                self.config.read_file_target_var = step
                    .config
                    .get("target_var")
                    .and_then(as_str)
                    .unwrap_or("")
                    .to_owned();
            }
            "core.random.int" => {
                self.kind = SubActionKindChoice::RandomInt;
                self.config.random_int_min = step
                    .config
                    .get("min")
                    .and_then(as_i64)
                    .unwrap_or(1)
                    .to_string();
                self.config.random_int_max = step
                    .config
                    .get("max")
                    .and_then(as_i64)
                    .unwrap_or(100)
                    .to_string();
                self.config.random_int_target_var = step
                    .config
                    .get("target_var")
                    .and_then(as_str)
                    .unwrap_or("")
                    .to_owned();
            }
            _ => {}
        }
    }

    pub fn is_valid(&self) -> bool {
        match self.kind {
            SubActionKindChoice::SendChat => !self.config.send_chat_message.trim().is_empty(),
            SubActionKindChoice::SetGlobal => !self.config.set_global_name.trim().is_empty(),
            SubActionKindChoice::Delay => self.config.delay_ms.trim().parse::<u64>().is_ok(),
            SubActionKindChoice::Log => !self.config.log_message.trim().is_empty(),
            SubActionKindChoice::PlaySound => self.config.play_sound_clip_id.is_some(),
            SubActionKindChoice::Speak => !self.config.speak_text.trim().is_empty(),
            SubActionKindChoice::ReadFile => {
                !self.config.read_file_path.trim().is_empty()
                    && !self.config.read_file_target_var.trim().is_empty()
            }
            SubActionKindChoice::RandomInt => {
                let min = self.config.random_int_min.trim().parse::<i64>().ok();
                let max = self.config.random_int_max.trim().parse::<i64>().ok();
                let target_ok = !self.config.random_int_target_var.trim().is_empty();
                matches!((min, max), (Some(lo), Some(hi)) if lo <= hi) && target_ok
            }
        }
    }
}

fn log_level_from_id(id: &str) -> LogLevel {
    match id {
        "trace" => LogLevel::Trace,
        "debug" => LogLevel::Debug,
        "warn" => LogLevel::Warn,
        "error" => LogLevel::Error,
        _ => LogLevel::Info,
    }
}

#[derive(Debug, Clone)]
pub enum AddSubActionMsg {
    OpenRequested(ActionId),
    KindSelected(SubActionKindChoice),
    SendChatMessageChanged(String),
    SendChatTargetChanged(String),
    SetGlobalNameChanged(String),
    SetGlobalValueChanged(String),
    DelayMsChanged(String),
    LogLevelSelected(LogLevel),
    LogMessageChanged(String),
    PlaySoundClipSelected(ClipId),
    SpeakTextChanged(String),
    SpeakVoiceOverrideChanged(String),
    ReadFilePathChanged(String),
    ReadFileTargetVarChanged(String),
    RandomIntMinChanged(String),
    RandomIntMaxChanged(String),
    RandomIntTargetVarChanged(String),
    ClipsLoaded(Vec<(ClipId, String)>),
    Cancel,
    Submit,
    Saved(Result<(), String>),
    DuplicateRequested(ActionId, usize),
    Duplicated(Result<ActionId, String>),
    EditRequested(ActionId, usize),
}

#[derive(Debug, Clone)]
pub enum RemoveSubActionMsg {
    Requested(ActionId, usize),
    Removed(Result<(), String>),
}
