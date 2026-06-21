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
    let mut ctx = LowerCtx {
        globals: globals.clone(),
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
    let tco_body = try_tco_rewrite(&ctx.fn_name, &func.params, &func.body);
    let ret = ctx.lower_body_into(tco_body.as_ref().unwrap_or(&func.body))?;
    // The function's EFFECT SIGNATURE → its declared capability bound. The v1 model
    // has one capability (Stdout); an `effect fn` declares it may reach the host, so
    // it admits the only modeled cap. A pure `fn` declares ∅ — so if it reached
    // Stdout (forbidden by the effect system) the proven `used ⊆ declared` checker
    // would REJECT it. The capability gate verifies `reachable ⊆ declared`, not just
    // "reaches nothing" — so an effectful function is now caps-VERIFIED against its
    // own declared bound, not merely excluded.
    let declared_caps = if func.is_effect {
        vec![crate::Capability::Stdout]
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

/// A function CAN-ERR (returns `Err` on some input) iff its body has a direct `err(…)` (`ResultErr`) OR
/// it `!`-PROPAGATES (an `Unwrap` over a `Named` call to) a can-err function. A function whose entire
/// `!`-call closure is err-free NEVER returns `Err`, so `let pat = f()!` over it is faithfully
/// `let pat = f()` (the same pass-through the tail `!` already uses). KEY: an error reached only through
/// a `match`/`??` (e.g. the yaml cluster calling the PURE `oct_rec`/`bin_rec` int parsers, which DO have
/// `err(…)`, but via `match` not `!`) is HANDLED, not propagated, so it does NOT make the caller can-err —
/// the yaml parser cluster is therefore entirely never-err.
fn has_result_err(body: &IrExpr) -> bool {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct V(bool);
    impl IrVisitor for V {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(&e.kind, IrExprKind::ResultErr { .. }) {
                self.0 = true;
            }
            walk_expr(self, e);
        }
    }
    let mut v = V(false);
    v.visit_expr(body);
    v.0
}

fn unwrap_named_callees(body: &IrExpr) -> std::collections::HashSet<String> {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct V(std::collections::HashSet<String>);
    impl IrVisitor for V {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Unwrap { expr } = &e.kind {
                if let IrExprKind::Call { target: CallTarget::Named { name }, .. } = &expr.kind {
                    self.0.insert(name.as_str().to_string());
                }
            }
            walk_expr(self, e);
        }
    }
    let mut v = V(std::collections::HashSet::new());
    v.visit_expr(body);
    v.0
}

/// The set of function names that CAN return `Err` — `has_result_err` seeds + `!`-propagation fixpoint.
pub fn compute_can_err(fns: &[IrFunction]) -> std::collections::HashSet<String> {
    use std::collections::HashSet;
    let mut can_err: HashSet<String> = fns
        .iter()
        .filter(|f| has_result_err(&f.body))
        .map(|f| f.name.as_str().to_string())
        .collect();
    let callees: Vec<(String, HashSet<String>)> = fns
        .iter()
        .map(|f| (f.name.as_str().to_string(), unwrap_named_callees(&f.body)))
        .collect();
    loop {
        let mut changed = false;
        for (name, cs) in &callees {
            if !can_err.contains(name) && cs.iter().any(|g| can_err.contains(g)) {
                can_err.insert(name.clone());
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    can_err
}

/// Strip `Unwrap` (`!`) over a NEVER-ERR `Named` call: `let pat = f()!` → `let pat = f()` and a
/// `f()!` self-call → bare `f()` (so `tco_collect` sees the recursion). SOUND — a never-err callee always
/// returns `Ok`, so the `!` is a no-op; a CAN-ERR callee's `!` is LEFT untouched (it still walls in
/// `lower_destructure`/`lower_bind`), so its error is never silently dropped (the blanket strip that did
/// drop it byte-mismatched safe_div_chain & co. — see the roadmap note).
pub fn strip_never_err_unwraps(body: &mut IrExpr, can_err: &std::collections::HashSet<String>) {
    use almide_ir::{walk_expr_mut, IrMutVisitor};
    struct S<'a>(&'a std::collections::HashSet<String>);
    impl IrMutVisitor for S<'_> {
        fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
            walk_expr_mut(self, expr);
            let strip = matches!(&expr.kind, IrExprKind::Unwrap { expr: inner }
                if matches!(&inner.kind, IrExprKind::Call { target: CallTarget::Named { name }, .. }
                    if !self.0.contains(name.as_str())));
            if strip {
                if let IrExprKind::Unwrap { expr: inner } = &expr.kind {
                    let inner = (**inner).clone();
                    *expr = inner;
                }
            }
        }
    }
    S(can_err).visit_expr_mut(body);
}

/// PROGRAM-level pre-pass: inline a MUTUAL-recursive tail SIBLING so the caller becomes DIRECT
/// self-recursive — exposing the parser loops (`flow_rec ⇄ flow_step`, `collect_seq ⇄ seq_item`, …)
/// to the append-accumulator TCO, which only fires on a SELF-call.
///
/// For a function F that calls a sibling G where G calls F back (a mutual pair) and G is called by
/// ONLY F (so dead after inlining), every `G(args)` in F is replaced by G's body with G's parameters
/// substituted by the call's `args`, and G is dropped. Semantics-preserving (a plain inline).
///
/// TRY-LOWER GUARD (no regression by construction): the inline is applied ONLY when F currently WALLS
/// *and* the inlined F then LOWERS — so a function that already lowers (e.g. `esc_rec`, `collect_block`)
/// is NEVER touched (inlining could make it self-recursive and push it into a TCO path that walls). The
/// guard lowers F and inlined-F with the program's `globals`/`record_layouts`, exactly as the real
/// lowering will, so its verdict matches.
pub fn inline_mutual_tail_recursion(
    fns: &[IrFunction],
    globals: &HashMap<VarId, Ty>,
    record_layouts: &RecordLayouts,
) -> Vec<IrFunction> {
    use std::collections::{HashMap as Map, HashSet};
    fn named_calls(body: &IrExpr) -> HashSet<String> {
        use almide_ir::visit::{walk_expr, IrVisitor};
        struct C {
            names: HashSet<String>,
        }
        impl IrVisitor for C {
            fn visit_expr(&mut self, e: &IrExpr) {
                if let IrExprKind::Call { target: CallTarget::Named { name }, .. } = &e.kind {
                    self.names.insert(name.as_str().to_string());
                }
                walk_expr(self, e);
            }
        }
        let mut c = C { names: HashSet::new() };
        c.visit_expr(body);
        c.names
    }
    // NEVER-ERR `!` STRIP (sound, the scoped form of the reverted blanket strip): an effect call whose
    // callee provably never returns `Err` has a no-op `!`, so `let pat = f()!` → `let pat = f()` and a
    // `f()!` self-call → bare `f()` (which `tco_collect` then recognizes). This is what lets the yaml
    // parser cluster (entirely never-err) TCO; `safe_div` & co. (can-err) keep their `!` and stay walled.
    // Done HERE, before the inline guard's try-lower, so inlined-F sees the stripped body and lowers.
    let can_err = compute_can_err(fns);
    let stripped: Vec<IrFunction> = fns
        .iter()
        .map(|f| {
            let mut nf = f.clone();
            strip_never_err_unwraps(&mut nf.body, &can_err);
            nf
        })
        .collect();
    let fns: &[IrFunction] = &stripped;
    let lowers =
        |f: &IrFunction| lower_function_all_with_types(f, globals, record_layouts).is_ok();
    let calls: Map<String, HashSet<String>> =
        fns.iter().map(|f| (f.name.as_str().to_string(), named_calls(&f.body))).collect();
    let mut callers: Map<String, HashSet<String>> = Map::new();
    for (f, cs) in &calls {
        for c in cs {
            callers.entry(c.clone()).or_default().insert(f.clone());
        }
    }
    let by_name: Map<&str, &IrFunction> = fns.iter().map(|f| (f.name.as_str(), f)).collect();
    let mut rewritten: Map<String, IrFunction> = Map::new();
    let mut dropped: HashSet<String> = HashSet::new();
    for f in fns {
        let fname = f.name.as_str();
        if dropped.contains(fname) {
            continue;
        }
        // G: F calls G, G calls F back, G ≠ F, G local, ONLY F calls G (droppable).
        let g = calls[fname].iter().find(|g| {
            g.as_str() != fname
                && !dropped.contains(g.as_str())
                && by_name.contains_key(g.as_str())
                && calls.get(*g).is_some_and(|gc| gc.contains(fname))
                && callers.get(*g).is_some_and(|cs| cs.len() == 1 && cs.contains(fname))
        });
        if let Some(g) = g {
            // Guard: only inline if F WALLS now and the inlined F LOWERS (else leave both untouched —
            // no regression of an already-lowering function).
            if !lowers(f) {
                let mut nf = f.clone();
                inline_sibling_calls(&mut nf.body, g, by_name[g.as_str()]);
                if lowers(&nf) {
                    rewritten.insert(fname.to_string(), nf);
                    dropped.insert(g.clone());
                }
            }
        }
    }
    fns.iter()
        .filter(|f| !dropped.contains(f.name.as_str()))
        .map(|f| rewritten.remove(f.name.as_str()).unwrap_or_else(|| f.clone()))
        .collect()
}

/// Replace every `Call(callee_name, args)` in `body` with `callee`'s body, its parameters substituted
/// by `args` (a single-level inline; the inlined body's calls — back to the OUTER fn — are left as-is,
/// turning the caller into a direct self-recursion).
fn inline_sibling_calls(body: &mut IrExpr, callee_name: &str, callee: &IrFunction) {
    use almide_ir::{walk_expr_mut, IrMutVisitor};
    struct V<'a> {
        name: &'a str,
        callee: &'a IrFunction,
    }
    impl IrMutVisitor for V<'_> {
        fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
            walk_expr_mut(self, expr);
            if let IrExprKind::Call { target: CallTarget::Named { name }, args, .. } = &expr.kind {
                if name.as_str() == self.name && args.len() == self.callee.params.len() {
                    let mut b = self.callee.body.clone();
                    for (p, a) in self.callee.params.iter().zip(args.iter()) {
                        b = almide_ir::substitute_var_in_expr(&b, p.var, a);
                    }
                    *expr = b;
                }
            }
        }
    }
    V { name: callee_name, callee }.visit_expr_mut(body);
}

/// Lower a function body expression to MIR (the param-free testable core;
/// `lower_function` is the wrapper that seeds parameters first).
pub fn lower_body(body: &IrExpr, name: &str) -> Result<MirFunction, LowerError> {
    let mut ctx = LowerCtx::default();
    let ret = ctx.lower_body_into(body)?;
    Ok(MirFunction { name: name.to_string(), ops: ctx.ops, ret, ..Default::default() })
}

/// Like [`lower_body`] but returns the main function PLUS any lambda-lifted auxiliaries
/// the body produced (index 0 is the main). The plain [`lower_body`] discards the lifted
/// set, so a test that lifts a closure must use this to see (and verify) the lifted
/// function where the closure's body — and its captured calls — now live.
#[cfg(test)]
pub(crate) fn lower_body_all(body: &IrExpr, name: &str) -> Result<Vec<MirFunction>, LowerError> {
    let mut ctx = LowerCtx { fn_name: name.to_string(), ..Default::default() };
    let ret = ctx.lower_body_into(body)?;
    let lifted = std::mem::take(&mut ctx.lifted);
    let mut all =
        vec![MirFunction { name: name.to_string(), ops: ctx.ops, ret, ..Default::default() }];
    all.extend(lifted);
    Ok(all)
}

/// Like [`lower_body`] but seeds the declared GLOBAL set (top-level `let`s) so a
/// reference to one is admitted by `value_or_global` instead of walled. Test/diagnostic
/// entry — `lower_function` builds the same context for real programs.
#[cfg(test)]
pub(crate) fn lower_body_with_globals(
    body: &IrExpr,
    name: &str,
    globals: HashMap<VarId, Ty>,
) -> Result<MirFunction, LowerError> {
    let mut ctx = LowerCtx { globals, ..Default::default() };
    let ret = ctx.lower_body_into(body)?;
    Ok(MirFunction { name: name.to_string(), ops: ctx.ops, ret, ..Default::default() })
}

#[derive(Default)]
pub(crate) struct LowerCtx {
    ops: Vec<Op>,
    /// VarId → the MIR value it denotes. Aliases map to the SAME ValueId.
    value_of: HashMap<VarId, ValueId>,
    /// Heap handles in binding order, for scope-end drops (one Drop per handle).
    live_heap_handles: Vec<ValueId>,
    /// The MIR values that are BORROWED heap parameters (the v1 calling
    /// convention): the caller owns the reference. A direct move-out/return or
    /// in-place mutation of one needs an explicit acquire (`Dup`) the body does
    /// not perform, so it is walled — never lowered to an unbacked cert event.
    param_values: HashSet<ValueId>,
    next_value: u32,
    /// Depth of enclosing control-flow FRAMES (branch arms / loop bodies). A heap
    /// reassignment at depth > 0 must NOT rebind `value_of` — the new handle would
    /// be frame-local (dropped at the frame's end), yet the var is read on the next
    /// iteration or after the branch merges, dereferencing a freed handle (a UAF the
    /// flat fold cannot see). Inside a frame such a reassignment is DEFERRED: the var
    /// keeps its still-live handle and the new value is carried like every `Opaque`.
    in_frame: u32,
    /// Depth of enclosing DEFUNCTIONALIZED HOF bodies (`list.map((x) => …)`) being lowered inline.
    /// When > 0, a SELF-RECURSIVE call in a heap-result body is BOUNDED (the map iterates a finite
    /// list; a `render_el(child, …)` recurses to the tree's depth, not unbounded), so it is ADMITTED —
    /// unlike a function-tail self-call (the unbounded TCO shape that overflows the stack), which the
    /// `lower_heap_result_arm` self-call gate still WALLS when this is 0.
    in_defunc_body: u32,
    /// Depth of enclosing SCALAR-STATE loops being lowered with real markers
    /// (`LoopStart`/`LoopBreakUnless`/`LoopEnd`). When > 0, a scalar `Assign` reassigns
    /// the var's STABLE local via [`Op::SetLocal`] (the loop-carried state) instead of
    /// rebinding `value_of` to a fresh value (which a loop back-edge could not see), and a
    /// HEAP reassignment ERRORS — that aborts the scalar-loop attempt so `lower_while`
    /// falls back to its sound model-one-iteration form (a heap accumulator is deferred,
    /// not run, exactly as before).
    scalar_loop_depth: u32,
    /// Depth of enclosing EXECUTABLE Unit (statement) `if`/`match` arms — lowered with
    /// real markers (`IfThen`/`Else`/`EndIf`) so exactly ONE arm runs at runtime. When
    /// > 0, a scalar `Assign` to a var that ALREADY has a stable local (declared outside
    /// the arm — `var r = 0`) mutates that local via [`Op::SetLocal`] instead of rebinding
    /// `value_of` to a fresh value. A fresh rebind is frame-local: `value_of[var]` ends up
    /// pointing at whichever arm was lowered LAST (last-writer-wins), so a read after the
    /// branch sees a local only that arm's `local.set` wrote — but at runtime the OTHER
    /// arm ran, leaving it unset (the `match n { 0 => {r=100}, x => {r=999} }` 0-vs-999
    /// silent miscompile). SetLocal-to-the-stable-local is the faithful in-place mutation
    /// v0 performs. Distinct from `scalar_loop_depth` (loops also block heap rebinds and
    /// roll back the whole attempt); here a heap reassignment keeps the existing branch-arm
    /// DEFER behavior. Cert-neutral: a scalar `SetLocal` carries no heap ownership (the
    /// same no-op `verify_ownership` already proves for the loop-carried SetLocal).
    unit_arm_depth: u32,
    /// The module's top-level `let` bindings (VarId → declared Ty). A reference to one
    /// of these resolves to no FUNCTION-local `value_of` entry; this DECLARED set lets
    /// `value_or_global` distinguish a legitimate global reference (materialize a fresh
    /// external value) from a genuine lowering gap (a local that should have been bound
    /// — still WALLED). Confirming against the declared set, not merely a `value_of`
    /// miss, is what keeps the boundary a wall instead of a silent hole.
    globals: HashMap<VarId, Ty>,
    /// MIR values KNOWN to be MATERIALIZED Options (the 0-or-1-element-list layout:
    /// `Some(x)` = `Init::OptSome` len=1, `None` = `Init::Opaque` len=0). A variant
    /// `match` may EXECUTE (read `len` as the tag, extract `data[0]`) ONLY over a
    /// subject in this set — every other Option (a closure/range/deferred `Opaque`, a
    /// non-self-host Option-returning call) is `Opaque` with len=0 and would MISREAD as
    /// `None`, so it keeps the sound LINEARIZED match. This is the gate that makes the
    /// len-as-tag execution safe without any global materialization invariant.
    materialized_options: HashSet<ValueId>,
    /// MIR values KNOWN to be MATERIALIZED Results (the DynListStr len-as-tag layout: `Ok(int)` =
    /// len 0 with the value in slot 0, `Err(string)` = len 1 owning the message). An `Ok`/`Err`
    /// `match` may EXECUTE (read `len` as the tag — len 0 → Ok, len != 0 → Err — and extract slot
    /// 0) ONLY over a subject in this set; any other Result is a deferred `Opaque` (len 0 → MISREADS
    /// as Ok) and keeps the sound LINEARIZED match. The Result analogue of `materialized_options`.
    materialized_results: HashSet<ValueId>,
    /// MIR values KNOWN to be MATERIALIZED HEAP-Ok Results (`Result[String, String]` etc.): a 1-slot
    /// DynListStr (cap 1, len 1 — IDENTICAL block size to every String, so the free-list reuses it)
    /// that ALWAYS owns one String in slot 0's LOW 32 bits (@12 — Ok's value OR Err's message), with
    /// the Ok/Err TAG in slot 0's HIGH 32 bits (@16: 0=Ok, 1=Err). `DropListStr` `i32.wrap`s the slot
    /// to the low-32 handle, so the high-32 tag is inert. An `Ok`/`Err` `match` reads @16 and binds
    /// the @12 handle as a borrowed String. The heap-Ok-payload analogue of `materialized_results`.
    materialized_results_str: HashSet<ValueId>,
    /// Lambda-lifted auxiliary functions produced while lowering this function's body
    /// (a non-capturing `let f = (x) => …` or a lambda call-argument lifts its body to a
    /// fresh MirFunction here, bound via `Op::FuncRef`). `lower_function_all` returns these
    /// alongside the main function so the program assembler tables + verifies them.
    lifted: Vec<crate::MirFunction>,
    /// The enclosing source function's name — the file-unique prefix for lifted lambda
    /// names (`__lambda_<fn_name>_<n>`). The corpus harness keys the in-profile map by name
    /// within a file, so two source functions each lifting `__lambda_0` would COLLIDE
    /// without this prefix (one lambda's certificate silently lost). Set by
    /// `lower_function_all`; empty for the param-free testable `lower_body` entry.
    fn_name: String,
    /// MIR values that denote a lifted lambda's table slot (an `Op::FuncRef` dst). A later
    /// call whose callee is one of these (`f(args)` where `f` bound a lifted lambda) lowers
    /// to `Op::CallIndirect` through it instead of deferring — the closure EXECUTES.
    funcref_values: HashSet<ValueId>,
    /// C1 DIRECT-CALL INLINE: source-`VarId` → the INLINE lambda (`params`, `body`) a `let f =
    /// (x) => body` statically bound. A later DIRECT call `f(args)` whose callee is this `f`
    /// is DEFUNCTIONALIZED — the body is lowered INLINE with each param bound to its arg, and
    /// the captures resolve through `value_of` (they are in scope at the call site). This is
    /// what makes `let s = "ab"; let f = (x) => string.len(s) + x; f(1)` EXECUTE (return 3)
    /// instead of deferring the capturing lambda to an Opaque + `Const 0`. A lambda that ALSO
    /// lifts (non-capturing) keeps its `funcref_values` CallIndirect path; this map is the
    /// inline route for the CAPTURING / non-lifted case (recorded for BOTH, the call site
    /// prefers inline). Cleared per function (Default).
    lambda_bindings: HashMap<VarId, (Vec<(VarId, Ty)>, IrExpr)>,
    /// MIR values that are `List[String]` (NESTED-OWNERSHIP lists — their i64 slots hold OWNED
    /// String handles). A scope-end drop of one emits [`Op::DropListStr`] (recursive free),
    /// not a flat [`Op::Drop`] — so the element Strings are reclaimed. Populated when an
    /// `alloc_list_str` result or a `List[String]`-typed bind is created (Machinery 2).
    heap_elem_lists: HashSet<ValueId>,
    /// MIR values that are a `List[List[String]]` (the csv `rows` shape: a list whose element slots
    /// hold owned `List[String]` blocks). A scope-end drop emits [`Op::DropListListStr`] (a NESTED
    /// free: each row's cell Strings, then each row block, then the outer block) — a flat
    /// `DropListStr` would only `rc_dec` each inner-list handle, LEAKING the cells. Populated by the
    /// list-of-lists concat (`rows + [cur]`).
    list_list_str_lists: HashSet<ValueId>,
    /// MIR values that are a `Result[Value, String]` (the `ok(value.array(...))` shape). A scope-end
    /// drop emits [`Op::DropResultValue`] (tag-dispatch: Ok → `$__drop_value`, Err → `rc_dec`) — a
    /// flat `DropListStr` would leak the Ok Value's nested payload.
    value_result_results: HashSet<ValueId>,
    /// MIR values KNOWN to be a REAL, POPULATED list block (a list LITERAL, a heap-list PARAM —
    /// the v1 convention passes a genuine block —, or a self-host list-returning CALL whose closure
    /// args ALL lifted, so the callee actually fills it). A direct `xs[i]` (`lower_scalar_index_access`)
    /// computes a bounds-checked `$elem_addr` load that TRAPS on `i >= cap`, so it may fire ONLY over
    /// a value in this set: an Opaque/deferred list (a `list.map` whose param-invoking lambda could
    /// NOT lift → an empty/garbage block) has cap 0 and would TRAP at `xs[0]`, a NEW crash where the
    /// deferred `Const 0` merely mis-valued. Gating on real materialization keeps `xs[i]` from
    /// regressing an unmaterialized-list program to a runtime trap.
    materialized_lists: HashSet<ValueId>,
    /// Set true by `lower_pure_module_call_args` when a closure ARGUMENT to a pure combinator could
    /// NOT be lifted to a FuncRef (a capturing / param-invoking lambda — `list.map(fns, (f) => f(10))`)
    /// and so fell back to `record_elided_calls`. The auto-linked self-host combinator then runs with
    /// a MISSING closure slot → an empty / garbage result list, NOT a faithfully-filled one. The
    /// `list.map` bind reads this to decide whether the result is a `materialized_lists` member (safe
    /// to index directly) — a genuinely-lifted map fills the list (admit `xs[i]`), an unlifted one
    /// does not (defer `xs[i]` to `Const 0`, no trap). Reset before each module-call arg lowering.
    last_call_had_unlifted_closure: bool,
    /// MIR values of the dynamic `Value` type (the Codec data model). A scope-end drop emits
    /// [`Op::DropValue`] (runtime-tag-dispatched: a Str/Array/Object Value frees its one heap
    /// payload, a scalar Value just frees the block) instead of a flat [`Op::Drop`]. Populated
    /// when a `Value`-typed bind is created.
    value_handles: HashSet<ValueId>,
    /// MIR values that are `List[Value]` (a list whose i64 slots hold OWNED dynamic `Value` handles,
    /// each itself possibly a heap-payload Str/Array). A scope-end drop emits [`Op::DropListValue`]
    /// (recursive `$__drop_value` per element) instead of the flat [`Op::DropListStr`], which would
    /// leak each element Value's nested payload. Populated when a `List[Value]` literal/arg is
    /// materialized. Distinct from `heap_elem_lists` (String elements, whose `rc_dec` is the full free).
    value_elem_lists: HashSet<ValueId>,
    /// MIR values that are a `List[(String, Value)]` whose element slots hold owned (String, Value)
    /// TUPLE blocks (the yaml `pairs` shape). A scope-end drop emits [`Op::DropListStrValue`]
    /// (`$__drop_list_str_value`: per tuple, rc_dec the String slot + recursive `$__drop_value` the Value
    /// slot, then the tuple, then the list) — a flat [`Op::DropListStr`] would leak each tuple's payloads.
    /// Populated when a `List[(String,Value)]` concat is materialized via `__list_concat_rc`.
    str_value_elem_lists: HashSet<ValueId>,
    /// MIR values that are a `List[(String, String)]` (the `map.entries` / svg render_attrs shape) —
    /// element slots hold owned (String, String) TUPLE blocks. A scope-end drop emits
    /// [`Op::DropListStrStr`] (`$__drop_list_str_str`: per tuple, rc_dec BOTH String slots, then the
    /// tuple, then the list). The (String,String) counterpart of `str_value_elem_lists`.
    str_str_elem_lists: HashSet<ValueId>,
    /// MIR values that are a `value.as_array` Result `Result[List[Value], String]` (the cap-as-tag
    /// 1-slot block whose Ok payload @12 is a `List[Value]`). A scope-end drop emits
    /// [`Op::DropResultListValue`] (`$__drop_result_lv`: Ok → recursive list free, Err → String free)
    /// instead of the flat [`Op::DropListStr`] (which leaks the list's element Values). Read by the
    /// SAME cap@16 match machinery as a str-result (`materialized_results_str`); only the DROP differs.
    value_result_lists: HashSet<ValueId>,
    /// MIR values KNOWN to be a record/tuple block this brick MATERIALIZED with the uniform
    /// slot layout (`try_lower_scalar_record_construct` / `try_lower_record_construct` /
    /// `try_lower_scalar_tuple_construct` / scalar-tuple/list-slot), plus aggregate-typed
    /// params (the v1 convention passes the same-layout block pointer). A PRECISE field read
    /// that DEREFERENCES a loaded slot — a heap-field BORROW (`b.label`), which passes the
    /// loaded handle to a String/List consumer — is admitted ONLY over a value in this set:
    /// a DEFERRED `Alloc{Opaque}` aggregate (a spread record / a call result) has ZERO
    /// (garbage) slot handles, so loading + dereferencing one would TRAP at `rc_dec`. (A
    /// scalar field read does not dereference, so it tolerates a 0 slot as a benign mis-read;
    /// but a heap-field deref must be gated on REAL materialization.)
    materialized_aggregates: HashSet<ValueId>,
    /// MIR values that are MIXED scalar+heap record/tuple blocks → the i64-SLOT INDICES that
    /// hold an OWNED heap handle (a `String`/`List`/nested-aggregate field). Such a value's
    /// scope-end / per-iteration drop emits a [`Op::DropListStr`] (cert = the SAME single `d`
    /// as any drop — each heap field was accounted `m` when stored), and the render frees
    /// exactly these slots (then the block) via the per-value mask carried on the
    /// [`MirFunction::heap_slot_masks`] side table. A value here is treated like a
    /// `heap_elem_lists` member for drop-op SELECTION, but the mask makes the recursive free
    /// touch only the heap slots (NOT every slot — the scalar fields must not be `rc_dec`'d).
    record_masks: HashMap<ValueId, Vec<usize>>,
    /// The CURRENT binding (`lower_bind`) is a MUTABLE `var` (set by `lower_stmt` from the
    /// `Bind` mutability). A `var b = r.items` heap-field extraction may be COW-mutated later,
    /// so it must take an OWNED container-grain `Dup` (mutable in place), NOT a precise borrow
    /// (a shared field handle the value-model refuses to mutate). Read by `lower_heap_extraction`.
    binding_is_mutable: bool,
    /// Named-record layout registry (the VALUE-MODEL field structure): type NAME →
    /// (declared generic param names, declared fields in declaration order). A record
    /// literal / field access typed `Ty::Named(name, args)` resolves its fields here
    /// (substituting the generic params with `args`), so `r.x` loads from the same slot
    /// construction stored to. Empty when lowering without a type registry (the
    /// param-free testable entry) — a `Ty::Named` aggregate then stays walled, a
    /// `Ty::Record`/`Ty::Tuple` (structurally typed) still resolves directly.
    record_layouts: RecordLayouts,
    /// Custom-variant (ADT) layout registry (the tag + per-constructor field structure):
    /// type NAME → its [`VariantLayout`], with a ctor-name → type reverse index. A variant
    /// CONSTRUCT / `match` resolves its tag and field slots here, the value-model sibling of
    /// `record_layouts`. Empty when lowering without a type registry — a variant value then
    /// stays walled (the pre-ADT-brick status quo). Populated by [`build_variant_layouts`]
    /// and threaded via [`lower_function_all_with_layouts`].
    variant_layouts: VariantLayouts,
    /// Constructed CUSTOM-VARIANT values whose scope-end drop must be the RECURSIVE
    /// [`Op::DropVariant`] (a nested-variant type — `Add(Expr, Expr)` — whose flat free would leak
    /// child blocks), mapped to their TYPE NAME (so the render calls the generated `$__drop_<ty>`).
    /// `drop_op_for` consults this before the flat/masked drops. Populated by
    /// `try_lower_variant_ctor` for a type that [`VariantLayouts::needs_recursive_drop`] (ADT brick 5b).
    variant_drop_handles: HashMap<ValueId, String>,
}

