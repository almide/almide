//! The host side: the WASI preview-1 calls a shipped artifact makes
//! (REQ-VM-4). `fd_write` to stdout or stderr, `fd_read` from stdin and
//! `proc_exit` are served. `random_get` and `clock_time_get` exist only
//! because every artifact declares them: calling one traps naming it, and a
//! Critical-profile program, which is granted no Rand or Time capability,
//! never does.

use std::io::{Read, Write};

use crate::error::Trap;
use crate::module::HostFn;

/// WASI errno values the served calls return.
pub const ESUCCESS: u64 = 0;
pub const EBADF: u64 = 8;
pub const EFAULT: u64 = 21;
pub const EINVAL: u64 = 28;
pub const EIO: u64 = 29;

/// Why a run stopped before `_start` returned.
#[derive(Debug, PartialEq, Eq)]
pub enum Stop {
    Trap(Trap),
    Exit(i32),
}

impl From<Trap> for Stop {
    fn from(t: Trap) -> Self {
        Stop::Trap(t)
    }
}

/// The three standard streams, and what the runner needs to know about
/// stderr's last line.
pub struct Io<'a> {
    pub input: &'a mut dyn Read,
    pub out: &'a mut dyn Write,
    pub err: &'a mut dyn Write,
    pub err_tail: LineTail,
}

impl<'a> Io<'a> {
    pub fn new(input: &'a mut dyn Read, out: &'a mut dyn Write, err: &'a mut dyn Write) -> Self {
        Io { input, out, err, err_tail: LineTail::default() }
    }
}

/// Serve one host call. `args` are the call's operands in parameter order;
/// `memory` is the linear memory at its current size.
pub fn call(host: HostFn, args: &[u64], memory: &mut [u8], io: &mut Io) -> Result<Option<u64>, Stop> {
    match host {
        HostFn::FdWrite => Ok(Some(fd_write(args, memory, io))),
        HostFn::FdRead => Ok(Some(fd_read(args, memory, io))),
        HostFn::ProcExit => Err(Stop::Exit(args[0] as u32 as i32)),
        HostFn::RandomGet | HostFn::ClockTimeGet => Err(Stop::Trap(Trap::HostCallNotServed(host.name()))),
    }
}

