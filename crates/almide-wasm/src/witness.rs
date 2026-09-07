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
//! PHASE B1 (the call boundary, still certificate v0 — #1696): a Bind
//! whose rhs is a call to a user fn over Var / literal arguments, and a
//! tail that is such a call or a fresh literal, are admitted. The
//! structural convention is CALLEE-OWNED: a droppable Var argument takes a
//! real `rc_inc` at the site (`a`) and its credit leaves the frame into
//! the callee (`m`); a fresh temporary argument is born (`i`) and leaves
//! (`m`); the callee hands back exactly one credit with a droppable
//! result (#1986), which the bind receives as a new object (`i`). A tail
//! call / fresh tail moves its one credit out (`im`). A `return_call`
//! site releases the owned params before the jump and records each `d`.
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
    /// A `return_call` replaced the frame: the releases it emitted are the
    /// frame's last events, and the fall-through epilogue the emitter still
    /// writes after the jump is dead code — its decs are not recorded.
    frame_replaced: bool,
}

impl Default for WitnessRecorder {
    fn default() -> Self {
        Self::new()
    }
}

impl WitnessRecorder {
    pub fn new() -> Self {
        Self {
            next_obj: 0,
            obj_of_local: HashMap::new(),
            streams: BTreeMap::new(),
            poisoned: false,
            frame_replaced: false,
        }
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

    /// A real `$dec_flat` on the local's object (epilogue / dec-old). After
    /// a frame replacement the epilogue's decs are dead code: attributed
    /// (the local is known) but not recorded.
    pub fn dec_local(&mut self, local: u32) -> bool {
        let Some(&o) = self.obj_of_local.get(&local) else { return false };
        if !self.frame_replaced {
            self.streams.entry(o).or_default().push('d');
        }
        true
    }

    /// The `return_call` site finished its releases: nothing emitted after
    /// this executes.
    pub fn frame_replaced(&mut self) {
        self.frame_replaced = true;
    }

    /// A droppable Var argument at a call site: the site's `rc_inc` is
    /// the share (`a`), and the credit moves into the callee (`m`).
    pub fn arg_share_move(&mut self, local: u32) -> bool {
        let Some(&o) = self.obj_of_local.get(&local) else { return false };
        let st = self.streams.entry(o).or_default();
        st.push('a');
        st.push('m');
        true
    }

    /// A fresh temporary handed to a callee: born here, consumed there.
    pub fn temp_move(&mut self) {
        let o = self.next_obj;
        self.next_obj += 1;
        self.streams.entry(o).or_default().push_str("im");
    }

    /// An owned tail value (a call result or a fresh literal) leaving the
    /// frame as the return: one credit received, one credit moved out.
    pub fn tail_owned_move(&mut self) {
        self.temp_move();
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

/// The phase-A/B1 subset gate: `None` = the body is straight-line and
/// every RC-affecting site is covered by the recorder hooks (bind,
/// call-argument, tail, epilogue / tail-release); `Some(reason)` = out
/// of subset, do not record. Deliberately conservative — admitting a
/// shape here without auditing its RC sites would let the witness
/// under-count real events, which is the one dishonesty the recorder
/// exists to rule out.
pub fn straightline_subset(body: &IrExpr, ret_is_heap: bool, self_name: &str) -> Option<String> {
    // `fn f(x) = expr` lowers exactly like `{ expr }`: a bare body is the
    // empty-statement block with that tail (B1: the tail-call and
    // literal-tail fns are almost all written this way).
    let bare: Option<Box<IrExpr>>;
    let (stmts, expr): (&[almide_ir::IrStmt], &Option<Box<IrExpr>>) = match &body.kind {
        IrExprKind::Block { stmts, expr } => (stmts, expr),
        _ => {
            bare = Some(Box::new(body.clone()));
            (&[], &bare)
        }
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
        // B1: an owned tail — a user-fn call over Var/literal args (the
        // call-arg hook covers its sites, the result moves out) or a
        // fresh literal (its alloc IS the credit that moves out).
        // A SELF tail call is loop-converted (tco.rs): the frame is not
        // replaced, the params are rebound by the loop-back and released
        // again by the epilogue — a loop, not a straight line. Out of
        // subset (the recorder is not loop-aware).
        Some(IrExprKind::Call { target: almide_ir::CallTarget::Named { name }, .. })
            if name.as_str() == self_name =>
        {
            Some("tail:self-call-loop".into())
        }
        Some(IrExprKind::Call { .. }) => expr.as_deref().and_then(user_call_subset),
        Some(k @ (IrExprKind::LitStr { .. } | IrExprKind::List { .. })) if ret_is_heap => {
            subset_rhs_literal(k)
        }
        other => Some(format!("tail:{other:?}").chars().take(40).collect()),
    }
}

/// A call the B1 hooks cover: a Named user fn (lowercase — ctors are
/// capitalized, the builtin `some`/`ok`/`err` are IR kinds, not calls)
/// over Var / literal arguments only. Module helpers have their own
/// borrow conventions and stay out.
fn user_call_subset(e: &IrExpr) -> Option<String> {
    let IrExprKind::Call { target, args, .. } = &e.kind else {
        return Some("call:not-a-call".into());
    };
    let almide_ir::CallTarget::Named { name } = target else {
        return Some("call:not-named".into());
    };
    if !name.as_str().starts_with(|c: char| c.is_ascii_lowercase() || c == '_') {
        return Some("call:ctor".into());
    }
    for a in args {
        match &a.kind {
            IrExprKind::Var { .. }
            | IrExprKind::LitInt { .. }
            | IrExprKind::LitFloat { .. }
            | IrExprKind::LitBool { .. }
            | IrExprKind::LitStr { .. } => {}
            IrExprKind::List { .. } => {
                if let Some(r) = subset_rhs_literal(&a.kind) {
                    return Some(r);
                }
            }
            other => return Some(format!("call-arg:{other:?}").chars().take(40).collect()),
        }
    }
    None
}

fn subset_rhs_literal(k: &IrExprKind) -> Option<String> {
    match k {
        IrExprKind::LitStr { .. } => None,
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

fn subset_rhs(value: &IrExpr) -> Option<String> {
    match &value.kind {
        IrExprKind::LitInt { .. }
        | IrExprKind::LitFloat { .. }
        | IrExprKind::LitBool { .. }
        | IrExprKind::LitStr { .. }
        | IrExprKind::Var { .. } => None,
        IrExprKind::List { .. } => subset_rhs_literal(&value.kind),
        // B1: a user-fn call — its arguments' RC sites are the call-arg
        // hook's, its droppable result is a received credit (#1986).
        IrExprKind::Call { .. } => user_call_subset(value),
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
    pub(crate) fn witness_bind(
        &mut self,
        idx: u32,
        declared: SliceTy,
        value: &almide_ir::IrExpr,
        seq0: u32,
    ) {
        let src_local = if let almide_ir::IrExprKind::Var { id } = &value.kind {
            self.locals.get(id).map(|&(l, _)| l)
        } else {
            None
        };
        // Mirrors the route exactly: an OWNED result (fresh, or a user-fn
        // call's handed-over credit, #1986) is a new object; a Map/Set Var
        // took `$block_copy`.
        let owned = self.rc_owned_result(value, seq0);
        let Some(w) = self.witness.as_mut() else { return };
        if owned || (src_local.is_some() && matches!(declared, SliceTy::Map(..) | SliceTy::Set(_))) {
            w.bind_fresh(idx);
            return;
        }
        match src_local {
            Some(src) if w.bind_alias(idx, src) => {}
            _ => w.poison(),
        }
    }

    /// The call-argument hook (calls.rs, right after `rc_arg_guard`):
    /// a droppable Var argument's object gained a real `rc_inc` and its
    /// credit moves into the callee; a fresh temporary is born and moves.
    /// Non-droppable arguments have no RC site. Anything else under an
    /// armed recorder is a gate/hook disagreement — poison.
    pub(crate) fn witness_arg(&mut self, e: &almide_ir::IrExpr, ty: SliceTy, seq0: u32) {
        if self.witness.is_none() || !self.rc_droppable(ty) {
            return;
        }
        let src_local = if let almide_ir::IrExprKind::Var { id } = &e.kind {
            self.locals.get(id).map(|&(l, _)| l)
        } else {
            None
        };
        // Mirrors rc_arg_guard exactly: an OWNED argument (fresh literal
        // or a call result carrying its one credit) is born and moves
        // (`im`); a Var shares and moves (`am`).
        let fresh = self.rc_owned_result(e, seq0);
        let Some(w) = self.witness.as_mut() else { return };
        match src_local {
            Some(l) if w.arg_share_move(l) => {}
            None if fresh => w.temp_move(),
            _ => w.poison(),
        }
    }

    /// The owned-tail hook (func.rs): a droppable tail that needs no
    /// ret-inc — a user-fn call result or a fresh literal — moves its
    /// one credit out of the frame.
    pub(crate) fn witness_tail_owned(&mut self) {
        if let Some(w) = self.witness.as_mut() {
            w.tail_owned_move();
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
    fn a_var_argument_shares_then_moves_into_the_callee() {
        // let a = [1]; f(a) — the site's rc_inc + the credit's move.
        let mut w = WitnessRecorder::new();
        w.bind_fresh(3);
        assert!(w.arg_share_move(3));
        assert!(w.dec_local(3));
        assert_eq!(w.certificate(), "iamd\n");
        assert!(balanced(&w.certificate()));
    }

    #[test]
    fn a_temporary_argument_and_an_owned_tail_each_move_one_credit() {
        let mut w = WitnessRecorder::new();
        w.temp_move();
        w.tail_owned_move();
        assert_eq!(w.certificate(), "im\nim\n");
        assert!(balanced(&w.certificate()));
    }

    #[test]
    fn an_over_release_fails_the_balance_mirror() {
        assert!(!balanced("idd\n"));
        assert!(!balanced("ia\n"));
        assert!(balanced("iadd\nid\n"));
    }
}
