use forge_registry::ConfigChoice;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutMessageKind {
    NoteOn,
    NoteOff,
    ControlChange,
    Raw,
}

impl ConfigChoice for OutMessageKind {
    const ALL: &'static [Self] = &[Self::NoteOn, Self::NoteOff, Self::ControlChange, Self::Raw];
    const WIRE_NAMES: &'static [&'static str] = &["note_on", "note_off", "cc", "raw"];

    fn wire_name(self) -> &'static str {
        Self::WIRE_NAMES[self as usize]
    }
}
