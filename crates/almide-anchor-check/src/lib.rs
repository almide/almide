//! The translation check of #4011 (d): the bounds and init witnesses'
//! EVENTS are the INSTRUCTIONS the shipped wasm bytes contain.
//!
//! The proven checkers (`BoundsCert.check_bounds_cert`,
//! `InitCert.check_init_cert`) judge an event program the structural emitter
//! records as it emits. That the program describes the bytes was trusted:
//! the recording call sat next to the instruction, by the emitter's
//! discipline. This checker decodes the shipped module ITSELF (wasmparser,
//! with its validator for the operand-stack heights) and holds every located
//! event to the instruction it is anchored at:
//!
//! INIT, a fixed site `h H w W n N r R s i ...` anchored at `a s_0 s_1 ...`:
//! - `a` is `i32.const N*W; call $alloc; local.set B`;
//! - each `s i` is a store of W bytes at offset `H + i*W` whose ADDRESS
//!   operand is pushed by `local.get B`, after `a`, with B written by nothing
//!   in between, at `a`'s control depth with nothing in between leaving it
//!   (no `end` below it, no branch to a label at or outside it);
//! - the stores lie in byte order, and every store in the window addressed
//!   through `local.get B` is one of them (none unrecorded).
//!
//! INIT, a loop fill `h H w W v K r R f L t S` anchored at `a f`:
//! `local.get K; i64.const W; i64.mul; i32.wrap_i64; call $alloc;
//! local.set B` at `a`, then the countdown exactly:
//! `local.get B; i32.const H; i32.add; local.set C; block; loop;
//! local.get L; i64.const 0; i64.le_s; br_if 1; <body>; local.get C;
//! i32.const S; i32.add; local.set C; local.get L; i64.const 1; i64.sub;
//! local.set L; br 0; end; end; local.get B` ending at `f`, where the body
//! holds exactly one store, of W bytes at offset 0 addressed by
//! `local.get C`, no branch, and no write of B, C, L or K.
//!
//! BOUNDS, per token: a `c` is `i64.ge_u` (the list compare) or `i32.eqz`
//! (the Bytes room test), then `if` (empty), then `i32.const` — anchored at
//! the const — and that `if` has no `else` and ends in `unreachable` (the
//! abort); or (#4281, a string's byte) the surface's own `i32.ge_u` /
//! `i32.eqz` test, then an `if` with an `else` — anchored at the `if` —
//! whose `else` holds the access; an `e` (#4280) is `i32.ge_u` then `br_if 1` — anchored at the
//! `br_if` — straight inside a `loop` whose parent `block` it leaves to (a
//! loop's exit test, operands.rs `loop_exit`); an `a` is a load (or a load then the `call` that releases what it
//! loaded); a `w` is a store; a slot's located events lie in one function,
//! in byte order (a `c` before the access it guards).
//!
//! A located event that fails any of these REJECTS. An event anchored `-`
//! (a kill, which no instruction carries; or one in a body that did not ship)
//! is counted, not checked.
//!
//! BOUNDS, the operands (operands.rs): each located `a` / `w` is held to its
//! slot's latest located `c` — the compare tests this access's index against
//! the count of the list it addresses, in the same slot units, the check
//! dominating the access and nothing changing the index or the list's count
//! in between (a recorded copy-on-write judge keeps the count). The anchor
//! file's optional `judges` line locates those judge calls.
//!
//! TRUSTED: that `alloc <index>` names `$alloc`, that a recorded judge site
//! calls a copy-on-write judge, that a recorded loop hold calls rc_inc
//! (#4280), and the representation R (init); the rest of
//! operands.rs's TRUSTED list.


use wasmparser::Operator as Op;

mod anchors;
mod bounds;
mod decode;
mod fields;
mod init;
mod operands;
pub mod rc;
pub use anchors::{fields_anchors, parse_anchors, parse_anchors_held, Anchors, AnchorsHeld, RcLines};
pub use bounds::{check_bounds, check_bounds_alloc, check_bounds_held, Sites};
pub use rc::{check_rc, RcFns, RcReport};
use anchors::helper_sigs;
use decode::branch_target;
pub use decode::{decode, function_imports, store_width, Body};
pub use fields::check_fields;
pub use init::check_init;

