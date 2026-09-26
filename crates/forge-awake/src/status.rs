#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Aspect {
    Display,
    System,
}

impl Aspect {
    pub const ALL: [Aspect; 2] = [Aspect::Display, Aspect::System];
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AspectState {
    Off,
    Pending,
    Held,
    Unavailable { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AwakeStatus {
    pub enabled: bool,
    pub display: AspectState,
    pub system: AspectState,
}

impl AwakeStatus {
    pub(crate) fn initial(enabled: bool) -> Self {
        let state = if enabled {
            AspectState::Pending
        } else {
            AspectState::Off
        };
        Self {
            enabled,
            display: state.clone(),
            system: state,
        }
    }

    pub fn aspect(&self, aspect: Aspect) -> &AspectState {
        match aspect {
            Aspect::Display => &self.display,
            Aspect::System => &self.system,
        }
    }

    pub fn fully_held(&self) -> bool {
        self.display == AspectState::Held && self.system == AspectState::Held
    }

    pub(crate) fn set(&mut self, aspect: Aspect, state: AspectState) {
        match aspect {
            Aspect::Display => self.display = state,
            Aspect::System => self.system = state,
        }
    }
}
