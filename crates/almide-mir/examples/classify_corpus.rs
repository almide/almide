//! Empirically verify the MIR-lowering WALL over the real v0 corpus — the
//! step-4 "continuous corpus verification = the definition of parity" gate, in
//! its honest first form. `proofs/corpus-wall.sh` drives this.
//!
//!   classify_corpus <file.almd | dir> ...
//!
//! For every function the frontend can hand to MIR lowering, `lower_function`
//! MUST be TOTAL: it returns `Ok(mir)` (in-profile) or `Err(Unsupported(reason))`
//! (explicitly walled). It must NEVER panic and never silently miscompile — that
//! is the wall the value-semantics subset stands behind, and this harness proves
//! it holds on real source, not just on hand-built MIR.
//!
//! Output split:
//!  - `--out DIR`: the witnesses of every IN-PROFILE function for ALL THREE
//!    proven properties, written as `.cert` files the kernel-proven checker
//!    re-verifies in one pass each:
//!      ownership.cert — one heap object per line (accept ⟹ no double-free/leak)
//!      names.cert     — one `defined|used` line per function (⟹ no dangling ref)
//!      caps.cert      — one `allowed|used` line per function (⟹ no undeclared
//!                       host effect)
//!    So accept ⟹ the FULL proven property set holds over the real corpus.
//!  - STDERR: the honest coverage report — files scanned, frontend-rejected,
//!    functions reaching MIR, in-profile count, and an Unsupported-reason
//!    histogram (so coverage growth is measurable per language feature).
//!
//! Exit code: non-zero iff `lower_function` PANICKED on any corpus function (a
//! wall breach to fix). Frontend rejects and explicit Unsupported are EXPECTED
//! and never fail the harness — they are the wall doing its job.

use almide_frontend::canonicalize;
use almide_frontend::check::Checker;
use almide_frontend::ir_link;
use almide_frontend::lower::lower_program;
use almide_lang::lexer::Lexer;
use almide_lang::parser::Parser;
use almide_ir::IrTypeDeclKind;
use almide_mir::certificate::{
    name_witness_string, ownership_certificate, program_cap_graph_witness, reachable_caps_or_tainted,
};
use almide_mir::{Capability, MirFunction, MirProgram, Op};
use almide_optimize::{mono, optimize};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};

/// Builtin free functions that reach STDERR / process-abort but NOT the modeled
/// `Stdout` capability (`assert*` print a diff to stderr then panic; `eprintln`
/// is stderr; `panic` aborts; `to_string` is pure). They are Stdout-free, so a
/// call to one cannot make a caller reach Stdout. NOTE: this is sound for the
/// CURRENT one-capability (Stdout-only) vocabulary — stderr/abort are real host
/// effects the model does not yet name (a wider Capability set is a later brick),
/// so the honest property is "no undeclared STDOUT effect", not "no host effect".
///
/// `__str_concat` is the self-host string-`+` runtime (`stdlib/string_concat.almd`):
/// `alloc_str(len a + len b)` then a recursive `prim.store8` byte-copy of both
/// halves — pure memory, no `fd_write`/Stdout. A deferred ConcatStr (a heap-result
/// match/if arm, an Opaque tail) surfaces as an elided `__str_concat` `Op::CallFn`
/// marker (`record_elided_calls`); admitting that name as Stdout-free lets the
/// enclosing function stay caps-VERIFIED instead of falsely tainting on an
/// "unanalyzable callee". SOUND: the concat reaches no Stdout, and its operands'
/// own calls are captured separately by the same marker pass.
const KNOWN_STDOUT_FREE_BUILTINS: &[&str] = &[
    "assert", "assert_eq", "assert_ne", "eprintln", "panic", "to_string", "__str_concat", "__list_concat", "option.unwrap_or_str",
];

