use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    ConfigProblem, ConfigSpec, PickedTextField, RunContext, StepCatalogEntry, StepTimer,
    SubActionCategory, SubActionConfig, SubActionRunner, TextEditor, TextField, TextPicker,
    TypedField,
};
use forge_types::{ArgStack, SubActionTelemetry};
use futures_util::future::BoxFuture;

use super::step_failure;
use crate::runners::fields::{ACTIVE, SCENE};
use crate::vocabulary::{config_keys, picker_keys, sub_action_ids};
use crate::{ObsError, ObsSink};

type SinkCall = for<'a> fn(&'a dyn ObsSink) -> BoxFuture<'a, Result<(), ObsError>>;
type NamedSinkCall = for<'a> fn(&'a dyn ObsSink, &'a str) -> BoxFuture<'a, Result<(), ObsError>>;
type ReadName = fn(&SubActionConfig, &ArgStack) -> Result<String, ConfigProblem>;

struct CommandCopy {
    id: &'static str,
    label: &'static str,
    summary: &'static str,
    search_text: &'static str,
    icon_name: &'static str,
}

struct SignalCommand {
    copy: CommandCopy,
    send: SinkCall,
}

struct ActiveCommand {
    copy: CommandCopy,
    start: SinkCall,
    stop: SinkCall,
}

struct NamedCommand {
    copy: CommandCopy,
    config: ConfigSpec,
    read: ReadName,
    send: NamedSinkCall,
}

const PROFILE_NAME: TextField = TextField {
    key: config_keys::NAME,
    label: "Profile Name",
    editor: TextEditor::Line {
        placeholder: "e.g. Streaming",
    },
    default: "",
    required: true,
};

const COLLECTION_NAME: TextField = TextField {
    key: config_keys::NAME,
    label: "Collection Name",
    editor: TextEditor::Line {
        placeholder: "e.g. Main Collection",
    },
    default: "",
    required: true,
};

const BROWSER_SOURCE: PickedTextField = PickedTextField {
    key: config_keys::SOURCE,
    label: "Browser Source",
    picker: TextPicker::Options {
        options_key: picker_keys::INPUT_NAMES,
    },
    default: "",
    required: true,
};

const MEDIA_SOURCE: PickedTextField = PickedTextField {
    key: config_keys::SOURCE,
    label: "Media Input",
    picker: TextPicker::Options {
        options_key: picker_keys::INPUT_NAMES,
    },
    default: "",
    required: true,
};

const TRANSITION_NAME: TextField = TextField {
    key: config_keys::TRANSITION,
    label: "Transition Name",
    editor: TextEditor::Line {
        placeholder: "e.g. Fade",
    },
    default: "",
    required: true,
};

