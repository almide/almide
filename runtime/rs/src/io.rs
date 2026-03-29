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

pub fn almide_rt_io_write_bytes(data: &Vec<i64>) {
    use std::io::Write;
    let bytes: Vec<u8> = data.iter().map(|&b| b as u8).collect();
    std::io::stdout().write_all(&bytes).unwrap();
}