/// Count call nodes (Call / RuntimeCall / TailCall) in an IR expression tree —
/// the SOURCE's call count. Compared to the MIR's call-op count to detect a call
/// ELIDED by Opaque lowering (a list element, ctor payload, BinOp operand): such
/// a call's effects are absent from the MIR, a caps blind spot the transitive
/// fold cannot see, so its function is conservatively tainted (not caps-verified).
/// How many synthetic eq CallFns a `left == right` of this type lowers to (mirrors
/// `lower_eq_typed`): String/Value/List → 1, a scalar → 0, a tuple/record → the SUM over its
/// fields (recursing). So `(String,Int)` `==` credits 1 (the string.eq), `(Int,Int)` credits 0 —
/// keeping `mir_calls <= ir_calls` exact for the field-wise compound eq.
fn count_eq_calls(
    ty: &almide_lang::types::Ty,
    registry: &almide_mir::lower::RecordLayouts,
    variant_layouts: &almide_mir::lower::VariantLayouts,
) -> usize {
    count_eq_calls_depth(ty, registry, variant_layouts, 0)
}

/// The STATIC synthetic-CallFn count `typed_slot_eq` emits for an `==` over `ty` — the
/// caps-gate credit for the operator node. Mirrors the engine EXACTLY (same recursion,
/// same MAX_EQ_DEPTH cap: a capped/unsupported shape emits nothing — its eq declines
/// and rolls back, so crediting 0 keeps `mir_calls <= ir_calls` by construction).
fn count_eq_calls_depth(
    ty: &almide_lang::types::Ty,
    registry: &almide_mir::lower::RecordLayouts,
    variant_layouts: &almide_mir::lower::VariantLayouts,
    depth: u32,
) -> usize {
    use almide_lang::types::{constructor::TypeConstructorId as TC, Ty};
    // Mirrors `typed_slot_eq`'s MAX_EQ_DEPTH (a recursive variant type would otherwise
    // recurse forever HERE too).
    if depth > 8 {
        return 0;
    }
    if matches!(ty, Ty::String) || almide_mir::lower::is_value_ty(ty) {
        return 1;
    }
    if let Ty::Applied(TC::List, es) = ty {
        // List[<custom variant>] — the synthesized loop-helper route (engine's
        // List-of-variant arm): the site's helper call + the generated bodies,
        // via the shared count (see synth_eq.rs; the site/list-body calls swap
        // roles vs a direct variant site, same total).
        if es.len() == 1 {
            let elem_variant = match &es[0] {
                Ty::Named(n, _) => Some(n.as_str().to_string()),
                Ty::Variant { name, .. } => Some(name.as_str().to_string()),
                Ty::Applied(TC::UserDefined(n), _) => Some(n.clone()),
                _ => None,
            };
            if let Some(n) = elem_variant {
                if variant_layouts.by_type.contains_key(&n) {
                    return almide_mir::lower::eq_helper_call_count(variant_layouts, &es[0]);
                }
            }
        }
        let nested_list = matches!(&es[..],
            [Ty::Applied(TC::List, inner)]
                if matches!(inner[..], [Ty::Int | Ty::Float | Ty::String]));
        // List[Option[Int/Bool]] — ONE list.eq_opt_int CallFn (the engine's
        // Option-element arm; Float payloads stay outside on both sides).
        let opt_scalar_elem = matches!(&es[..],
            [Ty::Applied(TC::Option, inner)]
                if matches!(inner[..], [Ty::Int | Ty::Bool]));
        return usize::from(
            es.len() == 1
                && (matches!(es[0], Ty::Int | Ty::String | Ty::Float | Ty::Bool)
                    || almide_mir::lower::is_value_ty(&es[0])
                    || nested_list
                    || opt_scalar_elem),
        );
    }
    // Map/Set `==` — the implemented repr variants lower to ONE synthetic eq CallFn
    // (`map.eq_ivh`/`map.eq_hval`/`map.eq_skv`/`set.eq_str`), all pure deep reads.
    // Mirror EXACTLY the operand shapes calls_p4's eq dispatch admits.
    if almide_mir::lower::is_map_ivh_ty(ty) || almide_mir::lower::is_map_hval_ty(ty) {
        return 1;
    }
    if let Ty::Applied(TC::Map, kv) = ty {
        if kv.len() == 2
            && matches!(kv[0], Ty::String)
            && !almide_mir::lower::is_heap_ty(&kv[1])
        {
            return 1;
        }
    }
    if let Ty::Applied(TC::Set, es) = ty {
        if es.len() == 1 && matches!(es[0], Ty::String) {
            return 1;
        }
    }
    // A custom VARIANT `==` — the tag-dispatched chain statically emits every fielded
    // case's per-field eq calls (only ONE case runs, but the caps witness is static).
    // Mirror `custom_variant_type_name`'s three type spellings.
    let variant_name: Option<String> = match ty {
        Ty::Named(n, _) => Some(n.as_str().to_string()),
        Ty::Variant { name, .. } => Some(name.as_str().to_string()),
        Ty::Applied(TC::UserDefined(n), _) => Some(n.clone()),
        _ => None,
    };
    if let Some(name) = variant_name {
        if let Some(layout) = variant_layouts.by_type.get(&name) {
            // A RECURSIVE variant routes through the synthesized helper family:
            // ONE site call + every generated helper body's static calls — the
            // SAME predicate + count the engine uses (synth_eq.rs), so mir == ir
            // holds by construction.
            if almide_mir::lower::variant_layout_recursive(variant_layouts, &name) {
                return almide_mir::lower::eq_helper_call_count(variant_layouts, ty);
            }
            return layout
                .cases
                .iter()
                .flat_map(|c| c.fields.iter())
                .map(|(_, t)| count_eq_calls_depth(t, registry, variant_layouts, depth + 1))
                .sum();
        }
    }
    match ty {
        Ty::Tuple(elems) => elems
            .iter()
            .map(|t| count_eq_calls_depth(t, registry, variant_layouts, depth + 1))
            .sum(),
        Ty::Record { fields } => fields
            .iter()
            .map(|(_, t)| count_eq_calls_depth(t, registry, variant_layouts, depth + 1))
            .sum(),
        Ty::Named(name, _args) => registry
            .get(name.as_str())
            .map(|(_, decl)| {
                decl.iter()
                    .map(|(_, t)| count_eq_calls_depth(t, registry, variant_layouts, depth + 1))
                    .sum()
            })
            .unwrap_or(0),
        // Option[heap] == recurses the payload's typed eq in the both-Some branch;
        // Option[scalar] == is branchless prims (0 calls).
        Ty::Applied(TC::Option, oa) if oa.len() == 1 => {
            count_eq_calls_depth(&oa[0], registry, variant_layouts, depth + 1)
        }
        // Result[scalar, String] == is the masked core: ONE string.eq (the Err compare).
        // Every other payload pair is the general core: BOTH gated payload compares are
        // statically present (ok-eq + err-eq), so credit their sum.
        Ty::Applied(TC::Result, ra) if ra.len() == 2 => {
            if !almide_mir::lower::is_heap_ty(&ra[0]) && matches!(ra[1], Ty::String) {
                1
            } else {
                count_eq_calls_depth(&ra[0], registry, variant_layouts, depth + 1)
                    + count_eq_calls_depth(&ra[1], registry, variant_layouts, depth + 1)
            }
        }
        _ => 0,
    }
}

