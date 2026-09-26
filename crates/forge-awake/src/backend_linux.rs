use std::time::Duration;

use async_trait::async_trait;
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;
use zbus::Connection;
use zbus::zvariant::OwnedFd;

use crate::Aspect;
use crate::AwakeError;
use crate::backend::{AwakeBackend, DisruptionSender};

const APP_NAME: &str = "forge";

const SCREEN_SAVER_NAME: &str = "org.freedesktop.ScreenSaver";
const SCREEN_SAVER_PATHS: [&str; 2] = ["/org/freedesktop/ScreenSaver", "/ScreenSaver"];

const LOGIN_NAME: &str = "org.freedesktop.login1";
const LOGIN_PATH: &str = "/org/freedesktop/login1";
const LOGIN_MANAGER: &str = "org.freedesktop.login1.Manager";
const IDLE_LOCK: &str = "idle";
const BLOCK_MODE: &str = "block";

const SERVICE_UNKNOWN: &str = "org.freedesktop.DBus.Error.ServiceUnknown";
const NAME_HAS_NO_OWNER: &str = "org.freedesktop.DBus.Error.NameHasNoOwner";
const UNKNOWN_OBJECT: &str = "org.freedesktop.DBus.Error.UnknownObject";
const UNKNOWN_METHOD: &str = "org.freedesktop.DBus.Error.UnknownMethod";

const SESSION_BUS: &str = "session bus";
const SYSTEM_BUS: &str = "system bus";
const SCREEN_SAVER_SERVICE: &str = "screen saver service";
const LOGIN_SERVICE: &str = "login manager";

const BUS_TIMEOUT: Duration = Duration::from_secs(5);

struct ScreenSaverHold {
    cookie: u32,
    path: &'static str,
}

pub(crate) struct LinuxBackend {
    reason: String,
    session: Result<Connection, String>,
    system: Result<Connection, String>,
    screen_saver: Option<ScreenSaverHold>,
    idle_lock: Option<OwnedFd>,
    watchers: Vec<JoinHandle<()>>,
}

impl LinuxBackend {
    pub(crate) async fn open(reason: String, disruptions: DisruptionSender) -> Self {
        let session = connect(Connection::session()).await;
        let system = connect(Connection::system()).await;
        let mut watchers = Vec::new();
        if let Ok(conn) = &session {
            watchers.push(spawn_owner_watch(
                conn.clone(),
                SCREEN_SAVER_NAME,
                Aspect::Display,
                disruptions.clone(),
            ));
        }
        if let Ok(conn) = &system {
            watchers.push(spawn_owner_watch(
                conn.clone(),
                LOGIN_NAME,
                Aspect::System,
                disruptions,
            ));
        }
        Self {
            reason,
            session,
            system,
            screen_saver: None,
            idle_lock: None,
            watchers,
        }
    }

    async fn inhibit_screen_saver(&mut self) -> Result<(), AwakeError> {
        let conn = self
            .session
            .as_ref()
            .map_err(|reason| AwakeError::Unreachable {
                service: SESSION_BUS,
                reason: reason.clone(),
            })?;
        let mut missing_object = None;
        for path in SCREEN_SAVER_PATHS {
            match call(
                conn,
                SCREEN_SAVER_NAME,
                path,
                SCREEN_SAVER_NAME,
                "Inhibit",
                &(APP_NAME, self.reason.as_str()),
            )
            .await
            {
                Ok(reply) => {
                    let cookie: u32 = reply
                        .body()
                        .deserialize()
                        .map_err(|e| refused(SCREEN_SAVER_SERVICE, e))?;
                    self.screen_saver = Some(ScreenSaverHold { cookie, path });
                    return Ok(());
                }
                Err(e) if is_error_named(&e, &[UNKNOWN_OBJECT, UNKNOWN_METHOD]) => {
                    missing_object = Some(e);
                }
                Err(e) => return Err(classify(e, SCREEN_SAVER_SERVICE)),
            }
        }
        Err(match missing_object {
            Some(e) => classify(e, SCREEN_SAVER_SERVICE),
            None => AwakeError::ServiceMissing {
                service: SCREEN_SAVER_SERVICE,
            },
        })
    }

