// net — TCP networking runtime for Almide.
// TcpStream/TcpListener are opaque i64 handles backed by a thread-local registry.
//
// NOTE: No top-level `use` for std::io / std::net — avoids duplicate
// import errors when both `net` and `http` runtimes are linked.

use std::cell::RefCell;
#[allow(unused_imports)]
use std::io::{Read as _, Write as _};
use std::time::Duration;

thread_local! {
    static STREAMS: RefCell<Vec<Option<std::net::TcpStream>>> = RefCell::new(Vec::new());
    static LISTENERS: RefCell<Vec<Option<std::net::TcpListener>>> = RefCell::new(Vec::new());
}

fn alloc_stream(s: std::net::TcpStream) -> i64 {
    STREAMS.with(|cell| {
        let mut v = cell.borrow_mut();
        // Reuse freed slots
        for (i, slot) in v.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(s);
                return i as i64;
            }
        }
        let idx = v.len();
        v.push(Some(s));
        idx as i64
    })
}

fn alloc_listener(l: std::net::TcpListener) -> i64 {
    LISTENERS.with(|cell| {
        let mut v = cell.borrow_mut();
        for (i, slot) in v.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(l);
                return i as i64;
            }
        }
        let idx = v.len();
        v.push(Some(l));
        idx as i64
    })
}

fn with_stream<F, R>(handle: i64, f: F) -> Result<R, String>
where F: FnOnce(&mut std::net::TcpStream) -> Result<R, String>
{
    STREAMS.with(|cell| {
        let mut v = cell.borrow_mut();
        let idx = handle as usize;
        if idx >= v.len() {
            return Err(format!("invalid TcpStream handle: {}", handle));
        }
        match v.get_mut(idx) {
            Some(Some(stream)) => f(stream),
            _ => Err(format!("TcpStream handle {} is closed", handle)),
        }
    })
}

// ── Connect ──

pub fn almide_rt_net_tcp_connect(host: &str, port: i64) -> Result<i64, String> {
    let addr = format!("{}:{}", host, port);
    let stream = std::net::TcpStream::connect(&addr)
        .map_err(|e| format!("tcp_connect({}): {}", addr, e))?;
    Ok(alloc_stream(stream))
}

// ── Read / Write ──

pub fn almide_rt_net_tcp_read(handle: i64, len: i64) -> Result<Vec<u8>, String> {
    with_stream(handle, |stream| {
        let mut buf = vec![0u8; len as usize];
        let n = stream.read(&mut buf)
            .map_err(|e| format!("tcp_read: {}", e))?;
        buf.truncate(n);
        Ok(buf)
    })
}

pub fn almide_rt_net_tcp_write(handle: i64, data: &[u8]) -> Result<(), String> {
    with_stream(handle, |stream| {
        stream.write_all(data)
            .map_err(|e| format!("tcp_write: {}", e))?;
        stream.flush()
            .map_err(|e| format!("tcp_write flush: {}", e))?;
        Ok(())
    })
}

pub fn almide_rt_net_tcp_read_exact(handle: i64, len: i64) -> Result<Vec<u8>, String> {
    with_stream(handle, |stream| {
        let mut buf = vec![0u8; len as usize];
        stream.read_exact(&mut buf)
            .map_err(|e| format!("tcp_read_exact: {}", e))?;
        Ok(buf)
    })
}

// ── Close ──

pub fn almide_rt_net_tcp_close(handle: i64) -> Result<(), String> {
    STREAMS.with(|cell| {
        let mut v = cell.borrow_mut();
        let idx = handle as usize;
        if idx >= v.len() {
            return Err(format!("invalid TcpStream handle: {}", handle));
        }
        if let Some(stream) = v[idx].take() {
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
        Ok(())
    })
}

// ── Status ──

pub fn almide_rt_net_tcp_is_open(handle: i64) -> bool {
    STREAMS.with(|cell| {
        let v = cell.borrow();
        let idx = handle as usize;
        idx < v.len() && v[idx].is_some()
    })
}

// ── Timeout ──

pub fn almide_rt_net_tcp_read_timeout(handle: i64, len: i64, timeout_ms: i64) -> Result<Vec<u8>, String> {
    with_stream(handle, |stream| {
        stream.set_read_timeout(Some(Duration::from_millis(timeout_ms as u64)))
            .map_err(|e| format!("tcp_read_timeout: {}", e))?;
        let mut buf = vec![0u8; len as usize];
        let result = stream.read(&mut buf);
        // Reset timeout
        let _ = stream.set_read_timeout(None);
        let n = result.map_err(|e| format!("tcp_read_timeout: {}", e))?;
        buf.truncate(n);
        Ok(buf)
    })
}

// ── Available bytes ──

pub fn almide_rt_net_tcp_available(handle: i64) -> Result<i64, String> {
    with_stream(handle, |stream| {
        // Peek with a zero-length read isn't portable; use nonblocking peek
        stream.set_nonblocking(true)
            .map_err(|e| format!("tcp_available: {}", e))?;
        let mut buf = [0u8; 65536];
        let result = stream.peek(&mut buf);
        let _ = stream.set_nonblocking(false);
        match result {
            Ok(n) => Ok(n as i64),
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(0),
            Err(e) => Err(format!("tcp_available: {}", e)),
        }
    })
}

// ── Server ──

pub fn almide_rt_net_tcp_listen(host: &str, port: i64) -> Result<i64, String> {
    let addr = format!("{}:{}", host, port);
    let listener = std::net::TcpListener::bind(&addr)
        .map_err(|e| format!("tcp_listen({}): {}", addr, e))?;
    Ok(alloc_listener(listener))
}

pub fn almide_rt_net_tcp_accept(handle: i64) -> Result<i64, String> {
    LISTENERS.with(|cell| {
        let v = cell.borrow();
        let idx = handle as usize;
        if idx >= v.len() {
            return Err(format!("invalid TcpListener handle: {}", handle));
        }
        match &v[idx] {
            Some(listener) => {
                let (stream, _addr) = listener.accept()
                    .map_err(|e| format!("tcp_accept: {}", e))?;
                Ok(alloc_stream(stream))
            }
            None => Err(format!("TcpListener handle {} is closed", handle)),
        }
    })
}

pub fn almide_rt_net_tcp_close_listener(handle: i64) -> Result<(), String> {
    LISTENERS.with(|cell| {
        let mut v = cell.borrow_mut();
        let idx = handle as usize;
        if idx >= v.len() {
            return Err(format!("invalid TcpListener handle: {}", handle));
        }
        v[idx].take();
        Ok(())
    })
}

// ── Set timeout ──

pub fn almide_rt_net_tcp_set_timeout(handle: i64, timeout_ms: i64) -> Result<(), String> {
    with_stream(handle, |stream| {
        let dur = if timeout_ms <= 0 { None } else { Some(Duration::from_millis(timeout_ms as u64)) };
        stream.set_read_timeout(dur).map_err(|e| format!("set_timeout: {}", e))?;
        stream.set_write_timeout(dur).map_err(|e| format!("set_timeout: {}", e))?;
        Ok(())
    })
}
