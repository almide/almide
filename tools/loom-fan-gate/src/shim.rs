//! What the extracted gate source is rewritten onto: loom's primitives, each
//! wrapped with a `Live` witness (see lib.rs).
//!
//! SeqCst. Loom models a SeqCst access as AcqRel, which admits store-buffering
//! outcomes C11 forbids — and the slot protocol is a store-buffering shape
//! (withdraw: store the null, load `readers`; steal: add to `readers`, load
//! the pointer). Taken as is, loom would report that outcome as a protocol
//! bug that no C11 execution has. Loom does model `fence(SeqCst)`, so a SeqCst
//! access here is a SeqCst fence, the access, and another SeqCst fence. That
//! restores the C11 SC order among SeqCst accesses; it also orders the
//! surrounding non-SeqCst accesses of the same thread more strictly than a
//! bare SeqCst access would, which is the model's trust boundary (recorded in
//! proofs/gate-verification.toml under this gate's `blind`).

use std::sync::atomic::Ordering as O;

pub struct Live(loom::cell::UnsafeCell<()>);

// SAFETY: the cell holds `()` and is only used as loom's access witness.
unsafe impl Sync for Live {}
unsafe impl Send for Live {}

impl Live {
    pub fn new() -> Self {
        Live(loom::cell::UnsafeCell::new(()))
    }
    #[inline]
    pub fn touch(&self) {
        self.0.with(|_| ());
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        self.0.with_mut(|_| ());
    }
}

#[inline]
fn sc<R>(o: O, f: impl FnOnce() -> R) -> R {
    if o == O::SeqCst {
        loom::sync::atomic::fence(O::SeqCst);
    }
    let r = f();
    if o == O::SeqCst {
        loom::sync::atomic::fence(O::SeqCst);
    }
    r
}

pub mod atomic {
    pub use std::sync::atomic::Ordering;
    use super::{sc, Live};

    macro_rules! int_atomic {
        ($name:ident, $t:ty) => {
            pub struct $name {
                a: loom::sync::atomic::$name,
                live: Live,
            }
            impl $name {
                pub fn new(v: $t) -> Self {
                    $name { a: loom::sync::atomic::$name::new(v), live: Live::new() }
                }
                pub fn load(&self, o: Ordering) -> $t {
                    self.live.touch();
                    sc(o, || self.a.load(o))
                }
                pub fn store(&self, v: $t, o: Ordering) {
                    self.live.touch();
                    sc(o, || self.a.store(v, o))
                }
                pub fn swap(&self, v: $t, o: Ordering) -> $t {
                    self.live.touch();
                    sc(o, || self.a.swap(v, o))
                }
                pub fn compare_exchange(&self, cur: $t, new: $t, s: Ordering, f: Ordering) -> Result<$t, $t> {
                    self.live.touch();
                    sc(s, || self.a.compare_exchange(cur, new, s, f))
                }
                pub fn compare_exchange_weak(&self, cur: $t, new: $t, s: Ordering, f: Ordering) -> Result<$t, $t> {
                    self.live.touch();
                    sc(s, || self.a.compare_exchange_weak(cur, new, s, f))
                }
            }
        };
    }
    macro_rules! num_ops {
        ($name:ident, $t:ty) => {
            impl $name {
                pub fn fetch_add(&self, v: $t, o: Ordering) -> $t {
                    self.live.touch();
                    sc(o, || self.a.fetch_add(v, o))
                }
                pub fn fetch_sub(&self, v: $t, o: Ordering) -> $t {
                    self.live.touch();
                    sc(o, || self.a.fetch_sub(v, o))
                }
                pub fn fetch_max(&self, v: $t, o: Ordering) -> $t {
                    self.live.touch();
                    sc(o, || self.a.fetch_max(v, o))
                }
            }
        };
    }
    int_atomic!(AtomicUsize, usize);
    int_atomic!(AtomicU64, u64);
    int_atomic!(AtomicU32, u32);
    int_atomic!(AtomicBool, bool);
    num_ops!(AtomicUsize, usize);
    num_ops!(AtomicU64, u64);
    num_ops!(AtomicU32, u32);

    pub struct AtomicPtr<T> {
        a: loom::sync::atomic::AtomicPtr<T>,
        live: Live,
    }
    impl<T> AtomicPtr<T> {
        pub fn new(p: *mut T) -> Self {
            AtomicPtr { a: loom::sync::atomic::AtomicPtr::new(p), live: Live::new() }
        }
        pub fn load(&self, o: Ordering) -> *mut T {
            self.live.touch();
            sc(o, || self.a.load(o))
        }
        pub fn store(&self, p: *mut T, o: Ordering) {
            self.live.touch();
            sc(o, || self.a.store(p, o))
        }
        pub fn compare_exchange(&self, cur: *mut T, new: *mut T, s: Ordering, f: Ordering) -> Result<*mut T, *mut T> {
            self.live.touch();
            sc(s, || self.a.compare_exchange(cur, new, s, f))
        }
    }
}

pub struct Mutex<T> {
    m: loom::sync::Mutex<T>,
    live: Live,
}

impl<T> Mutex<T> {
    pub fn new(v: T) -> Self {
        Mutex { m: loom::sync::Mutex::new(v), live: Live::new() }
    }
    pub fn lock(&self) -> std::sync::LockResult<loom::sync::MutexGuard<'_, T>> {
        self.live.touch();
        self.m.lock()
    }
}

/// The gate's borrowed taking loop. `get` is an access of the gate.
pub struct UnsafeCell<T> {
    c: std::cell::UnsafeCell<T>,
    live: Live,
}

impl<T> UnsafeCell<T> {
    pub fn new(v: T) -> Self {
        UnsafeCell { c: std::cell::UnsafeCell::new(v), live: Live::new() }
    }
    pub fn get(&self) -> *mut T {
        self.live.touch();
        self.c.get()
    }
}

pub mod thread {
    pub use loom::thread::{current, park, yield_now, Thread};
    pub type Result<T> = std::thread::Result<T>;
    /// A timed park may return before any unpark (the timeout); returning at
    /// once is that, and keeps every caller's wait loop finite under loom.
    pub fn park_timeout(_d: std::time::Duration) {
        loom::thread::yield_now();
    }
}

/// The wait loop's clock: every poll past the 64th counts as past the spin
/// window, so the model reaches the park path as well as the polling one.
#[derive(Clone, Copy)]
pub struct Instant;

impl Instant {
    pub fn now() -> Self {
        Instant
    }
    pub fn elapsed(&self) -> std::time::Duration {
        std::time::Duration::MAX
    }
}