const SIGNAL_COMMANDS: &[SignalCommand] = &[
    SignalCommand {
        copy: CommandCopy {
            id: sub_action_ids::RECORDING_START,
            label: "Start Recording",
            summary: "Starts OBS recording output.",
            search_text: "obs record start recording begin capture",
            icon_name: "record",
        },
        send: |sink| sink.start_record(),
    },
    SignalCommand {
        copy: CommandCopy {
            id: sub_action_ids::RECORDING_STOP,
            label: "Stop Recording",
            summary: "Stops OBS recording output.",
            search_text: "obs record stop recording end finish",
            icon_name: "player-stop",
        },
        send: |sink| sink.stop_record(),
    },
    SignalCommand {
        copy: CommandCopy {
            id: sub_action_ids::RECORDING_PAUSE,
            label: "Pause Recording",
            summary: "Pauses the OBS recording output.",
            search_text: "obs record pause recording hold",
            icon_name: "player-pause",
        },
        send: |sink| sink.pause_record(),
    },
    SignalCommand {
        copy: CommandCopy {
            id: sub_action_ids::RECORDING_RESUME,
            label: "Resume Recording",
            summary: "Resumes the OBS recording output after a pause.",
            search_text: "obs record resume recording continue unpause",
            icon_name: "player-play",
        },
        send: |sink| sink.resume_record(),
    },
    SignalCommand {
        copy: CommandCopy {
            id: sub_action_ids::RECORDING_TOGGLE_PAUSE,
            label: "Toggle Recording Pause",
            summary: "Toggles the pause state of the OBS recording output.",
            search_text: "obs record toggle pause resume unpause",
            icon_name: "player-pause",
        },
        send: |sink| sink.toggle_record_pause(),
    },
    SignalCommand {
        copy: CommandCopy {
            id: sub_action_ids::STREAMING_START,
            label: "Start Stream",
            summary: "Starts OBS streaming output.",
            search_text: "obs stream start streaming go live broadcast",
            icon_name: "broadcast",
        },
        send: |sink| sink.start_stream(),
    },
    SignalCommand {
        copy: CommandCopy {
            id: sub_action_ids::STREAMING_STOP,
            label: "Stop Stream",
            summary: "Stops OBS streaming output.",
            search_text: "obs stream stop streaming end live broadcast",
            icon_name: "player-stop",
        },
        send: |sink| sink.stop_stream(),
    },
    SignalCommand {
        copy: CommandCopy {
            id: sub_action_ids::REPLAY_START,
            label: "Start Replay Buffer",
            summary: "Starts the OBS replay buffer output.",
            search_text: "obs replay buffer start begin",
            icon_name: "player-record",
        },
        send: |sink| sink.start_replay_buffer(),
    },
    SignalCommand {
        copy: CommandCopy {
            id: sub_action_ids::REPLAY_STOP,
            label: "Stop Replay Buffer",
            summary: "Stops the OBS replay buffer output.",
            search_text: "obs replay buffer stop end halt",
            icon_name: "player-stop",
        },
        send: |sink| sink.stop_replay_buffer(),
    },
    SignalCommand {
        copy: CommandCopy {
            id: sub_action_ids::REPLAY_SAVE,
            label: "Save Replay Buffer",
            summary: "Saves the current contents of the OBS replay buffer to disk.",
            search_text: "obs replay buffer save clip export",
            icon_name: "device-floppy",
        },
        send: |sink| sink.save_replay_buffer(),
    },
    SignalCommand {
        copy: CommandCopy {
            id: sub_action_ids::STUDIO_ENABLE,
            label: "Enable Studio Mode",
            summary: "Enables OBS studio mode.",
            search_text: "obs studio mode enable on activate preview",
            icon_name: "layout-sidebar",
        },
        send: |sink| sink.set_studio_mode(true),
    },
    SignalCommand {
        copy: CommandCopy {
            id: sub_action_ids::STUDIO_DISABLE,
            label: "Disable Studio Mode",
            summary: "Disables OBS studio mode.",
            search_text: "obs studio mode disable off deactivate preview",
            icon_name: "layout-sidebar-right-collapse",
        },
        send: |sink| sink.set_studio_mode(false),
    },
    SignalCommand {
        copy: CommandCopy {
            id: sub_action_ids::STUDIO_TRIGGER_TRANSITION,
            label: "Trigger Studio Transition",
            summary: "Triggers the current transition in OBS studio mode, moving preview to program.",
            search_text: "obs studio mode transition trigger cut switch program preview",
            icon_name: "arrows-right-left",
        },
        send: |sink| sink.trigger_studio_transition(),
    },
];

