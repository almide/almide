// io extern — Rust native implementations

use std::io::Write;
use std::cell::RefCell;

thread_local! {
    static ALMIDE_STDOUT_BUF: RefCell<std::io::BufWriter<std::io::Stdout>> =
        RefCell::new(std::io::BufWriter::with_capacity(65536, std::io::stdout()));
}

/// Flush the buffered stdout writer. Called at program exit.
pub fn almide_rt_io_flush() {
    ALMIDE_STDOUT_BUF.with(|buf| { let _ = buf.borrow_mut().flush(); });
}

// print is for interactive output (prompts, streaming tokens) — flush so
// the text appears immediately even when stdout is block-buffered (#648).
pub fn almide_rt_io_print(s: &str) {
    print!("{}", s);
    let _ = std::io::stdout().flush();
}

pub fn almide_rt_io_read_line() -> String {
    let mut buf = String::new();
    std::io::stdin().read_line(&mut buf).unwrap_or(0);
    buf.trim_end_matches('\n').trim_end_matches('\r').to_string()
}

pub fn almide_rt_io_read_all() -> String {
    use std::io::Read;
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf).unwrap_or(0);
    buf
}

pub fn almide_rt_io_read_byte() -> i64 {
    use std::io::Read;
    let mut buf = [0u8; 1];
    match std::io::stdin().read(&mut buf) {
        Ok(1) => buf[0] as i64,
        _ => -1,
    }
}

// `read_n_bytes(n)` reads UP TO n bytes and answers what it got. The count is a LIMIT,
// not a size to reserve — so nothing here is allocated from it.
//
// The old body did `vec![0u8; n as usize]` first: a negative n became ~1.8e19 and the
// allocation aborted the process, and n = i64::MAX asked for 9 exabytes, where the wasm
// leg — which never reserved — simply answered the empty list. A ceiling would have made
// the two agree by teaching BOTH to abort; `take` makes them agree by teaching this one
// not to need the count at all, which is the better answer: `read_n_bytes(i64::MAX)` on
// empty stdin is honestly the empty list, not an error.
/// The chunk this leg's SELF-HOST twin reads in. Native has no reason to chunk — it
/// hands `n` straight to `take` — but the constant is rostered so the two halves of
/// `read_n_bytes` cannot drift apart in the ONE way that matters: the answer's length.
/// Keep equal to the literal in stdlib/io_read_n_bytes.almd.
pub const ALMIDE_IO_READ_CHUNK_BYTES: i64 = 1 << 26;

// `read_n_bytes(n)` answers min(n, what stdin has). No ceiling, no clamp.
//
// A clamp shipped here briefly and it was WRONG: it capped the answer at 2^26, so
// `read_n_bytes(100 MiB)` on 100 MiB of stdin returned 64 MiB and no error, on BOTH
// legs, for a call that used to work correctly on both. Silent truncation of a
// caller's data is the worst outcome this function has.
//
// The measurement that justified it was taken with FIVE BYTES on stdin: a large `n`
// failed there because the wasm floor pre-allocates the REQUESTED size, not because
// the data could not be delivered. With real input, 100 MiB round-trips fine. The
// real defect was only ever `n` too large to REPRESENT at the i32 host boundary
// (i64::MAX truncated to -1 and read nothing), and the self-host twin now solves that
// by looping over 2^26 chunks instead of asking the floor for the whole span at once.
pub fn almide_rt_io_read_n_bytes(n: i64) -> Vec<i64> {
    use std::io::Read;
    if n <= 0 { return Vec::new(); }
    let mut buf: Vec<u8> = Vec::new();
    let _ = std::io::stdin().take(n as u64).read_to_end(&mut buf);
    buf.into_iter().map(|b| b as i64).collect()
}

// `println!`/`print!` write through Rust's own `Stdout` handle, NOT through
// ALMIDE_STDOUT_BUF — two independent buffers over one fd. Without a flush here, a
// program interleaving `println` and `io.write` emitted them in BUFFER order
// (every io.write deferred to the exit flush) instead of PROGRAM order, while
// the wasm leg's direct `fd_write` kept program order: a cross-target stdout
// divergence, and wrong output even native-only. Flushing at the end of each
// write hands the bytes to the shared `Stdout` in program order; the buffer
// still batches WITHIN one call, which is where the byte-streaming benches
// spend their time.
pub fn almide_rt_io_write_bytes(data: &Vec<i64>) {
    ALMIDE_STDOUT_BUF.with(|buf| {
        let mut w = buf.borrow_mut();
        let bytes: Vec<u8> = data.iter().map(|&b| b as u8).collect();
        w.write_all(&bytes).unwrap();
        let _ = w.flush();
    });
}

pub fn almide_rt_io_write(data: &Vec<u8>) {
    ALMIDE_STDOUT_BUF.with(|buf| {
        let mut w = buf.borrow_mut();
        w.write_all(data).unwrap();
        let _ = w.flush();
    });
}
