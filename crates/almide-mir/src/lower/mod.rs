//! Core-IR → MIR lowering — the single ownership+layout DECISION pass (§3.1).
//!
//! This is the v1 thesis made real: ONE pass decides, per binding, the
//! ownership (fresh `Alloc` / alias `Dup` / scope-end `Drop` / mutate
//! `MakeUnique`) and the layout ([`Repr`]) — replacing the five scattered
//! codegen passes (`pass_perceus`/`pass_clone`/`pass_borrow_inference`/
//! `pass_capture_clone`/`pass_box_deref`) with a single source the renderers
//! only translate. The produced MIR is checked by [`crate::verify_ownership`].
//!
//! Build order (§6, risk-first): it consumes the EXISTING frontend IR
//! (`almide_ir`) as a temporary feeder so the novel core is validated before
//! the frontend is greenfielded.
//!
//! # Scope of this brick
//! The value-semantics subset, on a LINEAR function body: `Bind` of a fresh
//! heap value (list/record/string literal) or an alias (`var b = a`) or a
//! scalar; `IndexAssign` (copy-on-write `MakeUnique`); scope-end `Drop`s.
//! Anything outside the subset (control flow, calls, …) returns
//! [`LowerError::Unsupported`] — never a silent drop (flight-grade totality).

use crate::{Init, MirFunction, MirParam, Op, Repr, ValueId, PLACEHOLDER_LAYOUT};
use almide_ir::{
    CallTarget, IrExpr, IrExprKind, IrFunction, IrParam, IrStmt, IrStmtKind, IrStringPart, VarId,
};
use almide_lang::types::Ty;
use std::collections::{HashMap, HashSet};

/// A lowering could not proceed because the input is outside this brick's
/// subset (or violates a precondition such as concrete types). Carrying the
/// reason keeps the pass TOTAL — no case is silently skipped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LowerError {
    Unsupported(String),
}

/// Heap-managed types (need refcount: `Alloc`/`Dup`/`Drop`) vs `Copy` scalars.
/// Mirrors the old `pass_perceus::is_heap_type` / `emit_wasm` copy — but here it
/// is the SINGLE definition both renderers will read off the MIR.
pub fn is_heap_ty(ty: &Ty) -> bool {
    !matches!(
        ty,
        Ty::Int
            | Ty::Int8
            | Ty::Int16
            | Ty::Int32
            | Ty::Int64
            | Ty::UInt8
            | Ty::UInt16
            | Ty::UInt32
            | Ty::UInt64
            | Ty::Float
            | Ty::Float32
            | Ty::Float64
            | Ty::Bool
            | Ty::Unit
            | Ty::Never
            | Ty::RawPtr
            | Ty::ConstParam { .. }
            | Ty::ConstValue { .. }
    )
}

/// A CONST-foldable module-global initializer → its direct `Init` (NO runtime call), else `None`.
/// Admits exactly the compile-time-known heap constants the module-global materialization emits as
/// data: a string literal, an all-int-literal `List[Int]`, and `bytes.from_list([int literals])`.
/// Anything COMPUTED (a `string.from_codepoint(..)` / user call) returns `None` and keeps walling —
/// materializing it would inject a `CallFn` the gate's IR-side `count_ir_calls` cannot see (mir>ir).
fn const_global_init(init: &IrExpr) -> Option<crate::Init> {
    match &init.kind {
        IrExprKind::LitStr { value } => Some(crate::Init::Str(value.clone())),
        IrExprKind::List { elements } => {
            let ints: Option<Vec<i64>> = elements
                .iter()
                .map(|e| match &e.kind {
                    IrExprKind::LitInt { value } => Some(*value),
                    _ => None,
                })
                .collect();
            ints.map(crate::Init::IntList)
        }
        IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
            if module.as_str() == "bytes" && func.as_str() == "from_list" && args.len() == 1 =>
        {
            let IrExprKind::List { elements } = &args[0].kind else { return None };
            let bytes: Option<Vec<u8>> = elements
                .iter()
                .map(|e| match &e.kind {
                    IrExprKind::LitInt { value } => Some(*value as u8),
                    _ => None,
                })
                .collect();
            bytes.map(crate::Init::Bytes)
        }
        _ => None,
    }
}

