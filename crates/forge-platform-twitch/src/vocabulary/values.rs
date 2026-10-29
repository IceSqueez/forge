#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RaidDirection {
    Received,
    Sent,
}

impl RaidDirection {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Received => "received",
            Self::Sent => "sent",
        }
    }
}