fn count_ir_calls(
    body: &almide_ir::IrExpr,
    registry: &almide_mir::lower::RecordLayouts,
    variant_layouts: &almide_mir::lower::VariantLayouts,
) -> usize {
    struct CallCounter<'a> {
        n: usize,
        registry: &'a almide_mir::lower::RecordLayouts,
        variant_layouts: &'a almide_mir::lower::VariantLayouts,
    }
    impl almide_ir::visit::IrVisitor for CallCounter<'_> {
        fn visit_stmt(&mut self, s: &almide_ir::IrStmt) {
            // A map-insert STATEMENT `m[k] = v` rewrites to ONE synthetic `map.set`
            // CallFn (the functional rebind — mod_p3's MapInsert arm). Credit the
            // statement node so the synthetic call has a matching ir_call and
            // `mir_calls <= ir_calls` holds BY CONSTRUCTION. map.set is pure (a
            // COW block build, no Stdout), adding no real capability.
            if matches!(s.kind, almide_ir::IrStmtKind::MapInsert { .. }) {
                self.n += 1;
            }
            // A HEAP-element index-assign STATEMENT `xs[i] = "Z"` (String/Value
            // element — mod_p3's C-136 admission, mirrored exactly) rewrites to ONE
            // synthetic `list.set` CallFn (the functional rebind). Credit the
            // statement so the synthetic call has a matching ir_call and
            // `mir_calls <= ir_calls` holds BY CONSTRUCTION (the place_mutation
            // mir 19 > ir 18 breach). An un-admitted element class walls (no MIR
            // ops), where the extra credit only taints conservatively.
            if matches!(&s.kind, almide_ir::IrStmtKind::IndexAssign { value, .. }
                if matches!(value.ty, almide_lang::types::Ty::String)
                    || almide_mir::lower::is_value_ty(&value.ty))
            {
                self.n += 1;
            }
            almide_ir::visit::walk_stmt(self, s);
        }
        fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
            use almide_ir::IrExprKind::{Call, ClosureCreate, FnRef, RuntimeCall, TailCall};
            // A direct call is one ir_call. A FnRef / ClosureCreate passed to a pure
            // HOF is invoked by it — `lower_pure_module_call_args` emits ONE `Op::CallFn`
            // marker per such arg (a mir_call) to capture the closure's caps. Count those
            // function-reference nodes too, so a marker always has a matching ir_call and
            // `mir_calls <= ir_calls` holds BY CONSTRUCTION — not by the frontend happening
            // to eta-expand bare function-values to `Lambda` (which keeps them absent from
            // MIR input today). Without this, a FnRef over-count could cancel a Computed/
            // Method elision under-count, hiding a taint and falsely caps-verifying a fn.
            if matches!(e.kind, Call { .. } | RuntimeCall { .. } | TailCall { .. } | FnRef { .. } | ClosureCreate { .. })
                // A sized-int WIDENING conversion (`int8.to_int64(x)`) lowers to the
                // IDENTITY (no call op) — skip it by the SAME predicate the lowering
                // uses, so `mir == ir` holds by construction (its operand's own calls
                // are counted by the descent as usual).
                && almide_mir::lower::identity_int_widening_call(e).is_none()
                // `float.from_int` lowers to ONE F64FromInt prim (no call) — skip by
                // the SAME predicate the lowering uses (#806 step 2).
                && almide_mir::lower::float_from_int_prim_call(e).is_none()
            {
                self.n += 1;
            }
            // A string concat `a + b` (BinOp::ConcatStr) lowers to ONE synthetic `__str_concat`
            // CallFn (a mir_call). Count the operator NODE as one ir_call so that synthetic call
            // has a matching ir_call and `mir_calls <= ir_calls` holds BY CONSTRUCTION — a concat
            // not yet lowered in some position just leaves mir < ir (honest caps taint), never the
            // mir > ir over-count that would falsely caps-verify a fn. __str_concat is pure (the
            // transitive fold sees no Stdout), so the synthetic call adds no real capability.
            if matches!(&e.kind, almide_ir::IrExprKind::BinOp { op: almide_ir::BinOp::ConcatStr, .. }) {
                self.n += 1;
            }
            // A STRING equality `a == b` / `a != b` (BinOp::Eq/Neq over String operands) lowers
            // to ONE synthetic `string.eq` CallFn (the `!=` negate is a prim, not a call). Count
            // the operator NODE as one ir_call so the synthetic call has a matching ir_call and
            // `mir_calls <= ir_calls` holds BY CONSTRUCTION. Gated on a String LEFT operand — an
            // Int/Bool/Float `==` lowers to a prim compare (no call), so it is NOT counted.
            // `string.eq` is pure (byte compare, no Stdout), adding no real capability.
            // ALSO a Value `==`/`!=` (→ `value.eq`) and a List[Int|String|Value] `==`/`!=`
            // (→ `list.eq_int`/`list.eq_str`/`list.eq_value`) lower to ONE synthetic CallFn — count
            // the operator NODE for EXACTLY the operand shapes calls_p4 lowers to a call, so each has
            // a matching ir_call and `mir_calls <= ir_calls` holds BY CONSTRUCTION. All three eq
            // helpers are pure (deep reads, no Stdout), adding no real capability.
            if let almide_ir::IrExprKind::BinOp {
                op: almide_ir::BinOp::Eq | almide_ir::BinOp::Neq,
                left,
                ..
            } = &e.kind
            {
                // String/Value/List → 1 call; a tuple/record/variant → the SUM of its
                // fields' eq calls (recursing); Result general → ok+err; a scalar → 0.
                // Matches `typed_slot_eq`'s static per-field call emission exactly.
                self.n += count_eq_calls(&left.ty, self.registry, self.variant_layouts);
            }
            // A String ordering `< <= > >=` lowers to ONE `string.cmp` CallFn (then an Int compare
            // with 0, a prim). Credit the operator node so `mir_calls <= ir_calls` holds. string.cmp
            // is pure (byte compare, no Stdout).
            if let almide_ir::IrExprKind::BinOp {
                op: almide_ir::BinOp::Lt | almide_ir::BinOp::Lte | almide_ir::BinOp::Gt | almide_ir::BinOp::Gte,
                left,
                ..
            } = &e.kind
            {
                if matches!(left.ty, almide_lang::types::Ty::String) {
                    self.n += 1;
                }
            }
            // A STRING-subject `match s { "a" => .., "b" => .., _ => .. }` desugars to a nested
            // `if string.eq(s, "a") then .. else if string.eq(s, "b") then ..` — ONE synthetic
            // `string.eq` CallFn PER STRING-LITERAL arm (the catch-all/binder arm emits no call).
            // Count those arms here so the synthetic calls have matching ir_calls and
            // `mir_calls <= ir_calls` holds BY CONSTRUCTION. `string.eq` is pure (no Stdout).
            if let almide_ir::IrExprKind::Match { subject, arms } = &e.kind {
                if matches!(subject.ty, almide_lang::types::Ty::String) {
                    let lit_arms = arms
                        .iter()
                        .filter(|a| matches!(a.pattern, almide_ir::IrPattern::Literal { .. }))
                        .count();
                    self.n += lit_arms;
                }
            }
            // A list concat `a + b` (BinOp::ConcatList) lowers to ONE synthetic CallFn — `__list_concat`
            // (SCALAR-element, byte-copy) or `__list_concat_rc` (HEAP-element String/Value, rc-incrementing
            // copy). Count the operator NODE as one ir_call for EXACTLY the element shapes the lowering
            // emits a call for (scalar, or String/Value heap-element); a heap-FIELD aggregate element
            // (tuple/record) still DEFERS (no MIR call, no count). `mir_calls <= ir_calls` holds BY
            // CONSTRUCTION. Both concat runtimes are pure (prim memory ops, no Stdout).
            if let almide_ir::IrExprKind::BinOp { op: almide_ir::BinOp::ConcatList, .. } = &e.kind {
                // `try_lower_concat_list` emits AT MOST ONE synthetic `__list_concat`/`__list_concat_rc`
                // per ConcatList node (its operands materialize without their own concat call), and a
                // ConcatList it cannot lower WALLS the enclosing function (so that function is not
                // caps-checked). Therefore counting EVERY ConcatList node as one ir_call keeps
                // `mir_calls <= ir_calls` BY CONSTRUCTION for EVERY admitted element shape — scalar,
                // String/Value, the (String,String)/(Int,String)/(String,Value) tuples, List[List[String]],
                // a flat OR rich custom-variant element, and a recursive-drop RECORD element (the wasm
                // `acc + [{record}]` / `acc + [instr_r.val]` section-parser appends). A non-lowering
                // ConcatList just leaves mir < ir (honest caps taint), never the mir > ir over-count
                // that would falsely caps-verify a fn. Both concat runtimes are pure (no Stdout).
                let _ = (&self.registry, &self.variant_layouts);
                self.n += 1;
            }
            // The `**` OPERATOR (BinOp::PowFloat / PowInt) lowers to ONE synthetic CallFn —
            // `math.fpow` (the bit-exact libm pow) for Float, `math.pow` (int squaring) for Int.
            // Count the operator NODE as one ir_call so that synthetic call has a matching ir_call
            // and `mir_calls <= ir_calls` holds BY CONSTRUCTION (a `**` not yet lowered in some
            // position just leaves mir < ir — honest caps taint, never the mir > ir over-count that
            // would falsely caps-verify a fn). Both callees are PURE (math/math_fpow modules reach
            // no Stdout), so the synthetic call adds no real capability.
            if matches!(&e.kind, almide_ir::IrExprKind::BinOp {
                op: almide_ir::BinOp::PowFloat | almide_ir::BinOp::PowInt, ..
            }) {
                self.n += 1;
            }
            // A HEAP `Range` in a call-ARGUMENT position (`f(0..n)`) lowers to ONE synthetic
            // `list.range` CallFn (the materialized real list). Count the Range ARG node as one
            // ir_call so `mir_calls <= ir_calls` holds by construction; a Range the lowering
            // cannot materialize WALLS the function (never mir > ir). A `for i in 0..n` iterable
            // is NOT a call argument (no count — the loop lowers inline, no CallFn). list.range
            // is pure (no Stdout).
            if let almide_ir::IrExprKind::Call { args, .. } = &e.kind {
                self.n += args
                    .iter()
                    .filter(|a| {
                        matches!(&a.kind, almide_ir::IrExprKind::Range { .. })
                            && almide_mir::lower::is_heap_ty(&a.ty)
                    })
                    .count();
            }
            // A heap-String `??` (`Option[String] ?? default`) lowers to ONE synthetic
            // `option.unwrap_or_str` CallFn (a mir_call) — but ONLY when its operand can be
            // materialized as a self-host Option: a Var (possibly a materialized Option, which
            // lowers) or a direct self-host OPTION call (string.first / list.get / json.as_string
            // …). Count the operator node EXACTLY in those cases, so `mir_calls <= ir_calls` holds
            // by construction without over-tainting a `??` over a NON-Option operand (a `Result`
            // call like `value.as_string(x) ?? "?"`, or a not-yet-self-hosted `json.get_string`),
            // which does NOT lower to a call (mir+0). A scalar `??` (non-String fallback) is also
            // excluded (it lowers inline, no call). option.unwrap_or_str is pure (prim +
            // string.repeat, no Stdout), so the synthetic call adds no real capability.
            if let almide_ir::IrExprKind::UnwrapOr { expr, fallback } = &e.kind {
                // The operand must be an OPTION (not a Result — `value.as_string(x) ?? "?"` is a
                // Result `??`, expr.ty = Result, which the lowering does NOT route to
                // option.unwrap_or_str, so counting it would falsely taint mir<ir).
                let operand_is_option = matches!(
                    &expr.ty,
                    almide_lang::types::Ty::Applied(
                        almide_lang::types::constructor::TypeConstructorId::Option,
                        _
                    )
                );
                let operand_lowers = match &expr.kind {
                    almide_ir::IrExprKind::Var { .. } => true,
                    almide_ir::IrExprKind::Call {
                        target: almide_ir::CallTarget::Module { module, func, .. },
                        ..
                    } => {
                        almide_mir::lower::is_self_host_option_module_fn(module.as_str(), func.as_str())
                            // The NEW operand-materialization path (`process.env(k) ?? "/tmp"` — an
                            // IMPURE intrinsic `Option[String]`): it lowers to ONE synthetic
                            // `option.unwrap_or_str` CallFn, so credit the node +1.
                            || almide_mir::lower::unwrap_or_operand_admitted(expr)
                    }
                    _ => false,
                };
                if matches!(fallback.ty, almide_lang::types::Ty::String)
                    && operand_is_option
                    && operand_lowers
                {
                    self.n += 1;
                }
                // A Value / List[Value] -payload Result `??` (`value.get(o,k) ?? value.null()`,
                // `value.as_array(v) ?? []`) — the lowering routes it to ONE synthetic
                // result.value_unwrap_or / result.list_value_unwrap_or CALL (a pure value_core
                // helper, no Stdout). Count +1 so mir == ir, mirroring the String case.
                let operand_is_value_result = almide_mir::lower::is_value_result_ty(&expr.ty)
                    || almide_mir::lower::is_result_listval_ty(&expr.ty)
                    || almide_mir::lower::is_result_str_str_ty(&expr.ty)
                    || almide_mir::lower::is_option_value_ty(&expr.ty)
                    // An Option[List[Value]] / Option[List[String]] `??` (`json.get_array(v,k) ?? []`,
                    // `list.first_liststr(xs) ?? []`) routes to ONE synthetic option.listvalue_unwrap_or /
                    // option.liststr_unwrap_or CALL (pure value_core helpers, no Stdout) — count +1 so
                    // mir == ir, mirroring the Option[Value] case.
                    || almide_mir::lower::is_option_listvalue_ty(&expr.ty)
                    || almide_mir::lower::is_option_liststr_ty(&expr.ty)
                    // An Option[List[<scalar>]] `??` (`map.get(groups, k) ?? []` — B19's
                    // group_by class) routes to ONE synthetic option.listint_unwrap_or CALL —
                    // count +1 so mir == ir (the missing credit was the B19-ship breach the
                    // corpus gate caught: mir 2 > ir 1 on every listint `??` site).
                    || almide_mir::lower::is_option_listscalar_ty(&expr.ty);
                // value/list-Ok Result + Option[Value] Vars route (the handle Var-case admits
                // them) — INCLUDING a str-str Var, which now routes to `result.str_unwrap_or`
                // (the materialized_results_str Var-gate admission). An Option[Value] operand
                // is a self-host option CALL (list.get) or a Var.
                let value_operand_lowers = match &expr.kind {
                    almide_ir::IrExprKind::Var { .. } => true,
                    almide_ir::IrExprKind::Call {
                        target: almide_ir::CallTarget::Module { module, func, .. },
                        ..
                    } => {
                        almide_mir::lower::is_self_host_result_str_module_fn(
                            module.as_str(),
                            func.as_str(),
                        ) || ((almide_mir::lower::is_option_value_ty(&expr.ty)
                            || almide_mir::lower::is_option_listvalue_ty(&expr.ty)
                            || almide_mir::lower::is_option_liststr_ty(&expr.ty)
                            || almide_mir::lower::is_option_listscalar_ty(&expr.ty))
                            && almide_mir::lower::is_self_host_option_module_fn(
                                module.as_str(),
                                func.as_str(),
                            ))
                            // The NEW operand-materialization path (`json.parse(s) ?? json.array([])`
                            // — a PURE heap-`Result[Value, String]` module call): it lowers to ONE
                            // synthetic `result.value_unwrap_or` CallFn, so credit the node +1.
                            || almide_mir::lower::unwrap_or_operand_admitted(expr)
                    }
                    _ => false,
                };
                if operand_is_value_result
                    && almide_mir::lower::is_heap_ty(&fallback.ty)
                    && value_operand_lowers
                {
                    self.n += 1;
                }
            }
            // A STRING INTERPOLATION `"…${e}…"` desugars (the SHARED
            // `almide_mir::lower::desugar_string_interp`) to a synthetic `__str_concat`
            // chain (one per part, +1 empty-seed leaf ⇒ K concats) plus one
            // `<module>.to_string` per non-passthrough part (Int → int.to_string, Bool →
            // bool.to_string, Float/compound → <module>.to_string) — all MIR `Op::CallFn`s
            // SYNTHESIZED at lowering time (not present in this IR). Credit EXACTLY the call
            // NODES of that SAME desugared tree (`interp_str_synthetic_call_count`), so
            // `mir_calls == ir_calls` holds for the interp BY CONSTRUCTION — the gate counts
            // the identical tree the lowering emits. A NON-desugarable interp (a part with no
            // admitted `to_string` module — Tuple/Record/variant) is credited 0 here AND stays
            // the deferred `Alloc{Opaque}` in the lowering, so the count never over-credits an
            // interp that does not lower. Every synthetic callee is pure (no Stdout), adding no
            // real capability. The interp's OWN operand calls (a `${g(x)}` callee) are NOT
            // included by `interp_str_synthetic_call_count` (only the ConcatStr + to_string
            // wrappers), so the `walk_expr` descent below — which reaches each part's operand
            // expr — counts them exactly once, with no double-count.
            if let almide_ir::IrExprKind::StringInterp { parts } = &e.kind {
                self.n += almide_mir::lower::interp_str_synthetic_call_count(parts, self.registry);
            }
            almide_ir::visit::walk_expr(self, e);
        }
    }
    let mut cc = CallCounter { n: 0, registry, variant_layouts };
    // `visit_expr` (NOT `walk_expr`) so a ROOT-position call is counted too — an
    // expression-bodied `fn f() = g(x)` has the call AT the body root; `walk_expr`
    // would descend past it and undercount (masking a nested elision in its args,
    // e.g. `fn f() = g([h()])`). Counting the root keeps `mir_calls <= ir_calls`.
    almide_ir::visit::IrVisitor::visit_expr(&mut cc, body);
    cc.n
}

