//! `fan { … }` on the certified native render (#4006 family H).
//!
//! The standard codegen runs a fan block's arms on scoped threads through the
//! fan runtime (crates/almide-codegen/src/prelude_fan.rs: one output timeline
//! per arm, a trap waits for the arms below it, ADR-0024 D5/D6). The MIR the
//! certificate judges is SEQUENTIAL: the block desugars to its arms in order
//! (`desugar_fan_block`). This module keeps the arm boundary through that
//! desugar so the native render can run the arms on threads again.
//!
//! [`lift_fan_arms`] rewrites every fan arm `f(a, …)` (or `f(a, …)!`) of a
//! block with two or more arms into a call of a fresh function
//! `__fan_arm_<g>_<i>(x, …)` whose body is the original call, its params the
//! arm's variable arguments (literals stay in the body). Each lifted function
//! is an ordinary IR function: it is lowered to MIR, verified, and witnessed
//! like any other (proofs/native-wall.sh judges its ownership witness). The
//! lift records each one in a registry ([`arm`]) — the registry is filled by
//! the lift itself, never from a list of names.
//!
//! The PRECONDITIONS the concurrent run relies on (proofs/TRUSTED_BASE.md,
//! "fan arms on threads") are checked here, and a program whose fan does not
//! meet them keeps the honest decline (the standard codegen builds it):
//! - each threaded arm is a checked call/expression or an explicit Result
//!   scope, with only immutable free locals as captures; inline blocks keep
//!   the shared product classification and ordinary sequential MIR lowering;
//! - every argument and every arm result is of a type whose native value may
//!   cross a thread ([`thread_crossable`]); its strings are marked
//!   thread-shared before the spawn (ADR-0013 design point 3);
//! - the program has no metered region (a `fan.bounded` budget is a
//!   per-thread counter);
//! - higher-order calls selected by the shared routing keep registered
//!   parallel runtime sites. Ordered/effectful mappers use certified sequential MIR.

use almide_ir::visit::{walk_expr, IrVisitor};
use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
use almide_ir::{CallTarget, IrExpr, IrExprKind, IrFunction, IrParam, IrProgram, Mutability, VarId};
use almide_lang::types::constructor::TypeConstructorId as TC;
use almide_lang::types::Ty;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};

use crate::lower::LowerError;

/// A lifted arm: its group, its index in the group, and the group's size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Arm {
    pub group: usize,
    pub index: usize,
    pub count: usize,
}

thread_local! {
    static ARMS: RefCell<BTreeMap<String, Arm>> = const { RefCell::new(BTreeMap::new()) };
}

/// The lifted arm `name` is, if the last [`lift_fan_arms`] made it.
pub(crate) fn arm(name: &str) -> Option<Arm> {
    ARMS.with(|a| a.borrow().get(name).copied())
}

/// The arms the last lift made, by group: `(group, [arm name; count])`.
#[cfg(test)]
pub(crate) fn groups() -> BTreeMap<usize, Vec<String>> {
    let mut out: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    ARMS.with(|a| {
        for (name, arm) in a.borrow().iter() {
            let v = out.entry(arm.group).or_default();
            if v.len() < arm.count {
                v.resize(arm.count, String::new());
            }
            v[arm.index] = name.clone();
        }
    });
    out
}

/// The name of arm `index` of group `group`.
pub(crate) fn arm_name(group: usize, index: usize) -> String {
    format!("__fan_arm_{group}_{index}")
}

const DECLINE: &str = "native: fan concurrency requires the scoped-thread codegen";

/// Thread-local program slots may coexist with a fan only when no arm can
/// reach them. Follow user calls and closure references transitively; an
/// indirect/imported call is conservatively unknown. This runs before the
/// slot rewrite, while every slot access still carries its literal address.
pub(crate) fn reaches_global_slots(functions: &[crate::MirFunction]) -> bool {
    let roots = functions.iter().filter(|f| arm(&f.name).is_some() || crate::native_parallel_maps::is_wrapper(&f.name)).map(|f| f.name.as_str()).collect();
    reaches_slot_range(functions, roots, crate::lower::mutable_global_slot_count())
}

