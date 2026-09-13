use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum EmulatorError {
    #[error("control host `{host}` is not a loopback address")]
    NonLoopbackHost { host: String },

    #[error("control port must be non-zero")]
    ZeroPort,

    #[error("bearer token missing: set {variable}")]
    MissingToken { variable: &'static str },

    #[error("control connection failed: {reason}")]
    Connect { reason: String },

    #[error("control connection timed out")]
    ConnectTimeout,

    #[error("control connection closed")]
    ConnectionClosed,

    #[error("`{request}` got no response in time")]
    RequestTimeout { request: &'static str },

    #[error("authentication refused: {message}")]
    AuthRefused { message: String },

    #[error("`{request}` refused: {message}")]
    Refused {
        request: &'static str,
        code: Option<String>,
        message: String,
    },

    #[error("`{request}` answered with an unexpected shape: {reason}")]
    UnexpectedResponse {
        request: &'static str,
        reason: String,
    },

    #[error("writing output failed: {reason}")]
    Output { reason: String },

    #[error("refusing to seed: {variable} must name the fixture directory")]
    DataDirUnset { variable: &'static str },

    #[error("refusing to seed: {variable} is set, so forge would not read the fixture's key file")]
    KeyFileOverride { variable: &'static str },

    #[error("fixture directory {} is unusable: {reason}", path.display())]
    DataDir { path: PathBuf, reason: String },

    #[error("fixture directory {} is not empty; seed a fresh directory", path.display())]
    DataDirNotEmpty { path: PathBuf },

    #[error("invalid fixture: {reason}")]
    InvalidFixture { reason: String },

    #[error("no free loopback port: {reason}")]
    PortProbe { reason: String },

    #[error("seeding storage failed: {reason}")]
    Storage { reason: String },

    #[error("seeder process failed: {reason}")]
    SeederProcess { reason: String },

    #[error("fake platform could not listen on loopback: {reason}")]
    FakeBind { reason: String },

    #[error("invalid fake platform configuration: {reason}")]
    InvalidFakeConfig { reason: String },

    #[error("no live EventSub session holds a `{subscription_type}` subscription")]
    NotSubscribed { subscription_type: String },

    #[error("no live EventSub session is connected")]
    NoLiveSession,

    #[error("timed out waiting for {what}")]
    WaitTimeout { what: String },

    #[error("refusing to launch forge: a fullscreen window or a running game is open ({windows})")]
    GameInProgress { windows: String },

    #[error(
        "refusing to launch forge: Hyprland state is unreadable, so a fullscreen game cannot be ruled out: {reason}"
    )]
    GameStateUnreadable { reason: String },

    #[error("refusing to launch forge: {} overlaps the live forge data directory", path.display())]
    LiveDataDir { path: PathBuf },

    #[error("refusing to launch forge: {} would stand in for the real home directory", path.display())]
    LiveHome { path: PathBuf },

    #[error("invalid launch: {reason}")]
    InvalidLaunch { reason: String },

    #[error("forge could not be started: {reason}")]
    ForgeSpawn { reason: String },

    #[error("forge exited ({status}); stderr tail: {stderr_tail}; stdout tail: {stdout_tail}")]
    ForgeExited {
        status: String,
        code: Option<i32>,
        stderr_tail: String,
        stdout_tail: String,
    },

    #[error("forge refused its endpoint overrides and exited: {line}")]
    EndpointOverrideRefused { line: String },

    #[error("forge could not bind its seeded server port after {attempts} attempt(s)")]
    ServerPortTaken { attempts: u32 },

    #[error("forge did not authenticate a control connection within {}s; stderr tail: {stderr_tail}", waited.as_secs_f32())]
    ReadinessTimeout {
        waited: std::time::Duration,
        stderr_tail: String,
    },

    #[error("forge teardown failed: {reason}")]
    ForgeTeardown { reason: String },
}
