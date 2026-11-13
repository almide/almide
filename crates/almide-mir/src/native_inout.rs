//! In-out `mut` parameters on the native render: the caller lends its buffer
//! (`&mut` in the rendered Rust) instead of the C-132 move-mode convention's
//! value round trip (the callee returning the buffer in a tuple and every
//! call site writing it back).
//!
//! The move-mode form is right on every leg, but on this render it moves the
//! buffer through several locals per call: `fn add(mut acc, x) { list.push(acc,
//! x) }` called in a loop copied the `Vec` header param → local → tuple →
//! caller on every push, where the standard codegen pushes through the
//! caller's own `&mut Vec`. An in-out param keeps the buffer where the caller
//! has it.
//!
//! The convention, end to end:
//!
//! - **IR** ([`select`], native only, before the move-mode rewrite): a user
//!   fn whose one `mut` param is a heap buffer that its body only reads,
//!   rebinds (`acc = …`, the in-place mutators' functional rebinds) or lends
//!   on to another in-out position, and whose every call site lends a plain
//!   local `var` (or the caller's own `mut` param), keeps its source
//!   signature: neither it nor its call sites take the tuple rewrite.
//! - **MIR** (`lower`): the param is an owned SLOT of the body — every
//!   rebind releases the old object and `SetLocal`s the slot in place, on any
//!   path, the loop-carried slot discipline — and every exit hands the slot
//!   back with a `Consume` (`MirFunction::inout_params`). The caller passes the
//!   argument as a plain handle and keeps it.
//! - **Certificate**: the param's line opens with the `i` of the lent
//!   reference and closes with the hand-back `m`; the native-calls witness
//!   states the param borrowed (mode 0), as the caller neither moves nor copies.
//! - **Render**: the param is `&mut T`, a rebind is `*p = new` (or the in-place
//!   push window), a call lends `&mut v`.
//!
//! Anything outside that subset keeps the move-mode convention.

use almide_ir::visit::{walk_expr, walk_stmt, IrVisitor};
use almide_ir::visit_mut::{walk_expr_mut, walk_stmt_mut, IrMutVisitor};
use almide_ir::{CallTarget, IrExpr, IrExprKind, IrFunction, IrProgram, IrStmt, IrStmtKind, Mutability, VarId};
use almide_lang::types::constructor::TypeConstructorId;
use almide_lang::types::Ty;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