fn reaches_slot_range<'a>(functions: &'a [crate::MirFunction], mut todo: Vec<&'a str>, slots: u32) -> bool {
    let by_name: BTreeMap<_, _> = functions.iter().map(|f| (f.name.as_str(), f)).collect();
    let mut seen = HashSet::new();
    let base = crate::mg_slot_addr(0) as i64;
    let end = base + 8 * i64::from(slots);
    while let Some(name) = todo.pop() {
        if !seen.insert(name) { continue; }
        let Some(f) = by_name.get(name) else { continue };
        for op in &f.ops {
            match op {
                crate::Op::ConstInt { value, .. } if *value >= base && *value < end => return true,
                crate::Op::CallIndirect { .. } | crate::Op::CallImport { .. } => return true,
                crate::Op::CallFn { name, .. } | crate::Op::FuncRef { name, .. } => todo.push(name.as_str()),
                _ => {}
            }
        }
    }
    false
}

/// Lift every threadable fan block's arms (module header), or decline the
/// program with the standing fan wall.
pub(crate) fn lift_fan_arms(mut ir: IrProgram) -> Result<IrProgram, LowerError> {
    ARMS.with(|a| a.borrow_mut().clear());
    crate::native_parallel_maps::prepare(&mut ir)?;
    // Share the product backend's decision for ordered effects and captures
    // that cannot cross threads. These blocks use the ordinary MIR fan
    // desugar: settle every arm in order, then propagate the first error.
    almide_codegen::pass_fan_lowering::mark_inline_fans(&mut ir);
    let fans = Fans::of(&ir);
    if fans.blocks == 0 {
        return Ok(ir);
    }
    if almide_ir::meter_plan::program_has_regions(&ir) {
        return Err(LowerError::at(fans.first, format!("{DECLINE} (a metered region's budget is per thread)")));
    }
    let user: BTreeMap<String, (Vec<Ty>, Ty, bool)> = ir
        .functions
        .iter()
        .filter(|f| !f.is_test)
        .map(|f| (f.name.as_str().to_string(), (f.params.iter().map(|p| p.ty.clone()).collect(), f.ret_ty.clone(), f.is_effect)))
        .collect();
    let globals: HashSet<VarId> = ir.top_lets.iter().map(|t| t.var).collect();
    let mut lift = Lift { user: &user, globals: &globals, vt: &mut ir.var_table, decls: &ir.type_decls, inline: &ir.codegen_annotations, out: Vec::new(), err: None, next_group: 0 };
    for f in ir.functions.iter_mut().filter(|f| !f.is_test) {
        lift.visit_expr_mut(&mut f.body);
    }
    let Lift { out, err, .. } = lift;
    if let Some(e) = err {
        return Err(e);
    }
    ARMS.with(|a| {
        let mut a = a.borrow_mut();
        for (f, arm) in &out {
            a.insert(f.name.as_str().to_string(), *arm);
        }
    });
    let lifted = !out.is_empty();
    ir.functions.extend(out.into_iter().map(|(f, _)| f));
    if lifted {
        // The lifted fns' effect ABI (never-err, auto-wrap) is a fact of the
        // program, computed from every fn's body.
        crate::lower::install_effect_abi_facts(&ir);
    }
    Ok(ir)
}

/// What the program's fans need: how many blocks of two or more arms, and
/// the first fan form this render cannot thread (a decline).
struct Fans<'a> {
    inline: &'a almide_ir::annotations::CodegenAnnotations,
    blocks: usize,
    first: Option<almide_ir::Span>,
}

impl<'a> Fans<'a> {
    fn of(ir: &'a IrProgram) -> Fans<'a> {
        let mut v = Fans { inline: &ir.codegen_annotations, blocks: 0, first: None };
        for f in ir.functions.iter().filter(|f| !f.is_test) {
            v.visit_expr(&f.body);
        }
        v
    }
}

impl IrVisitor for Fans<'_> {
    fn visit_expr(&mut self, e: &IrExpr) {
        match &e.kind {
            IrExprKind::Fan { exprs } if !self.inline.is_inline_fan(e.span) => {
                self.first = self.first.or(e.span);
                if exprs.len() >= 2 {
                    self.blocks += 1;
                }
            }
            _ => {}
        }
        walk_expr(self, e);
    }
}



