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
/// `memory` is the linear memory at its current size. A pointer outside
/// memory, or a misaligned pointer to a 4-byte value, traps — the stock
/// runtime traps on both too, so neither can let a run go on differently.
pub fn call(host: HostFn, args: &[u64], memory: &mut [u8], io: &mut Io) -> Result<Option<u64>, Stop> {
    match host {
        HostFn::FdWrite => fd_write(args, memory, io).map(Some).map_err(Stop::Trap),
        HostFn::FdRead => fd_read(args, memory, io).map(Some).map_err(Stop::Trap),
        HostFn::ProcExit => Err(Stop::Exit(args[0] as u32 as i32)),
        HostFn::RandomGet | HostFn::ClockTimeGet => Err(Stop::Trap(Trap::HostCallNotServed(host.name()))),
    }
}

/// The range `[at, at + len)` of memory, when it lies inside it.
fn range(memory: &[u8], at: u64, len: u64) -> Result<(usize, usize), Trap> {
    let end = at.checked_add(len).filter(|&e| e <= memory.len() as u64).ok_or(Trap::HostPointerOutOfBounds)?;
    Ok((at as usize, end as usize))
}

/// A 4-byte-aligned pointer to a u32 inside memory.
fn word(memory: &[u8], at: u64) -> Result<usize, Trap> {
    if !at.is_multiple_of(4) {
        return Err(Trap::HostPointerMisaligned);
    }
    range(memory, at, 4).map(|(start, _)| start)
}

fn read_u32(memory: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([memory[at], memory[at + 1], memory[at + 2], memory[at + 3]])
}

/// The buffer the `k`th iovec of the array at `iovs` names.
fn iovec(memory: &[u8], iovs: u32, k: u32) -> Result<(usize, usize), Trap> {
    let at = u64::from(iovs) + 8 * u64::from(k);
    let ptr = read_u32(memory, word(memory, at)?);
    let len = read_u32(memory, word(memory, at + 4)?);
    range(memory, u64::from(ptr), u64::from(len))
}

/// `fd_write(fd, iovs, iovs_len, nwritten) -> errno`. Every iovec and the
/// `nwritten` pointer are checked before any byte is written, so a bad one
/// writes nothing.
fn fd_write(args: &[u64], memory: &mut [u8], io: &mut Io) -> Result<u64, Trap> {
    let (fd, iovs, count, nwritten) = (args[0] as u32, args[1] as u32, args[2] as u32, args[3] as u32);
    if fd != 1 && fd != 2 {
        return Ok(EBADF);
    }
    let mut total: u64 = 0;
    for k in 0..count {
        let (ptr, end) = iovec(memory, iovs, k)?;
        total += (end - ptr) as u64;
    }
    let at = word(memory, u64::from(nwritten))?;
    let Ok(total) = u32::try_from(total) else { return Ok(EINVAL) };
    for k in 0..count {
        // writing to a stream does not touch guest memory, so every iovec
        // still reads as it did in the check above
        let (ptr, end) = iovec(memory, iovs, k)?;
        let bytes = &memory[ptr..end];
        let sink: &mut dyn Write = if fd == 1 { io.out } else { io.err };
        if sink.write_all(bytes).is_err() {
            return Ok(EIO);
        }
        if fd == 2 {
            io.err_tail.feed(bytes);
        }
    }
    memory[at..at + 4].copy_from_slice(&total.to_le_bytes());
    Ok(ESUCCESS)
}

