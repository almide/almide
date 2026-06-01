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
