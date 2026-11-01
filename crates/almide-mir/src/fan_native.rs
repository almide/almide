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
//! - every arm is a direct call of a user function, and every argument is a
//!   literal or an immutable `let` local (a `var` reaches no arm);
//! - every argument and every arm result is of a type whose native value may
//!   cross a thread ([`thread_crossable`]); its strings are marked
//!   thread-shared before the spawn (ADR-0013 design point 3);
//! - the program has no metered region (a `fan.bounded` budget is a
//!   per-thread counter);
//! - no arm holds a higher-order call (`list.map` under `fan` is a parallel
//!   chunk map on the standard codegen; the sequential form would be slower),
//!   and the program calls no `fan.map` / `fan.any_map` (likewise).

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

/// Lift every threadable fan block's arms (module header), or decline the
/// program with the standing fan wall.
pub(crate) fn lift_fan_arms(mut ir: IrProgram) -> Result<IrProgram, LowerError> {
    ARMS.with(|a| a.borrow_mut().clear());
    let fans = Fans::of(&ir);
    if fans.blocks == 0 && fans.declined.is_none() {
        return Ok(ir);
    }
    if let Some(span) = fans.declined {
        return Err(LowerError::at(span, DECLINE));
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
    let mut lift = Lift { user: &user, globals: &globals, vt: &mut ir.var_table, decls: &ir.type_decls, out: Vec::new(), err: None, next_group: 0 };
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
    ir.functions.extend(out.into_iter().map(|(f, _)| f));
    Ok(ir)
}

/// What the program's fans need: how many blocks of two or more arms, and
/// the first fan form this render cannot thread (a decline).
struct Fans {
    blocks: usize,
    first: Option<almide_ir::Span>,
    declined: Option<Option<almide_ir::Span>>,
}

impl Fans {
    fn of(ir: &IrProgram) -> Fans {
        let mut v = Fans { blocks: 0, first: None, declined: None };
        for f in ir.functions.iter().filter(|f| !f.is_test) {
            v.visit_expr(&f.body);
        }
        v
    }
}

impl IrVisitor for Fans {
    fn visit_expr(&mut self, e: &IrExpr) {
        match &e.kind {
            IrExprKind::Fan { exprs } => {
                self.first = self.first.or(e.span);
                if exprs.len() >= 2 {
                    self.blocks += 1;
                }
                if exprs.iter().any(holds_higher_order_call) && self.declined.is_none() {
                    self.declined = Some(e.span);
                }
            }
            // An element-wise fan call — a list and a function over it
            // (`fan.map`, `fan.any_map`): a parallel map on the standard
            // codegen, so its sequential render here would be slower.
            IrExprKind::Call { target: CallTarget::Module { module, .. }, args, .. }
                if module.as_str() == "fan" && is_element_wise(args) && self.declined.is_none() =>
            {
                self.declined = Some(e.span);
            }
            _ => {}
        }
        walk_expr(self, e);
    }
}

/// A list argument and a function argument: the call maps the function over
/// the list's elements (read from the argument types, not the callee's name).
fn is_element_wise(args: &[IrExpr]) -> bool {
    args.iter().any(|a| matches!(&a.ty, Ty::Applied(TC::List, _))) && args.iter().any(|a| matches!(a.ty, Ty::Fn { .. }))
}

/// Does `e` hold a call that takes a function literal (a higher-order call)?
fn holds_higher_order_call(e: &IrExpr) -> bool {
    struct H(bool);
    impl IrVisitor for H {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Call { args, .. } = &e.kind {
                if args.iter().any(|a| matches!(a.kind, IrExprKind::Lambda { .. })) {
                    self.0 = true;
                }
            }
            walk_expr(self, e);
        }
    }
    let mut h = H(false);
    h.visit_expr(e);
    h.0
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
        Ty::Int | Ty::Float | Ty::Bool | Ty::Unit | Ty::String => true,
        Ty::Applied(TC::List | TC::Option | TC::Result, args) => args.iter().all(ok),
        Ty::Tuple(elems) => elems.iter().all(ok),
        Ty::Record { fields } => fields.iter().all(|(_, t)| ok(t)),
        Ty::Named(name, args) if args.is_empty() => decls.iter().find(|d| d.name == *name).is_some_and(|d| match &d.kind {
            almide_ir::IrTypeDeclKind::Record { fields } => fields.iter().all(|f| ok(&f.ty)),
            almide_ir::IrTypeDeclKind::Alias { target } => ok(target),
            almide_ir::IrTypeDeclKind::Variant { .. } => false,
        }),
        _ => false,
    }
}