const ACTIVE_COMMANDS: &[ActiveCommand] = &[
    ActiveCommand {
        copy: CommandCopy {
            id: sub_action_ids::STREAMING_SET_ACTIVE,
            label: "Set Stream Active",
            summary: "Starts or stops the OBS stream output to an explicit state.",
            search_text: "obs stream start stop set on off active broadcast",
            icon_name: "broadcast",
        },
        start: |sink| sink.start_stream(),
        stop: |sink| sink.stop_stream(),
    },
    ActiveCommand {
        copy: CommandCopy {
            id: sub_action_ids::RECORDING_SET_ACTIVE,
            label: "Set Record Active",
            summary: "Starts or stops the OBS record output to an explicit state.",
            search_text: "obs record start stop set on off active",
            icon_name: "record",
        },
        start: |sink| sink.start_record(),
        stop: |sink| sink.stop_record(),
    },
    ActiveCommand {
        copy: CommandCopy {
            id: sub_action_ids::REPLAY_SET_ACTIVE,
            label: "Set Replay Buffer Active",
            summary: "Starts or stops the OBS replay buffer to an explicit state.",
            search_text: "obs replay buffer start stop set on off active",
            icon_name: "player-record",
        },
        start: |sink| sink.start_replay_buffer(),
        stop: |sink| sink.stop_replay_buffer(),
    },
    ActiveCommand {
        copy: CommandCopy {
            id: sub_action_ids::VIRTUALCAM_SET_ACTIVE,
            label: "Set Virtual Camera Active",
            summary: "Starts or stops the OBS virtual camera output to an explicit state.",
            search_text: "obs virtual camera cam start stop set on off active",
            icon_name: "camera",
        },
        start: |sink| sink.start_virtual_cam(),
        stop: |sink| sink.stop_virtual_cam(),
    },
];

const NAMED_COMMANDS: &[NamedCommand] = &[
    NamedCommand {
        copy: CommandCopy {
            id: sub_action_ids::PROFILE_SWITCH,
            label: "Switch Profile",
            summary: "Switches OBS to a different profile by name.",
            search_text: "obs profile switch change settings config",
            icon_name: "user-cog",
        },
        config: ConfigSpec::new(&[&PROFILE_NAME]),
        read: |config, stack| PROFILE_NAME.read(config, stack),
        send: |sink, name| sink.set_current_profile(name),
    },
    NamedCommand {
        copy: CommandCopy {
            id: sub_action_ids::COLLECTION_SWITCH,
            label: "Switch Scene Collection",
            summary: "Switches OBS to a different scene collection by name.",
            search_text: "obs scene collection switch change load",
            icon_name: "stack-2",
        },
        config: ConfigSpec::new(&[&COLLECTION_NAME]),
        read: |config, stack| COLLECTION_NAME.read(config, stack),
        send: |sink, name| sink.set_current_scene_collection(name),
    },
    NamedCommand {
        copy: CommandCopy {
            id: sub_action_ids::BROWSER_REFRESH,
            label: "Refresh Browser Source",
            summary: "Refreshes a browser source, bypassing its cache.",
            search_text: "obs browser source refresh reload cache nocache",
            icon_name: "refresh",
        },
        config: ConfigSpec::new(&[&BROWSER_SOURCE]),
        read: |config, stack| BROWSER_SOURCE.read(config, stack),
        send: |sink, name| sink.refresh_browser_source(name),
    },
    NamedCommand {
        copy: CommandCopy {
            id: sub_action_ids::MEDIA_RESTART,
            label: "Restart Media Source",
            summary: "Restarts playback of an OBS media input from the beginning.",
            search_text: "obs media restart replay video source input playback",
            icon_name: "player-track-prev",
        },
        config: ConfigSpec::new(&[&MEDIA_SOURCE]),
        read: |config, stack| MEDIA_SOURCE.read(config, stack),
        send: |sink, name| sink.restart_media_input(name),
    },
    NamedCommand {
        copy: CommandCopy {
            id: sub_action_ids::SCENE_SET_PREVIEW,
            label: "Set Preview Scene",
            summary: "Sets the current preview scene in OBS Studio Mode.",
            search_text: "obs scene preview studio mode set switch",
            icon_name: "monitor",
        },
        config: ConfigSpec::new(&[&SCENE]),
        read: |config, stack| SCENE.read(config, stack),
        send: |sink, name| sink.set_preview_scene(name),
    },
    NamedCommand {
        copy: CommandCopy {
            id: sub_action_ids::SCENE_SET_TRANSITION,
            label: "Set Scene Transition",
            summary: "Sets the active scene transition by name.",
            search_text: "obs transition fade cut swipe set scene change",
            icon_name: "shuffle",
        },
        config: ConfigSpec::new(&[&TRANSITION_NAME]),
        read: |config, stack| TRANSITION_NAME.read(config, stack),
        send: |sink, name| sink.set_current_scene_transition(name),
    },
];

