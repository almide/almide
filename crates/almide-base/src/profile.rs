//! The ONE sanctioned place in the compiler that may read the wall clock.
//!
//! `std::time::Instant::now()` PANICS on `wasm32-unknown-unknown` (time is
//! unsupported there) — and the compiler runs on that target in the browser
//! playground. Every other crate is forbidden (CI-enforced, see the
//! `forbidden-impurities` check) from naming `std::time` / `Instant` /
//! `SystemTime` directly; they must time through this shim, which is
//! `#[cfg]`-gated to be a no-op on wasm32. This makes the "unconditional clock
//! read crashes the in-browser compiler" bug class un-writable by construction.
//!
//! Determinism corollary: timing must NEVER influence emitted output — it is
//! diagnostics only. This type only exposes elapsed seconds for `eprintln!`.
//!
//! Two consumers live here:
//!   * `ProfileTimer` — ad-hoc `ALMIDE_PROFILE` marks in codegen.
//!   * the PHASE ACCOUNTING below — `almide check --timings` (#1311), which
//!     needs lex / parse / check attributed separately across a whole
//!     multi-module resolve, not one span in one function.

/// A wall-clock timer for `ALMIDE_PROFILE` diagnostics. `start(false)` and every
/// call on wasm32 yield `None`, so profiling silently no-ops where there is no
/// clock — and a misuse that fed timing into codegen would have nothing to read.
pub struct ProfileTimer {
    #[cfg(not(target_arch = "wasm32"))]
    start: std::time::Instant,
}

impl ProfileTimer {
    /// Begin timing iff `enabled` and a clock exists (never on wasm32).
    #[inline]
    pub fn start(enabled: bool) -> Option<ProfileTimer> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            enabled.then(|| ProfileTimer { start: std::time::Instant::now() })
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = enabled;
            None
        }
    }

    /// Seconds since `start`. Always finite; 0.0 where there is no clock.
    #[inline]
    pub fn elapsed_secs(&self) -> f64 {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.start.elapsed().as_secs_f64()
        }
        #[cfg(target_arch = "wasm32")]
        {
            0.0
        }
    }
}

// ── Front-end phase accounting (#1311) ──────────────────────────────────────
//
// `almide check` on a 300-module project spends its time in three places, and
// one aggregate wall-clock number cannot tell them apart: a regression in the
// checker hidden behind a fast lexer looks exactly like no regression at all.
// These counters attribute the time, so the throughput ratchet can hold a band
// per phase.
//
// WHY A THREAD-LOCAL AND NOT ATOMICS. `AtomicU64`/`fetch_add` are forbidden in
// the compile-path crates (scripts/check-forbidden-impurities.sh) precisely
// because a never-reset global counter is the shape that leaks non-determinism
// into output. This accumulator is per-thread, off by default, write-only from
// the compiler's point of view (nothing in the pipeline can read it), and read
// once at the end of `cmd_check --timings`. The compile path is single-threaded,
// so a thread-local is also the cheaper primitive.
//
// The accounting is OFF unless `phase_accounting_on()` was called, so a normal
// `almide check` pays one thread-local read per lex/parse/check entry and no
// clock reads at all. That matters: the edit-loop scale ratchet
// (scripts/check-edit-loop-scale.sh) times the same command, and instrumentation
// that moved its numbers would be measuring itself.

/// The front-end phases accounted separately by `almide check --timings`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    Lex = 0,
    Parse = 1,
    Check = 2,
}

/// Stable wire names, in `Phase` discriminant order. The throughput ratchet's
/// baseline file keys off these, so they are API.
pub const PHASE_NAMES: [&str; 3] = ["lex", "parse", "check"];

#[derive(Copy, Clone)]
struct Acct {
    on: bool,
    nanos: [u64; 3],
    /// Re-entry depth per phase: only the OUTERMOST scope of a phase counts, so
    /// a nested call (a parser that re-lexes, a checker that re-enters itself)
    /// cannot bill the same wall time twice.
    depth: [u32; 3],
    lines: u64,
    bytes: u64,
    sources: u64,
}

const EMPTY: Acct = Acct { on: false, nanos: [0; 3], depth: [0; 3], lines: 0, bytes: 0, sources: 0 };

thread_local! {
    static ACCT: core::cell::Cell<Acct> = const { core::cell::Cell::new(EMPTY) };
}

#[inline]
fn with<R>(f: impl FnOnce(&mut Acct) -> R) -> R {
    ACCT.with(|c| {
        let mut a = c.get();
        let r = f(&mut a);
        c.set(a);
        r
    })
}

/// Arm phase accounting for the current thread. Idempotent. Where there is no
/// clock (wasm32) every scope still yields `None`, so the report reads zeros
/// rather than panicking.
pub fn phase_accounting_on() {
    with(|a| a.on = true);
}

/// An open phase span. Bills its elapsed time to the phase on drop, so an early
/// `return`/`?` out of a parse or an inference still lands in the right bucket.
pub struct PhaseScope {
    #[cfg(not(target_arch = "wasm32"))]
    phase: usize,
    #[cfg(not(target_arch = "wasm32"))]
    start: std::time::Instant,
}

/// Open a span for `p`. `None` — and therefore no clock read at all — when
/// accounting is off, when this phase is already open (re-entry), or on wasm32.
#[inline]
pub fn phase_scope(p: Phase) -> Option<PhaseScope> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let i = p as usize;
        let opened = with(|a| {
            if !a.on || a.depth[i] > 0 {
                return false;
            }
            a.depth[i] += 1;
            true
        });
        opened.then(|| PhaseScope { phase: i, start: std::time::Instant::now() })
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = p;
        None
    }
}

impl Drop for PhaseScope {
    fn drop(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let ns = self.start.elapsed().as_nanos() as u64;
            let i = self.phase;
            with(|a| {
                a.nanos[i] += ns;
                a.depth[i] -= 1;
            });
        }
    }
}

/// Note a source text that is about to be lexed. Called OUTSIDE the `Lex` scope
/// on purpose: the newline scan is the measurement's own cost, and billing it to
/// `lex` would inflate exactly the phase share the ratchet reads.
#[inline]
pub fn note_source(src: &str) {
    if !with(|a| a.on) {
        return;
    }
    let lines = src.as_bytes().iter().filter(|b| **b == b'\n').count() as u64;
    let bytes = src.len() as u64;
    with(|a| {
        a.lines += lines;
        a.bytes += bytes;
        a.sources += 1;
    });
}

/// What the front end spent, by phase. `None` when accounting was never armed.
pub struct PhaseReport {
    /// Nanoseconds per phase, indexed by `Phase` / [`PHASE_NAMES`].
    pub nanos: [u64; 3],
    /// Newlines across every source text handed to the lexer — the denominator
    /// of a lines/sec number, and the corpus-size floor a blind gate needs.
    pub lines: u64,
    pub bytes: u64,
    /// How many source texts were lexed (entry + modules + bundled stdlib).
    pub sources: u64,
}

/// Read the accumulated phase numbers. Does not reset — one process, one report.
pub fn phase_report() -> Option<PhaseReport> {
    with(|a| {
        a.on.then(|| PhaseReport { nanos: a.nanos, lines: a.lines, bytes: a.bytes, sources: a.sources })
    })
}
