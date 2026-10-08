//! Message framing (D22): every message is a 4-byte little-endian length followed by
//! that many bytes of FlatBuffer. This is FlatBuffers' own *size-prefixed* layout, so
//! a frame is exactly what `finish_size_prefixed_*` writes, and exactly what
//! `size_prefixed_root_as_*` reads.
//!
//! [`FrameDecoder`] does no IO. Callers push whatever bytes their socket delivered, in
//! any chunking, and take out whole frames. The declared length is checked against
//! the limit as soon as its 4 bytes arrive, before any of the body is buffered, so a
//! hostile peer can't make the receiver hold more than `limit + 4` bytes.

/// Why the byte stream can't be split into frames. Fatal to the connection: the
/// decoder refuses all further input after returning one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameError {
    /// The declared body length exceeds this direction's limit.
    TooLarge { len: usize, limit: usize },
    /// A zero-length frame, which can't hold a FlatBuffer.
    Empty,
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameError::TooLarge { len, limit } => write!(f, "frame of {len} bytes exceeds the {limit}-byte limit"),
            FrameError::Empty => write!(f, "empty frame"),
        }
    }
}

impl std::error::Error for FrameError {}

/// Splits a byte stream into size-prefixed frames, without doing any IO.
#[derive(Debug)]
pub struct FrameDecoder {
    buf: Vec<u8>,
    limit: usize,
    failed: Option<FrameError>,
}

impl FrameDecoder {
    /// A decoder accepting bodies of at most `limit` bytes
    /// ([`crate::MAX_CLIENT_FRAME`] or [`crate::MAX_SERVER_FRAME`]).
    pub fn new(limit: usize) -> Self {
        FrameDecoder { buf: Vec::new(), limit, failed: None }
    }

    /// Appends bytes as they arrived from the socket. After an error, input is
    /// ignored: the connection is being closed.
    pub fn push(&mut self, bytes: &[u8]) {
        if self.failed.is_none() {
            self.buf.extend_from_slice(bytes);
        }
    }

    /// The next complete frame, *including* its 4-byte length prefix (the form the
    /// size-prefixed readers expect), or `None` if more bytes are needed.
    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>, FrameError> {
        if let Some(e) = self.failed {
            return Err(e);
        }
        let Some(prefix) = self.buf.first_chunk::<4>() else {
            return Ok(None);
        };
        let len = u32::from_le_bytes(*prefix) as usize;
        let error = if len == 0 {
            Some(FrameError::Empty)
        } else if len > self.limit {
            Some(FrameError::TooLarge { len, limit: self.limit })
        } else {
            None
        };
        if let Some(e) = error {
            self.failed = Some(e);
            self.buf = Vec::new();
            return Err(e);
        }
        if self.buf.len() < 4 + len {
            return Ok(None);
        }
        let rest = self.buf.split_off(4 + len);
        Ok(Some(std::mem::replace(&mut self.buf, rest)))
    }

    /// Bytes received but not yet returned as a frame.
    pub fn buffered(&self) -> usize {
        self.buf.len()
    }
}
