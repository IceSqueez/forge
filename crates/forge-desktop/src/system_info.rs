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
