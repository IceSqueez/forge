use forge_runtime::{ClipboardPort, DesktopNotice, NotifyPort, OsPorts, RuntimeError, UrlOpenPort};
use std::sync::Arc;

#[cfg(target_os = "windows")]
const WINDOWS_APP_USER_MODEL_ID: &str = "IceSqueez.Forge";
#[cfg(all(unix, not(target_os = "macos")))]
const DESKTOP_APP_NAME: &str = "forge";

#[derive(Default)]
pub struct SystemNotifyPort;

impl SystemNotifyPort {
    #[cfg(target_os = "macos")]
    pub fn adopt_app_identity() -> Result<(), RuntimeError> {
        notify_rust::set_application(forge_paths::MACOS_BUNDLE_ID)
            .map_err(|e| RuntimeError::OsServiceUnavailable(e.to_string()))
    }

    #[cfg(not(target_os = "macos"))]
    pub fn adopt_app_identity() -> Result<(), RuntimeError> {
        Ok(())
    }
}

impl NotifyPort for SystemNotifyPort {
    fn show(&self, notice: DesktopNotice) -> Result<(), RuntimeError> {
        let mut builder = notify_rust::Notification::new();
        #[cfg(target_os = "windows")]
        builder.app_id(WINDOWS_APP_USER_MODEL_ID);
        #[cfg(all(unix, not(target_os = "macos")))]
        builder.appname(DESKTOP_APP_NAME);
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
                forge_runtime::NotifyUrgency::Low => notify_rust::Urgency::Low,
                forge_runtime::NotifyUrgency::Normal => notify_rust::Urgency::Normal,
                forge_runtime::NotifyUrgency::Critical => notify_rust::Urgency::Critical,
            };
            builder.hint(notify_rust::Hint::Urgency(urgency));
        }
        builder
            .show()
            .map(|_| ())
            .map_err(|e| RuntimeError::OsServiceUnavailable(e.to_string()))
    }
}

#[derive(Default)]
pub struct SystemClipboardPort;

impl ClipboardPort for SystemClipboardPort {
    fn copy(&self, text: String) -> Result<(), RuntimeError> {
        clipboard()?
            .set_text(text)
            .map_err(|e| RuntimeError::OsOperationFailed(e.to_string()))
    }

    fn read(&self) -> Result<String, RuntimeError> {
        match clipboard()?.get_text() {
            Ok(text) => Ok(text),
            Err(arboard::Error::ContentNotAvailable) => Ok(String::new()),
            Err(e) => Err(RuntimeError::OsOperationFailed(e.to_string())),
        }
    }
}

fn clipboard() -> Result<arboard::Clipboard, RuntimeError> {
    arboard::Clipboard::new().map_err(|e| RuntimeError::OsServiceUnavailable(e.to_string()))
}

#[derive(Default)]
pub struct SystemUrlOpenPort;

impl UrlOpenPort for SystemUrlOpenPort {
    fn open(&self, url: String) -> Result<(), RuntimeError> {
        open::that_detached(url).map_err(|e| launcher_failure(&e))
    }
}

fn launcher_failure(error: &std::io::Error) -> RuntimeError {
    RuntimeError::OsOperationFailed(format!("launcher failed: {}", error.kind()))
}

pub fn system_os_ports() -> OsPorts {
    OsPorts {
        notify: Arc::new(SystemNotifyPort),
        clipboard: Arc::new(SystemClipboardPort),
        url_open: Arc::new(SystemUrlOpenPort),
    }
}

#[cfg(test)]
mod tests {
    use super::{RuntimeError, launcher_failure};

    #[test]
    fn url_open_failure_text_carries_only_the_error_kind() {
        let cause = std::io::Error::other(
            "spawn https://leak-host.example/watch?access_token=QUERY-SECRET",
        );

        let err = launcher_failure(&cause);

        let RuntimeError::OsOperationFailed(msg) = err else {
            panic!("expected a Failed launcher error, got {err:?}");
        };
        assert!(
            !msg.contains("leak-host.example"),
            "leaked target host: {msg}"
        );
        assert!(!msg.contains("QUERY-SECRET"), "leaked query token: {msg}");
        assert_eq!(
            msg,
            format!("launcher failed: {}", std::io::ErrorKind::Other)
        );
    }
}
