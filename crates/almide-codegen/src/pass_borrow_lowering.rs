//! BorrowLoweringPass: spell every remaining ownership decision in the IR,
//! so the walker renders `Borrow` / `Clone` / `Var` nodes by their kind and
//! never by what surrounds them (#2186 step 3).
//!
//! Until now the walker re-derived, per site, what a borrow or a clone of a
//! by-reference parameter should become — drop the `&` on a param that is
//! already `&T`, `.to_string()` a `&str` instead of `.clone()`-ing it, own a
//! borrowed initializer into its `let`, `.clone()` a borrowed record before
//! spreading it, borrow INTO a list for an indexed read, take the runtime's
//! `_ref` twin for a decoded field, `.as_str()` a loop binder bound `&String`
//! — each a recogniser inside the renderer, and one (the indexed read) a
//! five-condition gate. This pass runs after every other pass that reads or
//! shapes a `Borrow` and rewrites each of those shapes into the IR form that
//! has exactly one spelling. It also publishes `param_borrows`, the one
//! table the walker reads where a param's mode still matters to a
//! statement's spelling (`*p = v` through a `&mut` param).

use std::collections::{HashMap, HashSet};
use almide_base::intern::{sym, Sym};
use almide_ir::*;
use almide_ir::annotations::CodegenAnnotations;
use almide_ir::top_let_storage::TopLetStorage;
use almide_ir::visit_mut::{walk_expr_mut, walk_stmt_mut, IrMutVisitor};
use almide_lang::types::{Ty, TypeConstructorId};
use super::pass::{NanoPass, PassResult, Target};

#[path = "pass_borrow_lowering_take.rs"]
mod take;

#[derive(Debug)]
pub struct BorrowLoweringPass;

impl NanoPass for BorrowLoweringPass {
    fn name(&self) -> &str { "BorrowLowering" }

    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::Rust])
    }

    /// Runs LAST: it reads the final `Borrow` forms, the loop-binder
    /// verdicts, the shared-cell set and the top-let storage attribute (a
    /// global's indexed read must not borrow into its snapshot), and what
    /// it emits (`RuntimeCall` macros, method calls) no earlier pass expects
    /// to see.
    fn depends_on(&self) -> Vec<&'static str> {
        vec!["CloneInsertion", "TailCallOpt", "SharedCellBorrow", "VarStorage", "IrLinkFlatten", "TopLetStorage"]
    }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut param_borrows: HashMap<VarId, ParamBorrow> = HashMap::new();
        let mut ref_binders: HashSet<VarId> = HashSet::new();
        let IrProgram { functions, modules, codegen_annotations, .. } = &mut program;
        let module_fns = modules.iter_mut().flat_map(|m| m.functions.iter_mut());
        for func in functions.iter_mut().chain(module_fns) {
            for p in &func.params {
                param_borrows.insert(p.var, p.borrow);
            }
            let mut lower = Lower { params: &func.params, ann: codegen_annotations, counting_binders: HashSet::new(), ref_binders: HashSet::new() };
            lower.visit_expr_mut(&mut func.body);
            lower.own_consumed_ref_mut(body_tail(&mut func.body));
            ref_binders.extend(lower.ref_binders);
        }
        // A top-level let's value is code too: the call-site borrow pass
        // inserts its `Borrow` nodes (`insert_borrows_at_call_sites`), so its
        // final form is decided here like a fn body's. Unvisited, a named fn
        // taken as a value (`let keep = apply`) kept `&_fn_arg0` of its
        // eta-expansion's closure param — `&Rc<dyn Fn>` at a `&dyn Fn` slot,
        // rustc E0277 (#3297). A top-let has no params of its own.
        let top_lets = program.top_lets.iter_mut()
            .chain(program.modules.iter_mut().flat_map(|m| m.top_lets.iter_mut()));
        for tl in top_lets {
            let mut lower = Lower { params: &[], ann: &program.codegen_annotations, counting_binders: HashSet::new(), ref_binders: HashSet::new() };
            lower.visit_expr_mut(&mut tl.value);
            ref_binders.extend(lower.ref_binders);
        }
        let mut guard_reads = GuardReads::default();
        let bodies = program.functions.iter().map(|f| &f.body)
            .chain(program.top_lets.iter().map(|tl| &tl.value))
            .chain(program.modules.iter().flat_map(|m| m.functions.iter().map(|f| &f.body)
                .chain(m.top_lets.iter().map(|tl| &tl.value))));
        for body in bodies {
            almide_ir::visit::IrVisitor::visit_expr(&mut guard_reads, body);
        }
        let codegen_annotations = &mut program.codegen_annotations;
        codegen_annotations.param_borrows = param_borrows;
        codegen_annotations.ref_binders = ref_binders;
        codegen_annotations.guard_read_vars = guard_reads.0;
        PassResult { program, changed: true }
    }
}

