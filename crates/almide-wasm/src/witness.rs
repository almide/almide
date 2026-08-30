//! Structural-leg ownership witness — #1696 step 4 phase A.
//!
//! The incumbent certifies memory-safety by projecting MIR ownership ops to
//! a per-object refcount-event stream the kernel-proven checker re-verifies
//! (crates/almide-mir/src/certificate.rs). The structural leg emits no MIR —
//! its ownership discipline lives in the Emitter's RC-3 routes — so its
//! witness is recorded AT EMISSION TIME, one event per RC-affecting
//! instruction actually emitted: the witness describes the emitted
//! instruction stream itself, one contract level closer to the bytes than
//! the incumbent's MIR-side projection.
//!
//! PHASE A SUBSET (the honest wall): recording engages only for a function
//! whose body the pre-scan proves STRAIGHT-LINE — a Block of Binds whose
//! rhs is a fresh heap/scalar literal or a plain Var alias, with a Unit or
//! scalar tail. In that subset a wasm local binds exactly once, so the
//! local-to-object map below is a faithful object identity, and the ONLY
//! RC-affecting sites are the Bind route and the fall-through epilogue —
//! exactly the two hooks in stmts.rs/func.rs. Everything else declines
//! (`None` from the gate), never records, never overclaims. Branches,
//! loops, calls and heap returns are phases B/C.
//!
//! Event vocabulary (certificate v0, the format `proofs/` checks):
//!   `i` = an ownership +1 backed by a real Alloc/copy (a fresh bind, or a
//!         droppable param — the structural convention is CALLEE-OWNED:
//!         the call site's rc_arg_guard pre-paid the +1 this records);
//!   `a` = a +1 backed by a real `rc_inc` (the borrowed-rhs bind share);
//!   `d` = a −1 backed by a real `$dec_flat` (epilogue release, dec-old).
//! Balance (every prefix nonnegative, every stream ending at zero) is
//! re-checked here by `balanced` — the Rust mirror of the proven rule —
//! and the certificate text is byte-compatible with the extracted checker
//! for the gate.sh hookup (phase A2).

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

use almide_ir::{IrExpr, IrExprKind, IrStmtKind};

pub struct WitnessRecorder {
    next_obj: u32,
    obj_of_local: HashMap<u32, u32>,
    streams: BTreeMap<u32, String>,
    /// A hook saw an event it could not attribute — the gate and the
    /// hooks disagree. The certificate becomes the loud `!poison`
    /// sentinel the floor test FAILS on, never a silent under-count.
    poisoned: bool,
}

impl Default for WitnessRecorder {
    fn default() -> Self {
        Self::new()
    }
}

impl WitnessRecorder {
    pub fn new() -> Self {
        Self { next_obj: 0, obj_of_local: HashMap::new(), streams: BTreeMap::new(), poisoned: false }
    }

    fn fresh_obj(&mut self, local: u32) -> u32 {
        let o = self.next_obj;
        self.next_obj += 1;
        self.obj_of_local.insert(local, o);
        o
    }

    /// A droppable param: callee-owned (+1 pre-paid by the call site's
    /// rc_arg_guard) — the object is born owned in this frame.
    pub fn param_owned(&mut self, local: u32) {
        let o = self.fresh_obj(local);
        self.streams.entry(o).or_default().push('i');
    }

    /// Bind of a certainly-fresh rhs (heap literal, block copy): a new
    /// object, one ownership.
    pub fn bind_fresh(&mut self, local: u32) {
        let o = self.fresh_obj(local);
        self.streams.entry(o).or_default().push('i');
    }

    /// Bind of a borrowed Var rhs: the SOURCE local's object gains a
    /// share (`rc_inc_top` at the bind), and the new local aliases it.
    pub fn bind_alias(&mut self, local: u32, src_local: u32) -> bool {
        let Some(&o) = self.obj_of_local.get(&src_local) else { return false };
        self.obj_of_local.insert(local, o);
        self.streams.entry(o).or_default().push('a');
        true
    }

