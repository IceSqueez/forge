use forge_registry::{ConfigReason, RegistryError};
use forge_types::SubActionConfig;

pub(super) trait ConfigChoice: Copy + 'static {
    const ALL: &'static [Self];
    const WIRE_NAMES: &'static [&'static str];

    fn wire_name(self) -> &'static str;

    fn parse(raw: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|choice| choice.wire_name() == raw)
    }
}

pub(super) fn read_choice<T: ConfigChoice>(
    config: &SubActionConfig,
    key: &str,
    when_absent: T,
) -> Result<T, RegistryError> {
    match config.get(key) {
        None => Ok(when_absent),
        Some(value) => value.as_str().and_then(T::parse).ok_or_else(|| {
            RegistryError::invalid_field(key, ConfigReason::choices(T::WIRE_NAMES.iter().copied()))
        }),
    }
}