/// Is `ty` an `Option[_]` / `Result[_, _]` — a tagged heap VARIANT? Used to gate the
/// value-position variant-match WALL: a scalar-result match over an Option/Result subject
/// that can't execute the tag-read must reject (a Const-0 would pick a wrong arm), but a
/// String/List literal match (a separate gap) keeps its existing deferred lowering.
pub fn is_variant_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(
        ty,
        Ty::Applied(TypeConstructorId::Option | TypeConstructorId::Result, _)
    )
}

/// Is `ty` a `Result[_, _]` (vs an `Option[_]`)? Selects the len-as-tag arm arrangement for a
/// `??` / `match` over a variant: Option `Some` = `tag != 0`, Result `Ok` = `tag == 0` (INVERSE).
pub fn is_result_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty, Ty::Applied(TypeConstructorId::Result, _))
}

/// The [`Repr`] of a value of type `ty` — the LAYOUT decision, made once here.
/// Heap types get `Ptr` with a placeholder [`LayoutId`] (the layout pass, a
/// later brick, assigns real ids); scalars get their named byte width.
pub fn repr_of(ty: &Ty) -> Result<Repr, LowerError> {
    if matches!(ty, Ty::Unknown) {
        // Repr demands concrete types — the AllTypesConcrete precondition (§4).
        return Err(LowerError::Unsupported(
            "Unknown type reached MIR lowering (AllTypesConcrete precondition violated)".into(),
        ));
    }
    if is_heap_ty(ty) {
        return Ok(Repr::Ptr { layout: PLACEHOLDER_LAYOUT });
    }
    use crate::ScalarWidth;
    let w = match ty {
        Ty::Int | Ty::Int64 | Ty::UInt64 | Ty::Float | Ty::Float64 => ScalarWidth::Double,
        Ty::Int32 | Ty::UInt32 | Ty::Float32 => ScalarWidth::Word,
        Ty::Int16 | Ty::UInt16 => ScalarWidth::Half,
        Ty::Int8 | Ty::UInt8 => ScalarWidth::Byte,
        Ty::Bool => ScalarWidth::Word, // Bool ABI slot is 4 bytes
        // Unit/Never/RawPtr/Const* are not values that get a scalar slot here.
        other => {
            return Err(LowerError::Unsupported(format!(
                "no scalar Repr for {other:?}"
            )))
        }
    };
    Ok(Repr::Scalar { width: w })
}

/// Lower one function to MIR. Parameters are seeded first (the v1 borrow-by-
/// default calling convention — see [`LowerCtx::bind_params`]), then the body.
pub fn lower_function(
    func: &IrFunction,
    globals: &HashMap<VarId, Ty>,
) -> Result<MirFunction, LowerError> {
    // The main function only; any lambda-lifted auxiliaries are dropped (callers that
    // need them — render/verify paths — use `lower_function_all`). Sound while no lambda
    // lifting is wired (lifted is empty); when it is, those paths verify the auxiliaries.
    let mut all = lower_function_all(func, globals)?;
    Ok(all.remove(0))
}

/// Lower a function to its MIR plus any lambda-lifted auxiliary functions (index 0 is the
/// main function). The closures machinery lifts `let f = (x) => …` bodies into fresh
/// functions accumulated in `LowerCtx::lifted`; this returns them so the program assembler
/// can table + verify them. With no lifting wired the result is just `[main]`.
pub fn lower_function_all(
    func: &IrFunction,
    globals: &HashMap<VarId, Ty>,
) -> Result<Vec<MirFunction>, LowerError> {
    lower_function_all_with_types(func, globals, &RecordLayouts::new())
}

