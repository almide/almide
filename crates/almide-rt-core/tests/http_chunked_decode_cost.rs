//! The chunked decoder's cost is linear in the bytes it is handed, not in
//! bytes × chunks (the GCE inspection of http.request_stream, 2026-10-17:
//! native moved ~2000× the body for `/chunked/{n}` — every chunk's size line,
//! data and CRLF were `drain`ed off the FRONT of the read buffer, each drain
//! moving everything behind it).
//!
//! One `http_call_decode` over a buffer of 200,000 one-byte chunks (1.2 MB)
//! must decode them all at once in well under a second; front-draining per
//! chunk moves ~120 GB for it.
use almide_rt_core::http_client_core::{http_call_decode, HttpCallFraming};

fn many_one_byte_chunks(n: usize) -> Vec<u8> {
    let mut raw = Vec::with_capacity(n * 6 + 5);
    for i in 0..n {
        raw.extend_from_slice(b"1\r\n");
        raw.push(b'a' + (i % 26) as u8);
        raw.extend_from_slice(b"\r\n");
    }
    raw.extend_from_slice(b"0\r\n\r\n");
    raw
}

#[test]
fn a_buffer_of_many_chunks_decodes_in_time_linear_in_its_length() {
    let n = 200_000;
    let mut raw = many_one_byte_chunks(n);
    let mut out = Vec::new();
    let mut f = HttpCallFraming::Chunked { remaining: 0, awaiting_size: true };
    let t = std::time::Instant::now();
    let complete = http_call_decode(&mut f, &mut raw, &mut out).expect("decodes");
    let took = t.elapsed();
    assert!(complete, "the terminal chunk ends the body");
    assert_eq!(out.len(), n);
    assert!(out.iter().enumerate().all(|(i, b)| *b == b'a' + (i % 26) as u8));
    // Only the CRLF that closes the (empty) trailer section may remain.
    assert!(raw.len() <= 2, "every consumed byte leaves the buffer: {} left", raw.len());
    assert!(took < std::time::Duration::from_millis(500), "decoding {n} chunks took {took:?}");
}

#[test]
fn a_chunk_split_across_buffers_keeps_its_state() {
    let whole = many_one_byte_chunks(1000);
    let mut out = Vec::new();
    let mut f = HttpCallFraming::Chunked { remaining: 0, awaiting_size: true };
    let mut raw = Vec::new();
    let mut complete = false;
    // Feed 7 bytes at a time: every cut point of the framing is crossed.
    for piece in whole.chunks(7) {
        raw.extend_from_slice(piece);
        complete = http_call_decode(&mut f, &mut raw, &mut out).expect("decodes");
    }
    assert!(complete);
    assert_eq!(out.len(), 1000);
    assert!(raw.len() <= 2, "{} left", raw.len());
}
