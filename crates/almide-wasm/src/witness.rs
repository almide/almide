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
            streams: BTreeMap::new(),
            poisoned: false,
            frame_replaced: false,
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
/// call-argument, store, tail, epilogue / tail-release); `Some(reason)` =
/// out of subset, do not record. Deliberately conservative — admitting a
/// shape here without auditing its RC sites would let the witness
/// under-count real events, which is the one dishonesty the recorder
/// exists to rule out.
///
/// #2755 (step 4, the flat alphabet on temporaries): the value forms are
/// one recursive predicate, [`value_subset`]. A nested call argument, a
/// binary operator, a constructor (`some` / `ok` / `err` / a variant
/// case) and a `{ let …; v }` block (what arg_temps.rs makes of a call
/// operand) are admitted wherever a value is, because every RC site they
/// reach is already a hook: an inner call's arguments are the call
/// hooks', its owned result is the temporary the enclosing site records
/// (`im` into an owned param or a payload slot, `id` when parked for a
/// borrowed one), a payload store is `witness_store`, a nested bind is the
/// Bind hook, and a concat reads its operands without a credit.
pub fn straightline_subset(body: &IrExpr, ret_is_heap: bool, self_name: &str) -> Option<String> {
    // `fn f(x) = expr` lowers exactly like `{ expr }`: a bare body is the
    // empty-statement block with that tail (B1: the tail-call and
    // literal-tail fns are almost all written this way).
    let (stmts, expr): (&[almide_ir::IrStmt], Option<&IrExpr>) = match &body.kind {
        IrExprKind::Block { stmts, expr } => (stmts, expr.as_deref()),
        _ => (&[], Some(body)),
    };
    if let Some(r) = stmts_subset(stmts) {
        return Some(r);
    }
    match expr.map(|t| &t.kind) {
        // A heap return is admitted only as a plain bound Var (the
        // ret-inc + move-out pair the func.rs hook records) or an OWNED
        // value (its one credit moves out); any other heap tail has
        // unrecorded RC sites.
        None | Some(IrExprKind::Unit) if !ret_is_heap => None,
        None => Some("tail:Unit-heap".into()),
        // A SELF tail call is loop-converted (tco.rs): the frame is not
        // replaced, the params are rebound by the loop-back and released
        // again by the epilogue — a loop, not a straight line. Out of
        // subset (the recorder is not loop-aware).
        Some(IrExprKind::Call { target: almide_ir::CallTarget::Named { name }, .. })
            if name.as_str() == self_name =>
        {
            Some("tail:self-call-loop".into())
        }
        // A scalar literal is no heap tail at all.
        Some(IrExprKind::LitInt { .. } | IrExprKind::LitBool { .. } | IrExprKind::LitFloat { .. })
            if ret_is_heap =>
        {
            Some("tail:scalar-lit-heap".into())
        }
        // A block tail: its statements join the frame's straight line, its
        // value is the tail's (rc_tail — the func.rs hooks read through it).
        Some(IrExprKind::Block { .. }) => straightline_subset(expr?, ret_is_heap, self_name),
        Some(_) => value_subset(expr?).map(|w| w.at("tail")),
    }
}

/// Why a value is out of subset: the node ITSELF (`Here(tag)`, reported
/// under the position it stands in — `rhs:If`, `call-arg:Lambda`), or a
/// node somewhere inside it (`Deep(reason)`, already reported under ITS
/// innermost position). The histogram thus names the shape to admit next
/// and the slot it sits in, never the path to it.
enum Why {
    Here(String),
    Deep(String),
}

impl Why {
    fn at(self, position: &str) -> String {
        match self {
            Why::Here(t) => format!("{position}:{t}"),
            Why::Deep(r) => r,
        }
    }

    /// The node is inside `position`: a `Here` becomes `Deep` there.
    fn inside(self, position: &str) -> Why {
        Why::Deep(self.at(position))
    }
}

