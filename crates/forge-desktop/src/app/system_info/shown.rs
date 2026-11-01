use forge_components::tr;

use super::{InfoField, InstallKind, InstallLayout, ManualReason, SpeechOutput, SystemInfo};

fn install_text(install: &InstallKind) -> String {
    match install {
        InstallKind::Updatable(updatable) => match updatable.layout {
            InstallLayout::MacAppBundle => tr!("settings_about_value_install_mac_bundle"),
            InstallLayout::WindowsPerUserMsi => tr!("settings_about_value_install_windows_user"),
            InstallLayout::WindowsPortable => tr!("settings_about_value_install_windows_portable"),
            InstallLayout::LinuxAppImage => tr!("settings_about_value_install_appimage"),
        },
        InstallKind::ManualOnly(reason) => match reason {
            ManualReason::NotReleaseBuild => tr!("settings_about_value_install_development"),
            ManualReason::UnsupportedPlatform => tr!("settings_about_value_install_unsupported"),
            ManualReason::UnknownExecutable => tr!("settings_about_value_install_unknown"),
            ManualReason::MacTranslocated => tr!("settings_about_value_install_mac_translocated"),
            ManualReason::ReadOnlyLocation => tr!("settings_about_value_install_read_only"),
            ManualReason::WindowsMachineWideInstall => {
                tr!("settings_about_value_install_windows_machine")
            }
            ManualReason::LinuxPackageManaged => tr!("settings_about_value_install_system_package"),
        },
    }
}

fn system_default_text(device: Option<&str>) -> String {
    match device {
        Some(device) => tr!(
            "settings_about_value_system_default_device",
            device = device
        ),
        None => tr!("settings_about_value_system_default"),
    }
}

fn speech_text(speech: &SpeechOutput) -> String {
    match speech {
        SpeechOutput::SystemDefault { device } => system_default_text(device.as_deref()),
        SpeechOutput::Chosen { device } => device.clone(),
        SpeechOutput::ChosenMissing { fallback } => tr!(
            "settings_about_value_chosen_missing",
            fallback = system_default_text(fallback.as_deref())
        ),
        SpeechOutput::Unknown => tr!("settings_about_unknown"),
    }
}

impl SystemInfo {
    pub(crate) fn shown_value(&self, field: InfoField) -> Option<String> {
        let shown = match field {
            InfoField::Build => Some(self.shown_build()),
            InfoField::Cpu => self.shown_cpu(),
            InfoField::Displays => self.shown_displays(),
            InfoField::AudioOutputs => self.shown_audio(),
            _ => return self.value(field),
        }?;
        Some(self.host.identity.scrub(&shown))
    }

    fn shown_build(&self) -> String {
        if !self.release_build {
            return tr!("settings_about_value_build_dev");
        }
        match &self.host.install {
            Some(install) => tr!(
                "settings_about_value_build_release_install",
                install = install_text(install)
            ),
            None => tr!("settings_about_value_build_release"),
        }
    }

    fn shown_cpu(&self) -> Option<String> {
        let model = self.host.cpu_model.as_deref()?;
        Some(match self.host.logical_cores {
            Some(cores) => tr!(
                "settings_about_value_cpu_cores",
                model = model,
                count = i64::try_from(cores).unwrap_or(i64::MAX)
            ),
            None => model.to_owned(),
        })
    }

    fn shown_displays(&self) -> Option<String> {
        let plain = self.value(InfoField::Displays)?;
        Some(match self.window.scale_factor {
            Some(scale) => tr!(
                "settings_about_value_displays_scaled",
                displays = self.display_sizes(),
                scale = scale.to_string()
            ),
            None => plain,
        })
    }

    fn shown_audio(&self) -> Option<String> {
        let outputs = self.host.audio_outputs.as_ref()?;
        if outputs.device_count == 0 {
            return Some(tr!("settings_about_value_audio_none"));
        }
        Some(tr!(
            "settings_about_value_audio_devices",
            count = i64::try_from(outputs.device_count).unwrap_or(i64::MAX),
            speech = speech_text(&outputs.speech)
        ))
    }
}
