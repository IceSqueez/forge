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
