//! The network side (D22, D23): one tokio task per connection.
//!
//! A connection task splits the byte stream into frames, verifies and decodes them
//! into [`Request`]s, enforces the framing-level session rules, and passes the rest
//! to the sim thread:
//! * `Hello` must come first, and only once;
//! * a silent client times out, and in multiplayer is first reported as stalled
//!   (D24's fairness pause), then as resumed if it speaks again;
//! * `Ping` is answered here.
//!
//! Whether a session is *welcomed*, and so may do anything else, is decided only by
//! the sim thread.
//!
//! Any protocol error ends the session with `Goodbye` and the reason (D22). The sim
//! thread answers through the connection's bounded outbound queue. A client that
//! stops reading fills the queue, and its connection is closed instead of growing
//! memory without limit.

use std::sync::Arc;
use std::time::Duration;

use pax_protocol::{Direction, FrameDecoder};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{Notify, mpsc};
use tracing::{debug, info, warn};

use crate::encode;
use crate::request::{self, Request};

/// Frames a connection may have queued for writing. A client that reads normally
/// never comes close: updates are capped by D23's 3-update window, and replies are
/// one per request.
pub(crate) const OUTBOUND_QUEUE: usize = 256;

/// Events all connections may have queued for the sim thread. When it is full, a
/// connection stops reading its socket until there is room: TCP backpressure,
/// not unbounded memory.
pub(crate) const INBOUND_QUEUE: usize = 1024;

/// After the session ends, how long the writer may take to flush its queue before
/// it is cut off (a client that isn't reading would hold it forever).
const FLUSH_TIMEOUT: Duration = Duration::from_secs(2);

/// How long a connection may stay silent (D22, D24).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Timing {
    /// Silence that closes the session: D22's 10 s in single player, D24's drop
    /// (30 s) in multiplayer.
    pub idle: Duration,
    /// Silence after which the sim thread hears `Stalled`, so it can pause the game
    /// for everyone (D24's fairness pause, 5 s). `None` in single player.
    pub stall_after: Option<Duration>,
}

/// What the sim thread sends a connection.
#[derive(Debug)]
pub(crate) enum Outbound {
    /// A complete frame to write.
    Frame(Vec<u8>),
    /// Flush what was queued, then close the connection.
    Close,
}

/// The sim thread's handle on one connection.
#[derive(Debug)]
pub(crate) struct ConnHandle {
    out: mpsc::Sender<Outbound>,
    kill: Arc<Notify>,
}

impl ConnHandle {
    /// Queues `out` without blocking the sim thread. If the queue is full, the
    /// client isn't reading, so the connection is closed instead. It can't be sent a
    /// `Goodbye`, because its queue is what's full. A closed connection is ignored.
    pub(crate) fn send(&self, out: Outbound) {
        if let Err(TrySendError::Full(_)) = self.out.try_send(out) {
            self.kill.notify_one();
        }
    }
}

#[cfg(test)]
impl ConnHandle {
    /// A handle whose frames the test reads from the returned receiver.
    pub(crate) fn for_test() -> (ConnHandle, mpsc::Receiver<Outbound>) {
        let (out, rx) = mpsc::channel(OUTBOUND_QUEUE);
        (ConnHandle { out, kill: Arc::new(Notify::new()) }, rx)
    }
}

/// What connections tell the sim thread.
pub(crate) enum Inbound {
    Connected {
        session: u64,
        conn: ConnHandle,
    },
    Request {
        session: u64,
        request: Request,
    },
    Closed {
        session: u64,
    },
    /// The client has sent nothing for `Timing::stall_after` (D24).
    Stalled {
        session: u64,
    },
    /// A stalled client spoke again.
    Resumed {
        session: u64,
    },
    /// Stop the server (tests, and future admin commands).
    Shutdown,
    /// Make the sim thread panic, to test how a failure reaches clients and the caller.
    #[cfg(test)]
    Crash,
}

/// The synchronous half of a connection: bytes in, requests out, with the
/// framing-level session rules (`Hello` first, and only once). The connection task
/// feeds it the socket; the cargo-fuzz target (`fuzz/`) feeds it fuzzed bytes, so it
/// fuzzes exactly this code.
#[derive(Debug)]
pub struct RequestReader {
    decoder: FrameDecoder,
    greeted: bool,
}

impl Default for RequestReader {
    fn default() -> Self {
        RequestReader { decoder: FrameDecoder::new(Direction::ClientToServer), greeted: false }
    }
}

impl RequestReader {
    /// Bytes as they arrived, in any chunking.
    pub fn push(&mut self, bytes: &[u8]) {
        self.decoder.push(bytes);
    }

    /// The next request; `Ok(None)` until more bytes arrive. An error is a protocol
    /// error that ends the session with `Goodbye` (D22); its text is the reason.
    pub fn next_request(&mut self) -> Result<Option<Request>, ReadError> {
        let Some(frame) = self.decoder.next_frame().map_err(ReadError::Frame)? else { return Ok(None) };
        let request = request::decode(&frame).map_err(ReadError::Decode)?;
        match (&request, self.greeted) {
            (Request::Hello { .. }, false) => self.greeted = true,
            (Request::Hello { .. }, true) => return Err(ReadError::HelloTwice),
            (_, false) => return Err(ReadError::NoHello),
            (_, true) => {}
        }
        Ok(Some(request))
    }
}