/// Build the [`RecordLayouts`] registry from a program's type declarations — the
/// VALUE-MODEL field structure the lowering consults to materialize records and
/// resolve `r.x`. Each `type R = { … }` becomes `R → (generic params, fields)`;
/// variant / alias decls carry no flat record layout and are skipped (a record
/// VARIANT is a separate, tagged shape — out of this brick). Call once per
/// program and pass the result into [`lower_function_all_with_types`].
pub fn build_record_layouts(type_decls: &[almide_ir::IrTypeDecl]) -> RecordLayouts {
    let mut out = RecordLayouts::new();
    for decl in type_decls {
        if let almide_ir::IrTypeDeclKind::Record { fields } = &decl.kind {
            let generics = decl
                .generics
                .as_ref()
                .map(|gs| gs.iter().map(|g| g.name).collect())
                .unwrap_or_default();
            let field_tys = fields.iter().map(|f| (f.name, f.ty.clone())).collect();
            out.insert(decl.name.as_str().to_string(), (generics, field_tys));
        }
    }
    out
}

/// Build the [`VariantLayouts`] registry from a program's type declarations — the
/// VALUE-MODEL tag + per-constructor field structure the ADT bricks consult to construct,
/// `match`, and drop a custom variant. Each `type V = A(..) | B { .. } | C` becomes
/// `V → VariantLayout { tag-indexed cases, slot_count }`; record / alias decls carry no
/// variant layout and are skipped. The tag is the declaration index and tuple-constructor
/// fields are named `_0`, `_1`, … — both matching v0's `emit_wasm` registration, so the
/// backends agree on tag and field identity. Call once per program and pass the result
/// into [`lower_function_all_with_layouts`].
pub fn build_variant_layouts(type_decls: &[almide_ir::IrTypeDecl]) -> VariantLayouts {
    use almide_ir::{IrTypeDeclKind, IrVariantKind};
    let mut out = VariantLayouts::default();
    for decl in type_decls {
        let IrTypeDeclKind::Variant { cases, .. } = &decl.kind else {
            continue;
        };
        let generics = decl
            .generics
            .as_ref()
            .map(|gs| gs.iter().map(|g| g.name).collect())
            .unwrap_or_default();
        let type_name = decl.name.as_str().to_string();
        let mut case_layouts = Vec::with_capacity(cases.len());
        let mut max_arity = 0usize;
        for (tag, case) in cases.iter().enumerate() {
            let fields: Vec<(almide_lang::intern::Sym, Ty)> = match &case.kind {
                IrVariantKind::Unit => Vec::new(),
                // A tuple constructor's positional fields get the same `_0`, `_1`, …
                // synthetic names v0 assigns, so field identity is shared across backends.
                IrVariantKind::Tuple { fields } => fields
                    .iter()
                    .enumerate()
                    .map(|(i, ty)| (almide_lang::intern::sym(&format!("_{i}")), ty.clone()))
                    .collect(),
                IrVariantKind::Record { fields } => {
                    fields.iter().map(|f| (f.name, f.ty.clone())).collect()
                }
            };
            max_arity = max_arity.max(fields.len());
            out.ctor_to_type
                .insert(case.name.as_str().to_string(), type_name.clone());
            case_layouts.push(VariantCaseLayout {
                ctor: case.name,
                tag: tag as u32,
                fields,
            });
        }
        out.by_type.insert(
            type_name,
            VariantLayout {
                generics,
                cases: case_layouts,
                // slot 0 is the tag; slots 1.. are the widest constructor's fields, so all
                // constructors of the type share one block size (uniform alloc + sound `==`).
                slot_count: 1 + max_arity,
            },
        );
    }
    out
}

/// If `ty` names a user VARIANT in `variant_names`, return that name (the recursion target for a
/// nested-variant ctor field's drop). Handles the three variant-type surface forms.
fn variant_field_name(ty: &Ty, variant_names: &std::collections::HashSet<String>) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    let n = match ty {
        Ty::Named(n, _) => n.as_str().to_string(),
        Ty::Variant { name, .. } => name.as_str().to_string(),
        Ty::Applied(TypeConstructorId::UserDefined(n), _) => n.clone(),
        _ => return None,
    };
    variant_names.contains(&n).then_some(n)
}

