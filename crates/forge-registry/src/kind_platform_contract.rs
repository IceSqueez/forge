use forge_types::PlatformId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KindPlatformContract {
    Universal,
    PlatformSpecific(PlatformId),
    MultiPlatform,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_specific_carries_platform() {
        let c = KindPlatformContract::PlatformSpecific(PlatformId::Twitch);
        assert!(matches!(
            c,
            KindPlatformContract::PlatformSpecific(PlatformId::Twitch)
        ));
    }

    #[test]
    fn universal_and_multiplatform_are_distinct() {
        assert_ne!(
            KindPlatformContract::Universal,
            KindPlatformContract::MultiPlatform
        );
    }
}
