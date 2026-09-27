#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DeliveryLane {
    #[default]
    Priority,
    Bulk,
}
