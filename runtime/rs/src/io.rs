// io extern — Rust native implementations

use std::io::Write;

// The stdout buffer, `almide_stdout_flush`, `almide_stdout_write_fmt` and
// `almide_stdout_write_bytes` live in the runtime PRELUDE (#2245: every
// stdout write — `println` included — goes through the one buffer, so it
// must exist in a program that never imports `io`). This module only adds
// the `io.*` surface over it.

// print is for interactive output (prompts, streaming tokens) — it always
// flushes, so the text appears immediately even when stdout is block-
// buffered (#648). It is also the explicit flush: `io.print("")`.
pub fn almide_rt_io_print(s: &str) {
    almide_stdout_write_bytes(s.as_bytes());
    almide_stdout_flush();
}

pub fn almide_rt_io_read_line() -> String {
    almide_stdout_flush();
    let mut buf = String::new();
    std::io::stdin().read_line(&mut buf).unwrap_or(0);
    buf.trim_end_matches('\n').trim_end_matches('\r').to_string()
}

// `read_line` answers "" both for an empty line and at end of input, so a loop
// that skips empty lines never sees the end (#2539). This twin says which: `None`
// when stdin had nothing left, `Some("")` for a line that was only a newline.
// The line is decoded lossily (as io.read_all's twin does), so a stray invalid
// byte costs one U+FFFD instead of the whole line — the wasm self-host
// (stdlib/io_read_line_opt.almd) decodes the same way.
pub fn almide_rt_io_read_line_opt() -> Option<String> {
    use std::io::BufRead;
    almide_stdout_flush();
    let mut buf: Vec<u8> = Vec::new();
    match std::io::stdin().lock().read_until(b'\n', &mut buf) {
        Ok(0) | Err(_) => None,
        Ok(_) => {
            let line = String::from_utf8_lossy(&buf);
            Some(line.trim_end_matches('\n').trim_end_matches('\r').to_string())
        }
    }
}

pub fn almide_rt_io_read_all() -> String {
    almide_stdout_flush();
    use std::io::Read;
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf).unwrap_or(0);
    buf
}

pub fn almide_rt_io_read_byte() -> i64 {
    almide_stdout_flush();
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
    almide_stdout_flush();
    if n <= 0 { return Vec::new(); }
    let mut buf: Vec<u8> = Vec::new();
    let _ = std::io::stdin().take(n as u64).read_to_end(&mut buf);
    buf.into_iter().map(|b| b as i64).collect()
}

// Byte writes share the buffer with `println` (#2245), so they interleave in
// PROGRAM order without a flush per call (the C-162 fixture pins the order on
// both legs); a terminal still sees each write as it happens.
pub fn almide_rt_io_write_bytes(data: &Vec<i64>) {
    let bytes: Vec<u8> = data.iter().map(|&b| b as u8).collect();
    almide_stdout_write_bytes(&bytes);
}

pub fn almide_rt_io_write(data: &Vec<u8>) {
    almide_stdout_write_bytes(data);
}
