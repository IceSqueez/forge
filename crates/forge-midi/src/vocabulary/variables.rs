pub(crate) mod input {
    pub(crate) const NOTE: &str = "midi.note";
    pub(crate) const VELOCITY: &str = "midi.velocity";
    pub(crate) const CONTROLLER: &str = "midi.controller";
    pub(crate) const VALUE: &str = "midi.value";
    pub(crate) const PROGRAM: &str = "midi.program";
    pub(crate) const CHANNEL: &str = "midi.channel";
    pub(crate) const PORT: &str = "midi.port";
}

pub(crate) mod device {
    pub(crate) const NAME: &str = "midi.device.name";
    pub(crate) const DIRECTION: &str = "midi.device.direction";
}
