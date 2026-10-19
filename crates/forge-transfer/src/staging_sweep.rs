use std::path::Path;

use crate::durable_fs::remove_dir_all_if_present;
use crate::error::TransferError;
use crate::pack::pack_staging_root;
use crate::setup::setup_staging_root;

pub fn sweep_import_staging(data_dir: &Path) -> Result<(), TransferError> {
    let packs = remove_dir_all_if_present(&pack_staging_root(data_dir));
    let setups = remove_dir_all_if_present(&setup_staging_root(data_dir));
    packs.and(setups).map_err(TransferError::from)
}