thread_local! {
    /// Whether a native render is in progress ([`ProgramScope`]): the only
    /// time [`INOUT`] is read or written, so no other leg ever sees it.
    static ACTIVE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// fn name → its in-out param positions, for the program being rendered.
    static INOUT: RefCell<BTreeMap<String, Vec<usize>>> = const { RefCell::new(BTreeMap::new()) };
    /// The in-out param vars of the function being lowered.
    static LOWERING: RefCell<HashSet<VarId>> = RefCell::new(HashSet::new());
    /// The non-generic record types the program declares, for [`select`].
    static RECORDS: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

/// The in-out candidates: fn name → its in-out param positions.
type Cands = BTreeMap<String, Vec<usize>>;

/// One native render: the in-out registry exists only inside it.
pub(crate) struct ProgramScope(bool);

impl ProgramScope {
    pub(crate) fn enter() -> Self {
        INOUT.with(|m| m.borrow_mut().clear());
        ProgramScope(ACTIVE.with(|a| a.replace(true)))
    }
}

impl Drop for ProgramScope {
    fn drop(&mut self) {
        INOUT.with(|m| m.borrow_mut().clear());
        ACTIVE.with(|a| a.set(self.0));
    }
}

/// The in-out param positions of `name`, if it takes any.
pub(crate) fn positions(name: &str) -> Option<Vec<usize>> {
    if !ACTIVE.with(|a| a.get()) {
        return None;
    }
    INOUT.with(|m| m.borrow().get(name).cloned())
}

/// The registry the render reads: each lowered fn's own in-out params
/// (`MirFunction::inout_params`, what a call lends `&mut` by), whatever a
/// prepare nested in this one left behind. With the registry emptied, a
/// render-time reader (a lent buffer is rebound, never a stack literal, a
/// mutable argument) took a lent buffer for unchanged: `var s = "  hi  "` lent
/// to `f(mut s)` printed the literal after the call.
pub(crate) fn register_lowered(functions: &[crate::MirFunction]) {
    let lowered = functions
        .iter()
        .filter(|f| !f.inout_params.is_empty())
        .map(|f| {
            let ks = f.params.iter().enumerate().filter(|(_, p)| f.inout_params.contains(&p.value)).map(|(k, _)| k).collect();
            (f.name.to_string(), ks)
        })
        .collect();
    INOUT.with(|m| *m.borrow_mut() = lowered);
}

/// Is position `k` of a call to `name` an in-out param?
pub(crate) fn is_inout_position(name: &str, k: usize) -> bool {
    positions(name).is_some_and(|ks| ks.contains(&k))
}

/// Is `v` an in-out param var of the function being lowered?
pub(crate) fn is_lowering_inout_var(v: VarId) -> bool {
    LOWERING.with(|s| s.borrow().contains(&v))
}

/// The in-out vars of one function's lowering, cleared when dropped.
pub(crate) struct LoweringScope;

impl LoweringScope {
    pub(crate) fn enter(vars: HashSet<VarId>) -> Self {
        LOWERING.with(|s| *s.borrow_mut() = vars);
        LoweringScope
    }
}

impl Drop for LoweringScope {
    fn drop(&mut self) {
        LOWERING.with(|s| s.borrow_mut().clear());
    }
}

/// A buffer type a caller can lend: a list, string, map or set, or a
/// declared non-generic record. (Bytes keep the move-mode convention: their
/// in-place appends render through floor calls that answer no value.)
fn lendable(ty: &Ty) -> bool {
    matches!(ty, Ty::String)
        || matches!(ty, Ty::Applied(TypeConstructorId::List | TypeConstructorId::Map | TypeConstructorId::Set, _))
        || is_record(ty)
}

/// A declared non-generic record type ([`RECORDS`]).
fn is_record(ty: &Ty) -> bool {
    let name = match ty {
        Ty::Named(n, args) if args.is_empty() => n.to_string(),
        Ty::Applied(TypeConstructorId::UserDefined(n), args) if args.is_empty() => n.to_string(),
        _ => return false,
    };
    RECORDS.with(|r| r.borrow().contains(&name))
}

/// A field write the in-out convention carries: a scalar value stored into
/// the record in place (`p.f = e`); a heap value rebinds the place to the
/// record with that field replaced, its other fields moved over.
fn field_write_ok(_value: &IrExpr) -> bool {
    true
}

/// The in-place mutator call (`list.push(p, x)`, `string.push`, the clears,
/// the byte appends) whose functional rebind targets `p`.
fn rebinds(e: &IrExpr, p: VarId) -> bool {
    crate::lower::inplace_rebind_target(e) == Some(p)
}

/// An operand the lowering evaluates in place: no `!`/`?` and no branch the
/// operand-order ANF would lift out of the call — lifting one snapshots every
/// earlier operand, the lent place included, and the write lands in the copy.
fn simple_operand(e: &IrExpr) -> bool {
    struct S(bool);
    impl IrVisitor for S {
        fn visit_expr(&mut self, e: &IrExpr) {
            match &e.kind {
                IrExprKind::Lambda { .. } => {}
                IrExprKind::Unwrap { .. } | IrExprKind::Try { .. } | IrExprKind::If { .. } | IrExprKind::Match { .. }
                | IrExprKind::Block { .. } | IrExprKind::ForIn { .. } | IrExprKind::While { .. } | IrExprKind::Fan { .. }
                | IrExprKind::UnwrapOr { .. } => self.0 = false,
                _ if self.0 => walk_expr(self, e),
                _ => {}
            }
        }
    }
    let mut s = S(true);
    s.visit_expr(e);
    s.0
}

/// The var a lent place names: the var itself, or the record var a field
/// path (`st.budget`, `st.a.b`) starts from; the caller's place either way,
/// lent `&mut` whole or by field (`crate::native_inout_field`).
fn place_root(e: &IrExpr) -> Option<VarId> {
    match &e.kind {
        IrExprKind::Var { id } => Some(*id),
        IrExprKind::Member { object, .. } if is_record(&object.ty) => place_root(object),
        _ => None,
    }
}

fn is_var(e: &IrExpr, p: VarId) -> bool {
    matches!(e.kind, IrExprKind::Var { id } if id == p)
}

fn mentions(e: &IrExpr, p: VarId) -> bool {
    struct M(VarId, bool);
    impl IrVisitor for M {
        fn visit_expr(&mut self, e: &IrExpr) {
            if is_var(e, self.0) {
                self.1 = true;
            }
            walk_expr(self, e);
        }
    }
    let mut m = M(p, false);
    m.visit_expr(e);
    m.1
}

/// Does `p` occur in `body` only where the in-out convention can carry it:
/// read by a call argument, an operator, an access or an interpolation;
/// rebound by an assignment or an in-place mutator; lent to an in-out
/// position of a candidate; read by a closure that does not outlive the
/// call it is passed to ([`scoped_read`]). A bind, a return, any other
/// capture, a container element, a match subject or an iteration keeps the
/// move-mode convention.
fn body_uses_ok(body: &IrExpr, p: VarId, cands: &Cands, writes: &HashMap<String, Vec<usize>>) -> bool {
    uses_ok(body, p, cands, writes, false)
}

/// [`body_uses_ok`]; with `read_only`, `p` is only read: no rebind, no field
/// write, no lend to a written position.
fn uses_ok(body: &IrExpr, p: VarId, cands: &Cands, writes: &HashMap<String, Vec<usize>>, read_only: bool) -> bool {
    struct C<'a> {
        p: VarId,
        cands: &'a Cands,
        writes: &'a HashMap<String, Vec<usize>>,
        read_only: bool,
        ok: bool,
    }
    impl C<'_> {
        /// The use that keeps `p` out of the convention (traced once).
        fn no(&mut self, why: &str) {
            if self.ok && !self.read_only {
                crate::trace::trace("ALMIDE_DBG_INOUT", || format!("[inout] {:?} stays move-mode: {why}", self.p));
            }
            self.ok = false;
        }
        fn read(&mut self, e: &IrExpr) {
            if !is_var(e, self.p) {
                self.visit_expr(e);
            }
        }
        fn call(&mut self, target: &CallTarget, args: &[IrExpr], ty: &Ty) {
            let captured = args.iter().any(|a| matches!(&a.kind, IrExprKind::Lambda { body, .. } if mentions(body, self.p)));
            if captured {
                if !scoped_read(target, args, ty, self.p, self.writes) {
                    self.no("closure outlives or writes");
                }
                return;
            }
            for (i, a) in args.iter().enumerate() {
                if !is_var(a, self.p) {
                    self.visit_expr(a);
                    continue;
                }
                let lent = match target {
                    CallTarget::Named { name } => match self.cands.get(name.as_str()) {
                        Some(ks) if ks.contains(&i) => {
                            !args.iter().enumerate().any(|(j, b)| j != i && (mentions(b, self.p) || !simple_operand(b)))
                        }
                        // A read position of a candidate: a borrow, unless the
                        // same call also lends the place out.
                        Some(ks) => !ks.iter().any(|&k| args.get(k).is_some_and(|b| mentions(b, self.p))),
                        None => !self.writes.get(name.as_str()).is_some_and(|ks| ks.contains(&i)),
                    },
                    CallTarget::Module { module, func, .. } => {
                        !almide_ir::mut_args::stdlib_mut_positions(module.as_str(), func.as_str())
                            .is_some_and(|ks| ks.contains(&i))
                    }
                    _ => false,
                };
                if !lent {
                    let callee = match target {
                        CallTarget::Named { name } => name.to_string(),
                        CallTarget::Module { module, func, .. } => format!("{module}.{func}"),
                        _ => "a computed callee".into(),
                    };
                    self.no(&format!("lend position {i} of {callee}"));
                }
            }
        }
    }
    impl IrVisitor for C<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            if !self.ok {
                return;
            }
            match &e.kind {
                IrExprKind::Var { id } if *id == self.p => self.no("bare value"),
                IrExprKind::Block { stmts, expr } => {
                    for (i, st) in stmts.iter().enumerate() {
                        if !scoped_let_closure(stmts, expr.as_deref(), i, self.p, self.writes) {
                            self.visit_stmt(st);
                        }
                    }
                    if let Some(t) = expr {
                        self.visit_expr(t);
                    }
                }
                IrExprKind::Lambda { body, .. } => {
                    if mentions(body, self.p) {
                        self.no("closure");
                    }
                }
                IrExprKind::Call { .. } if self.read_only && rebinds(e, self.p) => self.no("read-only rebind"),
                IrExprKind::Call { target: CallTarget::Module { .. }, args, .. } if rebinds(e, self.p) => {
                    for a in &args[1..] {
                        if mentions(a, self.p) || !simple_operand(a) {
                            self.no("mutator operand");
                        }
                        self.visit_expr(a);
                    }
                }
                IrExprKind::Call { target, args, .. } => self.call(target, args, &e.ty),
                IrExprKind::BinOp { left, right, .. } => {
                    self.read(left);
                    self.read(right);
                }
                IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => self.read(object),
                IrExprKind::IndexAccess { object, index } | IrExprKind::MapAccess { object, key: index } => {
                    self.read(object);
                    self.read(index);
                }
                IrExprKind::StringInterp { parts } => {
                    for part in parts {
                        if let almide_ir::IrStringPart::Expr { expr } = part {
                            self.read(expr);
                        }
                    }
                }
                IrExprKind::TailCall { .. } => {
                    if mentions(e, self.p) {
                        self.no("tail call");
                    }
                }
                _ => walk_expr(self, e),
            }
        }
        fn visit_stmt(&mut self, s: &IrStmt) {
            if !self.ok {
                return;
            }
            match &s.kind {
                IrStmtKind::Assign { var, .. } | IrStmtKind::FieldAssign { target: var, .. } if self.read_only && *var == self.p => {
                    self.no("read-only write")
                }
                // A snapshot of the place (`var trial = sol`): the bind owns a
                // copy of what the place holds now, as any bind of a var does.
                IrStmtKind::Bind { value, .. } if is_var(value, self.p) => {}
                IrStmtKind::Assign { value, .. } => self.read_assign(value),
                IrStmtKind::FieldAssign { target, value, .. } if *target == self.p && field_write_ok(value) => {
                    self.visit_expr(value)
                }
                IrStmtKind::FieldAssign { target, .. }
                | IrStmtKind::IndexAssign { target, .. }
                | IrStmtKind::MapInsert { target, .. }
                    if *target == self.p =>
                {
                    self.no("index or map write")
                }
                _ => walk_stmt(self, s),
            }
        }
    }
    impl C<'_> {
        /// The value of an assignment: a bare `p` there would alias the slot.
        fn read_assign(&mut self, value: &IrExpr) {
            if is_var(value, self.p) {
                self.no("assign alias");
            } else {
                self.visit_expr(value);
            }
        }
    }
    let mut c = C { p, cands, writes, read_only, ok: true };
    c.visit_expr(body);
    c.ok
}

