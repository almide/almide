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

fn discover_self_modules(
    path: &Path,
    prog: &almide_lang::ast::Program,
) -> Vec<(String, almide_lang::ast::Program, bool)> {
    // Resolve a `self.<submodule>` OR an EXTERNAL (non-stdlib) package import — so a
    // cross-module file works under `almide run`-equivalent resolution (incl. fetched deps),
    // not just self-imports. A lone / stdlib-only file stays a strict no-op.
    let needs_resolve = prog.imports.iter().any(|d| {
        matches!(d, almide_lang::ast::Decl::Import { path, .. }
            if path.first().map(|s| {
                let s = s.as_str();
                s == "self" || !almide_lang::stdlib_info::is_stdlib_module(s)
            }).unwrap_or(false))
    });
    if !needs_resolve {
        return Vec::new();
    }
    let deps = dep_paths_for(path);
    match almide::resolve::resolve_imports_with_deps(&path.to_string_lossy(), prog, &deps) {
        Ok(resolved) => resolved
            .modules
            .into_iter()
            .map(|(name, p, _pkg, is_self)| (name, p, is_self))
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// The mangled flat name a user-module function gets when resolved to a user `CallFn`
/// (`bindgen` + `get_str` → `almide_rt_bindgen_get_str`) — the v1 analogue of v0's
/// `ir_link_flatten` module-fn renaming.
fn user_module_fn_name(module: &str, func: &str) -> String {
    format!("almide_rt_{}_{}", module.replace('.', "_"), func.replace('.', "_"))
}

/// Build the set of NATIVE-FFI function keys over the LINKED IR: functions that TRANSITIVELY
/// reach a STRUCTURAL native root — a root being EITHER an `@extern(rust/rs)` declaration (no
/// wasm form: porta `wasm_rt.almd` `wt_*`, almide-sqlite `native_*`) OR a DIRECT call to a
/// permanently-no-wasm stdlib effect (`net.*`, `process.exec/exit/run`, `http.request`). Such a
/// function can NEVER lower to wasm; its wall is STRUCTURAL, not a v1 lowering gap, so it must be
/// EXCLUDED from the wall=0 metric exactly like the already-excluded `@extern(wasm)` WASI imports
/// (those lower via `extern_wasm_target` and never wall — they are not roots here).
///
/// SOUNDNESS (do-NOT-over-exclude): ONLY those two root shapes seed the set. A function that walls
/// on a PURE lowering gap (heap-result return, etc.) and never transitively reaches a root stays
/// REAL — its gap is NOT hidden. `process.args` is deliberately EXCLUDED from roots (WASI
/// `args_get` exists ⇒ uncertain ⇒ conservative REAL). `CallTarget::Method`/`Computed` are
/// unresolved ⇒ no edge, no root (conservative REAL). Bare `FnRef`/`ClosureCreate` references are
/// NOT root contributions.
///
/// Keys match the edge names `resolve_user_module_calls` produced: file functions (`ir.functions`)
/// by plain name; module functions (`ir.modules[].functions`) by `user_module_fn_name(module, func)`
/// — exactly the `CallTarget::Named` a resolved user-module call carries, so edges resolve.
fn compute_native_ffi_set(ir: &almide_ir::IrProgram) -> HashSet<String> {
    use almide_ir::CallTarget;
    // 1) NODE KEYS over BOTH function sets, paired with the body to scan for edges/roots.
    let mut nodes: Vec<(String, &almide_ir::IrFunction)> = Vec::new();
    for f in &ir.functions {
        nodes.push((f.name.as_str().to_string(), f));
    }
    for m in &ir.modules {
        for f in &m.functions {
            nodes.push((user_module_fn_name(m.name.as_str(), f.name.as_str()), f));
        }
    }
    // Root-(a): an `@extern` decl whose target is `rust`/`rs`/`c` (NOT `wasm`/`ts` — those lower
    // to a WASI/browser import via `extern_wasm_target` and never wall; this exclusion is the
    // precedent). `c` is a C-library link (`@extern(c, "m", "sqrt")` — extern_c_test, whose header
    // says "wasm:skip — @extern(c) not available in WASM"): structurally native, same as rust/rs.
    let is_native_extern = |f: &almide_ir::IrFunction| {
        f.extern_attrs.iter().any(|a| matches!(a.target.as_str(), "rust" | "rs" | "c"))
    };
    // Per-body collector: forward `Named` call edges + whether the body DIRECTLY calls an
    // enumerated permanently-no-wasm stdlib effect (root-(b)). A remaining `CallTarget::Module`
    // is ALWAYS a stdlib call here (user modules were already rewritten to `Named` by
    // `resolve_user_module_calls`), so this matches only the genuine stdlib effect roots.
    struct Collector {
        edges: Vec<String>,
        native_call: bool,
    }
    impl almide_ir::visit::IrVisitor for Collector {
        fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
            use almide_ir::IrExprKind::{Call, TailCall};
            let target = match &e.kind {
                Call { target, .. } | TailCall { target, .. } => Some(target),
                _ => None,
            };
            if let Some(t) = target {
                match t {
                    CallTarget::Named { name } => self.edges.push(name.as_str().to_string()),
                    CallTarget::Module { module, func, .. } => {
                        let (m, fname) = (module.as_str(), func.as_str());
                        // net.* (any func) / the no-wasm process fns / http.request / zlib.* —
                        // the tight, enumerated no-wasm set. process.args is EXCLUDED (WASI
                        // args_get exists AND v0's emit_wasm implements it — calls_process.rs
                        // handles exactly exit/stdin_lines/args). spawn/kill/is_alive/
                        // exec_status/env have NO v0 wasm form (the fixture headers declare
                        // them native-only: "wasm:skip — process.env/spawn/kill are
                        // native-only" / "process.exec_status is native-only"), and WASI
                        // preview1 has no child-process API — structural, not a v1 gap.
                        // zlib has NO v0 wasm runtime at all ("wasm:skip — OS/native-only").
                        // http.serve is a TCP LISTENER ("wasm:skip — http.serve is native-only",
                        // effect_intrinsic_tail_test) — the same no-wasm class as net.*.
                        // random is deliberately NOT here: v0's emit_wasm implements it over
                        // WASI random_get (calls_random.rs) — a REAL v1 gap.
                        // testing.assert_throws needs `std::panic::catch_unwind` (runtime/rs/src/
                        // testing.rs) — WASM's `unreachable` trap is NOT catchable (no unwind
                        // mechanism in the WASI MVP ABI v0 targets). v0's OWN emit_wasm has no
                        // wasm form either (calls_p2.rs's `assert_throws` arm is native-only) —
                        // the fixture header says so verbatim ("wasm:skip — WASM cannot catch
                        // panics"), matching CHANGELOG.md and wasm_dispatch_coverage_test.rs's
                        // independent documentation of the same limitation. Structural, not a v1
                        // lowering gap — the E3/E4/E5 native-root precedent.
                        if m == "net"
                            || m == "zlib"
                            || (m == "process"
                                && matches!(
                                    fname,
                                    "exec" | "exit" | "run" | "spawn" | "kill" | "is_alive"
                                        | "exec_status" | "env"
                                ))
                            || (m == "http" && matches!(fname, "request" | "serve"))
                            || (m == "testing" && fname == "assert_throws")
                        {
                            self.native_call = true;
                        }
                    }
                    // Method / Computed: unresolved → no edge, no root (conservative REAL).
                    _ => {}
                }
            }
            almide_ir::visit::walk_expr(self, e);
        }
    }
    // 2/3) Seed roots + record forward edges.
    let mut native: HashSet<String> = HashSet::new();
    let mut fwd: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (key, f) in &nodes {
        let mut c = Collector { edges: Vec::new(), native_call: false };
        almide_ir::visit::IrVisitor::visit_expr(&mut c, &f.body);
        if is_native_extern(f) || c.native_call {
            native.insert(key.clone());
        }
        fwd.insert(key.clone(), c.edges);
    }
    // 4) PROPAGATE callee→caller to a fixpoint: a caller of a native fn is itself native.
    loop {
        let mut changed = false;
        for (caller, callees) in &fwd {
            if native.contains(caller) {
                continue;
            }
            if callees.iter().any(|c| native.contains(c)) {
                native.insert(caller.clone());
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    native
}

/// Resolve a USER-package/-module call (`bindgen.get_str` via `import self as bindgen`,
/// `self.classifier.classify`) to a real user `CallFn` (`CallTarget::Module` → `Named`,
/// `almide_rt_<m>_<f>`). The MIR lowering then treats it as an ordinary user call instead of
/// walling it as an opaque "impure stdlib Module" call. SOUNDNESS (caps): the resolved name
/// carries NO dot, so the transitive caps gate analyzes it as a user call (via the in-profile
/// map, or TAINTS the caller if the callee is a cross-file/unanalyzable definition) — NOT as a
/// pure dotted stdlib call (`is_known_free`). A self-pkg call to an EFFECTFUL user fn thus
/// surfaces its capability transitively, exactly like any user call — never the
/// accept-but-unsafe omission the Module-call purity wall guarded against. STDLIB modules are
/// NOT rewritten. No-op when there are no linked user modules.
fn resolve_user_module_calls(ir: &mut almide_ir::IrProgram) {
    use almide_ir::{CallTarget, IrExprKind, IrMutVisitor, walk_expr_mut};
    use almide_lang::intern::sym;
    let user_mods: BTreeMap<String, HashSet<String>> = ir
        .modules
        .iter()
        .filter(|m| !almide_lang::stdlib_info::is_any_stdlib(m.name.as_str()))
        .map(|m| {
            (
                m.name.as_str().to_string(),
                m.functions.iter().map(|f| f.name.as_str().to_string()).collect(),
            )
        })
        .collect();
    if user_mods.is_empty() {
        return;
    }
    struct Rw<'a> {
        user_mods: &'a BTreeMap<String, HashSet<String>>,
    }
    impl IrMutVisitor for Rw<'_> {
        fn visit_expr_mut(&mut self, e: &mut almide_ir::IrExpr) {
            walk_expr_mut(self, e);
            if let IrExprKind::Call { target, .. } = &mut e.kind {
                if let CallTarget::Module { module, func, .. } = target {
                    let (m, f) = (module.as_str(), func.as_str());
                    if self.user_mods.get(m).is_some_and(|fs| fs.contains(f)) {
                        *target = CallTarget::Named { name: sym(&user_module_fn_name(m, f)) };
                    }
                }
            }
        }
    }
    let mut rw = Rw { user_mods: &user_mods };
    for func in &mut ir.functions {
        rw.visit_expr_mut(&mut func.body);
    }
    for tl in &mut ir.top_lets {
        rw.visit_expr_mut(&mut tl.value);
    }
    for m in &mut ir.modules {
        for func in &mut m.functions {
            rw.visit_expr_mut(&mut func.body);
        }
        for tl in &mut m.top_lets {
            rw.visit_expr_mut(&mut tl.value);
        }
    }
}

/// Drive source → linked IR with NO `die()` — every failure becomes a value, so
/// the sweep never aborts on a single bad file. Mirrors `emit_cert_from_source`'s
/// pipeline (the same public frontend functions almide-interp uses). `path` is the
/// file's location so `import self.<submodule>` resolves its sibling `src/*.almd`
/// (the canonical driver discovery) — exactly the cut point `render_program` uses, so
/// the wall report counts a cross-module function's REAL lowerability, not the
/// missing-sibling artifact. A lone single file (no `import self.*`) is unchanged.
fn source_to_ir(path: &Path, source: &str) -> FrontendOutcome {
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<almide_ir::IrProgram, String> {
        let tokens = Lexer::tokenize(source);
        let mut parser = Parser::new(tokens);
        let mut prog = parser.parse().map_err(|e| format!("parse error: {e:?}"))?;
        if !parser.errors.is_empty() {
            return Err(format!("parse errors: {:?}", parser.errors));
        }
        let modules = discover_self_modules(path, &prog);
        let canon = canonicalize::canonicalize_program(
            &prog,
            modules.iter().map(|(n, p, s)| (n.as_str(), p, *s)),
        );
        let mut checker = Checker::from_env(canon.env);
        let diags = checker.infer_program(&mut prog);
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.level == almide_frontend::diagnostic::Level::Error)
            .map(|d| d.message.clone())
            .collect();
        if !errors.is_empty() {
            return Err(format!("type errors ({} diag)", errors.len()));
        }
        let mut ir = lower_program(&prog, &checker.env, &checker.type_map);
        // Lower each resolved sibling MODULE into `ir.modules` — the SAME sequence the real
        // driver runs (infer_module → per-module import table → lower_module → push), so a
        // cross-module record/variant type reaches `build_record_layouts`. Non-bundled stdlib
        // modules are skipped (their defs come from the runtime registry, not user lowering).
        for (name, mod_prog, _is_self) in &modules {
            if almide_lang::stdlib_info::is_stdlib_module(name)
                && !almide_lang::stdlib_info::is_bundled_module(name)
            {
                continue;
            }
            let mut mod_prog = mod_prog.clone();
            checker.infer_module(&mut mod_prog, name);
            let self_name = checker.env.self_module_name.map(|s| s.to_string());
            let import_table_name = self_name.as_deref().unwrap_or(name.as_str());
            let (mod_table, _) = almide_frontend::import_table::build_import_table(
                &mod_prog,
                Some(import_table_name),
                &checker.env.user_modules,
            );
            let saved_table = std::mem::replace(&mut checker.env.import_table, mod_table);
            let mod_ir = almide_frontend::lower::lower_module(
                name,
                &mod_prog,
                &checker.env,
                &checker.type_map,
                None,
            );
            checker.env.import_table = saved_table;
            ir.modules.push(mod_ir);
        }
        // Resolve self-pkg / imported user-module calls to real user CallFns (Module → Named,
        // `almide_rt_<m>_<f>`), so the MIR lowering treats them as ordinary user calls instead of
        // walling them as opaque "impure stdlib Module" calls. SOUNDNESS: the resolved name has no
        // dot, so the transitive caps gate analyzes it as a user call (in-profile map / taint),
        // NOT a pure dotted stdlib call — a self-pkg call to an effectful user fn surfaces its
        // capability transitively. No-op when there are no linked user modules.
        resolve_user_module_calls(&mut ir);
        optimize::optimize_program(&mut ir);
        mono::monomorphize(&mut ir);
        ir_link::ir_link(&mut ir);
        // Transparent-newtype erasure LAST (post-link, pre-lowering) — the SAME pass the
        // pipeline runs, so the caps mir == ir count sees the erased tree on both sides.
        almide_mir::lower::erase_transparent_newtypes(&mut ir);
        almide_mir::lower::fill_record_defaults(&mut ir);
        almide_mir::lower::inline_pure_call_globals(&mut ir);
        // C-132 move-mode write-back — the SAME pre-lowering rewrite the pipeline
        // runs (see source_to_ir_with), so mir == ir on both sides.
        almide_ir::mut_param::lower_mut_params_move_mode(&mut ir);
        // Guard → if restructure — the SAME pre-lowering pass the pipeline runs.
        almide_mir::lower::desugar_fn_body_guards(&mut ir);
        almide_mir::lower::normalize_tail_err_raise_ifs(&mut ir);
        almide_mir::lower::hoist_block_call_args(&mut ir);
        almide_mir::lower::desugar_loop_early_returns(&mut ir);
        almide_mir::lower::hoist_spread_call_bases(&mut ir);
        almide_mir::lower::hoist_record_literal_args(&mut ir);
        Ok(ir)
    }));
    match result {
        Ok(Ok(ir)) => FrontendOutcome::Ir(ir),
        Ok(Err(_reason)) => FrontendOutcome::Rejected,
        Err(_) => FrontendOutcome::Panicked,
    }
}

