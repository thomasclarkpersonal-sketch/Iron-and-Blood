//! Whatever bytes a client sends, in whatever chunks they arrive, the server's frame
//! reader must split, verify and read them without panicking (D22, NETWORK_PROTOCOL
//! §9). This drives the same calls as `pax_server`'s connection task and its request
//! decoder: `FrameDecoder` on arbitrary chunking, then `read_client_message`, then
//! every field of every payload.
#![no_main]

use libfuzzer_sys::fuzz_target;
use pax_protocol::{Direction, FrameDecoder, read_client_message, wire};

fuzz_target!(|data: &[u8]| {
    // The first byte picks the chunk size, so chunk boundaries are fuzzed too.
    let Some((&chunk, bytes)) = data.split_first() else { return };
    let chunk = 1 + chunk as usize;
    let mut decoder = FrameDecoder::new(Direction::ClientToServer);
    for piece in bytes.chunks(chunk) {
        decoder.push(piece);
        loop {
            match decoder.next_frame() {
                Ok(Some(frame)) => read_everything(&frame),
                Ok(None) => break,
                Err(_) => return, // the connection would end with Goodbye
            }
        }
    }
});

fn read_everything(frame: &[u8]) {
    let Ok(msg) = read_client_message(frame) else { return };
    use wire::ClientPayload as P;
    match msg.payload_type() {
        P::Hello => {
            if let Some(h) = msg.payload_as_hello() {
                let _ =
                    (h.protocol_major(), h.protocol_minor(), h.client_name(), h.requested_nation(), h.resume_token());
            }
        }
        P::SubmitCommand => {
            if let Some(s) = msg.payload_as_submit_command() {
                let _ = s.client_seq();
                let _ = s.command_as_set_income_tax().map(|c| (c.nation(), c.rate().map(|r| r.raw())));
                let _ = s.command_as_set_transfer_rate().map(|c| (c.nation(), c.rate().map(|r| r.raw())));
                let _ = s.command_as_set_consumption_rate().map(|c| (c.nation(), c.rate().map(|r| r.raw())));
            }
        }
        P::SetSpeed => {
            let _ = msg.payload_as_set_speed().map(|s| s.speed());
        }
        P::Subscribe => {
            let _ = msg.payload_as_subscribe().map(|s| (s.map_mode(), s.map_good(), s.market(), s.province()));
        }
        P::Ack => {
            let _ = msg.payload_as_ack().map(|a| a.day());
        }
        P::Ping => {
            let _ = msg.payload_as_ping().map(|p| p.nonce());
        }
        P::SaveGame => {
            let _ = msg.payload_as_save_game().map(|s| s.name());
        }
        P::LoadGame => {
            let _ = msg.payload_as_load_game().map(|s| s.name());
        }
        P::ListSaves => {
            let _ = msg.payload_as_list_saves();
        }
        _ => {}
    }
}
