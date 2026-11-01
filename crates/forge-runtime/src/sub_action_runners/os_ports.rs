use std::sync::Arc;

use crate::error::RuntimeError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyUrgency {
    Low,
    Normal,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopNotice {
    pub title: String,
    pub body: String,
    pub urgency: NotifyUrgency,
    pub icon_path: Option<String>,
    pub timeout_ms: u32,
}

pub trait NotifyPort: Send + Sync {
    fn show(&self, notice: DesktopNotice) -> Result<(), RuntimeError>;
}

pub trait ClipboardPort: Send + Sync {
    fn copy(&self, text: String) -> Result<(), RuntimeError>;
    fn read(&self) -> Result<String, RuntimeError>;
}

pub trait UrlOpenPort: Send + Sync {
    fn open(&self, url: String) -> Result<(), RuntimeError>;
}

#[derive(Clone)]
pub struct OsPorts {
    pub notify: Arc<dyn NotifyPort>,
    pub clipboard: Arc<dyn ClipboardPort>,
    pub url_open: Arc<dyn UrlOpenPort>,
}

#[cfg(any(test, feature = "test-support"))]
mod unavailable {
    use std::sync::Arc;

    use super::{ClipboardPort, DesktopNotice, NotifyPort, OsPorts, UrlOpenPort};
    use crate::error::RuntimeError;

    struct NoDesktop;

    fn refused() -> RuntimeError {
        RuntimeError::OsServiceUnavailable("no desktop in this harness".to_owned())
    }

    impl NotifyPort for NoDesktop {
        fn show(&self, _notice: DesktopNotice) -> Result<(), RuntimeError> {
            Err(refused())
        }
    }

    impl ClipboardPort for NoDesktop {
        fn copy(&self, _text: String) -> Result<(), RuntimeError> {
            Err(refused())
        }

        fn read(&self) -> Result<String, RuntimeError> {
            Err(refused())
        }
    }

    impl UrlOpenPort for NoDesktop {
        fn open(&self, _url: String) -> Result<(), RuntimeError> {
            Err(refused())
        }
    }

    impl OsPorts {
        pub fn unavailable() -> Self {
            let port = Arc::new(NoDesktop);
            Self {
                notify: Arc::clone(&port) as Arc<dyn NotifyPort>,
                clipboard: Arc::clone(&port) as Arc<dyn ClipboardPort>,
                url_open: port,
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod test_ports {
    use std::sync::Mutex;

    use time::{Date, Month, OffsetDateTime, Time};

    use super::{ClipboardPort, DesktopNotice, NotifyPort, RuntimeError, UrlOpenPort};

    pub(crate) fn utc(y: i32, m: Month, d: u8, h: u8, min: u8, s: u8) -> OffsetDateTime {
        OffsetDateTime::new_utc(
            Date::from_calendar_date(y, m, d).unwrap(),
            Time::from_hms(h, min, s).unwrap(),
        )
    }

    #[derive(Clone, Copy)]
    pub(crate) enum MockErr {
        Unavailable,
        Failed,
    }

    impl MockErr {
        fn into_error(self) -> RuntimeError {
            match self {
                MockErr::Unavailable => {
                    RuntimeError::OsServiceUnavailable("mock unavailable".to_owned())
                }
                MockErr::Failed => RuntimeError::OsOperationFailed("mock failed".to_owned()),
            }
        }
    }

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
        fn open(&self, url: String) -> Result<(), RuntimeError> {
            self.opened.lock().unwrap().push(url);
            match self.err {
                Some(e) => Err(e.into_error()),
                None => Ok(()),
            }
        }
    }

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
        fn copy(&self, text: String) -> Result<(), RuntimeError> {
            self.written.lock().unwrap().push(text);
            match self.copy_err {
                Some(e) => Err(e.into_error()),
                None => Ok(()),
            }
        }

        fn read(&self) -> Result<String, RuntimeError> {
            self.read.clone().map_err(MockErr::into_error)
        }
    }

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
        fn show(&self, notice: DesktopNotice) -> Result<(), RuntimeError> {
            self.shown.lock().unwrap().push(notice);
            match self.err {
                Some(e) => Err(e.into_error()),
                None => Ok(()),
            }
        }
    }

    pub(crate) use forge_events::test_support::NullPublisher;
}
