use forge_registry::ConfigChoice;

#[derive(Clone, Copy)]
pub(crate) enum FadeMode {
    Linear,
    EaseIn,
    EaseOut,
    EaseBoth,
    Overshoot,
    Zip,
}

impl ConfigChoice for FadeMode {
    const ALL: &'static [Self] = &[
        Self::Linear,
        Self::EaseIn,
        Self::EaseOut,
        Self::EaseBoth,
        Self::Overshoot,
        Self::Zip,
    ];
    const WIRE_NAMES: &'static [&'static str] = &[
        "linear",
        "easeIn",
        "easeOut",
        "easeBoth",
        "overshoot",
        "zip",
    ];

    fn wire_name(self) -> &'static str {
        Self::WIRE_NAMES[self as usize]
    }
}

#[derive(Clone, Copy)]
pub(crate) enum AngleRelativeTo {
    World,
    CurrentItemRotation,
    Model,
    PinPosition,
}

impl ConfigChoice for AngleRelativeTo {
    const ALL: &'static [Self] = &[
        Self::World,
        Self::CurrentItemRotation,
        Self::Model,
        Self::PinPosition,
    ];
    const WIRE_NAMES: &'static [&'static str] = &[
        "RelativeToWorld",
        "RelativeToCurrentItemRotation",
        "RelativeToModel",
        "RelativeToPinPosition",
    ];

    fn wire_name(self) -> &'static str {
        Self::WIRE_NAMES[self as usize]
    }
}

#[derive(Clone, Copy)]
pub(crate) enum SizeRelativeTo {
    World,
    CurrentItemSize,
}

impl ConfigChoice for SizeRelativeTo {
    const ALL: &'static [Self] = &[Self::World, Self::CurrentItemSize];
    const WIRE_NAMES: &'static [&'static str] = &["RelativeToWorld", "RelativeToCurrentItemSize"];

    fn wire_name(self) -> &'static str {
        Self::WIRE_NAMES[self as usize]
    }
}

#[derive(Clone, Copy)]
pub(crate) enum VertexPinType {
    Center,
    Random,
}

impl ConfigChoice for VertexPinType {
    const ALL: &'static [Self] = &[Self::Center, Self::Random];
    const WIRE_NAMES: &'static [&'static str] = &["Center", "Random"];

    fn wire_name(self) -> &'static str {
        Self::WIRE_NAMES[self as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trips<C: ConfigChoice>() -> Vec<(&'static str, Option<&'static str>)> {
        assert_eq!(C::ALL.len(), C::WIRE_NAMES.len());
        C::ALL
            .iter()
            .zip(C::WIRE_NAMES)
            .map(|(choice, name)| {
                assert_eq!(choice.wire_name(), *name);
                (*name, C::parse(name).map(ConfigChoice::wire_name))
            })
            .collect()
    }

    #[test]
    fn every_choice_parses_back_from_its_own_wire_name() {
        for trips in [
            round_trips::<FadeMode>(),
            round_trips::<AngleRelativeTo>(),
            round_trips::<SizeRelativeTo>(),
            round_trips::<VertexPinType>(),
        ] {
            for (name, parsed) in trips {
                assert_eq!(parsed, Some(name));
            }
        }
    }
}