/// A variant type NEEDS a generated recursive drop fn (`Op::DropVariant` → `$__drop_<T>`) iff some
/// ctor field is itself a user variant: a flat `rc_dec` of that nested block would leak its own
/// heap children. A String-only-field variant uses the masked `DropListStr` (ADT brick 5a/5c)
/// instead — no recursive fn. Used by both the generator and `try_lower_variant_ctor` (to choose
/// `DropVariant` tracking), so the two never disagree.
pub fn variant_needs_recursive_drop(
    decl: &almide_ir::IrTypeDecl,
    variant_names: &std::collections::HashSet<String>,
) -> bool {
    use almide_ir::{IrTypeDeclKind, IrVariantKind};
    let IrTypeDeclKind::Variant { cases, .. } = &decl.kind else {
        return false;
    };
    cases.iter().any(|c| {
        let tys: Vec<&Ty> = match &c.kind {
            IrVariantKind::Unit => vec![],
            IrVariantKind::Tuple { fields } => fields.iter().collect(),
            IrVariantKind::Record { fields } => fields.iter().map(|f| &f.ty).collect(),
        };
        tys.iter().any(|t| variant_field_name(t, variant_names).is_some())
    })
}

/// The set of all user-variant type names in `type_decls` — the lookup `variant_field_name` uses.
pub fn variant_type_names(
    type_decls: &[almide_ir::IrTypeDecl],
) -> std::collections::HashSet<String> {
    use almide_ir::IrTypeDeclKind;
    type_decls
        .iter()
        .filter(|d| matches!(d.kind, IrTypeDeclKind::Variant { .. }))
        .map(|d| d.name.as_str().to_string())
        .collect()
}

/// Generate the ALMIDE SOURCE for each variant type's recursive drop fn `__drop_<T>` (ADT brick
/// 5b) — the `$__drop_value` shape: at the last ref (rc==1) read the tag, recursively
/// `__drop_<V>` each nested-variant field + `prim.rc_dec` each leaf `String` field, then release
/// the block. Returns the concatenated source to APPEND to the program (so the `type` decls it
/// references are in scope); only types that `variant_needs_recursive_drop` get a fn. The fn is
/// `prim`-only ⇒ empty ownership cert (a trusted routine — its leak/double-free correctness is
/// the create+drop LEAK LOOP's burden, exactly like `__drop_value`). The slot offsets match the
/// v1 construct (`[rc@0][len@4][cap@8][tag=slot0@12][field i @ 12+(1+i)*8]`).
pub fn generate_variant_drop_sources(type_decls: &[almide_ir::IrTypeDecl]) -> String {
    use almide_ir::{IrTypeDeclKind, IrVariantKind};
    let names = variant_type_names(type_decls);
    let mut out = String::new();
    for decl in type_decls {
        if !variant_needs_recursive_drop(decl, &names) {
            continue;
        }
        let IrTypeDeclKind::Variant { cases, .. } = &decl.kind else { continue };
        let tname = decl.name.as_str();
        out.push_str(&format!("fn __drop_{tname}(e: {tname}) -> Unit = {{\n"));
        out.push_str("  let h = prim.handle(e)\n");
        out.push_str("  if prim.load32(h + 0) == 1 then {\n");
        out.push_str(&format!("    let t = prim.load64(h + {})\n", layout::slot_offset(0)));
        // One tag branch per ctor that has a heap field; chained `if t == k then {..} else ..`.
        let mut branch = String::new();
        let mut first = true;
        for (tag, case) in cases.iter().enumerate() {
            let tys: Vec<Ty> = match &case.kind {
                IrVariantKind::Unit => vec![],
                IrVariantKind::Tuple { fields } => fields.clone(),
                IrVariantKind::Record { fields } => fields.iter().map(|f| f.ty.clone()).collect(),
            };
            // Per-field free statements (variant → recurse, String → rc_dec, scalar → skip).
            let mut frees = String::new();
            let mut idx = 0usize;
            for (i, ty) in tys.iter().enumerate() {
                let off = layout::slot_offset(1 + i);
                if let Some(fv) = variant_field_name(ty, &names) {
                    frees.push_str(&format!(
                        "        let f{idx}: {fv} = prim.load_handle(h + {off})\n        __drop_{fv}(f{idx})\n"
                    ));
                    idx += 1;
                } else if matches!(ty, Ty::String) {
                    frees.push_str(&format!(
                        "        prim.rc_dec(prim.load64(h + {off}))\n"
                    ));
                }
            }
            if frees.is_empty() {
                continue; // scalar/Unit ctor — nothing to free
            }
            let kw = if first { "if" } else { "else if" };
            branch.push_str(&format!("    {kw} t == {tag} then {{\n{frees}      }}\n"));
            first = false;
        }
        if branch.is_empty() {
            // No heap-field ctor (shouldn't happen — needs_recursive_drop was true), guard anyway.
            out.push_str("    ()\n");
        } else {
            out.push_str(&branch);
            out.push_str("    else ()\n");
        }
        out.push_str("  } else ()\n");
        out.push_str("  prim.rc_dec(h)\n");
        out.push_str("}\n");
    }
    out
}