/// The NON-RECURRING soundness gate for the borrow-by-default calling convention:
/// EVERY `+1` event in the ownership certificate must be BACKED by a real runtime
/// op — an `i` by an `Alloc` or a heap-result call, an `a` by a `Dup`. A heap
/// parameter must therefore inject NO `+1` (an owned-param `+1` would be synthetic,
/// unbacked by any runtime `Alloc`/`rc_inc` — the gate-blind use-after-free class).
/// If a future lowering re-introduces an unbacked param `+1`, this equality breaks
/// and the corpus gate fails — making the class structurally impossible to ship.
fn plus_one_events_backed(mir: &MirFunction) -> bool {
    let cert = ownership_certificate(mir);
    let i = cert.chars().filter(|c| *c == 'i').count();
    let a = cert.chars().filter(|c| *c == 'a').count();
    // A rung-4 `ListLit` is alloc-class (the `Alloc{DynList}` it replaced) — it
    // backs its `i` exactly like an Alloc.
    let allocs = mir
        .ops
        .iter()
        .filter(|o| matches!(o, Op::Alloc { .. } | Op::ListLit { .. }))
        .count();
    let heap_results = mir
        .ops
        .iter()
        .filter(|o| match o {
            Op::Call { dst: Some(_), result: Some(r), .. }
            | Op::CallFn { dst: Some(_), result: Some(r), .. }
            // A heap-returning CallIndirect (a closure that moves out a fresh owned value)
            // backs an `i` exactly like a heap-returning CallFn — keep the gate consistent.
            | Op::CallIndirect { dst: Some(_), result: Some(r), .. } => r.is_heap(),
            _ => false,
        })
        .count();
    let dups = mir.ops.iter().filter(|o| matches!(o, Op::Dup { .. })).count();
    // A branch-merge dst's `i` (a RELEASED merge's moved-in reference, or a
    // slot-FEEDER merge's routed `i` — see ownership_certificate) is backed by
    // the arm value's real producer: the merge is a reference changing hands
    // (the wasm merge local.set), not a synthetic +1. The certificate module
    // itself counts the merges it credits so the two stay in lockstep by
    // construction.
    let merge_credits = almide_mir::certificate::merge_dst_i_credits(mir);
    i == allocs + heap_results + merge_credits && a == dups
}

