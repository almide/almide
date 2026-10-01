//! A test instrument: the system allocator under a switchable allocation
//! pressure (#3143). Nothing in the compiler installs it — a determinism
//! gate's test binary does (`#[global_allocator]`), and lives here only
//! because the crates that own those gates forbid `unsafe`.
//!
//! The emitter's output must not depend on which addresses the allocator
//! hands out. It once did: a side table keyed by an IR node's address gave
//! a dropped temporary's fact to the next node allocated there, and a
//! node rebuilt right after its twin was dropped lands on the twin's
//! address every time under a LIFO free list — so one heap history is
//! not a test. [`Pressure::Quarantine`] holds every freed block in a ring
//! of [`RING`] before really freeing it, so an address is not handed
//! straight back; [`Pressure::Ballast`] takes and holds a pseudo-random
//! block on every fifth allocation, so size classes fill in another order.
//! Layouts are never changed — only WHEN a block is really freed and what
//! else the heap holds — so a pressure can be switched on mid-process
//! without a free ever disagreeing with its allocation.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};

/// Blocks held at once (quarantined frees, or ballast).
pub const RING: usize = 256;

/// One heap history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pressure {
    /// The system allocator as is.
    Plain,
    /// Every free delayed behind [`RING`] others.
    Quarantine,
    /// A held block of pseudo-random size on every fifth allocation.
    Ballast,
}

impl Pressure {
    pub const ALL: [Pressure; 3] = [Pressure::Plain, Pressure::Quarantine, Pressure::Ballast];

    pub fn name(self) -> &'static str {
        match self {
            Pressure::Plain => "plain",
            Pressure::Quarantine => "quarantine",
            Pressure::Ballast => "ballast",
        }
    }

    pub fn from_name(name: &str) -> Option<Pressure> {
        Pressure::ALL.into_iter().find(|p| p.name() == name)
    }
}

static MODE: AtomicU8 = AtomicU8::new(0);
static LOCK: AtomicBool = AtomicBool::new(false);
static COUNT: AtomicUsize = AtomicUsize::new(0);

/// A held block: (address, size, align).
type Block = (usize, usize, usize);

/// Held blocks, and the next slot to replace.
struct Held(UnsafeCell<([Block; RING], usize)>);
// SAFETY: every access goes through `park`, under LOCK.
unsafe impl Sync for Held {}
static HELD: Held = Held(UnsafeCell::new(([(0, 0, 0); RING], 0)));

/// Park a block in the ring; the block it displaces comes back for the
/// caller to really free (outside the lock).
fn park(block: Block) -> Block {
    while LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        std::hint::spin_loop();
    }
    // SAFETY: LOCK is held, so this is the only reference.
    let out = unsafe {
        let (ring, next) = &mut *HELD.0.get();
        let out = std::mem::replace(&mut ring[*next], block);
        *next = (*next + 1) % RING;
        out
    };
    LOCK.store(false, Ordering::Release);
    out
}

fn release(block: Block) {
    if block.0 != 0 {
        // SAFETY: the block came from `System` with exactly this layout.
        unsafe { System.dealloc(block.0 as *mut u8, Layout::from_size_align_unchecked(block.1, block.2)) }
    }
}

/// The allocator. Install it with `#[global_allocator]` in a test binary,
/// then pick a history with [`set_pressure`] before the work under test.
pub struct PressureAlloc;

// SAFETY: every path forwards to `System` with the caller's own layout;
// a quarantined block is freed later with the layout it was allocated with.
unsafe impl GlobalAlloc for PressureAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if MODE.load(Ordering::Relaxed) == Pressure::Ballast as u8 {
            let n = COUNT.fetch_add(1, Ordering::Relaxed);
            if n.is_multiple_of(5) {
                // A splitmix64 scramble: sizes 16..=1024 in no order.
                let mut h = (n as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
                h ^= h >> 31;
                h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
                h ^= h >> 29;
                let size = 16 + (h % 127) as usize * 8;
                // SAFETY: a non-zero size and a power-of-two align.
                let b = unsafe { System.alloc(Layout::from_size_align_unchecked(size, 8)) };
                if !b.is_null() {
                    release(park((b as usize, size, 8)));
                }
            }
        }
        // SAFETY: the caller's layout, forwarded.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if MODE.load(Ordering::Relaxed) == Pressure::Quarantine as u8 {
            release(park((ptr as usize, layout.size(), layout.align())));
        } else {
            // SAFETY: the caller's block and layout, forwarded.
            unsafe { System.dealloc(ptr, layout) }
        }
    }
}

/// Switch this process's heap history. Takes effect for every allocation
/// and free after it (blocks already held stay held).
pub fn set_pressure(p: Pressure) {
    MODE.store(p as u8, Ordering::Relaxed);
}
