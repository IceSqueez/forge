use std::collections::HashMap;
use std::future::Future as _;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex, PoisonError};
use std::task::{Context, Poll};
use std::time::Duration;

use axum::extract::connect_info::Connected;
use axum::serve::{IncomingStream, Listener};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::{Instant, Sleep};

pub(crate) const MAX_CONCURRENT_CONNECTIONS: usize = 256;
pub(crate) const PER_IP_MAX_CONNECTIONS: usize = 64;
pub(crate) const REQUEST_READ_TIMEOUT: Duration = Duration::from_secs(15);

pub(crate) struct GuardedListener {
    inner: TcpListener,
    permits: Arc<Semaphore>,
    per_ip: Arc<PerIpLimiter>,
}

impl GuardedListener {
    pub(crate) fn new(inner: TcpListener) -> Self {
        Self {
            inner,
            permits: Arc::new(Semaphore::new(MAX_CONCURRENT_CONNECTIONS)),
            per_ip: Arc::new(PerIpLimiter::default()),
        }
    }
}

impl Listener for GuardedListener {
    type Io = GuardedStream;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let (stream, addr) = Listener::accept(&mut self.inner).await;
            let permit = match Arc::clone(&self.permits).try_acquire_owned() {
                Ok(permit) => permit,
                Err(_) => {
                    tracing::debug!(%addr, "connection dropped: server is at its connection cap");
                    continue;
                }
            };
            let ip = addr.ip();
            let per_ip_permit = if ip.is_loopback() {
                None
            } else {
                match self.per_ip.try_acquire(ip) {
                    Some(per_ip_permit) => Some(per_ip_permit),
                    None => {
                        tracing::debug!(
                            %addr,
                            "connection dropped: address is at its per-IP connection cap"
                        );
                        continue;
                    }
                }
            };
            return (GuardedStream::new(stream, permit, per_ip_permit), addr);
        }
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        self.inner.local_addr()
    }
}

#[derive(Default)]
struct PerIpLimiter {
    counts: StdMutex<HashMap<IpAddr, usize>>,
}

impl PerIpLimiter {
    fn try_acquire(self: &Arc<Self>, ip: IpAddr) -> Option<PerIpPermit> {
        let mut counts = self.counts.lock().unwrap_or_else(PoisonError::into_inner);
        let slot = counts.entry(ip).or_insert(0);
        if *slot >= PER_IP_MAX_CONNECTIONS {
            return None;
        }
        *slot += 1;
        Some(PerIpPermit {
            limiter: Arc::clone(self),
            ip,
        })
    }
}

struct PerIpPermit {
    limiter: Arc<PerIpLimiter>,
    ip: IpAddr,
}

impl Drop for PerIpPermit {
    fn drop(&mut self) {
        let mut counts = self
            .limiter
            .counts
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(slot) = counts.get_mut(&self.ip) {
            *slot -= 1;
            if *slot == 0 {
                counts.remove(&self.ip);
            }
        }
    }
}

/// Until the connection is upgraded to a WebSocket, a read that has not completed within
/// `REQUEST_READ_TIMEOUT` of accept or of the last response write fails with `TimedOut`, which
/// bounds both a slow request head and an idle keep-alive.
pub(crate) struct GuardedStream {
    stream: TcpStream,
    read_deadline: Pin<Box<Sleep>>,
    upgraded: Arc<AtomicBool>,
    _permit: OwnedSemaphorePermit,
    _per_ip_permit: Option<PerIpPermit>,
}

impl GuardedStream {
    fn new(
        stream: TcpStream,
        permit: OwnedSemaphorePermit,
        per_ip_permit: Option<PerIpPermit>,
    ) -> Self {
        Self {
            stream,
            read_deadline: Box::pin(tokio::time::sleep(REQUEST_READ_TIMEOUT)),
            upgraded: Arc::new(AtomicBool::new(false)),
            _permit: permit,
            _per_ip_permit: per_ip_permit,
        }
    }

    fn is_upgraded(&self) -> bool {
        self.upgraded.load(Ordering::Acquire)
    }

    fn read_deadline_passed(&mut self, cx: &mut Context<'_>) -> bool {
        !self.is_upgraded() && self.read_deadline.as_mut().poll(cx).is_ready()
    }

    fn rearm_read_deadline(&mut self) {
        if !self.is_upgraded() {
            self.read_deadline
                .as_mut()
                .reset(Instant::now() + REQUEST_READ_TIMEOUT);
        }
    }