/// Outcome of driving one `.almd` source through the frontend to linked IR.
enum FrontendOutcome {
    /// Reached linked IR — carries the functions MIR lowering will see.
    Ir(almide_ir::IrProgram),
    /// The frontend itself rejected (parse / type error) — its OWN wall, not
    /// MIR's. Out of scope for this gate, but counted for an honest picture.
    Rejected,
    /// The frontend PANICKED — a frontend-totality issue (separate layer; still
    /// surfaced so it is never invisible).
    Panicked,
}

/// Discover the input file's SIBLING `src/*.almd` modules so `import self.<submodule>`
/// resolves exactly as it does under `almide run`/`almide check` — reusing the CANONICAL
/// driver discovery (`almide::resolve::resolve_imports_with_deps`). Pure-local (empty
/// `dep_paths` ⇒ only project `self.*` siblings, no network). A NON-self-import file
/// returns an empty set ⇒ the original single-file path (byte-identical for the v0 corpus
/// / spec fixtures). Resolution failure (an unfetched external dep in the chain) ⇒ empty
/// set, so the sweep still classifies the file rather than aborting.
/// The cached dep source dirs for the project owning `path` — walk up to its `almide.toml`,
/// parse it, `fetch_all_deps` (cache-hit ⇒ no network; the SAME computation `almide` runs).
/// Empty when no project / no deps / fetch failure (graceful: an unresolved external import
/// then walls honestly). Lets a file importing an EXTERNAL package (`import almai`) resolve
/// here as under `almide run`, instead of being skipped as an unmeasurable xmod under-count.
fn dep_paths_for(path: &Path) -> Vec<(almide::project::PkgId, std::path::PathBuf)> {
    let mut dir = path.parent();
    while let Some(d) = dir {
        let toml = d.join("almide.toml");
        if toml.exists() {
            if let Ok(proj) = almide::project::parse_toml(&toml) {
                if let Ok(deps) = almide::project_fetch::fetch_all_deps(&proj) {
                    return deps.into_iter().map(|fd| (fd.pkg_id, fd.source_dir)).collect();
                }
            }
            return Vec::new();
        }
        dir = d.parent();
    }
    Vec::new()
}

include!("classify_corpus_parts/classify_corpus_b.rs");
include!("classify_corpus_parts/classify_corpus_c.rs");