    /// The heap return of a bound Var: the ret-inc instruction is the
    /// share (`a`), and the value leaving the frame is the move-out
    /// (`m`) — together the transfer of one credit to the caller.
    pub fn ret_move(&mut self, local: u32) -> bool {
        let Some(&o) = self.obj_of_local.get(&local) else { return false };
        let st = self.streams.entry(o).or_default();
        st.push('a');
        st.push('m');
        true
    }

    /// A real `$dec_flat` on the local's object (epilogue / dec-old).
    pub fn dec_local(&mut self, local: u32) -> bool {
        let Some(&o) = self.obj_of_local.get(&local) else { return false };
        self.streams.entry(o).or_default().push('d');
        true
    }

    pub fn poison(&mut self) {
        self.poisoned = true;
    }

    /// One line per object, in object order — certificate v0.
    pub fn certificate(&self) -> String {
        if self.poisoned {
            return "!poison\n".to_string();
        }
        let mut s = String::new();
        for stream in self.streams.values() {
            s.push_str(stream);
            s.push('\n');
        }
        s
    }
}

/// The proven balance rule, mirrored: per stream, `i`/`a` = +1, `d`/`m` =
/// −1, every prefix nonnegative (no release at rc 0), final balance zero
/// (no leak). Arm braces are phase-B vocabulary — their presence here is
/// out of subset and fails.
pub fn balanced(cert: &str) -> bool {
    for line in cert.lines() {
        let mut bal: i64 = 0;
        for c in line.chars() {
            match c {
                'i' | 'a' => bal += 1,
                'd' | 'm' => bal -= 1,
                _ => return false,
            }
            if bal < 0 {
                return false;
            }
        }
        if bal != 0 {
            return false;
        }
    }
    true
}

/// The phase-A subset gate: `None` = the body is straight-line and every
/// RC-affecting site is covered by the two recorder hooks; `Some(reason)`
/// = out of subset, do not record. Deliberately conservative — admitting
/// a shape here without auditing its RC sites would let the witness
/// under-count real events, which is the one dishonesty the recorder
/// exists to rule out.
pub fn straightline_subset(body: &IrExpr, ret_is_heap: bool) -> Option<String> {
    let IrExprKind::Block { stmts, expr } = &body.kind else {
        return Some("non-block-body".into());
    };
    for s in stmts {
        match &s.kind {
            IrStmtKind::Bind { value, .. } => {
                if let Some(r) = subset_rhs(value) {
                    return Some(r);
                }
            }
            other => return Some(format!("stmt:{other:?}").chars().take(40).collect()),
        }
    }
    match expr.as_deref().map(|t| &t.kind) {
        // A heap return is admitted only as a plain bound Var (the
        // ret-inc + move-out pair the func.rs hook records); any other
        // heap tail has unrecorded RC sites.
        None | Some(IrExprKind::Unit) if !ret_is_heap => None,
        Some(IrExprKind::Var { .. }) => None,
        Some(IrExprKind::LitInt { .. } | IrExprKind::LitBool { .. } | IrExprKind::LitFloat { .. })
            if !ret_is_heap =>
        {
            None
        }
        other => Some(format!("tail:{other:?}").chars().take(40).collect()),
    }
}

fn subset_rhs(value: &IrExpr) -> Option<String> {
    match &value.kind {
        IrExprKind::LitInt { .. }
        | IrExprKind::LitFloat { .. }
        | IrExprKind::LitBool { .. }
        | IrExprKind::LitStr { .. }
        | IrExprKind::Var { .. } => None,
        IrExprKind::List { elements } => {
            for e in elements {
                if !matches!(
                    e.kind,
                    IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitBool { .. } | IrExprKind::LitStr { .. }
                ) {
                    return Some("list-elem".into());
                }
            }
            None
        }
        other => Some(format!("rhs:{other:?}").chars().take(40).collect()),
    }
}

