#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthState {
    Cold,
    AwaitingApproval,
    Connected,
    AuthRequired,
}