/// Type NAME → (generic param names, declaration-ordered fields) — the VALUE-MODEL
/// field registry threaded into lowering (see [`LowerCtx::record_layouts`]).
pub type RecordLayouts =
    HashMap<String, (Vec<almide_lang::intern::Sym>, Vec<(almide_lang::intern::Sym, Ty)>)>;

/// One constructor of a variant type, as the value model sees it: its name, its `tag`
/// (the declaration index — `type E = Lit(Int) | Add(E,E) | Neg(E)` gives Lit=0, Add=1,
/// Neg=2), and its declaration-ordered fields. A TUPLE constructor's positional fields
/// are named `_0`, `_1`, … and a RECORD constructor keeps its declared names — the same
/// synthesis v0 (`emit_wasm` variant registration) uses, so the two backends agree on
/// field identity. A UNIT constructor has no fields.
#[derive(Clone, Debug)]
pub struct VariantCaseLayout {
    pub ctor: almide_lang::intern::Sym,
    pub tag: u32,
    pub fields: Vec<(almide_lang::intern::Sym, Ty)>,
}

/// One variant type's VALUE-MODEL layout. A v1 variant value is a record-like heap block
/// in the SAME uniform-i64-slot model records use (NOT v0's byte-packed layout — only the
/// OBSERVABLE output must match v0, never the internal bytes): `slot 0` holds the tag and
/// `slots 1..` hold the ACTIVE constructor's fields. `slot_count` is `1 + max arity over
/// all cases`, so EVERY constructor of the type occupies an identically sized block — a
/// uniform alloc and a sound `==` over the whole block, the v1 analogue of v0's
/// max-payload padding (`variant_alloc_size`).
#[derive(Clone, Debug)]
pub struct VariantLayout {
    pub generics: Vec<almide_lang::intern::Sym>,
    /// Indexed by tag (`cases[t].tag == t`).
    pub cases: Vec<VariantCaseLayout>,
    pub slot_count: usize,
}

impl VariantLayout {
    /// The case whose constructor is `ctor`, if any.
    pub fn case_by_ctor(&self, ctor: &str) -> Option<&VariantCaseLayout> {
        self.cases.iter().find(|c| c.ctor.as_str() == ctor)
    }
}

/// The variant-type sibling of [`RecordLayouts`]: type NAME → its [`VariantLayout`], plus a
/// constructor-name → owning-type reverse index (a `Lit(7)` constructor expression carries
/// its ctor name; this resolves the variant type the way v0's `find_variant_tag_by_ctor`
/// fallback does). Threaded into lowering alongside `record_layouts` so a variant
/// construct / `match` can find its tag + field layout. Empty when lowering without a type
/// registry — a variant value then stays walled (the pre-ADT-brick status quo).
#[derive(Clone, Debug, Default)]
pub struct VariantLayouts {
    pub by_type: HashMap<String, VariantLayout>,
    pub ctor_to_type: HashMap<String, String>,
}

impl VariantLayouts {
    /// Resolve a constructor name to its owning type's name + layout + the specific case.
    pub fn lookup_ctor(&self, ctor: &str) -> Option<(&str, &VariantLayout, &VariantCaseLayout)> {
        let ty = self.ctor_to_type.get(ctor)?;
        let layout = self.by_type.get(ty)?;
        let case = layout.case_by_ctor(ctor)?;
        Some((ty.as_str(), layout, case))
    }

    /// Does the variant type `type_name` need the RECURSIVE [`Op::DropVariant`] (the generated
    /// `$__drop_<ty>`) — i.e. does some ctor field hold another user variant whose flat free would
    /// leak its children? A String-only-field variant uses the masked `DropListStr` instead (ADT
    /// brick 5a/5c). This is the lowering-side mirror of
    /// [`crate::lower::variant_needs_recursive_drop`], computed from the registry's field Tys.
    pub fn needs_recursive_drop(&self, type_name: &str) -> bool {
        let Some(layout) = self.by_type.get(type_name) else { return false };
        layout.cases.iter().any(|c| {
            c.fields.iter().any(|(_, ty)| self.field_is_variant(ty))
        })
    }

    /// Is `ty` one of the variant types in this registry (a nested-variant ctor field)?
    pub fn field_is_variant(&self, ty: &Ty) -> bool {
        use almide_lang::types::constructor::TypeConstructorId;
        let n = match ty {
            Ty::Named(n, _) => n.as_str(),
            Ty::Variant { name, .. } => name.as_str(),
            Ty::Applied(TypeConstructorId::UserDefined(n), _) => n.as_str(),
            _ => return false,
        };
        self.by_type.contains_key(n)
    }

    /// The variant type NAME of `ty` if it is a registry variant (the recursion / construct target).
    pub fn field_variant_name(&self, ty: &Ty) -> Option<String> {
        use almide_lang::types::constructor::TypeConstructorId;
        let n = match ty {
            Ty::Named(n, _) => n.as_str().to_string(),
            Ty::Variant { name, .. } => name.as_str().to_string(),
            Ty::Applied(TypeConstructorId::UserDefined(n), _) => n.clone(),
            _ => return None,
        };
        self.by_type.contains_key(&n).then_some(n)
    }
}

/// Is `ty` the dynamic `Value` type (the Codec data model)? Its scope-end drop is the
/// runtime-tag-dispatched [`Op::DropValue`], since a heap-payload Value (Str/Array/Object) owns a
/// handle the flat `Drop` would leak.
pub fn is_value_ty(ty: &Ty) -> bool {
    match ty {
        Ty::Named(name, _) => name.as_str() == "Value",
        Ty::Variant { name, .. } => name.as_str() == "Value",
        _ => false,
    }
}

/// Does `ty` CONTAIN a function type anywhere (a `Ty::Fn`, or a List/Option/etc. OF functions —
/// `List[(Int) -> Int]`)? A self-host list combinator over such an argument (`list.map(fns, …)`
/// where `fns: List[(Int)->Int]`) cannot faithfully fill its result (the v1 model has no
/// representation for a list of closures), so the result is empty/garbage and must NOT be treated
/// as a real `materialized_lists` block (a direct `xs[i]` over it would trap on cap 0).
pub(crate) fn ty_contains_fn(ty: &Ty) -> bool {
    match ty {
        Ty::Fn { .. } => true,
        Ty::Applied(_, args) => args.iter().any(ty_contains_fn),
        Ty::Tuple(tys) => tys.iter().any(ty_contains_fn),
        _ => false,
    }
}

/// Is `ty` a `List[T]` whose element `T` is a SCALAR (non-heap) type (`List[Int/Float/Bool]`)?
/// Such a list's slots are plain i64 values — a direct `xs[i]` reads one with `Load { width: 8 }`,
/// and `__list_concat` byte-copies them with no ownership. The complement of `is_heap_elem_list_ty`
/// for the List constructor.
pub(crate) fn is_scalar_elem_list_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty,
        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && !is_heap_ty(&a[0]))
}

/// Is `ty` a `List[T]` / `Option[T]` whose element `T` is itself a HEAP type (e.g. `List[String]`,
/// `Option[String]`)? Such a container OWNS its element(s) — it needs the recursive
/// [`Op::DropListStr`], not a flat drop. An `Option[String]` is physically a 0-or-1-element
/// `List[String]` (Machinery 2), so the SAME recursive free applies (len 0 frees nothing, len 1
/// frees the one element + the block).
/// A `List[List[String]]` — its element slots hold owned `List[String]` blocks (the csv `rows`
/// shape). Its scope-end drop must be [`Op::DropListListStr`] (the nested cell + row free); a flat
/// `DropListStr` (what `is_heap_elem_list_ty` would route it to, since List[List[String]] is also a
/// `List[heap]`) would only `rc_dec` each row HANDLE, leaking the cell Strings. So EVERY tracking
/// site checks this FIRST.
pub(crate) fn is_list_list_str_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty,
        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(&a[0],
            Ty::Applied(TypeConstructorId::List, b) if b.len() == 1 && matches!(b[0], Ty::String)))
}

/// A `List[(String, String)]` — the `map.entries` / render_attrs shape. Each element is an owned
/// (String, String) TUPLE; its scope-end drop must be [`Op::DropListStrStr`] (per tuple: rc_dec BOTH
/// String slots, then the tuple, then the list). The flat `DropListStr` (`heap_elem_lists`) would
/// rc_dec only the tuple HANDLE — freeing the tuple block but LEAKING its two Strings (a render loop
/// OOMs). Checked BEFORE `is_heap_elem_list_ty` (which also matches this List type).
pub(crate) fn is_list_str_str_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty,
        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(&a[0],
            Ty::Tuple(tys) if tys.len() == 2 && matches!(tys[0], Ty::String) && matches!(tys[1], Ty::String)))
}

/// A `Result[Value, String]` — the `ok(value.array(...))` shape. Its Ok payload is a dynamic Value
/// (freed RECURSIVELY via `$__drop_value`), its Err a String. Its scope-end drop must be
/// [`Op::DropResultValue`] (the tag-dispatched recursive free); a flat `DropListStr` would leak the
/// Ok Value's nested payload.
pub fn is_value_result_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty, Ty::Applied(TypeConstructorId::Result, a)
        if a.len() == 2 && is_value_ty(&a[0]) && matches!(a[1], Ty::String))
}

pub(crate) fn is_heap_elem_list_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    match ty {
        // `List[heap]` / `Option[heap]` / `Set[heap]` — heap element slots (DynListStr nested
        // ownership). A `Set[heap]` is physically a `List[heap]` of unique elements, so the SAME
        // recursive free applies (each owned element + the block).
        Ty::Applied(TypeConstructorId::List | TypeConstructorId::Option | TypeConstructorId::Set, args)
            if args.len() == 1 && is_heap_ty(&args[0]) =>
        {
            true
        }
        // `Result[_, heap-Err]` is physically the SAME DynListStr (the Ok/Err materialization reuses
        // it): `Err` owns the heap Err payload in slot 0 (len 1 → DropListStr frees it), `Ok` is
        // len 0 (frees nothing). So a Result value is dropped recursively, exactly like Option[heap].
        Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 && is_heap_ty(&args[1]) => {
            true
        }
        // `Map[heap, heap]` (e.g. `Map[String, String]`) — a DynListStr of INTERLEAVED key+value
        // String handles [k0,v0,k1,v1,...]; EVERY slot is a heap handle, so the uniform recursive
        // DropListStr frees all keys and values. (`len` = the slot count; map.len reads len/2.)
        Ty::Applied(TypeConstructorId::Map, args)
            if args.len() == 2 && is_heap_ty(&args[0]) && is_heap_ty(&args[1]) =>
        {
            true
        }
        _ => false,
    }
}

impl LowerCtx {
    pub(crate) fn fresh_value(&mut self) -> ValueId {
        let id = ValueId(self.next_value);
        self.next_value += 1;
        id
    }

    /// Seed the parameters: each param's VarId maps to a fresh MIR value (so uses
    /// in the body resolve) and becomes a [`MirParam`] carrying its [`Repr`] (so
    /// the name-totality witness counts it as DEFINED — every param use must have
    /// a defining param). A HEAP param is BORROWED (the caller owns the reference
    /// — it contributes no owned `+1` to the ownership certificate; the cert and
    /// verifier guard on `repr.is_heap()`) and is recorded in `param_values` so a
    /// later move-out/mutation of a bare borrowed param is walled, not faked. A
    /// scalar param carries no ownership but is still a defined value.
    pub(crate) fn bind_params(&mut self, params: &[IrParam]) -> Result<Vec<MirParam>, LowerError> {
        let mut out = Vec::new();
        for p in params {
            let v = self.fresh_value();
            self.value_of.insert(p.var, v);
            // A FUNCTION-typed param (`f: (Int) -> Int`, the closures machinery) is a SCALAR
            // table slot (an i64 index into the module function table), NOT a heap value:
            // the caller passes the lifted lambda's `FuncRef` value. So it gets a scalar
            // Repr and joins `funcref_values` — a `f(x)` call in the body then lowers to
            // `Op::CallIndirect` through it (the dynamic-closure path; cap_witness taints
            // it conservatively, so a higher-order function stays honestly caps-unverified).
            // This is what lets `list.map`/`filter`/`fold` be self-hosted in Almide.
            let repr = if matches!(p.ty, Ty::Fn { .. }) {
                let r = Repr::Scalar { width: crate::ScalarWidth::Double };
                self.funcref_values.insert(v);
                r
            } else {
                repr_of(&p.ty)? // Ptr (heap) / Scalar; Unsupported if Unknown or non-value
            };
            if repr.is_heap() {
                self.param_values.insert(v);
                // A heap variant param (`Option[T]` / `Result[T, String]`) is passed by the caller
                // as a REAL materialized block of the SAME layout the constructors build (the v1
                // calling convention — see `param_values` in `try_lower_option_unwrap_or`). SEED its
                // variant-tracking so a `match`/`??` over the PARAM inside the callee EXECUTES (reads
                // the real tag/payload) instead of LINEARIZING (running both arms = garbage). Without
                // this, `fn show(r: Result[Int,String]) = match r { Ok=>…, Err=>… }` ran both arms.
                // SOUND: a borrowed variant param owns nothing here (it stays `param_values`,
                // un-dropped — the caller owns it), so seeding it only changes how the match READS
                // the tag/payload (scalar prims, no ownership event), never the drop discipline.
                self.seed_variant_param(v, &p.ty);
            }
            out.push(MirParam { value: v, repr });
        }
        Ok(out)
    }

    /// Seed the variant-tracking sets for a heap `Option`/`Result` PARAM so a `match`/`??` over
    /// it executes (the caller passes a real same-layout block — the v1 calling convention). The
    /// classification MIRRORS the let-bind call-result tracking in `lower_bind` exactly:
    ///   - `Option[scalar]`        → `materialized_options`            (len-as-tag, scalar payload)
    ///   - `Option[heap]`          → `materialized_options` + `heap_elem_lists` (borrowed handle)
    ///   - `Result[scalar, heap]`  → `materialized_results`            (len-as-tag, scalar Ok)
    ///   - `Result[heap, heap]`    → `materialized_results_str` + `heap_elem_lists` (cap-as-tag)
    /// `param_values` already holds the borrowed handle (the caller owns it), so this adds only the
    /// READ-shape knowledge, no ownership change.
    fn seed_variant_param(&mut self, v: ValueId, ty: &Ty) {
        use almide_lang::types::constructor::TypeConstructorId;
        match ty {
            Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1 => {
                self.materialized_options.insert(v);
                if is_heap_ty(&a[0]) {
                    self.heap_elem_lists.insert(v);
                }
            }
            Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 => {
                if is_heap_ty(&a[0]) && is_heap_ty(&a[1]) {
                    // Both arms heap — the cap-as-tag 1-slot DynListStr. The DROP differs by Ok-arm:
                    // a `List[Value]` Ok (`value.as_array`) frees recursively (`value_result_lists`),
                    // else a String Ok (`value.as_string`) frees flat (`heap_elem_lists`).
                    self.materialized_results_str.insert(v);
                    if is_result_listval_ty(ty) {
                        self.value_result_lists.insert(v);
                    } else if is_value_result_ty(ty) {
                        self.value_result_results.insert(v);
                    } else {
                        self.heap_elem_lists.insert(v);
                    }
                } else {
                    // Scalar Ok (`Result[Int, String]`) — len-as-tag, scalar Ok payload. A heap Err
                    // payload is owned by the Result block (DropListStr frees it); mark the nested-
                    // ownership so an `Err(e)` arm binds the borrowed slot-0 handle.
                    self.materialized_results.insert(v);
                    if is_heap_ty(&a[1]) {
                        self.heap_elem_lists.insert(v);
                    }
                }
            }
            // A RECORD / TUPLE param (`fn f(r: R)`, `fn f(t: (Int, String))`, and the closure
            // params of a lifted lambda — `(r) => r.name` over a `List[R]`) is passed by the
            // caller as a REAL materialized block of the SAME uniform-slot layout the
            // constructors build (the v1 calling convention). SEED it as a materialized
            // aggregate so a `r.field` / `t.i` access inside the callee READS its real slot
            // (a scalar `Load`, a heap `LoadHandle` BORROW) instead of returning the empty
            // deferred value. Gated to a type the layout registry can RESOLVE (a registered
            // `Ty::Named` record or a structural `Ty::Record`/`Ty::Tuple`) — a String/List/
            // Map heap param is NOT an aggregate (`aggregate_field_tys` is `None`) so it is
            // never mis-seeded.
            //
            // SOUNDNESS: a record/tuple param is BORROWED (it stays in `param_values`,
            // un-dropped — the caller owns it). Seeding `materialized_aggregates` adds ONLY
            // the READ-shape knowledge (scalar/handle prim loads of its real slots), NEVER an
            // ownership event or a drop — exactly the variant-param reasoning above. A heap
            // FIELD read is a `LoadHandle` BORROW (recorded in `param_values`, not a second
            // owner), so the field's owner (the caller's block) frees it once — no leak / no
            // double-free.
            Ty::Record { .. } | Ty::Tuple(_) | Ty::Named(..)
                if self.aggregate_field_tys(ty).is_some() =>
            {
                self.materialized_aggregates.insert(v);
            }
            _ => {}
        }
    }