/// A call that lends a place evaluates its other operands first, in order,
/// then lends: an operand with control flow (`!`, `??`, a branch) or one that
/// reads the lent place is bound to a `let` before the call, as the call
/// would evaluate it anyway. The call is then a lend beside plain values.
fn bind_operands_before_lends(ir: &mut IrProgram, cands: &Cands) {
    struct B<'a> {
        cands: &'a Cands,
        vars: &'a mut almide_ir::VarTable,
    }
    impl IrMutVisitor for B<'_> {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::Call { target: CallTarget::Named { name }, args, .. } = &e.kind else { return };
            let Some(ks) = self.cands.get(name.as_str()) else { return };
            let roots: Vec<VarId> = ks.iter().filter_map(|&k| args.get(k).and_then(place_root)).collect();
            let plain = |a: &IrExpr| {
                matches!(
                    a.kind,
                    IrExprKind::Var { .. }
                        | IrExprKind::LitInt { .. }
                        | IrExprKind::LitFloat { .. }
                        | IrExprKind::LitBool { .. }
                        | IrExprKind::LitStr { .. }
                        | IrExprKind::Unit
                        | IrExprKind::OptionNone
                        | IrExprKind::EmptyMap
                        | IrExprKind::FnRef { .. }
                        | IrExprKind::Lambda { .. }
                )
            };
            let needs = args.iter().enumerate().any(|(j, a)| {
                !ks.contains(&j) && !plain(a) && (!simple_operand(a) || roots.iter().any(|&r| mentions(a, r)))
            });
            if roots.is_empty() || !needs {
                return;
            }
            let ks = ks.clone();
            let IrExprKind::Call { args, .. } = &mut e.kind else { return };
            let mut binds = Vec::new();
            for (j, a) in args.iter_mut().enumerate() {
                if ks.contains(&j) || plain(a) {
                    continue;
                }
                let (ty, span) = (a.ty.clone(), a.span);
                let id = self.vars.alloc(almide_base::intern::sym("__arg_before_lend"), ty.clone(), Mutability::Let, span);
                let value = std::mem::replace(a, IrExpr { kind: IrExprKind::Var { id }, ty: ty.clone(), span, def_id: None });
                binds.push(IrStmt { kind: IrStmtKind::Bind { var: id, mutability: Mutability::Let, ty, value }, span });
            }
            let (ty, span) = (e.ty.clone(), e.span);
            let node = std::mem::take(e);
            *e = IrExpr { kind: IrExprKind::Block { stmts: binds, expr: Some(Box::new(node)) }, ty, span, def_id: None };
        }
    }
    let mut b = B { cands, vars: &mut ir.var_table };
    for f in ir.functions.iter_mut() {
        b.visit_expr_mut(&mut f.body);
    }
}

