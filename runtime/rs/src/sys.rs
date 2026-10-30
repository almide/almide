// sys — the native runtime's foreign calls (#4007).
//
// The ONE file under runtime/rs/src where `unsafe` is allowed. Every C / OS
// function the runtime calls is declared here, behind a safe wrapper whose
// arguments make the call sound, so http.rs, list.rs, process.rs and
// random.rs hold no `unsafe` of their own. scripts/check-runtime-unsafe.sh
// counts this file's lines against its row in
// proofs/runtime-unsafe-allowlist.toml; a new foreign call is added HERE.
//
// None of these has a std form: std sets no signal disposition, sends no
// signal to a thread, has no `_exit` (std::process::exit runs atexit
// handlers, which a signal handler may not), no `waitpid(WUNTRACED)` (a
// stop is not an exit, and `Child::try_wait` reports exits only), and no
// per-thread CPU clock. The generated crate's dependencies (rustls and its
// tree) offer none of them either.
//
// The module is pulled into a program by name: a runtime module that calls
// `almide_rt_sys_ffi::…` depends on this one (RUNTIME_DEPS keys on the
// `almide_rt_sys_` prefix), so it is emitted only beside a caller.

#[allow(dead_code, non_snake_case)]
mod almide_rt_sys_ffi {
    #[cfg(unix)]
    mod c {
        // SAFETY: each declaration is the libc prototype on Linux and macOS
        // (pthread_t, sighandler_t as pointer-width integers; pid_t, int i32).
        unsafe extern "C" {
            pub fn signal(sig: i32, handler: usize) -> usize;
            pub fn pthread_self() -> usize;
            pub fn pthread_kill(thread: usize, sig: i32) -> i32;
            pub fn _exit(code: i32) -> !;
            pub fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32;
            pub fn kill(pid: i32, sig: i32) -> i32;
        }
    }

    // POSIX values, identical on Linux and macOS / BSD.
    pub const SIGINT: i32 = 2;
    pub const SIGPIPE: i32 = 13;
    pub const SIGTERM: i32 = 15;

    /// A signal disposition: the default action, ignore, a handler, or the
    /// value `set_signal` answered (the one to restore). Built only from those,
    /// so `signal(2)` is never handed an arbitrary address.
    #[cfg(unix)]
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct Disposition(usize);

    #[cfg(unix)]
    impl Disposition {
        pub const DEFAULT: Disposition = Disposition(0);
        pub const IGNORE: Disposition = Disposition(1);
        pub fn handler(f: extern "C" fn(i32)) -> Disposition {
            Disposition(f as usize)
        }
    }

    /// `signal(sig, d)`: answers the previous disposition.
    #[cfg(unix)]
    pub fn set_signal(sig: i32, d: Disposition) -> Disposition {
        // SAFETY: `d` is SIG_DFL, SIG_IGN, an `extern "C" fn(i32)`, or a value
        // signal() itself returned for this process (see `Disposition`).
        Disposition(unsafe { c::signal(sig, d.0) })
    }

    /// The calling thread's `pthread_t`.
    #[cfg(unix)]
    pub fn thread_self() -> usize {
        // SAFETY: no arguments; pthread_self cannot fail.
        unsafe { c::pthread_self() }
    }

    /// `pthread_kill(thread, sig)`. `thread` must be a `thread_self()` of a
    /// thread that has not been joined (the http serving thread, which is
    /// inside `http.serve` while its handler is installed).
    #[cfg(unix)]
    pub fn thread_kill(thread: usize, sig: i32) -> i32 {
        // SAFETY: the caller passes a live thread's id (above); the signal is
        // one this process has a handler or a disposition for.
        unsafe { c::pthread_kill(thread, sig) }
    }

    /// `_exit(code)`: ends the process now, without atexit handlers or
    /// unwinding — the one exit a signal handler may take.
    #[cfg(unix)]
    pub fn exit_now(code: i32) -> ! {
        // SAFETY: _exit takes any int and never returns.
        unsafe { c::_exit(code) }
    }

    /// `waitpid(pid, WNOHANG | WUNTRACED)`: the raw status when `pid` has
    /// exited or stopped, `None` while it runs (or on error).
    #[cfg(unix)]
    pub fn wait_exited_or_stopped(pid: i32) -> Option<i32> {
        const WNOHANG: i32 = 1;
        const WUNTRACED: i32 = 2;
        let mut raw: i32 = 0;
        // SAFETY: `raw` is a live, writable i32 for the whole call.
        let r = unsafe { c::waitpid(pid, &mut raw, WNOHANG | WUNTRACED) };
        (r == pid).then_some(raw)
    }