/// Count the `StringInterp` SITES in a body, split into `(proven, walled)`:
///   - PROVEN (a): the interp DESUGARS and EVERY synthetic `<module>.to_string` it
///     introduces is LINKABLE (in the self-host registry — `int`/`bool`). It folds to a
///     fully-resolvable `__str_concat` / `int.to_string` / `bool.to_string` chain that
///     renders to valid wasm and byte-matches v0.
///   - WALLED (b): the interp is NON-desugarable (a part with no admitted `to_string`
///     module ⇒ stays Opaque), OR it desugars but a synthetic `to_string` is UNLINKED
///     (Float/compound — `float.to_string`/`list.to_string` are not self-hosted), so the
///     enclosing function emits an unlinked call and the render wall rejects it. Either
///     way: no invalid wasm. (The unlinked-call OCCURRENCES are also folded into (b) by
///     the per-CallFn loop at the lowering site; this only counts the interp SITE once, as
///     walled, so a Float interp does not get mis-bucketed as proven.)
fn count_interp_sites(
    body: &almide_ir::IrExpr,
    linkable: &HashSet<String>,
    registry: &almide_mir::lower::RecordLayouts,
) -> (usize, usize) {
    struct InterpCounter<'a> {
        proven: usize,
        walled: usize,
        linkable: &'a HashSet<String>,
        registry: &'a almide_mir::lower::RecordLayouts,
    }
    impl almide_ir::visit::IrVisitor for InterpCounter<'_> {
        fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
            if let almide_ir::IrExprKind::StringInterp { parts } = &e.kind {
                // PROVEN iff desugarable AND every synthetic to_string is linkable. The
                // synthetic-name list is empty for a non-desugarable interp (so the all()
                // is vacuously true) — guard on desugarability explicitly. Only the dotted
                // `<module>.to_string` names matter for linkability (`__str_concat` is
                // always registered); checking the full list is sound (it contains it).
                let desugarable = almide_mir::lower::interp_str_desugarable(parts, self.registry);
                let all_linkable = almide_mir::lower::interp_synthetic_call_names(parts, self.registry)
                    .iter()
                    .all(|n| !n.contains('.') || self.linkable.contains(n));
                if desugarable && all_linkable {
                    self.proven += 1;
                } else {
                    self.walled += 1;
                }
            }
            almide_ir::visit::walk_expr(self, e);
        }
    }
    let mut c = InterpCounter { proven: 0, walled: 0, linkable, registry };
    almide_ir::visit::IrVisitor::visit_expr(&mut c, body);
    (c.proven, c.walled)
}

