// io extern — Rust native implementations

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