/// Every variable a `match` arm's guard reads, over the FINAL IR — the
/// `guard_read_vars` annotation the walker's box-pattern rewrite reads.
#[derive(Default)]
struct GuardReads(HashSet<VarId>);

impl almide_ir::visit::IrVisitor for GuardReads {
    fn visit_expr(&mut self, expr: &IrExpr) {
        if let IrExprKind::Match { arms, .. } = &expr.kind {
            for guard in arms.iter().filter_map(|a| a.guard.as_ref()) {
                self.0.extend(almide_ir::free_vars::free_vars(guard, &HashSet::new()));
            }
        }
        almide_ir::visit::walk_expr(self, expr);
    }
}

struct Lower<'a> {
    params: &'a [IrParam],
    ann: &'a CodegenAnnotations,
    /// Loop binders in `borrowed_loop_vars` whose head is a let-bound range
    /// the walker keeps as a bare `Range<i64>` (`range_counting_vars`,
    /// #1857): the head counts and binds an OWNED scalar, not `&T` (#2256).
    counting_binders: HashSet<VarId>,
    /// Payload binders of a `match` whose subject is a by-reference param
    /// (`s: &Shape`, the borrow pass's variant rule) or another such binder:
    /// Rust's default binding modes bind them `&T`, so a `Copy` scalar read
    /// derefs (`*n`) and a `&b` of a heap one is the naked binder, the same
    /// spellings a `&mut` param and a borrowed loop binder get.
    ref_binders: HashSet<VarId>,
}

/// Every variable a pattern binds (`Bind` and `As`, at any depth).
fn pattern_binders(p: &IrPattern, out: &mut Vec<VarId>) {
    match p {
        IrPattern::Wildcard | IrPattern::Literal { .. } | IrPattern::None => {}
        IrPattern::Bind { var, .. } => out.push(*var),
        IrPattern::As { var, inner, .. } => { out.push(*var); pattern_binders(inner, out); }
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => pattern_binders(inner, out),
        IrPattern::Constructor { args, .. } => args.iter().for_each(|a| pattern_binders(a, out)),
        IrPattern::Tuple { elements } => elements.iter().for_each(|e| pattern_binders(e, out)),
        IrPattern::List { elements, rest } => {
            elements.iter().for_each(|e| pattern_binders(e, out));
            if let Some(r) = rest { pattern_binders(r, out); }
        }
        IrPattern::RecordPattern { fields, .. } => {
            for f in fields { if let Some(p) = &f.pattern { pattern_binders(p, out); } }
        }
    }
}

/// The mode a param is passed in, by var.
fn param_mode(params: &[IrParam], id: VarId) -> Option<ParamBorrow> {
    params.iter().find(|p| p.var == id).map(|p| p.borrow)
}

/// Is `id` a param the fn receives by shared reference (`&T`, `&[T]`, `&str`)?
fn is_ref_param(params: &[IrParam], id: VarId) -> bool {
    matches!(param_mode(params, id), Some(ParamBorrow::Ref | ParamBorrow::RefSlice | ParamBorrow::RefStr))
}

fn is_ref_mut_param(params: &[IrParam], id: VarId) -> bool {
    matches!(param_mode(params, id), Some(ParamBorrow::RefMut))
}

/// A `Copy` scalar: a `mut` param of one of these is a `&mut T` whose every
/// READ must spell `*p` — an integer method auto-derefs, but `x * k`, `!b`
/// and a by-value argument slot do not (#2243).
fn is_copy_scalar(ty: &Ty) -> bool {
    matches!(ty,
        Ty::Int | Ty::Float | Ty::Bool
        | Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64
        | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
        | Ty::Float32 | Ty::Float64)
}

fn var_id(e: &IrExpr) -> Option<VarId> {
    match &e.kind {
        IrExprKind::Var { id } => Some(*id),
        _ => None,
    }
}

/// The expression a body's value is: the innermost block tail.
fn body_tail(e: &mut IrExpr) -> &mut IrExpr {
    if matches!(e.kind, IrExprKind::Block { expr: Some(_), .. }) {
        let IrExprKind::Block { expr: Some(tail), .. } = &mut e.kind else { unreachable!() };
        return body_tail(tail);
    }
    e
}

fn mk(kind: IrExprKind, ty: Ty, span: Option<almide_base::span::Span>) -> IrExpr {
    IrExpr { kind, ty, span, def_id: None }
}

