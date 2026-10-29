use forge_overlay::OverlayKindRegistry;

use crate::documents::model::OverlayDocument;
use crate::error::TransferError;

pub(crate) fn check_overlay_configs(
    overlays: &[OverlayDocument],
    kinds: &OverlayKindRegistry,
) -> Result<(), TransferError> {
    for overlay in overlays {
        kinds
            .validate_stored(
                &overlay.kind_id,
                overlay.config_schema_version,
                &overlay.config,
            )
            .map_err(|source| TransferError::InvalidOverlayConfig {
                overlay: overlay.display_name.clone(),
                source,
            })?;
    }
    Ok(())
}