/// Group an `Unsupported` reason into a stable histogram key: the leading clause
/// before the first variable-debug fragment (`:`, `(`, `{`). Keeps "no scalar
/// Repr for Named { .. }" and "no scalar Repr for Tuple [..]" in one bucket, so
/// the histogram tracks language FEATURES, not incidental type spellings.
fn reason_key(reason: &str) -> String {
    reason
        .split(|c| c == ':' || c == '(' || c == '{')
        .next()
        .unwrap_or(reason)
        .trim()
        .to_string()
}

#[derive(Default)]
struct Tally {
    files: usize,
    frontend_rejected: usize,
    frontend_panicked: usize,
    functions: usize,
    in_profile: usize,
    unsupported: BTreeMap<String, usize>,
    /// Walled functions split by category (the honest wall=0 end state):
    ///   - `walled_real`: a pure/WASI-able function the v1 compiler cannot YET lower — THIS is the
    ///     number the lowering-wall=0 goal drives to 0.
    ///   - `walled_native_ffi`: a function that transitively reaches a STRUCTURAL native root
    ///     (`@extern(rust/rs)` or a no-wasm stdlib effect) — can NEVER lower to wasm; EXCLUDED from
    ///     the wall=0 metric (like the `@extern(wasm)` WASI imports already are). NOT a lowering bug.
    /// Their sum equals the total `unsupported` count.
    walled_real: usize,
    walled_native_ffi: usize,
    lower_panics: Vec<String>,
    /// Functions whose certificate has an UNBACKED `+1` (the borrow-by-default
    /// soundness gate). Must stay empty — a non-empty list is a wall breach.
    cert_backing_breaches: Vec<String>,
    /// Functions whose MIR call-op count EXCEEDS their source call-node count — the
    /// caps-soundness gate for the elided-call effect markers (`record_elided_calls`).
    /// A marker may only ADD a call-op for a genuinely ELIDED call, so `mir_calls`
    /// can rise at most TO `ir_calls`; `mir_calls > ir_calls` means a marker
    /// DOUBLE-COUNTED a call already lowered — which could mask a real elision and
    /// FALSELY de-taint a Stdout-reaching function. Must stay empty (a wall breach).
    call_count_breaches: Vec<String>,
    /// In-profile functions whose Stdout-freedom is provable transitively (every
    /// `Op::CallFn` callee is Stdout-free): their empty capability witness is
    /// emitted for the proven checker. accept ⟹ no undeclared Stdout effect.
    caps_verified: usize,
    /// In-profile functions that call an UNANALYZABLE callee (a walled or
    /// cross-file user function), so their Stdout-freedom cannot be proven; their
    /// witness is NOT emitted (honest: not falsely claimed caps-safe).
    caps_unverified: usize,

