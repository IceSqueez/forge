/// Which queue an event rides inside each lossless consumer: priority is drained first and is
/// never shed for volume; bulk sheds its newest event (counted) when the consumer falls behind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DeliveryLane {
    #[default]
    Priority,
    Bulk,
}
