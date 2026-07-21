use forge_types::PlatformId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KindPlatformContract {
    Universal,
    PlatformSpecific(PlatformId),
}
