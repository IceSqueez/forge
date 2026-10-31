#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootWarning {
    Scripts,
    CoreSubActions,
    ScheduledRunSubActions,
    DonationSubActions,
    LatestValueSubActions,
    CoreTriggers,
    SoundboardSubActions,
    SpeechSubActions,
    TriggerDefaults,
    Queues,
    OverlayTypes,
    BotAccounts,
    VoiceGate,
    VoiceAliases,
    ServerBindAddress,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BootWarnings(Vec<BootWarning>);

impl BootWarnings {
    pub fn record(&mut self, warning: BootWarning) {
        if !self.0.contains(&warning) {
            self.0.push(warning);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = BootWarning> + '_ {
        self.0.iter().copied()
    }
}