    /// `kill(pid, sig)` (#4125): `Err(errno)` on failure. The syscall itself,
    /// so `process.kill` never runs a `kill` found on PATH. The
    /// caller (process_core's `almide_proc_kill`) passes only pid > 0 and
    /// 0 <= sig <= 31, so a process group or "every process" is never named.
    #[cfg(unix)]
    pub fn send_signal(pid: i32, sig: i32) -> Result<(), i32> {
        // SAFETY: kill takes two ints and touches no memory of ours.
        if unsafe { c::kill(pid, sig) } == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error().raw_os_error().unwrap_or(0))
        }
    }

    /// The calling thread's CPU time in ns (CLOCK_THREAD_CPUTIME_ID), `None`
    /// if the clock cannot be read.
    #[cfg(all(any(target_os = "linux", target_os = "macos"), target_pointer_width = "64"))]
    pub fn thread_cpu_ns() -> Option<u64> {
        #[repr(C)]
        struct Timespec { sec: i64, nsec: i64 }
        // `clockid_t` is `int` on Linux and an unsigned enum on macOS. Declaring
        // it i32 on both passed the same bits, but a foreign signature that does
        // not match the real one is UB, and Miri reports it (#4009).
        #[cfg(target_os = "macos")]
        type ClockId = u32;
        #[cfg(not(target_os = "macos"))]
        type ClockId = i32;
        // SAFETY: clock_gettime's prototype, with this target's clockid_t (above).
        unsafe extern "C" {
            fn clock_gettime(clock: ClockId, ts: *mut Timespec) -> i32;
        }
        const CLOCK: ClockId = if cfg!(target_os = "linux") { 3 } else { 16 };
        let mut ts = Timespec { sec: 0, nsec: 0 };
        // SAFETY: `ts` is a live, writable timespec (two 64-bit fields on both
        // 64-bit targets) for the whole call.
        if unsafe { clock_gettime(CLOCK, &mut ts) } != 0 {
            return None;
        }
        Some((ts.sec as u64).wrapping_mul(1_000_000_000).wrapping_add(ts.nsec as u64))
    }

    /// Fill `chunk` (at most 256 bytes) from the OS CSPRNG (`random.bytes`,
    /// C-394): getentropy(3) on every unix — glibc 2.25+ (the release that
    /// also added the getrandom(3) wrapper this replaced; both are the
    /// getrandom(2) syscall there), musl, macOS and the BSDs — and
    /// BCryptGenRandom with the system-preferred RNG on Windows. `false` when
    /// the source fails (or the chunk is over 256 bytes, which getentropy
    /// refuses with EIO); the caller aborts rather than hand back fewer bytes.
    #[cfg(unix)]
    pub fn fill_entropy(chunk: &mut [u8]) -> bool {
        // SAFETY: the declaration is getentropy's POSIX.1-2024 prototype.
        unsafe extern "C" {
            fn getentropy(buf: *mut u8, buflen: usize) -> i32;
        }
        // SAFETY: `chunk` is one live &mut [u8]; getentropy writes at most
        // chunk.len() bytes of it.
        unsafe { getentropy(chunk.as_mut_ptr(), chunk.len()) == 0 }
    }

    #[cfg(windows)]
    pub fn fill_entropy(chunk: &mut [u8]) -> bool {
        // SAFETY: the declaration is bcrypt.h's BCryptGenRandom (NTSTATUS is i32).
        #[link(name = "bcrypt")]
        unsafe extern "system" {
            fn BCryptGenRandom(alg: *mut std::ffi::c_void, buf: *mut u8, len: u32, flags: u32) -> i32;
        }
        const BCRYPT_USE_SYSTEM_PREFERRED_RNG: u32 = 0x0000_0002;
        let Ok(len) = u32::try_from(chunk.len()) else { return false };
        // SAFETY: a null algorithm handle is the documented form with
        // BCRYPT_USE_SYSTEM_PREFERRED_RNG; `chunk` is one live &mut [u8] of
        // `len` bytes, all the call writes.
        unsafe { BCryptGenRandom(std::ptr::null_mut(), chunk.as_mut_ptr(), len, BCRYPT_USE_SYSTEM_PREFERRED_RNG) == 0 }
    }

    /// `SetConsoleCtrlHandler(handler, add)`.
    #[cfg(windows)]
    pub fn set_console_ctrl_handler(handler: extern "system" fn(u32) -> i32, add: bool) -> bool {
        // SAFETY: wincon.h's prototype (a PHANDLER_ROUTINE, a BOOL).
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn SetConsoleCtrlHandler(handler: Option<extern "system" fn(u32) -> i32>, add: i32) -> i32;
        }
        // SAFETY: `handler` is a function of the PHANDLER_ROUTINE signature,
        // alive for the whole program.
        unsafe { SetConsoleCtrlHandler(Some(handler), add as i32) != 0 }
    }
}
