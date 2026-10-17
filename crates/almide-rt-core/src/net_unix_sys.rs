// net_unix_sys (#3620): the ONE sanctioned `unsafe` site of almide-rt-core —
// the four descriptor syscalls std has no safe form of on stable Rust:
// sendmsg / recvmsg with SCM_RIGHTS control messages, poll, and fcntl's
// close-on-exec. Everything else about Unix sockets and shared-memory files
// (the table of held descriptors, accept, connect, the file reads and writes,
// close) is safe std code in net_unix_core.rs, which `include!`s this file.
//
// Why here and not in each consumer: the embedded wasm host (almide-wasm-run)
// takes the workspace's `forbid(unsafe_code)` and cannot carry a copy, and two
// copies would be two behaviours. So this crate denies `unsafe` (its own
// lints, Cargo.toml) instead of forbidding it, and this one module is allowed
// it; tests/net_unix_core.rs fails if `unsafe` appears in any other file or a
// block here lacks its SAFETY line.
//
// Every raw descriptor passed in is borrowed from an `OwnedFd` the caller's
// table holds for the whole call, so no call here can reach a descriptor the
// program was not given. The layouts are the platform C structs, as
// runtime/rs/src/net.rs declared them before this move: Linux (glibc and
// musl alike on 64-bit: size_t lengths) and the BSDs / macOS.
//
// Splice discipline (lib.rs): no `crate::` paths, no `use` lines; the one
// inline `mod` carries the lint allowance.

#[cfg(unix)]
#[allow(unsafe_code)]
mod almide_unix_sys {
    /// The most descriptors one message carries or takes: Wayland's limit,
    /// and far above what any one request needs.
    pub const MAX_FDS: usize = 28;

    #[repr(C)]
    struct IoVec {
        base: *mut u8,
        len: usize,
    }

    #[cfg(target_os = "linux")]
    #[repr(C)]
    struct MsgHdr {
        name: *mut u8,
        namelen: u32,
        iov: *mut IoVec,
        iovlen: usize,
        control: *mut u8,
        controllen: usize,
        flags: i32,
    }
    #[cfg(not(target_os = "linux"))]
    #[repr(C)]
    struct MsgHdr {
        name: *mut u8,
        namelen: u32,
        iov: *mut IoVec,
        iovlen: i32,
        control: *mut u8,
        controllen: u32,
        flags: i32,
    }
    #[cfg(target_os = "linux")]
    type CmsgLen = usize;
    #[cfg(not(target_os = "linux"))]
    type CmsgLen = u32;
    #[cfg(target_os = "linux")]
    const SOL_SOCKET: i32 = 1;
    #[cfg(not(target_os = "linux"))]
    const SOL_SOCKET: i32 = 0xffff;
    const SCM_RIGHTS: i32 = 1;
    /// MSG_NOSIGNAL on send (a closed peer is an error, not SIGPIPE) and
    /// MSG_CMSG_CLOEXEC on receive, where the platform has them.
    #[cfg(target_os = "linux")]
    const SEND_FLAGS: i32 = 0x4000;
    #[cfg(not(target_os = "linux"))]
    const SEND_FLAGS: i32 = 0;
    #[cfg(target_os = "linux")]
    const RECV_FLAGS: i32 = 0x4000_0000;
    #[cfg(not(target_os = "linux"))]
    const RECV_FLAGS: i32 = 0;
    /// Control-message alignment: size_t on Linux, 4 bytes on the BSDs.
    const CMSG_ALIGN: usize = std::mem::size_of::<CmsgLen>();
    const CMSG_HDR: usize = (std::mem::size_of::<CmsgLen>() + 8 + CMSG_ALIGN - 1) & !(CMSG_ALIGN - 1);

    #[cfg(target_os = "linux")]
    type NFds = std::ffi::c_ulong;
    #[cfg(not(target_os = "linux"))]
    type NFds = u32;
    #[repr(C)]
    struct PollFd {
        fd: i32,
        events: i16,
        revents: i16,
    }
    const POLLIN: i16 = 1;
    const POLLHUP: i16 = 0x10;
    const F_SETFD: i32 = 2;
    const FD_CLOEXEC: i32 = 1;

    // SAFETY: the platform C library's own declarations of these four
    // functions, with the argument types of the structs above.
    unsafe extern "C" {
        fn sendmsg(fd: i32, msg: *const MsgHdr, flags: i32) -> isize;
        fn recvmsg(fd: i32, msg: *mut MsgHdr, flags: i32) -> isize;
        fn poll(fds: *mut PollFd, n: NFds, timeout: i32) -> i32;
        fn fcntl(fd: i32, cmd: i32, ...) -> i32;
    }

    fn cmsg_space(fds: usize) -> usize {
        CMSG_HDR + ((fds * 4 + CMSG_ALIGN - 1) & !(CMSG_ALIGN - 1))
    }

    /// One SCM_RIGHTS control message carrying `fds`.
    fn rights_message(fds: &[i32]) -> Vec<u8> {
        let mut control = vec![0u8; cmsg_space(fds.len())];
        let len = (CMSG_HDR + fds.len() * 4) as CmsgLen;
        let w = std::mem::size_of::<CmsgLen>();
        control[..w].copy_from_slice(&len.to_ne_bytes());
        control[w..w + 4].copy_from_slice(&SOL_SOCKET.to_ne_bytes());
        control[w + 4..w + 8].copy_from_slice(&SCM_RIGHTS.to_ne_bytes());
        for (i, f) in fds.iter().enumerate() {
            let at = CMSG_HDR + i * 4;
            control[at..at + 4].copy_from_slice(&f.to_ne_bytes());
        }
        control
    }

