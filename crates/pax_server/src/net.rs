//! The network side (D22, D23): one tokio task per connection.
//!
//! A connection task splits the byte stream into frames, verifies and decodes them
//! into [`Request`]s, enforces the session rules that need no game state, and passes
//! the rest to the sim thread:
//! * `Hello` must come first, and only once;
//! * a silent client times out;
//! * `Ping` is answered here.
//!
//! Any protocol error ends the session with `Goodbye` and the reason (D22). The sim
//! thread answers through the connection's outbound channel.

use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::Duration;

use pax_protocol::{Direction, FrameDecoder};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Notify, mpsc};
use tracing::{debug, info, warn};

use crate::encode;
use crate::request::{self, Request};

/// What the sim thread sends a connection.
#[derive(Debug)]
pub(crate) enum Outbound {
    /// A complete frame to write.
    Frame(Vec<u8>),
    /// Flush what was queued, then close the connection.
    Close,
}

/// What connections tell the sim thread.
pub(crate) enum Inbound {
    Connected {
        session: u64,
        out: mpsc::UnboundedSender<Outbound>,
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
}

pub(crate) async fn accept_loop(listener: TcpListener, to_sim: Sender<Inbound>, idle: Duration) {
    let mut next_session = 1u64;
    loop {
        match listener.accept().await {
            Ok((stream, peer)) => {
                let session = next_session;
                next_session += 1;
                info!(session, %peer, "connection");
                // Small, latency-sensitive messages: don't wait to coalesce them.
                let _ = stream.set_nodelay(true);
                tokio::spawn(connection(stream, session, to_sim.clone(), idle));
            }
            Err(e) => {
                // Usually transient (e.g. out of file descriptors): back off briefly.
                warn!(error = %e, "accept failed");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

async fn connection(stream: TcpStream, session: u64, to_sim: Sender<Inbound>, idle: Duration) {
    let (mut rd, mut wr) = stream.into_split();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Outbound>();
    if to_sim.send(Inbound::Connected { session, out: out_tx.clone() }).is_err() {
        return; // the server is shutting down
    }

    // The writer drains the outbound queue in order. When it closes the connection
    // it wakes the reader, which must stop too.
    let writer_done = Arc::new(Notify::new());
    let writer = tokio::spawn({
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
                let _ = out_tx.send(Outbound::Frame(encode::pong(nonce)));
                continue;
            }
            debug!(session, ?request, "request");
            if to_sim.send(Inbound::Request { session, request }).is_err() {
                break 'read Some("the server is shutting down".to_owned());
            }
        }
    };

    if let Some(reason) = &goodbye {
        info!(session, %reason, "closing session");
        let _ = out_tx.send(Outbound::Frame(encode::goodbye(reason)));
    }
    let _ = out_tx.send(Outbound::Close);
    let _ = to_sim.send(Inbound::Closed { session });
    let _ = writer.await;
    info!(session, "disconnected");
}