/// `receiver.method()` — a no-argument method call on a value.
fn method_call(receiver: IrExpr, method: &str, ty: Ty) -> IrExpr {
    let span = receiver.span;
    mk(IrExprKind::Call {
        target: CallTarget::Method { object: Box::new(receiver), method: sym(method) },
        args: vec![],
        type_args: vec![],
    }, ty, span)
}

/// The owned form of a by-reference param's value: a `&[T]` is `.to_vec()`,
/// a `&str` is `.to_string()`, anything else is `.clone()` — a `.clone()`
/// of a slice would stay a `&[T]` and mismatch the owned `Vec<T>` (#624).
fn owned_read(p: IrExpr) -> IrExpr {
    let ty = p.ty.clone();
    match &ty {
        Ty::String => method_call(p, "to_string", ty),
        Ty::Applied(TypeConstructorId::List, _) => method_call(p, "to_vec", ty),
        _ => {
            let span = p.span;
            mk(IrExprKind::Clone { expr: Box::new(p) }, ty, span)
        }
    }
}

/// The root variable of a place read through fields, tuple indices and
/// shared borrows — what an indexed read borrows INTO.
fn place_root(e: &IrExpr) -> Option<VarId> {
    match &e.kind {
        IrExprKind::Var { id } => Some(*id),
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. }
        | IrExprKind::Borrow { expr: object, mutable: false, .. } => place_root(object),
        _ => None,
    }
}

/// A Bytes / Matrix rvalue whose rendering PROPAGATES the callee's expected
/// type into its arms — a `match` (which `??` lowers to), an `if`, a block.
/// Their value convention is `AlmideRcCow<T>` while the runtime signature
/// takes the raw `&T`, and the deref coercion `&var` relies on does not reach
/// through a propagating expression (#1210, #617): the borrow must deref
/// explicitly.
fn propagating_rc_cow_value(e: &IrExpr) -> bool {
    let rc_cow = matches!(e.ty, Ty::Bytes | Ty::Matrix | Ty::Applied(TypeConstructorId::Matrix, _));
    rc_cow && matches!(
        e.kind,
        IrExprKind::If { .. } | IrExprKind::Match { .. } | IrExprKind::Block { .. } | IrExprKind::UnwrapOr { .. }
    )
}

/// A list value no binding holds — a call's result, a collected chain, a
/// literal: borrowing it borrows a temporary that dies with the statement, so
/// handing it over by value moves nothing anyone reads again.
fn is_temporary(e: &IrExpr) -> bool {
    match &e.kind {
        IrExprKind::RuntimeCall { symbol, .. } => !symbol.as_str().ends_with('!'),
        IrExprKind::Call { .. } | IrExprKind::List { .. } => true,
        IrExprKind::IterChain { collector, .. } => matches!(collector, IterCollector::Collect),
        _ => false,
    }
}

/// The runtime's borrowing twin of a decoded-field lookup (#1679): the plain
/// lookup returns a copy of the field, the twin a borrow into the object,
/// with the same two error strings on a miss.
fn field_ref_twin(symbol: Sym) -> Option<&'static str> {
    match symbol.as_str() {
        "almide_rt_value_field" => Some("almide_rt_value_field_ref"),
        "almide_rt_value_field_at" => Some("almide_rt_value_field_ref_at"),
        _ => None,
    }
}