    /// Lower a function body (statements + tail + scope-end drops) into `self` —
    /// the shared core of `lower_function` (params pre-seeded) and `lower_body`.
    ///
    /// An expression-bodied function (`fn f() = expr`) is the SAME value-semantics
    /// subset as a block body — just an empty statement list whose tail IS the
    /// expression. The tail lowering walls anything outside the subset, so the
    /// wrapping never weakens the boundary (control-flow / unsupported tails still
    /// become an explicit `Unsupported`).
    pub(crate) fn lower_body_into(&mut self, body: &IrExpr) -> Result<Option<ValueId>, LowerError> {
        // TAIL-DUPLICATION desugar: a `let s = <heap-result if/match>; <rest>` (which `lower_bind`
        // walls — the merged-dst has no sound flat-cert scope-end drop) is rewritten PURELY in the
        // IR to push the continuation `<rest>` into each arm (`if c then { let s = A; <rest> } else
        // …`), turning the branch into the block TAIL. The rewritten body then lowers through the
        // ordinary statements+tail path — no special dispatch — so each branch independently binds +
        // drops its own `s` (the per-arm `i…d` balance the proven checker already accepts). The
        // SAME rewrite runs in the caps `count_ir_calls` gate ("desugar-before-both"), so the
        // duplicated calls stay 1:1 between MIR and IR by construction. `lower_tail`'s per-position
        // `if` machinery (Unit/scalar/heap) walls any unfaithful arm explicitly.
        // ANF-LIFT a heap-result `if`/`match` out of a call ARGUMENT first (`println(if c then
        // "a" else "b")` → `let tmp = if..; println(tmp)`), so the tail-duplication below then
        // recovers it. Same rewrite runs in the count gate (desugar-before-both).
        if let Some(rewritten) = desugar_heap_branches(body) {
            return self.lower_body_into(&rewritten);
        }
        let (stmts, tail): (&[IrStmt], Option<&IrExpr>) = match &body.kind {
            IrExprKind::Block { stmts, expr } => (stmts, expr.as_deref()),
            _ => (&[], Some(body)),
        };
        for stmt in stmts {
            self.lower_stmt(stmt)?;
        }
        // The tail expression is the function's return value. A HEAP tail is MOVED
        // OUT to the caller (recorded as `ret`, not dropped at scope end); a scalar
        // tail carries no ownership; a Unit/absent tail is a Unit-returning body.
        let ret = self.lower_tail(tail)?;
        // Scope end: release every still-live heap handle (the moved-out return is
        // already removed). Aliases share a ValueId, so one Drop per HANDLE
        // balances the Alloc(+1) and each aliasing Dup(+1).
        self.emit_scope_end_drops();
        Ok(ret)
    }

    pub(crate) fn lower_stmt(&mut self, stmt: &IrStmt) -> Result<(), LowerError> {
        // (The Try/Unwrap early-return-over-a-live-heap-local wall is LIFTED: the v0 wasm
        // codegen now frees the live heap locals before the Err-path `return_`
        // [emit_wasm: emit_early_return_decs], so the deferred-continue cert is faithful
        // on both targets — no leak. See docs/roadmap/active/v0-unwrap-early-return-leak.md.)
        match &stmt.kind {
            IrStmtKind::Bind { var, ty, value, mutability } => {
                // A MUTABLE (`var`) binding may be COW-mutated later, so a heap-field
                // extraction (`var b = r.items`) must take an OWNED copy (container-grain
                // `Dup`), NOT a precise borrow (which cannot be mutated in place). Flag it so
                // `lower_heap_extraction` skips the borrow optimization for this bind.
                let prev = self.binding_is_mutable;
                self.binding_is_mutable = matches!(mutability, almide_ir::Mutability::Var);
                let r = self.lower_bind(*var, ty, value);
                self.binding_is_mutable = prev;
                r
            }
            // `x = value` — reassignment.
            //
            // At function TOP LEVEL: REBIND `x` to the new value (reusing
            // `lower_bind`). The OLD binding's handle stays in `live_heap_handles`
            // and is dropped at scope end — a conservative lifetime EXTENSION
            // (memory-safe, never a double-free: the old object is dropped exactly
            // once, at scope end, instead of at the reassignment). A read of the
            // old `x` inside `value` (e.g. `x = f(x)`) lowers BEFORE the rebind
            // overwrites `value_of[x]`, so it borrows the still-live old handle —
            // never a use-after-free.
            //
            // Inside a control-flow FRAME (`in_frame > 0`): a HEAP rebind would
            // repoint `value_of[x]` to a frame-local handle the per-iteration / per-arm
            // teardown drops, while `x` is read on the next iteration or after the
            // branch merges → UAF. So DEFER it — `x` keeps its still-live handle (the
            // loop/branch accumulator stays memory-safe), and the new value is carried
            // like every `Opaque`; capture its calls so the caps fold stays honest. A
            // SCALAR reassignment (`i = i + 1`) rebinds to a Copy `Const` with no handle
            // to dangle, so it is admitted unchanged (e.g. a loop counter).
            IrStmtKind::Assign { var, value } => {
                // Inside a scalar-marker loop, a reassignment mutates the var's STABLE
                // local (the loop-carried state) — `SetLocal`, not a fresh rebind. A heap
                // reassignment cannot run this way (the accumulator would need real heap
                // merge): ERROR to abort the attempt → `lower_while` falls back to its
                // sound model-one-iteration form.
                if self.scalar_loop_depth > 0 {
                    if is_heap_ty(&value.ty) {
                        // APPEND ACCUMULATOR (option C): `slot = slot + [x]` → alloc the new list, DROP
                        // the old slot, rebind the slot IN PLACE (`SetLocal`). The slot is an OWNED
                        // loop-carried list (initialized to an owned copy of the param before the loop by
                        // the TCO); each iteration drops the previous object + acquires the new one — the
                        // cert-`i(id)m` loop-carried slot PROVED leak/double-free-free for any iteration
                        // count (OwnershipChecker.v `check_line_unroll_sound`). Only a SELF-append
                        // (`Var(slot) + …`) qualifies; any other heap reassign still defers below.
                        if let IrExprKind::BinOp {
                            op: almide_ir::BinOp::ConcatList,
                            left,
                            ..
                        } = &value.kind
                        {
                            if matches!(&left.kind, IrExprKind::Var { id } if id == var) {
                                if let Some(&slot_local) = self.value_of.get(var) {
                                    if let Some(new) = self.try_lower_concat_list(value) {
                                        let drop_op = self.drop_op_for(slot_local);
                                        self.ops.push(drop_op);
                                        self.ops
                                            .push(Op::SetLocal { local: slot_local, src: new });
                                        return Ok(());
                                    }
                                }
                            }
                        }
                        // RESET to a fresh EMPTY heap value (`cur = []` / `acc = ""` — the parser
                        // resets the current-row accumulator after a delimiter): materialize the empty
                        // block, drop the old slot, rebind IN PLACE. Not a ConcatList (fast-path) nor
                        // a `lower_owned_heap_field` shape, so handle it here. Cert: drop-old (`d`) +
                        // alloc (`i`) = the same loop-carried `i(id)` the append slot proves.
                        if let Some(&slot_local) = self.value_of.get(var) {
                            let empty = match &value.kind {
                                IrExprKind::List { elements } if elements.is_empty() => Some(
                                    crate::Init::IntList(vec![]),
                                ),
                                IrExprKind::LitStr { value: s } if s.is_empty() => {
                                    Some(crate::Init::Str(String::new()))
                                }
                                _ => None,
                            };
                            if let Some(init) = empty {
                                let new = self.fresh_value();
                                self.ops.push(Op::Alloc {
                                    dst: new,
                                    repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
                                    init,
                                });
                                let drop_op = self.drop_op_for(slot_local);
                                self.ops.push(drop_op);
                                self.ops.push(Op::SetLocal { local: slot_local, src: new });
                                return Ok(());
                            }
                        }
                        // GENERAL loop-carried heap slot — `slot = <any fresh-owned heap expr>`: a
                        // non-self list/string concat (`result = rows + [cur]`), or a call result
                        // (`result = paf(text, np, rows, cur + [field])` — the TCO RESULT ACCUMULATOR
                        // that carries a base case out of the loop, where its loop-body-local inputs
                        // like a destructured `field` are still live). Each builds a FRESH owned value
                        // (cert `i`); drop the old slot (`d`) and rebind in place (`m`) — the SAME
                        // loop-carried `i(id)m` the self-append/reset slots prove (OwnershipChecker.v
                        // `check_line_unroll_sound`), generalized to any fresh-owned producer.
                        if let Some(&slot_local) = self.value_of.get(var) {
                            let new = match &value.kind {
                                IrExprKind::BinOp { op: almide_ir::BinOp::ConcatList, .. } => {
                                    self.try_lower_concat_list(value)
                                }
                                IrExprKind::BinOp { op: almide_ir::BinOp::ConcatStr, .. } => {
                                    self.try_lower_concat_str(value)
                                }
                                _ => self.lower_owned_heap_field(value),
                            };
                            if let Some(new) = new {
                                if new != slot_local {
                                    let drop_op = self.drop_op_for(slot_local);
                                    self.ops.push(drop_op);
                                    self.ops.push(Op::SetLocal { local: slot_local, src: new });
                                    self.live_heap_handles.retain(|&v| v != new);
                                    return Ok(());
                                }
                            }
                        }
                        return Err(LowerError::Unsupported(
                            "heap reassignment in a scalar loop body".into(),
                        ));
                    }
                    let local = *self.value_of.get(var).ok_or_else(|| {
                        LowerError::Unsupported("scalar loop reassigns an unbound var".into())
                    })?;
                    // The reassigned value is a SCALAR: a literal/arithmetic (lower_scalar_value) OR a
                    // scalar-returning CALL (`last = string.len(e)` / `list.len(xs)`). Without the call
                    // fallback the whole `while` rolls back to model-one-iteration (runs the body ONCE
                    // → wrong accumulation AND — worse — it MASKS per-iteration leaks: a body that
                    // leaks each turn looks clean when run once). A heap value was already rejected
                    // above, so this only admits a scalar; the call's caps stay in the cert (a real
                    // CallFn). Faithful-execution by design: this surfaces real leaks, it does not hide
                    // them (see the set.from_list/string.split in-loop known-hole).
                    let src = self
                        .lower_scalar_value(value)
                        .or_else(|| self.try_lower_scalar_call(value, &value.ty))
                        .ok_or_else(|| {
                            LowerError::Unsupported(
                                "non-scalar value in a scalar loop reassignment".into(),
                            )
                        })?;
                    self.ops.push(Op::SetLocal { local, src });
                    return Ok(());
                }
                // Inside an EXECUTABLE Unit (statement) arm, a SCALAR reassignment of a var
                // that ALREADY has a stable local (declared outside the arm) mutates that
                // local IN PLACE via `SetLocal` — exactly as v0 does — instead of a fresh
                // rebind. A rebind is frame-local: `value_of[var]` would end up pointing at
                // whichever arm lowered LAST, so a read after the branch sees a local only
                // that arm's `local.set` wrote, while at runtime the OTHER arm ran (the
                // `match n { 0 => {r=100}, x => {r=999} }` silent miscompile). The value must
                // be a SCALAR lowerable to a single value (literal/arithmetic/scalar call);
                // a heap reassignment keeps the existing branch-arm DEFER below. The local
                // is the var's own already-defined slot, so SetLocal carries no new heap
                // ownership (cert-neutral, like the loop-carried SetLocal above).
                if self.unit_arm_depth > 0 && !is_heap_ty(&value.ty) {
                    if let Some(&local) = self.value_of.get(var) {
                        if let Some(src) = self
                            .lower_scalar_value(value)
                            .or_else(|| self.try_lower_scalar_call(value, &value.ty))
                        {
                            self.ops.push(Op::SetLocal { local, src });
                            return Ok(());
                        }
                    }
                }
                if self.in_frame > 0 && is_heap_ty(&value.ty) {
                    self.record_elided_calls(value);
                    Ok(())
                } else {
                    self.lower_bind(*var, &value.ty, value)
                }
            }
            // `let (a, b) = (x, y)` — a TUPLE destructuring bind.
            IrStmtKind::BindDestructure { pattern, value } => {
                self.lower_destructure(pattern, value)
            }
            // In-place mutation of a place: `xs[i] = v` and `r.field = v` both
            // require the buffer to be UNIQUELY owned (copy-on-write) → `MakeUnique`.
            // The written value (and an index expression) are deferred — record any
            // call inside them so the caps fold is not blind to their effects.
            IrStmtKind::IndexAssign { target, index, value } => {
                self.lower_place_mutation(*target)?;
                self.record_elided_calls(index);
                self.record_elided_calls(value);
                Ok(())
            }
            IrStmtKind::FieldAssign { target, value, .. } => {
                self.lower_place_mutation(*target)?;
                self.record_elided_calls(value);
                Ok(())
            }
            // `m[k] = v` — map insertion/update, in-place on the buffer. Like
            // `IndexAssign` it requires the map to be UNIQUELY owned (copy-on-write) →
            // `MakeUnique`. The key and value are deferred — record their calls so the
            // caps fold is not blind to their effects.
            IrStmtKind::MapInsert { target, key, value } => {
                self.lower_place_mutation(*target)?;
                self.record_elided_calls(key);
                self.record_elided_calls(value);
                Ok(())
            }
            // A bare expression statement: an `if`/`match` in statement position is
            // LINEARIZED (control flow), an EFFECT call (`println(s)`) is lowered as a
            // runtime effect. Other non-call expr statements stay Unsupported (the
            // lower_effect_call guard rejects them — flight-grade totality).
            IrStmtKind::Expr { expr } => match &expr.kind {
                // A Unit `if` statement EXECUTES (only the taken arm's effects run) when
                // its cond is a scalar; otherwise it falls back to the linearization.
                IrExprKind::If { cond, then, else_ }
                    if self.try_lower_unit_if(cond, then, else_) =>
                {
                    Ok(())
                }
                // A Unit `match` over INT literal patterns EXECUTES: desugar to a nested
                // `if subject == lit then arm else …` and run it via try_lower_unit_if
                // (only the matched arm's effects run). Non-literal patterns / guards / a
                // non-scalar subject fall back to the linearization below.
                IrExprKind::Match { subject, arms } => {
                    if let Some(if_expr) = self.desugar_match_to_if(subject, arms, &Ty::Unit) {
                        if let IrExprKind::If { cond, then, else_ } = &if_expr.kind {
                            if self.try_lower_unit_if(cond, then, else_) {
                                return Ok(());
                            }
                        }
                    }
                    self.lower_branch(expr)
                }
                IrExprKind::If { .. } => self.lower_branch(expr),
                IrExprKind::ForIn { var, var_tuple, iterable, body } => {
                    self.lower_for_in(*var, var_tuple, iterable, body)
                }
                IrExprKind::While { cond, body } => self.lower_while(cond, body),
                // A BLOCK expression statement (`{ stmts; e }` for its effect): lower
                // its statements (locals ride to the enclosing scope), then its tail —
                // a Unit effect call, a nested branch, or a deferred value whose calls
                // we capture (its value is discarded in statement position).
                IrExprKind::Block { stmts, expr: tail } => {
                    for s in stmts {
                        self.lower_stmt(s)?;
                    }
                    if let Some(t) = tail {
                        match &t.kind {
                            IrExprKind::Call { .. } if matches!(t.ty, Ty::Unit) => {
                                self.lower_effect_call(t)?
                            }
                            // A Block-TAIL `if` (the TCO loop body is `{ if … }`, so the base-check
                            // arrives HERE, not via the bare-If statement arm): EXECUTE it via
                            // try_lower_unit_if (real branch — only the taken arm runs) so a loop
                            // base-check actually conditionally sets `rk`. Only if that declines do
                            // we consider linearization — and inside a scalar loop linearizing both
                            // arms runs the loop ONCE (the heap-`let`-in-body silent miscompile), so
                            // wall it there. Outside a loop, linearize as before.
                            IrExprKind::If { cond, then, else_ } => {
                                if !self.try_lower_unit_if(cond, then, else_) {
                                    self.lower_branch(t)?;
                                }
                            }
                            IrExprKind::Match { subject, arms } => {
                                let mut done = false;
                                if let Some(if_expr) =
                                    self.desugar_match_to_if(subject, arms, &Ty::Unit)
                                {
                                    if let IrExprKind::If { cond, then, else_ } = &if_expr.kind {
                                        done = self.try_lower_unit_if(cond, then, else_);
                                    }
                                }
                                if !done {
                                    self.lower_branch(t)?;
                                }
                            }
                            _ => self.record_elided_calls(t),
                        }
                    }
                    Ok(())
                }
                // `break` / `continue` — a Unit-typed, value-less, label-less early exit
                // (Almide has no `break x`, no labels, no `return`). It adds NO ownership
                // op: the cert models the loop running to completion, with the
                // per-iteration frame's Drops intact. This is leak-safe ONLY when the
                // frame holds no heap handle a real early exit could skip — the loop
                // lowerers enforce that with a post-lowering frame check (a heap-frame
                // loop with break/continue is WALLED, because the v0 wasm backend frees
                // AFTER the break branch target and would leak).
                IrExprKind::Break | IrExprKind::Continue => Ok(()),
                _ => self.lower_effect_call(expr),
            },
            // A source comment carries no ownership — skip it (it is not a
            // "silent drop": Comment is a no-op by definition, not an unhandled op).
            IrStmtKind::Comment { .. } => Ok(()),
            // `guard cond else { body }` — a CONDITIONAL early exit. The guard adds NO
            // ownership: the model takes the always-CONTINUE path (success), which is
            // self-consistent and memory-safe; the failure path's early exit and the
            // `else` body's effects are DEFERRED, like every Opaque (the guard's job is
            // functional, not a safety property). Capture the caps of any call in the
            // condition or the else body so a printing/effectful guard taints honestly.
            IrStmtKind::Guard { cond, else_ } => {
                self.record_elided_calls(cond);
                self.record_elided_calls(else_);
                Ok(())
            }
            other => Err(LowerError::Unsupported(format!(
                "statement {} not in the value-semantics subset",
                stmt_kind_name(other)
            ))),
        }
    }

    /// In-place mutation of a place (`xs[i] = v` / `r.field = v`): the write must
    /// land on a UNIQUELY-owned buffer, so emit `Op::MakeUnique` (copy-on-write if
    /// the buffer is shared). The written value is copied (value semantics; its
    /// content is deferred, and any call in it is caps-tainted by the elided-call
    /// gate, not silently dropped). A borrowed-param target is walled — mutating
    /// the caller's data needs the move-mode calling convention.
    pub(crate) fn lower_place_mutation(&mut self, target: VarId) -> Result<(), LowerError> {
        let v = self.value_for(target)?;
        if self.param_values.contains(&v) {
            return Err(LowerError::Unsupported(
                "in-place mutation of a borrowed param not in this brick".into(),
            ));
        }
        self.ops.push(Op::MakeUnique { v });
        Ok(())
    }

    pub(crate) fn value_for(&self, var: VarId) -> Result<ValueId, LowerError> {
        self.value_of
            .get(&var)
            .copied()
            .ok_or_else(|| LowerError::Unsupported(format!("use of unbound var {var:?}")))
    }

    /// Resolve a value-position variable reference, admitting a reference to a
    /// module-level `let` GLOBAL. A function-local var is in `value_of`. A miss is a
    /// global IFF it is in the DECLARED global set (`self.globals`) — the frontend
    /// guarantees every non-global reference is bound by a preceding local form, so a
    /// miss that is NOT a declared global is a genuine lowering gap and stays WALLED.
    ///
    /// A confirmed global is bound ONCE (cached in `value_of`, so repeated references
    /// reuse the one handle) as a fresh EXTERNAL value: a scalar global is a Copy
    /// `Const`; a heap global is a fresh owned `Alloc{Opaque}` dropped at scope end —
    /// we model an owned COPY rather than an alias of the module's object, which is
    /// memory-safe by construction (alloc once / drop once, the real global untouched)
    /// and its content deferred like every `Opaque`. Referencing a global does NOT
    /// re-run its initializer, so this adds no call/cap obligation.
    pub(crate) fn value_or_global(&mut self, var: VarId) -> Result<ValueId, LowerError> {
        if let Some(&v) = self.value_of.get(&var) {
            return Ok(v);
        }
        let ty = self
            .globals
            .get(&var)
            .cloned()
            .ok_or_else(|| LowerError::Unsupported(format!("use of unbound var {var:?}")))?;
        let dst = self.fresh_value();
        if is_heap_ty(&ty) {
            // A HEAP module-level global modeled as a fresh owned `Alloc{Opaque}` is an
            // EMPTY heap value — any read of the global (`println(g)`, returning it,
            // passing it on) observes empty bytes = a SILENT MISCOMPILE. Reject explicitly
            // until a global's real initializer can be faithfully reconstructed here.
            // (A SCALAR global is still a real `Const` below.)
            return Err(LowerError::Unsupported(format!(
                "reference to a heap module-level global {var:?} cannot be faithfully \
                 materialized in this brick (would observe an empty deferred heap value)"
            )));
        }
        self.ops.push(Op::Const { dst });
        self.value_of.insert(var, dst);
        Ok(dst)
    }

