//! String byte reads in user code (#4281), each recorded in the bounds
//! witness with the test that keeps it inside its block, so the proven
//! bounds checker (`BoundsCert`, W = 1) and almide-anchor-check
//! (operands.rs `covers`, the one-byte stride) hold it the way they hold a
//! list element:
//!
//! - a byte loop's exit test `k >=u n; br_if 1` with `n` the string's
//!   `len` word (captured or re-read), then the read `s + k` at the payload:
//!   `e N a N`, the shape of a `for` loop's element read (#4280);
//! - any other read: a guard `idx >=u len(s)` opening an `if` that aborts,
//!   then the read `s + idx` (or `s` for byte 0): `c N a N`. The guard is
//!   never taken where it is emitted (each sits behind the surface's own
//!   test); it is the test the byte check can read.
//!
//! The string routines' own bodies (equality, concat, the UTF-8 helpers,
//! the search table) are runtime routines: their reads belong to #4278's
//! per-helper certificates and record nothing here.

use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::string_scan::str_byte;
use crate::witness::bounds;
use crate::*;

impl Emitter<'_> {
    /// The guarded read of byte `idx` (`None`: byte 0) of the string in
    /// local `s`; the byte is left on the stack as an i32.
    pub(crate) fn str_guarded_byte(&mut self, s: u32, idx: Option<u32>) {
        let site = bounds::site(None);
        {
            let mut i = self.f.instructions();
            match idx {
                Some(x) => i.local_get(x),
                None => i.i32_const(0),
            };
            i.local_get(s).i32_load(len_memarg()).i32_ge_u().if_(BlockType::Empty).i32_const(0);
        }
        bounds::check(site, self.f);
        {
            let mut i = self.f.instructions();
            i.drop().unreachable().end();
            i.local_get(s);
            if let Some(x) = idx {
                i.local_get(x).i32_add();
            }
            i.i32_load8_u(str_byte());
        }
        bounds::access(site, false, self.f);
    }

    /// The exit test `k >=u n; br_if 1` of a byte loop over a string's
    /// `len` was just emitted: the slot its read below is recorded under.
    pub(crate) fn str_loop_exit(&mut self) -> Option<bounds::Site> {
        let site = bounds::site(None);
        bounds::exit(site, self.f);
        site
    }

    /// The read `s + k` of the byte loop whose exit test `site` recorded;
    /// the byte is left on the stack as an i32.
    pub(crate) fn str_loop_byte(&mut self, site: Option<bounds::Site>, s: u32, k: u32) {
        self.f.instructions().local_get(s).local_get(k).i32_add().i32_load8_u(str_byte());
        bounds::access(site, false, self.f);
    }
}