/// A located event: a defined function's position and an instruction index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct At {
    pub func: usize,
    pub op: usize,
}

/// An anchor token: located, unlocated (`-`), or broken (anything else).
pub(crate) fn parse_at(t: &str) -> Result<Option<At>, String> {
    if t == "-" {
        return Ok(None);
    }
    let (f, o) = t
        .split_once(':')
        .ok_or_else(|| format!("anchor `{t}` is not position:ordinal"))?;
    match (f.parse(), o.parse()) {
        (Ok(func), Ok(op)) => Ok(Some(At { func, op })),
        _ => Err(format!("anchor `{t}` is not position:ordinal")),
    }
}

/// What a check saw.
#[derive(Default, Debug)]
pub struct Report {
    pub init_sites: usize,
    pub init_stores: usize,
    pub fill_sites: usize,
    pub init_unlocated: usize,
    pub bounds_checks: usize,
    pub bounds_accesses: usize,
    pub bounds_unlocated: usize,
    /// Located accesses whose guard's operands were held to them.
    pub bounds_operands: usize,
    /// #4013 / #4279: the fields witness's located construction sites,
    /// their field stores, field accesses (of which `field_guarded` are
    /// variant case fields held to their tag test), tag reads, and events
    /// in bodies that did not ship.
    pub field_sites: usize,
    pub field_stores: usize,
    pub field_accesses: usize,
    pub field_guarded: usize,
    pub field_tags: usize,
    pub field_unlocated: usize,
    /// #4277, when the anchor file carries `rc` lines.
    pub rc: Option<RcReport>,
}

struct Ctx<'b, 'a> {
    bodies: &'b [Body<'a>],
    alloc: u32,
}

impl<'a> Ctx<'_, 'a> {
    fn body(&self, at: At) -> Result<&Body<'a>, String> {
        let b = self
            .bodies
            .get(at.func)
            .ok_or_else(|| format!("no defined function at position {}", at.func))?;
        if at.op >= b.ops.len() {
            return Err(format!("function {} has no instruction {}", at.func, at.op));
        }
        Ok(b)
    }

    /// `ops[at.op - back + k]` for `k` in the pattern.
    fn expect(
        &self,
        b: &Body<'a>,
        start: usize,
        want: &[&dyn Fn(&Op<'_>) -> bool],
        what: &str,
    ) -> Result<(), String> {
        for (k, f) in want.iter().enumerate() {
            match b.ops.get(start + k) {
                Some(op) if f(op) => {}
                other => return Err(format!("{what}: instruction {} is {other:?}", start + k)),
            }
        }
        Ok(())
    }

    /// Nothing in `(from, to)` leaves the control region of depth `d0`:
    /// no instruction at a depth below it, no branch to a label at or
    /// outside it.
    fn stays(
        &self,
        b: &Body<'a>,
        from: usize,
        to: usize,
        d0: u32,
        what: &str,
    ) -> Result<(), String> {
        for k in from + 1..to {
            if b.depth[k] < d0 || (b.depth[k] == d0 && matches!(b.ops[k], Op::End | Op::Else)) {
                return Err(format!("{what}: instruction {k} leaves the site's block"));
            }
            if let Some(t) = branch_target(&b.ops[k], b.depth[k]) {
                if t <= d0 {
                    return Err(format!(
                        "{what}: instruction {k} ({:?}) branches out of the site's window",
                        b.ops[k]
                    ));
                }
            }
        }
        Ok(())
    }

    fn writes(op: &Op<'_>, l: u32) -> bool {
        matches!(op, Op::LocalSet { local_index } | Op::LocalTee { local_index } if *local_index == l)
    }
}

/// The witness of `property` about `(program)` in a certificate bundle.
pub fn bundle_witness(bundle: &[u8], property: &str) -> Result<String, String> {
    let mut i = bundle
        .iter()
        .position(|&b| b == b'\n')
        .ok_or("bundle: no header")?
        + 1;
    while i < bundle.len() {
        let j = i + bundle[i..]
            .iter()
            .position(|&b| b == b'\n')
            .ok_or("bundle: unterminated line")?;
        let line = std::str::from_utf8(&bundle[i..j]).map_err(|e| e.to_string())?;
        i = j + 1;
        if let Some(rest) = line.strip_prefix("witness ") {
            let mut parts = rest.splitn(3, ' ');
            let (p, n, f) = (
                parts.next().unwrap_or(""),
                parts.next().unwrap_or(""),
                parts.next().unwrap_or(""),
            );
            let n: usize = n
                .parse()
                .map_err(|_| format!("bundle: bad length in `{line}`"))?;
            let w = bundle.get(i..i + n).ok_or("bundle: witness past the end")?;
            if p == property && f == "(program)" {
                return String::from_utf8(w.to_vec()).map_err(|e| e.to_string());
            }
            i += n + 1;
        }
    }
    Ok(String::new())
}

/// Every record of `property` in the bundle, as `(function, bytes)`.
pub fn bundle_records(bundle: &[u8], property: &str) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    let mut i = bundle.iter().position(|&b| b == b'\n').ok_or("bundle: no header")? + 1;
    while i < bundle.len() {
        let j = i + bundle[i..].iter().position(|&b| b == b'\n').ok_or("bundle: unterminated line")?;
        let line = std::str::from_utf8(&bundle[i..j]).map_err(|e| e.to_string())?;
        i = j + 1;
        if let Some(rest) = line.strip_prefix("witness ") {
            let mut parts = rest.splitn(3, ' ');
            let (p, n, f) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""), parts.next().unwrap_or(""));
            let n: usize = n.parse().map_err(|_| format!("bundle: bad length in `{line}`"))?;
            let w = bundle.get(i..i + n).ok_or("bundle: witness past the end")?;
            if p == property {
                out.push((f.to_string(), String::from_utf8(w.to_vec()).map_err(|e| e.to_string())?));
            }
            i += n + 1;
        }
    }
    Ok(out)
}

