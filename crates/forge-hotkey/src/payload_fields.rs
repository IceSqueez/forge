pub(crate) const COMBO: &str = "combo";
pub(crate) const ID: &str = "id";
pub(crate) const TIMESTAMP_US: &str = "timestamp_us";

#[cfg(target_os = "linux")]
pub(crate) mod portal {
    pub(crate) const REASON: &str = "reason";
    pub(crate) const DETAIL: &str = "detail";

    pub(crate) mod reason {
        pub(crate) const NO_BACKEND_AVAILABLE: &str = "no_hotkey_backend_available";
    }
}
