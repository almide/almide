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
}

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

/// A buffer type a caller can lend: a list, string, map or set. (Bytes keep
/// the move-mode convention: their in-place appends render through floor
/// calls that answer no value.)
fn lendable(ty: &Ty) -> bool {
    matches!(ty, Ty::String)
        || matches!(ty, Ty::Applied(TypeConstructorId::List | TypeConstructorId::Map | TypeConstructorId::Set, _))
}

/// Does `body` write `p` — an assignment, an in-place mutator, or a lend to
/// an in-out position of a candidate? A `mut` param the body never writes
/// has nothing to lend back and keeps the move-mode convention.
fn writes(body: &IrExpr, p: VarId, cands: &BTreeMap<String, usize>) -> bool {
    struct W<'a>(VarId, &'a BTreeMap<String, usize>, bool);
    impl IrVisitor for W<'_> {
        fn visit_stmt(&mut self, s: &IrStmt) {
            if matches!(s.kind, IrStmtKind::Assign { var, .. } if var == self.0) {
                self.2 = true;
            }
            walk_stmt(self, s);
        }
        fn visit_expr(&mut self, e: &IrExpr) {
            if rebinds(e, self.0) {
                self.2 = true;
            }
            if let IrExprKind::Call { target: CallTarget::Named { name }, args, .. } = &e.kind {
                if self.1.get(name.as_str()).is_some_and(|&k| args.get(k).is_some_and(|a| is_var(a, self.0))) {
                    self.2 = true;
                }
            }
            walk_expr(self, e);
        }
    }
    let mut w = W(p, cands, false);
    w.visit_expr(body);
    w.2
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
/// position of a candidate. A bind, a return, a capture, a container
/// element, a match subject or an iteration keeps the move-mode convention.
fn body_uses_ok(body: &IrExpr, p: VarId, cands: &BTreeMap<String, usize>, writes: &HashMap<String, Vec<usize>>) -> bool {
    struct C<'a> {
        p: VarId,
        cands: &'a BTreeMap<String, usize>,
        writes: &'a HashMap<String, Vec<usize>>,
        ok: bool,
    }
    impl C<'_> {
        fn read(&mut self, e: &IrExpr) {
            if !is_var(e, self.p) {
                self.visit_expr(e);
            }
        }
        fn call(&mut self, target: &CallTarget, args: &[IrExpr]) {
            for (i, a) in args.iter().enumerate() {
                if !is_var(a, self.p) {
                    self.visit_expr(a);
                    continue;
                }
                let lent = match target {
                    CallTarget::Named { name } => match self.cands.get(name.as_str()) {
                        Some(&k) => k == i && !args.iter().enumerate().any(|(j, b)| j != i && (mentions(b, self.p) || !simple_operand(b))),
                        None => !self.writes.get(name.as_str()).is_some_and(|ks| ks.contains(&i)),
                    },
                    CallTarget::Module { module, func, .. } => {
                        !almide_ir::mut_args::stdlib_mut_positions(module.as_str(), func.as_str())
                            .is_some_and(|ks| ks.contains(&i))
                    }
                    _ => false,
                };
                if !lent {
                    self.ok = false;
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
                IrExprKind::Var { id } if *id == self.p => self.ok = false,
                IrExprKind::Lambda { body, .. } => {
                    if mentions(body, self.p) {
                        self.ok = false;
                    }
                }
                IrExprKind::Call { target: CallTarget::Module { .. }, args, .. } if rebinds(e, self.p) => {
                    for a in &args[1..] {
                        if mentions(a, self.p) || !simple_operand(a) {
                            self.ok = false;
                        }
                        self.visit_expr(a);
                    }
                }
                IrExprKind::Call { target, args, .. } => self.call(target, args),
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
                        self.ok = false;
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
                IrStmtKind::Assign { value, .. } => self.read_assign(value),
                IrStmtKind::FieldAssign { target, .. }
                | IrStmtKind::IndexAssign { target, .. }
                | IrStmtKind::MapInsert { target, .. }
                    if *target == self.p =>
                {
                    self.ok = false
                }
                _ => walk_stmt(self, s),
            }
        }
    }
    impl C<'_> {
        /// The value of an assignment: a bare `p` there would alias the slot.
        fn read_assign(&mut self, value: &IrExpr) {
            if is_var(value, self.p) {
                self.ok = false;
            } else {
                self.visit_expr(value);
            }
        }
    }
    let mut c = C { p, cands, writes, ok: true };
    c.visit_expr(body);
    c.ok
}

/// The vars a call in `f` may lend: its `var` locals no closure captures,
/// plus its own `mut` params (`selected` or move-mode: the native
/// preparation owns a written param in a local).
fn lendable_vars(f: &IrFunction) -> HashSet<VarId> {
    struct Vars(HashSet<VarId>, HashSet<VarId>);
    impl IrVisitor for Vars {
        fn visit_stmt(&mut self, s: &IrStmt) {
            if let IrStmtKind::Bind { var, mutability: Mutability::Var, ty, .. } = &s.kind {
                if lendable(ty) {
                    self.0.insert(*var);
                }
            }
            walk_stmt(self, s);
        }
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Lambda { params, body, .. } = &e.kind {
                let bound = params.iter().map(|(v, _)| *v).collect();
                self.1.extend(almide_ir::free_vars::free_vars(body, &bound));
            }
            walk_expr(self, e);
        }
    }
    let mut v = Vars(HashSet::new(), HashSet::new());
    v.visit_expr(&f.body);
    let mut out: HashSet<VarId> = v.0.difference(&v.1).copied().collect();
    out.extend(f.params.iter().filter(|p| p.is_mut && lendable(&p.ty) && !v.1.contains(&p.var)).map(|p| p.var));
    out
}

