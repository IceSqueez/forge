#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GateSettings {
    pub(crate) threshold: f32,
    pub(crate) hold_nanos: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GateEdge {
    Opened,
    Closed,
}

#[derive(Debug, Default)]
pub(crate) struct GateMachine {
    open: bool,
    last_loud_nanos: u64,
}

impl GateMachine {
    pub(crate) fn tick(
        &mut self,
        peak: f32,
        settings: GateSettings,
        now_nanos: u64,
    ) -> Option<GateEdge> {
        if peak >= settings.threshold {
            self.last_loud_nanos = now_nanos;
            let opened = !self.open;
            self.open = true;
            return opened.then_some(GateEdge::Opened);
        }

        let quiet_nanos = now_nanos.saturating_sub(self.last_loud_nanos);
        if self.open && quiet_nanos >= settings.hold_nanos {
            self.open = false;
            return Some(GateEdge::Closed);
        }
        None
    }
}
