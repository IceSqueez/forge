use std::pin::Pin;

use forge_types::{Donation, IntegrationId};
use futures_core::Stream;

use crate::PlatformError;

pub type DonationStream =
    Pin<Box<dyn Stream<Item = Result<Donation, PlatformError>> + Send + 'static>>;

pub trait DonationProvider: Send + Sync {
    fn provider(&self) -> &IntegrationId;
    fn donations(&self) -> DonationStream;
}