/// A type whose native value may move to, or be read from, another thread
/// once its strings are marked thread-shared (ADR-0013 design point 3):
/// scalars, String, and lists / options / results / tuples / records of them.
/// A function value (a closure's dispatch index and environment) never
/// crosses, nor does any type this does not know.
pub(crate) fn thread_crossable(ty: &Ty, decls: &[almide_ir::IrTypeDecl], depth: u32) -> bool {
    if depth > 8 {
        return false;
    }
    let ok = |t: &Ty| thread_crossable(t, decls, depth + 1);
    match ty {
        Ty::Int | Ty::Float | Ty::Bool | Ty::Unit | Ty::String | Ty::Bytes => true,
        t if crate::render_native_rec::is_lane_scalar(t) => true,
        Ty::Applied(TC::List | TC::Option | TC::Result | TC::Map | TC::Set, args) => args.iter().all(ok),
        Ty::Tuple(elems) => elems.iter().all(ok),
        Ty::Record { fields } => fields.iter().all(|(_, t)| ok(t)),
        Ty::Named(name, args) if args.is_empty() => decls.iter().find(|d| d.name == *name).is_some_and(|d| match &d.kind {
            almide_ir::IrTypeDeclKind::Record { fields } => fields.iter().all(|f| ok(&f.ty)),
            almide_ir::IrTypeDeclKind::Alias { target } => ok(target),
            almide_ir::IrTypeDeclKind::Variant { cases, .. } => cases.iter().all(|c| match &c.kind {
                almide_ir::IrVariantKind::Unit => true,
                almide_ir::IrVariantKind::Tuple { fields } => fields.iter().all(ok),
                almide_ir::IrVariantKind::Record { fields } => fields.iter().all(|f| ok(&f.ty)),
            }),
        }),
        _ => false,
    }
}

struct Lift<'a> {
    inline: &'a almide_ir::annotations::CodegenAnnotations,
    user: &'a BTreeMap<String, (Vec<Ty>, Ty, bool)>,
    globals: &'a HashSet<VarId>,
    vt: &'a mut almide_ir::VarTable,
    decls: &'a [almide_ir::IrTypeDecl],
    out: Vec<(IrFunction, Arm)>,
    err: Option<LowerError>,
    next_group: usize,
}

impl IrMutVisitor for Lift<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        walk_expr_mut(self, e);
        if self.err.is_some() {
            return;
        }
        let span = e.span;
        if self.inline.is_inline_fan(span) { return; }
        let IrExprKind::Fan { exprs } = &mut e.kind else { return };
        if exprs.len() < 2 {
            return;
        }
        let group = self.next_group;
        self.next_group += 1;
        let count = exprs.len();
        for (index, arm) in exprs.iter_mut().enumerate() {
            if let Err(why) = self.lift_arm(arm, Arm { group, index, count }) {
                self.err = Some(LowerError::at(span, format!("{DECLINE} ({why})")));
                return;
            }
        }
    }
}

