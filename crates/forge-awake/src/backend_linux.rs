use std::time::Duration;

use async_trait::async_trait;
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;
use zbus::Connection;
use zbus::OwnedFd;

use crate::Aspect;
use crate::backend::{AwakeBackend, DisruptionSender};
use crate::{AwakeError, AwakeService};

const APP_NAME: &str = "forge";

const SCREEN_SAVER_NAME: &str = "org.freedesktop.ScreenSaver";
const SCREEN_SAVER_PATHS: [&str; 2] = ["/org/freedesktop/ScreenSaver", "/ScreenSaver"];

const LOGIN_NAME: &str = "org.freedesktop.login1";
const LOGIN_PATH: &str = "/org/freedesktop/login1";
const LOGIN_MANAGER: &str = "org.freedesktop.login1.Manager";
const INHIBIT_METHOD: &str = "Inhibit";
const UNINHIBIT_METHOD: &str = "UnInhibit";
const IDLE_LOCK: &str = "idle";
const BLOCK_MODE: &str = "block";

const SERVICE_UNKNOWN: &str = "org.freedesktop.DBus.Error.ServiceUnknown";
const NAME_HAS_NO_OWNER: &str = "org.freedesktop.DBus.Error.NameHasNoOwner";
const UNKNOWN_OBJECT: &str = "org.freedesktop.DBus.Error.UnknownObject";
const UNKNOWN_METHOD: &str = "org.freedesktop.DBus.Error.UnknownMethod";

const SESSION_BUS: AwakeService = AwakeService::SessionBus;
const SYSTEM_BUS: AwakeService = AwakeService::SystemBus;
const SCREEN_SAVER_SERVICE: AwakeService = AwakeService::ScreenSaver;
const LOGIN_SERVICE: AwakeService = AwakeService::LoginManager;

const BUS_TIMEOUT: Duration = Duration::from_secs(5);
const CONNECT_TIMED_OUT: &str = "connection timed out";

struct ScreenSaverHold {
    cookie: u32,
    path: &'static str,
}

pub(crate) struct LinuxBackend {
    reason: String,
    session: Result<Connection, AwakeError>,
    system: Result<Connection, AwakeError>,
    screen_saver: Option<ScreenSaverHold>,
    idle_lock: Option<OwnedFd>,
    disruptions: DisruptionSender,
    watchers: Vec<JoinHandle<()>>,
}