/// `fd_read(fd, iovs, iovs_len, nread) -> errno` on stdin. As the stock
/// runtime does, one read fills the FIRST non-empty buffer only; a caller
/// that wants more calls again, as the read-to-end shim does. The iovec array
/// is read once, before the read, so a buffer that overlaps it cannot change
/// which buffer is filled.
fn fd_read(args: &[u64], memory: &mut [u8], io: &mut Io) -> Result<u64, Trap> {
    let (fd, iovs, count, nread) = (args[0] as u32, args[1] as u32, args[2] as u32, args[3] as u32);
    if fd != 0 {
        return Ok(EBADF);
    }
    let mut target = None;
    for k in 0..count {
        let (ptr, end) = iovec(memory, iovs, k)?;
        if target.is_none() && end > ptr {
            target = Some((ptr, end));
        }
    }
    let at = word(memory, u64::from(nread))?;
    let n = match target {
        None => 0,
        Some((ptr, end)) => loop {
            match io.input.read(&mut memory[ptr..end]) {
                Ok(n) => break n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => return Ok(EIO),
            }
        },
    };
    memory[at..at + 4].copy_from_slice(&(n as u32).to_le_bytes());
    Ok(ESUCCESS)
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

    fn with_io<T>(input: &[u8], f: impl FnOnce(&mut Io) -> T) -> (T, Vec<u8>) {
        let (mut input, mut out, mut err) = (input, Vec::new(), Vec::new());
        let t = f(&mut Io::new(&mut input, &mut out, &mut err));
        (t, out)
    }

    #[test]
    fn fd_write_writes_and_reports_the_count() {
        let mut m = mem_with_iovec(32, 5);
        let (r, out) = with_io(b"", |io| fd_write(&[1, 0, 1, 16], &mut m, io));
        assert_eq!(r, Ok(ESUCCESS));
        assert_eq!(out, b"hello");
        assert_eq!(&m[16..20], &5u32.to_le_bytes());
    }

    #[test]
    fn fd_write_refuses_a_bad_fd_and_traps_on_a_bad_pointer_without_writing() {
        let mut m = mem_with_iovec(62, 5);
        let (r, out) = with_io(b"", |io| {
            [
                fd_write(&[3, 0, 1, 16], &mut m, io),
                fd_write(&[1, 0, 1, 16], &mut m, io),
                fd_write(&[1, 60, 1, 16], &mut m, io),
                fd_write(&[1, 2, 1, 16], &mut m, io),
            ]
        });
        assert_eq!(r[0], Ok(EBADF));
        assert_eq!(r[1], Err(Trap::HostPointerOutOfBounds), "the buffer runs past the end");
        assert_eq!(r[2], Err(Trap::HostPointerOutOfBounds), "the iovec itself is out of bounds");
        assert_eq!(r[3], Err(Trap::HostPointerMisaligned));
        assert!(out.is_empty());
        let mut m = mem_with_iovec(32, 5);
        let (r, out) = with_io(b"", |io| fd_write(&[1, 0, 1, 18], &mut m, io));
        assert_eq!(r, Err(Trap::HostPointerMisaligned), "a misaligned nwritten");
        assert!(out.is_empty(), "nothing is written before the check fails");
    }

    #[test]
    fn fd_read_fills_the_first_non_empty_buffer() {
        let mut m = mem_with_iovec(40, 0);
        m[8..12].copy_from_slice(&50u32.to_le_bytes());
        m[12..16].copy_from_slice(&4u32.to_le_bytes());
        let (r, _) = with_io(b"abcdef", |io| fd_read(&[0, 0, 2, 20], &mut m, io));
        assert_eq!(r, Ok(ESUCCESS));
        assert_eq!(&m[50..54], b"abcd", "the empty first iovec is skipped");
        assert_eq!(&m[20..24], &4u32.to_le_bytes());
        let (r, _) = with_io(b"", |io| fd_read(&[0, 0, 2, 20], &mut m, io));
        assert_eq!((r, &m[20..24]), (Ok(ESUCCESS), &0u32.to_le_bytes()[..]), "end of input reads 0");
        let (r, _) = with_io(b"", |io| fd_read(&[1, 0, 1, 20], &mut m, io));
        assert_eq!(r, Ok(EBADF));
    }

    #[test]
    fn fd_read_into_its_own_iovec_array_cannot_redirect_the_read() {
        // iov[0] covers the iovec array itself; the read rewrites iov[1]
        // with a pointer far outside memory — which must not matter
        let mut m = vec![0u8; 256];
        m[0..8].copy_from_slice(&[0, 0, 0, 0, 16, 0, 0, 0]);
        m[8..16].copy_from_slice(&[100, 0, 0, 0, 4, 0, 0, 0]);
        let input = [0u8, 0, 0, 0, 16, 0, 0, 0, 0xff, 0xff, 0xff, 0xff, 4, 0, 0, 0];
        let (r, _) = with_io(&input, |io| fd_read(&[0, 0, 2, 200], &mut m, io));
        assert_eq!(r, Ok(ESUCCESS));
        assert_eq!(&m[200..204], &16u32.to_le_bytes());
    }
}