    /// The correct release op for a heap value at scope/frame end, by its tracking set (the SINGLE
    /// source of truth for drop-op selection — used by `emit_scope_end_drops`, `drop_arm_locals`, and
    /// the variant-match subject drop). Order matters: the recursive value-drops are checked BEFORE
    /// the flat `DropListStr`, since a `value.as_array` Result / a `List[Value]` is ALSO a
    /// `heap_elem_list`, but a flat per-slot `rc_dec` there would leak the nested element Values.
    /// The NAMED record type of `ty` iff it needs the recursive `$__drop_<R>` (some field is a
    /// `Map`/`Value`/record/`List[heap]` — [`record_field_needs_recursive_drop`]). A record VALUE of
    /// such a type is registered in `variant_drop_handles` so `drop_op_for` routes it to the recursive
    /// `Op::DropVariant` instead of the flat `DropListStr` (which would leak its nested heap fields).
    pub(crate) fn record_drop_type_name(&self, ty: &Ty) -> Option<String> {
        use almide_lang::types::constructor::TypeConstructorId;
        let name = match ty {
            Ty::Named(n, _) => n.as_str().to_string(),
            Ty::Applied(TypeConstructorId::UserDefined(n), _) => n.clone(),
            _ => return None,
        };
        let (_, tys) = self.aggregate_field_tys(ty)?;
        tys.iter()
            .any(record_field_needs_recursive_drop)
            .then_some(name)
    }

    pub(crate) fn drop_op_for(&self, v: ValueId) -> Op {
        if let Some(ty) = self.variant_drop_handles.get(&v) {
            Op::DropVariant { v, ty: ty.clone() }
        } else if self.value_result_lists.contains(&v) {
            Op::DropResultListValue { v }
        } else if self.value_result_results.contains(&v) {
            Op::DropResultValue { v }
        } else if self.value_elem_lists.contains(&v) {
            Op::DropListValue { v }
        } else if self.str_value_elem_lists.contains(&v) {
            Op::DropListStrValue { v }
        } else if self.str_str_elem_lists.contains(&v) {
            Op::DropListStrStr { v }
        } else if self.list_list_str_lists.contains(&v) {
            // `List[List[String]]` — checked BEFORE heap_elem_lists (it also matches
            // is_heap_elem_list_ty): the nested loop frees each inner row's cell Strings, which a
            // flat DropListStr would leak.
            Op::DropListListStr { v }
        } else if self.heap_elem_lists.contains(&v) || self.record_masks.contains_key(&v) {
            Op::DropListStr { v }
        } else if self.value_handles.contains(&v) {
            Op::DropValue { v }
        } else {
            Op::Drop { v }
        }
    }

    pub(crate) fn emit_scope_end_drops(&mut self) {
        // Reverse binding order (LIFO scope teardown). A `List[String]` value is released by a
        // RECURSIVE `DropListStr` (frees its owned element Strings); every other heap value by
        // a flat `Drop`.
        let drops: Vec<Op> =
            self.live_heap_handles.iter().rev().map(|v| self.drop_op_for(*v)).collect();
        self.ops.extend(drops);
    }
}

mod binds;
mod layout;
mod tail;
mod control;
mod calls;


/// Does a statement list contain a `break`/`continue` that targets THIS loop — i.e.
/// not nested inside another loop (which captures its own)? Used to wall a loop body
/// whose early-exit path would skip the per-iteration frame's drops (a leak).
pub(crate) fn body_breaks_or_continues(stmts: &[IrStmt]) -> bool {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct Scan {
        found: bool,
    }
    impl IrVisitor for Scan {
        fn visit_expr(&mut self, e: &IrExpr) {
            match &e.kind {
                IrExprKind::Break | IrExprKind::Continue => self.found = true,
                // A nested loop captures its OWN break/continue — do not descend.
                IrExprKind::ForIn { .. } | IrExprKind::While { .. } => {}
                _ => walk_expr(self, e),
            }
        }
    }
    let mut s = Scan { found: false };
    for stmt in stmts {
        s.visit_stmt(stmt);
    }
    s.found
}

/// Does a loop body REASSIGN a HEAP variable (`acc = acc + "x"`, `xs = xs + [e]`) in a
/// position the THIS-loop model-one-iteration fallback would reach (not nested inside an
/// inner loop, which manages its own)? Such a reassignment is the loop ACCUMULATOR: the
/// fallback DEFERS it (it emits no rebind, `value_of[acc]` stays pinned to the pre-loop
/// handle) — memory-safe but the accumulation is DROPPED, so the loop prints the initial
/// value (e.g. `var acc="S"; while i<3 { acc=acc+"x" }` → v0 `Sxxx`, the fallback `S`).
/// The executable `try_lower_scalar_while`/`_for_*` paths already decline a heap reassign
/// and roll back, so a body reaching the fallback with one cannot be faithfully run — the
/// caller WALLs it instead of silently eliding the accumulation.
pub(crate) fn body_reassigns_heap(stmts: &[IrStmt]) -> bool {
    use almide_ir::visit::{walk_expr, walk_stmt, IrVisitor};
    struct Scan {
        found: bool,
    }
    impl IrVisitor for Scan {
        fn visit_stmt(&mut self, stmt: &IrStmt) {
            if self.found {
                return;
            }
            if let IrStmtKind::Assign { value, .. } = &stmt.kind {
                if is_heap_ty(&value.ty) {
                    self.found = true;
                    return;
                }
            }
            walk_stmt(self, stmt);
        }
        fn visit_expr(&mut self, e: &IrExpr) {
            if self.found {
                return;
            }
            match &e.kind {
                // A nested loop captures its OWN accumulator — do not descend.
                IrExprKind::ForIn { .. } | IrExprKind::While { .. } => {}
                _ => walk_expr(self, e),
            }
        }
    }
    let mut s = Scan { found: false };
    for stmt in stmts {
        s.visit_stmt(stmt);
    }
    s.found
}

/// Find the type a variable is USED at in a body (its first reference's `ty`) — for
/// a `for-in` loop variable, this is its element type (the `ForIn` node carries no
/// explicit element type). `None` if the variable is unused (then its heap-ness does
/// not matter — nothing references it to manage).
pub(crate) fn find_var_ty(stmts: &[IrStmt], var: VarId) -> Option<Ty> {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct Find {
        var: VarId,
        ty: Option<Ty>,
    }
    impl IrVisitor for Find {
        fn visit_expr(&mut self, e: &IrExpr) {
            if self.ty.is_some() {
                return;
            }
            if let IrExprKind::Var { id } = &e.kind {
                if *id == self.var {
                    self.ty = Some(e.ty.clone());
                    return;
                }
            }
            walk_expr(self, e);
        }
    }
    let mut f = Find { var, ty: None };
    for stmt in stmts {
        if f.ty.is_some() {
            break;
        }
        f.visit_stmt(stmt);
    }
    f.ty
}

/// Extract a concrete initializer from a fresh-heap bind value. A `List[Int]`
/// literal yields [`Init::IntList`]; everything else is [`Init::Opaque`] (the
/// computation is carried by a later brick).
/// Does the stdlib `module.func` call return a real MATERIALIZED 0-or-1-element-list
/// Option (a self-host Option fn whose impl returns through tail-materialized `Some`/
/// `None`)? Its result may be tracked in `materialized_options` so a `match` over it
/// EXECUTES. The SINGLE SOURCE for both the bound-var path (binds.rs) and the direct-
/// subject path (control.rs) — keep them in sync to avoid tracking a non-materialized
/// call (which would misread as `None`). Add a name only when its self-host impl lands.
pub fn is_self_host_option_module_fn(module: &str, func: &str) -> bool {
    match module {
        "list" => {
            matches!(func, "get" | "first" | "last" | "index_of" | "binary_search" | "max" | "min" | "find" | "find_index" | "reduce" | "get_str" | "first_str" | "last_str")
        }
        "string" => matches!(func, "index_of" | "last_index_of" | "codepoint" | "first" | "last" | "get" | "strip_prefix" | "strip_suffix"),
        "bytes" => matches!(func, "get" | "index_of"),
        // result.to_option builds a materialized Option[Int] from a Result's len-tag (Ok → Some,
        // Err → None); option.map rebuilds a materialized Option (Some(f(x)) / None) — a `match`
        // over either result EXECUTES.
        "result" => matches!(func, "to_option" | "to_err_option"),
        "option" => matches!(func, "map" | "filter" | "flat_map" | "or_else" | "flatten" | "zip" | "collect"),
        // map.get(m, k) builds a materialized Option[Int] (Some(value) when the key is found via
        // the paired-slot scan, None otherwise) — a `match` over it EXECUTES.
        "map" => matches!(func, "get"),
        // int.to_{int,uint}N_checked builds a materialized Option[Int] (Some(n) when n fits the
        // N-bit range, None otherwise) — a `match` over it EXECUTES.
        "int" => matches!(
            func,
            "to_int8_checked"
                | "to_int16_checked"
                | "to_int32_checked"
                | "to_uint8_checked"
                | "to_uint16_checked"
                | "to_uint32_checked"
                | "to_uint64_checked"
                | "to_float32_checked"
        ),
        // float.to_{int,uint}N_checked builds a materialized Option[IntN] (Some(to_T(n)) when n is
        // an exact integer in range, None otherwise) — a `match` over it EXECUTES. Same scalar shape
        // as the int variants (IntN is i64-repr); to_int64/to_uint64/to_float32 are not yet hosted.
        "float" => matches!(
            func,
            "to_int8_checked"
                | "to_int16_checked"
                | "to_int32_checked"
                | "to_uint8_checked"
                | "to_uint16_checked"
                | "to_uint32_checked"
                | "to_int64_checked"
                | "to_uint64_checked"
                | "to_float32_checked"
        ),
        // json.as_int/as_float/as_bool build a materialized Option (Some(scalar) / None) by reading
        // the shared Value tag (@4) — a `match`/`??` over the result EXECUTES. as_int/as_float WIDEN
        // across Int/Float exactly like v0. json.as_string is the heap-payload case: Some(a deep copy
        // of the Str payload @12) / None — the repr-poly Option[String] materialization (a 0-or-1-
        // element DynListStr, same path as list.get_str); as_array (List[Value]) is a refinement.
        "json" => matches!(func, "as_int" | "as_float" | "as_bool" | "as_string"),
        _ => false,
    }
}

/// A `Sym`-interning shorthand for the recursive Display builders below.
fn sym(s: &str) -> almide_lang::intern::Sym {
    almide_lang::intern::sym(s)
}

/// A `LitStr` IR leaf (a static text fragment of a Display expansion — `"Point { "`,
/// `", "`, `" }"`, `"("`, `")"`). No call, the no-op leaf of the `ConcatStr` fold.
fn lit_str(s: &str) -> IrExpr {
    IrExpr { kind: IrExprKind::LitStr { value: s.to_string() }, ty: Ty::String, span: None, def_id: None }
}

/// Left-nest `parts` into a `ConcatStr` fold seeded by `""` — the SAME shape
/// [`desugar_string_interp`] builds, reused for a record/tuple body so the whole
/// expansion is one uniform `ConcatStr` tree (K parts ⇒ K `__str_concat` folds).
fn concat_all(parts: Vec<IrExpr>) -> IrExpr {
    let mut acc = lit_str("");
    for p in parts {
        acc = IrExpr {
            kind: IrExprKind::BinOp {
                op: almide_ir::BinOp::ConcatStr,
                left: Box::new(acc),
                right: Box::new(p),
            },
            ty: Ty::String,
            span: None,
            def_id: None,
        };
    }
    acc
}

/// Wrap `value` in `module.func(value)` (a single `Call { Module }` node), the Display
/// leaf for a scalar/list/string field — `int.to_string(r.x)`, `string.quote(r.name)`,
/// `list.to_string(r.items)`, `float.to_string_compound(r.v)`.
fn to_string_call(module: &str, func: &str, value: IrExpr) -> IrExpr {
    IrExpr {
        kind: IrExprKind::Call {
            target: CallTarget::Module { module: sym(module), func: sym(func), def_id: None },
            args: vec![value],
            type_args: Vec::new(),
        },
        ty: Ty::String,
        span: None,
        def_id: None,
    }
}

/// The DECLARATION-ordered fields of an aggregate `ty`, for the recursive Display
/// expansion: `(opt_type_name, Vec<(opt_field_name, field_ty)>)`. A `Ty::Named(name, args)`
/// resolves its fields via the layout `registry` (substituting generics) and carries the
/// type NAME (records print `Point { … }`); a structural `Ty::Record`/`Ty::Tuple` carries
/// no name. Returns `None` for a non-aggregate or unregistered type (the Display then
/// declines, the interp walls). MIRRORS `LowerCtx::aggregate_field_tys` exactly so the
/// desugar and the lowering agree on field count, order, and types.
fn resolve_aggregate(
    ty: &Ty,
    registry: &RecordLayouts,
) -> Option<(Option<String>, bool, Vec<(Option<String>, Ty)>)> {
    // `(type_name, is_tuple, [(field_name, field_ty)])`.
    match ty {
        Ty::Tuple(elems) => {
            Some((None, true, elems.iter().map(|t| (None, t.clone())).collect()))
        }
        Ty::Record { fields } => Some((
            None,
            false,
            fields.iter().map(|(n, t)| (Some(n.as_str().to_string()), t.clone())).collect(),
        )),
        Ty::Named(name, args) => {
            // Only registry-declared records resolve here; a `Ty::Named` that names no
            // record layout (an enum / alias / unknown) returns `None` and walls.
            let (generics, decl_fields) = registry.get(name.as_str())?;
            let mut subst: HashMap<almide_lang::intern::Sym, Ty> = HashMap::new();
            for (g, a) in generics.iter().zip(args.iter()) {
                subst.insert(*g, a.clone());
            }
            let fields = decl_fields
                .iter()
                .map(|(n, t)| (Some(n.as_str().to_string()), calls::subst_type_var(t, &subst)))
                .collect();
            Some((Some(name.as_str().to_string()), false, fields))
        }
        _ => None,
    }
}

/// Build the Display IR expression for an aggregate VALUE `obj` of type `ty` (a record or
/// tuple) — the recursive heart of `${record}` / `${tuple}`. Expands to a `ConcatStr` tree:
///   record: `"Name { " ++ "f0: " ++ fmt(obj.f0) ++ ", " ++ "f1: " ++ fmt(obj.f1) ++ " }"`
///   tuple:  `"(" ++ fmt(obj.0) ++ ", " ++ fmt(obj.1) ++ ")"`
/// where `fmt(field)` is [`display_value`] over the field-access node (`Member`/`TupleIndex`).
/// Returns `None` (the whole interp walls — NEVER wrong bytes) if `ty` is not a resolvable
/// aggregate or ANY field's type has no Display leaf. The `Member`/`TupleIndex` nodes lower
/// through the EXISTING value-model field access (scalar slot load / heap-field borrow), so
/// no new lowering machinery is needed — only this IR shape.
fn display_aggregate(obj: &IrExpr, ty: &Ty, registry: &RecordLayouts) -> Option<IrExpr> {
    let (type_name, is_tuple, fields) = resolve_aggregate(ty, registry)?;
    let mut parts: Vec<IrExpr> = Vec::new();
    // Opening: `Name { ` for a record, `(` for a tuple. A structural (un-named) record has
    // no v0 Display form (v0 only Displays a NAMED record), so wall it.
    if is_tuple {
        parts.push(lit_str("("));
    } else {
        let name = type_name?;
        parts.push(lit_str(&format!("{name} {{ ")));
    }
    for (idx, (fname, fty)) in fields.iter().enumerate() {
        if idx > 0 {
            parts.push(lit_str(", "));
        }
        if let Some(fname) = fname {
            parts.push(lit_str(&format!("{fname}: ")));
        }
        // The field-access node: `obj.fname` (Member) or `obj.idx` (TupleIndex), typed `fty`.
        let access = if is_tuple {
            IrExpr {
                kind: IrExprKind::TupleIndex { object: Box::new(obj.clone()), index: idx },
                ty: fty.clone(),
                span: None,
                def_id: None,
            }
        } else {
            IrExpr {
                kind: IrExprKind::Member {
                    object: Box::new(obj.clone()),
                    field: sym(fname.as_deref().unwrap_or("")),
                },
                ty: fty.clone(),
                span: None,
                def_id: None,
            }
        };
        parts.push(display_value(&access, registry)?);
    }
    parts.push(lit_str(if is_tuple { ")" } else { " }" }));
    Some(concat_all(parts))
}

/// Build the Display IR (a String-producing expression) for a VALUE `expr` of ANY type —
/// the per-field formatter the record/tuple Display calls recursively. Byte-matches v0's
/// AlmideRepr for the value's type:
///   - `Int`     → `int.to_string(expr)`              (signed decimal)
///   - `Bool`    → `bool.to_string(expr)`             (`true`/`false`)
///   - `Float`   → `float.to_string_compound(expr)`   (compound form — DROPS the `.0`)
///   - `String`  → `string.quote(expr)`               (double-quoted + escaped)
///   - `List[T]` → `list.to_string*(expr)`            (element-type-keyed, as the top-level interp)
///   - Record/Tuple → [`display_aggregate`] recursively (no call — an inline `ConcatStr`)
/// Returns `None` (so the enclosing Display declines and the interp walls) for any type
/// with no Display leaf — a nested `List[List[_]]` element, a Map/Set/Option field, an
/// unresolved var. NEVER emits a wrong-byte fallback.
fn display_value(expr: &IrExpr, registry: &RecordLayouts) -> Option<IrExpr> {
    // A nested record/tuple expands INLINE (recursive `ConcatStr`, no `to_string` call).
    if matches!(expr.ty, Ty::Record { .. } | Ty::Tuple(_) | Ty::Named(..))
        && resolve_aggregate(&expr.ty, registry).is_some()
    {
        return display_aggregate(expr, &expr.ty, registry);
    }
    // Every other value type wraps in its single `to_string`-family call.
    let (module, func) = display_leaf_call(&expr.ty)?;
    Some(to_string_call(module, func, expr.clone()))
}

/// The SINGLE `(module, func)` Display wrapper for a NON-aggregate value type — the source both
/// [`display_value`] (the IR builder) and [`value_synthetic_names`] (the gate counter) consult, so
/// the emitted call and the counted call AGREE by construction:
///   - `Int`     → `int.to_string`            `Bool`  → `bool.to_string`
///   - `Float`   → `float.to_string_compound` (compound form — drops the `.0`)
///   - `String`  → `string.quote`             (double-quoted + escaped)
///   - `List[T]` → `list.to_string*`          (element-type-keyed; unsupported → unlinked, walls)
///   - Map/Set/Option/Result → the unlinked `<module>.to_string` (walls — never wrong bytes)
/// `None` for a type with NO Display leaf at all (a bare unresolved var) — the Display declines.
fn display_leaf_call(ty: &Ty) -> Option<(&'static str, &'static str)> {
    match ty {
        Ty::Int => Some(("int", "to_string")),
        Ty::Bool => Some(("bool", "to_string")),
        Ty::Float => Some(("float", "to_string_compound")),
        Ty::String => Some(("string", "quote")),
        // List / Map / Set / Option / Result route through the element-type-keyed
        // `interp_to_string_call` (List → a self-host variant; the rest → an unlinked
        // `<module>.to_string` that walls). A Tuple/Record/variant/unresolved returns the
        // unlinked `compound.to_string` there, so the enclosing aggregate also walls.
        _ => interp_to_string_call(ty),
    }
}

/// The `(module, func)` pair whose call renders a value of type `ty` to its Almide-Display form
/// for the string-interpolation desugar. The MIR `CallFn` name is `"<module>.<func>"`, so this is
/// the SINGLE source both the leaf builder ([`interp_part_leaf`]) and the gate name-lister
/// ([`interp_synthetic_call_names`]) consult — they agree on the exact call name BY CONSTRUCTION,
/// keeping `mir == ir` for the corpus caps gate. The module MUST be pure (`purity::is_pure`).
///
/// For a `List[T]` the func is ELEMENT-TYPE-KEYED so each variant is a monomorphic self-host impl
/// that reads the slot at the right width/repr and formats the element in v0's COMPOUND form (NB:
/// the compound-Float element drops the trailing `.0` — see `list_to_string_f.almd`):
///   - `List[Int]`            → `list.to_string`     (i64 slot, decimal digits)
///   - `List[Float]`          → `list.to_string_f`   (f64-bits slot, compound float, drops `.0`)
///   - `List[Bool]`           → `list.to_string_b`   (i64 0/1 slot, `true`/`false`)
///   - `List[String]`         → `list.to_string_s`   (i32-handle slot, quoted+escaped)
/// Any OTHER element type (NESTED `List[List[_]]`, Map/Set/Option/Record element, an unresolved var)
/// returns `None`: the whole interp declines the desugar and stays cleanly walled — NEVER a wrong
/// byte. Nested lists are walled deliberately: v1 does not yet materialize a `List[List[_]]` literal
/// (the inner handles are never stored), so a nested element formatter would read garbage slots;
/// walling is the sound choice. (Map/Set/Option/Result top-level `to_string` stay unlinked = walled.)
fn interp_to_string_call(ty: &Ty) -> Option<(&'static str, &'static str)> {
    use almide_lang::types::constructor::TypeConstructorId;
    Some(match ty {
        Ty::Int => ("int", "to_string"),
        Ty::Bool => ("bool", "to_string"),
        // Scalar `${f}` interp uses v0's Display format, which DROPS the `.0` for integer-valued
        // floats (`3.0`->`3`, `100.0`->`100`) — exactly the compound formatter
        // `float.to_string_compound`, NOT `float.to_string` (which keeps `.0` for an EXPLICIT
        // `float.to_string(x)` call). Same drop-.0 Display a Float record/list field already uses.
        Ty::Float => ("float", "to_string_compound"),
        Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => match &args[0] {
            Ty::Int => ("list", "to_string"),
            Ty::Float => ("list", "to_string_f"),
            Ty::Bool => ("list", "to_string_b"),
            Ty::String => ("list", "to_string_s"),
            // An unsupported element type (NESTED `List[List[_]]`, `List[Map]`, …) routes to an
            // UNLINKED variant name so the interp DESUGARS to a real `list.to_string_x` CallFn that
            // the render wall then REJECTS — the function walls cleanly. Returning `None` here would
            // instead leave the interp Opaque and the `println` would emit NOTHING (a silent empty
            // miscompile); routing-to-unlinked preserves the all-or-nothing wall. NEVER registered.
            _ => ("list", "to_string_x"),
        },
        // Map/Set/Option/Result top-level `to_string` are not self-hosted → the synthesized call is
        // UNLINKED, so the using function walls at render (never a wrong byte). Keep routing them so
        // the gate accounts the same call name the lowering emits (mir == ir), exactly as before.
        Ty::Applied(TypeConstructorId::Map, _) => ("map", "to_string"),
        Ty::Applied(TypeConstructorId::Set, _) => ("set", "to_string"),
        Ty::Applied(TypeConstructorId::Option, _) => ("option", "to_string"),
        Ty::Applied(TypeConstructorId::Result, _) => ("result", "to_string"),
        // Tuple / Record / variant / any other type has no self-hosted `to_string` yet.
        // Route to an UNLINKED `to_string` so the interp DESUGARS to a real CallFn that the
        // render wall REJECTS (the function walls cleanly) — NEVER leave it Opaque, which
        // makes `println("${tuple}")` emit NOTHING (a silent empty miscompile). This is the
        // nested-`List` lesson (above) applied UNIFORMLY: no interp Expr part may fall to
        // Opaque. NEVER registered, so every such function walls all-or-nothing.
        _ => ("compound", "to_string"),
    })
}