fn read_u32(memory: &[u8], at: u64) -> Option<u32> {
    let at = usize::try_from(at).ok()?;
    let b = memory.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// The byte range an iovec names, when it lies inside memory.
fn iovec(memory: &[u8], iovs: u32, k: u32) -> Option<(usize, usize)> {
    let at = u64::from(iovs) + 8 * u64::from(k);
    let ptr = read_u32(memory, at)? as usize;
    let len = read_u32(memory, at + 4)? as usize;
    let end = ptr.checked_add(len)?;
    (end <= memory.len()).then_some((ptr, end))
}

/// `fd_write(fd, iovs, iovs_len, nwritten) -> errno`. Every iovec is checked
/// before any byte is written, so an out-of-bounds one writes nothing.
fn fd_write(args: &[u64], memory: &mut [u8], io: &mut Io) -> u64 {
    let (fd, iovs, count, nwritten) = (args[0] as u32, args[1] as u32, args[2] as u32, args[3] as u32);
    if fd != 1 && fd != 2 {
        return EBADF;
    }
    let mut total: u64 = 0;
    for k in 0..count {
        match iovec(memory, iovs, k) {
            Some((ptr, end)) => total += (end - ptr) as u64,
            None => return EFAULT,
        }
    }
    let Ok(total) = u32::try_from(total) else { return EINVAL };
    let at = nwritten as usize;
    if memory.get(at..at + 4).is_none() {
        return EFAULT;
    }
    for k in 0..count {
        let (ptr, end) = iovec(memory, iovs, k).expect("checked above");
        let bytes = &memory[ptr..end];
        let sink: &mut dyn Write = if fd == 1 { io.out } else { io.err };
        if sink.write_all(bytes).is_err() {
            return EIO;
        }
        if fd == 2 {
            io.err_tail.feed(bytes);
        }
    }
    memory[at..at + 4].copy_from_slice(&total.to_le_bytes());
    ESUCCESS
}

/// `fd_read(fd, iovs, iovs_len, nread) -> errno` on stdin: one read per
/// iovec, in order, stopping at the first short one — a caller that wants
/// everything loops until a read returns 0, as the read-to-end shim does.
fn fd_read(args: &[u64], memory: &mut [u8], io: &mut Io) -> u64 {
    let (fd, iovs, count, nread) = (args[0] as u32, args[1] as u32, args[2] as u32, args[3] as u32);
    if fd != 0 {
        return EBADF;
    }
    for k in 0..count {
        if iovec(memory, iovs, k).is_none() {
            return EFAULT;
        }
    }
    let at = nread as usize;
    if memory.get(at..at + 4).is_none() {
        return EFAULT;
    }
    let mut total: u32 = 0;
    for k in 0..count {
        let (ptr, end) = iovec(memory, iovs, k).expect("checked above");
        let n = loop {
            match io.input.read(&mut memory[ptr..end]) {
                Ok(n) => break n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => return EIO,
            }
        };
        total = total.saturating_add(n as u32);
        if n < end - ptr {
            break;
        }
    }
    memory[at..at + 4].copy_from_slice(&total.to_le_bytes());
    ESUCCESS
}

/// Whether the last line written to stderr starts with `Error: ` — the die
/// convention: a defined guard prints its `Error: <msg>` line and then
/// executes `unreachable`, and that trap is already named, so the runner
/// adds no trap line after it. "Last line" follows `str::lines`: the partial
/// line when output does not end in a newline, else the last whole line.
#[derive(Default, Debug)]
pub struct LineTail {
    head: [u8; 7],
    len: usize,
    finished_is_error: bool,
}

impl LineTail {
    pub fn feed(&mut self, bytes: &[u8]) {
        for &b in bytes {
            if b == b'\n' {
                self.finished_is_error = self.head_is_error();
                self.len = 0;
            } else {
                if self.len < self.head.len() {
                    self.head[self.len] = b;
                }
                self.len += 1;
            }
        }
    }

    fn head_is_error(&self) -> bool {
        self.len >= self.head.len() && &self.head == b"Error: "
    }

    pub fn last_line_is_error(&self) -> bool {
        if self.len > 0 { self.head_is_error() } else { self.finished_is_error }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tail(s: &str) -> bool {
        let mut t = LineTail::default();
        t.feed(s.as_bytes());
        t.last_line_is_error()
    }

    #[test]
    fn the_last_line_is_judged_as_str_lines_would() {
        for s in ["", "x", "Error: a\n\n", "Error: a\nb", "Error:", "Error:\n", "a\nError: b"] {
            assert_eq!(tail(s), s.lines().last().is_some_and(|l| l.starts_with("Error: ")), "{s:?}");
        }
        assert!(tail("Error: a\n"));
        assert!(tail("x\nError: b"));
    }

    fn mem_with_iovec(ptr: u32, len: u32) -> Vec<u8> {
        let mut m = vec![0u8; 64];
        m[0..4].copy_from_slice(&ptr.to_le_bytes());
        m[4..8].copy_from_slice(&len.to_le_bytes());
        m[32..37].copy_from_slice(b"hello");
        m
    }

    #[test]
    fn fd_write_writes_and_reports_the_count() {
        let mut m = mem_with_iovec(32, 5);
        let (mut input, mut out, mut err) = (&b""[..], Vec::new(), Vec::new());
        let mut io = Io::new(&mut input, &mut out, &mut err);
        assert_eq!(fd_write(&[1, 0, 1, 16], &mut m, &mut io), ESUCCESS);
        assert_eq!(out, b"hello");
        assert_eq!(&m[16..20], &5u32.to_le_bytes());
    }

    #[test]
    fn fd_write_refuses_a_bad_fd_and_an_out_of_bounds_iovec_without_writing() {
        let mut m = mem_with_iovec(62, 5);
        let (mut input, mut out, mut err) = (&b""[..], Vec::new(), Vec::new());
        let mut io = Io::new(&mut input, &mut out, &mut err);
        assert_eq!(fd_write(&[3, 0, 1, 16], &mut m, &mut io), EBADF);
        assert_eq!(fd_write(&[1, 0, 1, 16], &mut m, &mut io), EFAULT);
        assert_eq!(fd_write(&[1, 60, 1, 16], &mut m, &mut io), EFAULT, "the iovec itself is out of bounds");
        assert!(out.is_empty());
    }

    #[test]
    fn fd_read_fills_iovecs_in_order_and_reports_the_count() {
        let mut m = mem_with_iovec(40, 3);
        m[8..12].copy_from_slice(&50u32.to_le_bytes());
        m[12..16].copy_from_slice(&4u32.to_le_bytes());
        let (mut input, mut out, mut err) = (&b"abcdef"[..], Vec::new(), Vec::new());
        let mut io = Io::new(&mut input, &mut out, &mut err);
        assert_eq!(fd_read(&[0, 0, 2, 20], &mut m, &mut io), ESUCCESS);
        assert_eq!(&m[40..43], b"abc");
        assert_eq!(&m[50..53], b"def", "the second iovec takes what remains");
        assert_eq!(&m[20..24], &6u32.to_le_bytes());
        assert_eq!(fd_read(&[0, 0, 1, 20], &mut m, &mut io), ESUCCESS);
        assert_eq!(&m[20..24], &0u32.to_le_bytes(), "end of input reads 0");
        assert_eq!(fd_read(&[1, 0, 1, 20], &mut m, &mut io), EBADF);
    }
}
