use std::io;
use std::path::Path;
use std::sync::mpsc as thread_mpsc;
use std::thread;
use std::time::Duration;

use gpui::App;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::sync::mpsc;

const RAISE_REQUEST: &[u8] = b"forge-raise\n";
const REQUEST_DEADLINE: Duration = Duration::from_secs(2);
const REQUEST_READ_TIMEOUT: Duration = Duration::from_secs(1);

pub type RaiseRequests = mpsc::UnboundedReceiver<()>;

pub fn ask_running_instance(data_dir: &Path) -> io::Result<()> {
    let data_dir = data_dir.to_owned();
    let (done_tx, done_rx) = thread_mpsc::channel();
    thread::Builder::new()
        .name("forge-raise-request".to_owned())
        .spawn(move || {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "after the deadline nobody waits for this result"
            )]
            let _ = done_tx.send(endpoint::send(&data_dir, RAISE_REQUEST));
        })?;
    done_rx
        .recv_timeout(REQUEST_DEADLINE)
        .map_err(|_| io::Error::from(io::ErrorKind::TimedOut))?
}

pub fn listen(data_dir: &Path) -> io::Result<RaiseRequests> {
    let (requests_tx, requests_rx) = mpsc::unbounded_channel();
    endpoint::serve(data_dir, requests_tx)?;
    Ok(requests_rx)
}

pub fn follow_raise_requests(mut requests: RaiseRequests, cx: &mut App) {
    cx.spawn(async move |cx| {
        while requests.recv().await.is_some() {
            cx.update(raise_windows);
        }
    })
    .detach();
}

fn raise_windows(cx: &mut App) {
    tracing::info!("second launch asked to raise the window");
    cx.activate(true);
    for window in cx.windows() {
        if let Err(error) = window.update(cx, |_, window, _| window.activate_window()) {
            tracing::debug!(%error, "window closed before it could be raised");
        }
    }
}

async fn read_raise_request(mut client: impl AsyncRead + Unpin) -> bool {
    let mut received = [0_u8; RAISE_REQUEST.len()];
    matches!(
        tokio::time::timeout(REQUEST_READ_TIMEOUT, client.read_exact(&mut received)).await,
        Ok(Ok(_))
    ) && received == RAISE_REQUEST
}

fn forward_request(requests: &mpsc::UnboundedSender<()>) -> bool {
    requests.send(()).is_ok()
}

#[cfg(unix)]
mod endpoint {
    use std::io::{self, Write as _};
    use std::os::unix::net::UnixStream;
    use std::path::{Path, PathBuf};

    use tokio::net::UnixListener;
    use tokio::sync::mpsc;

    use super::{forward_request, read_raise_request};

    const SOCKET_FILE_NAME: &str = "forge.sock";

    fn socket_path(data_dir: &Path) -> PathBuf {
        data_dir.join(SOCKET_FILE_NAME)
    }

    pub(super) fn send(data_dir: &Path, request: &[u8]) -> io::Result<()> {
        UnixStream::connect(socket_path(data_dir))?.write_all(request)
    }

    pub(super) fn serve(data_dir: &Path, requests: mpsc::UnboundedSender<()>) -> io::Result<()> {
        let path = socket_path(data_dir);
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let listener = UnixListener::bind(path)?;
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((client, _)) => {
                        if read_raise_request(client).await && !forward_request(&requests) {
                            return;
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%error, "raise socket stopped accepting");
                        return;
                    }
                }
            }
        });
        Ok(())
    }
}

#[cfg(windows)]
mod endpoint {
    use std::fs::OpenOptions;
    use std::hash::{DefaultHasher, Hash as _, Hasher as _};
    use std::io::{self, Write as _};
    use std::path::Path;

    use tokio::net::windows::named_pipe::ServerOptions;
    use tokio::sync::mpsc;

    use super::{forward_request, read_raise_request};

    fn pipe_name(data_dir: &Path) -> String {
        let mut hasher = DefaultHasher::new();
        data_dir.hash(&mut hasher);
        format!(r"\\.\pipe\forge-{:016x}", hasher.finish())
    }

    pub(super) fn send(data_dir: &Path, request: &[u8]) -> io::Result<()> {
        OpenOptions::new()
            .write(true)
            .open(pipe_name(data_dir))?
            .write_all(request)
    }

    pub(super) fn serve(data_dir: &Path, requests: mpsc::UnboundedSender<()>) -> io::Result<()> {
        let name = pipe_name(data_dir);
        let mut server = ServerOptions::new().create(&name)?;
        tokio::spawn(async move {
            loop {
                if let Err(error) = server.connect().await {
                    tracing::warn!(%error, "raise pipe stopped accepting");
                    return;
                }
                let client = server;
                server = match ServerOptions::new().create(&name) {
                    Ok(next) => next,
                    Err(error) => {
                        tracing::warn!(%error, "raise pipe could not be reopened");
                        return;
                    }
                };
                if read_raise_request(client).await && !forward_request(&requests) {
                    return;
                }
            }
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncWriteExt as _, duplex};

    use super::*;

    #[tokio::test]
    async fn the_exact_request_is_a_raise() {
        let (mut client, server) = duplex(RAISE_REQUEST.len());
        client.write_all(RAISE_REQUEST).await.unwrap();

        assert!(read_raise_request(server).await);
    }

    #[tokio::test]
    async fn anything_but_the_exact_request_is_no_raise() {
        for sent in [
            &b"forge-raisE\n"[..],
            &b"GET / HTTP/1.1\r\n"[..],
            &b"forge-raise"[..],
            &[][..],
        ] {
            let (mut client, server) = duplex(64);
            client.write_all(sent).await.unwrap();
            drop(client);

            assert!(!read_raise_request(server).await, "{sent:?}");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_client_that_stays_silent_is_given_up_on_at_the_read_timeout() {
        let (mut client, server) = duplex(64);
        client.write_all(&RAISE_REQUEST[..1]).await.unwrap();
        let started = tokio::time::Instant::now();

        let raised = read_raise_request(server).await;

        assert!(!raised);
        assert_eq!(started.elapsed(), REQUEST_READ_TIMEOUT);
    }

    #[test]
    fn a_raise_is_not_forwarded_once_nobody_listens() {
        let (requests, listener) = mpsc::unbounded_channel();

        assert!(forward_request(&requests));
        drop(listener);
        assert!(!forward_request(&requests));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_second_launch_reaches_the_listener_past_a_leftover_socket_file() {
        let data_dir = tempfile::tempdir().unwrap();
        std::fs::write(data_dir.path().join("forge.sock"), b"").unwrap();
        let mut requests = listen(data_dir.path()).unwrap();

        ask_running_instance(data_dir.path()).unwrap();

        let raised = tokio::time::timeout(Duration::from_secs(30), requests.recv())
            .await
            .expect("the raise never reached the listener");
        assert_eq!(raised, Some(()));
    }

    #[cfg(unix)]
    #[test]
    fn asking_with_no_running_instance_fails() {
        let data_dir = tempfile::tempdir().unwrap();

        assert!(ask_running_instance(data_dir.path()).is_err());
    }
}