/// Does a record/tuple/list/scalar VALUE of type `ty` materialize with REAL slots the recursive
/// Display can read — the STATIC (IR-type-only) predicate the gate and the lowering BOTH consult so
/// they agree on expand-vs-wrap BY CONSTRUCTION (no runtime-`materialized_aggregates` divergence).
/// Matches exactly what the construction path materializes:
///   - Int/Bool/Float/String          → yes (scalar / single heap leaf)
///   - List[scalar]                    → yes (scalar-element block); List[heap] → NO (not materialized)
///   - a registered record/tuple whose every field is itself `field_displayable` → yes (the
///     nested-aggregate construction admits a SCALAR-ONLY nested block; a heap-IN-nested field would
///     leak under the single-level mask, so it is NO)
///   - Map/Set/Option/Result/variant/unresolved → NO
fn field_displayable(ty: &Ty, registry: &RecordLayouts) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    match ty {
        Ty::Int | Ty::Bool | Ty::Float | Ty::String => true,
        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => !is_heap_ty(&a[0]),
        Ty::Record { .. } | Ty::Tuple(_) | Ty::Named(..) => match resolve_aggregate(ty, registry) {
            // A NESTED aggregate must be SCALAR-ONLY (the construction's `lower_owned_heap_field`
            // admits only a scalar-only nested block — a heap-in-nested field would leak).
            Some((_, _, fields)) => fields.iter().all(|(_, t)| !is_heap_ty(t)),
            None => false,
        },
        _ => false,
    }
}

/// Is a record/tuple interpolation PART statically EXPAND-foldable — i.e. the lowering will
/// materialize it and read its real slots? True iff the part expr is a `Var` (a materialized
/// aggregate binding; a literal/call result is not a tracked block) AND every field of the
/// (resolvable) aggregate is `field_displayable`. The gate and the lowering both gate on THIS, so
/// the synthetic-call count the gate credits equals the calls the lowering emits — for both the
/// EXPAND path (recursive tree) and the WALL path (one `compound.to_string`).
pub(crate) fn aggregate_part_expandable(expr: &IrExpr, registry: &RecordLayouts) -> bool {
    if !matches!(expr.kind, IrExprKind::Var { .. }) {
        return false; // a literal `${P{..}}` / a call `${f()}` is not a tracked materialized block
    }
    match resolve_aggregate(&expr.ty, registry) {
        Some((_, _, fields)) => fields.iter().all(|(_, t)| field_displayable(t, registry)),
        None => false,
    }
}

/// Build the String-producing LEAF for ONE interpolation part, by type:
///   - a literal text part → a `LitStr` (no call),
///   - a String-typed part → the expr itself (identity, no call),
///   - an EXPAND-foldable RECORD/TUPLE part (a materialized Var with displayable fields) → the
///     recursive layout-driven Display ([`display_aggregate`]), an INLINE `ConcatStr` tree of
///     per-field formatters; a NON-expandable record/tuple part → ONE unlinked `compound.to_string`
///     wrapper (the function walls at render — never a wrong byte),
///   - any other part with a pure `module.to_string` → `module.to_string(expr)`.
/// Returns `None` for a part whose type has no admitted Display at all (an unresolved type) — the
/// caller then declines the whole desugar.
fn interp_part_leaf(p: &IrStringPart, registry: &RecordLayouts) -> Option<IrExpr> {
    match p {
        IrStringPart::Lit { value } => Some(lit_str(value)),
        IrStringPart::Expr { expr } if matches!(expr.ty, Ty::String) => Some(expr.clone()),
        // A record/tuple part: EXPAND if the lowering will materialize it; else wrap in the
        // unlinked `compound.to_string` so the function walls (the SAME decision the gate makes).
        IrStringPart::Expr { expr }
            if matches!(expr.ty, Ty::Record { .. } | Ty::Tuple(_) | Ty::Named(..))
                && resolve_aggregate(&expr.ty, registry).is_some() =>
        {
            if aggregate_part_expandable(expr, registry) {
                display_aggregate(expr, &expr.ty, registry)
            } else {
                Some(to_string_call("compound", "to_string", expr.clone()))
            }
        }
        IrStringPart::Expr { expr } => {
            let (module, func) = interp_to_string_call(&expr.ty)?;
            Some(to_string_call(module, func, expr.clone()))
        }
    }
}

/// Desugar a STRING INTERPOLATION `"…${e}…"` into a left-nested `ConcatStr` fold,
/// seeded by an empty `""` literal: `(((("" ++ p0) ++ p1) … ) ++ p_{K-1})`. Each
/// part is wrapped in its type's `to_string` ([`interp_part_leaf`]) — a Lit/String
/// part is a no-call leaf, every other part a single `module.to_string` call.
/// Concatenating with the leading `""` is byte-identical to v0's `emit_string_interp`
/// (`"" ++ bytes == bytes`), so the folded String matches v0 in EVERY position.
///
/// This is the SINGLE source the lowering ([`LowerCtx::try_lower_string_interp`])
/// AND the corpus caps gate (`count_ir_calls` in classify_corpus) BOTH consult: the
/// gate counts the call NODES of the very tree the lowering emits, so the synthetic
/// MIR `Op::CallFn`s are 1:1 backed by IR call nodes — `mir_calls == ir_calls` for an
/// in-profile interp BY CONSTRUCTION (no `mir > ir` over-count, no spurious caps
/// taint). Soundness rests on one invariant: when this returns `Some(tree)`, every
/// leaf lowers to exactly one `CallFn` (a pure `module.to_string`, admitted by
/// `purity::is_pure`) or a no-call passthrough — so `try_lower_concat_str` never
/// rolls back. Returns `None` (the interp stays the deferred Opaque, credited 0 by
/// the gate) iff a part has no admitted `to_string` module — a memory-safe defer.
///
/// THE WALL DOES THE HEAVY LIFTING: a part whose `to_string` is UNLINKED (Float /
/// compound — registered in `PURE_MODULES` but not in the self-host runtime) still
/// desugars to a real `CallFn`, so the enclosing function emits an unlinked call and
/// the render wall (`try_render_wasm_program`) REJECTS it as `Unsupported`. Such a
/// function is OUT of profile, so it can never contribute a `count != lower`
/// mismatch — the only IN-profile interps are the fully-linkable ones (Lit/String/
/// Int/Bool), where `count == lower` is trivially exact.
pub fn desugar_string_interp(parts: &[IrStringPart], registry: &RecordLayouts) -> Option<IrExpr> {
    let mut acc = lit_str("");
    for p in parts {
        let leaf = interp_part_leaf(p, registry)?;
        acc = IrExpr {
            kind: IrExprKind::BinOp {
                op: almide_ir::BinOp::ConcatStr,
                left: Box::new(acc),
                right: Box::new(leaf),
            },
            ty: Ty::String,
            span: None,
            def_id: None,
        };
    }
    Some(acc)
}

/// The SYNTHETIC call names the recursive Display ([`display_value`]) introduces for a
/// single value of type `ty` — the `<module>.to_string`-family wrappers, recursively. A
/// scalar/string/float/list value contributes ONE name; a record/tuple value contributes
/// none itself but recurses via [`aggregate_synthetic_names`] into its fields. This DOES
/// NOT count the value's OWN inner calls (it counts the WRAPPERS the desugar adds, not the
/// operand) — keeping the `count_ir_calls` operand-descent free of double counting.
fn value_synthetic_names(ty: &Ty, registry: &RecordLayouts, out: &mut Vec<String>) {
    match ty {
        // A nested record/tuple expands INLINE (recursive `__str_concat` + field formatters).
        Ty::Record { .. } | Ty::Tuple(_) | Ty::Named(..) if resolve_aggregate(ty, registry).is_some() => {
            aggregate_synthetic_names(ty, registry, out);
        }
        // Every OTHER value type routes to exactly ONE `to_string`-family call — the SAME single
        // wrapper [`display_value`] / [`interp_part_leaf`] emit (Int → int.to_string, Float →
        // float.to_string_compound, String → string.quote, List → list.to_string*, Map/Set/Option/
        // Result → the unlinked `<module>.to_string` that walls). Keyed off `display_leaf_call` so
        // the gate's count is BY CONSTRUCTION the lowering's emitted call set.
        _ => {
            if let Some((m, f)) = display_leaf_call(ty) {
                out.push(format!("{m}.{f}"));
            }
        }
    }
}

/// The SYNTHETIC call names the recursive Display ([`display_aggregate`]) introduces for an
/// aggregate of type `ty`: one `__str_concat` per `ConcatStr` fold the expansion builds
/// (= the number of `concat_all` parts at this level) plus the field formatters recursively.
/// MIRRORS `display_aggregate`'s structure EXACTLY so the gate credits precisely the
/// synthetic CallFns the lowering emits (count == lower for the aggregate, by construction).
fn aggregate_synthetic_names(ty: &Ty, registry: &RecordLayouts, out: &mut Vec<String>) {
    // A non-resolvable aggregate (structural record, unregistered) yields no Display tree —
    // the part declines and the whole interp credits 0 (matched by `interp_synthetic_call_names`).
    let Some((type_name, is_tuple, fields)) = resolve_aggregate(ty, registry) else {
        return;
    };
    if !is_tuple && type_name.is_none() {
        return; // structural record has no Display → walls, credits 0
    }
    // `concat_all` parts at this level: opening + (per field: a leading ", " for idx>0,
    // a "field: " label for a record, the field formatter) + closing.
    //   record: 1 (open) + Σ_i [ (i>0 → 1) + 1 (label) + 1 (formatter) ] + 1 (close)
    //   tuple:  1 (open) + Σ_i [ (i>0 → 1) +            1 (formatter) ] + 1 (close)
    let mut concat_parts = 2; // open + close
    for (idx, _) in fields.iter().enumerate() {
        if idx > 0 {
            concat_parts += 1; // ", "
        }
        if !is_tuple {
            concat_parts += 1; // "field: "
        }
        concat_parts += 1; // the field formatter expression
    }
    for _ in 0..concat_parts {
        out.push("__str_concat".to_string());
    }
    for (_, fty) in &fields {
        value_synthetic_names(fty, registry, out);
    }
}

/// Count the synthetic `CallFn`s [`desugar_string_interp`] yields for `parts` — the
/// `ConcatStr` and `module.to_string`-family call NODES of the desugared tree. The corpus
/// gate adds exactly this to its IR call count for each interp (it counts the same tree),
/// so the MIR calls the lowering emits are 1:1 backed. `None` (a part with no admitted
/// Display) ⇒ 0 (the interp stays Opaque, lowering emits no synthetic call).
pub fn interp_str_synthetic_call_count(parts: &[IrStringPart], registry: &RecordLayouts) -> usize {
    interp_synthetic_call_names(parts, registry).len()
}

/// The SYNTHETIC call names [`desugar_string_interp`] introduces for `parts`: one
/// `__str_concat` per TOP-LEVEL fold step (= `parts.len()`: K parts over the `""` seed ⇒ K
/// concats) and, per non-passthrough part, the Display wrappers it adds — a scalar part one
/// `<module>.to_string`, a RECORD/TUPLE part the full recursive `__str_concat` + field-
/// formatter set ([`aggregate_synthetic_names`]). It DOES NOT include the operands' OWN
/// inner calls (a `${g(x)}` callee) — those live in the original part exprs and are reached
/// separately by `count_ir_calls`'s descent, so no double count. Empty (a `None` desugar —
/// a part with no admitted Display) ⇒ the interp stays Opaque, crediting none.
pub fn interp_synthetic_call_names(parts: &[IrStringPart], registry: &RecordLayouts) -> Vec<String> {
    // A part with no admitted Display ⇒ the whole interp is non-desugarable (the lowering
    // returns `None` and defers to Opaque), so it credits zero synthetic calls.
    if desugar_string_interp(parts, registry).is_none() {
        return Vec::new();
    }
    let mut names = Vec::with_capacity(parts.len() * 2);
    // The TOP-LEVEL fold: K parts over the `""` seed ⇒ K `__str_concat` (the interp's own
    // outer concatenation — a record/tuple part is ONE top-level part here, its INNER
    // `__str_concat`s are added by `value_synthetic_names` below).
    for _ in 0..parts.len() {
        names.push("__str_concat".to_string());
    }
    for p in parts {
        if let IrStringPart::Expr { expr } = p {
            if matches!(expr.ty, Ty::String) {
                continue; // a String part is a no-call passthrough
            }
            // A TOP-LEVEL record/tuple part mirrors `interp_part_leaf`'s expand-vs-wrap: an
            // EXPAND-foldable part (a materialized Var with displayable fields) credits the full
            // recursive tree; a NON-expandable one credits ONE `compound.to_string` (the wall).
            if matches!(expr.ty, Ty::Record { .. } | Ty::Tuple(_) | Ty::Named(..))
                && resolve_aggregate(&expr.ty, registry).is_some()
            {
                if aggregate_part_expandable(expr, registry) {
                    aggregate_synthetic_names(&expr.ty, registry, &mut names);
                } else {
                    names.push("compound.to_string".to_string());
                }
            } else {
                value_synthetic_names(&expr.ty, registry, &mut names);
            }
        }
    }
    names
}

/// Is a WHOLE interpolation DESUGARABLE (every part has an admitted Display)? When true, the
/// lowering folds it to a `ConcatStr` chain; when false, it stays the deferred Opaque.
/// (Desugarable does NOT imply LINKABLE — a Float part desugars but float.to_string is
/// unlinked, so the function walls at render. Use the registry to split proven-vs-walled;
/// this predicate only answers "does the lowering fold it".)
pub fn interp_str_desugarable(parts: &[IrStringPart], registry: &RecordLayouts) -> bool {
    desugar_string_interp(parts, registry).is_some()
}

/// Does `module.func` return a real MATERIALIZED `Result[Int, String]` (the DynListStr len-as-tag
/// layout)? Its result may be tracked in `materialized_results` so an `Ok`/`Err` `match` over it
/// EXECUTES. NARROW to fns actually self-hosted — any other Result is a deferred `Opaque` (len 0,
/// would misread as `Ok`). `int.parse` is the canonical for string.to_int/to_integer/parse_int.
/// The CallFn name for a stdlib `module.func` call, routing the REPR-POLYMORPHIC list combinators
/// to their `_str` variant when the RESULT is a `List[heap]` (e.g. `list.map` over a `List[String]`
/// → `list.map_str`, a DynListStr-result impl). The element repr (i64 vs i32 handle) demands a
/// separate variant; the variant reads/writes via the heap-aware prim ops. Scalar-result lists keep
/// the plain name. `module.func` is unchanged for everything else.
pub(crate) fn list_heap_call_name(module: &str, func: &str, arg_tys: &[Ty], result_ty: &Ty) -> String {
    use almide_lang::types::constructor::TypeConstructorId;
    // `fold` threads an ACCUMULATOR (= the result type). A HEAP accumulator (e.g. a String built up
    // across the fold) needs the closure-result + accumulator to be an i32 handle, not the i64 the
    // scalar-accumulator fold variants hardcode — emitting an i32 there is invalid wasm. No heap-
    // accumulator fold variant is self-hosted yet, so route it to an UNREGISTERED name: render walls
    // it cleanly (a controlled reject) rather than emitting a repr-mismatched module. (Soundness-
    // preserving: a wall is never a miscompile.)
    if func == "fold" && matches!(module, "list" | "map" | "set") && is_heap_ty(result_ty) {
        return format!("{module}.fold_hacc");
    }
    if module == "list" {
        // List[Float] ordering uses IEEE-754 totalOrder (f64::total_cmp), NOT a signed-int slot
        // compare. Float is SCALAR (is_heap_ty false), so the heap routes below never fire for it —
        // route sort/min/max explicitly on the element being Ty::Float (C-055). sort_by keys on the
        // CLOSURE (arg 1) RETURN type being Float — the element list may be any type (e.g. List[R]).
        if matches!(func, "sort" | "min" | "max") {
            if let Some(Ty::Applied(TypeConstructorId::List, a)) = arg_tys.first() {
                if a.len() == 1 && a[0] == Ty::Float {
                    return format!("list.{func}_float");
                }
            }
        }
        if func == "sort_by" {
            if let Some(Ty::Fn { ret, .. }) = arg_tys.get(1) {
                if **ret == Ty::Float {
                    return "list.sort_by_float".to_string();
                }
            }
        }
        // `list.map` is the one combinator whose SOURCE and RESULT element reprs may DIFFER (the
        // closure transforms the type). A heap RESULT over a SCALAR source (`float.to_string` over a
        // List[Float], `int.to_string` over a List[Int]) must read the source slot as a raw i64
        // scalar (load64), not as a String handle (load_str) — that is `map_s2h`; a heap result over
        // a heap source is the all-String `map_str`.
        if func == "map" {
            if let Ty::Applied(TypeConstructorId::List, rargs) = result_ty {
                if rargs.len() == 1 && is_heap_ty(&rargs[0]) {
                    let src_heap = matches!(
                        arg_tys.first(),
                        Some(Ty::Applied(TypeConstructorId::List, s)) if s.len() == 1 && is_heap_ty(&s[0])
                    );
                    return if src_heap {
                        "list.map_str".to_string()
                    } else {
                        "list.map_s2h".to_string()
                    };
                }
            }
        }
        // The element-PRESERVING List[heap]-returning combinators (source elem == result elem).
        if matches!(func, "filter" | "reverse" | "take" | "drop" | "unique" | "dedup" | "intersperse") {
            if let Ty::Applied(TypeConstructorId::List, args) = result_ty {
                // A List[List[String]] result element is itself a heap list — the `_str` deep-copy
                // (string.repeat) would read its length word as a byte count. take/drop SHARE the inner
                // lists by handle via the `_liststr` variant; the other combinators are a later brick.
                if args.len() == 1
                    && matches!(func, "take" | "drop")
                    && matches!(&args[0], Ty::Applied(TypeConstructorId::List, e)
                        if e.len() == 1 && matches!(e[0], Ty::String))
                {
                    return format!("list.{func}_liststr");
                }
                if args.len() == 1 && is_heap_ty(&args[0]) {
                    return format!("list.{func}_str");
                }
            }
        }
        // Element-RETURNING accessors / search over a List[heap] (the result is an Option[heap]):
        // get/first/last (positional) + find (predicate higher-order).
        if matches!(func, "get" | "first" | "last" | "find") {
            if let Ty::Applied(TypeConstructorId::Option, args) = result_ty {
                // A List[Value] element is a dynamic Value, NOT a String — the `_str` variant DEEP-
                // COPIES via `string.repeat` (corrupting an Object to {}). Route get/first/last to the
                // Value accessor, which SHARES the element (rc_inc, like value.get's Ok). (find's
                // closure-keyed Value form is a later brick — only the positional accessors here.)
                if args.len() == 1 && is_value_ty(&args[0]) && matches!(func, "get" | "first" | "last")
                {
                    return format!("list.{func}_value");
                }
                // A `List[List[String]]` element is itself a heap list, NOT a String — the `_str`
                // variant would DEEP-COPY it via `string.repeat`, reading the inner list's length word
                // as a byte count (garbage). Route to the handle-SHARE `_liststr` accessor (the
                // `List[String]` analogue of `_value`); the inner list is co-owned, dropped DropListStr.
                if args.len() == 1
                    && matches!(func, "get" | "first" | "last")
                    && matches!(&args[0], Ty::Applied(TypeConstructorId::List, e)
                        if e.len() == 1 && matches!(e[0], Ty::String))
                {
                    return format!("list.{func}_liststr");
                }
                if args.len() == 1 && is_heap_ty(&args[0]) {
                    return format!("list.{func}_str");
                }
            }
        }
        // get_or returns the ELEMENT directly (not an Option). Over a List[heap] it must return
        // an i32 handle (a deep copy), so it is keyed on the heap RESULT being the element type.
        if func == "get_or" && is_heap_ty(result_ty) {
            return "list.get_or_str".to_string();
        }
        // SUBJECT-keyed (arg 0) over a List[heap], where the result is scalar (Bool/Int/Option[Int])
        // so it can't be keyed on the result type: search (contains/index_of) + the predicate
        // higher-order all/any/count.
        if matches!(func, "contains" | "index_of" | "all" | "any" | "count" | "fold") {
            if let Some(Ty::Applied(TypeConstructorId::List, a)) = arg_tys.first() {
                if a.len() == 1 && is_heap_ty(&a[0]) {
                    return format!("list.{func}_str");
                }
            }
        }
    }
    if module == "set" {
        // `Set[heap]`-RETURNING constructors key on the RESULT element type; `set.to_list` over a
        // `Set[heap]` returns a `List[heap]`; the predicate `set.contains` keys on its SUBJECT
        // (arg 0) element type (its result is Bool). Each routes to the heap-element `_str` variant.
        let result_is_heap_container = matches!(
            result_ty,
            Ty::Applied(TypeConstructorId::Set | TypeConstructorId::List, a)
                if a.len() == 1 && is_heap_ty(&a[0])
        );
        // RESULT-keyed: constructors / Set-returning algebra over heap elements.
        if matches!(
            func,
            "from_list" | "to_list" | "union" | "intersection" | "difference"
                | "new" | "insert" | "remove" | "symmetric_difference" | "filter"
        ) && result_is_heap_container
        {
            return format!("set.{func}_str");
        }
        // ARG-keyed: a Bool/scalar-returning fn over a `Set[heap]` subject (arg 0).
        let arg0_is_heap_set = matches!(
            arg_tys.first(),
            Some(Ty::Applied(TypeConstructorId::Set, a)) if a.len() == 1 && is_heap_ty(&a[0])
        );
        if matches!(func, "contains" | "is_subset" | "is_disjoint" | "all" | "any" | "fold")
            && arg0_is_heap_set
        {
            return format!("set.{func}_str");
        }
    }
    if module == "map" {
        // A map's REPR is set by its (key, value) heap-ness, read from whichever Map type the call
        // exposes: arg 0 (the SUBJECT of set/get/fold/filter/…) takes priority, else the RESULT
        // (map.new() has no args). The two repr families:
        //   key heap, value heap  → `_str` (map_str: interleaved all-String entries)
        //   key heap, value scalar → `_skv` (map_skv: String keys + i64 values, serves
        //                            Map[String,Int] AND Map[String,Float] — the value is one i64)
        //   key scalar             → the plain map_core (Map[Int,Int]); a scalar-key heap-value map
        //                            has no variant yet, so it falls through (walled by repr).
        // The element-returning forms (get → Option[V], keys/values → List[elem]) read the same
        // key/value reprs off the subject map (arg 0).
        let map_kv = |ty: &Ty| match ty {
            Ty::Applied(TypeConstructorId::Map, a) if a.len() == 2 => {
                Some((is_heap_ty(&a[0]), is_heap_ty(&a[1])))
            }
            _ => None,
        };
        let kv = arg_tys
            .first()
            .and_then(&map_kv)
            .or_else(|| map_kv(result_ty));
        if let Some((key_heap, val_heap)) = kv {
            // Each repr family routes ONLY the funcs its self-hosted variant file actually defines
            // (an unlisted func keeps the plain name — never a dangling `_str`/`_skv` reference).
            let variant = match (key_heap, val_heap) {
                (true, true) => matches!(
                    func,
                    "new" | "set" | "remove" | "merge" | "update" | "filter" | "get" | "keys"
                        | "values" | "len" | "is_empty" | "contains" | "all" | "any" | "count" | "fold"
                        | "entries"
                )
                .then_some("_str"),
                (true, false) => matches!(
                    func,
                    "new" | "set" | "remove" | "filter" | "get" | "get_or" | "keys" | "values"
                        | "len" | "is_empty" | "contains" | "all" | "any" | "count" | "fold"
                )
                .then_some("_skv"),
                _ => None,
            };
            if let Some(suffix) = variant {
                return format!("map.{func}{suffix}");
            }
        }
    }
    format!("{module}.{func}")
}

