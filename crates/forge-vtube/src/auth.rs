#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AuthState {
    Cold,
    AwaitingApproval,
    Connected,
    AuthRequired,
}
