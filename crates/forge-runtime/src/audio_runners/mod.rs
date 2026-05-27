mod play_sound;
mod speak;

pub use play_sound::PlaySoundRunner;
pub use speak::SpeakRunner;

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
    reg.register(Box::new(SpeakRunner::new(speak)))?;
    Ok(())
}