    // ── interp / call-link coverage visibility metric (a/b/c) ────────────────────
    // Every StringInterp SITE + every dotted stdlib CallFn in the corpus, bucketed
    // HONESTLY against what the render-side wall (`try_render_wasm_program`) guarantees:
    /// (a) PROVEN: a lowerable interp in a lowered function — it folds to a registered
    /// `__str_concat` / `int.to_string` chain (byte-match v0 by the render_wasm
    /// detectors). The interp's synthetic to_string callee (`int.to_string`) is in the
    /// registry, so it renders to VALID wasm. (Interp sites only; the link metric below
    /// covers the broader unlinkable-stdlib-call surface.)
    interp_lowered: usize,
    /// (b) cleanly WALLED — acceptable (no output, but never invalid wasm). Three honest
    /// sources, all caught BEFORE any invalid module ships:
    ///   • a non-subset interp stays the sound Opaque fallback (its fn lowers, the interp
    ///     just defers — no synthetic call emitted);
    ///   • an interp inside a function the lowering returned `Unsupported` for;
    ///   • a function that emits an UNLINKABLE dotted CallFn (a stdlib fn with no
    ///     self-host registry definition, e.g. `string.to_upper`): the render-side
    ///     `try_render_wasm_program` REJECTS the whole program with `LowerError::
    ///     Unsupported` rather than emit a dangling `(call $…)`. Conservative, loud, sound.
    interp_walled: usize,
    /// The DISTINCT unlinkable dotted stdlib callees seen across the corpus — the visible
    /// "would be walled at render" frontier (the remaining self-host-registry gap). Folded
    /// into bucket (b): every using program is cleanly REJECTED by the render wall, never
    /// silently miscompiled. This is a MEASUREMENT (the gap is visible every build), NOT a
    /// soundness breach — the wall converts each of these from a (c) hole into a (b) reject.
    would_wall_callees: BTreeMap<String, usize>,
    /// (c) FORBIDDEN — a silent-miscompile / invalid-wasm-passing-as-Ok: a call that
    /// renders to a dangling `(call $…)` AND is presented as `Ok`. The render-side wall
    /// (`try_render_wasm_program`) catches EVERY unlinkable `Op::CallFn` before output, so
    /// no such site can escape to an `Ok` invalid module — this is 0 BY CONSTRUCTION. The
    /// list records any site that would escape the wall (a wall-completeness breach); it
    /// MUST stay empty.
    forbidden_unwalled: Vec<String>,
}

/// All self-host registry CALL-NAMES (the `module.func` a `CallFn` resolves to once the
/// v1 linker auto-includes + renames the impl) plus the always-linked `print_str`. A
/// `CallFn` to any of these is render-resolvable. Built once from the single-source
/// registry so it can never drift from what render_program / lower_source actually link.
fn auto_linkable_call_names() -> HashSet<String> {
    let mut s: HashSet<String> = almide_mir::render_wasm::self_host_runtime()
        .iter()
        .flat_map(|(_, entries)| entries.iter().map(|(_, call)| call.to_string()))
        .collect();
    s.insert("print_str".to_string());
    s
}

/// Recursively collect `.almd` files under a path (file or directory).
fn collect_almd(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        let mut entries: Vec<_> = match std::fs::read_dir(path) {
            Ok(rd) => rd.filter_map(|e| e.ok().map(|e| e.path())).collect(),
            Err(_) => return,
        };
        entries.sort();
        for e in entries {
            collect_almd(&e, out);
        }
    } else if path.extension().is_some_and(|x| x == "almd") {
        out.push(path.to_path_buf());
    }
}