pub(crate) fn is_self_host_result_module_fn(module: &str, func: &str) -> bool {
    matches!(
        (module, func),
        ("int", "parse")
            // `float.parse` is the same intrinsic-Result shape as `int.parse` (Result[Float, String],
            // a materialized scalar Result read len-as-tag); a `match` over it EXECUTES the same way.
            | ("float", "parse")
            | ("int", "from_hex")
            | ("option", "to_result")
            | ("result", "map")
            | ("result", "flat_map")
            | ("result", "map_err")
            | ("result", "filter")
            | ("result", "or_else")
            | ("result", "flatten")
            | ("error", "context")
            // value.as_int/as_bool/as_float build a materialized Result[T, String] (Ok(payload)
            // on a tag match, else Err("expected T")) — a `match` over the result EXECUTES.
            | ("value", "as_int")
            | ("value", "as_bool")
            | ("value", "as_float")
    )
}

/// Does `module.func` return a materialized HEAP-Ok `Result[String, String]` (the cap-as-tag
/// DynListStr layout, both Ok and Err owning a String)? Its result is tracked in
/// `materialized_results_str` so an `Ok`/`Err` `match` over it EXECUTES reading cap@8.
pub fn is_self_host_result_str_module_fn(module: &str, func: &str) -> bool {
    matches!(
        (module, func),
        ("value", "as_string") | ("result", "zip") | ("value", "as_array") | ("value", "get")
    )
}

/// Is `ty` a `value.as_array`-style Result whose Ok arm is a `List[Value]` (a heap-Ok Result with a
/// LIST-of-Value payload)? Such a Result reuses the cap@16 str-result MATCH machinery, but its DROP
/// must free the list RECURSIVELY (`Op::DropResultListValue`/`value_result_lists`), not flat
/// (`DropListStr` would leak the list's element Values). The DISTINGUISHER from `value.as_string`'s
/// `Result[String, String]` is the Ok-arm being a `List`, so the tracking is TYPE-driven (sound
/// wherever only the `ValueId` + its `ty` are known — seed_variant_param, the match subject).
pub fn is_result_listval_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty, Ty::Applied(TypeConstructorId::Result, a)
        if a.len() == 2 && matches!(&a[0], Ty::Applied(TypeConstructorId::List, _)))
}

/// Is `ty` a `Result[String, String]` (the value.as_string shape — both arms a flat String)? The
/// PRECISE str-str distinguisher (vs the broader `is_heap_ok_result`, which also matches a tuple-Ok
/// `result.zip`), so the `??` routes only a genuine String-payload Result to `result.str_unwrap_or`.
pub fn is_result_str_str_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty, Ty::Applied(TypeConstructorId::Result, a)
        if a.len() == 2 && matches!(&a[0], Ty::String) && matches!(&a[1], Ty::String))
}

/// Is `ty` an `Option[Value]` (the `list.get(rows, i)` shape — a dynamic Value Some-payload)? Its
/// `??` routes to `option.value_unwrap_or` (the prim-based unwrap, since the value-match Some-arm's
/// scalar_bind rejects a heap Value payload).
pub fn is_option_value_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty, Ty::Applied(TypeConstructorId::Option, a)
        if a.len() == 1 && is_value_ty(&a[0]))
}

/// Is `ty` an `Option[List[String]]` (the `list.get_liststr(rows, i)` shape — a nested-heap-list
/// Some-payload)? Its `??` routes to `option.liststr_unwrap_or`, the List[String] analogue of
/// `option.value_unwrap_or`.
pub fn is_option_liststr_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty, Ty::Applied(TypeConstructorId::Option, a)
        if a.len() == 1 && matches!(&a[0], Ty::Applied(TypeConstructorId::List, e)
            if e.len() == 1 && matches!(e[0], Ty::String)))
}

pub(crate) fn alloc_init(value: &IrExpr) -> Init {
    if let IrExprKind::LitStr { value } = &value.kind {
        return Init::Str(value.clone());
    }
    // A list OR tuple of scalar literals materializes its slots: an Int element stores its value, a
    // Float element stores its f64 BITS (the i64-uniform Float repr — read back via load64 +
    // ffrombits). A `(3, 7)` tuple is physically a 2-slot block [3@12, 7@20], exactly a List[Int]
    // literal — so a scalar-literal-field tuple shares the IntList materialization. A mixed/
    // non-literal list or tuple stays Opaque.
    if let IrExprKind::List { elements } | IrExprKind::Tuple { elements } = &value.kind {
        let ints: Option<Vec<i64>> = elements
            .iter()
            .map(|e| match &e.kind {
                IrExprKind::LitInt { value } => Some(*value),
                IrExprKind::LitFloat { value } => Some(value.to_bits() as i64),
                // A Bool literal occupies its 8-byte slot as 0/1 (the i64-uniform Bool repr), so a
                // `[true, false]` literal materializes exactly like an IntList of [1, 0] — read back
                // via load64 as 0/1. (`${bool_list}` → list.to_string_b reads these slots.)
                IrExprKind::LitBool { value } => Some(*value as i64),
                _ => None,
            })
            .collect();
        if let Some(ints) = ints {
            return Init::IntList(ints);
        }
    }
    Init::Opaque
}

pub(crate) fn stmt_kind_name(k: &IrStmtKind) -> &'static str {
    match k {
        IrStmtKind::Bind { .. } => "Bind",
        IrStmtKind::BindDestructure { .. } => "BindDestructure",
        IrStmtKind::Assign { .. } => "Assign",
        IrStmtKind::IndexAssign { .. } => "IndexAssign",
        IrStmtKind::MapInsert { .. } => "MapInsert",
        IrStmtKind::FieldAssign { .. } => "FieldAssign",
        IrStmtKind::Guard { .. } => "Guard",
        IrStmtKind::Expr { .. } => "Expr",
        IrStmtKind::Comment { .. } => "Comment",
        IrStmtKind::RcInc { .. } => "RcInc",
        IrStmtKind::RcDec { .. } => "RcDec",
        IrStmtKind::ListSwap { .. } => "ListSwap",
        IrStmtKind::ListReverse { .. } => "ListReverse",
        IrStmtKind::ListRotateLeft { .. } => "ListRotateLeft",
        IrStmtKind::ListCopySlice { .. } => "ListCopySlice",
    }
}

/// The CONTAINER expression of a field/element/tuple/map extraction, if `expr`
/// is one — the source whose object the extracted value aliases (the
/// container-grain field access, see [`LowerCtx::lower_heap_extraction`]).
pub(crate) fn extraction_container(expr: &IrExpr) -> Option<&IrExpr> {
    match &expr.kind {
        IrExprKind::Member { object, .. }
        | IrExprKind::IndexAccess { object, .. }
        | IrExprKind::TupleIndex { object, .. }
        | IrExprKind::MapAccess { object, .. } => Some(object),
        _ => None,
    }
}

/// True if any argument is a FUNCTION-typed value (a closure / lambda / fn-ref).
/// A stdlib call with such an argument invokes USER code, so its effective
/// capabilities are its-own ∪ the closure's — unmodelled in the pure-only Module
/// slice — and a captured-heap closure carries ownership this brick does not
/// track. Such calls are walled. The TYPE test catches every form (a lambda
/// literal, a fn-ref, OR a variable of function type) under the AllTypesConcrete
/// precondition; the kind test is a belt-and-suspenders for any arg whose type
/// was not concretized.
pub(crate) fn is_higher_order(args: &[IrExpr]) -> bool {
    args.iter().any(|a| {
        matches!(a.ty, Ty::Fn { .. })
            || matches!(
                a.kind,
                IrExprKind::Lambda { .. }
                    | IrExprKind::ClosureCreate { .. }
                    | IrExprKind::FnRef { .. }
            )
    })
}

/// TAIL-DUPLICATION desugar for a `let s = <heap-result if/match>; <rest>` in a NON-tail,
/// let-bound position — the shape `lower_bind` walls (a merged-dst heap value has no sound
/// scope-end drop in the flat certificate).
///
/// This is a PURE IR→IR rewrite applied to a function BODY *before* both lowering and the
/// caps `count_ir_calls` gate ("desugar-before-both"): they see the IDENTICAL node tree, so the
/// duplicated continuation's calls are counted exactly as the lowering emits them and the
/// `mir == ir` 1:1 invariant holds BY CONSTRUCTION — no special-casing in either side, no risk
/// of an IR-structure count formula leaking a false `mir > ir` (or masking an elision).
///
/// Scan the body block's `(stmts, tail)` for the FIRST `Bind { s, ty, value }` whose `value` is a
/// heap-result `if`/`match` and `ty` is heap. Found at index `i`, push the continuation `<rest>`
/// (`stmts[i+1..] ++ tail`) into each arm:
///   `… ; let s = if c then A else B; <rest>`  →  `… ; if c then { let s = A; <rest> } else { let s = B; <rest> }`
/// (and the `match` analog — each literal-pattern arm, via `desugar_match_to_if`, binds its value
/// then runs `<rest>`). The rewritten branch becomes the block's TAIL, so the EXISTING `lower_tail`
/// machinery executes it by result kind (Unit/scalar/heap `if`) — each arm independently binds `s`
/// (cert `i`), runs `<rest>` and drops `s` + the continuation's locals at the arm frame end (cert
/// `d`): the per-arm `i…d` balance the proven checker already accepts. Only ONE arm runs at runtime,
/// so duplicating `<rest>` is semantically identical to v0. NO certificate / Coq change.
///
/// GATE (bounded + sound — WALL what cannot be duplicated cleanly; the rewritten tree still routes
/// through the per-position `if` machinery, which itself rolls back to an explicit wall on an
/// unfaithful arm/cond):
///  - The continuation `<rest>` must NOT itself carry another unresolved heap let-bound `if`/`match`
///    (duplicating a duplicating continuation risks exponential blow-up) — left to the wall.
///  - A `match` not reducible to a literal-pattern else-if chain (`desugar_match_to_if`) — left to
///    the wall.
///
/// Returns `Some(rewritten_body)` when the desugar applies, `None` (the body is unchanged) otherwise.
/// The max `VarId` used anywhere in `body` (0 if none) — so a fresh synthetic var can be
/// allocated as `max + 1` without a frontend var-table round-trip.
pub(crate) fn max_var_id(body: &IrExpr) -> u32 {
    use almide_ir::visit::IrVisitor;
    use almide_ir::IrPattern;
    // A pattern binds variables (`some(ch)`, `ok(x)`, `(a, b)`) that are NOT `IrExprKind::Var` /
    // `IrStmtKind::Bind` nodes, so the visitor's expr/stmt hooks miss them. A fresh synthetic var
    // (`rk`/`idx` = max+1/+2) MUST clear them too — else it COLLIDES with a pattern bind and the
    // renderer reuses one local for two types (an i32 element handle AND an i64 flag = invalid wasm).
    fn pat_max(p: &IrPattern, acc: &mut u32) {
        match p {
            IrPattern::Bind { var, .. } => *acc = (*acc).max(var.0),
            IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => {
                pat_max(inner, acc)
            }
            IrPattern::Tuple { elements } | IrPattern::List { elements }
            | IrPattern::Constructor { args: elements, .. } => {
                for e in elements {
                    pat_max(e, acc);
                }
            }
            IrPattern::RecordPattern { fields, .. } => {
                for f in fields {
                    if let Some(fp) = &f.pattern {
                        pat_max(fp, acc);
                    }
                }
            }
            IrPattern::Wildcard | IrPattern::None | IrPattern::Literal { .. } => {}
        }
    }
    struct M(u32);
    impl IrVisitor for M {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Var { id } = &e.kind {
                self.0 = self.0.max(id.0);
            }
            if let IrExprKind::Match { arms, .. } = &e.kind {
                for arm in arms {
                    pat_max(&arm.pattern, &mut self.0);
                }
            }
            almide_ir::visit::walk_expr(self, e);
        }
        fn visit_stmt(&mut self, s: &IrStmt) {
            if let IrStmtKind::Bind { var, .. } = &s.kind {
                self.0 = self.0.max(var.0);
            }
            almide_ir::visit::walk_stmt(self, s);
        }
    }
    let mut m = M(0);
    m.visit_expr(body);
    m.0
}

/// Is `e` a HEAP-result `if`/`match` (the form `lower_bind` walls / the tail-dup recovers)?
fn is_heap_branch(e: &IrExpr) -> bool {
    is_heap_ty(&e.ty) && matches!(e.kind, IrExprKind::If { .. } | IrExprKind::Match { .. })
}

// ─────────────────── TCO: tail-self-recursion → scalar loop ───────────────────
// A tail-self-recursive `f(p…) = <if/block tree whose leaves are self-calls f(p'…) or base
// exprs>` is rewritten to the GATE-VERIFIABLE cert-clean shape: a SCALAR-only top-test loop
// (the loop body only reassigns the scalar loop-carried params + a `result_kind` flag) followed
// by a POST-LOOP dispatch that builds the heap result from `result_kind` + the final scalars.
// No new MIR primitive, no cert change — the existing scalar-while + heap-result-if lowering
// verify it. Replaces the self-rec-guard wall for the reconstructible-base subset (scan_quote,
// find_colon_at, …). See docs/roadmap/active/v1-tco-self-recursion.md.

fn tco_ir(kind: IrExprKind, ty: Ty) -> IrExpr {
    IrExpr { kind, ty, span: None, def_id: None }
}

/// An empty value of `ty` for the TCO result accumulator's INITIAL binding — a placeholder the first
/// base case overwrites (its scope-end-style drop on reassignment must be a no-op-equivalent, so it is
/// a genuine empty heap block, not a deferred Opaque). `List → []`, `String → ""`. Other heap results
/// (Value, Result) have no clean empty literal, so the accumulator path declines (`None`) and the
/// caller keeps the post-loop dispatch (or walls, when a base references a loop-body-local).
fn tco_empty_for(ty: &Ty) -> Option<IrExpr> {
    use almide_lang::types::constructor::TypeConstructorId;
    match ty {
        Ty::String => Some(tco_ir(IrExprKind::LitStr { value: String::new() }, Ty::String)),
        Ty::Applied(TypeConstructorId::List, _) => {
            Some(tco_ir(IrExprKind::List { elements: vec![] }, ty.clone()))
        }
        _ => None,
    }
}

fn tco_contains_self(e: &IrExpr, fn_name: &str) -> bool {
    use almide_ir::visit::IrVisitor;
    struct S<'a>(&'a str, bool);
    impl IrVisitor for S<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Call { target: CallTarget::Named { name }, .. } = &e.kind {
                if name.as_str() == self.0 {
                    self.1 = true;
                }
            }
            almide_ir::visit::walk_expr(self, e);
        }
    }
    let mut s = S(fn_name, false);
    s.visit_expr(e);
    s.1
}

/// Does expression `e` read variable `v` anywhere (a `Var { id: v }` node)?
fn expr_reads_var(e: &IrExpr, v: VarId) -> bool {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct R {
        v: VarId,
        found: bool,
    }
    impl IrVisitor for R {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Var { id } = &e.kind {
                if *id == self.v {
                    self.found = true;
                }
            }
            walk_expr(self, e);
        }
    }
    let mut r = R { v, found: false };
    r.visit_expr(e);
    r.found
}

/// Order the changed heap-accumulator param indices `idxs` so that an accumulator whose new value
/// READS another changed heap accumulator is assigned BEFORE that one — the reader must observe the
/// OLD value (a `rows = rows + [cur]` self-call alongside `cur = []` must run rows FIRST, while `cur`
/// still holds the old row). Edge `a → b` (emit a before b) iff `args[idxs[a]]` reads
/// `params[idxs[b]].var`. Kahn's topological sort; `None` if the read-graph is CYCLIC (e.g.
/// `a = a + b; b = b + a` — no order sees both olds; that residual needs owned-temp staging).
fn order_heap_accs_by_read_dep(
    idxs: &[usize],
    args: &[IrExpr],
    params: &[almide_ir::IrParam],
) -> Option<Vec<usize>> {
    let n = idxs.len();
    let mut indeg = vec![0usize; n];
    let mut edges: Vec<Vec<usize>> = vec![Vec::new(); n];
    for a in 0..n {
        for b in 0..n {
            if a != b && expr_reads_var(&args[idxs[a]], params[idxs[b]].var) {
                edges[a].push(b); // idxs[a] reads idxs[b] ⇒ a before b
                indeg[b] += 1;
            }
        }
    }
    let mut queue: Vec<usize> = (0..n).filter(|&i| indeg[i] == 0).collect();
    let mut order: Vec<usize> = Vec::new();
    while let Some(a) = queue.pop() {
        order.push(idxs[a]);
        for &b in &edges[a] {
            indeg[b] -= 1;
            if indeg[b] == 0 {
                queue.push(b);
            }
        }
    }
    if order.len() == n {
        Some(order)
    } else {
        None // a cycle — no read-before-reset order exists
    }
}

/// Walk tail-position leaves: a self-call pushes its args to `calls`; any other tail leaf is a
/// base (pushed to `bases`). `None` if a self-call sits in a NON-tail position (not TCO-able).
fn tco_collect<'a>(
    body: &'a IrExpr,
    fn_name: &str,
    calls: &mut Vec<&'a [IrExpr]>,
    bases: &mut Vec<&'a IrExpr>,
) -> Option<()> {
    match &body.kind {
        IrExprKind::If { then, else_, .. } => {
            tco_collect(then, fn_name, calls, bases)?;
            tco_collect(else_, fn_name, calls, bases)
        }
        IrExprKind::Block { expr: Some(tail), .. } => tco_collect(tail, fn_name, calls, bases),
        IrExprKind::Call { target: CallTarget::Named { name }, args, .. }
            if name.as_str() == fn_name =>
        {
            calls.push(args);
            Some(())
        }
        _ => {
            if tco_contains_self(body, fn_name) {
                return None; // a self-call buried in a non-tail leaf — not TCO-able here
            }
            bases.push(body);
            Some(())
        }
    }
}

