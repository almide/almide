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
    static ALMIDE_STREAMS: RefCell<Vec<Option<std::net::TcpStream>>> = RefCell::new(Vec::new());
    static ALMIDE_LISTENERS: RefCell<Vec<Option<std::net::TcpListener>>> = RefCell::new(Vec::new());
}

fn alloc_stream(s: std::net::TcpStream) -> i64 {
    ALMIDE_STREAMS.with(|cell| {
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
    ALMIDE_LISTENERS.with(|cell| {
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
    ALMIDE_STREAMS.with(|cell| {
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
    ALMIDE_STREAMS.with(|cell| {
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
    ALMIDE_STREAMS.with(|cell| {
        let v = cell.borrow();
        let idx = handle as usize;
        idx < v.len() && v[idx].is_some()
    })
}

// ── Timeout ──

pub fn almide_rt_net_tcp_read_timeout(handle: i64, len: i64, timeout_ms: i64) -> Result<Vec<u8>, String> {
    with_stream(handle, |stream| {
        // A negative duration is an elapsed one (#2486, the env.sleep_ms rule):
        // `timeout_ms as u64` made -1 into u64::MAX milliseconds, a read that
        // never gave up. Clamped to 0 it takes the zero-duration refusal below.
        stream.set_read_timeout(Some(Duration::from_millis(timeout_ms.max(0) as u64)))
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
    ALMIDE_LISTENERS.with(|cell| {
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
    ALMIDE_LISTENERS.with(|cell| {
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

// ══════════════════════════════════════════════════════════════════════════
// Unix-domain sockets and shared-memory files
// ══════════════════════════════════════════════════════════════════════════
//
// The local half of `net`: a stream socket to a peer on the same machine, the
// file descriptors it can carry alongside its bytes (SCM_RIGHTS), and the
// shared-memory files such descriptors usually point at. This is the floor of
// Wayland, X11's shm extension, PipeWire and D-Bus fd passing.
//
// Unlike `tcp_*`, these speak RAW file descriptors, not registry handles: a
// descriptor is what crosses to the peer, and one received from it has no
// handle until the program gives it one. Every descriptor opened here is
// close-on-exec.

#[cfg(unix)]
mod almide_net_unix {
    use std::collections::{HashMap, VecDeque};
    use std::os::fd::{FromRawFd as _, IntoRawFd as _};

    thread_local! {
        /// Descriptors received on each socket, in arrival order, until taken.
        pub static RECEIVED: std::cell::RefCell<HashMap<i64, VecDeque<i64>>> =
            std::cell::RefCell::new(HashMap::new());
    }

    /// The most descriptors one message carries or takes: Wayland's limit,
    /// and far above what any one request needs.
    pub const MAX_FDS: usize = 28;

    #[repr(C)]
    pub struct IoVec { pub base: *mut u8, pub len: usize }

    // struct msghdr / cmsghdr and the socket-level constants differ between
    // Linux (glibc and musl alike on 64-bit: size_t lengths) and the BSDs.
    #[cfg(target_os = "linux")]
    #[repr(C)]
    pub struct MsgHdr {
        pub name: *mut u8, pub namelen: u32,
        pub iov: *mut IoVec, pub iovlen: usize,
        pub control: *mut u8, pub controllen: usize,
        pub flags: i32,
    }
    #[cfg(not(target_os = "linux"))]
    #[repr(C)]
    pub struct MsgHdr {
        pub name: *mut u8, pub namelen: u32,
        pub iov: *mut IoVec, pub iovlen: i32,
        pub control: *mut u8, pub controllen: u32,
        pub flags: i32,
    }
    #[cfg(target_os = "linux")]
    pub type CmsgLen = usize;
    #[cfg(not(target_os = "linux"))]
    pub type CmsgLen = u32;
    #[cfg(target_os = "linux")]
    pub const SOL_SOCKET: i32 = 1;
    #[cfg(not(target_os = "linux"))]
    pub const SOL_SOCKET: i32 = 0xffff;
    pub const SCM_RIGHTS: i32 = 1;
    /// MSG_NOSIGNAL on send (a closed peer is an error, not SIGPIPE) and
    /// MSG_CMSG_CLOEXEC on receive, where the platform has them.
    #[cfg(target_os = "linux")]
    pub const SEND_FLAGS: i32 = 0x4000;
    #[cfg(not(target_os = "linux"))]
    pub const SEND_FLAGS: i32 = 0;
    #[cfg(target_os = "linux")]
    pub const RECV_FLAGS: i32 = 0x4000_0000;
    #[cfg(not(target_os = "linux"))]
    pub const RECV_FLAGS: i32 = 0;
    /// Control-message alignment: size_t on Linux, 4 bytes on the BSDs.
    pub const CMSG_ALIGN: usize = std::mem::size_of::<CmsgLen>();
    pub const CMSG_HDR: usize = (std::mem::size_of::<CmsgLen>() + 8 + CMSG_ALIGN - 1) & !(CMSG_ALIGN - 1);

    pub fn cmsg_space(fds: usize) -> usize {
        CMSG_HDR + ((fds * 4 + CMSG_ALIGN - 1) & !(CMSG_ALIGN - 1))
    }

    extern "C" {
        pub fn sendmsg(fd: i32, msg: *const MsgHdr, flags: i32) -> isize;
        pub fn recvmsg(fd: i32, msg: *mut MsgHdr, flags: i32) -> isize;
        pub fn poll(fds: *mut PollFd, n: NFds, timeout: i32) -> i32;
        pub fn close(fd: i32) -> i32;
        pub fn fcntl(fd: i32, cmd: i32, ...) -> i32;
    }

    #[cfg(target_os = "linux")]
    pub type NFds = std::ffi::c_ulong;
    #[cfg(not(target_os = "linux"))]
    pub type NFds = u32;
    #[repr(C)]
    pub struct PollFd { pub fd: i32, pub events: i16, pub revents: i16 }
    pub const POLLIN: i16 = 1;
    pub const POLLHUP: i16 = 0x10;
    pub const F_SETFD: i32 = 2;
    pub const FD_CLOEXEC: i32 = 1;

    /// A descriptor as i32, or the error naming `call`.
    pub fn raw(fd: i64, call: &str) -> Result<i32, String> {
        i32::try_from(fd).ok().filter(|f| *f >= 0)
            .ok_or_else(|| format!("{call}: not a file descriptor: {fd}"))
    }

    /// Run `f` on the file behind `fd` without taking ownership of it.
    pub fn with_file<R>(fd: i32, f: impl FnOnce(&std::fs::File) -> R) -> R {
        let file = std::mem::ManuallyDrop::new(unsafe { std::fs::File::from_raw_fd(fd) });
        f(&file)
    }

    pub fn into_fd(file: std::fs::File) -> i64 {
        i64::from(file.into_raw_fd())
    }

    pub fn stream_fd(s: std::os::unix::net::UnixStream) -> i64 {
        i64::from(s.into_raw_fd())
    }

    pub fn listener_fd(l: std::os::unix::net::UnixListener) -> i64 {
        i64::from(l.into_raw_fd())
    }

    pub fn with_listener<R>(fd: i32, f: impl FnOnce(&std::os::unix::net::UnixListener) -> R) -> R {
        let l = std::mem::ManuallyDrop::new(unsafe { std::os::unix::net::UnixListener::from_raw_fd(fd) });
        f(&l)
    }
}

/// Connect a stream socket to the listener at `path`; its descriptor.
pub fn almide_rt_net_unix_connect(path: &str) -> Result<i64, String> {
    #[cfg(unix)]
    {
        std::os::unix::net::UnixStream::connect(path)
            .map(almide_net_unix::stream_fd)
            .map_err(|e| format!("unix_connect({path}): {e}"))
    }
    #[cfg(not(unix))]
    {
        let _ = (path,);
        Err("unix_connect: Unix-domain sockets and shared-memory files need a Unix platform".to_string())
    }
}

/// Listen for stream connections at `path` (which must not exist yet).
pub fn almide_rt_net_unix_listen(path: &str) -> Result<i64, String> {
    #[cfg(unix)]
    {
        std::os::unix::net::UnixListener::bind(path)
            .map(almide_net_unix::listener_fd)
            .map_err(|e| format!("unix_listen({path}): {e}"))
    }
    #[cfg(not(unix))]
    {
        let _ = (path,);
        Err("unix_listen: Unix-domain sockets and shared-memory files need a Unix platform".to_string())
    }
}

/// Wait for the next connection on `listener`; the connected descriptor.
pub fn almide_rt_net_unix_accept(listener: i64) -> Result<i64, String> {
    #[cfg(unix)]
    {
        let fd = almide_net_unix::raw(listener, "unix_accept")?;
        almide_net_unix::with_listener(fd, |l| l.accept())
            .map(|(s, _)| almide_net_unix::stream_fd(s))
            .map_err(|e| format!("unix_accept: {e}"))
    }
    #[cfg(not(unix))]
    {
        let _ = (listener,);
        Err("unix_accept: Unix-domain sockets and shared-memory files need a Unix platform".to_string())
    }
}

/// Send all of `data`, the descriptors `fds` riding with its first byte.
pub fn almide_rt_net_unix_send(sock: i64, data: &[u8], fds: &[i64]) -> Result<(), String> {
    #[cfg(unix)]
    {
        use almide_net_unix as u;
        let fd = u::raw(sock, "unix_send")?;
        if fds.len() > u::MAX_FDS {
            return Err(format!("unix_send: {} descriptors in one message (at most {})", fds.len(), u::MAX_FDS));
        }
        if fds.is_empty() && data.is_empty() {
            return Ok(());
        }
        let mut raw_fds = Vec::with_capacity(fds.len());
        for &f in fds {
            raw_fds.push(u::raw(f, "unix_send")?);
        }
        let mut sent = 0usize;
        let mut first = true;
        while sent < data.len() || first {
            let rest = &data[sent..];
            let mut iov = u::IoVec { base: rest.as_ptr() as *mut u8, len: rest.len() };
            let mut control = vec![0u8; if first && !raw_fds.is_empty() { u::cmsg_space(raw_fds.len()) } else { 0 }];
            if !control.is_empty() {
                let len = (u::CMSG_HDR + raw_fds.len() * 4) as u::CmsgLen;
                let w = std::mem::size_of::<u::CmsgLen>();
                control[..w].copy_from_slice(&len.to_ne_bytes());
                control[w..w + 4].copy_from_slice(&u::SOL_SOCKET.to_ne_bytes());
                control[w + 4..w + 8].copy_from_slice(&u::SCM_RIGHTS.to_ne_bytes());
                for (i, f) in raw_fds.iter().enumerate() {
                    let at = u::CMSG_HDR + i * 4;
                    control[at..at + 4].copy_from_slice(&f.to_ne_bytes());
                }
            }
            let msg = u::MsgHdr {
                name: std::ptr::null_mut(), namelen: 0,
                iov: &mut iov, iovlen: 1,
                control: if control.is_empty() { std::ptr::null_mut() } else { control.as_mut_ptr() },
                controllen: control.len() as _,
                flags: 0,
            };
            let n = unsafe { u::sendmsg(fd, &msg, u::SEND_FLAGS) };
            if n < 0 {
                let e = std::io::Error::last_os_error();
                if e.kind() == std::io::ErrorKind::Interrupted { continue; }
                return Err(format!("unix_send: {e}"));
            }
            sent += n as usize;
            first = false;
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = (sock, data, fds);
        Err("unix_send: Unix-domain sockets and shared-memory files need a Unix platform".to_string())
    }
}

/// Up to `max` bytes (blocking until some arrive); empty once the peer
/// closed. Descriptors that came with them wait for `unix_take_fds`.
pub fn almide_rt_net_unix_recv(sock: i64, max: i64) -> Result<Vec<u8>, String> {
    #[cfg(unix)]
    {
        use almide_net_unix as u;
        let fd = u::raw(sock, "unix_recv")?;
        let mut buf = vec![0u8; max.max(0) as usize];
        let mut control = vec![0u8; u::cmsg_space(u::MAX_FDS)];
        loop {
            let mut iov = u::IoVec { base: buf.as_mut_ptr(), len: buf.len() };
            let mut msg = u::MsgHdr {
                name: std::ptr::null_mut(), namelen: 0,
                iov: &mut iov, iovlen: 1,
                control: control.as_mut_ptr(), controllen: control.len() as _,
                flags: 0,
            };
            let n = unsafe { u::recvmsg(fd, &mut msg, u::RECV_FLAGS) };
            if n < 0 {
                let e = std::io::Error::last_os_error();
                if e.kind() == std::io::ErrorKind::Interrupted { continue; }
                return Err(format!("unix_recv: {e}"));
            }
            // Walk the control messages for SCM_RIGHTS.
            let total = msg.controllen as usize;
            let w = std::mem::size_of::<u::CmsgLen>();
            let mut at = 0usize;
            let mut got = Vec::new();
            while at + u::CMSG_HDR <= total {
                let mut lb = [0u8; 8];
                lb[..w].copy_from_slice(&control[at..at + w]);
                let len = if w == 8 { u64::from_ne_bytes(lb) as usize } else { u32::from_ne_bytes([lb[0], lb[1], lb[2], lb[3]]) as usize };
                if len < u::CMSG_HDR || at + len > total { break; }
                let level = i32::from_ne_bytes(control[at + w..at + w + 4].try_into().unwrap());
                let kind = i32::from_ne_bytes(control[at + w + 4..at + w + 8].try_into().unwrap());
                if level == u::SOL_SOCKET && kind == u::SCM_RIGHTS {
                    let count = (len - u::CMSG_HDR) / 4;
                    for i in 0..count {
                        let p = at + u::CMSG_HDR + i * 4;
                        let f = i32::from_ne_bytes(control[p..p + 4].try_into().unwrap());
                        // Where MSG_CMSG_CLOEXEC does not exist, set it here.
                        unsafe { u::fcntl(f, u::F_SETFD, u::FD_CLOEXEC) };
                        got.push(i64::from(f));
                    }
                }
                at += (len + u::CMSG_ALIGN - 1) & !(u::CMSG_ALIGN - 1);
            }
            if !got.is_empty() {
                u::RECEIVED.with(|r| r.borrow_mut().entry(sock).or_default().extend(got));
            }
            buf.truncate(n as usize);
            return Ok(buf);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (sock, max);
        Err("unix_recv: Unix-domain sockets and shared-memory files need a Unix platform".to_string())
    }
}

/// The descriptors received on `sock` so far and not yet taken, oldest first.
pub fn almide_rt_net_unix_take_fds(sock: i64) -> Vec<i64> {
    #[cfg(unix)]
    {
        almide_net_unix::RECEIVED.with(|r| {
            r.borrow_mut().remove(&sock).map(Vec::from).unwrap_or_default()
        })
    }
    #[cfg(not(unix))]
    {
        let _ = (sock,);
        Vec::new()
    }
}

/// Whether `fd` has something to read (data, a connection, or the peer's
/// close) within `timeout_ms` milliseconds; 0 checks without waiting, a
/// negative timeout waits as long as it takes.
pub fn almide_rt_net_unix_poll(fd: i64, timeout_ms: i64) -> Result<bool, String> {
    #[cfg(unix)]
    {
        use almide_net_unix as u;
        let raw = u::raw(fd, "unix_poll")?;
        let timeout = if timeout_ms < 0 { -1 } else { timeout_ms.min(i64::from(i32::MAX)) as i32 };
        loop {
            let mut p = u::PollFd { fd: raw, events: u::POLLIN, revents: 0 };
            let n = unsafe { u::poll(&mut p, 1, timeout) };
            if n < 0 {
                let e = std::io::Error::last_os_error();
                if e.kind() == std::io::ErrorKind::Interrupted { continue; }
                return Err(format!("unix_poll: {e}"));
            }
            return Ok(n > 0 && (p.revents & (u::POLLIN | u::POLLHUP)) != 0);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (fd, timeout_ms);
        Err("unix_poll: Unix-domain sockets and shared-memory files need a Unix platform".to_string())
    }
}

/// The descriptors among `fds` that have something to read (data, a
/// connection, or the peer's close) within `timeout_ms` milliseconds, in the
/// order given; empty when none did. 0 checks without waiting, a negative
/// timeout waits for the first. One wait for many sources: a Wayland socket
/// and a compositor's event socket together, with no busy loop.
pub fn almide_rt_net_unix_wait(fds: &[i64], timeout_ms: i64) -> Result<Vec<i64>, String> {
    #[cfg(unix)]
    {
        use almide_net_unix as u;
        let mut polls = Vec::with_capacity(fds.len());
        for &f in fds {
            polls.push(u::PollFd { fd: u::raw(f, "unix_wait")?, events: u::POLLIN, revents: 0 });
        }
        let timeout = if timeout_ms < 0 { -1 } else { timeout_ms.min(i64::from(i32::MAX)) as i32 };
        loop {
            for p in polls.iter_mut() { p.revents = 0; }
            let n = unsafe { u::poll(polls.as_mut_ptr(), polls.len() as u::NFds, timeout) };
            if n < 0 {
                let e = std::io::Error::last_os_error();
                if e.kind() == std::io::ErrorKind::Interrupted { continue; }
                return Err(format!("unix_wait: {e}"));
            }
            return Ok(polls.iter().zip(fds)
                .filter(|(p, _)| (p.revents & (u::POLLIN | u::POLLHUP)) != 0)
                .map(|(_, &f)| f)
                .collect());
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (fds, timeout_ms);
        Err("unix_wait: Unix-domain sockets and shared-memory files need a Unix platform".to_string())
    }
}

/// Close a descriptor this module opened or received.
pub fn almide_rt_net_unix_close(fd: i64) -> Result<(), String> {
    #[cfg(unix)]
    {
        use almide_net_unix as u;
        let raw = u::raw(fd, "unix_close")?;
        u::RECEIVED.with(|r| r.borrow_mut().remove(&fd));
        if unsafe { u::close(raw) } != 0 {
            return Err(format!("unix_close: {}", std::io::Error::last_os_error()));
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = (fd,);
        Err("unix_close: Unix-domain sockets and shared-memory files need a Unix platform".to_string())
    }
}

/// A new shared-memory file of `size` bytes, zero-filled, with no name left
/// in any directory: its descriptor, to write into and pass to a peer.
pub fn almide_rt_net_shm_create(size: i64) -> Result<i64, String> {
    #[cfg(unix)]
    {
        if size < 0 {
            return Err(format!("shm_create: negative size {size}"));
        }
        // Where the session keeps its runtime files (a tmpfs on Linux), else the
        // system temp dir. The name exists only between create and unlink.
        let dir = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from)
            .filter(|d| d.is_dir())
            .unwrap_or_else(std::env::temp_dir);
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        for attempt in 0..16u32 {
            let path = dir.join(format!("almide-shm-{}-{}-{}", std::process::id(), stamp, attempt));
            match std::fs::OpenOptions::new().read(true).write(true).create_new(true).open(&path) {
                Ok(file) => {
                    let _ = std::fs::remove_file(&path);
                    file.set_len(size as u64).map_err(|e| format!("shm_create({size}): {e}"))?;
                    return Ok(almide_net_unix::into_fd(file));
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(format!("shm_create({size}): {e}")),
            }
        }
        Err(format!("shm_create({size}): no free name"))
    }
    #[cfg(not(unix))]
    {
        let _ = (size,);
        Err("shm_create: Unix-domain sockets and shared-memory files need a Unix platform".to_string())
    }
}

/// Grow or shrink the shared-memory file `fd` to `size` bytes.
pub fn almide_rt_net_shm_resize(fd: i64, size: i64) -> Result<(), String> {
    #[cfg(unix)]
    {
        let raw = almide_net_unix::raw(fd, "shm_resize")?;
        if size < 0 {
            return Err(format!("shm_resize: negative size {size}"));
        }
        almide_net_unix::with_file(raw, |f| f.set_len(size as u64)).map_err(|e| format!("shm_resize: {e}"))
    }
    #[cfg(not(unix))]
    {
        let _ = (fd, size);
        Err("shm_resize: Unix-domain sockets and shared-memory files need a Unix platform".to_string())
    }
}

/// Write `data` into the shared-memory file `fd` at byte `offset`.
pub fn almide_rt_net_shm_write(fd: i64, offset: i64, data: &[u8]) -> Result<(), String> {
    #[cfg(unix)]
    {
        let raw = almide_net_unix::raw(fd, "shm_write")?;
        if offset < 0 {
            return Err(format!("shm_write: negative offset {offset}"));
        }
        almide_net_unix::with_file(raw, |f| std::os::unix::fs::FileExt::write_all_at(f, data, offset as u64)).map_err(|e| format!("shm_write: {e}"))
    }
    #[cfg(not(unix))]
    {
        let _ = (fd, offset, data);
        Err("shm_write: Unix-domain sockets and shared-memory files need a Unix platform".to_string())
    }
}

/// Up to `len` bytes of the file `fd` from byte `offset` (fewer at its end).
pub fn almide_rt_net_shm_read(fd: i64, offset: i64, len: i64) -> Result<Vec<u8>, String> {
    #[cfg(unix)]
    {
        let raw = almide_net_unix::raw(fd, "shm_read")?;
        if offset < 0 || len < 0 {
            return Err(format!("shm_read: negative offset or length ({offset}, {len})"));
        }
        let mut buf = vec![0u8; len as usize];
        let mut got = 0usize;
        almide_net_unix::with_file(raw, |f| -> Result<(), String> {
            while got < buf.len() {
                match std::os::unix::fs::FileExt::read_at(f, &mut buf[got..], offset as u64 + got as u64) {
                    Ok(0) => break,
                    Ok(n) => got += n,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(e) => return Err(format!("shm_read: {e}")),
                }
            }
            Ok(())
        })?;
        buf.truncate(got);
        Ok(buf)
    }
    #[cfg(not(unix))]
    {
        let _ = (fd, offset, len);
        Err("shm_read: Unix-domain sockets and shared-memory files need a Unix platform".to_string())
    }
}
