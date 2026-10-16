use std::fs;
use std::path::Path;

use forge_update::{InstallKind, InstallLayout, ManualReason, detect_install, is_release_build};
use gpui::{App, Window};
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};

pub const BUILD_COMMIT: &str = env!("FORGE_BUILD_COMMIT");

const BYTES_PER_KIB: u64 = 1024;
const BYTE_UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
const MIN_SCRUBBED_IDENTITY_LEN: usize = 3;
const REDACTED: &str = "[redacted]";
const UNKNOWN: &str = "unknown";
const USER_NAME_ENVS: [&str; 3] = ["USER", "USERNAME", "LOGNAME"];

#[cfg(target_os = "macos")]
const GPU_BACKEND: &str = "Metal";
#[cfg(target_os = "windows")]
const GPU_BACKEND: &str = "DirectX 11";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const GPU_BACKEND: &str = "wgpu (Vulkan or OpenGL)";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuFacts {
    pub device: String,
    pub driver: String,
    pub driver_info: String,
    pub software_emulated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplaySize {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct WindowFacts {
    pub gpu: Option<GpuFacts>,
    pub displays: Vec<DisplaySize>,
    pub scale_factor: Option<f32>,
}

impl WindowFacts {
    pub fn read(window: &Window, cx: &App) -> Self {
        let gpu = window.gpu_specs().map(|specs| GpuFacts {
            device: specs.device_name,
            driver: specs.driver_name,
            driver_info: specs.driver_info,
            software_emulated: specs.is_software_emulated,
        });
        let displays = cx
            .displays()
            .iter()
            .map(|display| {
                let size = display.bounds().size;
                DisplaySize {
                    width: u32::from(size.width),
                    height: u32::from(size.height),
                }
            })
            .collect();
        Self {
            gpu,
            displays,
            scale_factor: Some(window.scale_factor()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioOutput {
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HostFacts {
    pub install: Option<InstallKind>,
    pub os_name: Option<String>,
    pub os_version: Option<String>,
    pub kernel: Option<String>,
    pub cpu_model: Option<String>,
    pub logical_cores: Option<usize>,
    pub total_memory_bytes: Option<u64>,
    pub audio_outputs: Option<Vec<AudioOutput>>,
    pub data_dir_bytes: Option<u64>,
    pub identity: IdentityScrubber,
}

impl HostFacts {
    pub fn collect(data_dir: &Path) -> Self {
        let system = System::new_with_specifics(
            RefreshKind::new()
                .with_cpu(CpuRefreshKind::new())
                .with_memory(MemoryRefreshKind::new().with_ram()),
        );
        let cpus = system.cpus();
        let cpu_model = cpus
            .first()
            .map(|cpu| cpu.brand().trim().to_owned())
            .filter(|brand| !brand.is_empty());
        let logical_cores = (!cpus.is_empty()).then_some(cpus.len());
        let total_memory_bytes = Some(system.total_memory()).filter(|bytes| *bytes > 0);
        let audio_outputs = forge_audio::list_output_devices().ok().map(|devices| {
            devices
                .into_iter()
                .map(|device| AudioOutput {
                    name: device.name,
                    is_default: device.is_default,
                })
                .collect()
        });
        Self {
            install: Some(detect_install()),
            os_name: System::name(),
            os_version: System::os_version(),
            kernel: System::kernel_version(),
            cpu_model,
            logical_cores,
            total_memory_bytes,
            audio_outputs,
            data_dir_bytes: directory_size(data_dir),
            identity: IdentityScrubber::current(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IdentityScrubber {
    needles: Vec<String>,
}

impl IdentityScrubber {
    pub fn new(identities: impl IntoIterator<Item = String>) -> Self {
        let mut needles: Vec<String> = identities
            .into_iter()
            .map(|identity| identity.trim().to_ascii_lowercase())
            .filter(|identity| identity.len() >= MIN_SCRUBBED_IDENTITY_LEN)
            .collect();
        needles.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
        needles.dedup();
        Self { needles }
    }

    pub fn current() -> Self {
        let user_names = USER_NAME_ENVS
            .iter()
            .filter_map(|name| std::env::var(name).ok());
        let home = std::env::home_dir().map(|dir| dir.to_string_lossy().into_owned());
        Self::new(
            System::host_name()
                .into_iter()
                .chain(user_names)
                .chain(home),
        )
    }

    pub fn scrub(&self, text: &str) -> String {
        let mut scrubbed = text.to_owned();
        for needle in &self.needles {
            let lowered = scrubbed.to_ascii_lowercase();
            let mut result = String::with_capacity(scrubbed.len());
            let mut cursor = 0;
            while let Some(offset) = lowered[cursor..].find(needle.as_str()) {
                let start = cursor + offset;
                result.push_str(&scrubbed[cursor..start]);
                result.push_str(REDACTED);
                cursor = start + needle.len();
            }
            result.push_str(&scrubbed[cursor..]);
            scrubbed = result;
        }
        scrubbed
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InfoField {
    Forge,
    Build,
    Os,
    Kernel,
    Cpu,
    Memory,
    Gpu,
    GpuBackend,
    Displays,
    AudioOutputs,
    DataDir,
}

impl InfoField {
    pub const ALL: [InfoField; 11] = [
        InfoField::Forge,
        InfoField::Build,
        InfoField::Os,
        InfoField::Kernel,
        InfoField::Cpu,
        InfoField::Memory,
        InfoField::Gpu,
        InfoField::GpuBackend,
        InfoField::Displays,
        InfoField::AudioOutputs,
        InfoField::DataDir,
    ];

    pub fn report_label(self) -> &'static str {
        match self {
            InfoField::Forge => "forge",
            InfoField::Build => "Build",
            InfoField::Os => "OS",
            InfoField::Kernel => "Kernel",
            InfoField::Cpu => "CPU",
            InfoField::Memory => "Memory",
            InfoField::Gpu => "GPU",
            InfoField::GpuBackend => "GPU backend",
            InfoField::Displays => "Displays",
            InfoField::AudioOutputs => "Audio outputs",
            InfoField::DataDir => "Data folder size",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SystemInfo {
    pub forge_version: String,
    pub build_commit: String,
    pub release_build: bool,
    pub window: WindowFacts,
    pub host: HostFacts,
}

impl SystemInfo {
    pub fn assemble(window: WindowFacts, host: HostFacts) -> Self {
        Self {
            forge_version: env!("CARGO_PKG_VERSION").to_owned(),
            build_commit: BUILD_COMMIT.to_owned(),
            release_build: is_release_build(),
            window,
            host,
        }
    }

    pub fn value(&self, field: InfoField) -> Option<String> {
        let raw = match field {
            InfoField::Forge => Some(format!("v{} ({})", self.forge_version, self.build_commit)),
            InfoField::Build => Some(self.build_value()),
            InfoField::Os => self.os_value(),
            InfoField::Kernel => self.host.kernel.clone(),
            InfoField::Cpu => self.cpu_value(),
            InfoField::Memory => self.host.total_memory_bytes.map(format_bytes),
            InfoField::Gpu => self.window.gpu.as_ref().map(gpu_value),
            InfoField::GpuBackend => Some(GPU_BACKEND.to_owned()),
            InfoField::Displays => self.displays_value(),
            InfoField::AudioOutputs => self.audio_value(),
            InfoField::DataDir => self.host.data_dir_bytes.map(format_bytes),
        }?;
        Some(self.host.identity.scrub(&raw))
    }

    pub fn to_report_text(&self) -> String {
        let mut lines = Vec::with_capacity(InfoField::ALL.len());
        for field in InfoField::ALL {
            let value = self.value(field).unwrap_or_else(|| UNKNOWN.to_owned());
            let label = field.report_label();
            if value.contains('\n') {
                lines.push(format!("{label}:"));
                lines.extend(value.lines().map(|line| format!("  - {line}")));
            } else {
                lines.push(format!("{label}: {value}"));
            }
        }
        lines.join("\n")
    }

    fn build_value(&self) -> String {
        if !self.release_build {
            return "dev".to_owned();
        }
        match &self.host.install {
            Some(install) => format!("release, {}", install_label(install)),
            None => "release".to_owned(),
        }
    }

    fn os_value(&self) -> Option<String> {
        let name = self.host.os_name.as_deref()?;
        let arch = std::env::consts::ARCH;
        Some(match self.host.os_version.as_deref() {
            Some(version) => format!("{name} {version} ({arch})"),
            None => format!("{name} ({arch})"),
        })
    }

    fn cpu_value(&self) -> Option<String> {
        let model = self.host.cpu_model.as_deref()?;
        Some(match self.host.logical_cores {
            Some(cores) => format!("{model}, {cores} logical cores"),
            None => model.to_owned(),
        })
    }

    fn displays_value(&self) -> Option<String> {
        let displays = &self.window.displays;
        if displays.is_empty() {
            return None;
        }
        let sizes = displays
            .iter()
            .map(|size| format!("{}x{}", size.width, size.height))
            .collect::<Vec<_>>()
            .join(", ");
        Some(match self.window.scale_factor {
            Some(scale) => format!("{}: {sizes} (window scale {scale})", displays.len()),
            None => format!("{}: {sizes}", displays.len()),
        })
    }

    fn audio_value(&self) -> Option<String> {
        let outputs = self.host.audio_outputs.as_ref()?;
        if outputs.is_empty() {
            return Some("none".to_owned());
        }
        Some(
            outputs
                .iter()
                .map(|output| {
                    if output.is_default {
                        format!("{} (default)", output.name)
                    } else {
                        output.name.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n"),
        )
    }
}

fn gpu_value(gpu: &GpuFacts) -> String {
    let driver = [gpu.driver.trim(), gpu.driver_info.trim()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let mut value = if driver.is_empty() {
        gpu.device.clone()
    } else {
        format!("{} ({driver})", gpu.device)
    };
    if gpu.software_emulated {
        value.push_str(" [software]");
    }
    value
}

pub fn install_label(install: &InstallKind) -> &'static str {
    match install {
        InstallKind::Updatable(updatable) => match updatable.layout {
            InstallLayout::MacAppBundle => "macOS app bundle",
            InstallLayout::WindowsPerUserMsi => "Windows installer (per-user)",
            InstallLayout::WindowsPortable => "Windows portable",
            InstallLayout::LinuxAppImage => "AppImage",
        },
        InstallKind::ManualOnly(reason) => match reason {
            ManualReason::NotReleaseBuild => "development build",
            ManualReason::UnsupportedPlatform => "unsupported platform",
            ManualReason::UnknownExecutable => "unknown location",
            ManualReason::MacTranslocated => "macOS app (translocated)",
            ManualReason::ReadOnlyLocation => "read-only location",
            ManualReason::WindowsMachineWideInstall => "Windows installer (all users)",
            ManualReason::LinuxPackageManaged => "system package",
        },
    }
}

pub fn format_bytes(bytes: u64) -> String {
    let divisor = BYTES_PER_KIB as f64;
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= divisor && unit + 1 < BYTE_UNITS.len() {
        value /= divisor;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", BYTE_UNITS[0])
    } else {
        format!("{value:.1} {}", BYTE_UNITS[unit])
    }
}

pub fn directory_size(root: &Path) -> Option<u64> {
    if !fs::symlink_metadata(root).ok()?.is_dir() {
        return None;
    }
    let mut total: u64 = 0;
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.is_dir() {
                pending.push(entry.path());
            } else if metadata.is_file() {
                total = total.saturating_add(metadata.len());
            }
        }
    }
    Some(total)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::collections::HashSet;
    use std::fs;
    use std::path::PathBuf;

    use forge_update::{InstallKind, InstallLayout, ManualReason, UpdatableInstall};

    use super::{
        AudioOutput, DisplaySize, GpuFacts, HostFacts, IdentityScrubber, InfoField, SystemInfo,
        WindowFacts, directory_size, format_bytes, install_label,
    };

    const KIB: u64 = 1024;
    const MIB: u64 = KIB * KIB;
    const GIB: u64 = MIB * KIB;
    const TIB: u64 = GIB * KIB;

    fn scrubber(identities: &[&str]) -> IdentityScrubber {
        IdentityScrubber::new(identities.iter().map(|identity| (*identity).to_owned()))
    }

    fn info(window: WindowFacts, host: HostFacts) -> SystemInfo {
        SystemInfo {
            forge_version: "0.9.0".to_owned(),
            build_commit: "abc1234".to_owned(),
            release_build: true,
            window,
            host,
        }
    }

    fn appimage() -> InstallKind {
        InstallKind::Updatable(UpdatableInstall {
            layout: InstallLayout::LinuxAppImage,
            target: PathBuf::from("/opt/forge/forge.AppImage"),
        })
    }

    fn full_window() -> WindowFacts {
        WindowFacts {
            gpu: Some(GpuFacts {
                device: "Radeon RX".to_owned(),
                driver: "radv".to_owned(),
                driver_info: "Mesa 25.2".to_owned(),
                software_emulated: false,
            }),
            displays: vec![
                DisplaySize {
                    width: 2560,
                    height: 1440,
                },
                DisplaySize {
                    width: 1920,
                    height: 1080,
                },
            ],
            scale_factor: Some(1.5),
        }
    }

    fn full_host() -> HostFacts {
        HostFacts {
            install: Some(appimage()),
            os_name: Some("Arch Linux".to_owned()),
            os_version: Some("rolling".to_owned()),
            kernel: Some("6.17.1".to_owned()),
            cpu_model: Some("Ryzen 9".to_owned()),
            logical_cores: Some(32),
            total_memory_bytes: Some(64 * GIB),
            audio_outputs: Some(vec![
                AudioOutput {
                    name: "Speakers".to_owned(),
                    is_default: true,
                },
                AudioOutput {
                    name: "HDMI".to_owned(),
                    is_default: false,
                },
            ]),
            data_dir_bytes: Some(3 * MIB / 2),
            identity: IdentityScrubber::default(),
        }
    }

    fn report_line(report: &str, label: &str) -> String {
        let prefix = format!("{label}:");
        report
            .lines()
            .find(|line| line.starts_with(&prefix))
            .map(str::to_owned)
            .expect("report has a line for the label")
    }

    #[test]
    fn format_bytes_switches_units_at_each_power_of_1024() {
        for (bytes, expected) in [
            (0, "0 B"),
            (1, "1 B"),
            (KIB - 1, "1023 B"),
            (KIB, "1.0 KiB"),
            (KIB + KIB / 2, "1.5 KiB"),
            (MIB, "1.0 MiB"),
            (GIB, "1.0 GiB"),
            (5 * GIB / 2, "2.5 GiB"),
            (TIB, "1.0 TiB"),
        ] {
            assert_eq!(format_bytes(bytes), expected, "{bytes}");
        }
    }

    #[test]
    fn format_bytes_stays_in_tib_above_the_largest_unit() {
        assert_eq!(format_bytes(2048 * TIB), "2048.0 TiB");
        assert_eq!(format_bytes(u64::MAX), "16777216.0 TiB");
    }

    #[test]
    fn directory_size_sums_files_in_every_nested_directory() {
        let root = tempfile::tempdir().expect("tempdir");
        let nested = root.path().join("a").join("b");
        fs::create_dir_all(&nested).expect("nested dirs");
        fs::create_dir(root.path().join("empty")).expect("empty dir");
        fs::write(root.path().join("top.db"), [0u8; 100]).expect("top file");
        fs::write(root.path().join("a").join("mid.log"), [0u8; 20]).expect("mid file");
        fs::write(nested.join("deep.bin"), [0u8; 3]).expect("deep file");

        assert_eq!(directory_size(root.path()), Some(123));
    }

    #[test]
    fn directory_size_of_an_empty_directory_is_zero() {
        let root = tempfile::tempdir().expect("tempdir");

        assert_eq!(directory_size(root.path()), Some(0));
    }

    #[test]
    fn directory_size_is_unknown_for_a_missing_or_file_root() {
        let root = tempfile::tempdir().expect("tempdir");
        let file = root.path().join("forge.db");
        fs::write(&file, [0u8; 10]).expect("file");

        assert_eq!(directory_size(&root.path().join("missing")), None);
        assert_eq!(directory_size(&file), None);
    }

    #[cfg(unix)]
    #[test]
    fn directory_size_does_not_follow_symlinks_inside_the_tree() {
        let outside = tempfile::tempdir().expect("outside");
        fs::write(outside.path().join("huge.bin"), [0u8; 500]).expect("outside file");
        let root = tempfile::tempdir().expect("tempdir");
        fs::write(root.path().join("own.db"), [0u8; 7]).expect("own file");
        std::os::unix::fs::symlink(outside.path(), root.path().join("linked_dir"))
            .expect("dir symlink");
        std::os::unix::fs::symlink(
            outside.path().join("huge.bin"),
            root.path().join("linked_file"),
        )
        .expect("file symlink");

        assert_eq!(directory_size(root.path()), Some(7));
    }

    #[cfg(unix)]
    #[test]
    fn directory_size_survives_a_symlink_loop() {
        let root = tempfile::tempdir().expect("tempdir");
        fs::write(root.path().join("own.db"), [0u8; 4]).expect("own file");
        std::os::unix::fs::symlink(root.path(), root.path().join("loop")).expect("loop symlink");

        assert_eq!(directory_size(root.path()), Some(4));
    }

    #[test]
    fn scrubber_redacts_every_occurrence_regardless_of_case() {
        let scrubber = scrubber(&["Alice"]);

        assert_eq!(
            scrubber.scrub("ALICE met alice and aLiCe"),
            "[redacted] met [redacted] and [redacted]",
        );
    }

    #[test]
    fn scrubber_ignores_identities_shorter_than_three_characters_after_trimming() {
        let scrubber = scrubber(&["ab", "  xy  ", "", "abc"]);

        assert_eq!(scrubber.scrub("ab xy abcd"), "ab xy [redacted]d");
    }

    #[test]
    fn scrubber_prefers_the_longer_overlapping_identity_whatever_the_input_order() {
        for identities in [["alice", "/home/alice"], ["/home/alice", "alice"]] {
            let scrubber = scrubber(&identities);

            assert_eq!(
                scrubber.scrub("/home/alice/.local/share/forge"),
                "[redacted]/.local/share/forge",
                "{identities:?}",
            );
        }
    }

    #[test]
    fn scrubber_leaves_non_ascii_text_around_a_hit_intact() {
        let scrubber = scrubber(&["alice"]);

        assert_eq!(
            scrubber.scrub("Динамік alice ünïcode 🎧"),
            "Динамік [redacted] ünïcode 🎧",
        );
    }

    #[test]
    fn scrubber_without_identities_returns_text_unchanged() {
        let text = "Arch Linux rolling (x86_64)";

        assert_eq!(IdentityScrubber::default().scrub(text), text);
        assert_eq!(scrubber(&[]).scrub(text), text);
    }

    #[test]
    fn install_label_names_every_install_kind_distinctly() {
        let updatable = [
            InstallLayout::MacAppBundle,
            InstallLayout::WindowsPerUserMsi,
            InstallLayout::WindowsPortable,
            InstallLayout::LinuxAppImage,
        ]
        .map(|layout| {
            InstallKind::Updatable(UpdatableInstall {
                layout,
                target: PathBuf::from("/opt/forge"),
            })
        });
        let manual = [
            ManualReason::NotReleaseBuild,
            ManualReason::UnsupportedPlatform,
            ManualReason::UnknownExecutable,
            ManualReason::MacTranslocated,
            ManualReason::ReadOnlyLocation,
            ManualReason::WindowsMachineWideInstall,
            ManualReason::LinuxPackageManaged,
        ]
        .map(InstallKind::ManualOnly);
        let kinds: Vec<InstallKind> = updatable.into_iter().chain(manual).collect();

        let labels: HashSet<&str> = kinds.iter().map(install_label).collect();

        assert_eq!(labels.len(), kinds.len(), "{labels:?}");
        assert!(labels.iter().all(|label| !label.trim().is_empty()));
    }

    #[test]
    fn report_lists_every_field_in_order_with_its_formatted_value() {
        let info = info(full_window(), full_host());
        let backend = info.value(InfoField::GpuBackend).expect("backend");
        let arch = std::env::consts::ARCH;

        let expected = [
            "forge: v0.9.0 (abc1234)".to_owned(),
            "Build: release, AppImage".to_owned(),
            format!("OS: Arch Linux rolling ({arch})"),
            "Kernel: 6.17.1".to_owned(),
            "CPU: Ryzen 9, 32 logical cores".to_owned(),
            "Memory: 64.0 GiB".to_owned(),
            "GPU: Radeon RX (radv Mesa 25.2)".to_owned(),
            format!("GPU backend: {backend}"),
            "Displays: 2: 2560x1440, 1920x1080 (window scale 1.5)".to_owned(),
            "Audio outputs:".to_owned(),
            "  - Speakers (default)".to_owned(),
            "  - HDMI".to_owned(),
            "Data folder size: 1.5 MiB".to_owned(),
        ]
        .join("\n");

        assert_eq!(info.to_report_text(), expected);
    }

    #[test]
    fn report_labels_follow_info_field_order() {
        let report = info(WindowFacts::default(), HostFacts::default()).to_report_text();

        let labels: Vec<&str> = report
            .lines()
            .map(|line| line.split_once(':').map_or(line, |(label, _)| label))
            .collect();
        let expected: Vec<&str> = InfoField::ALL
            .iter()
            .map(|field| field.report_label())
            .collect();

        assert_eq!(labels, expected);
    }

    #[test]
    fn report_marks_every_missing_fact_unknown() {
        let report = info(WindowFacts::default(), HostFacts::default()).to_report_text();

        for field in [
            InfoField::Os,
            InfoField::Kernel,
            InfoField::Cpu,
            InfoField::Memory,
            InfoField::Gpu,
            InfoField::Displays,
            InfoField::AudioOutputs,
            InfoField::DataDir,
        ] {
            let label = field.report_label();
            assert_eq!(report_line(&report, label), format!("{label}: unknown"));
        }
    }

    #[test]
    fn report_says_none_for_an_empty_audio_list_and_inlines_a_single_output() {
        for (outputs, expected) in [
            (Vec::new(), "Audio outputs: none"),
            (
                vec![AudioOutput {
                    name: "Headset".to_owned(),
                    is_default: false,
                }],
                "Audio outputs: Headset",
            ),
        ] {
            let host = HostFacts {
                audio_outputs: Some(outputs),
                ..full_host()
            };
            let report = info(full_window(), host).to_report_text();

            assert_eq!(report_line(&report, "Audio outputs"), expected);
        }
    }

    #[test]
    fn build_value_reports_dev_or_release_with_the_install_label() {
        for (release_build, install, expected) in [
            (false, Some(appimage()), "dev"),
            (false, None, "dev"),
            (true, Some(appimage()), "release, AppImage"),
            (
                true,
                Some(InstallKind::ManualOnly(ManualReason::LinuxPackageManaged)),
                "release, system package",
            ),
            (true, None, "release"),
        ] {
            let mut info = info(
                WindowFacts::default(),
                HostFacts {
                    install,
                    ..HostFacts::default()
                },
            );
            info.release_build = release_build;

            assert_eq!(info.value(InfoField::Build).as_deref(), Some(expected));
        }
    }

    #[test]
    fn gpu_value_joins_present_driver_parts_and_flags_software_rendering() {
        for (driver, driver_info, software_emulated, expected) in [
            ("radv", "Mesa 25.2", false, "Radeon RX (radv Mesa 25.2)"),
            (" radv ", "  ", false, "Radeon RX (radv)"),
            ("", "Mesa 25.2", false, "Radeon RX (Mesa 25.2)"),
            ("", " ", false, "Radeon RX"),
            ("llvmpipe", "", true, "Radeon RX (llvmpipe) [software]"),
            ("", "", true, "Radeon RX [software]"),
        ] {
            let window = WindowFacts {
                gpu: Some(GpuFacts {
                    device: "Radeon RX".to_owned(),
                    driver: driver.to_owned(),
                    driver_info: driver_info.to_owned(),
                    software_emulated,
                }),
                ..WindowFacts::default()
            };

            assert_eq!(
                info(window, HostFacts::default())
                    .value(InfoField::Gpu)
                    .as_deref(),
                Some(expected),
            );
        }
    }

    #[test]
    fn partial_os_cpu_and_display_facts_omit_only_the_missing_part() {
        let arch = std::env::consts::ARCH;
        let host = HostFacts {
            os_version: None,
            logical_cores: None,
            ..full_host()
        };
        let window = WindowFacts {
            scale_factor: None,
            ..full_window()
        };
        let info = info(window, host);

        assert_eq!(
            info.value(InfoField::Os),
            Some(format!("Arch Linux ({arch})"))
        );
        assert_eq!(info.value(InfoField::Cpu).as_deref(), Some("Ryzen 9"));
        assert_eq!(
            info.value(InfoField::Displays).as_deref(),
            Some("2: 2560x1440, 1920x1080"),
        );
    }

    #[test]
    fn report_redacts_identities_from_every_value() {
        let leak = "BoxOfAlice";
        let mut info = info(
            WindowFacts {
                gpu: Some(GpuFacts {
                    device: format!("{leak} GPU"),
                    driver: format!("{leak}-driver"),
                    driver_info: format!("/home/{leak}/mesa"),
                    software_emulated: false,
                }),
                ..full_window()
            },
            HostFacts {
                os_name: Some(format!("{leak} OS")),
                os_version: Some(format!("{leak}-1")),
                kernel: Some(format!("6.1-{leak}")),
                cpu_model: Some(format!("{leak} CPU")),
                audio_outputs: Some(vec![
                    AudioOutput {
                        name: format!("{leak} speakers"),
                        is_default: true,
                    },
                    AudioOutput {
                        name: format!("{leak}'s headset"),
                        is_default: false,
                    },
                ]),
                identity: scrubber(&["boxofalice"]),
                ..full_host()
            },
        );
        info.forge_version = format!("0.9.0-{leak}");
        info.build_commit = format!("{leak}-commit");

        let report = info.to_report_text();

        assert!(
            !report.to_ascii_lowercase().contains("boxofalice"),
            "{report}"
        );
        assert_eq!(report.matches("[redacted]").count(), 11, "{report}");
    }
}