/// `stmts[i]` binds a closure that only reads `p` (`let member = (n) =>
/// lookup(env, n)`), and the closure lives no longer than a call: every use of
/// the name is a direct argument of a call that cannot keep it, and nothing
/// writes `p` from the bind until the last use is evaluated. A `for` whose
/// iterable holds the last use evaluates it before its body runs.
fn scoped_let_closure(stmts: &[IrStmt], tail: Option<&IrExpr>, i: usize, p: VarId, writes: &HashMap<String, Vec<usize>>) -> bool {
    let IrStmtKind::Bind { var: v, value, .. } = &stmts[i].kind else { return false };
    let IrExprKind::Lambda { body, .. } = &value.kind else { return false };
    let none = Cands::new();
    if !mentions(body, p) {
        return false;
    }
    let why = |w: &str| crate::trace::trace("ALMIDE_DBG_INOUT", || format!("[inout] let closure {v:?} over {p:?}: {w}"));
    if !uses_ok(body, p, &none, writes, true) {
        why("its body writes");
        return false;
    }
    let as_expr = |s: &IrStmt| match &s.kind {
        IrStmtKind::Expr { expr } => Some(expr.clone()),
        _ => None,
    };
    // The rest of the block, as expressions (a statement wrapped in a block).
    let wrap = |s: &IrStmt| IrExpr {
        kind: IrExprKind::Block { stmts: vec![s.clone()], expr: None },
        ty: Ty::Unit,
        span: None,
        def_id: None,
    };
    let rest: Vec<IrExpr> = stmts[i + 1..].iter().map(|s| as_expr(s).unwrap_or_else(|| wrap(s))).chain(tail.cloned()).collect();
    let Some(last) = rest.iter().rposition(|e| mentions(e, *v)) else { return false };
    if !rest[..=last].iter().all(|e| only_called_with(e, *v, writes)) {
        why("a use that may keep it");
        return false;
    }
    let read_only = |e: &IrExpr| !mentions(e, p) || uses_ok(e, p, &none, writes, true);
    // Another closure bound here that only reads `p` writes nothing either
    // (its own uses are checked where it is bound).
    let reading_bind = |k: usize| {
        stmts.get(i + 1 + k).is_some_and(|s| matches!(&s.kind,
            IrStmtKind::Bind { value, .. } if matches!(&value.kind, IrExprKind::Lambda { body, .. } if uses_ok(body, p, &none, writes, true))))
    };
    if !rest[..last].iter().enumerate().all(|(k, e)| reading_bind(k) || read_only(e)) {
        why("a write before its last use");
        return false;
    }
    let ok = match &rest[last].kind {
        IrExprKind::ForIn { iterable, body, .. } => {
            read_only(iterable) && !body.iter().any(|s| mentions(&wrap(s), *v))
        }
        _ => read_only(&rest[last]),
    };
    if !ok {
        why("a write beside its last use");
    }
    ok
}

/// Every use of `v` in `e` is a direct argument of a call that cannot keep it
/// (its answer holds no function, no `mut` param of it can hold one).
fn only_called_with(e: &IrExpr, v: VarId, writes: &HashMap<String, Vec<usize>>) -> bool {
    struct U<'a>(VarId, &'a HashMap<String, Vec<usize>>, bool);
    impl IrVisitor for U<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            match &e.kind {
                IrExprKind::Var { id } if *id == self.0 => self.2 = false,
                IrExprKind::Call { target, args, .. } if args.iter().any(|a| is_var(a, self.0)) => {
                    if !keeps_nothing(target, args, &e.ty, self.1) {
                        self.2 = false;
                    }
                    let v = self.0;
                    for a in args.iter().filter(|a| !is_var(a, v)) {
                        self.visit_expr(a);
                    }
                    if let CallTarget::Computed { callee } = target {
                        self.visit_expr(callee);
                    }
                }
                _ => walk_expr(self, e),
            }
        }
    }
    let mut u = U(v, writes, true);
    u.visit_expr(e);
    u.2
}

fn has_fn(ty: &Ty) -> bool {
    matches!(ty, Ty::Fn { .. }) || ty.children().into_iter().any(has_fn)
}

/// A call that cannot keep a closure past itself: its answer holds no
/// function value, and a user callee has no `mut` param that can hold one.
fn keeps_nothing(target: &CallTarget, args: &[IrExpr], ty: &Ty, writes: &HashMap<String, Vec<usize>>) -> bool {
    let callee = match target {
        CallTarget::Module { .. } => true,
        CallTarget::Named { name } => {
            !writes.get(name.as_str()).is_some_and(|ks| ks.iter().any(|&k| args.get(k).is_none_or(|a| has_fn(&a.ty))))
        }
        _ => false,
    };
    callee && !has_fn(ty)
}

/// A call that passes a closure capturing `p` lends it the place for the
/// call alone: the callee's answer holds no function value and no `mut`
/// param of the callee can hold one (the closure cannot leave the call), and
/// every argument, the closure bodies included, only reads `p` — nothing
/// writes the place while a closure that reads it is alive.
fn scoped_read(target: &CallTarget, args: &[IrExpr], ty: &Ty, p: VarId, writes: &HashMap<String, Vec<usize>>) -> bool {
    let none = Cands::new();
    keeps_nothing(target, args, ty, writes)
        && args.iter().all(|a| match &a.kind {
            IrExprKind::Lambda { body, .. } => uses_ok(body, p, &none, writes, true),
            _ => !mentions(a, p) || uses_ok(a, p, &none, writes, true),
        })
}