/// One corpus file's classification (the body of main's per-file loop —
/// decomposed #781, cog 199; early `continue`s became early `return`s).
fn classify_file(
    file: &std::path::Path,
    t: &mut Tally,
    s: &mut CertStreams,
    auto_linkable: &std::collections::HashSet<String>,
) {
    t.files += 1;
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(_) => {
            t.frontend_rejected += 1;
            return;
        }
    };
    let ir = match source_to_ir(file, &source) {
        FrontendOutcome::Ir(ir) => ir,
        FrontendOutcome::Rejected => {
            t.frontend_rejected += 1;
            return;
        }
        FrontendOutcome::Panicked => {
            t.frontend_panicked += 1;
            return;
        }
    };

    // The NATIVE-FFI closure over the linked IR (transitive over `@extern(rust/rs)` + the
    // enumerated no-wasm stdlib effects). A WALLED function in this set is a STRUCTURAL wall
    // (no wasm host equivalent), tagged NATIVE-FFI and excluded from the wall=0 metric; every
    // other wall is a REAL lowering gap. Computed once per file before the lowering loop.
    let native_ffi_set = compute_native_ffi_set(&ir);

    // Variant constructors of this program are PURE data builders (no host
    // effect), so a `CallFn` to one is Stdout-free — collected for the
    // capability soundness fold below.
    let ctors: HashSet<String> = ir
        .type_decls
        .iter()
        .flat_map(|td| match &td.kind {
            IrTypeDeclKind::Variant { cases, .. } => {
                cases.iter().map(|c| c.name.as_str().to_string()).collect::<Vec<_>>()
            }
            _ => Vec::new(),
        })
        .collect();

    // Pass 1: lower every function; emit the LOCAL witnesses (ownership, names)
    // and collect the in-profile MIRs (the capability fold needs the whole
    // file's in-profile set before it can judge any one function's callees).
    let mut file_mirs: Vec<(String, MirFunction)> = Vec::new();
    // In-profile functions whose source had a call ELIDED by Opaque lowering —
    // their capability witness is incompletely captured, so the caps fold below
    // conservatively taints them (and their callers).
    let mut elided_call_fns: HashSet<String> = HashSet::new();
    // The module's top-level `let` globals (VarId -> declared Ty): a function that
    // references one resolves to no function-local binding, so the lowering needs
    // this DECLARED set to admit the reference (`value_or_global`) instead of
    // walling it. Union of program- and module-level top_lets.
    let mut globals: std::collections::HashMap<almide_ir::VarId, almide_lang::types::Ty> =
        std::collections::HashMap::new();
    // The globals' INITIALIZERS too, so the gate VERIFIES the same heap-global materialization
    // render_program executes (a heap global lowers to its real const value, not a wall).
    let mut global_inits: std::collections::HashMap<almide_ir::VarId, almide_ir::IrExpr> =
        std::collections::HashMap::new();
    for tl in &ir.top_lets {
        globals.insert(tl.var, tl.ty.clone());
        global_inits.insert(tl.var, tl.value.clone());
    }
    // MAIN-REGION precedence (this loop lowers the MAIN program's fns only): the raw
    // module union above stays as the FALLBACK (a shared-allocator id matches it — the
    // init_order shapes), the cross-module NAME bridge OVERRIDES a colliding module-raw
    // key (the byvalue shapes), and main's own top-lets (re-inserted last) win where the
    // name bridge would misfire — composition order: module union → bridge → main.
    almide_mir::lower::bridge_cross_module_toplets(&ir, &mut globals, &mut global_inits, &mut std::collections::HashMap::new());
    for tl in &ir.top_lets {
        globals.insert(tl.var, tl.ty.clone());
        global_inits.insert(tl.var, tl.value.clone());
    }
    // MUTABLE module-level `var`s route through their storage slots — publish the
    // VarId → (slot, Ty) map exactly as the render pipeline does (declaration order
    // = VarId order), so the gate classifies with the same slot lowering the real
    // emit performs (and the same walls for shapes beyond the slot subset).
    let mut mutable_tls: Vec<_> = ir
        .top_lets
        .iter()
        .chain(ir.modules.iter().flat_map(|m| m.top_lets.iter()))
        .filter(|tl| tl.mutable)
        .collect();
    mutable_tls.sort_by_key(|tl| tl.var.0);
    almide_mir::lower::set_mutable_global_vars(
        mutable_tls
            .iter()
            .enumerate()
            .map(|(i, tl)| (tl.var.0, (i as u32, tl.ty.clone())))
            .collect(),
    );
    // The functions DEFINED in this file (their names). A PROTOCOL METHOD is a
    // user-defined function whose name is dotted (`Type.method`, e.g. `MathExpr.eval`)
    // — it resolves to ITSELF / a sibling method, NOT a stdlib call. The unlinkable-
    // stdlib detector must exclude these (a dotted name is unlinkable only if it is also
    // NOT a function defined here), else a self-recursive method call falsely flags.
    let file_fn_names: HashSet<String> =
        ir.functions.iter().map(|f| f.name.as_str().to_string()).collect();
    // The record-layout registry (type name → fields) for the VALUE MODEL, so the
    // corpus-wall exercises (and the proven checker re-verifies) record/`r.x`
    // materialization over the whole v0 corpus, not just the structurally-typed forms.
    let mut record_layouts = almide_mir::lower::build_record_layouts(&ir.type_decls);
    for m in &ir.modules {
        record_layouts.extend(almide_mir::lower::build_record_layouts(&m.type_decls));
    }
    // The variant-layout registry (custom ADTs) — the value-model sibling of
    // `record_layouts`, so the corpus-wall exercises variant construct / `match` too.
    let mut variant_layouts = almide_mir::lower::build_variant_layouts(&ir.type_decls);
    for m in &ir.modules {
        let m_vl = almide_mir::lower::build_variant_layouts(&m.type_decls);
        variant_layouts.by_type.extend(m_vl.by_type);
        variant_layouts.ctor_to_type.extend(m_vl.ctor_to_type);
        variant_layouts.ctor_field_defaults.extend(m_vl.ctor_field_defaults);
    }
    // PROGRAM pre-pass: inline mutual-recursive tail siblings → direct self-recursion (exposed to
    // the append-accumulator TCO). Guarded: only where it makes a walled fn lower (no regression).
    let inlined_fns =
        almide_mir::lower::inline_mutual_tail_recursion(&ir.functions, &globals, &record_layouts);
    for func in &inlined_fns {
        t.functions += 1;
        let lowered = catch_unwind(AssertUnwindSafe(|| {
            almide_mir::lower::lower_function_all_with_globals(
                func,
                &globals,
                &global_inits,
                &record_layouts,
                &variant_layouts,
            )
        }));
        match lowered {
            Ok(Ok(mirs)) => {
                // `mirs[0]` is the source function; `mirs[1..]` are lambda-lifted
                // auxiliaries (the closures machinery lifts `let f = (x) => …` bodies
                // into fresh functions). Every one is a real MIR function the proven
                // checker re-verifies, so backing / ownership / names witnesses are
                // emitted for ALL of them and the program assembler tables them by the
                // same position. With no lifting wired the vector is just `[main]` and
                // this is byte-identical to the prior single-function pass.
                t.in_profile += mirs.len();
                // The EFFECTIVE body the lowering actually saw: a let-bound heap-result
                // `if`/`match` is tail-duplicated PURELY in the IR before lowering, so the
                // caps `count_ir_calls` 1:1 gate and the interp-coverage count must read the
                // SAME rewritten tree (the duplicated continuation's calls / interps appear
                // once per arm in BOTH MIR and this counted IR — `mir == ir` by construction).
                // desugar-before-both: the SAME ANF-lift (call-arg heap-if → let) then
                // tail-duplication the lowering applies, so the duplicated calls are counted
                // 1:1 (mir == ir) and the caps gate stays exact.
                // desugar-before-both: read the SAME fully-desugared tree the lowering emits
                // its MIR from (the full guard → beta → tuple-unwrap → effect-unwrap →
                // heap-branches fixpoint), so a tail-duplicating rewrite duplicates a call in
                // BOTH the MIR and this counted IR — `mir == ir` by construction. A subset
                // (only guard + heap-branches) missed `desugar_tuple_unwrap_or`, so a
                // `let r = opt.unwrap_or((tuple)); f(r.0)` mir>ir-breached.
                let eff_body = almide_mir::lower::desugar_all(
                    &func.body,
                    func.name.as_str() == "main",
                    &variant_layouts,
                    &record_layouts,
                    &func.params,
                );
                // INTERP COVERAGE (a): this function LOWERED, so its FULLY-LINKABLE
                // interps (Lit/String/Int/Bool parts) fold to a registered __str_concat /
                // int.to_string / bool.to_string chain (proven byte-match v0 by the
                // render_wasm detectors); a non-desugarable interp stays the sound Opaque
                // fallback, and a desugarable-but-UNLINKED one (Float/compound) walls at
                // render — both are (b) cleanly walled (no invalid wasm). The per-CallFn
                // loop below ADDS the unlinked-occurrence count to (b); this counts the
                // interp SITE once, as proven or walled, with no proven mis-bucket.
                let (proven, walled) = count_interp_sites(&eff_body, &auto_linkable, &record_layouts);
                t.interp_lowered += proven;
                t.interp_walled += walled;
                // LINK COVERAGE: a LOWERED function emitting a dotted `Op::CallFn` whose
                // name the v1 linker cannot resolve (not in the self-host registry) would,
                // if rendered, emit a dangling `(call $name)` — invalid wasm. The render-
                // side `try_render_wasm_program` now WALLS the WHOLE program in that case
                // (a clean `LowerError::Unsupported`, never an `Ok` invalid module). So each
                // such site is a bucket-(b) cleanly-walled, NOT a (c) forbidden hole. We
                // MEASURE the distinct unlinkable callees (the visible self-host gap) and
                // fold each occurrence into (b). The wall's COMPLETENESS — that no such site
                // escapes to `Ok` — is what keeps (c) == 0 (asserted below).
                for mir in &mirs {
                    for op in &mir.ops {
                        if let Op::CallFn { name, .. } = op {
                            // Unlinkable stdlib call ⟺ a DOTTED name that is neither in the
                            // self-host registry NOR a function defined in this file (a dotted
                            // user PROTOCOL METHOD resolves to itself/a sibling, so it is NOT a
                            // dangling stdlib call). This is exactly the class the render wall
                            // rejects; a user method / cross-file call is out of scope here.
                            let unlinkable = name.contains('.')
                                && !auto_linkable.contains(name)
                                && !file_fn_names.contains(name);
                            if unlinkable {
                                *t.would_wall_callees.entry(name.clone()).or_insert(0) += 1;
                                t.interp_walled += 1;
                                // (c) completeness backstop: this site is genuinely unlinkable,
                                // so the render wall MUST flag it. `unlinked_call_names` is the
                                // wall's OWN predicate; if it does NOT contain the name, a site
                                // escaped the wall → a real (c) breach. A single-fn probe is
                                // sound here: the name is not file-defined, so adding sibling
                                // functions could not make it resolve (only the registry could,
                                // which `auto_linkable` already ruled out).
                                let probe = MirProgram { functions: vec![mir.clone()], exports: vec![], mutable_global_count: 0 };
                                if !almide_mir::render_wasm::unlinked_call_names(&probe)
                                    .contains(name)
                                {
                                    t.forbidden_unwalled.push(format!(
                                        "{}::{} -> {name} (escaped the render wall)",
                                        file.display(),
                                        mir.name
                                    ));
                                }
                            }
                        }
                    }
                }
                for mir in &mirs {
                    // The borrow-by-default soundness gate: every `+1` event must
                    // be backed by a real runtime op (no synthetic param `+1`).
                    if !plus_one_events_backed(mir) {
                        t.cert_backing_breaches
                            .push(format!("{}::{}", file.display(), mir.name));
                    }
                    // Ownership is one heap object per line; names are one line per
                    // function. Both are LOCAL properties — no transitivity.
                    let cert = ownership_certificate(mir);
                    // Parallel name index (ownership.names): one `<file>::<fn>` line per
                    // cert line, so a checker REJECT bisects straight to its function
                    // (the anonymous 20k-line cert made a reject a needle hunt).
                    for _ in cert.lines() {
                        s.ownership_names
                            .push_str(&format!("{}::{}\n", file.display(), mir.name));
                    }
                    s.ownership.push_str(&cert);
                    s.names.push_str(&name_witness_string(mir));
                    s.names.push('\n');
                }
                // CAPS SOUNDNESS: count the source's call nodes. A call ELIDED by
                // Opaque lowering (a list element, ctor payload, BinOp operand, …) is
                // absent from the MIR ops, so the transitive caps fold over CallFn /
                // FuncRef edges cannot see its effects — if it reached Stdout the
                // function would be falsely caps-verified. The IR call count covers the
                // WHOLE source body (including any lambda later lifted out), so the MIR
                // call count is summed across the main AND its lifted auxiliaries — a
                // lifted lambda carries its body's calls, and a `CallIndirect` (a
                // lowered closure invocation) is a genuine call counted here too. If the
                // cluster has MORE IR calls than MIR call-ops some call was elided
                // SOMEWHERE within it, so EVERY function of the cluster is conservatively
                // TAINTED below (we cannot tell which member hid it).
                let ir_calls = count_ir_calls(&eff_body, &record_layouts, &variant_layouts);
                let mir_calls = mirs
                    .iter()
                    .flat_map(|m| m.ops.iter())
                    .filter(|o| {
                        matches!(
                            o,
                            Op::Call { .. } | Op::CallFn { .. } | Op::CallIndirect { .. }
                        )
                    })
                    // `$__mg_take` is a COMPILER-INJECTED slot accessor (a raw i32.load,
                    // Stdout-free — the trusted-prim class), not a lowering of any IR
                    // call node: a mutable-global heap assign injects one with no IR
                    // counterpart, so counting it would false-breach `mir <= ir`.
                    .filter(|o| !matches!(o, Op::CallFn { name, .. } if name == "__mg_take"))
                    .count();
                if ir_calls > mir_calls {
                    for mir in &mirs {
                        elided_call_fns.insert(mir.name.clone());
                    }
                }
                // SOUNDNESS BACKSTOP for the elided-call effect markers: a marker
                // (`record_elided_calls`) may only surface a genuinely ELIDED
                // call, so the MIR call count can rise at most TO the IR's. If it
                // EXCEEDS, a marker double-counted a lowered call — which could
                // mask another elision and falsely de-taint. A wall breach.
                if mir_calls > ir_calls {
                    t.call_count_breaches.push(format!(
                        "{}::{} (mir {mir_calls} > ir {ir_calls})",
                        file.display(),
                        func.name.as_str()
                    ));
                }
                for mir in mirs {
                    file_mirs.push((mir.name.clone(), mir));
                }
            }
            Ok(Err(almide_mir::lower::LowerError::Unsupported(reason))) => {
                // Categorize the wall: NATIVE-FFI (structural, excluded) iff this function is
                // in the transitive native-FFI closure; else REAL (a lowering gap to close).
                // A name absent from the node map (an inline_mutual_tail_recursion-synthesized
                // aux) defaults REAL (conservative — never over-excludes a real gap).
                let is_native = native_ffi_set.contains(func.name.as_str());
                if is_native {
                    t.walled_native_ffi += 1;
                } else {
                    t.walled_real += 1;
                }
                if std::env::var("WALL_NAMES").is_ok() {
                    let tag = if is_native { "NATIVE-FFI" } else { "REAL" };
                    eprintln!(
                        "WALLED {tag} {} :: {} :: {}",
                        file.display(),
                        func.name.as_str(),
                        reason
                    );
                }
                *t.unsupported.entry(reason_key(&reason)).or_insert(0) += 1;
                // INTERP COVERAGE (b): every interp site inside a WALLED function is
                // cleanly walled too (its function emits no wasm) — never a miscompile.
                let (proven, walled) = count_interp_sites(&func.body, &auto_linkable, &record_layouts);
                t.interp_walled += proven + walled;
            }
            Err(_) => {
                // THE wall breach: lowering must be total. Record file::func.
                t.lower_panics
                    .push(format!("{}::{}", file.display(), func.name.as_str()));
            }
        }
    }

    // Pass 2 (capability SOUNDNESS): a function's empty capability witness is a
    // sound claim of Stdout-freedom ONLY if it reaches no Stdout TRANSITIVELY —
    // the direct witness alone misses what a callee reaches. Emit the witness
    // only for functions provably Stdout-free across `Op::CallFn` edges; the
    // rest are NOT claimed caps-safe (honest scope), never falsely accepted.
    let in_profile_map: BTreeMap<String, MirFunction> = file_mirs.iter().cloned().collect();
    // The conservative free-callee policy: a callee not in the in-profile map
    // is Stdout-free only if it is a pure stdlib `Module` call (a dotted name,
    // purity-gated at lowering), a variant constructor, or a known Stdout-free
    // builtin. Everything else (walled / cross-file user fns) is tainted.
    let is_known_free = |n: &str| {
        n.contains('.') || ctors.contains(n) || KNOWN_STDOUT_FREE_BUILTINS.contains(&n)
    };
    let is_elided = |n: &str| elided_call_fns.contains(n);
    let cap_ids =
        |c: &[Capability]| c.iter().map(|x| x.id().to_string()).collect::<Vec<_>>().join(" ");
    // Track whether the WHOLE file is analyzable + within-bound: only then is the
    // call-graph witness meaningful (any unanalyzable/over-bound function would route to
    // the UNIVERSE sentinel and reject — but unanalyzable is honest scope, not a failure).
    let mut file_graph_clean = !file_mirs.is_empty();
    for (name, mir) in &file_mirs {
        let mut visited = BTreeSet::new();
        match reachable_caps_or_tainted(name, &in_profile_map, &is_known_free, &is_elided, &mut visited)
        {
            // Unanalyzable (an unknown/cross-file or elided callee hides effects).
            None => { t.caps_unverified += 1; file_graph_clean = false; }
            // Fully-known reachable set. Caps-VERIFIED iff it is within the
            // DECLARED bound (`reachable ⊆ declared`): then emit the
            // `<declared>|<reachable>` witness for the proven `check_caps_cert` to
            // re-verify. A function reaching a capability it did NOT declare (e.g.
            // a non-`effect fn` that prints — `is_effect` does not capture every
            // Stdout reach) is conservatively caps-UNVERIFIED — it has an
            // undeclared effect, honestly not claimed safe (emitting it would
            // (correctly) fault the proven subset checker and fail the gate).
            Some(reachable) => {
                let declared: std::collections::BTreeSet<u32> =
                    mir.declared_caps.iter().map(|c| c.id()).collect();
                if reachable.iter().all(|c| declared.contains(&c.id())) {
                    s.caps.push_str(&format!(
                        "{}|{}\n",
                        cap_ids(&mir.declared_caps),
                        cap_ids(&reachable)
                    ));
                    t.caps_verified += 1;
                } else {
                    t.caps_unverified += 1;
                    file_graph_clean = false;
                }
            }
        }
    }
    // The file is fully analyzable + within bound: emit its call-graph witness as ONE line
    // for the proven `check_prog_cert` (caps-transitive), which re-derives the transitive
    // reach itself. (UNIVERSE is unreferenced here — every callee is in-file or known-free.)
    if file_graph_clean {
        s.caps_graph
            .push_str(&program_cap_graph_witness(&in_profile_map, &is_known_free, &is_elided));
        s.caps_graph.push('\n');
    }
}

