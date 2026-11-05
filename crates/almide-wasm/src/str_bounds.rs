//! String byte reads in user code (#4281), each recorded in the bounds
//! witness with the test that ALREADY keeps it inside its block — no test
//! is added for the record — so the proven bounds checker (`BoundsCert`,
//! W = 1) and almide-anchor-check (operands.rs `covers`, the one-byte
//! stride) hold it the way they hold a list element:
//!
//! - a byte loop's exit test `k >=u n; br_if 1`, then the read `s + k` at
//!   the payload: `e N a N`, the shape of a `for` loop's element read
//!   (#4280). `n` is the string's `len` word (captured or re-read), or a
//!   count an enclosing test bounds by it (`n >u len(s)` / `len(s) != n`,
//!   the read in that test's `else`);
//! - the surface's own test opening an `if` whose `else` holds the read:
//!   `idx >=u len(s)` (the read `s + idx`) or `len(s) == 0` (the read of
//!   byte 0): `c N a N`, the `c` anchored at that `if`.
//!
//! The string routines' own bodies (equality, concat, the UTF-8 helpers,
//! the search table) are runtime routines and record nothing here; those
//! whose byte loop has the shape above record it themselves (#4005,
//! `witness::bounds::helper_loop_exit`), the rest are listed in
//! proofs/TRUSTED_BASE.md. A user-code read whose
//! bound takes more than one of these facts (a suffix compare at
//! `len(s) - len(p) + k`, a backward walk, a read at `k - 1`) records
//! nothing either: it is listed, uncounted by this mechanism, in
//! docs/TRUSTED_BASE.md.

use crate::emitter::Emitter;
use crate::string_scan::str_byte;
use crate::witness::bounds;

impl Emitter<'_> {
    /// The surface's own test just opened its `if` (`idx >=u len(s)` or
    /// `len(s) == 0`, the read below to come in its `else`): the slot that
    /// read is recorded under.
    pub(crate) fn str_test(&mut self) -> Option<bounds::Site> {
        let site = bounds::site(None);
        bounds::check(site, self.f);
        site
    }

    /// The exit test `k >=u n; br_if 1` of a byte loop was just emitted: a
    /// slot a read below it is recorded under (one per string it reads).
    pub(crate) fn str_loop_exit(&mut self) -> Option<bounds::Site> {
        let site = bounds::site(None);
        bounds::exit(site, self.f);
        site
    }

    /// A byte read under the test `site` was just emitted (its address
    /// computed by the caller).
    pub(crate) fn str_read(&mut self, site: Option<bounds::Site>) {
        bounds::access(site, false, self.f);
    }

    /// The read `s + k` (`None`: byte 0, `s`) under the test `site`
    /// recorded; the byte is left on the stack as an i32.
    pub(crate) fn str_byte_at(&mut self, site: Option<bounds::Site>, s: u32, k: Option<u32>) {
        let mut i = self.f.instructions();
        i.local_get(s);
        if let Some(k) = k {
            i.local_get(k).i32_add();
        }
        i.i32_load8_u(str_byte());
        bounds::access(site, false, self.f);
    }
}
