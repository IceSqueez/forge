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

#[cfg(test)]
mod tests {
    use super::*;
    use forge_types::Variant;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Side {
        Left,
        Right,
    }

    impl ConfigChoice for Side {
        const ALL: &'static [Self] = &[Self::Left, Self::Right];
        const WIRE_NAMES: &'static [&'static str] = &["left", "right"];

        fn wire_name(self) -> &'static str {
            match self {
                Self::Left => "left",
                Self::Right => "right",
            }
        }
    }

    fn config_with(value: Variant) -> SubActionConfig {
        let mut config = SubActionConfig::new();
        config.insert("side".to_owned(), value);
        config
    }

    #[test]
    fn absent_key_yields_the_fallback_choice() {
        assert_eq!(
            read_choice(&SubActionConfig::new(), "side", Side::Right).unwrap(),
            Side::Right
        );
    }

    #[test]
    fn every_wire_name_reads_back_as_its_choice() {
        for choice in Side::ALL {
            let config = config_with(Variant::String(choice.wire_name().to_owned()));
            assert_eq!(
                read_choice(&config, "side", Side::Left).unwrap(),
                *choice,
                "{}",
                choice.wire_name()
            );
        }
    }

    #[test]
    fn values_outside_the_exact_wire_names_are_rejected_listing_every_choice() {
        for bad in [
            Variant::String("up".to_owned()),
            Variant::String("Left".to_owned()),
            Variant::String(" left".to_owned()),
            Variant::String(String::new()),
            Variant::Int(0),
        ] {
            let err = read_choice(&config_with(bad.clone()), "side", Side::Left).unwrap_err();
            assert!(
                matches!(
                    &err,
                    RegistryError::InvalidConfig(problem)
                        if problem.field.as_deref() == Some("side")
                            && problem.reason
                                == ConfigReason::choices(["left", "right"])
                ),
                "{bad:?}: {err:?}"
            );
        }
    }
}
