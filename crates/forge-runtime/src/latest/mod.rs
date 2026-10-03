mod feeds;
mod projector;
mod read_step;
mod slots;
mod values;

pub use feeds::BITS_UNIT;
pub use projector::spawn_latest_projector;
pub use read_step::{LATEST_GET_SUB_ACTION, LatestGetRunner, register_latest_sub_actions};
pub use slots::{
    FeedContext, LATEST_SLOTS, LatestSlotDeclaration, SlotFeed, SlotReading, SlotRetention,
    latest_slot_ids, slot_declaration,
};
pub use values::{LatestResetError, LatestValues};
