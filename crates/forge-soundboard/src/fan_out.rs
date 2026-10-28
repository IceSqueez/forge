use forge_types::OutputDevice;

pub(crate) fn fan_out_targets(primary: &OutputDevice, also_headphones: bool) -> Vec<OutputDevice> {
    let mut targets = vec![primary.clone()];
    if also_headphones && !matches!(primary, OutputDevice::Default) {
        targets.push(OutputDevice::Default);
    }
    targets
}
