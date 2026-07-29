pub mod alert;
pub mod frame;
pub mod ticker;

use crate::error::OverlayError;
use crate::registry::OverlayKindRegistry;

pub fn register_builtin_kinds(reg: &mut OverlayKindRegistry) -> Result<(), OverlayError> {
    reg.register(Box::new(alert::AlertOverlayKind))?;
    reg.register(Box::new(frame::FrameOverlayKind))?;
    reg.register(Box::new(ticker::TickerOverlayKind))?;
    Ok(())
}
