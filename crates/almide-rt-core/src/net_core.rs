// net_core (#3620, the net/tcp design): the tcp family, ONE source for two
// consumers —
//
// - runtime/rs/src/net.rs `include!`s this file verbatim and keeps one
//   `AlmideNetTables` per thread, so the native `almide_rt_net_tcp_*`
//   intrinsics are thin wrappers over these functions;
// - almide-wasm-run links this crate and keeps one table per run for the
//   embedded host's net ops (dropped with the run, which closes every socket,
//   as process exit does natively).
//
// Handles are plain integers: streams and listeners are separate tables, each
// counting from 0, and a new handle takes the lowest free slot. Every err is
// net_error_core's text — the class of the failure, never the platform's
// errno string.
//
// Splice discipline (lib.rs): no `crate::` paths, no `mod`, no `use` lines.

include!("net_error_core.rs");

#[derive(Default)]
pub struct AlmideNetTables {
    streams: Vec<Option<std::net::TcpStream>>,
    listeners: Vec<Option<std::net::TcpListener>>,
}

fn almide_net_slot<T>(v: &mut Vec<Option<T>>, x: T) -> i64 {
    match v.iter().position(|s| s.is_none()) {
        Some(i) => {
            v[i] = Some(x);
            i as i64
        }
        None => {
            v.push(Some(x));
            (v.len() - 1) as i64
        }
    }
}

/// The slot behind `h`: the invalid-handle err past the end (a negative
/// handle included), the closed err for an emptied slot.
fn almide_net_get<'a, T>(v: &'a mut [Option<T>], kind: &str, h: i64) -> Result<&'a mut T, String> {
    let i = usize::try_from(h).ok().filter(|i| *i < v.len()).ok_or_else(|| net_err_invalid_handle(kind, h))?;
    v[i].as_mut().ok_or_else(|| net_err_closed_handle(kind, h))
}

fn almide_net_len(f: &str, len: i64) -> Result<usize, String> {
    usize::try_from(len).map_err(|_| net_err_negative_len(f, len))
}

/// Resolve `host:port`. An IP literal (IPv6 bare or in brackets) is parsed,
/// never looked up; a name goes to the platform resolver.
fn almide_net_resolve(f: &str, host: &str, port: i64) -> Result<Vec<std::net::SocketAddr>, String> {
    let head = format!("{f}({host}:{port})");
    let port = u16::try_from(port).map_err(|_| net_err_port_range(&head))?;
    let bare = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')).unwrap_or(host);
    if let Ok(ip) = bare.parse::<std::net::IpAddr>() {
        return Ok(vec![std::net::SocketAddr::new(ip, port)]);
    }
    match std::net::ToSocketAddrs::to_socket_addrs(&(bare, port)) {
        Ok(addrs) => {
            let addrs: Vec<_> = addrs.collect();
            if addrs.is_empty() { Err(net_err_unresolvable(&head, host)) } else { Ok(addrs) }
        }
        Err(_) => Err(net_err_unresolvable(&head, host)),
    }
}

/// Try each address in turn; the last failure classifies the err.
fn almide_net_each<T>(f: &str, host: &str, port: i64, mut go: impl FnMut(&std::net::SocketAddr) -> std::io::Result<T>) -> Result<T, String> {
    let addrs = almide_net_resolve(f, host, port)?;
    let mut last = None;
    for a in &addrs {
        match go(a) {
            Ok(x) => return Ok(x),
            Err(e) => last = Some(e),
        }
    }
    let head = format!("{f}({host}:{port})");
    Err(last.map_or_else(|| net_err_unresolvable(&head, host), |e| net_io_error_text(&head, &e)))
}

impl AlmideNetTables {
    fn stream(&mut self, h: i64) -> Result<&mut std::net::TcpStream, String> {
        almide_net_get(&mut self.streams, "TcpStream", h)
    }

    fn listener(&mut self, h: i64) -> Result<&mut std::net::TcpListener, String> {
        almide_net_get(&mut self.listeners, "TcpListener", h)
    }

    pub fn tcp_connect(&mut self, host: &str, port: i64) -> Result<i64, String> {
        let s = almide_net_each("tcp_connect", host, port, |a| std::net::TcpStream::connect(a))?;
        Ok(almide_net_slot(&mut self.streams, s))
    }

    pub fn tcp_listen(&mut self, host: &str, port: i64) -> Result<i64, String> {
        let l = almide_net_each("tcp_listen", host, port, |a| std::net::TcpListener::bind(a))?;
        Ok(almide_net_slot(&mut self.listeners, l))
    }

    pub fn tcp_local_port(&mut self, h: i64) -> Result<i64, String> {
        let l = self.listener(h)?;
        let a = l.local_addr().map_err(|e| net_io_error_text("tcp_local_port", &e))?;
        Ok(a.port() as i64)
    }

    pub fn tcp_accept(&mut self, h: i64) -> Result<i64, String> {
        let (s, _) = self.listener(h)?.accept().map_err(|e| net_io_error_text("tcp_accept", &e))?;
        Ok(almide_net_slot(&mut self.streams, s))
    }