const ACTIVE_CONFIG: ConfigSpec = ConfigSpec::new(&[&ACTIVE]);

#[async_trait]
trait ObsCommand: Send + Sync + 'static {
    fn copy(&self) -> &CommandCopy;

    fn config_spec(&self) -> Option<ConfigSpec> {
        None
    }

    async fn run(
        &self,
        sink: &dyn ObsSink,
        config: &SubActionConfig,
        timer: StepTimer,
        stack: &ArgStack,
    ) -> SubActionTelemetry;
}

#[async_trait]
impl ObsCommand for SignalCommand {
    fn copy(&self) -> &CommandCopy {
        &self.copy
    }

    async fn run(
        &self,
        sink: &dyn ObsSink,
        _config: &SubActionConfig,
        timer: StepTimer,
        _stack: &ArgStack,
    ) -> SubActionTelemetry {
        timer.finish(step_failure::outcome(&(self.send)(sink).await))
    }
}

#[async_trait]
impl ObsCommand for ActiveCommand {
    fn copy(&self) -> &CommandCopy {
        &self.copy
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(ACTIVE_CONFIG)
    }

    async fn run(
        &self,
        sink: &dyn ObsSink,
        config: &SubActionConfig,
        timer: StepTimer,
        stack: &ArgStack,
    ) -> SubActionTelemetry {
        let on = match ACTIVE.read(config, stack) {
            Ok(on) => on,
            Err(problem) => return timer.invalid_config(&problem),
        };
        let send = if on { self.start } else { self.stop };
        timer.finish(step_failure::outcome(&send(sink).await))
    }
}

#[async_trait]
impl ObsCommand for NamedCommand {
    fn copy(&self) -> &CommandCopy {
        &self.copy
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(self.config)
    }

    async fn run(
        &self,
        sink: &dyn ObsSink,
        config: &SubActionConfig,
        timer: StepTimer,
        stack: &ArgStack,
    ) -> SubActionTelemetry {
        let name = match (self.read)(config, stack) {
            Ok(name) => name,
            Err(problem) => return timer.invalid_config(&problem),
        };
        timer.finish(step_failure::outcome(&(self.send)(sink, &name).await))
    }
}

struct CommandRunner<C: 'static> {
    command: &'static C,
    sink: Arc<dyn ObsSink>,
}

impl<C: ObsCommand> StepCatalogEntry for CommandRunner<C> {
    fn category(&self) -> SubActionCategory {
        SubActionCategory::Integration
    }
    fn label(&self) -> &str {
        self.command.copy().label
    }
    fn summary(&self) -> &str {
        self.command.copy().summary
    }
    fn search_text(&self) -> &str {
        self.command.copy().search_text
    }
    fn icon_name(&self) -> &str {
        self.command.copy().icon_name
    }
}

#[async_trait]
impl<C: ObsCommand> SubActionRunner for CommandRunner<C> {
    fn id(&self) -> &str {
        self.command.copy().id
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        self.command.config_spec()
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.command.copy().id);
        let telemetry = self
            .command
            .run(self.sink.as_ref(), config, timer, ctx.arg_stack)
            .await;
        (telemetry, None)
    }
}

fn runners_for<C: ObsCommand>(
    commands: &'static [C],
    sink: &Arc<dyn ObsSink>,
) -> impl Iterator<Item = Box<dyn SubActionRunner>> {
    let sink = Arc::clone(sink);
    commands.iter().map(move |command| {
        Box::new(CommandRunner {
            command,
            sink: Arc::clone(&sink),
        }) as Box<dyn SubActionRunner>
    })
}

pub(super) fn command_runners(
    sink: &Arc<dyn ObsSink>,
) -> impl Iterator<Item = Box<dyn SubActionRunner>> {
    runners_for(SIGNAL_COMMANDS, sink)
        .chain(runners_for(ACTIVE_COMMANDS, sink))
        .chain(runners_for(NAMED_COMMANDS, sink))
}
