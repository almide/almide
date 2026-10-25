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

// One line, read as BYTES and decoded lossily (#4142): each invalid sequence becomes one
// U+FFFD (`String::from_utf8_lossy`), as the wasm self-host (stdlib/io_read_line_opt.almd)
// decodes it. `None` when stdin had nothing left, `Some("")` for a line that was only a
// newline (#2539, R-io-01): a loop that skips empty lines still sees the end. An I/O error
// ends the input, as it does on the wasm legs (an errno ends their fd_read loop).
pub fn almide_rt_io_read_line() -> Option<AlmideString> {
    let __almide_v: Option<String> = {
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
    };
    __almide_v.into_almide()
}

pub fn almide_rt_io_read_all() -> AlmideString {
    let __almide_out: String = {
        almide_stdout_flush();
        use std::io::Read;
        // Bytes first, decoded lossily once (#4142): `read_to_string` refused the whole
        // stream on one invalid byte and `unwrap_or(0)` answered "", where the wasm
        // self-host (stdlib/io_read_all.almd) decodes the same bytes with U+FFFD. The
        // bytes read before an I/O error are kept — the wasm fd_read loop stops there too.
        let mut buf: Vec<u8> = Vec::new();
        let _ = std::io::stdin().lock().read_to_end(&mut buf);
        String::from_utf8_lossy(&buf).into_owned()
    };
    __almide_out.into()
}

// `io.read(io.stdin(), n)` (the leaf `__read_stdin_bytes`) reads UP TO n bytes and answers
// what it got. The count is a LIMIT,
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
/// the stdin read cannot drift apart in the ONE way that matters: the answer's length.
/// Keep equal to the literal in stdlib/io_read_n_bytes.almd.
pub const ALMIDE_IO_READ_CHUNK_BYTES: i64 = 1 << 26;

// The stdin read answers min(n, what stdin has). No ceiling, no clamp.
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
pub fn almide_rt_io_read_bytes(n: i64) -> Vec<u8> {
    use std::io::Read;
    almide_stdout_flush();
    if n <= 0 { return Vec::new(); }
    // Slices of at most 64 KiB read straight into one zeroed-once chunk and
    // widened into the answer: a slice of 8 KiB or more bypasses stdin's
    // buffer, so its bytes are not copied through it. `take(n).read_to_end`
    // grew its Vec from 32 bytes and sized each read adaptively, and the reads
    // that fit stdin's buffer went through it and its copy — all 20000 bytes
    // of a call at n = 20000, 8 KiB of one at n >= 80000 (GCE c4-standard-4),
    // a per-call cost that moved with n. Nothing is sized from n past one slice.
    const SLICE: usize = 1 << 16;
    thread_local! {
        static CHUNK: std::cell::RefCell<Vec<u8>> = std::cell::RefCell::new(vec![0u8; SLICE]);
    }
    CHUNK.with(|chunk| {
        let mut chunk = chunk.borrow_mut();
        let mut out: Vec<u8> = Vec::with_capacity((n as u64).min(SLICE as u64) as usize);
        let mut left = n as u64;
        let mut input = std::io::stdin().lock();
        while left > 0 {
            let want = left.min(SLICE as u64) as usize;
            match input.read(&mut chunk[..want]) {
                Ok(0) => break,
                Ok(got) => {
                    out.extend_from_slice(&chunk[..got]);
                    left -= got as u64;
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
        out
    })
}

// Byte writes share the buffer with `println` (#2245), so they interleave in
// PROGRAM order without a flush per call (the C-162 fixture pins the order on
// both legs); a terminal still sees each write as it happens.
pub fn almide_rt_io_write(data: &Vec<u8>) {
    almide_stdout_write_bytes(data);
}

// stderr is unbuffered: stdout is flushed first so the two streams keep
// program order on a terminal, as `eprintln` does.
pub fn almide_rt_io_write_stderr(data: &Vec<u8>) {
    almide_stdout_flush();
    let _ = std::io::stderr().write_all(data);
}

pub fn almide_rt_io_eprint(s: &str) {
    almide_stdout_flush();
    let _ = std::io::stderr().write_all(s.as_bytes());
}
