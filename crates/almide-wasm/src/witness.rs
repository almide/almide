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

use std::collections::HashMap;
use std::sync::Mutex;

use crate::witness_paths::{Branches, Ev};


pub struct WitnessRecorder {
    next_obj: u32,
    obj_of_local: HashMap<u32, u32>,
    /// Every event in emission order, with the branch structure it was
    /// emitted under (#2756); rendered per object by witness_paths.rs.
    log: Vec<Ev>,
    /// The open branch sites and what is dead code right now.
    branches: Branches,
    /// A hook saw an event it could not attribute — the gate and the
    /// hooks disagree. The certificate becomes the loud `!poison`
    /// sentinel the floor test FAILS on, never a silent under-count.
    poisoned: bool,
    /// An EMISSION-TIME decline (#1696 step 4): the gate admitted the
    /// body's shape, but the route it took has an RC site this phase does
    /// not record (a View result, a native arm that bypasses `lower_arg`).
    /// The certificate becomes `!decline:<reason>` — counted by the
    /// histogram, neither a certificate nor a poison.
    declined: Option<String>,
    /// The argument expressions a hook fired for, by node address, in
    /// order (step 4): the module-call wrapper audits an arm by asking
    /// whether EACH of its own argument nodes went through a hook. A count
    /// stopped being enough once nested arguments were admitted (#2755):
    /// an inner call's argument hooks fire inside the outer arm's window,
    /// so an arm that lowered its argument outside `lower_arg` could still
    /// have matched the count.
    hooked: Vec<usize>,
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
            log: Vec::new(),
            branches: Branches::default(),
            poisoned: false,
            declined: None,
            hooked: Vec::new(),
        }
    }

    /// One argument hook fired for the node at `node` (any convention,
    /// droppable or not).
    pub fn note_arg(&mut self, node: usize) {
        self.hooked.push(node);
    }

    /// The audit window's start: hooks fired so far.
    pub fn arg_hooks(&self) -> usize {
        self.hooked.len()
    }

    /// Did a hook fire for `node` since the window opened at `since`?
    /// Every argument node is alive (borrowed from the IR) for the whole
    /// window, so its address cannot be reused by a clone an arm made.
    pub fn hooked_since(&self, since: usize, node: usize) -> bool {
        self.hooked.get(since..).is_some_and(|w| w.contains(&node))
    }

    /// An owned droppable call result discarded in statement position:
    /// born (the callee's handed-over credit) and released by the route.
    pub fn temp_discarded(&mut self) {
        self.temp_borrowed();
    }

    /// A new object: logged as born here unless this is dead code.
    fn new_obj(&mut self) -> u32 {
        let o = self.next_obj;
        self.next_obj += 1;
        if !self.branches.dead() {
            self.log.push(Ev::Birth(o));
        }
        o
    }

    fn fresh_obj(&mut self, local: u32) -> u32 {
        let o = self.new_obj();
        self.obj_of_local.insert(local, o);
        o
    }

    /// Record `ops` on object `o`, unless this is dead code (after an exit
    /// on this path — the instructions are emitted but never run).
    fn ops(&mut self, o: u32, ops: &str) {
        if !self.branches.dead() {
            self.log.extend(ops.chars().map(|c| Ev::Op(o, c)));
        }
    }

    fn held_ops(&mut self, local: u32, ops: &str) -> bool {
        let Some(&o) = self.obj_of_local.get(&local) else { return false };
        self.ops(o, ops);
        true
    }

    /// A droppable param: callee-owned (+1 pre-paid by the call site's
    /// rc_arg_guard) — the object is born owned in this frame.
    pub fn param_owned(&mut self, local: u32) {
        let o = self.fresh_obj(local);
        self.ops(o, "i");
    }

    /// A droppable param this frame only BORROWS (param_borrow.rs, #2028):
    /// the object is known, no credit of it is held here — a share or a
    /// ret-move on it balances against nothing this frame owns. A pattern
    /// bind (a view of the subject's payload, #2756) is the same.
    pub fn param_borrowed(&mut self, local: u32) {
        self.fresh_obj(local);
    }

    /// A fresh temporary lent to a borrowed param: born at the site,
    /// released by the site right after the call.
    pub fn temp_borrowed(&mut self) {
        let o = self.new_obj();
        self.ops(o, "id");
    }

    /// Bind of a certainly-fresh rhs (heap literal, block copy): a new
    /// object, one ownership.
    pub fn bind_fresh(&mut self, local: u32) {
        let o = self.fresh_obj(local);
        self.ops(o, "i");
    }

    /// Bind of a borrowed Var rhs: the SOURCE local's object gains a
    /// share (`rc_inc_top` at the bind), and the new local aliases it.
    pub fn bind_alias(&mut self, local: u32, src_local: u32) -> bool {
        let Some(&o) = self.obj_of_local.get(&src_local) else { return false };
        self.obj_of_local.insert(local, o);
        self.ops(o, "a");
        true
    }

    /// The heap return of a bound Var: the ret-inc instruction is the
    /// share (`a`), and the value leaving the frame is the move-out
    /// (`m`) — together the transfer of one credit to the caller.
    pub fn ret_move(&mut self, local: u32) -> bool {
        self.held_ops(local, "am")
    }

    /// A real `$dec_flat` on the local's object (epilogue / dec-old). In
    /// dead code (after a frame replacement on this path) it is attributed
    /// (the local is known) but not recorded.
    pub fn dec_local(&mut self, local: u32) -> bool {
        self.held_ops(local, "d")
    }

    /// A frame-ending edge (a `return_call`) finished its releases: nothing
    /// emitted after it on this path executes (#2756: inside an arm, only
    /// that arm is over).
    pub fn frame_replaced(&mut self) {
        self.branches.exit(&mut self.log);
    }

    /// A branch site opens (`if` / `match`, #2756).
    pub fn branch_open(&mut self) {
        self.branches.open(&mut self.log);
    }

    /// The next arm of the innermost open site begins.
    pub fn branch_arm(&mut self) {
        self.branches.arm(&mut self.log);
    }

    /// The innermost open site joins.
    pub fn branch_close(&mut self) {
        self.branches.close(&mut self.log);
    }

    /// A local's credit moves without a share (#2757).
    pub fn move_local(&mut self, local: u32) -> bool {
        self.held_ops(local, "m")
    }

    /// A droppable Var argument at a call site: the site's `rc_inc` is
    /// the share (`a`), and the credit moves into the callee (`m`).
    pub fn arg_share_move(&mut self, local: u32) -> bool {
        self.held_ops(local, "am")
    }

    /// A fresh temporary handed to a callee: born here, consumed there.
    pub fn temp_move(&mut self) {
        let o = self.new_obj();
        self.ops(o, "im");
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
        if !self.branches.settled() {
            return "!poison\n".to_string();
        }
        match crate::witness_paths::render(&self.log, self.next_obj) {
            Ok(s) => s,
            Err(r) => format!("{DECLINE_PREFIX}{r}\n"),
        }
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

// The subset gate lives in witness_gate.rs (the file budget).
pub use crate::witness_gate::straightline_subset;

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

/// Every frame the sweep collected, over EVERY emission pass (the pass
/// markers are stripped).
pub fn take() -> Vec<(String, String)> {
    take_with_shipped().0
}

/// The sink's pass boundaries (#2754). `emit_program` emits in up to three
/// passes — the first over the WHOLE linked registry graph, the next over
/// the reachable set, a third when the bounded-line rewrites fired — and
/// ships one of them. A frame only an earlier pass emitted (a dead linked
/// stdlib body) is not in the artifact, so a per-program verdict reads the
/// SHIPPED pass. The markers use a name no function can spell.
const PASS_MARK: &str = "\u{0}pass";
const SHIPPED_MARK: &str = "\u{0}shipped";

/// Both markers go through `push`, a no-op unless a sweep collects.
pub(crate) fn mark_pass(pass: usize) {
    push(PASS_MARK, pass.to_string());
}

pub(crate) fn mark_shipped(pass: usize) {
    push(SHIPPED_MARK, pass.to_string());
}

/// `(function name, certificate)` pairs, in emission order.
pub type Frames = Vec<(String, String)>;

/// `(every frame of every pass, the frames of the pass that shipped)`. The
/// second is empty when no pass was marked as shipped (a refused program).
pub fn take_with_shipped() -> (Frames, Frames) {
    let (all, shipped) = take_by_pass();
    (all.into_iter().map(|(_, n, c)| (n, c)).collect(), shipped)
}

/// The pass `emit_program` emits WITHOUT the bounded-line rewrites (#2312,
/// `line_bounded.rs`): its code differs from passes 1 and 2 by design (a
/// `println(int.to_string(x))` builds and releases a block there instead of
/// printing from the itoa scratch), so a sweep holds only the passes of one
/// configuration to agreeing certificates.
pub const CHECKED_PASS: usize = 3;

/// `(every frame of every pass WITH its pass number, the frames of the pass
/// that shipped)`. Pass numbers are those `emit_program` marks (1, 2,
/// [`CHECKED_PASS`]); a frame pushed before any marker reads as pass 0.
pub fn take_by_pass() -> (Vec<(usize, String, String)>, Frames) {
    let raw = sink().lock().expect("witness sink").take().unwrap_or_default();
    let shipped = raw.iter().rev().find(|(n, _)| n == SHIPPED_MARK).map(|(_, p)| p.clone());
    let mut all = Vec::new();
    let mut in_shipped = Vec::new();
    let mut current: Option<String> = None;
    for (name, cert) in raw {
        if name == PASS_MARK {
            current = Some(cert);
        } else if name == SHIPPED_MARK {
        } else {
            if shipped.is_some() && current == shipped {
                in_shipped.push((name.clone(), cert.clone()));
            }
            let pass = current.as_deref().and_then(|p| p.parse().ok()).unwrap_or(0);
            all.push((pass, name, cert));
        }
    }
    (all, in_shipped)
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

/// A frame the emitter builds WITHOUT a recorder — a lifted lambda, a
/// display / equality / scan helper — counted as a decline while a sweep
/// collects (#2754), so a program whose such frames went unrecorded never
/// reads as fully certified. A no-op otherwise.
pub(crate) fn decline_unrecorded(name: &str, reason: &str) {
    if collecting() {
        push_decline(name, reason);
    }
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
