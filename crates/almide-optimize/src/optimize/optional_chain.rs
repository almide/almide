//! Optional-chain desugar: rewrite `p?.f` into a call to a synthesized tail
//! helper function so BOTH backends (and the v1 trust-spine wasm renderer in
//! particular) see a proven shape.
//!
//! ## The problem this solves
//!
//! `p?.f` reaches the shared pipeline as `IrExprKind::OptionalChain`. The
//! MIR-side desugar (`almide-mir/src/lower/mod_c.rs::desugar_optional_chain`)
//! rewrites it into `match p { some(x) => some(x.f), none => none }` — but it
//! runs AFTER `optimize::optimize_program`, so `branch_lift::lift_heap_branch_binds`
//! (which runs HERE, at the shared cut point) never sees the match. The
//! let-bound form then falls to the MIR tail-duplication desugar, which copies
//! the call-bearing continuation into both arms of a match over an untracked
//! subject — the "match over an UNTRACKED subject with a call-bearing arm"
//! wall. An ARG-position chain (`int.to_string(np?.x ?? -9)`) walls too: a
//! `??` over a non-Var Option operand in call-argument position is not
//! lowerable, while `??` over a CALL operand is (measured).
//!
//! ## The fix (validated empirically, the branch_lift discipline)
//!
//! Rewrite the chain into a call to a fresh top-level helper whose BODY is the
//! some/none match in tail position — the shape `try_lower_variant_value_match`
//! already renders for scalar and heap payloads:
//!
//! ```almide
//! fn optional_chain_synth_0(s: Pt?) -> Int? = match s { some(x) => some(x.x), none => none }
//! // …
//! let px = optional_chain_synth_0(p)          // bind position: proven heap call-result
//! println(int.to_string(optional_chain_synth_0(np) ?? -9))  // arg position: proven ?? operand
//! ```
//!
//! A Named call is a proven shape in EVERY position (bind, call argument,
//! `??` operand, tail), so one desugar covers all chain sites uniformly. The
//! subject expression moves into the argument slot — still evaluated exactly
//! once, in the same order.
//!
//! ## Why this lives in `almide-optimize`
//!
//! Same reason as `branch_lift` (see its module docs): `optimize_program` is
//! the shared cut point, so the v1 trust-spine path, the native codegen path,
//! AND the interp/classify counters all see the identical rewritten tree —
//! the helper call is a real IR `Call` node counted by `count_ir_calls`, and
//! the MIR lowering emits exactly one `CallFn` for it (`mir == ir` by
//! construction). Running before mono means the helper is monomorphized and
//! linked like any other user function.
//!
//! ## Scope (minimal blast radius)
//!
//! Fires only for a chain whose subject type is a concrete `Option[P]`, whose
//! result type is a concrete `Option[F]`, AND whose field `F` is a SCALAR
//! (Int/Float/Bool/…). Two decline classes keep their existing route (the
//! native codegen node + the MIR-side desugar) exactly as before:
//!
//! - `TypeVar`/`Unknown` anywhere in those types (a generic fn body pre-mono,
//!   or checker error recovery) — a helper fn cannot carry the enclosing fn's
//!   type variables.
//! - a HEAP field (`u?.name` over `name: String`): the helper's
//!   `Option[String]`-returning tail match does NOT lower on the v1 spine
//!   (measured: the user-written identical helper walls "heap-result `match`
//!   … would move out an empty deferred heap value"), while the existing MIR
//!   route passes the unwrap_operators test-mode corpus shapes today.
//!   Desugaring it here would trade a passing shape for an unlinked-call wall
//!   — declined until the heap-payload variant-return brick lands.

use almide_base::intern::sym;
use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
use almide_ir::*;
use almide_lang::types::{constructor::TypeConstructorId, is_heap_ty, Ty};

/// Rewrite every concrete-typed `OptionalChain` into a synthesized-helper call.
pub fn desugar_optional_chains(program: &mut IrProgram) {
    let mut counter: u32 = 0;

    // Root program: function bodies + top-level let initializers share the
    // program-wide `var_table` (the branch_lift region discipline).
    {
        let IrProgram { functions, top_lets, var_table, .. } = &mut *program;
        let mut d = ChainDesugarer { vt: var_table, counter: &mut counter, new_funcs: Vec::new() };
        for func in functions.iter_mut() {
            d.visit_expr_mut(&mut func.body);
        }
        for tl in top_lets.iter_mut() {
            d.visit_expr_mut(&mut tl.value);
        }
        let lifted = d.new_funcs;
        functions.extend(lifted);
    }

    // Imported modules: each carries its own `var_table`, so helpers synthesized
    // from a module's functions live in that module (their VarIds index the
    // module's table, not the program's).
    for module in program.modules.iter_mut() {
        let IrModule { functions, top_lets, var_table, .. } = &mut *module;
        let mut d = ChainDesugarer { vt: var_table, counter: &mut counter, new_funcs: Vec::new() };
        for func in functions.iter_mut() {
            d.visit_expr_mut(&mut func.body);
        }
        for tl in top_lets.iter_mut() {
            d.visit_expr_mut(&mut tl.value);
        }
        let lifted = d.new_funcs;
        functions.extend(lifted);
    }
}

