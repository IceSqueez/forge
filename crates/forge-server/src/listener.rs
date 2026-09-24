use std::future::Future as _;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};
use std::time::Duration;

use axum::extract::connect_info::Connected;
use axum::serve::{IncomingStream, Listener};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::{Instant, Sleep};

pub(crate) const MAX_CONCURRENT_CONNECTIONS: usize = 256;
pub(crate) const REQUEST_READ_TIMEOUT: Duration = Duration::from_secs(15);

pub(crate) struct GuardedListener {
    inner: TcpListener,
    permits: Arc<Semaphore>,
}

impl GuardedListener {
    pub(crate) fn new(inner: TcpListener) -> Self {
        Self {
            inner,
            permits: Arc::new(Semaphore::new(MAX_CONCURRENT_CONNECTIONS)),
        }
    }
}

impl Listener for GuardedListener {
    type Io = GuardedStream;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let (stream, addr) = Listener::accept(&mut self.inner).await;
            match Arc::clone(&self.permits).try_acquire_owned() {
                Ok(permit) => return (GuardedStream::new(stream, permit), addr),
                Err(_) => {
                    tracing::debug!(%addr, "connection dropped: server is at its connection cap");
                }
            }
        }
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        self.inner.local_addr()
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
}

impl GuardedStream {
    fn new(stream: TcpStream, permit: OwnedSemaphorePermit) -> Self {
        Self {
            stream,
            read_deadline: Box::pin(tokio::time::sleep(REQUEST_READ_TIMEOUT)),
            upgraded: Arc::new(AtomicBool::new(false)),
            _permit: permit,
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