/// The vars a call in `f` may lend: its `var` locals no closure captures,
/// plus its own `mut` params (`selected` or move-mode: the native
/// preparation owns a written param in a local).
fn lendable_vars(f: &IrFunction, writes: &HashMap<String, Vec<usize>>) -> HashSet<VarId> {
    struct Vars<'a>(HashSet<VarId>, HashSet<VarId>, &'a HashMap<String, Vec<usize>>, HashSet<VarId>);
    impl IrVisitor for Vars<'_> {
        fn visit_stmt(&mut self, s: &IrStmt) {
            if let IrStmtKind::Bind { var, mutability: Mutability::Var, ty, .. } = &s.kind {
                if lendable(ty) {
                    self.0.insert(*var);
                }
            }
            walk_stmt(self, s);
        }
        fn visit_expr(&mut self, e: &IrExpr) {
            match &e.kind {
                // A closure passed straight to a call that it cannot outlive
                // and that only reads the var ([`scoped_read`]) leaves the
                // var lendable around the call.
                IrExprKind::Call { target, args, .. } if args.iter().any(|a| matches!(a.kind, IrExprKind::Lambda { .. })) => {
                    for a in args {
                        let IrExprKind::Lambda { params, body, .. } = &a.kind else {
                            self.visit_expr(a);
                            continue;
                        };
                        let bound = params.iter().map(|(v, _)| *v).collect();
                        for v in almide_ir::free_vars::free_vars(body, &bound) {
                            self.3.insert(v);
                            if !scoped_read(target, args, &e.ty, v, self.2) {
                                self.1.insert(v);
                            }
                        }
                        self.visit_expr(body);
                    }
                    if let CallTarget::Computed { callee } = target {
                        self.visit_expr(callee);
                    }
                }
                IrExprKind::Lambda { params, body, .. } => {
                    let bound = params.iter().map(|(v, _)| *v).collect();
                    let free = almide_ir::free_vars::free_vars(body, &bound);
                    self.3.extend(free.iter().copied());
                    self.1.extend(free);
                    walk_expr(self, e);
                }
                _ => walk_expr(self, e),
            }
        }
    }
    let mut v = Vars(HashSet::new(), HashSet::new(), writes, HashSet::new());
    v.visit_expr(&f.body);
    let mut out: HashSet<VarId> = v.0.difference(&v.1).copied().collect();
    // A move-mode fn keeps a `mut` param any closure captures in a write-back
    // cell, which has no place to lend (a candidate's own params are added
    // where its body passed `body_uses_ok`).
    out.extend(f.params.iter().filter(|p| p.is_mut && lendable(&p.ty) && !v.3.contains(&p.var)).map(|p| p.var));
    out
}

/// Does `e` mention `r` other than as a place lent to a candidate's in-out
/// position? Two lends side by side borrow the place one after the other; a
/// read may stay borrowed across the other's lend.
fn reads_besides_lends(e: &IrExpr, r: VarId, cands: &Cands) -> bool {
    struct R<'a>(VarId, &'a Cands, bool);
    impl IrVisitor for R<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            match &e.kind {
                IrExprKind::Var { id } if *id == self.0 => self.2 = true,
                IrExprKind::Call { target: CallTarget::Named { name }, args, .. } if self.1.contains_key(name.as_str()) => {
                    let ks = &self.1[name.as_str()];
                    for (k, a) in args.iter().enumerate() {
                        if !(ks.contains(&k) && is_var(a, self.0)) {
                            self.visit_expr(a);
                        }
                    }
                }
                _ => walk_expr(self, e),
            }
        }
    }
    let mut v = R(r, cands, false);
    v.visit_expr(e);
    v.2
}

/// The candidate calls inside `e` (outside closures) and the root of each
/// place they lend.
fn nested_lends(e: &IrExpr, cands: &Cands) -> Vec<(String, VarId)> {
    struct N<'a>(&'a Cands, Vec<(String, VarId)>);
    impl IrVisitor for N<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            match &e.kind {
                IrExprKind::Lambda { .. } => return,
                IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                    for &k in self.0.get(name.as_str()).into_iter().flatten() {
                        if let Some(r) = args.get(k).and_then(place_root) {
                            self.1.push((name.to_string(), r));
                        }
                    }
                }
                _ => {}
            }
            walk_expr(self, e);
        }
    }
    let mut n = N(cands, Vec::new());
    n.visit_expr(e);
    n.1
}