/// Rewrite tail leaves: a self-call → a Block assigning each CARRIED param to its new arg; a base
/// → `result_kind = <its 1-based kind>` (kinds assigned in `tco_collect`'s left-to-right order).
fn tco_rewrite(
    body: &IrExpr,
    fn_name: &str,
    params: &[almide_ir::IrParam],
    carried: &[bool],
    rk: VarId,
    next_kind: &mut i64,
    idx: Option<VarId>,
    next_var: &mut u32,
    result: Option<VarId>,
) -> IrExpr {
    match &body.kind {
        IrExprKind::If { cond, then, else_ } => tco_ir(
            IrExprKind::If {
                cond: cond.clone(),
                then: Box::new(tco_rewrite(then, fn_name, params, carried, rk, next_kind, idx, next_var, result)),
                else_: Box::new(tco_rewrite(else_, fn_name, params, carried, rk, next_kind, idx, next_var, result)),
            },
            Ty::Unit,
        ),
        IrExprKind::Block { stmts, expr: Some(tail) } => tco_ir(
            IrExprKind::Block {
                stmts: stmts.clone(),
                expr: Some(Box::new(tco_rewrite(tail, fn_name, params, carried, rk, next_kind, idx, next_var, result))),
            },
            Ty::Unit,
        ),
        IrExprKind::Call { target: CallTarget::Named { name }, args, .. }
            if name.as_str() == fn_name =>
        {
            // SIMULTANEOUS UPDATE (the loop carries all params at once): a self-call arg may read ANOTHER
            // carried param (`acc + [string.slice(s, pos, …)]` reads `pos`; `start = pos + 1` reads `pos`),
            // so a plain sequential assign would see already-updated values — an off-by-one. Stage every
            // carried SCALAR's new value in a fresh temp (reading OLD params), THEN do the HEAP
            // accumulator assigns (which read the still-OLD scalar locals), THEN commit the scalar temps.
            // An IDENTITY arg (`acc` passed unchanged) is skipped (the stable local already holds it).
            let changed = |i: usize| {
                carried[i] && !matches!(&args[i].kind, IrExprKind::Var { id } if *id == params[i].var)
            };
            let mut stmts: Vec<IrStmt> = Vec::new();
            let mut finals: Vec<(VarId, VarId, Ty)> = Vec::new();
            // Phase 1: stage carried SCALAR args in temps (read OLD params).
            for i in 0..params.len() {
                if changed(i) && !is_heap_ty(&params[i].ty) {
                    let t = VarId(*next_var);
                    *next_var += 1;
                    stmts.push(IrStmt {
                        kind: IrStmtKind::Bind {
                            var: t,
                            mutability: almide_ir::Mutability::Let,
                            ty: params[i].ty.clone(),
                            value: args[i].clone(),
                        },
                        span: None,
                    });
                    finals.push((params[i].var, t, params[i].ty.clone()));
                }
            }
            // Phase 2: HEAP append/reset accumulator(s) — `acc = acc + [x]` reads the still-OLD scalar
            // locals. Emit in READ-DEPENDENCY order so a heap accumulator that reads ANOTHER heap
            // accumulator (`rows = rows + [cur]` alongside `cur = []`) is assigned BEFORE that one is
            // updated — the reader must observe the old value. try_tco_rewrite already walled the
            // cyclic case, so the order always exists (the unwrap_or is a defensive param-order
            // fallback).
            let heap_changed: Vec<usize> = (0..params.len())
                .filter(|&i| changed(i) && is_heap_ty(&params[i].ty))
                .collect();
            let heap_order = order_heap_accs_by_read_dep(&heap_changed, args, params)
                .unwrap_or(heap_changed);
            for i in heap_order {
                stmts.push(IrStmt {
                    kind: IrStmtKind::Assign { var: params[i].var, value: args[i].clone() },
                    span: None,
                });
            }
            // Phase 3: commit the staged scalar updates.
            for (p, t, ty) in finals {
                stmts.push(IrStmt {
                    kind: IrStmtKind::Assign {
                        var: p,
                        value: tco_ir(IrExprKind::Var { id: t }, ty),
                    },
                    span: None,
                });
            }
            // LIST-ITERATOR self-call: the consumed list param is INVARIANT (carried[ci]=false), so
            // advancing it `list.drop(cs,1)` becomes `idx = idx + 1` — the cert-clean iterator bump.
            if let Some(iv) = idx {
                stmts.push(IrStmt {
                    kind: IrStmtKind::Assign {
                        var: iv,
                        value: tco_ir(
                            IrExprKind::BinOp {
                                op: almide_ir::BinOp::AddInt,
                                left: Box::new(tco_ir(IrExprKind::Var { id: iv }, Ty::Int)),
                                right: Box::new(tco_ir(IrExprKind::LitInt { value: 1 }, Ty::Int)),
                            },
                            Ty::Int,
                        ),
                    },
                    span: None,
                });
            }
            tco_ir(IrExprKind::Block { stmts, expr: None }, Ty::Unit)
        }
        _ => {
            // A BASE case (a non-self tail). Set `rk` to a non-zero kind so the `while rk == 0` loop
            // exits. The base VALUE is delivered one of two ways:
            //   • result accumulator (`result = Some`): assign `<base>` to the carried result var HERE,
            //     IN the loop — where the base's inputs (carried params AND loop-body-local bindings
            //     like a destructured `let (field, _) = pf(…)`) are all live. The post-loop trivially
            //     returns the accumulator. This is the only correct place when the base reads a
            //     loop-body-local (those are dead in the post-loop dispatch — the parse_rows_rec bug).
            //   • post-loop dispatch (`result = None`): just record WHICH base via `rk = k`; the value
            //     is recomputed after the loop. Sound ONLY when the base closes over carried params.
            let k = *next_kind;
            *next_kind += 1;
            let mut stmts: Vec<IrStmt> = Vec::new();
            if let Some(rv) = result {
                stmts.push(IrStmt {
                    kind: IrStmtKind::Assign { var: rv, value: body.clone() },
                    span: None,
                });
            }
            stmts.push(IrStmt {
                kind: IrStmtKind::Assign {
                    var: rk,
                    value: tco_ir(IrExprKind::LitInt { value: k }, Ty::Int),
                },
                span: None,
            });
            tco_ir(IrExprKind::Block { stmts, expr: None }, Ty::Unit)
        }
    }
}

/// Rewrite a tail-self-recursive function body to a scalar loop + post-loop dispatch, or `None`
/// if it is outside the TCO subset (no self-call, a heap loop-carried arg, a self-call in a
/// non-tail position, or no base). The result lowers through the ordinary statements+tail path.
pub(crate) fn try_tco_rewrite(
    fn_name: &str,
    params: &[almide_ir::IrParam],
    body: &IrExpr,
) -> Option<IrExpr> {
    // Only a HEAP-result self-rec function (the kind the self-rec guard walls — it returns an
    // Option/Result/Value/String the deep recursion would build then trap on). A SCALAR self-rec
    // already lowers (shallow-correct), so leave it untouched (no regression risk).
    if !is_heap_ty(&body.ty) {
        return None;
    }
    let n = params.len();
    let max_v = max_var_id(body).max(params.iter().map(|p| p.var.0).max().unwrap_or(0));
    let rk = VarId(max_v + 1);
    // LIST-ITERATOR rewrite (the heap-loop-carried escape): a HEAP carried param `cs` consumed in
    // EVERY self-call ONLY as `list.drop(cs, 1)`, with the body matching on `list.first(cs)`, is a
    // forward list scan. Rewrite it to an INVARIANT borrowed `cs` + a synthetic scalar INDEX `idx`:
    // `match list.first(cs) { none => BASE, some(ch) => BODY }` → `if idx < list.len(cs) then { let
    // ch = cs[idx]; BODY } else BASE`, and each `f(list.drop(cs,1), …)` self-call bumps `idx += 1`
    // (handled in `tco_rewrite`). `cs` becomes invariant, so the loop is the cert-clean scalar form —
    // NO heap back-edge merge, NO cert change. Closes oct_rec/bin_rec. Done BEFORE `tco_collect`
    // (which bails on a `match` body), so the rewritten `if` body is what gets collected + lowered.
    let lit = try_list_iter_rewrite(fn_name, body, params, max_v + 2);
    let work_body: &IrExpr = lit.as_ref().map(|(b, _, _)| b).unwrap_or(body);
    let idx_var = lit.as_ref().map(|(_, iv, _)| *iv);

    // FIRST collection — detect the self-calls + carried params (on the pre-substitution body).
    let mut calls0: Vec<&[IrExpr]> = Vec::new();
    let mut bases0: Vec<&IrExpr> = Vec::new();
    tco_collect(work_body, fn_name, &mut calls0, &mut bases0)?;
    if calls0.is_empty() || bases0.is_empty() {
        return None;
    }
    if calls0.iter().any(|c| c.len() != n) {
        return None;
    }
    let mut carried0 = vec![false; n];
    for c in &calls0 {
        for i in 0..n {
            if !matches!(&c[i].kind, IrExprKind::Var { id } if *id == params[i].var) {
                carried0[i] = true;
            }
        }
    }
    if let Some((_, _, ci)) = &lit {
        carried0[*ci] = false;
    }
    // APPEND ACCUMULATORS (option C producer): a heap carried param whose EVERY self-call value is
    // `acc + [x]` (`BinOp::ConcatList` appending the accumulator to itself). Each becomes an OWNED
    // loop-carried SLOT — a fresh var initialized to `acc + []` (an owned copy: a `__list_concat`
    // Call heap-result, so `of[slot]=slot` and cert `i`), substituted for `acc` throughout, then
    // drop-old/alloc-new per iteration (cert `i(id)m`, accepted by the proven `check_cert_lc`). A heap
    // carried param that is NOT a self-append needs a general heap back-edge merge — still unsupported.
    // A self-call value that GROWS the accumulator from itself: `acc + [x]` (`ConcatList`) OR
    // `acc + s` (`ConcatStr`, the STRING accumulator — `parse_unquoted_field(text, pos+1, acc + c)`).
    // Both allocate a FRESH owned heap value; the TCO makes the accumulator an owned loop-carried
    // slot (drop-old/alloc-new per iter, cert `i(id)m`).
    let is_self_append = |e: &IrExpr, acc: VarId| -> bool {
        matches!(&e.kind, IrExprKind::BinOp { op: almide_ir::BinOp::ConcatList | almide_ir::BinOp::ConcatStr, left, .. }
            if matches!(&left.kind, IrExprKind::Var { id } if *id == acc))
    };
    let is_identity = |e: &IrExpr, acc: VarId| -> bool {
        matches!(&e.kind, IrExprKind::Var { id } if *id == acc)
    };
    // A RESET to a FRESH EMPTY heap value (`cur = []` / `acc = ""`): the parser-row shape resets the
    // current-row accumulator after a delimiter. Like a self-append it is a fresh owned heap value the
    // loop-carried slot takes via drop-old/alloc-new (cert `i(id)m`); the in-loop `Assign` lowering's
    // general `lower_owned_heap_field` path materializes the empty literal.
    let is_reset = |e: &IrExpr| -> bool {
        matches!(&e.kind, IrExprKind::List { elements } if elements.is_empty())
            || matches!(&e.kind, IrExprKind::LitStr { value } if value.is_empty())
    };
    let mut append_accs: Vec<usize> = Vec::new();
    for i in 0..n {
        if carried0[i] && is_heap_ty(&params[i].ty) {
            // Each self-call passes the accumulator UNCHANGED (`acc`, a pass-through branch), APPENDED
            // (`acc + [x]`), or RESET to a fresh empty (`[]`/`""`); at least one grows/resets it (else
            // not carried). A heap carry outside these needs a general back-edge merge — unsupported.
            if calls0.iter().all(|c| {
                is_identity(&c[i], params[i].var)
                    || is_self_append(&c[i], params[i].var)
                    || is_reset(&c[i])
            }) {
                append_accs.push(i);
            } else {
                return None;
            }
        }
    }
    drop(calls0);
    drop(bases0);

    // Build the (possibly substituted) working body + params + upfront slot-init binds.
    let mut slot_next = max_v + 3;
    let mut upfront: Vec<IrStmt> = Vec::new();
    let mut params_v: Vec<almide_ir::IrParam> = params.to_vec();
    let subst_body: Option<IrExpr> = if append_accs.is_empty() {
        None
    } else {
        let mut b = work_body.clone();
        for &ai in &append_accs {
            let slot = VarId(slot_next);
            slot_next += 1;
            let acc_var = params[ai].var;
            let list_ty = params[ai].ty.clone();
            // upfront: `let slot = acc + <empty>` — a fresh OWNED copy of the borrowed accumulator
            // param (the concat always allocates, so the slot never aliases it). A String
            // accumulator copies via `acc + ""` (`ConcatStr`); a list via `acc + []` (`ConcatList`).
            let (empty, concat_op) = if matches!(list_ty, Ty::String) {
                (tco_ir(IrExprKind::LitStr { value: String::new() }, Ty::String), almide_ir::BinOp::ConcatStr)
            } else {
                (tco_ir(IrExprKind::List { elements: vec![] }, list_ty.clone()), almide_ir::BinOp::ConcatList)
            };
            let copy = tco_ir(
                IrExprKind::BinOp {
                    op: concat_op,
                    left: Box::new(tco_ir(IrExprKind::Var { id: acc_var }, list_ty.clone())),
                    right: Box::new(empty),
                },
                list_ty.clone(),
            );
            upfront.push(IrStmt {
                kind: IrStmtKind::Bind {
                    var: slot,
                    mutability: almide_ir::Mutability::Var,
                    ty: list_ty.clone(),
                    value: copy,
                },
                span: None,
            });
            let slot_ref = tco_ir(IrExprKind::Var { id: slot }, list_ty);
            b = almide_ir::substitute_var_in_expr(&b, acc_var, &slot_ref);
            params_v[ai].var = slot;
        }
        Some(b)
    };
    let work_ref: &IrExpr = subst_body.as_ref().unwrap_or(work_body);
    let params2: &[almide_ir::IrParam] = &params_v;

    // SECOND collection — on the substituted body, with the slot params.
    let mut calls: Vec<&[IrExpr]> = Vec::new();
    let mut bases: Vec<&IrExpr> = Vec::new();
    tco_collect(work_ref, fn_name, &mut calls, &mut bases)?;
    if calls.is_empty() || bases.is_empty() {
        return None;
    }
    if calls.iter().any(|c| c.len() != n) {
        return None;
    }
    // A param is loop-CARRIED iff some self-call passes a value other than the param itself.
    let mut carried = vec![false; n];
    for c in &calls {
        for i in 0..n {
            if !matches!(&c[i].kind, IrExprKind::Var { id } if *id == params2[i].var) {
                carried[i] = true;
            }
        }
    }
    // The list-iterator param is now INVARIANT — its `list.drop(cs,1)` self-call arg is replaced by
    // the `idx` bump (in `tco_rewrite`), so `cs` is never reassigned in the loop.
    if let Some((_, _, ci)) = &lit {
        carried[*ci] = false;
    }
    // A carried HEAP arg is admitted ONLY as an append-accumulator SLOT (handled below by the in-loop
    // `Assign` lowering as drop-old/alloc-new); any other heap carry needs a general back-edge merge.
    let append_slots: std::collections::BTreeSet<VarId> =
        append_accs.iter().map(|&i| params2[i].var).collect();
    if (0..n)
        .any(|i| carried[i] && is_heap_ty(&params2[i].ty) && !append_slots.contains(&params2[i].var))
    {
        return None;
    }
    // SIMULTANEOUS-UPDATE SAFETY. `tco_rewrite` stages scalar updates in temps and runs the heap
    // accumulator assigns BEFORE committing them, so scalar↔scalar and heap-reads-scalar are correct.
    // A HEAP accumulator arg that reads ANOTHER carried HEAP accumulator (`rows = rows + [cur]` while
    // `cur = []`) is handled by emitting the heap assigns in READ-DEPENDENCY order (reader before the
    // accumulator it reads — `order_heap_accs_by_read_dep` in tco_rewrite), so the reader sees the OLD
    // value. WALL only the residual the topological order CANNOT serialize: a CYCLE (`a = a + b`,
    // `b = b + a` — no order sees both olds; needs owned-temp staging, not in this brick).
    {
        for c in &calls {
            let changed_heap: Vec<usize> = (0..n)
                .filter(|&i| {
                    carried[i]
                        && is_heap_ty(&params2[i].ty)
                        && !matches!(&c[i].kind, IrExprKind::Var { id } if *id == params2[i].var)
                })
                .collect();
            if order_heap_accs_by_read_dep(&changed_heap, c, params2).is_none() {
                return None; // a heap-accumulator read cycle — unsupported
            }
        }
        // PURE-VAR ALIAS HAZARD: a carried scalar whose new value is exactly ANOTHER carried param
        // (`start = pos`) cannot be staged in a copy temp — `let t = pos` ALIASES pos's local, so the
        // later `start = t` reads pos's ALREADY-updated value (off-by-one). A COMPUTED arg (`pos + 1`)
        // stages a fresh value and is fine. Wall the pure-var-aliasing form (rare; the parser loops use
        // computed indices like `pos + 1`).
        let carried_scalars: std::collections::BTreeSet<VarId> = (0..n)
            .filter(|&i| carried[i] && !is_heap_ty(&params2[i].ty))
            .map(|i| params2[i].var)
            .collect();
        for c in &calls {
            for i in 0..n {
                if carried[i] {
                    if let IrExprKind::Var { id } = &c[i].kind {
                        if *id != params2[i].var && carried_scalars.contains(id) {
                            return None;
                        }
                    }
                }
            }
        }
    }
    let base_exprs: Vec<IrExpr> = bases.iter().map(|b| (*b).clone()).collect();
    let ret_ty = body.ty.clone();

    // Does ANY base case reference a LOOP-BODY-LOCAL binding — a `let`/destructure in the loop body
    // (e.g. `let (field, np) = pf(…)`) — rather than only carried params? Such a base must be computed
    // IN the loop (the binding is dead in the post-loop dispatch — the parse_rows_rec use-after-free).
    // `free_vars(base)` excludes anything the base binds internally, so the intersection is exactly the
    // loop-body bindings the base READS from an enclosing scope.
    let loop_lets = almide_ir::free_vars::bound_vars(work_ref);
    let base_reads_loop_local = base_exprs.iter().any(|b| {
        almide_ir::free_vars::free_vars(b, &std::collections::HashSet::new())
            .iter()
            .any(|v| loop_lets.contains(v))
    });
    // When it does, carry the base value out through a RESULT ACCUMULATOR computed in the loop, and
    // the post-loop is a trivial read. Needs an empty initial value of the result type; without one
    // (Value/Result) DECLINE the TCO entirely — the function keeps its memory-safe non-TCO form (a
    // clean wall), never the dispatch's use-after-free.
    let result_var: Option<VarId> = if base_reads_loop_local {
        tco_empty_for(&ret_ty)?;
        let rv = VarId(slot_next);
        slot_next += 1;
        Some(rv)
    } else {
        None
    };

    let mut next_kind = 1i64;
    // `slot_next` is the next free VarId (after rk / list-iter idx / append slots / result) — tco_rewrite
    // draws its simultaneous-update temps from here.
    let loop_body = tco_rewrite(
        work_ref, fn_name, params2, &carried, rk, &mut next_kind, idx_var, &mut slot_next, result_var,
    );

    // `rk == k` (the loop guard uses `rk == 0`; the post-loop dispatch uses `rk == <base kind>`).
    let eq_rk = |k: i64| {
        tco_ir(
            IrExprKind::BinOp {
                op: almide_ir::BinOp::Eq,
                left: Box::new(tco_ir(IrExprKind::Var { id: rk }, Ty::Int)),
                right: Box::new(tco_ir(IrExprKind::LitInt { value: k }, Ty::Int)),
            },
            Ty::Bool,
        )
    };
    // Post-loop: the accumulator path just READS the result the loop computed; otherwise the dispatch
    // `if rk == 1 then base_1 else if … else base_N` recomputes the hit base from the carried params.
    let post = if let Some(rv) = result_var {
        tco_ir(IrExprKind::Var { id: rv }, ret_ty.clone())
    } else {
        let mut post = base_exprs.last()?.clone();
        for (idx, base) in base_exprs.iter().enumerate().rev().skip(1) {
            post = tco_ir(
                IrExprKind::If {
                    cond: Box::new(eq_rk((idx + 1) as i64)),
                    then: Box::new(base.clone()),
                    else_: Box::new(post),
                },
                ret_ty.clone(),
            );
        }
        post
    };

    // `{ [let slot = acc + [];]* [var idx = 0;] var rk = 0; while (rk == 0) { <loop_body> }; <post> }`
    // The append-accumulator slot inits (owned copies of the borrowed `acc` params) come FIRST.
    let mut inits: Vec<IrStmt> = upfront;
    if let Some(iv) = idx_var {
        inits.push(IrStmt {
            kind: IrStmtKind::Bind {
                var: iv,
                mutability: almide_ir::Mutability::Var,
                ty: Ty::Int,
                value: tco_ir(IrExprKind::LitInt { value: 0 }, Ty::Int),
            },
            span: None,
        });
    }
    // The result accumulator (when used) starts at an empty value of the result type — a placeholder
    // the first base case overwrites IN the loop; declared mutable so the in-loop base assigns it.
    if let Some(rv) = result_var {
        inits.push(IrStmt {
            kind: IrStmtKind::Bind {
                var: rv,
                mutability: almide_ir::Mutability::Var,
                ty: ret_ty.clone(),
                value: tco_empty_for(&ret_ty).expect("checked Some above"),
            },
            span: None,
        });
    }
    let init = IrStmt {
        kind: IrStmtKind::Bind {
            var: rk,
            mutability: almide_ir::Mutability::Var,
            ty: Ty::Int,
            value: tco_ir(IrExprKind::LitInt { value: 0 }, Ty::Int),
        },
        span: None,
    };
    inits.push(init);
    let while_stmt = IrStmt {
        kind: IrStmtKind::Expr {
            expr: tco_ir(
                IrExprKind::While {
                    cond: Box::new(eq_rk(0)),
                    body: vec![IrStmt { kind: IrStmtKind::Expr { expr: loop_body }, span: None }],
                },
                Ty::Unit,
            ),
        },
        span: None,
    };
    inits.push(while_stmt);
    Some(tco_ir(
        IrExprKind::Block { stmts: inits, expr: Some(Box::new(post)) },
        ret_ty,
    ))
}

