//! Owned ports over OS integration crates (clipboard, desktop notifications, URL
//! opening). The runner layer depends only on these traits; the backing crate
//! types never cross this boundary.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum OsPortError {
    #[error("os service unavailable: {0}")]
    Unavailable(String),
    #[error("os operation failed: {0}")]
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyUrgency {
    Low,
    Normal,
    Critical,
}

pub struct DesktopNotice {
    pub title: String,
    pub body: String,
    pub urgency: NotifyUrgency,
    pub icon_path: Option<String>,
    pub timeout_ms: u32,
}

pub trait NotifyPort: Send + Sync {
    fn show(&self, notice: DesktopNotice) -> Result<(), OsPortError>;
}

pub trait ClipboardPort: Send + Sync {
    fn copy(&self, text: String) -> Result<(), OsPortError>;
    /// An accessible-but-empty clipboard yields `Ok("")`; only a missing
    /// clipboard service is an error.
    fn read(&self) -> Result<String, OsPortError>;
}

pub trait UrlOpenPort: Send + Sync {
    fn open(&self, url: String) -> Result<(), OsPortError>;
}

#[derive(Default)]
pub struct SystemNotifyPort;

impl NotifyPort for SystemNotifyPort {
    fn show(&self, notice: DesktopNotice) -> Result<(), OsPortError> {
        let mut builder = notify_rust::Notification::new();
        builder
            .summary(&notice.title)
            .body(&notice.body)
            .timeout(notify_rust::Timeout::Milliseconds(notice.timeout_ms));
        if let Some(icon) = notice.icon_path.as_deref().filter(|s| !s.is_empty()) {
            builder.icon(icon);
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let urgency = match notice.urgency {
                NotifyUrgency::Low => notify_rust::Urgency::Low,
                NotifyUrgency::Normal => notify_rust::Urgency::Normal,
                NotifyUrgency::Critical => notify_rust::Urgency::Critical,
            };
            builder.hint(notify_rust::Hint::Urgency(urgency));
        }
        builder
            .show()
            .map(|_| ())
            .map_err(|e| OsPortError::Unavailable(e.to_string()))
    }
}

#[derive(Default)]
pub struct SystemClipboardPort;

impl ClipboardPort for SystemClipboardPort {
    fn copy(&self, text: String) -> Result<(), OsPortError> {
        clipboard()?
            .set_text(text)
            .map_err(|e| OsPortError::Failed(e.to_string()))
    }

    fn read(&self) -> Result<String, OsPortError> {
        match clipboard()?.get_text() {
            Ok(text) => Ok(text),
            Err(arboard::Error::ContentNotAvailable) => Ok(String::new()),
            Err(e) => Err(OsPortError::Failed(e.to_string())),
        }
    }
}

// Why: a fresh handle per call lets a headless session surface a per-action
// failure instead of aborting sub-action registration at boot.
fn clipboard() -> Result<arboard::Clipboard, OsPortError> {
    arboard::Clipboard::new().map_err(|e| OsPortError::Unavailable(e.to_string()))
}

#[derive(Default)]
pub struct SystemUrlOpenPort;

impl UrlOpenPort for SystemUrlOpenPort {
    fn open(&self, url: String) -> Result<(), OsPortError> {
        open::that_detached(url).map_err(|e| OsPortError::Failed(e.to_string()))
    }
}