/// A record field whose flat `rc_dec` of its handle would LEAK nested heap (so the record needs a
/// generated recursive `$__drop_<R>`): a `Map`/`Value`/record/`List[heap]` field. A plain `String`
/// (its `rc_dec` IS its full free), a scalar, or a `List[scalar]` (the block frees flat) does NOT.
pub fn record_field_needs_recursive_drop(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    match ty {
        Ty::String => false,
        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => is_heap_ty(&a[0]),
        _ => is_heap_ty(ty),
    }
}

/// The set of RECORD type names whose drop must be the recursive `$__drop_<R>` (any field
/// [`record_field_needs_recursive_drop`]). A scalar/String-only record keeps the flat masked
/// `DropListStr`. Mirrors [`variant_needs_recursive_drop`] for records.
pub fn recursive_record_drop_names(
    type_decls: &[almide_ir::IrTypeDecl],
) -> std::collections::HashSet<String> {
    use almide_ir::IrTypeDeclKind;
    type_decls
        .iter()
        .filter_map(|d| match &d.kind {
            IrTypeDeclKind::Record { fields }
                if fields.iter().any(|f| record_field_needs_recursive_drop(&f.ty)) =>
            {
                Some(d.name.as_str().to_string())
            }
            _ => None,
        })
        .collect()
}

/// `Some(name)` iff `ty` is a NAMED record/aggregate whose `$__drop_<name>` is generated (it is in
/// `rec_names`) — so a field of that type recurses via `__drop_<name>`. A non-recursive (scalar-only)
/// record is `None`: it is freed by a flat `rc_dec` of its block.
fn recursive_aggregate_name(ty: &Ty, rec_names: &std::collections::HashSet<String>) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    let n = match ty {
        Ty::Named(n, _) => n.as_str().to_string(),
        Ty::Applied(TypeConstructorId::UserDefined(n), _) => n.clone(),
        _ => return None,
    };
    rec_names.contains(&n).then_some(n)
}

