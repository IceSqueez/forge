use std::collections::BTreeMap;

use forge_speech_types::{AssignmentStrategy, IgnoreProfile, VoiceAlias};
use forge_storage::{DataProvider, FilterRule, SettingsCatalog, SettingsRepo, TtsPipelineSettings};

use crate::documents::inventory::{PackInventory, load_pack_inventory};
use crate::error::TransferError;

#[derive(Debug, Clone)]
pub(crate) struct SetupInventory {
    pub(crate) configuration: PackInventory,
    pub(crate) tts_pipeline: TtsPipelineSettings,
    pub(crate) filter_rules: Vec<FilterRule>,
    pub(crate) voice_aliases: Vec<VoiceAlias>,
    pub(crate) voice_strategy: AssignmentStrategy,
    pub(crate) ignore_profile: IgnoreProfile,
    pub(crate) portable_settings: BTreeMap<String, String>,
}

pub(crate) async fn load_setup_inventory(
    provider: &dyn DataProvider,
    settings_catalog: &SettingsCatalog,
) -> Result<SetupInventory, TransferError> {
    let configuration = load_pack_inventory(provider).await?;
    let tts = provider.tts_filters_repo();
    let voices = provider.voice_alias_repo();
    let portable_settings = SettingsRepo::load_all(provider)
        .await?
        .into_iter()
        .filter(|(key, _)| settings_catalog.is_portable(key))
        .collect();
    Ok(SetupInventory {
        configuration,
        tts_pipeline: tts.get_pipeline_settings().await?,
        filter_rules: tts.list_rules().await?,
        voice_aliases: voices.list().await?,
        voice_strategy: voices.get_strategy().await?,
        ignore_profile: voices.get_ignore_profile().await?,
        portable_settings,
    })
}