impl LinuxBackend {
    pub(crate) async fn open(reason: String, disruptions: DisruptionSender) -> Self {
        let (session, system) = tokio::join!(
            connect(SESSION_BUS, Connection::session()),
            connect(SYSTEM_BUS, Connection::system())
        );
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
                disruptions.clone(),
            ));
        }
        Self {
            reason,
            session,
            system,
            screen_saver: None,
            idle_lock: None,
            disruptions,
            watchers,
        }
    }

    async fn session_connection(&mut self) -> Result<Connection, AwakeError> {
        if let Ok(conn) = &self.session {
            return Ok(conn.clone());
        }
        self.session = connect(SESSION_BUS, Connection::session()).await;
        match &self.session {
            Ok(conn) => {
                self.watchers.push(spawn_owner_watch(
                    conn.clone(),
                    SCREEN_SAVER_NAME,
                    Aspect::Display,
                    self.disruptions.clone(),
                ));
                Ok(conn.clone())
            }
            Err(error) => Err(error.clone()),
        }
    }

    async fn system_connection(&mut self) -> Result<Connection, AwakeError> {
        if let Ok(conn) = &self.system {
            return Ok(conn.clone());
        }
        self.system = connect(SYSTEM_BUS, Connection::system()).await;
        match &self.system {
            Ok(conn) => {
                self.watchers.push(spawn_owner_watch(
                    conn.clone(),
                    LOGIN_NAME,
                    Aspect::System,
                    self.disruptions.clone(),
                ));
                Ok(conn.clone())
            }
            Err(error) => Err(error.clone()),
        }
    }

    async fn inhibit_screen_saver(&mut self) -> Result<(), AwakeError> {
        let conn = self.session_connection().await?;
        let mut missing_object = None;
        for path in SCREEN_SAVER_PATHS {
            match call(
                &conn,
                SCREEN_SAVER_NAME,
                path,
                SCREEN_SAVER_NAME,
                INHIBIT_METHOD,
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
                Err(e) => {
                    if is_connection_lost(&e) {
                        self.session = Err(unreachable(SESSION_BUS, &e));
                    }
                    return Err(classify(e, SCREEN_SAVER_SERVICE));
                }
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
            UNINHIBIT_METHOD,
            &(hold.cookie,),
        )
        .await
        {
            tracing::debug!(error = %e, "stay awake: screen saver inhibit was already gone");
        }
    }

    async fn take_idle_lock(&mut self) -> Result<(), AwakeError> {
        let conn = self.system_connection().await?;
        let reply = call(
            &conn,
            LOGIN_NAME,
            LOGIN_PATH,
            LOGIN_MANAGER,
            INHIBIT_METHOD,
            &(IDLE_LOCK, APP_NAME, self.reason.as_str(), BLOCK_MODE),
        )
        .await
        .map_err(|e| {
            if is_connection_lost(&e) {
                self.system = Err(unreachable(SYSTEM_BUS, &e));
            }
            classify(e, LOGIN_SERVICE)
        })?;
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
    bus: AwakeService,
    connecting: impl Future<Output = zbus::Result<Connection>>,
) -> Result<Connection, AwakeError> {
    match tokio::time::timeout(BUS_TIMEOUT, connecting).await {
        Ok(Ok(conn)) => Ok(conn),
        Ok(Err(e)) => Err(unreachable(bus, &e)),
        Err(_) => Err(unreachable(bus, &CONNECT_TIMED_OUT)),
    }
}

fn unreachable(bus: AwakeService, cause: &dyn std::fmt::Display) -> AwakeError {
    AwakeError::Unreachable {
        service: bus,
        reason: cause.to_string(),
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
        if let Err(e) = forward_owner_changes(&conn, name, aspect, &disruptions).await {
            tracing::warn!(name, error = %e, "stay awake: cannot watch the service; a restart will not be noticed");
        }
    })
}

async fn forward_owner_changes(
    conn: &Connection,
    name: &'static str,
    aspect: Aspect,
    disruptions: &DisruptionSender,
) -> zbus::Result<()> {
    let proxy = zbus::fdo::DBusProxy::new(conn).await?;
    let changes = proxy
        .receive_name_owner_changed_with_args(&[(0, name)])
        .await?;
    let mut changes = std::pin::pin!(changes);
    while changes.next().await.is_some() {
        if disruptions.send(aspect).is_err() {
            break;
        }
    }
    Ok(())
}

fn is_connection_lost(err: &zbus::Error) -> bool {
    matches!(err, zbus::Error::InputOutput(_))
}

fn is_error_named(err: &zbus::Error, names: &[&str]) -> bool {
    matches!(err, zbus::Error::MethodError(name, _, _) if names.contains(&name.as_str()))
}

fn classify(err: zbus::Error, service: AwakeService) -> AwakeError {
    if is_error_named(&err, &[SERVICE_UNKNOWN, NAME_HAS_NO_OWNER]) {
        return AwakeError::ServiceMissing { service };
    }
    refused(service, err)
}

fn refused(service: AwakeService, err: impl std::fmt::Display) -> AwakeError {
    AwakeError::Refused {
        service,
        reason: err.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::sync::Arc;

    use super::*;

    #[test]
    fn only_a_broken_transport_counts_as_a_lost_connection() {
        let broken = zbus::Error::InputOutput(Arc::new(io::Error::from(io::ErrorKind::BrokenPipe)));
        let answered = zbus::Error::Failure("the service said no".to_owned());

        assert!(is_connection_lost(&broken));
        assert!(!is_connection_lost(&answered));
    }
}