/// Generate the ALMIDE SOURCE for each RECORD type's recursive drop `$__drop_<R>` (the records
/// counterpart of [`generate_variant_drop_sources`]). Records have NO tag — fields sit at
/// `slot_offset(i)`, freed per CONCRETE field type: `String → rc_dec`, `Map[String,String] →
/// __drop_map_ss`, `List[String] → __drop_list_str`, `List[<recursive record>] → __drop_list_<R>`,
/// a recursive record → `__drop_<R>`, a `Value → __drop_value`, a scalar-only nested aggregate or
/// `List[scalar]` → flat `rc_dec` of the block, a scalar → skip. Emits the needed `__drop_list_<R>`
/// loops + the generic `__drop_map_ss` / `__drop_list_str` helpers. All `__drop_`-prefixed ⇒ on the
/// `prim.rc_dec` whitelist + an empty ownership cert (a trusted free, leak-loop verified).
pub fn generate_record_drop_sources(type_decls: &[almide_ir::IrTypeDecl]) -> String {
    use almide_ir::IrTypeDeclKind;
    use almide_lang::types::constructor::TypeConstructorId;
    let rec_names = recursive_record_drop_names(type_decls);
    let mut out = String::new();
    let mut list_drops: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut need_map_ss = false;
    let mut need_list_str = false;
    for decl in type_decls {
        let IrTypeDeclKind::Record { fields } = &decl.kind else { continue };
        if !rec_names.contains(decl.name.as_str()) {
            continue;
        }
        let tname = decl.name.as_str();
        out.push_str(&format!("fn __drop_{tname}(e: {tname}) -> Unit = {{\n"));
        out.push_str("  let h = prim.handle(e)\n");
        out.push_str("  if prim.load32(h + 0) == 1 then {\n");
        let mut frees = String::new();
        for (i, f) in fields.iter().enumerate() {
            let off = layout::slot_offset(i);
            match &f.ty {
                Ty::String => {
                    frees.push_str(&format!("    prim.rc_dec(prim.load64(h + {off}))\n"));
                }
                Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => {
                    if let Some(rn) = recursive_aggregate_name(&a[0], &rec_names) {
                        list_drops.insert(rn.clone());
                        frees.push_str(&format!(
                            "    let f{i}: List[{rn}] = prim.load_handle(h + {off})\n    __drop_list_{rn}(f{i})\n"
                        ));
                    } else if matches!(a[0], Ty::String) {
                        need_list_str = true;
                        frees.push_str(&format!(
                            "    let f{i}: List[String] = prim.load_handle(h + {off})\n    __drop_list_str(f{i})\n"
                        ));
                    } else {
                        // List[scalar] or List[non-recursive heap]: flat free the block.
                        frees.push_str(&format!("    prim.rc_dec(prim.load64(h + {off}))\n"));
                    }
                }
                Ty::Applied(TypeConstructorId::Map, a)
                    if a.len() == 2 && matches!(a[0], Ty::String) && matches!(a[1], Ty::String) =>
                {
                    need_map_ss = true;
                    frees.push_str(&format!(
                        "    let f{i}: Map[String, String] = prim.load_handle(h + {off})\n    __drop_map_ss(f{i})\n"
                    ));
                }
                t if is_value_ty(t) => {
                    frees.push_str(&format!(
                        "    let f{i}: Value = prim.load_handle(h + {off})\n    __drop_value(f{i})\n"
                    ));
                }
                t => {
                    if let Some(rn) = recursive_aggregate_name(t, &rec_names) {
                        frees.push_str(&format!(
                            "    let f{i}: {rn} = prim.load_handle(h + {off})\n    __drop_{rn}(f{i})\n"
                        ));
                    } else if is_heap_ty(t) {
                        // a non-recursive heap field (scalar-only nested record, scalar map) — flat free.
                        frees.push_str(&format!("    prim.rc_dec(prim.load64(h + {off}))\n"));
                    }
                    // a scalar field — skip (no free).
                }
            }
        }
        out.push_str(&frees);
        out.push_str("  } else ()\n");
        out.push_str("  prim.rc_dec(h)\n");
        out.push_str("}\n");
    }
    // A per-element-recursive `$__drop_list_<R>` for EVERY recursive-drop record R (not just the
    // field-referenced ones in `list_drops`) — so a standalone `List[R]` LITERAL value (`group([…])`)
    // routes its drop here too. Sorted for host-determinism.
    let _ = &list_drops; // (subsumed by rec_names below)
    let mut list_drop_names: Vec<&String> = rec_names.iter().collect();
    list_drop_names.sort();
    for rn in list_drop_names {
        out.push_str(&format!(
            "fn __drop_list_{rn}(xs: List[{rn}]) -> Unit = {{\n  \
               let h = prim.handle(xs)\n  \
               if prim.load32(h + 0) == 1 then __drop_list_{rn}_loop(h, prim.load32(h + 4), 0) else ()\n  \
               prim.rc_dec(h)\n}}\n\
             fn __drop_list_{rn}_loop(h: Int, n: Int, i: Int) -> Unit =\n  \
               if i >= n then ()\n  \
               else {{ let e: {rn} = prim.load_handle(h + 12 + i * 8)\n         __drop_{rn}(e)\n         __drop_list_{rn}_loop(h, n, i + 1) }}\n"
        ));
    }
    if need_map_ss {
        // v1's `Map[String,String]` borrows the `map_skv` (String,Int) layout: the n KEYS are the
        // first n slots (`@ 12 + i*8`), DEEP-COPIED + owned by the map (`__skv_store_key` store_str);
        // the n VALUES are the next n slots, stored RAW (`store64`) — NOT owned by the map (the proper
        // owned-value `Map[String,String]` self-host is a separate brick, docs/roadmap v1-records-svg).
        // So the drop frees ONLY the owned key copies (rc_dec the first n slots) — freeing the borrowed
        // values would DOUBLE-FREE. (`n = load32(h+4)` is the entry count.)
        out.push_str(
            "fn __drop_map_ss(m: Map[String, String]) -> Unit = {\n  \
               let h = prim.handle(m)\n  \
               if prim.load32(h + 0) == 1 then __drop_map_ss_loop(h, prim.load32(h + 4), 0) else ()\n  \
               prim.rc_dec(h)\n}\n\
             fn __drop_map_ss_loop(h: Int, n: Int, i: Int) -> Unit =\n  \
               if i >= n then ()\n  \
               else { prim.rc_dec(prim.load64(h + 12 + i * 8))\n         __drop_map_ss_loop(h, n, i + 1) }\n",
        );
    }
    if need_list_str {
        out.push_str(
            "fn __drop_list_str(xs: List[String]) -> Unit = {\n  \
               let h = prim.handle(xs)\n  \
               if prim.load32(h + 0) == 1 then __drop_list_str_loop(h, prim.load32(h + 4), 0) else ()\n  \
               prim.rc_dec(h)\n}\n\
             fn __drop_list_str_loop(h: Int, n: Int, i: Int) -> Unit =\n  \
               if i >= n then ()\n  \
               else { prim.rc_dec(prim.load64(h + 12 + i * 8))\n         __drop_list_str_loop(h, n, i + 1) }\n",
        );
    }
    out
}

