//! Whatever bytes a client sends, in whatever chunks they arrive, the server must
//! split, verify and decode them without panicking (D22, NETWORK_PROTOCOL §9). This
//! drives the connection task's own input side, `RequestReader` (exposed by
//! `pax_server`'s `fuzzing` feature): framing, the request decoder and the `Hello`
//! rules, with fuzzed chunk boundaries.
#![no_main]

use libfuzzer_sys::fuzz_target;
use pax_server::fuzzing::RequestReader;

fuzz_target!(|data: &[u8]| {
    // The first byte picks the chunk size, so chunk boundaries are fuzzed too.
    let Some((&chunk, bytes)) = data.split_first() else { return };
    let chunk = 1 + chunk as usize;
    let mut reader = RequestReader::default();
    for piece in bytes.chunks(chunk) {
        reader.push(piece);
        loop {
            match reader.next_request() {
                Ok(Some(_)) => {}
                Ok(None) => break,
                // The session would end with Goodbye here, as in the server.
                Err(_) => return,
            }
        }
    }
});