/// Every call to a candidate in `e` lends a var of `ok_vars` at the in-out
/// position, outside any closure and named by no other argument; returns the
/// candidates some call refuses.
fn refused_sites(e: &IrExpr, cands: &Cands, ok_vars: &HashSet<VarId>, refused: &mut BTreeSet<String>) {
    struct S<'a> {
        cands: &'a Cands,
        ok_vars: &'a HashSet<VarId>,
        refused: &'a mut BTreeSet<String>,
        in_lambda: usize,
    }
    impl IrVisitor for S<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            // Operands evaluated side by side: a lend inside one while another
            // reads the same place would hold a borrow of it across the lend.
            let siblings: Vec<&IrExpr> = match &e.kind {
                IrExprKind::StringInterp { parts } => parts
                    .iter()
                    .filter_map(|p| match p {
                        almide_ir::IrStringPart::Expr { expr } => Some(expr),
                        _ => None,
                    })
                    .collect(),
                IrExprKind::BinOp { left, right, .. } => vec![&**left, &**right],
                IrExprKind::Call { args, .. } => args.iter().collect(),
                _ => Vec::new(),
            };
            for (j, sib) in siblings.iter().enumerate() {
                for (name, r) in nested_lends(sib, self.cands) {
                    if siblings.iter().enumerate().any(|(i, o)| i != j && reads_besides_lends(o, r, self.cands)) {
                        self.refused.insert(name);
                    }
                }
            }
            match &e.kind {
                IrExprKind::Lambda { .. } => {
                    self.in_lambda += 1;
                    walk_expr(self, e);
                    self.in_lambda -= 1;
                    return;
                }
                IrExprKind::FnRef { name } if self.cands.contains_key(name.as_str()) => {
                    self.refused.insert(name.to_string());
                }
                IrExprKind::TailCall { target: CallTarget::Named { name }, .. } if self.cands.contains_key(name.as_str()) => {
                    self.refused.insert(name.to_string());
                }
                IrExprKind::Call { target, args, .. } => {
                    let named = match target {
                        CallTarget::Named { name } => Some(name.to_string()),
                        CallTarget::Module { module, func, .. } => {
                            let full = format!("{module}.{func}");
                            self.cands.contains_key(&full).then_some(full)
                        }
                        _ => None,
                    };
                    if let Some(name) = named {
                        if let Some(ks) = self.cands.get(&name) {
                            let good = !matches!(target, CallTarget::Module { .. })
                                && self.in_lambda == 0
                                && ks.iter().all(|&k| match args.get(k).and_then(place_root) {
                                    Some(id) => {
                                        self.ok_vars.contains(&id)
                                            && !args.iter().enumerate().any(|(j, b)| j != k && (mentions(b, id) || !simple_operand(b)))
                                    }
                                    None => false,
                                });
                            if !good {
                                crate::trace::trace("ALMIDE_DBG_INOUT", || {
                                    let why = if matches!(target, CallTarget::Module { .. }) {
                                        "a module path".to_string()
                                    } else if self.in_lambda > 0 {
                                        "inside a closure".to_string()
                                    } else {
                                        ks.iter()
                                            .map(|&k| match args.get(k).and_then(place_root) {
                                                None => format!("arg {k} is no place"),
                                                Some(id) if !self.ok_vars.contains(&id) => format!("arg {k} root {id:?} not lendable"),
                                                Some(_) => format!("arg {k} beside an operand that reads it or branches"),
                                            })
                                            .collect::<Vec<_>>()
                                            .join("; ")
                                    };
                                    format!("[inout] site of {name}: {why}")
                                });
                                self.refused.insert(name);
                            }
                        }
                    }
                }
                _ => {}
            }
            walk_expr(self, e);
        }
    }
    let mut s = S { cands, ok_vars, refused, in_lambda: 0 };
    s.visit_expr(e);
}

/// Wrap an in-place mutator of an in-out param that sits in expression
/// position (`fn add(mut xs, x) -> Unit = list.push(xs, x)`, an arm's tail)
/// as the statement the lowering rebinds from: `{ list.push(xs, x); () }`.
fn statement_mutators(f: &mut IrFunction, p: VarId) {
    struct W(VarId);
    impl IrMutVisitor for W {
        fn visit_stmt_mut(&mut self, s: &mut IrStmt) {
            if let IrStmtKind::Expr { expr } = &mut s.kind {
                if rebinds(expr, self.0) {
                    if let IrExprKind::Call { args, .. } = &mut expr.kind {
                        for a in args.iter_mut() {
                            self.visit_expr_mut(a);
                        }
                    }
                    return;
                }
            }
            walk_stmt_mut(self, s);
        }
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            if rebinds(e, self.0) {
                let span = e.span;
                let call = std::mem::replace(e, IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span, def_id: None });
                *e = IrExpr {
                    kind: IrExprKind::Block {
                        stmts: vec![IrStmt { kind: IrStmtKind::Expr { expr: call }, span }],
                        expr: Some(Box::new(IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span, def_id: None })),
                    },
                    ty: Ty::Unit,
                    span,
                    def_id: None,
                };
            }
        }
    }
    W(p).visit_expr_mut(&mut f.body);
}

/// Whether a call to `name` sits in a tail position of `e` (a block's
/// tail, an `if` or `match` arm in tail position), the self-call shape the
/// lowering turns into a loop.
fn tail_calls_named(e: &IrExpr, name: &str) -> bool {
    match &e.kind {
        IrExprKind::Call { target: CallTarget::Named { name: n }, .. }
        | IrExprKind::TailCall { target: CallTarget::Named { name: n }, .. } => n.as_str() == name,
        // `let r = f(..); r` returns the call's result unchanged: a tail call too.
        IrExprKind::Block { stmts, expr: Some(tail) } => {
            tail_calls_named(tail, name)
                || matches!((&tail.kind, stmts.last().map(|s| &s.kind)),
                    (IrExprKind::Var { id }, Some(IrStmtKind::Bind { var, value, .. })) if id == var && tail_calls_named(value, name))
        }
        IrExprKind::If { then, else_, .. } => tail_calls_named(then, name) || tail_calls_named(else_, name),
        IrExprKind::Match { arms, .. } => arms.iter().any(|a| tail_calls_named(&a.body, name)),
        _ => false,
    }
}

/// Whether every self call in `e` passes each in-out param `ps[i]` at its own
/// position: the tail-call loop then carries none of them (the place stays
/// where the caller lent it, the same on every iteration).
fn self_calls_pass_places(e: &IrExpr, name: &str, ps: &[(usize, VarId)]) -> bool {
    struct S<'a>(&'a str, &'a [(usize, VarId)], bool);
    impl IrVisitor for S<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Call { target: CallTarget::Named { name }, args, .. }
            | IrExprKind::TailCall { target: CallTarget::Named { name }, args, .. } = &e.kind
            {
                if name.as_str() == self.0 && !self.1.iter().all(|&(k, p)| args.get(k).is_some_and(|a| is_var(a, p))) {
                    self.2 = false;
                }
            }
            walk_expr(self, e);
        }
    }
    let mut s = S(name, ps, true);
    s.visit_expr(e);
    s.2
}

fn calls_named(e: &IrExpr, name: &str) -> bool {
    struct N<'a>(&'a str, bool);
    impl IrVisitor for N<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Call { target: CallTarget::Named { name }, .. }
            | IrExprKind::TailCall { target: CallTarget::Named { name }, .. } = &e.kind
            {
                if name.as_str() == self.0 {
                    self.1 = true;
                }
            }
            walk_expr(self, e);
        }
    }
    let mut n = N(name, false);
    n.visit_expr(e);
    n.1
}