/// The five certificate output streams main accumulates across files.
#[derive(Default)]
struct CertStreams {
    ownership: String,
    ownership_names: String,
    names: String,
    caps: String,
    caps_graph: String,
}

fn main() {
    // Parse `--out DIR` (where the three witness `.cert` files are written); the
    // remaining args are corpus paths (files or dirs).
    let mut out_dir: Option<PathBuf> = None;
    let mut paths: Vec<String> = Vec::new();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        if a == "--out" {
            out_dir = it.next().map(PathBuf::from);
        } else {
            paths.push(a);
        }
    }
    if paths.is_empty() || out_dir.is_none() {
        eprintln!("usage: classify_corpus --out DIR <file.almd | dir> ...");
        std::process::exit(2);
    }
    let out_dir = out_dir.unwrap();

    // The sweep catches panics deliberately; silence the default hook so a
    // walled-off panic does not spray a backtrace over the honest report.
    std::panic::set_hook(Box::new(|_| {}));

    let mut files = Vec::new();
    for a in &paths {
        collect_almd(Path::new(a), &mut files);
    }

    let mut t = Tally::default();
    // One witness stream per proven property. ownership = one heap object per
    // line; names/caps = one `<superset>|<subset>` line per in-profile function.
    let mut streams = CertStreams::default();
                // One line per FULLY-ANALYZABLE+within-bound file: the call-graph witness for the proven
    // `check_prog_cert` (caps-transitive), which COMPUTES the transitive reach itself — the
    // fold moves out of this untrusted classifier into the proof. Partially-analyzable files
    // stay on the per-function `caps.cert` (honest scope), not emitted here.
    
    // The render-resolvable name oracle for the interp-coverage (c) detector: a DOTTED
    // `CallFn` name (a stdlib `module.func`) renders to `(call $name)` and resolves ONLY
    // if the v1 linker auto-includes it (it is in the self-host registry). A dotted name
    // NOT here would dangle (invalid wasm). User functions can't hold a dot, so dotted +
    // not-auto-linkable is precisely the unlinkable-stdlib-call class (no cross-file user
    // false positives, which a per-file harness could not otherwise rule out).
    let auto_linkable = auto_linkable_call_names();

    for file in &files {
        classify_file(file, &mut t, &mut streams, &auto_linkable);
    }

    // Restore a sane hook before we print (catch window is over).
    let _ = std::panic::take_hook();

    // Write the three witness streams for the proven checker. ownership may be
    // empty if no in-profile function emits a heap object (trivially accepted);
    // names/caps have one line per in-profile function.
    let write = |name: &str, body: &str| {
        let p = out_dir.join(name);
        if let Err(e) = std::fs::write(&p, body) {
            eprintln!("cannot write {}: {e}", p.display());
            std::process::exit(2);
        }
    };
    write("ownership.cert", &streams.ownership);
    write("ownership.names", &streams.ownership_names);
    write("names.cert", &streams.names);
    write("caps.cert", &streams.caps);
    write("caps_graph.cert", &streams.caps_graph);

    // STDERR: the honest coverage report.
    eprintln!("== v0-corpus MIR-lowering wall report ==");
    eprintln!("files scanned          : {}", t.files);
    eprintln!("  frontend-rejected    : {}", t.frontend_rejected);
    eprintln!("  frontend-panicked    : {}", t.frontend_panicked);
    eprintln!("functions reaching MIR : {}", t.functions);
    eprintln!(
        "  in-profile (lowers)  : {}  <- proven-checker re-verifies these",
        t.in_profile
    );
    let walled: usize = t.unsupported.values().sum();
    eprintln!("  walled (Unsupported) : {walled}");
    eprintln!(
        "    walled real (lowering)   : {}  <- the wall=0 metric (pure/WASI-able gaps to close)",
        t.walled_real
    );
    eprintln!(
        "    walled native-FFI (excl) : {}  <- structural (@extern rust/rs + no-wasm stdlib effect); excluded",
        t.walled_native_ffi
    );
    for (reason, n) in &t.unsupported {
        eprintln!("      {n:>4}  {reason}");
    }
    eprintln!("  lower panics (BUG)   : {}", t.lower_panics.len());
    for p in &t.lower_panics {
        eprintln!("      PANIC {p}");
    }
    eprintln!(
        "  unbacked +1 (BUG)    : {}  <- borrow-by-default backing gate",
        t.cert_backing_breaches.len()
    );
    eprintln!(
        "  mir>ir calls (BUG)   : {}  <- elided-call marker double-count gate",
        t.call_count_breaches.len()
    );
    eprintln!(
        "  caps-verified        : {}  <- provably reach no Stdout (transitive); witness emitted",
        t.caps_verified
    );
    eprintln!(
        "  caps-unverified      : {}  <- call an unanalyzable callee; not claimed caps-safe (honest scope)",
        t.caps_unverified
    );
    // INTERP / LINK COVERAGE visibility metric (a/b/c) — measurement only, no soundness
    // DECISION (mir<=ir is unchanged; this neither weakens nor strengthens any detector).
    let would_wall_sites: usize = t.would_wall_callees.values().sum();
    eprintln!("-- interp / call-link coverage (visibility metric) --");
    eprintln!(
        "  (a) lowered (proven) : {}  <- lowerable interp in a lowered fn; folds to a registered chain (byte-match v0)",
        t.interp_lowered
    );
    eprintln!(
        "  (b) walled (no out)  : {}  <- non-subset interp stays Opaque, interp in a walled fn, OR an unlinkable stdlib call the render wall rejects; acceptable, never invalid wasm",
        t.interp_walled
    );
    eprintln!(
        "  (c) FORBIDDEN        : {}  <- a site that renders to dangling `(call $…)` AND escapes the render wall (invalid-wasm-as-Ok); MUST be 0",
        t.forbidden_unwalled.len()
    );
    eprintln!(
        "      of (b): {} unlinkable dotted stdlib call-site(s) across {} distinct callee(s) — the visible self-host-registry gap (render wall rejects each using program cleanly):",
        would_wall_sites,
        t.would_wall_callees.len()
    );
    for (callee, n) in t.would_wall_callees.iter() {
        eprintln!("        {n:>4}  {callee}");
    }
    for p in &t.cert_backing_breaches {
        eprintln!("      UNBACKED {p}");
    }
    for p in &t.call_count_breaches {
        eprintln!("      MIR>IR {p}");
    }
    for p in &t.forbidden_unwalled {
        eprintln!("      FORBIDDEN {p}");
    }

    let total_breaches = t.lower_panics.len()
        + t.cert_backing_breaches.len()
        + t.call_count_breaches.len()
        + t.forbidden_unwalled.len();
    if total_breaches == 0 {
        eprintln!(
            "WALL OK: lower_function was TOTAL over {} corpus functions \
             (Ok or explicit Unsupported, zero panics, zero undetected refusals \
             — a totality + certificate claim, NOT output correctness: that is \
             output-parity's, on its baseline set), and every in-profile \
             certificate `+1` is backed by a real runtime op \
             (no synthetic param ownership — the borrow-by-default gate).",
            t.functions
        );
    } else {
        if !t.lower_panics.is_empty() {
            eprintln!(
                "WALL BREACH: lower_function panicked on {} function(s) — must return \
                 Ok or Unsupported, never panic.",
                t.lower_panics.len()
            );
        }
        if !t.cert_backing_breaches.is_empty() {
            eprintln!(
                "WALL BREACH: {} function(s) emitted an UNBACKED certificate `+1` — \
                 a param or op injected ownership no runtime op performs \
                 (the gate-blind use-after-free class).",
                t.cert_backing_breaches.len()
            );
        }
        if !t.call_count_breaches.is_empty() {
            eprintln!(
                "WALL BREACH: {} function(s) have MORE MIR call-ops than IR call-nodes — \
                 an elided-call effect marker double-counted a lowered call, which could \
                 mask a real elision and falsely de-taint a Stdout-reaching function.",
                t.call_count_breaches.len()
            );
        }
        if !t.forbidden_unwalled.is_empty() {
            eprintln!(
                "WALL BREACH: {} unlinkable stdlib call-site(s) ESCAPED the render wall — a \
                 dangling `(call $…)` would render as a valid-looking `Ok` module (invalid wasm \
                 passing as Ok). `try_render_wasm_program` must reject EVERY unlinkable CallFn; \
                 a gap here is a wall-completeness bug.",
                t.forbidden_unwalled.len()
            );
        }
        std::process::exit(1);
    }
}
