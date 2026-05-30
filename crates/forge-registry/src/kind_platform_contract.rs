use forge_types::PlatformId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KindPlatformContract {
    /// Fires from any source; UI hides the platform scope picker entirely.
    Universal,
    /// Pinned to one platform by construction.
    /// UI shows a greyed-out informational badge for the named platform.
    PlatformSpecific(PlatformId),
    /// Can fire from multiple platforms.
    /// UI shows an editable dropdown (Any | per-platform | Custom multi-select).
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
