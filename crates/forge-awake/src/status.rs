use forge_types::MessageRef;

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
    Unavailable { reason: MessageRef },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AwakeStatus {
    pub(crate) enabled: bool,
    pub(crate) display: AspectState,
    pub(crate) system: AspectState,
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

    pub fn new(enabled: bool, display: AspectState, system: AspectState) -> Self {
        Self {
            enabled,
            display,
            system,
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn aspect(&self, aspect: Aspect) -> &AspectState {
        match aspect {
            Aspect::Display => &self.display,
            Aspect::System => &self.system,
        }
    }

    #[cfg(test)]
    pub(crate) fn fully_held(&self) -> bool {
        self.display == AspectState::Held && self.system == AspectState::Held
    }

    pub(crate) fn set(&mut self, aspect: Aspect, state: AspectState) {
        match aspect {
            Aspect::Display => self.display = state,
            Aspect::System => self.system = state,
        }
    }
}