impl Lower<'_> {
    /// A `Borrow` node's final form.
    fn lower_borrow(&self, expr: &mut IrExpr) {
        let IrExprKind::Borrow { expr: inner, as_str, mutable } = &mut expr.kind else { return };
        let (as_str, mutable) = (*as_str, *mutable);
        // `&p` / `&mut p` of a param that already IS that reference: Rust
        // auto-reborrows the naked var, and the extra `&` was a `&&T`.
        if let Some(id) = var_id(inner)
            && ((!as_str && !mutable && is_ref_param(self.params, id))
                || (mutable && is_ref_mut_param(self.params, id)))
        {
            expr.kind = std::mem::replace(&mut inner.kind, IrExprKind::Unit);
            return;
        }
        // `&g` of a closure VALUE (an `Rc<dyn Fn>` handle: a local, a field, a
        // stored callable) at a borrowed fn slot (#2288) is `&Rc<dyn Fn>`,
        // which does not coerce to `&dyn Fn`: `&*g` reaches the callable
        // through the handle. A lambda literal and a fn item borrow as they
        // are (`&|x| …`, `&dbl` — both coerce to `&dyn Fn`), and a
        // by-reference fn param already IS the reference (handled above).
        if !as_str && !mutable
            && matches!(inner.ty, Ty::Fn { .. })
            && !matches!(inner.kind, IrExprKind::Lambda { .. } | IrExprKind::FnRef { .. } | IrExprKind::Deref { .. } | IrExprKind::Block { .. })
        {
            let value = std::mem::replace(inner.as_mut(), mk(IrExprKind::Unit, Ty::Unit, None));
            let ty = value.ty.clone();
            let span = value.span;
            *inner.as_mut() = mk(IrExprKind::Deref { expr: Box::new(value) }, ty, span);
            return;
        }
        // `&c` of a loop binder bound `&String` off `xs.iter()` (#1673) is
        // `&&String`. A concrete `&String` slot deref-coerces it, but the
        // generic key slot of `map.get` / `map.contains` (`K: Borrow<Q>`)
        // resolves `Q = &String` and rustc reports E0277 (#2256). The naked
        // binder IS the reference; a param of this fn is never its binder
        // (a `branch_lift` helper keeps the enclosing fn's ids, #2194).
        if !as_str && !mutable
            && let Some(id) = var_id(inner)
            && ((self.ann.borrowed_loop_vars.contains(&id) && param_mode(self.params, id).is_none() && !self.counting_binders.contains(&id))
                || self.ref_binders.contains(&id))
        {
            expr.kind = std::mem::replace(&mut inner.kind, IrExprKind::Unit);
            return;
        }
        // `&*c` of a loop binder bound `&String` off `xs.iter()` (#1673)
        // re-derefs to `&String`, and an ordering against a `&str` literal
        // has no `PartialOrd<&str> for &String` (#2188). `String::as_str`
        // reaches the `&str` through the reference and the owned binding
        // alike; a `&str` param keeps `&*` (`str::as_str` is unstable).
        if as_str && !mutable
            && let Some(id) = var_id(inner)
            && (self.ann.borrowed_loop_vars.contains(&id) || self.ref_binders.contains(&id))
        {
            let receiver = std::mem::replace(inner.as_mut(), mk(IrExprKind::Unit, Ty::Unit, None));
            *expr = method_call(receiver, "as_str", expr.ty.clone());
            return;
        }
        // `&xs[i]` (and the `&*xs[i]` a `&str` slot asks for — the `&String`
        // deref-coerces) borrows INTO the list instead of cloning the element
        // out (`almide_index_ref!`) — when the list is a place a reference
        // can live in: a global or a captured cell read renders an owned
        // snapshot, and a reference returned from the macro would outlive
        // that temporary.
        if !mutable
            && let IrExprKind::IndexAccess { object, index } = &inner.kind
            && let Some(root) = place_root(object)
            && self.ann.global(root).is_none()
            && !self.ann.is_shared_mut(&root)
            && super::pass_clone_projection::borrowable_index(index)
        {
            let IrExprKind::IndexAccess { object, index } = std::mem::replace(&mut inner.kind, IrExprKind::Unit) else { unreachable!() };
            expr.kind = IrExprKind::RuntimeCall { symbol: sym("almide_index_ref!"), args: vec![*object, *index] };
            return;
        }
        if mutable || as_str {
            return;
        }
        // `&(almide_rt_value_field(v, k))?` → `almide_rt_value_field_ref(v, k)?`:
        // the borrow's operand is consumed by the call it sits in, so the
        // twin's borrow into the object lives exactly as long as the copy
        // would have.
        if let IrExprKind::Try { expr: tried } = &mut inner.kind
            && let IrExprKind::RuntimeCall { symbol, .. } = &mut tried.kind
            && let Some(twin) = field_ref_twin(*symbol)
        {
            *symbol = sym(twin);
            expr.kind = std::mem::replace(&mut inner.kind, IrExprKind::Unit);
            return;
        }
        if propagating_rc_cow_value(inner) {
            let value = std::mem::replace(inner.as_mut(), mk(IrExprKind::Unit, Ty::Unit, None));
            let ty = value.ty.clone();
            let span = value.span;
            *inner.as_mut() = mk(IrExprKind::Deref { expr: Box::new(value) }, ty, span);
        }
    }

    /// A read of a `mut` scalar param is `*p` (#2243): the param is a
    /// `&mut i64` / `&mut f64` / `&mut bool`, and every value position — a
    /// binary operand, `!b`, an interpolation part, a by-value argument —
    /// needs the place, not the reference. A `&mut p` forwarding it to
    /// another `mut` callee becomes `&mut *p`, the explicit reborrow; the
    /// assignment target is a `VarId` the walker already spells `*p =`.
    fn lower_scalar_ref_read(&self, expr: &mut IrExpr) {
        let IrExprKind::Var { id } = &expr.kind else { return };
        if !(is_ref_mut_param(self.params, *id) || self.ref_binders.contains(id)) || !is_copy_scalar(&expr.ty) {
            return;
        }
        let var = std::mem::replace(expr, mk(IrExprKind::Unit, Ty::Unit, None));
        let ty = var.ty.clone();
        let span = var.span;
        *expr = mk(IrExprKind::Deref { expr: Box::new(var) }, ty, span);
    }

    /// `p.clone()` of a `&str` param yields a `&str`; the owned `String` the
    /// context expects is `.to_string()`.
    /// A `Clone` of a by-reference `String` / `List` param is its owned
    /// form (`.to_string()` / `.to_vec()`): `.clone()` of a `&str` or a
    /// `&[T]` would copy the REFERENCE and mismatch the owned type the
    /// consuming site expects (a guarded list pattern's `list.drop(xs, 1)`
    /// on a borrowed `xs`, #2231).
    fn lower_clone(&self, expr: &mut IrExpr) {
        let IrExprKind::Clone { expr: inner } = &mut expr.kind else { return };
        if let Some(id) = var_id(inner)
            && is_ref_param(self.params, id)
            && matches!(inner.ty, Ty::String | Ty::Applied(TypeConstructorId::List, _))
        {
            let receiver = std::mem::replace(inner.as_mut(), mk(IrExprKind::Unit, Ty::Unit, None));
            *expr = owned_read(receiver);
            return;
        }
        // A borrowed callable (`f: &dyn Fn`, #2288) is cloned by the clone
        // pass's always-clone rule for `Ty::Fn` (an `Rc` handle's refcount
        // bump); the reference itself is what every call reads — no clone.
        // So is a scope closure's binder (#3455): it is the `&|..| ..` itself.
        if let Some(id) = var_id(inner)
            && (is_ref_param(self.params, id) || self.ann.scope_closure_binders.contains(&id))
            && matches!(inner.ty, Ty::Fn { .. })
        {
            let value = std::mem::replace(inner.as_mut(), mk(IrExprKind::Unit, Ty::Unit, None));
            *expr = value;
        }
    }

    /// A struct update spreads an OWNED base; a by-reference param
    /// (`..st` with `st: &State`, #2037), a lazy global (a `LazyLock` deref)
    /// and a cross-module accessor read all need `.clone()` first.
    fn lower_spread(&self, expr: &mut IrExpr) {
        let IrExprKind::SpreadRecord { base, .. } = &mut expr.kind else { return };
        let Some(id) = var_id(base) else { return };
        let needs_clone = is_ref_param(self.params, id)
            || is_ref_mut_param(self.params, id)
            || self.ann.global_alias.contains_key(&id)
            || matches!(self.ann.global(id).map(|g| g.storage), Some(TopLetStorage::Lazy { .. }));
        if needs_clone {
            let value = std::mem::replace(base.as_mut(), mk(IrExprKind::Unit, Ty::Unit, None));
            let ty = value.ty.clone();
            let span = value.span;
            *base.as_mut() = mk(IrExprKind::Clone { expr: Box::new(value) }, ty, span);
        }
    }

    /// `lit == p` with `p: &T` compares a value against a reference
    /// (rustc E0308 behind a green check): the reference side derefs.
    /// Both sides references (`p == q`) compare as `&T == &T` and need
    /// nothing; strings are already borrowed on both sides as `&str`.
    ///
    /// A binder a match over a borrowed subject bound (`ref_binders`) is a
    /// reference exactly like the param it reads: `other => other == b` was
    /// `other == (*b)`, `&T == T` (rustc E0277, #3437). A `Copy` scalar
    /// binder is already read as `*n` and is a value here.
    fn lower_compare(&self, expr: &mut IrExpr) {
        let IrExprKind::BinOp { op: BinOp::Eq | BinOp::Neq, left, right } = &mut expr.kind else { return };
        // The box-deref of a boxed payload bound by reference (`Node(l, _)`
        // over a borrowed subject binds `l: &Box<T>`) is `*l`, a `Box<T>`,
        // which compares with neither a `T` nor a `&T`: read the value
        // through both (`**l`).
        for side in [&mut *left, &mut *right] {
            if let IrExprKind::Deref { expr: inner } = &side.kind
                && let Some(id) = var_id(inner)
                && self.ref_binders.contains(&id)
                && self.ann.box_binders.contains(&id)
                && !is_copy_scalar(&side.ty)
            {
                let value = std::mem::replace(side.as_mut(), mk(IrExprKind::Unit, Ty::Unit, None));
                let ty = value.ty.clone();
                let span = value.span;
                *side.as_mut() = mk(IrExprKind::Deref { expr: Box::new(value) }, ty, span);
            }
        }
        let is_ref = |e: &IrExpr| var_id(e).is_some_and(|id| {
            matches!(param_mode(self.params, id), Some(ParamBorrow::Ref | ParamBorrow::RefSlice))
                || (self.ref_binders.contains(&id) && !is_copy_scalar(&e.ty))
        });
        let (l, r) = (is_ref(left), is_ref(right));
        if l == r {
            return;
        }
        let side = if l { left } else { right };
        let value = std::mem::replace(side.as_mut(), mk(IrExprKind::Unit, Ty::Unit, None));
        let ty = value.ty.clone();
        let span = value.span;
        *side.as_mut() = mk(IrExprKind::Deref { expr: Box::new(value) }, ty, span);
    }

    /// The `map_get` template borrows the key itself (`.get(&{key})`), so
    /// the key must be an OWNED `K`: a `&str` param is `&&str`, not
    /// `&String` (rustc E0308, #1874) — materialize the owned key.
    fn lower_map_access(&self, expr: &mut IrExpr) {
        let IrExprKind::MapAccess { key, .. } = &mut expr.kind else { return };
        if let Some(id) = var_id(key)
            && is_ref_param(self.params, id)
            && matches!(key.ty, Ty::String)
        {
            let receiver = std::mem::replace(key.as_mut(), mk(IrExprKind::Unit, Ty::Unit, None));
            *key.as_mut() = method_call(receiver, "to_string", Ty::String);
        }
    }

    /// A statement that stores a by-reference param's value into an OWNED
    /// place — a `let` / `var` initializer, a reassignment, an element or a
    /// field — owns it first (#624, #1560, #2189). A `Clone` the clone pass
    /// already placed is the same request spelled once.
    fn lower_stored_value(&self, value: &mut IrExpr) {
        let id = match &value.kind {
            IrExprKind::Var { id } => *id,
            IrExprKind::Clone { expr } => match var_id(expr) { Some(id) => id, None => return },
            _ => return,
        };
        // A payload binder of a match over a borrowed subject is bound `&T`
        // by Rust's default binding modes (`ref_binders`): stored into an
        // owned place it is owned first, like the param it was read from
        // (#3303 — `color = c` in `Solid(c) => ..` was `expected C, found &C`).
        // A `Copy` scalar binder is already read as `*n`.
        let owns_first = is_ref_param(self.params, id)
            || ((is_ref_mut_param(self.params, id) || self.ref_binders.contains(&id)) && !is_copy_scalar(&value.ty));
        if !owns_first {
            return;
        }
        let ty = value.ty.clone();
        let span = value.span;
        *value = owned_read(mk(IrExprKind::Var { id }, ty, span));
    }

    /// A `mut` param is `&mut T` for its whole body — the writes through it
    /// are the point — so unlike a shared-borrow param it is both borrowed
    /// and, wherever the body hands the value on, consumed: inference never
    /// flips it to `Own`, and the clone pass's last-use move leaves the
    /// consuming occurrence a bare `Var`. Rust refuses that with E0308
    /// (`expected Table, found &mut Table`, #2266). Every by-value position
    /// owns the read first: a bare argument in an owned call slot, a
    /// constructor field or element, a concatenation operand, the body's
    /// result. A `Copy` scalar's read is `*p` ([`Lower::lower_scalar_ref_read`]).
    fn own_consumed_ref_mut(&self, e: &mut IrExpr) {
        // A by-value position hands on whatever value its branches end in
        // (`if c then ws else []`, a match arm, a block's tail — #3170).
        let branches: Vec<&mut IrExpr> = match &mut e.kind {
            IrExprKind::If { then, else_, .. } => vec![then.as_mut(), else_.as_mut()],
            IrExprKind::Match { arms, .. } => arms.iter_mut().map(|a| &mut a.body).collect(),
            IrExprKind::Block { expr: Some(tail), .. } => vec![tail.as_mut()],
            // A leaf value: judged below.
            _ => Vec::new(),
        };
        if !branches.is_empty() {
            return branches.into_iter().for_each(|b| self.own_consumed_ref_mut(b));
        }
        // The box-deref of a payload a by-reference match bound (`*a` with
        // `a: &Box<T>`, #3434) is a `Box<T>` place behind a reference: a
        // by-value slot owns the boxed value, `(**a).clone()`.
        if let IrExprKind::Deref { expr: inner } = &e.kind
            && var_id(inner).is_some_and(|id| self.ref_binders.contains(&id))
            && !is_copy_scalar(&e.ty)
        {
            let ty = e.ty.clone();
            let span = e.span;
            let place = std::mem::replace(e, mk(IrExprKind::Unit, Ty::Unit, None));
            *e = owned_read(mk(IrExprKind::Deref { expr: Box::new(place) }, ty, span));
            return;
        }
        let Some(id) = var_id(e) else { return };
        // A field a destructure bound by reference off a borrowed record
        // (`let { b, n } = p`, #3303) is in the same position: the binder is
        // `&T`, and a by-value slot owns it first.
        if !(is_ref_mut_param(self.params, id) || self.ref_binders.contains(&id)) || is_copy_scalar(&e.ty) {
            return;
        }
        let ty = e.ty.clone();
        let span = e.span;
        *e = owned_read(mk(IrExprKind::Var { id }, ty, span));
    }

    /// The source of a range op whose runtime takes it either way (#3398,
    /// `owned_source.rs`). A source that is an owned value — the bare `Var`
    /// the clone pass or TailCallOpt left at its last use, or a temporary the
    /// borrow would have pointed at — is passed by value to the op's `_owned`
    /// twin, which moves the kept elements instead of cloning them. A bare
    /// `Var` that is a reference in Rust (a by-reference param or binder, a
    /// borrowed loop binder) or a place a move cannot leave (a global, a
    /// shared cell, a copy-on-write `var`) is borrowed again: the borrowing
    /// op is what it takes.
    fn lower_either_way_source(&self, expr: &mut IrExpr) {
        let IrExprKind::RuntimeCall { symbol, args } = &mut expr.kind else { return };
        let Some(twin) = crate::owned_source::owned_twin(symbol.as_str()) else { return };
        let Some(source) = args.first_mut() else { return };
        match &source.kind {
            IrExprKind::Var { id } if self.stays_borrowed(*id) => {
                let value = std::mem::replace(source, mk(IrExprKind::Unit, Ty::Unit, None));
                let (ty, span) = (value.ty.clone(), value.span);
                *source = mk(IrExprKind::Borrow { expr: Box::new(value), as_str: false, mutable: false }, ty, span);
            }
            IrExprKind::Var { .. } => *symbol = sym(twin),
            IrExprKind::Borrow { expr: inner, as_str: false, mutable: false } if is_temporary(inner) => {
                let IrExprKind::Borrow { expr: inner, .. } = std::mem::replace(&mut source.kind, IrExprKind::Unit) else { unreachable!() };
                *source = *inner;
                *symbol = sym(twin);
            }
            _ => {}
        }
    }

    /// A var the walker renders as a reference or as a place nothing may be
    /// moved out of.
    fn stays_borrowed(&self, id: VarId) -> bool {
        is_ref_param(self.params, id)
            || is_ref_mut_param(self.params, id)
            || self.ref_binders.contains(&id)
            || self.ann.borrowed_loop_vars.contains(&id)
            || self.ann.global(id).is_some()
            || self.ann.is_shared_mut(&id)
            || self.ann.is_rc_cow(&id)
    }

    /// The by-value positions [`Lower::own_consumed_ref_mut`] applies to. A
    /// borrowed slot is a `Borrow` node here (BorrowInsertion ran), so a
    /// bare `Var` argument is an owned slot by construction.
    fn lower_consumers(&self, expr: &mut IrExpr) {
        let slots: Vec<&mut IrExpr> = match &mut expr.kind {
            IrExprKind::Call { args, .. } | IrExprKind::TailCall { args, .. }
            | IrExprKind::RuntimeCall { args, .. } => args.iter_mut().collect(),
            // A chain consumes a bare source by value (`.into_iter()`, #3170).
            IrExprKind::IterChain { source, consume: true, .. } => vec![source.as_mut()],
            IrExprKind::OptionSome { expr: e } | IrExprKind::ResultOk { expr: e }
            | IrExprKind::ResultErr { expr: e } => vec![e.as_mut()],
            IrExprKind::Record { fields, .. } => fields.iter_mut().map(|(_, f)| f).collect(),
            IrExprKind::List { elements } | IrExprKind::Tuple { elements } => elements.iter_mut().collect(),
            IrExprKind::BinOp { op: BinOp::ConcatStr | BinOp::ConcatList, left, right } => {
                vec![left.as_mut(), right.as_mut()]
            }
            // Not a consumer: the visitor walks its children.
            _ => Vec::new(),
        };
        for slot in slots { self.own_consumed_ref_mut(slot); }
    }
}