/// The vars `e` passes to a `mut` position of a user fn outside `cands`.
fn tuple_written_vars(e: &IrExpr, writes: &HashMap<String, Vec<usize>>, cands: &Cands) -> HashSet<VarId> {
    struct T<'a>(&'a HashMap<String, Vec<usize>>, &'a Cands, HashSet<VarId>);
    impl IrVisitor for T<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Call { target: CallTarget::Named { name }, args, .. } = &e.kind {
                if !self.1.contains_key(name.as_str()) {
                    for &k in self.0.get(name.as_str()).into_iter().flatten() {
                        if let Some(v) = args.get(k).and_then(almide_ir::mut_args::place_root) {
                            self.2.insert(v);
                        }
                    }
                }
            }
            walk_expr(self, e);
        }
    }
    let mut t = T(writes, cands, HashSet::new());
    t.visit_expr(e);
    t.2
}

/// Choose the program's in-out functions (see the module doc), record them,
/// and take them out of the move-mode rewrite (their `mutated_params` are
/// cleared; the param keeps `is_mut`).
pub(crate) fn select(ir: &mut IrProgram) {
    INOUT.with(|m| m.borrow_mut().clear());
    if !ACTIVE.with(|a| a.get()) || !ir.modules.is_empty() {
        return;
    }
    RECORDS.with(|r| {
        *r.borrow_mut() = ir
            .type_decls
            .iter()
            .filter(|d| matches!(d.kind, almide_ir::IrTypeDeclKind::Record { .. }) && d.generics.as_ref().is_none_or(|g| g.is_empty()))
            .map(|d| d.name.to_string())
            .collect()
    });
    let mut names: HashMap<String, usize> = HashMap::new();
    for f in &ir.functions {
        *names.entry(f.name.to_string()).or_insert(0) += 1;
    }
    let mut cands: Cands =
        ir.functions.iter().filter_map(|f| Some((f.name.to_string(), candidate_positions(f, &names)?))).collect();
    // Every caller is a direct call site: a candidate no function calls is
    // reached some other way (the host, an export) and keeps its convention.
    cands.retain(|n, _| ir.functions.iter().any(|f| calls_named(&f.body, n)));
    bind_operands_before_lends(ir, &cands);
    prune(ir, &mut cands);
    for f in ir.functions.iter_mut() {
        if let Some(ks) = cands.get(f.name.as_str()) {
            f.mutated_params.clear();
            for &k in ks {
                let p = f.params[k].var;
                statement_mutators(f, p);
            }
        }
    }
    INOUT.with(|m| *m.borrow_mut() = cands);
}

/// [`select`]'s shape test: `f`'s written `mut` params, each a lendable
/// buffer, where `f` is a plain, uniquely named, non-generic user fn.
fn candidate_positions(f: &IrFunction, names: &HashMap<String, usize>) -> Option<Vec<usize>> {
    let name = f.name.as_str();
    let mut ks = f.mutated_params.clone();
    ks.sort_unstable();
    ks.dedup();
    if ks.is_empty() || !ks.iter().all(|&k| f.params.get(k).is_some_and(|p| p.is_mut && lendable(&p.ty))) {
        return None;
    }
    // A program module's fn (`kernel.spend`) is as plain as an entry fn; a
    // library module's keeps its own convention.
    let library = name.split_once('.').is_some_and(|(m, _)| almide_lang::stdlib_info::is_any_stdlib(m));
    let plain = !library && !name.starts_with("__") && name != "main" && names.get(name) == Some(&1);
    let generic = f.generics.as_ref().is_some_and(|g| !g.is_empty());
    // A self tail call becomes the loop rewrite's carried params, which this
    // convention does not carry: it stays out unless every self call passes
    // each place on as itself (the loop then carries none of them). A
    // non-tail self call lends the place on, as a call to any other
    // candidate does.
    let places: Vec<(usize, VarId)> = ks.iter().map(|&k| (k, f.params[k].var)).collect();
    let recursive = tail_calls_named(&f.body, name) && !self_calls_pass_places(&f.body, name, &places);
    // A function the host's native code calls back keeps the move-mode
    // convention its callback adapter writes back from.
    let host_called = crate::native_callbacks::is_root(name);
    let attrs = f.extern_attrs.is_empty() && f.export_attrs.is_empty();
    (plain && !generic && !recursive && !host_called && !f.is_test && attrs).then_some(ks)
}

/// [`select`]'s fixpoint: drop every candidate whose body or one of whose
/// call sites leaves the subset, until none does.
fn prune(ir: &IrProgram, cands: &mut Cands) {
    // The positions every user fn writes back through (`mut` params).
    let writes: HashMap<String, Vec<usize>> = ir
        .functions
        .iter()
        .map(|f| (f.name.to_string(), f.params.iter().enumerate().filter(|(_, p)| p.is_mut).map(|(i, _)| i).collect()))
        .collect();
    let dbg = almide_base::env::flag("ALMIDE_DBG_INOUT");
    loop {
        let mut refused: BTreeSet<String> = BTreeSet::new();
        for f in &ir.functions {
            if let Some(ks) = cands.get(f.name.as_str()) {
                let ok = ks.iter().all(|&k| {
                    let p = f.params[k].var;
                    body_uses_ok(&f.body, p, cands, &writes)
                });
                if !ok {
                    crate::trace::trace("ALMIDE_DBG_INOUT", || format!("[inout] {} refused: its body", f.name));
                    refused.insert(f.name.to_string());
                }
            }
            // A var the move-mode convention writes back (a `mut` position of
            // a non-candidate) holds a borrowed payload of the returned tuple
            // afterwards, not a buffer of its own to lend.
            let mut ok_vars = lendable_vars(f, &writes);
            // A candidate's own in-out param passed `body_uses_ok`: every
            // closure that captures it only reads it for one call's length.
            for &k in cands.get(f.name.as_str()).into_iter().flatten() {
                ok_vars.insert(f.params[k].var);
            }
            for v in tuple_written_vars(&f.body, &writes, &cands) {
                ok_vars.remove(&v);
            }
            let before = if dbg { refused.clone() } else { BTreeSet::new() };
            refused_sites(&f.body, &cands, &ok_vars, &mut refused);
            if dbg {
                for n in refused.difference(&before) {
                    eprintln!("[inout] {n} refused: a call site in {}", f.name);
                }
            }
        }
        for t in &ir.top_lets {
            refused_sites(&t.value, &cands, &HashSet::new(), &mut refused);
        }
        if refused.is_empty() {
            break;
        }
        cands.retain(|n, _| !refused.contains(n));
    }
}

