mod config;
mod fake;
mod ledger;
mod protocol;
mod studio;

pub use config::{FakeInput, FakeObsConfig};
pub use fake::FakeObs;
pub use ledger::{Authentication, ObsLedger};
pub(crate) use ledger::{ObsRequest, ObsSession};
pub(crate) use protocol::{
    CURRENT_PROGRAM_SCENE_CHANGED, INPUT_MUTE_STATE_CHANGED, STREAM_STATE_CHANGED,
};
pub use protocol::{
    STATUS_OUTPUT_NOT_RUNNING, STATUS_STUDIO_MODE_NOT_ACTIVE, STATUS_UNKNOWN_REQUEST_TYPE,
    authentication_string,
};
