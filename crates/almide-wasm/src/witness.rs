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
//! structural convention is CALLEE-OWNED for a param the callee consumes
//! (param_borrow.rs, #2028 — a param it only reads is BORROWED: a Var
//! argument records nothing, a fresh temporary is born and released by
//! the site, `id`): a droppable Var argument takes a real `rc_inc` at the
//! site (`a`) and its credit leaves the frame into the callee (`m`); a
//! fresh temporary argument is born (`i`) and leaves (`m`); the callee
//! hands back exactly one credit with a droppable
//! result (#1986), which the bind receives as a new object (`i`). A tail
//! call / fresh tail moves its one credit out (`im`). A `return_call`
//! site releases the owned params before the jump and records each `d`.
//!
//! STEP 4 (#1696, statement calls + module calls): a statement-position
//! call over Var / literal args is admitted — its argument sites are the
//! call hooks', and an OWNED droppable result is released by the discard
//! route right there (`id`). A `CallTarget::Module` call is admitted in
//! every call position: the registry route consults the callee's
//! param_owned table exactly as the Named route does, and a native arm's
//! declared `ArgMode` is recorded at `lower_arg` (witness_hooks.rs). What
//! the arms do NOT yet cover DECLINES at emission time with a counted
//! reason (`!decline:…`, the measurement channel this step opened):
//! an arm that lowers an argument outside `lower_arg`, a droppable View
//! result, a Retain of a flat / cell local. The frame's certificate is
//! withdrawn, never under-recorded.
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
    /// An EMISSION-TIME decline (#1696 step 4): the gate admitted the
    /// body's shape, but the route it took has an RC site this phase does
    /// not record (a View result, a native arm that bypasses `lower_arg`).
    /// The certificate becomes `!decline:<reason>` — counted by the
    /// histogram, neither a certificate nor a poison.
    declined: Option<String>,
    /// Argument hooks fired so far (step 4): the module-call wrapper
    /// audits an arm by this count against its argument count.
    arg_hooks: u32,
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
            declined: None,
            arg_hooks: 0,
        }
    }

    /// One argument hook fired (any convention, droppable or not).
    pub fn note_arg(&mut self) {
        self.arg_hooks += 1;
    }

    pub fn arg_hooks(&self) -> u32 {
        self.arg_hooks
    }

    /// An owned droppable call result discarded in statement position:
    /// born (the callee's handed-over credit) and released by the route.
    pub fn temp_discarded(&mut self) {
        self.temp_borrowed();
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

    /// A droppable param this frame only BORROWS (param_borrow.rs, #2028):
    /// the object is known, no credit of it is held here — a share or a
    /// ret-move on it balances against nothing this frame owns.
    pub fn param_borrowed(&mut self, local: u32) {
        let o = self.fresh_obj(local);
        self.streams.entry(o).or_default();
    }

    /// A fresh temporary lent to a borrowed param: born at the site,
    /// released by the site right after the call.
    pub fn temp_borrowed(&mut self) {
        let o = self.next_obj;
        self.next_obj += 1;
        self.streams.entry(o).or_default().push_str("id");
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

    /// Withdraw this frame's certificate: the first reason wins (the
    /// shape that turned the recorder away is the one to count).
    pub fn decline(&mut self, reason: &str) {
        if self.declined.is_none() {
            self.declined = Some(reason.to_string());
        }
    }

    /// One line per object, in object order — certificate v0. A poison
    /// outranks a decline: a hook disagreement is a bug even in a frame
    /// that withdrew.
    pub fn certificate(&self) -> String {
        if self.poisoned {
            return "!poison\n".to_string();
        }
        if let Some(r) = &self.declined {
            return format!("{DECLINE_PREFIX}{r}\n");
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
            // Step 4: a statement-position call over Var / literal args —
            // its argument sites are the call hooks', and an owned
            // droppable result is released by the discard route (`id`).
            IrStmtKind::Expr { expr } if matches!(expr.kind, IrExprKind::Call { .. }) => {
                if let Some(r) = call_subset(expr) {
                    return Some(format!("stmt:Expr:{r}"));
                }
            }
            IrStmtKind::Expr { expr } => return Some(format!("stmt:Expr:{}", expr_tag(expr))),
            other => return Some(format!("stmt:{}", tag(other))),
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
        Some(IrExprKind::Call { .. }) => expr.as_deref().and_then(call_subset),
        Some(k @ (IrExprKind::LitStr { .. } | IrExprKind::List { .. })) if ret_is_heap => {
            subset_rhs_literal(k)
        }
        Some(other) => Some(format!("tail:{}", tag(other))),
        None => Some("tail:Unit-heap".into()),
    }
}

/// A space-free tag of an IR node's variant, for the decline histogram
/// (`grep -o '^!decline:[^ ]*' | sort | uniq -c` over the floor dump).
fn tag<T: std::fmt::Debug>(v: &T) -> String {
    format!("{v:?}").chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect()
}

/// A statement-position expression's tag, one level deeper for a call
/// (its target family is what the next increment chooses by).
fn expr_tag(e: &IrExpr) -> String {
    match &e.kind {
        IrExprKind::Call { target, .. } => format!("Call:{}", tag(target)),
        other => tag(other),
    }
}

/// A call the hooks cover: a Named user fn (lowercase — ctors are
/// capitalized, the builtin `some`/`ok`/`err` are IR kinds, not calls)
/// or, since step 4, a Module call (the native arms' declared modes are
/// recorded at `lower_arg`; the registry route consults the callee's
/// param_owned table like the Named route), over Var / literal
/// arguments only.
fn call_subset(e: &IrExpr) -> Option<String> {
    let IrExprKind::Call { target, args, .. } = &e.kind else {
        return Some("call:not-a-call".into());
    };
    match target {
        almide_ir::CallTarget::Named { name } => {
            if !name.as_str().starts_with(|c: char| c.is_ascii_lowercase() || c == '_') {
                return Some("call:ctor".into());
            }
            // The http_framed host-op leaves (calls.rs) intercept before
            // resolution and lower their args outside every hook.
            if name.as_str().starts_with("__http_framed_") {
                return Some("call:host-splice".into());
            }
        }
        almide_ir::CallTarget::Module { .. } => {}
        other => return Some(format!("call:target:{}", tag(other))),
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
            other => return Some(format!("call-arg:{}", tag(other))),
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
        other => Some(format!("rhs:{}", tag(other))),
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
        IrExprKind::Call { .. } => call_subset(value),
        other => Some(format!("rhs:{}", tag(other))),
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

/// The certificate prefix of a DECLINED frame — the measurement channel
/// (#1696 step 4): `!decline:<reason>`, one space-free reason tag. The
/// floor test counts these and fails on neither; only `!poison` fails.
pub const DECLINE_PREFIX: &str = "!decline:";

/// A frame the pre-gate or the straightline gate turned away, while the
/// sweep collects: its reason goes to the sink so the corpus histogram
/// names the next shape to admit.
pub(crate) fn push_decline(name: &str, reason: &str) {
    push(name, format!("{DECLINE_PREFIX}{reason}\n"));
}

// The Emitter-side hooks live in witness_hooks.rs.

use crate::{Scalar, SliceTy};

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

    #[test]
    fn a_decline_withdraws_the_certificate_and_a_poison_outranks_it() {
        let mut w = WitnessRecorder::new();
        w.bind_fresh(3);
        w.decline("module-result:view");
        w.decline("second-reason-loses");
        assert_eq!(w.certificate(), "!decline:module-result:view\n");
        w.poison();
        assert_eq!(w.certificate(), "!poison\n");
    }
}
