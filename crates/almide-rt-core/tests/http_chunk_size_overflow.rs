//! The buffered chunked decoder errs, instead of panicking, on a chunk size
//! near `usize::MAX`. `data_start + size` was checked but `data_end + 2` was
//! not, so a size line the server chose could overflow (a panic with overflow
//! checks on) or wrap to an inverted slice (a panic without them).
use almide_rt_core::http_client_core::decode_chunked_bytes;

#[test]
fn a_chunk_size_near_usize_max_is_an_error_not_a_panic() {
    // "FFFFFFFFFFFFFFED\r\n" puts data_start at 18, so data_end = usize::MAX
    // on 64-bit targets; every size from there to the top must err too.
    for size in ["FFFFFFFFFFFFFFED", "FFFFFFFFFFFFFFEE", "FFFFFFFFFFFFFFFF"] {
        let body = format!("{size}\r\nab\r\n0\r\n\r\n");
        let r = std::panic::catch_unwind(|| decode_chunked_bytes(body.as_bytes()));
        let r = r.unwrap_or_else(|_| panic!("chunk size {size} panicked"));
        assert!(r.is_err(), "chunk size {size} must not decode: {r:?}");
    }
}

#[test]
fn ordinary_chunks_still_decode() {
    let body = b"3\r\nabc\r\n2\r\nde\r\n0\r\n\r\n";
    assert_eq!(decode_chunked_bytes(body).unwrap(), b"abcde");
}
