mod clip_field;
mod play_sound;
mod set_master_volume;
mod stop_all_sounds;
mod stop_sound;
mod volume_field;

pub use play_sound::PlaySoundRunner;
pub use set_master_volume::SetMasterVolumeRunner;
pub use stop_all_sounds::StopAllSoundsRunner;
pub use stop_sound::StopSoundRunner;

use std::sync::Arc;

use forge_registry::{RegistryError, SubActionRegistry};

use crate::control::SoundboardControl;

pub fn register_soundboard_sub_actions(
    reg: &mut SubActionRegistry,
    control: Arc<dyn SoundboardControl>,
) -> Result<(), RegistryError> {
    reg.register(Box::new(PlaySoundRunner::new(Arc::clone(&control))))?;
    reg.register(Box::new(StopSoundRunner::new(Arc::clone(&control))))?;
    reg.register(Box::new(StopAllSoundsRunner::new(Arc::clone(&control))))?;
    reg.register(Box::new(SetMasterVolumeRunner::new(control)))?;
    Ok(())
}
