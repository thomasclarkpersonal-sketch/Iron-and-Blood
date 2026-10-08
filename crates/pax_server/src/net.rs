//! The network side (D22, D23): one tokio task per connection.
//!
//! A connection task splits the byte stream into frames, verifies and decodes them
//! into [`Request`]s, enforces the framing-level session rules, and passes the rest
//! to the sim thread:
//! * `Hello` must come first, and only once;
//! * a silent client times out;
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
use std::sync::mpsc::{SyncSender, TrySendError as SyncTrySendError};
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
    /// Stop the server (tests, and future admin commands).
    Shutdown,
    /// Make the sim thread panic, to test how a failure reaches clients and the caller.
    #[cfg(test)]
    Crash,
}

/// Hands `event` to the sim thread, waiting (without blocking the runtime) while its
/// queue is full. `Err` once the sim thread has stopped.
async fn to_sim(sim: &SyncSender<Inbound>, mut event: Inbound) -> Result<(), ()> {
    loop {
        match sim.try_send(event) {
            Ok(()) => return Ok(()),
            Err(SyncTrySendError::Full(back)) => {
                event = back;
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            Err(SyncTrySendError::Disconnected(_)) => return Err(()),
        }
    }
}

pub(crate) async fn accept_loop(listener: TcpListener, sim: SyncSender<Inbound>, idle: Duration) {
    let mut next_session = 1u64;
    loop {
        match listener.accept().await {
            Ok((stream, peer)) => {
                let session = next_session;
                next_session += 1;
                info!(session, %peer, "connection");
                // Small, latency-sensitive messages: don't wait to coalesce them.
                let _ = stream.set_nodelay(true);
                tokio::spawn(connection(stream, session, sim.clone(), idle));
            }
            Err(e) => {
                // Usually transient (e.g. out of file descriptors): back off briefly.
                warn!(error = %e, "accept failed");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

async fn connection(stream: TcpStream, session: u64, sim: SyncSender<Inbound>, idle: Duration) {
    let (mut rd, mut wr) = stream.into_split();
    let (out_tx, mut out_rx) = mpsc::channel::<Outbound>(OUTBOUND_QUEUE);
    let kill = Arc::new(Notify::new());
    let conn = ConnHandle { out: out_tx.clone(), kill: kill.clone() };
    if to_sim(&sim, Inbound::Connected { session, conn }).await.is_err() {
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

    let mut decoder = FrameDecoder::new(Direction::ClientToServer);
    let mut buf = vec![0u8; 16 * 1024];
    let mut greeted = false;
    let goodbye: Option<String> = 'read: loop {
        let n = tokio::select! {
            _ = writer_done.notified() => break 'read None,
            _ = kill.notified() => break 'read Some("the client is not reading its messages".to_owned()),
            read = tokio::time::timeout(idle, rd.read(&mut buf)) => match read {
                Err(_) => break 'read Some(format!("no message for {} s", idle.as_secs_f32())),
                Ok(Ok(0) | Err(_)) => break 'read None,
                Ok(Ok(n)) => n,
            },
        };
        decoder.push(&buf[..n]);
        loop {
            let frame = match decoder.next_frame() {
                Ok(Some(frame)) => frame,
                Ok(None) => break,
                Err(e) => break 'read Some(e.to_string()),
            };
            let request = match request::decode(&frame) {
                Ok(request) => request,
                Err(e) => break 'read Some(e.to_string()),
            };
            match (&request, greeted) {
                (Request::Hello { .. }, false) => greeted = true,
                (Request::Hello { .. }, true) => break 'read Some("Hello sent twice".to_owned()),
                (_, false) => break 'read Some("the first message must be Hello".to_owned()),
                (_, true) => {}
            }
            if let Request::Ping { nonce } = request {
                if out_tx.try_send(Outbound::Frame(encode::pong(nonce))).is_err() {
                    break 'read Some("the client is not reading its messages".to_owned());
                }
                continue;
            }
            debug!(session, ?request, "request");
            if to_sim(&sim, Inbound::Request { session, request }).await.is_err() {
                break 'read Some("the server is shutting down".to_owned());
            }
        }
    };

    if let Some(reason) = &goodbye {
        info!(session, %reason, "closing session");
        let _ = out_tx.try_send(Outbound::Frame(encode::goodbye(reason)));
    }
    let _ = out_tx.try_send(Outbound::Close);
    let _ = to_sim(&sim, Inbound::Closed { session }).await;
    // A client that isn't reading would keep the writer blocked forever.
    if tokio::time::timeout(FLUSH_TIMEOUT, &mut writer).await.is_err() {
        writer.abort();
    }
    info!(session, "disconnected");
}