/// The statement rules of a straight line: a Bind of an admissible value,
/// a statement-position call (its owned droppable result is released by
/// the discard route, `id`).
fn stmts_subset(stmts: &[almide_ir::IrStmt]) -> Option<String> {
    for s in stmts {
        match &s.kind {
            IrStmtKind::Bind { value, .. } => {
                if let Some(w) = value_subset(value) {
                    return Some(w.at("rhs"));
                }
            }
            IrStmtKind::Expr { expr } if matches!(expr.kind, IrExprKind::Call { .. }) => {
                if let Some(w) = call_subset(expr) {
                    return Some(w.at("stmt:Expr"));
                }
            }
            IrStmtKind::Expr { expr } => return Some(format!("stmt:Expr:{}", expr_tag(expr))),
            other => return Some(format!("stmt:{}", tag(other))),
        }
    }
    None
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

/// Is the value a scalar (no block, no RC site of its own)?
fn scalar_ty(t: &almide_types::types::Ty) -> bool {
    use almide_types::types::Ty;
    matches!(
        t,
        Ty::Int
            | Ty::Float
            | Ty::Int8
            | Ty::Int16
            | Ty::Int32
            | Ty::Int64
            | Ty::UInt8
            | Ty::UInt16
            | Ty::UInt32
            | Ty::UInt64
            | Ty::Float32
            | Ty::Float64
            | Ty::Bool
            | Ty::Unit
    )
}

/// A value whose every RC site is a recorder hook (#2755). `Some(tag)` names
/// the innermost shape that is not — the reason the histogram counts, under
/// the caller's position prefix (`rhs:`, `tail:`, `call-arg:` …).
fn value_subset(e: &IrExpr) -> Option<Why> {
    match &e.kind {
        IrExprKind::LitInt { .. }
        | IrExprKind::LitFloat { .. }
        | IrExprKind::LitBool { .. }
        | IrExprKind::LitStr { .. }
        | IrExprKind::Unit
        | IrExprKind::Var { .. }
        // `none` is NULL_ADDR: no block, no site.
        | IrExprKind::OptionNone => None,
        // A list literal: the spine is fresh, each element store is
        // `witness_store` exactly like a constructor payload's.
        IrExprKind::List { elements } => elements.iter().find_map(|x| value_subset(x).map(|w| w.inside("list-elem"))),
        // A call: its arguments are the call hooks' (recursively), its
        // droppable result is a received credit (#1986) the enclosing
        // site records.
        IrExprKind::Call { .. } => call_subset(e),
        // A one-slot constructor: the block is fresh (the enclosing site's
        // `i` / `im`), its payload store is `witness_store` — a Var shares
        // and moves in (`am`), an owned temporary moves in (`im`).
        IrExprKind::OptionSome { expr } | IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr } => {
            value_subset(expr).map(|w| w.inside("payload"))
        }
        IrExprKind::BinOp { op, left, right } => binop_subset(*op, left, right),
        // `{ let t = f(x); op(t) }` — arg_temps.rs's shape: the binds are the
        // Bind hook's (the frame's exit plan releases them), the value is
        // the tail's (`rc_owned_result` and the hooks read through blocks).
        IrExprKind::Block { stmts, expr: Some(tail) } => {
            stmts_subset(stmts).map(Why::Deep).or_else(|| value_subset(tail))
        }
        other => Some(Why::Here(tag(other))),
    }
}

/// A binary operator. The operators read their operands and spend no
/// credit (`$concat` copies, a comparison reads), so a HEAP operand is
/// admitted only as a Var or a pool-static literal: a fresh heap operand
/// would be an unowned temporary no hook records (arg_temps.rs binds every
/// such operand first, so this is the shape the emitter actually sees). A
/// SCALAR operand is any admissible value — except under `and` / `or`,
/// whose right operand runs conditionally: there it must be RC-free.
fn binop_subset(op: almide_ir::BinOp, left: &IrExpr, right: &IrExpr) -> Option<Why> {
    let operand = |x: &IrExpr| -> Option<Why> {
        if scalar_ty(&x.ty) {
            return value_subset(x).map(|w| w.inside("operand"));
        }
        match &x.kind {
            IrExprKind::Var { .. } | IrExprKind::LitStr { .. } => None,
            other => Some(Why::Deep(format!("heap-operand:{}", tag(other)))),
        }
    };
    if let Some(w) = operand(left) {
        return Some(w);
    }
    if matches!(op, almide_ir::BinOp::And | almide_ir::BinOp::Or) && !rc_free(right) {
        return Some(Why::Deep("short-circuit-operand".into()));
    }
    operand(right)
}

/// A value with no RC site anywhere inside (Vars, literals, operators over
/// them): safe to evaluate conditionally inside a straight-line frame.
fn rc_free(e: &IrExpr) -> bool {
    match &e.kind {
        IrExprKind::LitInt { .. }
        | IrExprKind::LitFloat { .. }
        | IrExprKind::LitBool { .. }
        | IrExprKind::LitStr { .. }
        | IrExprKind::Var { .. } => true,
        IrExprKind::BinOp { left, right, .. } => rc_free(left) && rc_free(right),
        IrExprKind::UnOp { operand, .. } => rc_free(operand),
        _ => false,
    }
}

/// A call the hooks cover: a Named user fn or variant constructor (the
/// builtin `some`/`ok`/`err` are IR kinds, not calls) or, since step 4, a
/// Module call (the native arms' declared modes are recorded at
/// `lower_arg`; the registry route consults the callee's param_owned
/// table like the Named route), over admissible arguments (#2755: nested
/// calls, operators, constructors — each argument's own sites are hooks,
/// and its owned result is the temporary the argument hook records).
fn call_subset(e: &IrExpr) -> Option<Why> {
    let IrExprKind::Call { target, args, .. } = &e.kind else {
        return Some(Why::Deep("call:not-a-call".into()));
    };
    match target {
        almide_ir::CallTarget::Named { name } => {
            // The http_framed host-op leaves (calls.rs) intercept before
            // resolution and lower their args outside every hook.
            if name.as_str().starts_with("__http_framed_")
                || name.as_str().starts_with("__http_call_")
                || name.as_str().starts_with("__http_serve_")
            {
                return Some(Why::Deep("call:host-splice".into()));
            }
            // `__is_null` reads the Value tag of its lowered argument, and
            // `panic` concatenates its message into a line it never binds:
            // no argument hook fires for either, so only an RC-free
            // argument is honest.
            if matches!(name.as_str(), "__is_null" | "panic") && !args.iter().all(rc_free) {
                return Some(Why::Deep(format!("call:{name}-arg")));
            }
        }
        almide_ir::CallTarget::Module { .. } => {}
        other => return Some(Why::Deep(format!("call:target:{}", tag(other)))),
    }
    for a in args {
        if let Some(w) = value_subset(a) {
            return Some(w.inside("call-arg"));
        }
    }
    None
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
