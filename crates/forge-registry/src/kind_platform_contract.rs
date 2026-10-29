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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_platform_specific_contract_names_a_platform() {
        assert_eq!(KindPlatformContract::Universal.platform(), None);
        for platform in PlatformId::ALL {
            assert_eq!(
                KindPlatformContract::PlatformSpecific(platform).platform(),
                Some(platform)
            );
        }
    }
}
