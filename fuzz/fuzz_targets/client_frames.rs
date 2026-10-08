//! Whatever bytes a client sends, in whatever chunks they arrive, the server must
//! split, verify and decode them without panicking (D22, NETWORK_PROTOCOL §9). This
//! runs exactly what `pax_server`'s connection task runs on its input: a
//! `FrameDecoder` fed in fuzzed chunks, then the server's own request decoder
//! (`request::decode`, exposed by the `fuzzing` feature) on every frame.
#![no_main]

use libfuzzer_sys::fuzz_target;
use pax_protocol::{Direction, FrameDecoder};
use pax_server::fuzzing::decode;

fuzz_target!(|data: &[u8]| {
    // The first byte picks the chunk size, so chunk boundaries are fuzzed too.
    let Some((&chunk, bytes)) = data.split_first() else { return };
    let chunk = 1 + chunk as usize;
    let mut decoder = FrameDecoder::new(Direction::ClientToServer);
    for piece in bytes.chunks(chunk) {
        decoder.push(piece);
        loop {
            match decoder.next_frame() {
                // An error ends the session with Goodbye, as in the server.
                Ok(Some(frame)) => {
                    if decode(&frame).is_err() {
                        return;
                    }
                }
                Ok(None) => break,
                Err(_) => return,
            }
        }
    }
});