struct Lift<'a> {
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
        let call = match &mut arm.kind {
            IrExprKind::Unwrap { expr } => &mut **expr,
            _ => arm,
        };
        let call_ty = call.ty.clone();
        let IrExprKind::Call { target: CallTarget::Named { name }, args, .. } = &mut call.kind else {
            return Err("an arm that is not a direct call".into());
        };
        let Some((_, ret, is_effect)) = self.user.get(name.as_str()).cloned() else {
            return Err(format!("an arm calling `{name}`, not a user function"));
        };
        if !thread_crossable(&call_ty, self.decls, 0) || !thread_crossable(&ret, self.decls, 0) {
            return Err(format!("an arm whose result `{call_ty:?}` cannot cross a thread"));
        }
        let mut params = Vec::new();
        let mut inner_args = Vec::new();
        let mut outer_args = Vec::new();
        for a in args.iter() {
            match &a.kind {
                IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. } | IrExprKind::Unit => {
                    inner_args.push(a.clone());
                }
                IrExprKind::Var { id } => {
                    let info = self.vt.get(*id).clone();
                    if info.mutability == Mutability::Var {
                        return Err(format!("an arm that captures the `var` `{}`", info.name));
                    }
                    if self.globals.contains(id) {
                        return Err(format!("an arm that reads the top-level `{}`", info.name));
                    }
                    if !thread_crossable(&info.ty, self.decls, 0) {
                        return Err(format!("an arm argument `{}` that cannot cross a thread", info.name));
                    }
                    let p = self.vt.alloc_like(*id, info.ty.clone(), Mutability::Let);
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
                    inner_args.push(IrExpr { kind: IrExprKind::Var { id: p }, ..a.clone() });
                    outer_args.push(a.clone());
                }
                _ => return Err("an arm argument that is not a literal or a local".into()),
            }
        }
        let lifted = arm_name(at.group, at.index);
        let mut body = call.clone();
        if let IrExprKind::Call { args, .. } = &mut body.kind {
            *args = inner_args;
        }
        *name = almide_base::intern::sym(&lifted);
        *args = outer_args;
        let f = IrFunction {
            name: almide_base::intern::sym(&lifted),
            params,
            ret_ty: call_ty,
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
            mutated_params: Vec::new(),
            module_origin: None,
        };
        self.out.push((f, at));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    // `var` is refused, so the program keeps the standard codegen.
    #[test]
    fn an_arm_capturing_a_var_is_refused() {
        let e = lift(
            "fn f(n: Int) -> Int = n + 1\n\
             effect fn main() -> Unit = {\n  var n = 1\n  n = n + 1\n  let (a, b) = fan { f(n), f(2) }\n  println(int.to_string(a + b))\n}\n",
        );
        match e {
            Err(e) => assert!(e.to_string().contains("var"), "{e}"),
            Ok(_) => {
                // The frontend may itself refuse a `var` reaching a fan arm;
                // then no arm with a var can be lifted at all.
                assert!(groups().is_empty() || arm("__fan_arm_0_0").is_none());
            }
        }
    }

    // ...and an arm whose argument is a function value cannot cross a thread.
    #[test]
    fn an_arm_with_a_closure_argument_is_refused() {
        assert!(!thread_crossable(
            &Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Int) },
            &[],
            0
        ));
        assert!(thread_crossable(&Ty::Applied(TC::List, vec![Ty::String]), &[], 0));
    }

    #[test]
    fn a_fan_holding_a_higher_order_call_declines() {
        let e = lift(
            "effect fn main() -> Unit = {\n  let ys = fan { list.map([1, 2], (x) => x + 1) }\n  println(int.to_string(list.len(ys)))\n}\n",
        );
        assert!(e.is_err());
    }
}