/// Detect + rewrite the LIST-ITERATOR heap-loop-carried pattern (oct_rec/bin_rec): a heap carried
/// param `cs` consumed in EVERY self-call ONLY as `list.drop(Var(cs), 1)`, with the body an outer
/// `match list.first(Var(cs)) { none => BASE, some(ch) => BODY }`. Returns the rewritten body (the
/// match → `if idx < list.len(cs) then { let ch = cs[idx]; BODY } else BASE`) + the fresh `idx`
/// VarId, and FLIPS `carried[ci]` to false (cs is now invariant — the iterator is `idx`, bumped per
/// self-call in `tco_rewrite`). `None` if the pattern does not hold. Cert-clean: the result is the
/// scalar-TCO loop over `idx` + the borrowed-stable `cs`; no heap back-edge merge.
fn try_list_iter_rewrite(
    fn_name: &str,
    body: &IrExpr,
    params: &[almide_ir::IrParam],
    fresh: u32,
) -> Option<(IrExpr, VarId, usize)> {
    // The body must be `match SUBJ { none => .., some(ch) => .. }` with SUBJ = `list.first(Var(cs))`.
    let IrExprKind::Match { subject, arms } = &body.kind else { return None };
    if arms.len() != 2 {
        return None;
    }
    let (cs_var, first_ty) = match &subject.kind {
        IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
            if module.as_str() == "list" && func.as_str() == "first" && args.len() == 1 =>
        {
            match &args[0].kind {
                IrExprKind::Var { id } => (*id, subject.ty.clone()),
                _ => return None,
            }
        }
        _ => return None,
    };
    // `cs` must be a param, and EVERY self-call must pass `list.drop(Var(cs), 1)` in its slot.
    let ci = params.iter().position(|p| p.var == cs_var)?;
    if !is_heap_ty(&params[ci].ty) {
        return None;
    }
    let is_drop1 = |e: &IrExpr| -> bool {
        matches!(&e.kind, IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
            if module.as_str() == "list" && func.as_str() == "drop" && args.len() == 2
                && matches!(&args[0].kind, IrExprKind::Var { id } if *id == cs_var)
                && matches!(&args[1].kind, IrExprKind::LitInt { value: 1 }))
    };
    // Collect EVERY self-call anywhere in the body (not just tail position) and require each to pass
    // `list.drop(cs,1)` in slot `ci` — so `cs` is a pure forward iterator with no other use.
    let mut ok = true;
    let mut any_self = false;
    {
        use almide_ir::visit::IrVisitor;
        struct W<'a> {
            fn_name: &'a str,
            ci: usize,
            is_drop1: &'a dyn Fn(&IrExpr) -> bool,
            ok: &'a mut bool,
            any: &'a mut bool,
        }
        impl IrVisitor for W<'_> {
            fn visit_expr(&mut self, e: &IrExpr) {
                if let IrExprKind::Call { target: CallTarget::Named { name }, args, .. } = &e.kind {
                    if name.as_str() == self.fn_name {
                        *self.any = true;
                        if self.ci >= args.len() || !(self.is_drop1)(&args[self.ci]) {
                            *self.ok = false;
                        }
                    }
                }
                almide_ir::visit::walk_expr(self, e);
            }
        }
        let mut w = W { fn_name, ci, is_drop1: &is_drop1, ok: &mut ok, any: &mut any_self };
        w.visit_expr(body);
    }
    if !ok || !any_self {
        return None;
    }
    // Parse the two arms: a `None` arm (the BASE) and a `Some(ch | _)` arm (the BODY). `ch` is a
    // scalar element bind (String element) — bound to `cs[idx]` (a borrow) in the rewrite.
    use almide_ir::IrPattern;
    let mut none_body: Option<&IrExpr> = None;
    let mut some_body: Option<(&IrExpr, Option<(VarId, Ty)>)> = None;
    for arm in arms {
        if arm.guard.is_some() {
            return None;
        }
        match &arm.pattern {
            IrPattern::None | IrPattern::Wildcard if none_body.is_none() => none_body = Some(&arm.body),
            IrPattern::Some { inner } if some_body.is_none() => {
                let bind = match inner.as_ref() {
                    IrPattern::Bind { var, ty } => Some((*var, ty.clone())),
                    IrPattern::Wildcard => None,
                    _ => return None,
                };
                some_body = Some((&arm.body, bind));
            }
            _ => return None,
        }
    }
    let none_body = none_body?;
    let (some_body, ch_bind) = some_body?;
    let idx = VarId(fresh);
    let elem_ty = match &first_ty {
        Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, a) if a.len() == 1 => {
            a[0].clone()
        }
        _ => return None,
    };
    // list.len(cs): clone the `list.first` subject node + retarget to `len`, typed Int.
    let len_call = match &subject.kind {
        IrExprKind::Call { target: CallTarget::Module { module, def_id, .. }, args, type_args } => {
            tco_ir(
                IrExprKind::Call {
                    target: CallTarget::Module {
                        module: *module,
                        func: almide_lang::intern::sym("len"),
                        def_id: *def_id,
                    },
                    args: args.clone(),
                    type_args: type_args.clone(),
                },
                Ty::Int,
            )
        }
        _ => return None,
    };
    // cond: `idx < list.len(cs)`
    let cond = tco_ir(
        IrExprKind::BinOp {
            op: almide_ir::BinOp::Lt,
            left: Box::new(tco_ir(IrExprKind::Var { id: idx }, Ty::Int)),
            right: Box::new(len_call),
        },
        Ty::Bool,
    );
    // then: `{ [let ch = cs[idx]]; SOME_BODY }` — the element BORROW.
    let mut then_stmts: Vec<IrStmt> = Vec::new();
    if let Some((ch_var, ch_ty)) = ch_bind {
        let elem = tco_ir(
            IrExprKind::IndexAccess {
                object: Box::new(tco_ir(IrExprKind::Var { id: cs_var }, params[ci].ty.clone())),
                index: Box::new(tco_ir(IrExprKind::Var { id: idx }, Ty::Int)),
            },
            elem_ty,
        );
        then_stmts.push(IrStmt {
            kind: IrStmtKind::Bind {
                var: ch_var,
                mutability: almide_ir::Mutability::Let,
                ty: ch_ty,
                value: elem,
            },
            span: None,
        });
    }
    let then_expr = tco_ir(
        IrExprKind::Block { stmts: then_stmts, expr: Some(Box::new(some_body.clone())) },
        body.ty.clone(),
    );
    let new_body = tco_ir(
        IrExprKind::If {
            cond: Box::new(cond),
            then: Box::new(then_expr),
            else_: Box::new(none_body.clone()),
        },
        body.ty.clone(),
    );
    Some((new_body, idx, ci))
}

/// Find the FIRST heap-result `if`/`match` sitting in a call-ARGUMENT position anywhere within
/// `e` (recursing through nested calls), and return `(the branch, e with that branch replaced by
/// `Var(tmp)`)`. Each call's nested arguments are searched BEFORE the call's own direct args, so
/// `f(g(if..))` lifts the inner `if` first; the caller re-runs to a fixpoint to lift the rest.
/// Recursion is confined to `Call` nodes — a heap-branch that is NOT a call argument (e.g. a bare
/// `let s = if..`, or an `if`-arm interior) is left for the tail-duplication / per-arm machinery.
fn extract_first_callarg_branch(e: &IrExpr, tmp: VarId) -> Option<(IrExpr, IrExpr)> {
    // A TUPLE element may itself wrap a call-arg branch (`(value.str(if c then a else b), end)` — the
    // block_scalar/block_line return shape). Recurse into each element so the inner `if` is ANF-lifted
    // out (`let t = if c then a else b; (value.str(t), end)`), which `desugar_let_bound_heap_branch`
    // then tail-duplicates into a heap-result `if` with Tuple arms — both of which already lower.
    if let IrExprKind::Tuple { elements } = &e.kind {
        for (idx, el) in elements.iter().enumerate() {
            if let Some((branch, new_el)) = extract_first_callarg_branch(el, tmp) {
                let mut new_elements = elements.clone();
                new_elements[idx] = new_el;
                return Some((
                    branch,
                    IrExpr {
                        kind: IrExprKind::Tuple { elements: new_elements },
                        ty: e.ty.clone(),
                        span: e.span.clone(),
                        def_id: e.def_id,
                    },
                ));
            }
        }
        return None;
    }
    let IrExprKind::Call { target, args, type_args } = &e.kind else {
        return None;
    };
    let rebuild = |new_args: Vec<IrExpr>| IrExpr {
        kind: IrExprKind::Call {
            target: target.clone(),
            args: new_args,
            type_args: type_args.clone(),
        },
        ty: e.ty.clone(),
        span: e.span.clone(),
        def_id: e.def_id,
    };
    // (1) Innermost-first: a heap-branch nested inside a sub-call argument.
    for (idx, a) in args.iter().enumerate() {
        if let Some((branch, new_a)) = extract_first_callarg_branch(a, tmp) {
            let mut new_args = args.clone();
            new_args[idx] = new_a;
            return Some((branch, rebuild(new_args)));
        }
    }
    // (2) This call's own direct heap-branch argument.
    let arg_idx = args.iter().position(is_heap_branch)?;
    let branch = args[arg_idx].clone();
    let mut new_args = args.clone();
    new_args[arg_idx] = IrExpr {
        kind: IrExprKind::Var { id: tmp },
        ty: branch.ty.clone(),
        span: branch.span.clone(),
        def_id: None,
    };
    Some((branch, rebuild(new_args)))
}

/// ANF-LIFT a heap-result `if`/`match` out of a CALL-ARGUMENT into a fresh let-bind, so the
/// existing `desugar_let_bound_heap_branch` tail-duplication then makes it lower. Rewrites the
/// FIRST `f(.., if c then A else B, ..)` (including a nested `f(g(if..))` and the block's TAIL
/// expression `{ ..; f(if..) }`) to `let tmp = if c then A else B; f(.., tmp, ..)` (tmp = a fresh
/// `Var` of the arg's type). Returns `None` if no such call-arg exists. MUST be applied in BOTH
/// the lowering and the `count_ir_calls` gate via [`desugar_heap_branches`] (desugar-before-both)
/// so the duplicated calls stay 1:1 (mir == ir).
pub fn desugar_callarg_heap_if(body: &IrExpr, next_var: &mut u32) -> Option<IrExpr> {
    let IrExprKind::Block { stmts, expr: tail } = &body.kind else {
        // A BARE call/tuple body (not in a block) with a call-arg heap branch — `collect_block(..,
        // if list.is_empty(acc) then acc else acc+[""])`, a `block_line` if-arm reached via
        // `desugar_nested_branch_arms`. Lift the branch to a block `{ let tmp = if…; <body'> }`. The
        // fresh id comes from the FUNCTION-WIDE `next_var` counter, NOT `max_var_id(this arm)` — the arm
        // omits a sibling-arm var (`line`, used only in the else arm), so an arm-local max would alias
        // it and the renderer would read one arm's value in the other (block_line's `string.drop(v19)`).
        let tmp = VarId(*next_var);
        *next_var += 1;
        let (branch, new_body) = extract_first_callarg_branch(body, tmp)?;
        let lift = IrStmt {
            kind: IrStmtKind::Bind {
                var: tmp,
                mutability: almide_ir::Mutability::Let,
                ty: branch.ty.clone(),
                value: branch,
            },
            span: body.span.clone(),
        };
        return Some(IrExpr {
            kind: IrExprKind::Block { stmts: vec![lift], expr: Some(Box::new(new_body)) },
            ty: body.ty.clone(),
            span: body.span.clone(),
            def_id: body.def_id,
        });
    };
    let tmp = VarId(*next_var);
    *next_var += 1;
    // STATEMENT position: the first `Expr`/`Bind`/`Assign` whose value contains a call-arg branch.
    for (i, s) in stmts.iter().enumerate() {
        let value = match &s.kind {
            IrStmtKind::Expr { expr } => Some(expr),
            IrStmtKind::Bind { value, .. } => Some(value),
            IrStmtKind::Assign { value, .. } => Some(value),
            _ => None,
        };
        let Some(v) = value else { continue };
        let Some((branch, new_v)) = extract_first_callarg_branch(v, tmp) else {
            continue;
        };
        let lift = IrStmt {
            kind: IrStmtKind::Bind {
                var: tmp,
                mutability: almide_ir::Mutability::Let,
                ty: branch.ty.clone(),
                value: branch,
            },
            span: s.span.clone(),
        };
        let new_stmt = IrStmt {
            kind: match &s.kind {
                IrStmtKind::Expr { .. } => IrStmtKind::Expr { expr: new_v },
                IrStmtKind::Bind { var, mutability, ty, .. } => IrStmtKind::Bind {
                    var: *var,
                    mutability: *mutability,
                    ty: ty.clone(),
                    value: new_v,
                },
                IrStmtKind::Assign { var, .. } => IrStmtKind::Assign { var: *var, value: new_v },
                other => other.clone(),
            },
            span: s.span.clone(),
        };
        let mut new_stmts: Vec<IrStmt> = stmts[..i].to_vec();
        new_stmts.push(lift);
        new_stmts.push(new_stmt);
        new_stmts.extend(stmts[i + 1..].iter().cloned());
        return Some(IrExpr {
            kind: IrExprKind::Block { stmts: new_stmts, expr: tail.clone() },
            ty: body.ty.clone(),
            span: body.span.clone(),
            def_id: body.def_id,
        });
    }
    // TAIL position: `{ ..; f(if..) }` — the call is the block's return expression, not a
    // statement, so the lifted `let tmp = if..` is APPENDED and the rewritten call becomes the
    // new tail. The tail-duplication then pushes that tail into each arm.
    if let Some(t) = tail.as_deref() {
        if let Some((branch, new_t)) = extract_first_callarg_branch(t, tmp) {
            let lift = IrStmt {
                kind: IrStmtKind::Bind {
                    var: tmp,
                    mutability: almide_ir::Mutability::Let,
                    ty: branch.ty.clone(),
                    value: branch,
                },
                span: t.span.clone(),
            };
            let mut new_stmts = stmts.clone();
            new_stmts.push(lift);
            return Some(IrExpr {
                kind: IrExprKind::Block { stmts: new_stmts, expr: Some(Box::new(new_t)) },
                ty: body.ty.clone(),
                span: body.span.clone(),
                def_id: body.def_id,
            });
        }
    }
    None
}

/// Apply the call-arg ANF-lift ([`desugar_callarg_heap_if`]) and the heap-branch tail-duplication
/// ([`desugar_let_bound_heap_branch`]) repeatedly to a FIXPOINT — the exact rewrite sequence
/// `lower_body_into` performs before lowering. Both the lowering and the `count_ir_calls` caps gate
/// call this, so the duplicated calls are counted 1:1 (mir == ir) regardless of how many branches
/// a body lifts. Returns `None` if the body is already in normal form (no rewrite applied).
pub fn desugar_heap_branches(body: &IrExpr) -> Option<IrExpr> {
    // Seed a FUNCTION-WIDE fresh-VarId counter ABOVE every id in the whole body, then thread it through
    // the recursion so a lift inside one `if` arm never reuses an id live in a SIBLING arm (block_line's
    // `string.drop` read the then-arm's concat because an arm-local `max_var_id` aliased `line`).
    let mut next_var = max_var_id(body) + 1;
    desugar_heap_branches_inner(body, &mut next_var)
}

fn desugar_heap_branches_inner(body: &IrExpr, next_var: &mut u32) -> Option<IrExpr> {
    let mut cur: Option<IrExpr> = None;
    loop {
        let src = cur.as_ref().unwrap_or(body);
        if let Some(r) = desugar_callarg_heap_if(src, next_var) {
            cur = Some(r);
            continue;
        }
        if let Some(r) = desugar_let_bound_heap_branch(src) {
            cur = Some(r);
            continue;
        }
        if let Some(r) = desugar_nested_branch_arms(src, next_var) {
            cur = Some(r);
            continue;
        }
        return cur;
    }
}

/// Recurse the heap-branch desugar INTO an `if`/`match` arm and a block TAIL. After a let-bound
/// duplication the body becomes `Block{prefix; if c then {<nested branch>} else {…}}`, whose arm
/// blocks may still hide a call-arg `if` (`(value.str(if…), end)`) or another let-bound branch (the
/// block_scalar two-`if` shape). Normalizing those HERE — inside the SHARED `desugar_heap_branches`
/// both `lower_body_into` and the `count_ir_calls` caps gate call — keeps the duplicated calls 1:1
/// (mir == ir); doing it lowering-side only (in `lower_heap_result_arm`) would double-count.
fn desugar_nested_branch_arms(body: &IrExpr, next_var: &mut u32) -> Option<IrExpr> {
    match &body.kind {
        IrExprKind::If { cond, then, else_ } => {
            let nt = desugar_heap_branches_inner(then, next_var);
            let ne = desugar_heap_branches_inner(else_, next_var);
            if nt.is_none() && ne.is_none() {
                return None;
            }
            Some(IrExpr {
                kind: IrExprKind::If {
                    cond: cond.clone(),
                    then: Box::new(nt.unwrap_or_else(|| (**then).clone())),
                    else_: Box::new(ne.unwrap_or_else(|| (**else_).clone())),
                },
                ty: body.ty.clone(),
                span: body.span.clone(),
                def_id: body.def_id,
            })
        }
        IrExprKind::Match { subject, arms } => {
            let mut changed = false;
            let new_arms: Vec<almide_ir::IrMatchArm> = arms
                .iter()
                .map(|a| match desugar_heap_branches_inner(&a.body, next_var) {
                    Some(nb) => {
                        changed = true;
                        almide_ir::IrMatchArm {
                            pattern: a.pattern.clone(),
                            guard: a.guard.clone(),
                            body: nb,
                        }
                    }
                    None => a.clone(),
                })
                .collect();
            if !changed {
                return None;
            }
            Some(IrExpr {
                kind: IrExprKind::Match { subject: subject.clone(), arms: new_arms },
                ty: body.ty.clone(),
                span: body.span.clone(),
                def_id: body.def_id,
            })
        }
        IrExprKind::Block { stmts, expr: Some(tail) } => {
            let nt = desugar_heap_branches_inner(tail, next_var)?;
            Some(IrExpr {
                kind: IrExprKind::Block { stmts: stmts.clone(), expr: Some(Box::new(nt)) },
                ty: body.ty.clone(),
                span: body.span.clone(),
                def_id: body.def_id,
            })
        }
        _ => None,
    }
}

pub fn desugar_let_bound_heap_branch(body: &IrExpr) -> Option<IrExpr> {
    let IrExprKind::Block { stmts, expr: tail } = &body.kind else {
        return None;
    };
    // Find the first heap let-bound `if`/`match` bind.
    let (i, bind_var, bind_ty, branch) = stmts.iter().enumerate().find_map(|(i, s)| match &s.kind {
        IrStmtKind::Bind { var, ty, value, .. }
            if is_heap_ty(ty)
                && matches!(&value.kind, IrExprKind::If { .. } | IrExprKind::Match { .. }) =>
        {
            Some((i, *var, ty.clone(), value))
        }
        _ => None,
    })?;
    // BOUNDED-DUPLICATION gate: refuse when the continuation itself carries another unresolved
    // heap let-bound `if`/`match`.
    // BOUNDED-DUPLICATION: the continuation is copied into BOTH arms, so each remaining heap let-bound
    // `if`/`match` in `rest` doubles the leaf-arm count as the fixpoint resolves them one at a time. A
    // FEW are fine (block_scalar = 2: `let joined = if…; let tmp = if…(value.str arg, ANF-lifted)`), so
    // allow up to 2 (≤ 2^3 = 8 leaves) and refuse beyond that to keep the duplication bounded.
    let rest_branch_binds = stmts[i + 1..]
        .iter()
        .filter(|s| {
            matches!(
                &s.kind,
                IrStmtKind::Bind { ty, value, .. }
                    if is_heap_ty(ty)
                        && matches!(&value.kind, IrExprKind::If { .. } | IrExprKind::Match { .. })
            )
        })
        .count();
    if rest_branch_binds > 2 {
        return None;
    }
    let result_ty = &body.ty;
    let rest_stmts: Vec<IrStmt> = stmts[i + 1..].to_vec();
    let rest_tail: Option<Box<IrExpr>> = tail.clone();
    // Reduce a `match` to a nested literal-pattern `if` chain (the same `desugar_match_to_if`
    // the tail/scalar machinery uses) — a pure builder, so a throwaway default ctx suffices.
    let if_branch = match &branch.kind {
        IrExprKind::If { .. } => (*branch).clone(),
        IrExprKind::Match { subject, arms } => {
            LowerCtx::default().desugar_match_to_if(subject, arms, &branch.ty)?
        }
        _ => return None,
    };
    let rewritten_branch = LowerCtx::wrap_branch_arms(
        &if_branch, bind_var, &bind_ty, &rest_stmts, &rest_tail, result_ty,
    );
    // The prefix statements `stmts[0..i]` stay; the rewritten branch is the new block TAIL.
    let prefix: Vec<IrStmt> = stmts[..i].to_vec();
    Some(IrExpr {
        kind: IrExprKind::Block { stmts: prefix, expr: Some(Box::new(rewritten_branch)) },
        ty: result_ty.clone(),
        span: body.span.clone(),
        def_id: body.def_id,
    })
}

/// The kind of a call's resolved target — used to make a walled `Call`'s reason
/// precise (the histogram then names which call SHAPE to admit next: a free
/// `Named` call vs a stdlib `Module` dispatch vs an unresolved `Method` vs a
/// `Computed` callee), so the coverage roadmap is evidence-based, not guessed.
pub(crate) fn call_target_kind(t: &CallTarget) -> &'static str {
    match t {
        CallTarget::Named { .. } => "Named",
        CallTarget::Module { .. } => "Module",
        CallTarget::Method { .. } => "Method",
        CallTarget::Computed { .. } => "Computed",
    }
}

pub(crate) fn kind_name(k: &IrExprKind) -> &'static str {
    // Named precisely so the corpus-wall `<other>` buckets break down into the
    // exact expression forms still to admit (an evidence-based roadmap, the same
    // discipline as `call_target_kind`). Unnamed kinds remain `<other>`.
    match k {
        IrExprKind::LitInt { .. } => "LitInt",
        IrExprKind::LitFloat { .. } => "LitFloat",
        IrExprKind::LitStr { .. } => "LitStr",
        IrExprKind::LitBool { .. } => "LitBool",
        IrExprKind::Unit => "Unit",
        IrExprKind::Var { .. } => "Var",
        IrExprKind::List { .. } => "List",
        IrExprKind::Record { .. } => "Record",
        IrExprKind::Tuple { .. } => "Tuple",
        IrExprKind::Block { .. } => "Block",
        IrExprKind::Call { .. } => "Call",
        IrExprKind::RuntimeCall { .. } => "RuntimeCall",
        IrExprKind::BinOp { .. } => "BinOp",
        IrExprKind::UnOp { .. } => "UnOp",
        IrExprKind::If { .. } => "If",
        IrExprKind::Match { .. } => "Match",
        IrExprKind::Member { .. } => "Member",
        IrExprKind::TupleIndex { .. } => "TupleIndex",
        IrExprKind::IndexAccess { .. } => "IndexAccess",
        IrExprKind::MapAccess { .. } => "MapAccess",
        IrExprKind::Range { .. } => "Range",
        IrExprKind::MapLiteral { .. } => "MapLiteral",
        IrExprKind::EmptyMap => "EmptyMap",
        IrExprKind::StringInterp { .. } => "StringInterp",
        IrExprKind::Lambda { .. } => "Lambda",
        IrExprKind::ClosureCreate { .. } => "ClosureCreate",
        IrExprKind::FnRef { .. } => "FnRef",
        IrExprKind::ResultOk { .. } => "ResultOk",
        IrExprKind::ResultErr { .. } => "ResultErr",
        IrExprKind::OptionSome { .. } => "OptionSome",
        IrExprKind::OptionNone => "OptionNone",
        IrExprKind::Try { .. } => "Try",
        IrExprKind::Unwrap { .. } => "Unwrap",
        IrExprKind::UnwrapOr { .. } => "UnwrapOr",
        IrExprKind::ForIn { .. } => "ForIn",
        IrExprKind::While { .. } => "While",
        IrExprKind::Fan { .. } => "Fan",
        IrExprKind::Break => "Break",
        IrExprKind::Continue => "Continue",
        IrExprKind::TailCall { .. } => "TailCall",
        IrExprKind::IterChain { .. } => "IterChain",
        IrExprKind::Await { .. } => "Await",
        IrExprKind::Clone { .. } => "Clone",
        IrExprKind::Deref { .. } => "Deref",
        IrExprKind::Borrow { .. } => "Borrow",
        IrExprKind::ToVec { .. } => "ToVec",
        IrExprKind::BoxNew { .. } => "BoxNew",
        IrExprKind::SpreadRecord { .. } => "SpreadRecord",
        _ => "<other>",
    }
}

#[cfg(test)]
mod tests;

