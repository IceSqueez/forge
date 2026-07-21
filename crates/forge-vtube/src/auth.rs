#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthState {
    Cold,
    Connected,
    AuthRequired,
}