/// Every call to a candidate in `e` lends a var of `ok_vars` at the in-out
/// position, outside any closure and named by no other argument; returns the
/// candidates some call refuses.
fn refused_sites(e: &IrExpr, cands: &BTreeMap<String, usize>, ok_vars: &HashSet<VarId>, refused: &mut BTreeSet<String>) {
    struct S<'a> {
        cands: &'a BTreeMap<String, usize>,
        ok_vars: &'a HashSet<VarId>,
        refused: &'a mut BTreeSet<String>,
        in_lambda: usize,
    }
    impl IrVisitor for S<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
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
                        if let Some(&k) = self.cands.get(&name) {
                            let good = !matches!(target, CallTarget::Module { .. })
                                && self.in_lambda == 0
                                && match args.get(k).map(|a| &a.kind) {
                                    Some(IrExprKind::Var { id }) => {
                                        self.ok_vars.contains(id)
                                            && !args.iter().enumerate().any(|(j, b)| j != k && (mentions(b, *id) || !simple_operand(b)))
                                    }
                                    _ => false,
                                };
                            if !good {
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
fn tuple_written_vars(e: &IrExpr, writes: &HashMap<String, Vec<usize>>, cands: &BTreeMap<String, usize>) -> HashSet<VarId> {
    struct T<'a>(&'a HashMap<String, Vec<usize>>, &'a BTreeMap<String, usize>, HashSet<VarId>);
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
    let mut cands: BTreeMap<String, usize> = BTreeMap::new();
    let mut names: HashMap<String, usize> = HashMap::new();
    for f in &ir.functions {
        *names.entry(f.name.to_string()).or_insert(0) += 1;
    }
    for f in &ir.functions {
        let name = f.name.as_str();
        let mut ks = f.mutated_params.clone();
        ks.sort_unstable();
        ks.dedup();
        let [k] = ks.as_slice() else { continue };
        let Some(p) = f.params.get(*k) else { continue };
        let plain = !name.contains('.') && !name.starts_with("__") && name != "main" && names.get(name) == Some(&1);
        let generic = f.generics.as_ref().is_some_and(|g| !g.is_empty());
        // A self-recursive fn stays out: its tail call becomes the loop
        // rewrite's carried params, which this convention does not carry.
        let recursive = calls_named(&f.body, name);
        // A function the host's native code calls back keeps the move-mode
        // convention its callback adapter writes back from.
        let host_called = crate::native_callbacks::is_root(name);
        if plain && !generic && !recursive && !host_called && !f.is_test && f.extern_attrs.is_empty() && f.export_attrs.is_empty()
            && p.is_mut && lendable(&p.ty)
        {
            cands.insert(name.to_string(), *k);
        }
    }
    // The positions every user fn writes back through (`mut` params).
    let writes: HashMap<String, Vec<usize>> = ir
        .functions
        .iter()
        .map(|f| (f.name.to_string(), f.params.iter().enumerate().filter(|(_, p)| p.is_mut).map(|(i, _)| i).collect()))
        .collect();
    // Every caller is a direct call site: a candidate no function calls is
    // reached some other way (the host, an export) and keeps its convention.
    cands.retain(|n, _| ir.functions.iter().any(|f| calls_named(&f.body, n)));
    loop {
        let mut refused: BTreeSet<String> = BTreeSet::new();
        for f in &ir.functions {
            if let Some(&k) = cands.get(f.name.as_str()) {
                let p = f.params[k].var;
                if !body_uses_ok(&f.body, p, &cands, &writes) || !self::writes(&f.body, p, &cands) {
                    refused.insert(f.name.to_string());
                }
            }
            // A var the move-mode convention writes back (a `mut` position of
            // a non-candidate) holds a borrowed payload of the returned tuple
            // afterwards, not a buffer of its own to lend.
            let mut ok_vars = lendable_vars(f);
            for v in tuple_written_vars(&f.body, &writes, &cands) {
                ok_vars.remove(&v);
            }
            refused_sites(&f.body, &cands, &ok_vars, &mut refused);
        }
        for t in &ir.top_lets {
            refused_sites(&t.value, &cands, &HashSet::new(), &mut refused);
        }
        if refused.is_empty() {
            break;
        }
        cands.retain(|n, _| !refused.contains(n));
    }
    for f in ir.functions.iter_mut() {
        if let Some(&k) = cands.get(f.name.as_str()) {
            f.mutated_params.clear();
            let p = f.params[k].var;
            statement_mutators(f, p);
        }
    }
    INOUT.with(|m| *m.borrow_mut() = cands.into_iter().map(|(n, k)| (n, vec![k])).collect());
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