/// The whole check: the shipped bytes, the bundle's two witnesses and the
/// anchor file (whose `alloc` is a function index, as a `call` names it).
pub fn check(wasm: &[u8], bundle: &[u8], anchors: &str) -> Result<Report, String> {
    let bodies = decode(wasm)?;
    let (alloc, ia, ba, judges, holds, rc_lines) = parse_anchors_held(anchors)?;
    let mut rep = Report::default();
    check_init(
        &bodies,
        alloc,
        &bundle_witness(bundle, "init")?,
        &ia,
        &mut rep,
    )?;
    check_bounds_alloc(&bodies, &bundle_witness(bundle, "bounds")?, &ba, Sites { judges: &judges, holds: &holds }, alloc, &mut rep)?;
    check_fields(&bodies, function_imports(wasm), alloc, &bundle_witness(bundle, "fields")?, &fields_anchors(anchors), &mut rep)?;
    if let Some((inc, decs, frames, arms, routines, helpers)) = rc_lines {
        let mut r = RcReport::default();
        let sigs = helper_sigs(bundle, function_imports(wasm), &frames, &helpers)?;
        check_rc(&bodies, function_imports(wasm), RcFns { inc, releases: &decs, sigs: &sigs, judges: &judges }, &frames, &[arms, routines], &mut r)?;
        rep.rc = Some(r);
    }
    Ok(rep)
}

/// The wall's per-artifact time limit (seconds) for `check`.
pub const DEADLINE_SECS: u64 = 60;

/// `check` held to a deadline. A check that has not finished when the time
/// runs out is a REJECT, never a pass and never a skip: an artifact whose
/// witness could not be matched in time is one the wall has not certified.
pub fn check_within(wasm: Vec<u8>, bundle: Vec<u8>, anchors: String, limit: std::time::Duration) -> Result<Report, String> {
    deadline(limit, move || check(&wasm, &bundle, &anchors))
}

/// A verdict held to `limit`: one not reached in time is an `Err`.
pub fn deadline<T: Send + 'static>(limit: std::time::Duration, f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    within(limit, f).unwrap_or_else(|| {
        Err(format!("the check did not finish within its {} s deadline; a timeout is a reject", limit.as_secs_f64()))
    })
}

/// Runs `f` on its own thread; `None` when it has not returned within `limit`.
pub fn within<T: Send + 'static>(limit: std::time::Duration, f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(limit).ok()
}
