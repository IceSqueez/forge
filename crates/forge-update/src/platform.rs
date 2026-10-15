#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    MacosUniversal,
    WindowsX64,
    LinuxX64,
}

impl Platform {
    pub const fn id(self) -> &'static str {
        match self {
            Self::MacosUniversal => "macos-universal",
            Self::WindowsX64 => "windows-x64",
            Self::LinuxX64 => "linux-x64",
        }
    }

    pub const fn running() -> Option<Self> {
        if cfg!(target_os = "macos") {
            Some(Self::MacosUniversal)
        } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
            Some(Self::WindowsX64)
        } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
            Some(Self::LinuxX64)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArtifactKind {
    AppBundleArchive,
    Msi,
    PortableExe,
    AppImage,
}

impl ArtifactKind {
    pub const fn id(self) -> &'static str {
        match self {
            Self::AppBundleArchive => "app-tar-gz",
            Self::Msi => "msi",
            Self::PortableExe => "exe",
            Self::AppImage => "appimage",
        }
    }
}