struct ChainDesugarer<'a> {
    vt: &'a mut VarTable,
    counter: &'a mut u32,
    new_funcs: Vec<IrFunction>,
}

impl IrMutVisitor for ChainDesugarer<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        // Bottom-up: a chained subject (`a?.b` inside another chain's subject)
        // is rewritten first, so the outer helper's argument is already a call.
        walk_expr_mut(self, e);
        let (subj_ty, payload_ty, field_ty, field) = {
            let IrExprKind::OptionalChain { expr, field } = &e.kind else {
                return;
            };
            let Ty::Applied(TypeConstructorId::Option, subj_args) = &expr.ty else {
                return;
            };
            let [payload_ty] = subj_args.as_slice() else {
                return;
            };
            let Ty::Applied(TypeConstructorId::Option, res_args) = &e.ty else {
                return;
            };
            let [field_ty] = res_args.as_slice() else {
                return;
            };
            // A generic-context chain (pre-mono TypeVar) or an error-recovery
            // Unknown keeps the node — a helper fn cannot carry the enclosing
            // fn's type variables, and the existing paths still cover it.
            if payload_ty.has_unresolved_deep() || field_ty.has_unresolved_deep() {
                return;
            }
            // A HEAP field keeps the node: the helper's Option[heap]-returning
            // tail match walls on the v1 spine (see module docs) while the
            // existing route covers the corpus shapes — declining preserves
            // today's behavior for that class byte-for-byte.
            if is_heap_ty(field_ty) {
                return;
            }
            (expr.ty.clone(), payload_ty.clone(), field_ty.clone(), *field)
        };
        let res_ty = e.ty.clone();
        let span = e.span;

        // Synthesize `fn optional_chain_synth_N(s: Option[P]) -> Option[F] =
        //   match s { some(x) => some(x.field), none => none }`.
        // NOT `__`-prefixed: the codegen builtin-lowering pass rewrites every
        // `__`-prefixed Named CALL to a runtime intrinsic (`almide_rt_<name>`),
        // which would mismatch this real user-fn definition on the native path
        // (the branch_lift_synth naming precedent).
        let id = *self.counter;
        *self.counter = id + 1;
        let func_name = sym(&format!("optional_chain_synth_{}", id));
        let subj_var = self.vt.alloc(sym("ocs_subj"), subj_ty.clone(), Mutability::Let, span);
        let payload_var = self.vt.alloc(sym("ocs_payload"), payload_ty.clone(), Mutability::Let, span);

        let mk = |kind: IrExprKind, ty: Ty| IrExpr { kind, ty, span, def_id: None };
        let payload_read = mk(IrExprKind::Var { id: payload_var }, payload_ty.clone());
        let member = mk(IrExprKind::Member { object: Box::new(payload_read), field }, field_ty);
        let some_body = mk(IrExprKind::OptionSome { expr: Box::new(member) }, res_ty.clone());
        let none_body = mk(IrExprKind::OptionNone, res_ty.clone());
        let subj_read = mk(IrExprKind::Var { id: subj_var }, subj_ty.clone());
        let arms = vec![
            IrMatchArm {
                pattern: IrPattern::Some {
                    inner: Box::new(IrPattern::Bind { var: payload_var, ty: payload_ty }),
                },
                guard: None,
                body: some_body,
            },
            IrMatchArm { pattern: IrPattern::None, guard: None, body: none_body },
        ];
        let body = mk(IrExprKind::Match { subject: Box::new(subj_read), arms }, res_ty.clone());

        self.new_funcs.push(IrFunction {
            name: func_name,
            params: vec![IrParam {
                var: subj_var,
                ty: subj_ty,
                name: sym("ocs_subj"),
                borrow: ParamBorrow::Own,
                is_mut: false,
                open_record: None,
                default: None,
                attrs: vec![],
            }],
            ret_ty: res_ty.clone(),
            body,
            is_effect: false,
            is_test: false,
            generics: None,
            extern_attrs: vec![],
            export_attrs: vec![],
            attrs: vec![],
            visibility: IrVisibility::Private,
            doc: None,
            blank_lines_before: 0,
            def_id: None,
            mutated_params: vec![],
            module_origin: None,
        });

        // Replace the chain with `optional_chain_synth_N(<subject>)` — the
        // subject expression becomes the (single) argument, evaluated exactly
        // once in the same position it occupied before.
        let IrExprKind::OptionalChain { expr, .. } = std::mem::replace(
            &mut e.kind,
            IrExprKind::OptionNone,
        ) else {
            unreachable!("checked above");
        };
        e.kind = IrExprKind::Call {
            target: CallTarget::Named { name: func_name },
            args: vec![*expr],
            type_args: vec![],
        };
    }
}
