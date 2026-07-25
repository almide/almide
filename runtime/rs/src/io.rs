// io extern — Rust native implementations

use std::io::Write;
use std::cell::RefCell;

thread_local! {
    static STDOUT_BUF: RefCell<std::io::BufWriter<std::io::Stdout>> =
        RefCell::new(std::io::BufWriter::with_capacity(65536, std::io::stdout()));
}

/// Flush the buffered stdout writer. Called at program exit.
pub fn almide_rt_io_flush() {
    STDOUT_BUF.with(|buf| { let _ = buf.borrow_mut().flush(); });
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

pub fn almide_rt_io_read_n_bytes(n: i64) -> Vec<i64> {
    use std::io::Read;
    let n = n as usize;
    let mut buf = vec![0u8; n];
    let mut total = 0;
    while total < n {
        match std::io::stdin().read(&mut buf[total..]) {
            Ok(0) => break,
            Ok(k) => total += k,
            Err(_) => break,
        }
    }
    buf[..total].iter().map(|&b| b as i64).collect()
}

// `println!`/`print!` write through Rust's own `Stdout` handle, NOT through
// STDOUT_BUF — two independent buffers over one fd. Without a flush here, a
// program interleaving `println` and `io.write` emitted them in BUFFER order
// (every io.write deferred to the exit flush) instead of PROGRAM order, while
// the wasm leg's direct `fd_write` kept program order: a cross-target stdout
// divergence, and wrong output even native-only. Flushing at the end of each
// write hands the bytes to the shared `Stdout` in program order; the buffer
// still batches WITHIN one call, which is where the byte-streaming benches
// spend their time.
pub fn almide_rt_io_write_bytes(data: &Vec<i64>) {
    STDOUT_BUF.with(|buf| {
        let mut w = buf.borrow_mut();
        let bytes: Vec<u8> = data.iter().map(|&b| b as u8).collect();
        w.write_all(&bytes).unwrap();
        let _ = w.flush();
    });
}

pub fn almide_rt_io_write(data: &Vec<u8>) {
    STDOUT_BUF.with(|buf| {
        let mut w = buf.borrow_mut();
        w.write_all(data).unwrap();
        let _ = w.flush();
    });
}