/// Why a connection's input ends the session, by kind, so callers (the fuzz target,
/// future metrics) can tell them apart. `Display` is the `Goodbye` reason.
#[derive(Debug)]
pub enum ReadError {
    /// The byte stream can't be split into frames.
    Frame(pax_protocol::FrameError),
    /// A frame isn't a usable request.
    Decode(request::RequestError),
    /// The session rules: `Hello` first, and only once.
    HelloTwice,
    NoHello,
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadError::Frame(e) => write!(f, "{e}"),
            ReadError::Decode(e) => write!(f, "{e}"),
            ReadError::HelloTwice => write!(f, "Hello sent twice"),
            ReadError::NoHello => write!(f, "the first message must be Hello"),
        }
    }
}

pub(crate) async fn accept_loop(listener: TcpListener, sim: flume::Sender<Inbound>, timing: Timing) {
    let mut next_session = 1u64;
    loop {
        match listener.accept().await {
            Ok((stream, peer)) => {
                let session = next_session;
                next_session += 1;
                info!(session, %peer, "connection");
                // Small, latency-sensitive messages: don't wait to coalesce them.
                let _ = stream.set_nodelay(true);
                tokio::spawn(connection(stream, session, sim.clone(), timing));
            }
            Err(e) => {
                // Usually transient (e.g. out of file descriptors): back off briefly.
                warn!(error = %e, "accept failed");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

async fn connection(stream: TcpStream, session: u64, sim: flume::Sender<Inbound>, timing: Timing) {
    let idle = timing.idle;
    let (mut rd, mut wr) = stream.into_split();
    let (out_tx, mut out_rx) = mpsc::channel::<Outbound>(OUTBOUND_QUEUE);
    let kill = Arc::new(Notify::new());
    let conn = ConnHandle { out: out_tx.clone(), kill: kill.clone() };
    // `send_async` waits, in FIFO order and without blocking the runtime, while the
    // sim thread's queue is full; it fails only once the sim thread has stopped.
    if sim.send_async(Inbound::Connected { session, conn }).await.is_err() {
        return; // the server is shutting down
    }

    // The writer drains the outbound queue in order. When it closes the connection
    // it wakes the reader, which must stop too.
    let writer_done = Arc::new(Notify::new());
    let mut writer = tokio::spawn({
        let writer_done = writer_done.clone();
        async move {
            while let Some(out) = out_rx.recv().await {
                match out {
                    Outbound::Frame(frame) => {
                        if wr.write_all(&frame).await.is_err() {
                            break;
                        }
                    }
                    Outbound::Close => break,
                }
            }
            let _ = wr.shutdown().await;
            writer_done.notify_one();
        }
    });

    let mut reader = RequestReader::default();
    let mut buf = vec![0u8; 16 * 1024];
    // Silence is measured from the last bytes received. A stalled client is
    // reported once, and once more when it speaks again (D24).
    let mut last_heard = tokio::time::Instant::now();
    let mut stalled = false;
    let goodbye: Option<String> = 'read: loop {
        let stall_at = timing.stall_after.filter(|_| !stalled).map(|s| last_heard + s);
        let deadline = stall_at.unwrap_or(last_heard + idle);
        let n = tokio::select! {
            _ = writer_done.notified() => break 'read None,
            _ = kill.notified() => break 'read Some("the client is not reading its messages".to_owned()),
            read = tokio::time::timeout_at(deadline, rd.read(&mut buf)) => match read {
                Err(_) if stall_at.is_some() => {
                    stalled = true;
                    if sim.send_async(Inbound::Stalled { session }).await.is_err() {
                        break 'read Some("the server is shutting down".to_owned());
                    }
                    continue;
                }
                Err(_) => break 'read Some(format!("no message for {} s", idle.as_secs_f32())),
                Ok(Ok(0) | Err(_)) => break 'read None,
                Ok(Ok(n)) => n,
            },
        };
        last_heard = tokio::time::Instant::now();
        if stalled {
            stalled = false;
            if sim.send_async(Inbound::Resumed { session }).await.is_err() {
                break 'read Some("the server is shutting down".to_owned());
            }
        }
        reader.push(&buf[..n]);
        loop {
            let request = match reader.next_request() {
                Ok(Some(request)) => request,
                Ok(None) => break,
                Err(e) => break 'read Some(e.to_string()),
            };
            if let Request::Ping { nonce } = request {
                if out_tx.try_send(Outbound::Frame(encode::pong(nonce))).is_err() {
                    break 'read Some("the client is not reading its messages".to_owned());
                }
                continue;
            }
            debug!(session, ?request, "request");
            if sim.send_async(Inbound::Request { session, request }).await.is_err() {
                break 'read Some("the server is shutting down".to_owned());
            }
        }
    };

    if let Some(reason) = &goodbye {
        info!(session, %reason, "closing session");
        let _ = out_tx.try_send(Outbound::Frame(encode::goodbye(reason)));
    }
    let _ = out_tx.try_send(Outbound::Close);
    let _ = sim.send_async(Inbound::Closed { session }).await;
    // A client that isn't reading would keep the writer blocked forever.
    if tokio::time::timeout(FLUSH_TIMEOUT, &mut writer).await.is_err() {
        writer.abort();
    }
    info!(session, "disconnected");
}