    pub fn tcp_read(&mut self, h: i64, len: i64) -> Result<Vec<u8>, String> {
        let n = almide_net_len("tcp_read", len)?;
        almide_net_read_once(self.stream(h)?, n, "tcp_read")
    }

    /// Read until `len` bytes arrived; EOF first is an err naming how many did.
    pub fn tcp_read_exact(&mut self, h: i64, len: i64) -> Result<Vec<u8>, String> {
        let n = almide_net_len("tcp_read_exact", len)?;
        let s = self.stream(h)?;
        let mut buf = vec![0u8; n];
        let mut got = 0;
        while got < n {
            match std::io::Read::read(s, &mut buf[got..]) {
                Ok(0) => return Err(net_err_short_read(got, len)),
                Ok(k) => got += k,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => return Err(net_io_error_text("tcp_read_exact", &e)),
            }
        }
        Ok(buf)
    }

    /// One read bounded by `ms`; the stream's own timeout (`tcp_set_timeout`)
    /// is back in force afterwards.
    pub fn tcp_read_timeout(&mut self, h: i64, len: i64, ms: i64) -> Result<Vec<u8>, String> {
        let s = self.stream(h)?;
        let n = almide_net_len("tcp_read_timeout", len)?;
        if ms <= 0 {
            return Err(net_err_timeout_not_positive(ms));
        }
        let before = s.read_timeout().map_err(|e| net_io_error_text("tcp_read_timeout", &e))?;
        s.set_read_timeout(Some(std::time::Duration::from_millis(ms as u64)))
            .map_err(|e| net_io_error_text("tcp_read_timeout", &e))?;
        let r = almide_net_read_once(s, n, "tcp_read_timeout");
        let _ = s.set_read_timeout(before);
        r
    }

    pub fn tcp_write(&mut self, h: i64, data: &[u8]) -> Result<(), String> {
        let s = self.stream(h)?;
        std::io::Write::write_all(s, data).map_err(|e| net_io_error_text("tcp_write", &e))?;
        std::io::Write::flush(s).map_err(|e| net_io_error_text("tcp_write", &e))
    }

    /// The bytes readable now without waiting, at most 65536; 0 at EOF.
    pub fn tcp_available(&mut self, h: i64) -> Result<i64, String> {
        let s = self.stream(h)?;
        s.set_nonblocking(true).map_err(|e| net_io_error_text("tcp_available", &e))?;
        let mut buf = vec![0u8; 65536];
        let r = s.peek(&mut buf);
        let _ = s.set_nonblocking(false);
        match r {
            Ok(n) => Ok(n as i64),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(0),
            Err(e) => Err(net_io_error_text("tcp_available", &e)),
        }
    }

    /// `ms <= 0` clears the timeout.
    pub fn tcp_set_timeout(&mut self, h: i64, ms: i64) -> Result<(), String> {
        let s = self.stream(h)?;
        let d = (ms > 0).then(|| std::time::Duration::from_millis(ms as u64));
        s.set_read_timeout(d).map_err(|e| net_io_error_text("tcp_set_timeout", &e))?;
        s.set_write_timeout(d).map_err(|e| net_io_error_text("tcp_set_timeout", &e))
    }

    /// Closing a closed stream is ok; a handle never issued is not.
    pub fn tcp_close(&mut self, h: i64) -> Result<(), String> {
        let i = usize::try_from(h).ok().filter(|i| *i < self.streams.len())
            .ok_or_else(|| net_err_invalid_handle("TcpStream", h))?;
        if let Some(s) = self.streams[i].take() {
            let _ = s.shutdown(std::net::Shutdown::Both);
        }
        Ok(())
    }

    pub fn tcp_close_listener(&mut self, h: i64) -> Result<(), String> {
        let i = usize::try_from(h).ok().filter(|i| *i < self.listeners.len())
            .ok_or_else(|| net_err_invalid_handle("TcpListener", h))?;
        self.listeners[i].take();
        Ok(())
    }

    pub fn tcp_is_open(&self, h: i64) -> bool {
        usize::try_from(h).ok().and_then(|i| self.streams.get(i)).is_some_and(|s| s.is_some())
    }
}

/// One `read` of at most `n` bytes; 0 bytes is EOF (or `n == 0`).
/// A zero-length read answers empty WITHOUT the syscall: Linux's `recv` on
/// a zero-length buffer waits for data where macOS returns at once, so
/// `tcp_read(c, 0)` on an idle stream hung on Linux (#3679's CI).
fn almide_net_read_once(s: &mut std::net::TcpStream, n: usize, f: &str) -> Result<Vec<u8>, String> {
    if n == 0 {
        return Ok(Vec::new());
    }
    let mut buf = vec![0u8; n];
    loop {
        match std::io::Read::read(s, &mut buf) {
            Ok(k) => {
                buf.truncate(k);
                return Ok(buf);
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(net_io_error_text(f, &e)),
        }
    }
}
