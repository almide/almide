//! Call-stack exhaustion as the defined abort on native (C-196, ALS-T6) —
//! the Rust source both native renderers emit (`almide-codegen`'s runtime
//! prelude and `almide-mir`'s v1 trust-spine render), kept here, the one
//! crate both depend on, so there is one copy.
//!
//! A native program that runs out of call stack hits its thread's guard
//! region. Rust's std answers that with its own SIGSEGV/SIGBUS handler —
//! `thread 'main' has overflowed its stack` / `fatal runtime error: stack
//! overflow, aborting`, SIGABRT, exit 134 — which is ALS-T6's forbidden form.
//! `almide_stack_guard::install()` (the first statement of every native
//! `main`) replaces that handler with one that prints the termination
//! convention's line, `Error: stack overflow`, and exits 1, as the embedded
//! wasm host does for wasmtime's stack-overflow trap. The THRESHOLD is not
//! touched: it stays each thread's own stack, declared per target in C-196.
//!
//! The handler runs on the faulting thread's alternate signal stack. std puts
//! one on the main thread and on every thread it spawns whenever it installs
//! its own handler at startup (SIGSEGV/SIGBUS at their default disposition),
//! and replacing the handler keeps those stacks. It does only what a signal
//! handler may do: read the fault address, compare it with the thread's stack
//! bounds, `write(2)` and `_exit(1)`. A fault outside the guard region goes to
//! the handler that was installed before (std's, which re-raises it with the
//! default action), so a genuine wild access still dies the way it did.
//!
//! The stack bounds of the faulting thread:
//! - macOS: read in the handler (`pthread_get_stackaddr_np` /
//!   `pthread_get_stacksize_np` read the thread's descriptor; no lock, no
//!   allocation).
//! - Linux: `pthread_getattr_np` allocates (and reads /proc for the main
//!   thread), so it cannot run in a handler. Each thread records its bounds
//!   once, in a const thread-local, through `almide_stack_guard::register()`:
//!   `install` does it for `main`, and the codegen runtime calls it at every
//!   worker entry (`almide_fan_enter`, `almide_fan_adopt`, the `fs`
//!   line-fold workers). A thread that never registered keeps std's answer —
//!   the residual C-196 declares.
//!
//! The emitting side supplies `almide_stack_guard_stdout()` next to this
//! module: what the handler writes out of the faulting thread's stdout buffer
//! before the line (nothing, where stdout is std's own line-buffered handle).