    fn after_write(&mut self, written: &Poll<io::Result<usize>>) {
        if matches!(written, Poll::Ready(Ok(bytes)) if *bytes > 0) {
            self.rearm_read_deadline();
        }
    }
}

impl AsyncRead for GuardedStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if this.read_deadline_passed(cx) {
            return Poll::Ready(Err(io::Error::from(io::ErrorKind::TimedOut)));
        }
        Pin::new(&mut this.stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for GuardedStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        let written = Pin::new(&mut this.stream).poll_write(cx, buf);
        this.after_write(&written);
        written
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        let written = Pin::new(&mut this.stream).poll_write_vectored(cx, bufs);
        this.after_write(&written);
        written
    }

    fn is_write_vectored(&self) -> bool {
        self.stream.is_write_vectored()
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().stream).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().stream).poll_shutdown(cx)
    }
}

#[derive(Clone)]
pub(crate) struct PeerInfo {
    pub(crate) addr: SocketAddr,
    upgraded: Arc<AtomicBool>,
}

impl PeerInfo {
    /// Lifts the read deadline for good: a WebSocket may stay silent for as long as it likes.
    pub(crate) fn mark_upgraded(&self) {
        self.upgraded.store(true, Ordering::Release);
    }
}

impl Connected<IncomingStream<'_, GuardedListener>> for PeerInfo {
    fn connect_info(stream: IncomingStream<'_, GuardedListener>) -> Self {
        Self {
            addr: *stream.remote_addr(),
            upgraded: Arc::clone(&stream.io().upgraded),
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::io;
    use std::net::{IpAddr, Ipv4Addr};
    use std::sync::Arc;
    use std::time::Duration;

    use axum::serve::Listener;
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    use tokio::net::{TcpListener, TcpStream};
    use tokio::sync::Semaphore;

    use super::{
        GuardedListener, GuardedStream, MAX_CONCURRENT_CONNECTIONS, PER_IP_MAX_CONNECTIONS,
        PerIpLimiter, REQUEST_READ_TIMEOUT,
    };

    const BYTE_BUDGET: Duration = Duration::from_secs(5);
    const MARGIN: Duration = Duration::from_secs(1);

    async fn guarded_pair() -> (TcpStream, GuardedStream) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let client = TcpStream::connect(listener.local_addr().expect("addr"))
            .await
            .expect("connect");
        let (server, _) = listener.accept().await.expect("accept");
        let permit = Arc::new(Semaphore::new(1))
            .try_acquire_owned()
            .expect("permit");
        (client, GuardedStream::new(server, permit, None))
    }

    async fn read_one(stream: &mut GuardedStream) -> io::Result<usize> {
        let mut buf = [0_u8; 16];
        stream.read(&mut buf).await
    }

    #[tokio::test(start_paused = true)]
    async fn a_trickling_request_head_is_cut_at_the_deadline_counted_from_accept() {
        let (mut client, mut guarded) = guarded_pair().await;

        tokio::time::sleep(REQUEST_READ_TIMEOUT - MARGIN).await;
        client.write_all(b"G").await.expect("write");
        assert_eq!(
            read_one(&mut guarded)
                .await
                .expect("a byte inside the deadline is read"),
            1
        );

        tokio::time::sleep(MARGIN * 2).await;
        client.write_all(b"E").await.expect("write");
        let late = read_one(&mut guarded).await;

        assert_eq!(
            late.expect_err("reading must not push the deadline out")
                .kind(),
            io::ErrorKind::TimedOut
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_response_write_restarts_the_read_deadline() {
        let (mut client, mut guarded) = guarded_pair().await;

        tokio::time::sleep(REQUEST_READ_TIMEOUT - MARGIN).await;
        guarded
            .write_all(b"HTTP/1.1 204 No Content\r\n\r\n")
            .await
            .expect("respond");
        let responded_at = tokio::time::Instant::now();
        tokio::time::sleep(MARGIN * 2).await;
        client.write_all(b"G").await.expect("write");

        assert_eq!(
            read_one(&mut guarded)
                .await
                .expect("a keep-alive request after a response is read past the first deadline"),
            1
        );
        let idle = read_one(&mut guarded).await;
        assert_eq!(
            idle.expect_err("an idle keep-alive must still time out")
                .kind(),
            io::ErrorKind::TimedOut
        );
        assert!(responded_at.elapsed() >= REQUEST_READ_TIMEOUT);
    }

    #[tokio::test]
    async fn the_connection_cap_drops_the_one_over_it_until_a_held_connection_closes() {
        let inner = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = inner.local_addr().expect("addr");
        let mut listener = GuardedListener::new(inner);
        let mut clients = Vec::with_capacity(MAX_CONCURRENT_CONNECTIONS);
        let mut held = Vec::with_capacity(MAX_CONCURRENT_CONNECTIONS);
        for _ in 0..MAX_CONCURRENT_CONNECTIONS {
            clients.push(TcpStream::connect(addr).await.expect("connect"));
            held.push(Listener::accept(&mut listener).await.0);
        }

        let mut over_cap = TcpStream::connect(addr).await.expect("connect over cap");
        let accepting = tokio::spawn(async move { Listener::accept(&mut listener).await.1 });
        let mut buf = [0_u8; 1];
        let closed = tokio::time::timeout(BYTE_BUDGET, over_cap.read(&mut buf))
            .await
            .expect("the connection over the cap was left hanging");
        assert!(
            matches!(closed, Ok(0) | Err(_)),
            "the connection over the cap was served"
        );

        held.pop();
        let admitted = TcpStream::connect(addr)
            .await
            .expect("connect after a slot frees");
        let accepted_peer = tokio::time::timeout(BYTE_BUDGET, accepting)
            .await
            .expect("a freed slot must admit the next connection")
            .expect("accept task");
        assert_eq!(accepted_peer, admitted.local_addr().expect("client addr"));
    }

    // Why: 127.0.0.2 is still `is_loopback()`, so a real second peer address cannot be dialled
    // in-process; the per-IP cap and its cross-address isolation are driven directly against
    // `PerIpLimiter`, the same seam `GuardedListener::accept` calls for a non-loopback peer.
    fn non_loopback_ip(last_octet: u8) -> IpAddr {
        // RFC 5737 TEST-NET-3, guaranteed non-routable and never loopback.
        IpAddr::V4(Ipv4Addr::new(203, 0, 113, last_octet))
    }

    #[test]
    fn the_permit_past_the_per_ip_cap_is_refused_while_another_address_is_unaffected() {
        let limiter = Arc::new(PerIpLimiter::default());
        let busy = non_loopback_ip(1);
        let mut held = Vec::with_capacity(PER_IP_MAX_CONNECTIONS);
        for _ in 0..PER_IP_MAX_CONNECTIONS {
            held.push(limiter.try_acquire(busy).expect("under the per-IP cap"));
        }

        assert!(
            limiter.try_acquire(busy).is_none(),
            "the permit past the cap must be refused"
        );
        assert!(
            limiter.try_acquire(non_loopback_ip(2)).is_some(),
            "a saturated address must not affect a different address"
        );
    }

    #[test]
    fn dropping_a_per_ip_permit_frees_a_slot_for_the_same_address() {
        let limiter = Arc::new(PerIpLimiter::default());
        let ip = non_loopback_ip(3);
        let mut held = Vec::with_capacity(PER_IP_MAX_CONNECTIONS);
        for _ in 0..PER_IP_MAX_CONNECTIONS {
            held.push(limiter.try_acquire(ip).expect("under the per-IP cap"));
        }
        assert!(limiter.try_acquire(ip).is_none(), "at the cap");

        held.pop();

        assert!(
            limiter.try_acquire(ip).is_some(),
            "a permit freed by drop must be available to acquire again"
        );
    }

    #[tokio::test]
    async fn loopback_connections_are_exempt_from_the_per_ip_cap() {
        let inner = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = inner.local_addr().expect("addr");
        let mut listener = GuardedListener::new(inner);

        let mut clients = Vec::with_capacity(PER_IP_MAX_CONNECTIONS + 1);
        let mut held = Vec::with_capacity(PER_IP_MAX_CONNECTIONS + 1);
        for _ in 0..=PER_IP_MAX_CONNECTIONS {
            clients.push(TcpStream::connect(addr).await.expect("connect"));
            let (stream, _addr) =
                tokio::time::timeout(BYTE_BUDGET, Listener::accept(&mut listener))
                    .await
                    .expect("a per-IP cap applied to loopback would hang this accept");
            held.push(stream);
        }

        assert_eq!(
            held.len(),
            PER_IP_MAX_CONNECTIONS + 1,
            "loopback connections from the same address must not be capped"
        );
    }
}