/// [`lower_function_all`] WITH the program's record-layout registry threaded in —
/// the entry the real pipeline (render_program) uses so a `Ty::Named` record
/// resolves its fields (and `r.x` materializes). The plain [`lower_function_all`]
/// passes an empty registry (the structurally-typed `Ty::Record`/`Ty::Tuple`
/// paths still work; a `Ty::Named` aggregate stays walled without it). Delegates to
/// [`lower_function_all_with_layouts`] with an empty VARIANT registry — so a custom
/// variant stays walled (the ADT bricks call `_with_layouts` to admit it).
pub fn lower_function_all_with_types(
    func: &IrFunction,
    globals: &HashMap<VarId, Ty>,
    record_layouts: &RecordLayouts,
) -> Result<Vec<MirFunction>, LowerError> {
    lower_function_all_with_layouts(func, globals, record_layouts, &VariantLayouts::default())
}

/// [`lower_function_all_with_layouts`] WITH the module-level globals' INITIALIZERS threaded
/// in, so a HEAP global reference materializes its real const value (the base64 alphabet /
/// aes S-box) instead of walling. The `_with_layouts` entry delegates here with empty inits
/// (every heap-global reference there still walls, as before — no regression).
pub fn lower_function_all_with_globals(
    func: &IrFunction,
    globals: &HashMap<VarId, Ty>,
    global_inits: &HashMap<VarId, IrExpr>,
    record_layouts: &RecordLayouts,
    variant_layouts: &VariantLayouts,
) -> Result<Vec<MirFunction>, LowerError> {
    lower_function_all_impl(func, globals, global_inits, record_layouts, variant_layouts)
}

/// [`lower_function_all_with_types`] WITH the program's VARIANT-layout registry threaded in
/// too — the entry the real pipeline uses once custom ADTs participate in the value model
/// (the construct / `match` / drop bricks consult [`LowerCtx::variant_layouts`]). The
/// record-only entry above delegates here with an empty variant registry.
pub fn lower_function_all_with_layouts(
    func: &IrFunction,
    globals: &HashMap<VarId, Ty>,
    record_layouts: &RecordLayouts,
    variant_layouts: &VariantLayouts,
) -> Result<Vec<MirFunction>, LowerError> {
    lower_function_all_impl(func, globals, &HashMap::new(), record_layouts, variant_layouts)
}

