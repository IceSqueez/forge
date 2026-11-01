mod fake;
mod multipart;

pub(crate) use fake::RecordedPost;
pub use fake::{FakeDiscord, UNKNOWN_WEBHOOK_CODE};
