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

pub fn almide_rt_io_print(s: String) { print!("{}", s); }

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

pub fn almide_rt_io_write_bytes(data: &Vec<i64>) {
    STDOUT_BUF.with(|buf| {
        let mut w = buf.borrow_mut();
        let bytes: Vec<u8> = data.iter().map(|&b| b as u8).collect();
        w.write_all(&bytes).unwrap();
    });
}