fn lower_function_all_impl(
    func: &IrFunction,
    globals: &HashMap<VarId, Ty>,
    global_inits: &HashMap<VarId, IrExpr>,
    record_layouts: &RecordLayouts,
    variant_layouts: &VariantLayouts,
) -> Result<Vec<MirFunction>, LowerError> {
    let mut ctx = LowerCtx {
        globals: globals.clone(),
        global_inits: global_inits.clone(),
        fn_name: func.name.as_str().to_string(),
        record_layouts: record_layouts.clone(),
        variant_layouts: variant_layouts.clone(),
        ..Default::default()
    };
    let params = ctx.bind_params(&func.params)?;
    // TCO: a tail-self-recursive heap-result function is rewritten to a scalar loop + post-loop
    // dispatch (the existing self-rec guard would otherwise wall it). The rewritten body lowers
    // through the ordinary statements+tail path; if it is out of the TCO subset, `None` keeps the
    // original body (which the self-rec guard walls as before — no regression).
    // PRE-DESUGAR before the TCO: a recursive body `{ let c = if k then A else B; recurse(acc + c) }`
    // has a let-bound heap-result `if` the loop-body lowering would wall. Tail-duplication
    // (`desugar_heap_branches`) pushes the continuation — INCLUDING the recursive call — into each arm,
    // yielding BRANCHED recursion `if k then recurse(acc+A) else recurse(acc+B)` that `tco_collect`
    // handles (it recurses both `if` arms). The let-bound `if` is ELIMINATED, so the loop body lowers.
    // `lower_body_into` desugars again (idempotent) for the non-TCO path; the caps gate counts the
    // SAME desugared tree (desugar-before-both), so mir == ir. Unblocks base64 encode/decode_chunks +
    // toml read_basic/parse_val (the let-bound-heap-`if`-in-a-loop frontier).
    let pre_tco = desugar_heap_branches(&func.body);
    let body_ref: &IrExpr = pre_tco.as_ref().unwrap_or(&func.body);
    let tco_body = try_tco_rewrite(&ctx.fn_name, &func.params, body_ref);
    let ret = ctx.lower_body_into(tco_body.as_ref().unwrap_or(body_ref))?;
    // The function's EFFECT SIGNATURE → its declared capability bound. The v1 model
    // has one capability (Stdout); an `effect fn` declares it may reach the host, so
    // it admits the only modeled cap. A pure `fn` declares ∅ — so if it reached
    // Stdout (forbidden by the effect system) the proven `used ⊆ declared` checker
    // would REJECT it. The capability gate verifies `reachable ⊆ declared`, not just
    // "reaches nothing" — so an effectful function is now caps-VERIFIED against its
    // own declared bound, not merely excluded.
    // An `effect fn` declares it MAY reach the modeled host capabilities (the v1 effect system is
    // binary: pure vs host-reaching, not per-capability). So it admits Stdout, Entropy, CliArgs AND
    // FsRead — the `used ⊆ declared` checker then verifies its body stays within that bound. A pure
    // `fn` declares ∅, so reaching ANY cap (a `print`/`random.int`/`env.args`/`fs.read_text` from a
    // non-effect fn — already a frontend type error) would REJECT here too: the soundness floor (pure
    // stays pure) is unchanged; only the host-reaching set grows. (A per-capability effect signature
    // is a later precision refinement.)
    let declared_caps = if func.is_effect {
        vec![
            crate::Capability::Stdout,
            crate::Capability::Entropy,
            crate::Capability::CliArgs,
            crate::Capability::FsRead,
        ]
    } else {
        Vec::new()
    };
    let lifted = std::mem::take(&mut ctx.lifted);
    let heap_slot_masks = ctx.record_masks.iter().map(|(v, m)| (*v, m.clone())).collect();
    let main = MirFunction {
        name: func.name.as_str().to_string(),
        params,
        ops: ctx.ops,
        ret,
        declared_caps,
        heap_slot_masks,
    };
    let mut all = vec![main];
    all.extend(lifted);
    Ok(all)
}

mod binds;
mod layout;
mod tail;
mod control;
mod calls;


#[cfg(test)]
mod tests;

include!("mod_p2.rs");
include!("mod_p3.rs");
include!("mod_p4.rs");
include!("mod_p5.rs");
include!("mod_p6.rs");