    async fn uninhibit_screen_saver(&mut self) {
        let (Some(hold), Ok(conn)) = (self.screen_saver.take(), &self.session) else {
            return;
        };
        if let Err(e) = call(
            conn,
            SCREEN_SAVER_NAME,
            hold.path,
            SCREEN_SAVER_NAME,
            "UnInhibit",
            &(hold.cookie,),
        )
        .await
        {
            tracing::debug!(error = %e, "stay awake: screen saver inhibit was already gone");
        }
    }

    async fn take_idle_lock(&mut self) -> Result<(), AwakeError> {
        let conn = self
            .system
            .as_ref()
            .map_err(|reason| AwakeError::Unreachable {
                service: SYSTEM_BUS,
                reason: reason.clone(),
            })?;
        let reply = call(
            conn,
            LOGIN_NAME,
            LOGIN_PATH,
            LOGIN_MANAGER,
            "Inhibit",
            &(IDLE_LOCK, APP_NAME, self.reason.as_str(), BLOCK_MODE),
        )
        .await
        .map_err(|e| classify(e, LOGIN_SERVICE))?;
        let fd: OwnedFd = reply
            .body()
            .deserialize()
            .map_err(|e| refused(LOGIN_SERVICE, e))?;
        self.idle_lock = Some(fd);
        Ok(())
    }
}

impl Drop for LinuxBackend {
    fn drop(&mut self) {
        for watcher in &self.watchers {
            watcher.abort();
        }
    }
}

#[async_trait]
impl AwakeBackend for LinuxBackend {
    async fn acquire(&mut self, aspect: Aspect) -> Result<(), AwakeError> {
        self.release(aspect).await;
        match aspect {
            Aspect::Display => self.inhibit_screen_saver().await,
            Aspect::System => self.take_idle_lock().await,
        }
    }

    async fn release(&mut self, aspect: Aspect) {
        match aspect {
            Aspect::Display => self.uninhibit_screen_saver().await,
            Aspect::System => self.idle_lock = None,
        }
    }

    fn is_held(&self, aspect: Aspect) -> bool {
        match aspect {
            Aspect::Display => self.screen_saver.is_some(),
            Aspect::System => self.idle_lock.is_some(),
        }
    }
}

async fn connect(
    connecting: impl Future<Output = zbus::Result<Connection>>,
) -> Result<Connection, String> {
    match tokio::time::timeout(BUS_TIMEOUT, connecting).await {
        Ok(Ok(conn)) => Ok(conn),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err("connection timed out".to_owned()),
    }
}

async fn call<B>(
    conn: &Connection,
    destination: &str,
    path: &str,
    interface: &str,
    method: &str,
    body: &B,
) -> zbus::Result<zbus::Message>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    tokio::time::timeout(
        BUS_TIMEOUT,
        conn.call_method(Some(destination), path, Some(interface), method, body),
    )
    .await
    .map_err(|_| zbus::Error::Failure(format!("{method} timed out")))?
}

fn spawn_owner_watch(
    conn: Connection,
    name: &'static str,
    aspect: Aspect,
    disruptions: DisruptionSender,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let proxy = match zbus::fdo::DBusProxy::new(&conn).await {
            Ok(proxy) => proxy,
            Err(e) => {
                tracing::warn!(name, error = %e, "stay awake: cannot watch the service; a restart will not be noticed");
                return;
            }
        };
        let changes = match proxy
            .receive_name_owner_changed_with_args(&[(0, name)])
            .await
        {
            Ok(changes) => changes,
            Err(e) => {
                tracing::warn!(name, error = %e, "stay awake: cannot watch the service; a restart will not be noticed");
                return;
            }
        };
        let mut changes = std::pin::pin!(changes);
        while changes.next().await.is_some() {
            if disruptions.send(aspect).is_err() {
                return;
            }
        }
    })
}

fn is_error_named(err: &zbus::Error, names: &[&str]) -> bool {
    matches!(err, zbus::Error::MethodError(name, _, _) if names.contains(&name.as_str()))
}

fn classify(err: zbus::Error, service: &'static str) -> AwakeError {
    if is_error_named(&err, &[SERVICE_UNKNOWN, NAME_HAS_NO_OWNER]) {
        return AwakeError::ServiceMissing { service };
    }
    refused(service, err)
}

fn refused(service: &'static str, err: impl std::fmt::Display) -> AwakeError {
    AwakeError::Refused {
        service,
        reason: err.to_string(),
    }
}