// ── the collection sink (diagnostic channel, test-enabled) ──────────────

/// (function name, certificate) pairs collected while a sweep runs.
/// `start_collecting` arms it; `take` disarms and returns the batch. The
/// emitter pushes only while armed, so product builds never pay.
type Sink = Mutex<Option<Vec<(String, String)>>>;

fn sink() -> &'static Sink {
    use std::sync::OnceLock;
    static SINK: OnceLock<Sink> = OnceLock::new();
    SINK.get_or_init(|| Mutex::new(None))
}

pub fn start_collecting() {
    *sink().lock().expect("witness sink") = Some(Vec::new());
}

pub fn take() -> Vec<(String, String)> {
    sink().lock().expect("witness sink").take().unwrap_or_default()
}

pub(crate) fn collecting() -> bool {
    sink().lock().expect("witness sink").is_some()
}

pub(crate) fn push(name: &str, cert: String) {
    if let Some(v) = sink().lock().expect("witness sink").as_mut() {
        v.push((name.to_string(), cert));
    }
}

// ── the Emitter-side hooks ──────────────────────────────────────────────

use crate::emitter::Emitter;
use crate::{Scalar, SliceTy};

impl Emitter<'_> {
    /// The Bind-route hook (stmts.rs): called right after the local joins
    /// `rc_owned`. Attribution mirrors the instructions the route just
    /// emitted: a certainly-fresh rhs (heap literal) and a Map/Set Var rhs
    /// (which took `$block_copy`) are NEW objects; a List/Str/Bytes Var
    /// rhs took `rc_inc_top`, so the SOURCE object gains a share. Anything
    /// else under an armed recorder is a gate/hook disagreement — poison.
    pub(crate) fn witness_bind(&mut self, idx: u32, declared: SliceTy, value: &almide_ir::IrExpr) {
        let src_local = if let almide_ir::IrExprKind::Var { id } = &value.kind {
            self.locals.get(id).map(|&(l, _)| l)
        } else {
            None
        };
        let Some(w) = self.witness.as_mut() else { return };
        if crate::rc_ownership::rc_certainly_fresh(&value.kind)
            || (src_local.is_some() && matches!(declared, SliceTy::Map(..) | SliceTy::Set(_)))
        {
            w.bind_fresh(idx);
            return;
        }
        match src_local {
            Some(src) if w.bind_alias(idx, src) => {}
            _ => w.poison(),
        }
    }

    /// The epilogue hook (func.rs): one `d` per `$dec_flat` emitted.
    pub(crate) fn witness_dec(&mut self, idx: u32) {
        if let Some(w) = self.witness.as_mut()
            && !w.dec_local(idx)
        {
            w.poison();
        }
    }
}

/// Is the slice type outside the phase-A scalar/Unit return set?
pub(crate) fn heapish_ret(t: SliceTy) -> bool {
    !matches!(t, SliceTy::Unit | SliceTy::Scalar(Scalar::Int | Scalar::Float | Scalar::Bool))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_bind_and_its_epilogue_release_balance() {
        let mut w = WitnessRecorder::new();
        w.bind_fresh(3);
        assert!(w.dec_local(3));
        assert_eq!(w.certificate(), "id\n");
        assert!(balanced(&w.certificate()));
    }

    #[test]
    fn an_alias_bind_shares_the_source_object_and_both_release() {
        // let a = [1]; let b = a — one object, streams to the canonical
        // shared shape the incumbent's tests pin ("iadd").
        let mut w = WitnessRecorder::new();
        w.bind_fresh(3);
        assert!(w.bind_alias(4, 3));
        assert!(w.dec_local(3));
        assert!(w.dec_local(4));
        assert_eq!(w.certificate(), "iadd\n");
        assert!(balanced(&w.certificate()));
    }

    #[test]
    fn an_over_release_fails_the_balance_mirror() {
        assert!(!balanced("idd\n"));
        assert!(!balanced("ia\n"));
        assert!(balanced("iadd\nid\n"));
    }
}