impl IrMutVisitor for Lower<'_> {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        if let IrExprKind::ForIn { var, iterable, .. } = &expr.kind
            && let Some(head) = var_id(iterable)
            && self.ann.range_counting_vars.contains(&head)
        {
            self.counting_binders.insert(*var);
        }
        // Before the consumer rule, which would own a bare reference source
        // with `.to_vec()` where a borrow is what the op takes.
        self.lower_either_way_source(expr);
        // Before the walk: the rule reads the ORIGINAL bare `Var` operands,
        // which the scalar-read lowering below would otherwise turn into
        // `Deref` first (a scalar is exempt anyway; the order keeps the two
        // rules independent).
        self.lower_consumers(expr);
        // A match on a by-reference param (or on a binder such a match
        // bound) binds its payloads by reference: note them before the arms
        // are walked, so their reads lower like a `&mut` param's.
        if let IrExprKind::Match { subject, arms } = &mut expr.kind {
            // A cloned subject (`xs.clone()`) is an owned value and binds by
            // value; only the variable itself, its borrow, or the box-deref
            // of a reference binder matches by reference.
            let root = match &subject.kind {
                IrExprKind::Var { id } => Some(*id),
                IrExprKind::Borrow { expr: inner, .. } | IrExprKind::Deref { expr: inner } => var_id(inner),
                _ => None,
            };
            if let Some(id) = root
                && (matches!(param_mode(self.params, id), Some(ParamBorrow::Ref)) || self.ref_binders.contains(&id))
            {
                // `match *t` of a boxed payload bound `&Box<T>` reads the
                // box's value through two references: `match &**t`, which
                // binds the arms' payloads by reference again.
                if matches!(subject.kind, IrExprKind::Deref { .. }) && self.ref_binders.contains(&id) {
                    let ty = subject.ty.clone();
                    let span = subject.span;
                    let inner = std::mem::replace(subject.as_mut(), mk(IrExprKind::Unit, Ty::Unit, None));
                    let deref2 = mk(IrExprKind::Deref { expr: Box::new(inner) }, ty.clone(), span);
                    **subject = mk(IrExprKind::Borrow { expr: Box::new(deref2), as_str: false, mutable: false }, ty, span);
                }
                // The by-reference param the subject reads is itself a
                // reference binding: recorded with the binders, so the
                // walker learns "this subject is borrowed" from the one
                // annotation and never reads a param's borrow mode.
                self.ref_binders.insert(id);
                let mut bound = Vec::new();
                for arm in arms.iter() { pattern_binders(&arm.pattern, &mut bound); }
                self.ref_binders.extend(bound);
            }
        }
        walk_expr_mut(self, expr);
        match &expr.kind {
            IrExprKind::Var { .. } => self.lower_scalar_ref_read(expr),
            IrExprKind::Borrow { .. } => self.lower_borrow(expr),
            IrExprKind::Clone { .. } => self.lower_clone(expr),
            IrExprKind::SpreadRecord { .. } => self.lower_spread(expr),
            IrExprKind::BinOp { .. } => self.lower_compare(expr),
            IrExprKind::MapAccess { .. } => self.lower_map_access(expr),
            _ => {}
        }
    }

    fn visit_stmt_mut(&mut self, stmt: &mut IrStmt) {
        // The stored-value rule reads the value's ORIGINAL shape: a
        // `Borrow { Var p }` the TCO rotation binds into a `_`-typed temp
        // carries the reference on purpose, and only becomes a bare `Var`
        // once the expression walk below lowers it.
        self.take_overwritten_ref_mut(stmt);
        // `let { b, n } = p` over a by-reference param (or a binder such a
        // match or destructure bound) binds its fields by reference, exactly
        // like a match arm's payloads (#3303): record them before the
        // statements that read them are walked.
        if let IrStmtKind::BindDestructure { pattern, value } = &stmt.kind
            && let Some(id) = (match &value.kind {
                IrExprKind::Borrow { expr: inner, .. } => var_id(inner),
                _ => var_id(value),
            })
            && (matches!(param_mode(self.params, id), Some(ParamBorrow::Ref)) || self.ref_binders.contains(&id))
        {
            let mut bound = Vec::new();
            pattern_binders(pattern, &mut bound);
            self.ref_binders.insert(id);
            self.ref_binders.extend(bound);
        }
        match &mut stmt.kind {
            IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. }
            | IrStmtKind::FieldAssign { value, .. } | IrStmtKind::IndexAssign { value, .. } => {
                self.lower_stored_value(value);
                self.own_consumed_ref_mut(value);
            }
            _ => {}
        }
        walk_stmt_mut(self, stmt);
    }
}
