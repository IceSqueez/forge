#![allow(clippy::expect_used, clippy::panic)]

pub mod log_capture;
mod settle;

pub use settle::{HANG_GUARD, settle_until};