impl Lift<'_> {
    /// Rewrite one arm in place to a call of its lifted function.
    fn lift_arm(&mut self, arm: &mut IrExpr, at: Arm) -> Result<(), String> {
        // The arm as written (`f(x)`), its scoped `f(x)!`, or the frontend's
        // `ok(e)` lift of an infallible arm: the expression inside.
        let x = match &mut arm.kind {
            IrExprKind::Unwrap { expr } | IrExprKind::ResultOk { expr } => &mut **expr,
            _ => arm,
        };
        let direct = match &x.kind {
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                self.user.get(name.as_str()).cloned().filter(|_| args.iter().all(is_arg_leaf))
            }
            _ => None,
        };
        // An effect fn declared `-> T` with T not a Result carries its Err in
        // the effect ABI only: the threaded take hands back that ABI's Result
        // while the arm's binding is typed T, and the render has no step that
        // unwraps it and propagates the lowest-index Err (C-199). Such an arm
        // keeps the honest decline; the standard codegen builds it.
        if calls_implicit_result_effect_fn(x, self.user) {
            return Err("an arm calling an effect fn whose declared result is not a Result".to_string());
        }
        let lifted = arm_name(at.group, at.index);
        let (body, ret, is_effect) = match direct {
            // A direct call `f(a, …)`: the lifted fn is `f`'s own signature,
            // its body the call (typed as `f` declares it, so the lift's
            // effect ABI is `f`'s).
            Some((_, ret, is_effect)) => {
                let mut body = x.clone();
                body.ty = ret.clone();
                (body, ret, is_effect)
            }
            // An operator expression over locals, literals and user calls
            // (an arm the optimizer inlined): the lifted fn returns its value.
            None => {
                if !is_arm_expr(x) {
                    let shape: String = format!("{:?}", x.kind).chars().take(120).collect();
                    return Err(format!("an arm that is not a call or an operator expression: {shape}"));
                }
                let is_effect = calls_effect_fn(x, self.user);
                // The fresh lifted function is the scoped arm's function
                // boundary, so keep its body without an extra closure call.
                let body = match &x.kind {
                    IrExprKind::Call { target: CallTarget::Computed { callee }, args, .. } if args.is_empty() => {
                        match &callee.kind {
                            IrExprKind::Lambda { params, body, .. } if params.is_empty() => (**body).clone(),
                            _ => x.clone(),
                        }
                    }
                    _ => x.clone(),
                };
                (body, x.ty.clone(), is_effect)
            }
        };
        if !thread_crossable(&x.ty, self.decls, 0) || !thread_crossable(&ret, self.decls, 0) {
            return Err(format!("an arm whose result `{:?}` cannot cross a thread", x.ty));
        }
        // Every local the arm reads becomes a parameter of the lifted fn.
        let free = almide_ir::free_vars::free_vars(&body, &HashSet::new());
        let mut subst: BTreeMap<VarId, VarId> = BTreeMap::new();
        let mut params = Vec::new();
        let mut outer_args = Vec::new();
        for id in free {
            if subst.contains_key(&id) {
                continue;
            }
            let info = self.vt.get(id).clone();
            if info.mutability == Mutability::Var {
                return Err(format!("an arm that captures the `var` `{}`", info.name));
            }
            if self.globals.contains(&id) {
                return Err(format!("an arm that reads the top-level `{}`", info.name));
            }
            if !thread_crossable(&info.ty, self.decls, 0) {
                return Err(format!("an arm argument `{}` that cannot cross a thread", info.name));
            }
            let p = self.vt.alloc_like(id, info.ty.clone(), Mutability::Let);
            subst.insert(id, p);
            params.push(IrParam {
                var: p,
                ty: info.ty.clone(),
                name: info.name,
                borrow: almide_ir::ParamBorrow::Own,
                is_mut: false,
                open_record: None,
                default: None,
                attrs: Vec::new(),
            });
            outer_args.push(IrExpr { kind: IrExprKind::Var { id }, ty: info.ty.clone(), span: x.span, def_id: None });
        }
        let mut body = body;
        rename_vars(&mut body, &subst);
        let call_ty = x.ty.clone();
        *x = IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Named { name: almide_base::intern::sym(&lifted) },
                args: outer_args,
                type_args: Vec::new(),
            },
            ty: call_ty,
            span: x.span,
            def_id: None,
        };
        let f = IrFunction {
            name: almide_base::intern::sym(&lifted),
            params,
            ret_ty: ret,
            body,
            is_effect,
            is_test: false,
            generics: None,
            extern_attrs: Vec::new(),
            export_attrs: Vec::new(),
            attrs: Vec::new(),
            visibility: almide_ir::IrVisibility::Private,
            doc: None,
            blank_lines_before: 0,
            def_id: None,
            mutated_params: Vec::new(), // fresh-fn: a lifted fan arm's params are fresh immutable lets (a `var` arm is refused above)
            module_origin: None,
        };
        self.out.push((f, at));
        Ok(())
    }
}

/// A literal or a local: an argument the lifted call can take as is.
fn is_arg_leaf(a: &IrExpr) -> bool {
    matches!(
        a.kind,
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. } | IrExprKind::Unit | IrExprKind::Var { .. }
    )
}

/// An arm the lift moves whole: literals, locals, operators, `if`, and
/// calls of user functions over them. A frontend scoped arm is already a
/// zero-argument closure with its own explicit Result; keep that scope whole.
fn is_arm_expr(e: &IrExpr) -> bool {
    match &e.kind {
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. } | IrExprKind::Unit | IrExprKind::Var { .. } => true,
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => elements.iter().all(is_arm_expr),
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr } | IrExprKind::OptionSome { expr } => is_arm_expr(expr),
        IrExprKind::OptionNone | IrExprKind::Lambda { .. } => true,
        IrExprKind::BinOp { left, right, .. } => is_arm_expr(left) && is_arm_expr(right),
        IrExprKind::UnOp { operand, .. } => is_arm_expr(operand),
        IrExprKind::If { cond, then, else_ } => is_arm_expr(cond) && is_arm_expr(then) && is_arm_expr(else_),
        IrExprKind::Call { target: CallTarget::Named { .. }, args, .. } => args.iter().all(is_arm_expr),
        IrExprKind::Call { target: CallTarget::Computed { callee }, args, .. } if args.is_empty() =>
            matches!(&callee.kind, IrExprKind::Lambda { params, body, .. }
                if params.is_empty() && body.ty == e.ty && e.ty.is_result()),
        IrExprKind::UnwrapOr { expr, fallback } => is_arm_expr(expr) && is_arm_expr(fallback),
        IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } =>
            crate::purity::is_pure(module.as_str(), func.as_str()) && args.iter().all(is_arm_expr),
        _ => false,
    }
}