/// The `almide_stack_guard` module, verbatim Rust source. It calls
/// `super::almide_stack_guard_stdout()`, which the emitter defines beside it
/// under the same `cfg`.
pub const NATIVE_STACK_GUARD: &str = r#"#[cfg(any(target_os = "linux", target_os = "macos"))]
mod almide_stack_guard {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    #[cfg(target_os = "linux")]
    mod sys {
        pub const SIGBUS: i32 = 7;
        pub const SA_SIGINFO: i32 = 4;
        pub const SA_ONSTACK: i32 = 0x0800_0000;
        pub const ADDR_OFFSET: usize = 16;
        #[repr(C)]
        pub struct SigAction { pub handler: usize, pub mask: [u64; 16], pub flags: i32, pub restorer: usize }
        impl SigAction { pub fn new(handler: usize, flags: i32) -> Self { SigAction { handler, mask: [0; 16], flags, restorer: 0 } } }
        extern "C" {
            pub fn pthread_self() -> usize;
            pub fn pthread_getattr_np(thread: usize, attr: *mut u64) -> i32;
            pub fn pthread_attr_getstack(attr: *const u64, addr: *mut usize, size: *mut usize) -> i32;
            pub fn pthread_attr_getguardsize(attr: *const u64, size: *mut usize) -> i32;
            pub fn pthread_attr_destroy(attr: *mut u64) -> i32;
        }
    }
    #[cfg(target_os = "macos")]
    mod sys {
        pub const SIGBUS: i32 = 10;
        pub const SA_SIGINFO: i32 = 0x40;
        pub const SA_ONSTACK: i32 = 0x1;
        pub const ADDR_OFFSET: usize = 24;
        #[repr(C)]
        pub struct SigAction { pub handler: usize, pub mask: u32, pub flags: i32 }
        impl SigAction { pub fn new(handler: usize, flags: i32) -> Self { SigAction { handler, mask: 0, flags } } }
        extern "C" {
            pub fn pthread_self() -> usize;
            pub fn pthread_get_stackaddr_np(thread: usize) -> usize;
            pub fn pthread_get_stacksize_np(thread: usize) -> usize;
        }
    }
    const SIGSEGV: i32 = 11;
    const MSG: &[u8] = b"Error: stack overflow\n";
    // A fault within this distance of the stack's lowest address, on either
    // side, is the stack running out. Below: a frame is probed page by page,
    // so the first fault lands in the first page past the limit. Above: the
    // guard can sit inside the reported stack (macOS reports the main thread's
    // 16 KiB guard page inside its 8 MiB; glibc before 2.27 did the same), and
    // memory there faults for no other reason. 64 KiB covers 64 KiB pages.
    const MARGIN: usize = 64 * 1024;
    extern "C" {
        fn sigaction(sig: i32, act: *const sys::SigAction, old: *mut sys::SigAction) -> i32;
        fn write(fd: i32, buf: *const std::ffi::c_void, n: usize) -> isize;
        fn pause() -> i32;
        fn _exit(code: i32) -> !;
    }
    // The action each signal had before ours: [SIGSEGV, SIGBUS] x (handler, flags).
    static PREV: [(AtomicUsize, AtomicUsize); 2] = [(AtomicUsize::new(0), AtomicUsize::new(0)), (AtomicUsize::new(0), AtomicUsize::new(0))];
    static FIRED: AtomicBool = AtomicBool::new(false);
    #[cfg(target_os = "linux")]
    thread_local! {
        static LOW: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
        static GUARD: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }
    pub fn install() {
        for (slot, &sig) in [SIGSEGV, sys::SIGBUS].iter().enumerate() {
            let act = sys::SigAction::new(on_fault as extern "C" fn(i32, *mut u8, *mut u8) as usize, sys::SA_SIGINFO | sys::SA_ONSTACK);
            let mut old = sys::SigAction::new(0, 0);
            if unsafe { sigaction(sig, &act, &mut old) } == 0 {
                PREV[slot].0.store(old.handler, Ordering::SeqCst);
                PREV[slot].1.store(old.flags as u32 as usize, Ordering::SeqCst);
            }
        }
        register();
    }
    #[cfg(target_os = "linux")]
    pub fn register() {
        if LOW.with(|c| c.get()) != 0 {
            return;
        }
        let mut attr = [0u64; 16];
        unsafe {
            if sys::pthread_getattr_np(sys::pthread_self(), attr.as_mut_ptr()) != 0 {
                return;
            }
            let (mut addr, mut size, mut guard) = (0usize, 0usize, 0usize);
            if sys::pthread_attr_getstack(attr.as_ptr(), &mut addr, &mut size) == 0 && addr != 0 {
                sys::pthread_attr_getguardsize(attr.as_ptr(), &mut guard);
                GUARD.with(|c| c.set(guard));
                LOW.with(|c| c.set(addr));
            }
            sys::pthread_attr_destroy(attr.as_mut_ptr());
        }
    }
    #[cfg(target_os = "macos")]
    pub fn register() {}
    // (lowest stack address, guard bytes at or above it) of the current thread.
    #[cfg(target_os = "linux")]
    fn bounds() -> Option<(usize, usize)> {
        let low = LOW.with(|c| c.get());
        if low == 0 { None } else { Some((low, GUARD.with(|c| c.get()))) }
    }
    #[cfg(target_os = "macos")]
    fn bounds() -> Option<(usize, usize)> {
        let t = unsafe { sys::pthread_self() };
        let top = unsafe { sys::pthread_get_stackaddr_np(t) };
        let size = unsafe { sys::pthread_get_stacksize_np(t) };
        if top == 0 || size == 0 || size > top { None } else { Some((top - size, 0)) }
    }
    pub fn raw_write(fd: i32, mut bytes: &[u8]) {
        while !bytes.is_empty() {
            let n = unsafe { write(fd, bytes.as_ptr().cast(), bytes.len()) };
            if n <= 0 {
                return;
            }
            bytes = &bytes[n as usize..];
        }
    }
    extern "C" fn on_fault(sig: i32, info: *mut u8, ctx: *mut u8) {
        let addr = unsafe { std::ptr::read_unaligned(info.add(sys::ADDR_OFFSET) as *const usize) };
        if let Some((low, guard)) = bounds() {
            let margin = MARGIN.max(guard);
            if addr >= low.saturating_sub(margin) && addr < low.saturating_add(margin) {
                if FIRED.swap(true, Ordering::SeqCst) {
                    loop { unsafe { pause(); } }
                }
                super::almide_stack_guard_stdout();
                raw_write(2, MSG);
                unsafe { _exit(1) }
            }
        }
        let slot = if sig == SIGSEGV { 0 } else { 1 };
        let (handler, flags) = (PREV[slot].0.load(Ordering::SeqCst), PREV[slot].1.load(Ordering::SeqCst));
        if handler > 1 {
            if flags & sys::SA_SIGINFO as u32 as usize != 0 {
                let f: extern "C" fn(i32, *mut u8, *mut u8) = unsafe { std::mem::transmute(handler) };
                f(sig, info, ctx);
            } else {
                let f: extern "C" fn(i32) = unsafe { std::mem::transmute(handler) };
                f(sig);
            }
            return;
        }
        // No handler before ours: restore the default and return, so the
        // faulting instruction re-runs and takes the default action.
        let dfl = sys::SigAction::new(0, 0);
        unsafe { sigaction(sig, &dfl, std::ptr::null_mut()); }
    }
}
"#;
