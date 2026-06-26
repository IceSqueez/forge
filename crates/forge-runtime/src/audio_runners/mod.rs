mod alias_set;
mod alias_switch;
mod play_sound;
mod queue_clear;
mod queue_pause;
mod queue_resume;
mod queue_skip;
mod speak;
mod speak_stop;
mod speak_with_engine;

pub use alias_set::AliasSetRunner;
pub use alias_switch::AliasSwitchRunner;
pub use play_sound::PlaySoundRunner;
pub use queue_clear::QueueClearRunner;
pub use queue_pause::QueuePauseRunner;
pub use queue_resume::QueueResumeRunner;
pub use queue_skip::QueueSkipRunner;
pub use speak::SpeakRunner;
pub use speak_stop::SpeakStopRunner;
pub use speak_with_engine::SpeakWithEngineRunner;

use std::sync::Arc;

use forge_registry::{RegistryError, SubActionRegistry};

use crate::sound_player::SoundPlayer;
use crate::speak_dispatcher::SpeakDispatcher;

pub fn register_audio_sub_actions(
    reg: &mut SubActionRegistry,
    sound_player: Arc<dyn SoundPlayer>,
    speak: Arc<dyn SpeakDispatcher>,
) -> Result<(), RegistryError> {
    reg.register(Box::new(PlaySoundRunner::new(sound_player)))?;
    reg.register(Box::new(SpeakRunner::new(Arc::clone(&speak))))?;
    reg.register(Box::new(SpeakWithEngineRunner::new(Arc::clone(&speak))))?;
    reg.register(Box::new(SpeakStopRunner::new(Arc::clone(&speak))))?;
    reg.register(Box::new(QueuePauseRunner::new(Arc::clone(&speak))))?;
    reg.register(Box::new(QueueResumeRunner::new(Arc::clone(&speak))))?;
    reg.register(Box::new(QueueClearRunner::new(Arc::clone(&speak))))?;
    reg.register(Box::new(QueueSkipRunner::new(Arc::clone(&speak))))?;
    reg.register(Box::new(AliasSetRunner::new(Arc::clone(&speak))))?;
    reg.register(Box::new(AliasSwitchRunner::new(speak)))?;
    Ok(())
}