/// Does `e` call a user function declared `effect` whose declared result is
/// not a `Result` (its Err rides the effect ABI only)?
fn calls_implicit_result_effect_fn(e: &IrExpr, user: &BTreeMap<String, (Vec<Ty>, Ty, bool)>) -> bool {
    struct C<'a>(&'a BTreeMap<String, (Vec<Ty>, Ty, bool)>, bool);
    impl IrVisitor for C<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Call { target: CallTarget::Named { name }, .. } = &e.kind {
                if let Some((_, ret, true)) = self.0.get(name.as_str()) {
                    self.1 |= !matches!(ret, Ty::Applied(TC::Result, _))
                        && !crate::lower::is_never_err_lifted(name.as_str());
                }
            }
            walk_expr(self, e);
        }
    }
    let mut c = C(user, false);
    c.visit_expr(e);
    c.1
}

/// Does `e` call a user function declared `effect`?
fn calls_effect_fn(e: &IrExpr, user: &BTreeMap<String, (Vec<Ty>, Ty, bool)>) -> bool {
    struct C<'a>(&'a BTreeMap<String, (Vec<Ty>, Ty, bool)>, bool);
    impl IrVisitor for C<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Call { target: CallTarget::Named { name }, .. } = &e.kind {
                if self.0.get(name.as_str()).is_some_and(|(_, _, eff)| *eff) {
                    self.1 = true;
                }
            }
            walk_expr(self, e);
        }
    }
    let mut c = C(user, false);
    c.visit_expr(e);
    c.1
}

