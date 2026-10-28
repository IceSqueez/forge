use forge_types::OutputDevice;

pub(crate) fn fan_out_targets(primary: &OutputDevice, also_headphones: bool) -> Vec<OutputDevice> {
    let mut targets = vec![primary.clone()];
    if also_headphones && !matches!(primary, OutputDevice::Default) {
        targets.push(OutputDevice::Default);
    }
    targets
}

#[cfg(test)]
mod tests {
    use super::fan_out_targets;
    use forge_types::OutputDevice;

    #[test]
    fn fan_out_adds_default_only_for_non_default_primary_with_headphones() {
        let by_id = OutputDevice::ById { id: "dev-1".into() };
        let by_name = OutputDevice::ByName {
            name: "Speakers".into(),
        };
        let cases = [
            (OutputDevice::Default, false, vec![OutputDevice::Default]),
            (OutputDevice::Default, true, vec![OutputDevice::Default]),
            (by_id.clone(), false, vec![by_id.clone()]),
            (
                by_id.clone(),
                true,
                vec![by_id.clone(), OutputDevice::Default],
            ),
            (by_name.clone(), false, vec![by_name.clone()]),
            (
                by_name.clone(),
                true,
                vec![by_name.clone(), OutputDevice::Default],
            ),
        ];
        for (primary, headphones, expected) in cases {
            assert_eq!(
                fan_out_targets(&primary, headphones),
                expected,
                "{primary:?} headphones={headphones}"
            );
        }
    }
}
