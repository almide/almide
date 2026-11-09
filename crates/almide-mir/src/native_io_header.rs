// Signal shutdown probes the same stdout handle without blocking on a borrow.
thread_local! { static ALMIDE_STDOUT_BUF: std::cell::RefCell<std::io::Stdout> = std::cell::RefCell::new(std::io::stdout()); }
// The native floor shares std's line-buffered stdout with its println shims.
// A threaded fan substitutes its timeline sink for these three functions.
fn almide_stdout_flush() {
    let _ = std::io::Write::flush(&mut std::io::stdout());
}
fn almide_stdout_write_bytes(bytes: &[u8]) {
    let mut out = std::io::stdout().lock();
    let _ = std::io::Write::write_all(&mut out, bytes);
    if bytes.contains(&b'\n') || std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        let _ = std::io::Write::flush(&mut out);
    }
}
fn almide_stderr_write_bytes(bytes: &[u8]) {
    almide_stdout_flush();
    let _ = std::io::Write::write_all(&mut std::io::stderr().lock(), bytes);
}