    fn i32_at(b: &[u8], at: usize) -> i32 {
        i32::from_ne_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
    }

    fn cmsg_len_at(b: &[u8], at: usize) -> usize {
        let w = std::mem::size_of::<CmsgLen>();
        let mut lb = [0u8; 8];
        lb[..w].copy_from_slice(&b[at..at + w]);
        if w == 8 { u64::from_ne_bytes(lb) as usize } else { u32::from_ne_bytes([lb[0], lb[1], lb[2], lb[3]]) as usize }
    }

    /// The descriptors of every SCM_RIGHTS message in the first `total`
    /// bytes of `control`.
    fn rights_in(control: &[u8], total: usize) -> Vec<i32> {
        let w = std::mem::size_of::<CmsgLen>();
        let mut at = 0usize;
        let mut got = Vec::new();
        while at + CMSG_HDR <= total {
            let len = cmsg_len_at(control, at);
            if len < CMSG_HDR || at + len > total {
                break;
            }
            if i32_at(control, at + w) == SOL_SOCKET && i32_at(control, at + w + 4) == SCM_RIGHTS {
                got.extend((0..(len - CMSG_HDR) / 4).map(|i| i32_at(control, at + CMSG_HDR + i * 4)));
            }
            at += (len + CMSG_ALIGN - 1) & !(CMSG_ALIGN - 1);
        }
        got
    }

    /// One `sendmsg` of `data` on `sock`, with `fds` as SCM_RIGHTS when
    /// non-empty; the byte count sent.
    pub fn send(sock: &std::os::fd::BorrowedFd<'_>, data: &[u8], fds: &[std::os::fd::BorrowedFd<'_>]) -> std::io::Result<usize> {
        let raw: Vec<i32> = fds.iter().map(std::os::fd::AsRawFd::as_raw_fd).collect();
        let mut control = if raw.is_empty() { Vec::new() } else { rights_message(&raw) };
        let mut iov = IoVec { base: data.as_ptr() as *mut u8, len: data.len() };
        let msg = MsgHdr {
            name: std::ptr::null_mut(),
            namelen: 0,
            iov: &mut iov,
            iovlen: 1,
            control: if control.is_empty() { std::ptr::null_mut() } else { control.as_mut_ptr() },
            controllen: control.len() as _,
            flags: 0,
        };
        // SAFETY: `msg` points at one iovec over `data` (read only by the
        // kernel) and at `control`, both alive for the call; `sock` and every
        // descriptor in the control message are borrowed from held OwnedFds.
        let n = unsafe { sendmsg(std::os::fd::AsRawFd::as_raw_fd(sock), &msg, SEND_FLAGS) };
        if n < 0 { Err(std::io::Error::last_os_error()) } else { Ok(n as usize) }
    }

    /// One `recvmsg` into `buf` on `sock`: the byte count, and the
    /// descriptors that arrived with it, now owned (close-on-exec).
    pub fn recv(sock: &std::os::fd::BorrowedFd<'_>, buf: &mut [u8]) -> std::io::Result<(usize, Vec<std::os::fd::OwnedFd>)> {
        let mut control = vec![0u8; cmsg_space(MAX_FDS)];
        let mut iov = IoVec { base: buf.as_mut_ptr(), len: buf.len() };
        let mut msg = MsgHdr {
            name: std::ptr::null_mut(),
            namelen: 0,
            iov: &mut iov,
            iovlen: 1,
            control: control.as_mut_ptr(),
            controllen: control.len() as _,
            flags: 0,
        };
        // SAFETY: `msg` points at one iovec over `buf` and at `control`, both
        // writable and alive for the call; the kernel writes at most their
        // lengths. `sock` is borrowed from a held OwnedFd.
        let n = unsafe { recvmsg(std::os::fd::AsRawFd::as_raw_fd(sock), &mut msg, RECV_FLAGS) };
        if n < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let fds = rights_in(&control, msg.controllen as usize)
            .into_iter()
            .map(|f| {
                // SAFETY: the kernel installed `f` in this process for this
                // message and nothing else refers to it: owning it here is
                // what closes it later. F_SETFD only sets a flag on it.
                unsafe {
                    fcntl(f, F_SETFD, FD_CLOEXEC);
                    <std::os::fd::OwnedFd as std::os::fd::FromRawFd>::from_raw_fd(f)
                }
            })
            .collect();
        Ok((n as usize, fds))
    }

    /// `poll` for readability (data, a connection, or the peer's close) on
    /// each of `fds`, `timeout` ms (negative: no limit); whether each is ready.
    pub fn readable(fds: &[std::os::fd::BorrowedFd<'_>], timeout: i32) -> std::io::Result<Vec<bool>> {
        let mut polls: Vec<PollFd> = fds
            .iter()
            .map(|f| PollFd { fd: std::os::fd::AsRawFd::as_raw_fd(f), events: POLLIN, revents: 0 })
            .collect();
        // SAFETY: `polls` is a live, writable array of `polls.len()` pollfd
        // structs; every descriptor in it is borrowed from a held OwnedFd.
        let n = unsafe { poll(polls.as_mut_ptr(), polls.len() as NFds, timeout) };
        if n < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(polls.iter().map(|p| n > 0 && (p.revents & (POLLIN | POLLHUP)) != 0).collect())
    }
}
