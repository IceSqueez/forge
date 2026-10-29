use forge_types::PlatformId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KindPlatformContract {
    Universal,
    PlatformSpecific(PlatformId),
}

impl KindPlatformContract {
    pub const fn platform(self) -> Option<PlatformId> {
        match self {
            Self::Universal => None,
            Self::PlatformSpecific(platform) => Some(platform),
        }
    }
}