/// Every exit of an in-out function hands its slots back: a `Consume` of each
/// before every `Return` and at the end of the body.
pub(crate) fn hand_back(ops: Vec<crate::Op>, slots: &BTreeSet<crate::ValueId>) -> Vec<crate::Op> {
    if slots.is_empty() {
        return ops;
    }
    let mut out = Vec::with_capacity(ops.len() + slots.len() * 2);
    for op in ops {
        if matches!(op, crate::Op::Return { .. }) {
            out.extend(slots.iter().map(|&v| crate::Op::Consume { v }));
        }
        out.push(op);
    }
    out.extend(slots.iter().map(|&v| crate::Op::Consume { v }));
    out
}

thread_local! {
    /// The in-out param values of the function being rendered.
    static RENDERING: RefCell<BTreeSet<crate::ValueId>> = const { RefCell::new(BTreeSet::new()) };
}

/// Is `v` an in-out param of the function being rendered (a `&mut T` name)?
pub(crate) fn is_rendering_inout(v: crate::ValueId) -> bool {
    RENDERING.with(|s| s.borrow().contains(&v))
}

/// The in-out params of one function's render; the enclosing render's set
/// comes back when dropped.
pub(crate) struct RenderScope(BTreeSet<crate::ValueId>);

impl RenderScope {
    pub(crate) fn enter(func: &crate::MirFunction) -> Self {
        crate::native_inout_field::note_function(func);
        RenderScope(RENDERING.with(|s| std::mem::replace(&mut *s.borrow_mut(), func.inout_params.clone())))
    }
}

impl Drop for RenderScope {
    fn drop(&mut self) {
        let prev = std::mem::take(&mut self.0);
        RENDERING.with(|s| *s.borrow_mut() = prev);
    }
}

/// The values `func` lends to an in-out param of a call.
pub(crate) fn lent_values(func: &crate::MirFunction) -> Vec<crate::ValueId> {
    let mut out = Vec::new();
    for op in &func.ops {
        if let crate::Op::CallFn { name, args, .. } = op {
            for (k, a) in args.iter().enumerate() {
                if let crate::CallArg::Handle(v) = a {
                    if is_inout_position(name, k) {
                        out.push(*v);
                    }
                }
            }
        }
    }
    out
}

/// Whether the field `place` (`v0.fK`) of the in-out param `base`, read at
/// `ops[at]`, may move out of the caller's record: on the straight path
/// after `at` the place is rebound whole (`SetLocal`) before anything reads
/// that field again, reads the record other than through the addresses of
/// its other fields, or hands it back. The spread rebind of a field write
/// (`p.f = op(p.f, ..)` ≡ `p = { ...p, f: op(p.f, ..) }`) is this shape: each
/// field leaves the old record once, into the op or the new record, and
/// `*p = ..` writes the new one over what is left.
pub(crate) fn field_moves_out(func: &crate::MirFunction, at: usize, base: crate::ValueId, place: &str) -> bool {
    use crate::Op;
    if !func.inout_params.contains(&base) {
        return false;
    }
    let Some(k) = place.strip_prefix(&format!("{}.f", crate::render_native::var(base))).and_then(|k| k.parse::<i64>().ok()) else {
        return false;
    };
    let offset = i64::from(crate::lower::layout::BLOCK_HEADER) + k * i64::from(crate::lower::layout::SLOT_SIZE);
    let consts: HashMap<crate::ValueId, i64> =
        func.ops.iter().filter_map(|op| if let Op::ConstInt { dst, value } = op { Some((*dst, *value)) } else { None }).collect();
    // Address values rooted at `base` → their byte offset into the block.
    let mut addrs: HashMap<crate::ValueId, Option<i64>> = HashMap::new();
    let mut reads = Vec::new();
    for op in &func.ops[at + 1..] {
        match op {
            Op::SetLocal { local, .. } if *local == base => return true,
            Op::IfThen { .. } | Op::Else { .. } | Op::EndIf { .. } | Op::LoopStart | Op::LoopEnd | Op::LoopBreakUnless { .. } | Op::Return { .. } => {
                return false
            }
            Op::Prim { kind: crate::PrimKind::Handle, dst: Some(d), args } if args.as_slice() == [base] => {
                addrs.insert(*d, Some(0));
                continue;
            }
            Op::IntBinOp { dst, op: crate::IntOp::Add, a, b } if addrs.contains_key(a) || addrs.contains_key(b) => {
                let (root, other) = if addrs.contains_key(a) { (a, b) } else { (b, a) };
                let at_off = addrs[root].zip(consts.get(other).copied()).map(|(x, y)| x + y);
                addrs.insert(*dst, at_off);
                continue;
            }
            // The hand-back is a read by the caller; the release of the old
            // record before the rebind frees what the moves left.
            Op::Consume { v } | Op::Borrow { v } if *v == base => return false,
            _ if crate::render_native_move::bookkeeping(op) => continue,
            _ => {}
        }
        reads.clear();
        crate::mir_ops::op_reads(op, &mut reads);
        if reads.contains(&base) {
            return false;
        }
        // A read through an address of the moved field, or of an address
        // whose offset is unknown, sees the emptied slot.
        if reads.iter().any(|r| addrs.get(r).is_some_and(|o| o.is_none_or(|o| o == offset))) {
            return false;
        }
    }
    false
}