/// Rename the locals of `e` by `subst`.
pub(crate) fn rename_vars(e: &mut IrExpr, subst: &BTreeMap<VarId, VarId>) {
    struct R<'a>(&'a BTreeMap<VarId, VarId>);
    impl IrMutVisitor for R<'_> {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            if let IrExprKind::Var { id } = &mut e.kind {
                if let Some(n) = self.0.get(id) {
                    *id = *n;
                }
            }
            walk_expr_mut(self, e);
        }
    }
    R(subst).visit_expr_mut(e);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thread_local_slots_require_no_transitive_arm_access() {
        use crate::{MirFunction, Op, ValueId};
        let f = |name: &str, ops| MirFunction { name: name.into(), ops, ..Default::default() };
        let call = |name: &str| Op::CallFn { name: name.into(), dst: None, args: vec![], result: None };
        let slot = || Op::ConstInt { dst: ValueId(0), value: crate::mg_slot_addr(0) as i64 };
        let functions = vec![f("arm", vec![call("leaf")]), f("leaf", vec![]), f("main", vec![slot()])];
        assert!(!reaches_slot_range(&functions, vec!["arm"], 1), "main-only slots are safe");
        let mut functions = functions;
        functions[1].ops.push(slot());
        assert!(reaches_slot_range(&functions, vec!["arm"], 1), "transitive slot read");
        functions[0].ops = vec![slot()];
        assert!(reaches_slot_range(&functions, vec!["arm"], 1), "direct slot read");
        functions[0].ops = vec![Op::FuncRef { dst: ValueId(0), name: "leaf".into() }];
        assert!(reaches_slot_range(&functions, vec!["arm"], 1), "closure reaches slot");
        functions[0].ops = vec![Op::CallIndirect { dst: None, table_idx: ValueId(0), args: vec![], result: None }];
        assert!(reaches_slot_range(&functions, vec!["arm"], 1), "unknown closure");
        functions[0].ops = vec![Op::CallImport { dst: None, module: "host".into(), name: "f".into(),
            args: vec![], abi: vec![], result: None, result_abi: None }];
        assert!(reaches_slot_range(&functions, vec!["arm"], 1), "unknown import");
        functions[0].ops = vec![call("leaf")];
        functions[1].ops = vec![call("arm")];
        assert!(!reaches_slot_range(&functions, vec!["arm"], 1), "slot-free cycle terminates");
        functions[1].ops.push(slot());
        assert!(reaches_slot_range(&functions, vec!["arm"], 1), "cycle still visits every body");
    }

    fn lift(src: &str) -> Result<IrProgram, LowerError> {
        let ir = crate::pipeline::source_to_ir_for_tests(src).expect("ir");
        lift_fan_arms(ir)
    }

    #[test]
    fn two_call_arms_lift_into_registered_functions() {
        let ir = lift(
            "effect fn f(t: String) -> Result[Int, String] = ok(string.len(t))\n\
             effect fn main() -> Unit = {\n  let s = \"ab\"\n  let (a, b) = fan { f(s), f(\"c\") }\n  println(int.to_string(a + b))\n}\n",
        )
        .expect("lifts");
        assert!(ir.functions.iter().any(|f| f.name.as_str() == "__fan_arm_0_0"));
        assert_eq!(arm("__fan_arm_0_1"), Some(Arm { group: 0, index: 1, count: 2 }));
        assert_eq!(groups().get(&0).map(Vec::len), Some(2));
    }

    // The gate (TRUSTED_BASE "fan arms on threads"): an arm that captures a
    // `var` is refused, so the program keeps the standard codegen. The
    // frontend already refuses a `var` in a fan arm, so the control flips the
    // argument's binding to `var` in the IR the lift sees.
    #[test]
    fn an_arm_capturing_a_var_is_refused() {
        let src = "fn f(n: Int) -> Int = n + 1\n\
                   effect fn main() -> Unit = {\n  let n = 1\n  let (a, b) = fan { f(n), f(2) }\n  println(int.to_string(a + b))\n}\n";
        let ir = crate::pipeline::source_to_ir_for_tests(src).expect("ir");
        assert!(lift_fan_arms(ir.clone()).is_ok(), "the `let` form lifts");
        let mut ir = ir;
        for v in ir.var_table.entries.iter_mut().filter(|v| v.name.as_str() == "n") {
            v.mutability = Mutability::Var;
        }
        let e = lift_fan_arms(ir).expect_err("a var argument is refused");
        assert!(e.to_string().contains("captures the `var`"), "{e}");
    }

    // ...and an arm whose argument is a function value cannot cross a thread.
    #[test]
    fn an_arm_with_a_closure_argument_is_refused() {
        assert!(!thread_crossable(
            &Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Int), is_effect: false },
            &[],
            0
        ));
        assert!(thread_crossable(&Ty::Applied(TC::List, vec![Ty::String].into()), &[], 0));
    }

    #[test]
    fn container_threadability_recurses_into_every_stored_type() {
        let closure = Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Int), is_effect: false };
        for container in [TC::List, TC::Option, TC::Set] {
            assert!(thread_crossable(&Ty::Applied(container.clone(), vec![Ty::String].into()), &[], 0));
            assert!(!thread_crossable(&Ty::Applied(container, vec![closure.clone()].into()), &[], 0));
        }
        assert!(thread_crossable(&Ty::Applied(TC::Map, vec![Ty::String, Ty::Int].into()), &[], 0));
        assert!(!thread_crossable(&Ty::Applied(TC::Map, vec![Ty::String, closure].into()), &[], 0));
    }

    #[test]
    fn ordered_mappers_share_the_product_parallel_decision() {
        let ordered = "fn f(x: Int) -> Result[Int, String] = { println(int.to_string(x)); ok(x) }\neffect fn main() -> Unit = { let xs = fan.map([1, 2], (x) => f(x))!; println(int.to_string(list.len(xs))) }\n";
        assert!(lift(ordered).is_ok(), "observable callback effects stay in list order");
        let parallel = "effect fn main() -> Unit = { let xs = fan.map([1, 2], (x) => ok(x + 1))!; println(int.to_string(list.len(xs))) }\n";
        let lifted = lift(parallel).expect("parallel site lifts");
        assert!(lifted.functions.iter().any(|f| crate::native_parallel_maps::is_wrapper(f.name.as_str())),
            "parallel mapper must retain its registered runtime site");
    }

    #[test]
    fn a_fan_holding_a_higher_order_call_keeps_its_selected_runtime() {
        let e = lift(
            "effect fn main() -> Unit = {\n  let ys = fan { list.map([1, 2], (x) => x + 1) }\n  println(int.to_string(list.len(ys)))\n}\n",
        );
        let ir = e.expect("a declared parallel callback lifts");
        assert!(ir.functions.iter().any(|f| crate::native_parallel_maps::is_wrapper(f.name.as_str())));
    }
}
