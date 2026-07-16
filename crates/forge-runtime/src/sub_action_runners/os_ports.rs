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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
pub(crate) mod test_ports {
    //! Recording / programmable mock ports for runner tests. These NEVER touch a
    //! real clipboard, notification daemon, or browser - they record every call
    //! and return a pre-programmed outcome, so the security gate and field
    //! marshaling can be asserted without OS side effects.

    use std::sync::Mutex;

    use forge_events::{Event, EventPublisher};

    use super::{ClipboardPort, DesktopNotice, NotifyPort, OsPortError, UrlOpenPort};

    #[derive(Clone, Copy)]
    pub(crate) enum MockErr {
        Unavailable,
        Failed,
    }

    impl MockErr {
        fn into_error(self) -> OsPortError {
            match self {
                MockErr::Unavailable => OsPortError::Unavailable("mock unavailable".to_owned()),
                MockErr::Failed => OsPortError::Failed("mock failed".to_owned()),
            }
        }
    }

    /// Records every URL handed to the OS opener; performs no real I/O.
    pub(crate) struct RecordingUrlOpenPort {
        opened: Mutex<Vec<String>>,
        err: Option<MockErr>,
    }

    impl RecordingUrlOpenPort {
        pub(crate) fn new() -> Self {
            Self {
                opened: Mutex::new(Vec::new()),
                err: None,
            }
        }

        pub(crate) fn failing(err: MockErr) -> Self {
            Self {
                opened: Mutex::new(Vec::new()),
                err: Some(err),
            }
        }

        pub(crate) fn opened(&self) -> Vec<String> {
            self.opened.lock().unwrap().clone()
        }

        pub(crate) fn call_count(&self) -> usize {
            self.opened.lock().unwrap().len()
        }
    }

    impl UrlOpenPort for RecordingUrlOpenPort {
        fn open(&self, url: String) -> Result<(), OsPortError> {
            self.opened.lock().unwrap().push(url);
            match self.err {
                Some(e) => Err(e.into_error()),
                None => Ok(()),
            }
        }
    }

    /// Records clipboard writes and serves a programmed read result.
    pub(crate) struct RecordingClipboardPort {
        written: Mutex<Vec<String>>,
        copy_err: Option<MockErr>,
        read: Result<String, MockErr>,
    }

    impl RecordingClipboardPort {
        pub(crate) fn new() -> Self {
            Self {
                written: Mutex::new(Vec::new()),
                copy_err: None,
                read: Ok(String::new()),
            }
        }

        pub(crate) fn reads(mut self, text: &str) -> Self {
            self.read = Ok(text.to_owned());
            self
        }

        pub(crate) fn read_fails(mut self, err: MockErr) -> Self {
            self.read = Err(err);
            self
        }

        pub(crate) fn copy_fails(mut self, err: MockErr) -> Self {
            self.copy_err = Some(err);
            self
        }

        pub(crate) fn written(&self) -> Vec<String> {
            self.written.lock().unwrap().clone()
        }
    }

    impl ClipboardPort for RecordingClipboardPort {
        fn copy(&self, text: String) -> Result<(), OsPortError> {
            self.written.lock().unwrap().push(text);
            match self.copy_err {
                Some(e) => Err(e.into_error()),
                None => Ok(()),
            }
        }

        fn read(&self) -> Result<String, OsPortError> {
            self.read.clone().map_err(MockErr::into_error)
        }
    }

    /// Records every notice handed to the OS notification layer.
    pub(crate) struct RecordingNotifyPort {
        shown: Mutex<Vec<DesktopNotice>>,
        err: Option<MockErr>,
    }

    impl RecordingNotifyPort {
        pub(crate) fn new() -> Self {
            Self {
                shown: Mutex::new(Vec::new()),
                err: None,
            }
        }

        pub(crate) fn failing(err: MockErr) -> Self {
            Self {
                shown: Mutex::new(Vec::new()),
                err: Some(err),
            }
        }

        pub(crate) fn shown(&self) -> std::sync::MutexGuard<'_, Vec<DesktopNotice>> {
            self.shown.lock().unwrap()
        }

        pub(crate) fn call_count(&self) -> usize {
            self.shown.lock().unwrap().len()
        }
    }

    impl NotifyPort for RecordingNotifyPort {
        fn show(&self, notice: DesktopNotice) -> Result<(), OsPortError> {
            self.shown.lock().unwrap().push(notice);
            match self.err {
                Some(e) => Err(e.into_error()),
                None => Ok(()),
            }
        }
    }

    /// Sink that drops events - RunContext requires a publisher but these runners
    /// emit none.
    pub(crate) struct NullPublisher;

    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }
}
