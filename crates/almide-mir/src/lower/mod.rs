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

/// Is `init` a PURE (call-free) LITERAL `List` — every element a bare `LitStr` / `LitInt` /
/// `LitFloat` / `LitBool`, NO nested call/var/interpolation? This is the admission gate for
/// materializing a NESTED-OWNERSHIP module-level list global (`let DIFFICULTIES = ["a", "b"]`)
/// via the `DynListStr` builder: a call-free literal list injects ZERO `CallFn`, so the gate's
/// IR-side `count_ir_calls` (which sees the reference as a single `Var` = 0 calls) stays exact.
/// A computed element (a call, a var, a `${...}`) returns `false` → the global keeps walling
/// (materializing it would inject an uncounted call ⇒ a false caps de-taint).
fn is_pure_literal_list(init: &IrExpr) -> bool {
    let IrExprKind::List { elements } = &init.kind else {
        return false;
    };
    !elements.is_empty()
        && elements.iter().all(|e| {
            matches!(
                &e.kind,
                IrExprKind::LitStr { .. }
                    | IrExprKind::LitInt { .. }
                    | IrExprKind::LitFloat { .. }
                    | IrExprKind::LitBool { .. }
            )
        })
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

/// Map a declared Almide scalar/heap type to its host wasm IMPORT valtype (the
/// `@extern(wasm, …)` ABI): `Int`/narrow ints → `I64`, `Float` → `F64`, `Bool` →
/// `I32`, a `String`/heap pointer → `I32`. A type with no flat valtype mapping
/// (a record/tuple/Value/Unknown) returns `None` — the caller WALLS rather than
/// guess an ABI. `Unit` is handled by the caller (a void result), not here.
fn extern_wasm_abi(ty: &Ty) -> Option<crate::WasmAbi> {
    use crate::WasmAbi;
    match ty {
        Ty::Int | Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64 | Ty::UInt8 | Ty::UInt16
        | Ty::UInt32 | Ty::UInt64 => Some(WasmAbi::I64),
        Ty::Float | Ty::Float32 | Ty::Float64 => Some(WasmAbi::F64),
        Ty::Bool => Some(WasmAbi::I32),
        // A String / list / map / any heap value crosses the boundary as an i32 POINTER.
        _ if is_heap_ty(ty) => Some(WasmAbi::I32),
        _ => None,
    }
}

/// The `@extern(wasm, module, name)` attribute on a function, iff present (the
/// browser-import case — a `rust`/`rs` target keeps walling: there is no wasm host
/// for it, so emitting an import would be a hollow lie). Returns `(module, name)`.
fn extern_wasm_target(func: &IrFunction) -> Option<(String, String)> {
    func.extern_attrs.iter().find_map(|a| {
        if a.target.as_str() == "wasm" {
            Some((a.module.as_str().to_string(), a.function.as_str().to_string()))
        } else {
            None
        }
    })
}

/// Lower a body-less `@extern(wasm, module, name)` function to a thin wasm-IMPORT
/// call body (the browser dom/fetch/timer/console host stubs). The function becomes
/// a `(call $__import_module_name <params>)` that returns the host's result —
/// FAITHFUL: its behavior IS the host's, so it calls the host, it does NOT fabricate
/// a value (an `Opaque`/`0` would be a silent miscompile). The wasm module is valid;
/// a browser host satisfies the import (it does not instantiate under wasmtime, which
/// is expected — these fns are 🟡 lower, not byte-matchable on the wasmtime oracle).
///
/// Returns `Ok(Some(MirFunction))` when this is a wasm-extern fn whose param + return
/// types all map to flat valtypes; `Ok(None)` when it is NOT a wasm-extern (the caller
/// lowers it normally); `Err(Unsupported)` when a param/return type has no flat-valtype
/// ABI (WALL — never guess a signature). SOUNDNESS: a `rust`/`rs` extern is NOT a wasm
/// import (no wasm host) → `extern_wasm_target` is `None` → it keeps walling.
fn try_lower_extern_wasm(func: &IrFunction) -> Result<Option<MirFunction>, LowerError> {
    let Some((module, name)) = extern_wasm_target(func) else { return Ok(None) };
    // Bind params to fresh MIR values (the borrow-by-default convention) — a heap param
    // is a borrowed i32 pointer, a scalar an i64 local; both are read into the call.
    let mut ctx = LowerCtx { fn_name: func.name.as_str().to_string(), ..Default::default() };
    let params = ctx.bind_params(&func.params)?;
    // The import-call args + their per-arg valtypes, parallel to the params. A heap param
    // is BORROWED (a `Handle` — the caller owns it, no refcount change here); a scalar is
    // passed by value (`Scalar`). The ABI of each comes from the DECLARED param type.
    let mut args: Vec<crate::CallArg> = Vec::new();
    let mut abi: Vec<crate::WasmAbi> = Vec::new();
    for (p, ip) in params.iter().zip(func.params.iter()) {
        let a = extern_wasm_abi(&ip.ty).ok_or_else(|| {
            LowerError::Unsupported(format!(
                "@extern(wasm) param type {:?} has no flat wasm valtype (not lowered to an import)",
                ip.ty
            ))
        })?;
        abi.push(a);
        args.push(if p.repr.is_heap() {
            crate::CallArg::Handle(p.value)
        } else {
            crate::CallArg::Scalar(p.value)
        });
    }
    // The result: `Unit` → a void import (no MIR result); else map the return type to its
    // valtype + a fresh dst the call binds. A heap return is a FRESH OWNED pointer the host
    // returns (the caller now owns it — moved out as `ret`, like an `Alloc` result).
    let (dst, result, result_abi, ret) = if matches!(func.ret_ty, Ty::Unit) {
        (None, None, None, None)
    } else {
        let rabi = extern_wasm_abi(&func.ret_ty).ok_or_else(|| {
            LowerError::Unsupported(format!(
                "@extern(wasm) return type {:?} has no flat wasm valtype (not lowered to an import)",
                func.ret_ty
            ))
        })?;
        let repr = repr_of(&func.ret_ty)?;
        let d = ctx.fresh_value();
        (Some(d), Some(repr), Some(rabi), Some(d))
    };
    ctx.ops.push(Op::CallImport { dst, module, name, args, abi, result, result_abi });
    Ok(Some(MirFunction {
        name: func.name.as_str().to_string(),
        params,
        ops: ctx.ops,
        // A wasm import reaches a BROWSER host capability (dom/fetch/timer/console), which is
        // OUTSIDE the v1 WASI-floor cap vocabulary (Stdout/Entropy/CliArgs/FsRead). So it
        // declares no MODELED cap here; the `CallImport` reaches no modeled WASI cap either,
        // so `used ⊆ declared` holds vacuously — honest (it is not claimed to reach a WASI cap).
        ret,
        declared_caps: Vec::new(),
        heap_slot_masks: Default::default(),
    }))
}

/// Lower one function to MIR. Parameters are seeded first (the v1 borrow-by-
/// default calling convention — see [`LowerCtx::bind_params`]), then the body.
/// STRICT VALUE MODE (flight-evidence-gaps F2 — retiring the deferred-Const):
/// when set, every lowering site that would fall back to `Op::Const` (the
/// deferred ZERO whose only legitimate consumer is the caps-counting
/// classifier) REFUSES instead — a walled function can never print a silently
/// wrong value (the `prim.handle(<literal>)` → address-0 class). Set ONCE by
/// the render/output entrypoints (render_program); the classifier and the
/// in-process unit tests keep the permissive caps-counting behavior.
pub static STRICT_VALUES: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(crate) fn strict_values() -> bool {
    STRICT_VALUES.load(std::sync::atomic::Ordering::Relaxed)
}

pub(crate) fn strict_const_wall(what: &str) -> LowerError {
    LowerError::Unsupported(format!(
        "scalar {what} outside the value subset cannot be faithfully computed in this          brick (the permissive caps-counting path defers it to Const 0; STRICT value          mode refuses instead of risking a silently wrong value)"
    ))
}

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

/// Substitute every `Var { id: from }` in `e` with `Var { id: to }` — the binder rebind
/// the defunc match-arm transforms use (`some(b) => X` becomes `X[b := payload_var]`).
pub(crate) fn subst_var_ir(e: &almide_ir::IrExpr, from: VarId, to: VarId) -> almide_ir::IrExpr {
    fn walk(e: almide_ir::IrExpr, from: VarId, to: VarId) -> almide_ir::IrExpr {
        let mut e = e.map_children(&mut |c| walk(c, from, to));
        if let almide_ir::IrExprKind::Var { id } = &mut e.kind {
            if *id == from {
                *id = to;
            }
        }
        e
    }
    walk(e.clone(), from, to)
}

/// Resolve a TYPE NAME against the record registry, accepting the BARE spelling of a
/// cross-module type when it is UNAMBIGUOUS: the frontend qualifies an imported DECL
/// (`types_mod.Lin`) but leaves some USE-site `Ty::Named`s bare (`Lin` — the alias-typed
/// annotation `tm.Lin` resolves to the decl's own Sym), so an exact miss falls back to
/// the unique `".{name}"`-suffixed key. Two modules exporting the same bare name stay
/// unresolved (`None`) — the caller walls, never a wrong-layout guess. Returns the
/// CANONICAL registry key, which is also the drop-fn identity (`$__drop_<canonical>`),
/// so lowering-side routing and the decl-side generators can never disagree on a name.
pub(crate) fn canonical_record_key<'a>(layouts: &'a RecordLayouts, name: &str) -> Option<&'a str> {
    if let Some((k, _)) = layouts.get_key_value(name) {
        return Some(k.as_str());
    }
    let suffix = format!(".{name}");
    let mut found: Option<&'a str> = None;
    for k in layouts.keys() {
        if k.ends_with(&suffix) {
            if found.is_some() {
                return None; // ambiguous bare name — walled, never a guess
            }
            found = Some(k.as_str());
        }
    }
    found
}

/// The [`canonical_record_key`] resolution over a NAME SET (the drop generators'
/// `rec_names`) instead of the layout map — the same exact-then-unique-suffix rule.
pub(crate) fn canonical_name_in<'a>(
    names: &'a std::collections::HashSet<String>,
    name: &str,
) -> Option<&'a str> {
    if let Some(k) = names.get(name) {
        return Some(k.as_str());
    }
    let suffix = format!(".{name}");
    let mut found: Option<&'a str> = None;
    for k in names {
        if k.ends_with(&suffix) {
            if found.is_some() {
                return None;
            }
            found = Some(k.as_str());
        }
    }
    found
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
    record_names: &std::collections::HashSet<String>,
) -> bool {
    use almide_ir::{IrTypeDeclKind, IrVariantKind};
    let IrTypeDeclKind::Variant { cases, .. } = &decl.kind else {
        return false;
    };
    // A ctor field the generated `$__drop_<V>` can free: a nested variant (recurse), a String
    // (rc_dec), a List[scalar] (flat rc_dec), a List[<variant>] (per-element), or a RECORD (recurse
    // via `$__drop_<R>` / a scalar-only record's flat rc_dec — see the drop generator's field loop).
    let supported_heap = |t: &Ty| -> bool {
        use almide_lang::types::constructor::TypeConstructorId;
        variant_field_name(t, variant_names).is_some()
            || matches!(t, Ty::Named(n, _) if record_names.contains(n.as_str()))
            || matches!(t, Ty::String)
            || matches!(t, Ty::Applied(TypeConstructorId::List, a)
                if a.len() == 1
                    && (!is_heap_ty(&a[0])
                        || variant_field_name(&a[0], variant_names).is_some()))
    };
    let mut any_heap = false;
    let mut all_supported = true;
    let mut has_variant_field = false;
    for c in cases {
        let tys: Vec<&Ty> = match &c.kind {
            IrVariantKind::Unit => vec![],
            IrVariantKind::Tuple { fields } => fields.iter().collect(),
            IrVariantKind::Record { fields } => fields.iter().map(|f| &f.ty).collect(),
        };
        for t in tys {
            if variant_field_name(t, variant_names).is_some() {
                has_variant_field = true;
            }
            if is_heap_ty(t) {
                any_heap = true;
                if !supported_heap(t) {
                    all_supported = false;
                }
            }
        }
    }
    // The ORIGINAL rule (a nested-variant field) OR the widened one: some heap field,
    // ALL of them freeable by the generator (String / List[scalar] / List[variant]) —
    // the GGUFValue shape (ValString + ValArray(List[GGUFValue])). A type with an
    // unsupported heap field (e.g. a Map) keeps needing=false → its list stays WALLED
    // (never a silent leak).
    has_variant_field || (any_heap && all_supported)
}

/// The set of FLAT variant type names — every constructor scalar-only, so the block owns NO inner
/// handle (a nullary enum like `Capability`, or a scalar-payload variant). A `List[flat-variant]`
/// record/anon field is freed per-element by `__drop_list_str` (`rc_dec` of each flat element block +
/// the list block); a variant carrying a `String`/nested/`List` field is NOT flat (its block owns an
/// inner handle) and is excluded — its `List` field stays on the existing flat-block `rc_dec` (the
/// materializer also walls a non-flat-variant list, so such a field is never built). The drop-side
/// mirror of [`crate::lower::VariantLayouts::is_flat_variant_ty`].
pub fn flat_variant_type_names(
    type_decls: &[almide_ir::IrTypeDecl],
) -> std::collections::HashSet<String> {
    use almide_ir::{IrTypeDeclKind, IrVariantKind};
    type_decls
        .iter()
        .filter_map(|d| {
            let IrTypeDeclKind::Variant { cases, .. } = &d.kind else { return None };
            let flat = cases.iter().all(|c| {
                let tys: Vec<&Ty> = match &c.kind {
                    IrVariantKind::Unit => vec![],
                    IrVariantKind::Tuple { fields } => fields.iter().collect(),
                    IrVariantKind::Record { fields } => fields.iter().map(|f| &f.ty).collect(),
                };
                tys.iter().all(|t| !is_heap_ty(t))
            });
            flat.then(|| d.name.as_str().to_string())
        })
        .collect()
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

/// The `__drop_<T>` FUNCTION IDENTIFIER for a (possibly module-prefixed) type name. A cross-module
/// type carries its module prefix in the IR (`self.types.RunResult` → `Ty::Named("types.RunResult")`);
/// a dot is illegal in an Almide function name, so the generated drop fn / its call sites / the
/// rendered `(call $__drop_…)` all sanitize dots to underscores — the SAME mangling v0 codegen
/// applies (`almide_rt_types_RunResult`). For a single-file (dot-free) type this is the identity, so
/// the v0 corpus / spec fixtures render byte-identically. The `Op::DropVariant` renderer applies the
/// IDENTICAL transform, keeping the call site and the definition in lockstep.
pub fn drop_fn_ident(type_name: &str) -> String {
    type_name.replace('.', "_")
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
    // A variant FIELD that is itself a FLAT variant (e.g. `BlockType.BlockVal(ValType)`) is a single
    // owned tag-block with no inner handle: it must be freed by a flat `rc_dec`, NOT a recursive
    // `__drop_<flatvariant>` (which is never generated for a flat variant — it has no heap field — and
    // would render a DANGLING call). Mirrors the record-drop generator's `is_flat_variant_elem` treatment.
    let flat_names = flat_variant_type_names(type_decls);
    // The RICH (recursive-drop) variant type names — those for which `$__drop_<V>` is generated below.
    // A `List[<rich variant>]` ctor field (the wasm `Instr.Block(BlockType, List[Instr])` shape) is
    // freed RECURSIVELY via `$__drop_list_<V>` (each element → `$__drop_<V>`, mutually recursive); a
    // flat one-level `rc_dec` of the list block would leak every element's nested children.
    // A ctor field that is itself a RECORD: freed via `$__drop_<R>` (recursive-drop record — a
    // nested String/heap field) or a flat `rc_dec` (scalar-only record). `all_record_names` gates the
    // detection + the `needs_recursive_drop` widening, `rec_record_names` selects the free.
    let all_record_names: std::collections::HashSet<String> = type_decls
        .iter()
        .filter(|d| matches!(&d.kind, IrTypeDeclKind::Record { .. }))
        .map(|d| d.name.as_str().to_string())
        .collect();
    let rec_record_names = recursive_record_drop_names(type_decls);
    let rec_variant_names: std::collections::HashSet<String> = type_decls
        .iter()
        .filter(|d| variant_needs_recursive_drop(d, &names, &all_record_names))
        .map(|d| d.name.as_str().to_string())
        .collect();
    let mut out = String::new();
    for decl in type_decls {
        if !variant_needs_recursive_drop(decl, &names, &all_record_names) {
            continue;
        }
        let IrTypeDeclKind::Variant { cases, .. } = &decl.kind else { continue };
        let tname = decl.name.as_str();
        // The fn NAME sanitizes the module prefix (`types.RunResult` → `types_RunResult`); the param
        // TYPE annotation keeps the dotted module-qualified name (a valid Almide type reference).
        let fname = drop_fn_ident(tname);
        out.push_str(&format!("fn __drop_{fname}(e: {tname}) -> Unit = {{\n"));
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
                    if flat_names.contains(&fv) {
                        // A flat-variant field — a single owned block, freed by one `rc_dec` (no
                        // recursive `__drop_<fv>` exists for a flat variant). No `let` binding needed.
                        frees.push_str(&format!(
                            "        prim.rc_dec(prim.load64(h + {off}))\n"
                        ));
                    } else {
                        let fv_fn = drop_fn_ident(&fv);
                        frees.push_str(&format!(
                            "        let f{idx}: {fv} = prim.load_handle(h + {off})\n        __drop_{fv_fn}(f{idx})\n"
                        ));
                        idx += 1;
                    }
                } else if matches!(ty, Ty::String) {
                    frees.push_str(&format!(
                        "        prim.rc_dec(prim.load64(h + {off}))\n"
                    ));
                } else if matches!(ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                    if a.len() == 1 && !is_heap_ty(&a[0]))
                {
                    // A List[scalar] ctor field — a FLAT block, one rc_dec is its full free.
                    frees.push_str(&format!(
                        "        prim.rc_dec(prim.load64(h + {off}))
"
                    ));
                } else if let Some(ev) = list_rich_variant_elem(ty, &rec_variant_names) {
                    // A `List[<rich variant>]` ctor field (`Block(_, List[Instr])`): each element is a
                    // recursive-drop variant block, freed per-element by the generated `$__drop_list_<ev>`
                    // (→ `$__drop_<ev>`). A flat `rc_dec` of the list block would leak every element.
                    let ev_fn = drop_fn_ident(&ev);
                    frees.push_str(&format!(
                        "        let f{idx}: List[{ev}] = prim.load_handle(h + {off})\n        __drop_list_{ev_fn}(f{idx})\n"
                    ));
                    idx += 1;
                } else if let Ty::Named(rn, _) = ty {
                    if all_record_names.contains(rn.as_str()) {
                        // A RECORD-type ctor field (`Wrap(Color)` / `Box(Inner)`). A recursive-drop
                        // record (a String / nested-heap field) recurses via `$__drop_<R>`; a
                        // scalar-only record block is a single owned allocation, one `rc_dec` its full
                        // free. Either way the ctor stored its HANDLE at this slot.
                        if rec_record_names.contains(rn.as_str()) {
                            let rn_fn = drop_fn_ident(rn.as_str());
                            let rn_s = rn.as_str();
                            frees.push_str(&format!(
                                "        let f{idx}: {rn_s} = prim.load_handle(h + {off})\n        __drop_{rn_fn}(f{idx})\n"
                            ));
                            idx += 1;
                        } else {
                            frees.push_str(&format!(
                                "        prim.rc_dec(prim.load64(h + {off}))\n"
                            ));
                        }
                    }
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
    // A per-element-recursive `$__drop_list_<V>` for EVERY rich variant V — so a `List[V]` value (the
    // wasm `read_instrs` accumulator) AND a `List[V]` FIELD of a record (`Global.init`, freed via
    // `record_drop_field_frees` → `__drop_list_<V>`) reclaim each element through `$__drop_<V>`. Mirrors
    // the record list-drop loop in `generate_record_drop_sources` (the variant is the element drop). The
    // recursion `$__drop_<V> ↔ $__drop_list_<V>` terminates on a finite (parsed) tree; both are trusted
    // prim-only routines (empty cert), verified by the create+drop leak loop. Sorted for host-determinism.
    let mut list_drop_names: Vec<&String> = rec_variant_names.iter().collect();
    list_drop_names.sort();
    for vn in list_drop_names {
        let vn_fn = drop_fn_ident(vn);
        out.push_str(&format!(
            "fn __drop_list_{vn_fn}(xs: List[{vn}]) -> Unit = {{\n  \
               let h = prim.handle(xs)\n  \
               if prim.load32(h + 0) == 1 then __drop_list_{vn_fn}_loop(h, prim.load32(h + 4), 0) else ()\n  \
               prim.rc_dec(h)\n}}\n\
             fn __drop_list_{vn_fn}_loop(h: Int, n: Int, i: Int) -> Unit =\n  \
               if i >= n then ()\n  \
               else {{ let e: {vn} = prim.load_handle(h + 12 + i * 8)\n         __drop_{vn_fn}(e)\n         __drop_list_{vn_fn}_loop(h, n, i + 1) }}\n"
        ));
    }
    out
}

/// Does a record carrying a field of type `ty` need a generated recursive `$__drop_<R>` (rather than
/// a flat one-level `rc_dec` of its block)? ANY heap field does: a flat `rc_dec` of the record block
/// frees only the block, leaking every owned heap SLOT (a `String` handle, a `List`/`Map`/`Value`
/// handle, a nested record). This was historically `false` for `String` / `List[scalar]` because the
/// DIRECT-drop path masks those slots (`record_masks` → `DropListStr`); but a record so classified
/// gets NO `$__drop_<R>`, so when it is NESTED as a field of ANOTHER recursive record the outer's
/// per-field free (`record_drop_field_frees`) has no routine to call and falls back to a flat
/// `rc_dec` that LEAKS the inner slot (the porta `Parser = { bytes: List[Int], pos: Int }` nested in
/// `{ val, next: Parser }` — its `bytes` list leaked). Generating `$__drop_<R>` for every heap-field
/// record closes that: for an already-direct-dropped record the generated body frees the SAME slots
/// as the mask (`String`/`List[scalar]` → one `rc_dec` each), so the output is byte-identical and the
/// ownership cert stays a single `d`; the only delta is that the routine now EXISTS for nesting.
pub fn record_field_needs_recursive_drop(ty: &Ty) -> bool {
    is_heap_ty(ty)
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
        // An ANONYMOUS record field that itself needs the recursive drop (a heap-nested anon
        // record, e.g. `{ st: Cfb8State }` inside another anon record) routes to its synthesized
        // `__drop_<anon_hash>` (registered by `anon_record_drop_name`). It is NOT in `type_decls`,
        // so `rec_names` won't carry it — admit it directly here.
        Ty::Record { fields } if anon_record_needs_recursive_drop(fields) => {
            return Some(anon_record_drop_name(fields));
        }
        _ => return None,
    };
    // A cross-module field may be spelled BARE (`Lin`) while `rec_names` carries the
    // QUALIFIED decl name (`types_mod.Lin`) — resolve via the unique-suffix rule so the
    // generated free targets the real `$__drop_<canonical>` (an ambiguous bare name stays
    // unresolved → the field falls to the flat arm, never a wrong-name dangling call).
    canonical_name_in(rec_names, &n).map(|k| k.to_string())
}

/// A DETERMINISTIC, host-independent synthetic type name for an ANONYMOUS record shape, used as the
/// suffix of its synthesized recursive drop `$__drop_<name>` (and the `variant_drop_handles` route).
/// FNV-1a over the ordered `(field-name, field-type-tag)` shape — the SAME shape two structurally
/// equal anon records share, so they dedup to one `__drop`. The `anonrec_` prefix keeps it disjoint
/// from any user type name. Stable across native/wasm hosts (pure arithmetic, no pointer/order deps).
pub(crate) fn anon_record_drop_name(fields: &[(almide_lang::intern::Sym, Ty)]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    let mut mix = |bytes: &[u8]| {
        for &b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    };
    for (name, ty) in fields {
        mix(name.as_str().as_bytes());
        mix(b"\x00");
        mix(ty_shape_tag(ty).as_bytes());
        mix(b"\x00");
    }
    format!("anonrec_{h:016x}")
}

/// A structural string tag for a field type, fine enough that two anon records with DIFFERENT field
/// types (hence different drop bodies) get different names, recursing into nested aggregates so a
/// `{ st: A }` and `{ st: B }` never collide. Only the drop-relevant structure matters.
fn ty_shape_tag(ty: &Ty) -> String {
    use almide_lang::types::constructor::TypeConstructorId;
    match ty {
        Ty::Named(n, _) => format!("N{}", n.as_str()),
        Ty::Applied(TypeConstructorId::UserDefined(n), _) => format!("N{n}"),
        Ty::Applied(c, a) => {
            let inner: Vec<String> = a.iter().map(ty_shape_tag).collect();
            format!("A{c:?}[{}]", inner.join(","))
        }
        Ty::Record { fields } | Ty::OpenRecord { fields } => {
            let inner: Vec<String> =
                fields.iter().map(|(k, t)| format!("{}:{}", k.as_str(), ty_shape_tag(t))).collect();
            format!("R{{{}}}", inner.join(","))
        }
        Ty::Tuple(elems) => {
            let inner: Vec<String> = elems.iter().map(ty_shape_tag).collect();
            format!("T({})", inner.join(","))
        }
        other => format!("{other:?}"),
    }
}

/// Does an ANONYMOUS record (`Ty::Record`) need a SYNTHESIZED recursive `$__drop_<hash>`? It does iff
/// ANY field needs a recursive drop ([`record_field_needs_recursive_drop`]) — EXACTLY the predicate
/// `recursive_record_drop_names` uses for NAMED records, since the slot layout is identical. A flat
/// one-level mask `rc_dec`s only each field's HANDLE: that fully frees a flat-heap field (Bytes /
/// String — a single buffer) but only frees the BLOCK of a field that itself holds heap handles (a
/// nested record / Value / Map / `List[heap]`), leaking what's inside. So an anon record that owns
/// any heap field at all needs the synthesized recursive drop (the body flat-frees the
/// single-buffer fields and recurses into the handle-holding ones via `record_drop_field_frees`).
/// `record_field_needs_recursive_drop` is structural and host-independent.
pub(crate) fn anon_record_needs_recursive_drop(fields: &[(almide_lang::intern::Sym, Ty)]) -> bool {
    fields.iter().any(|(_, t)| record_field_needs_recursive_drop(t))
}

/// The per-field FREE statements of a record's recursive `$__drop` body (shared by the named-record
/// and the synthesized anon-record generators — the SINGLE source of truth for record field drops,
/// so the two can never drift). Each field at `slot_offset(i)` is freed by its CONCRETE type:
/// `String → rc_dec`, `Map[String,String] → __drop_map_ss`, `List[String] → __drop_list_str`,
/// `List[<recursive record>] → __drop_list_<R>`, a recursive record (named or anon) → `__drop_<R>`,
/// a `Value → __drop_value`, a scalar-only nested aggregate / `List[scalar]` → flat `rc_dec`, a
/// scalar → skip. Records the needed shared-helper flags into the caller's accumulators so they are
/// emitted once at the end.
#[allow(clippy::too_many_arguments)]
fn record_drop_field_frees(
    field_tys: &[Ty],
    rec_names: &std::collections::HashSet<String>,
    flat_variant_names: &std::collections::HashSet<String>,
    rec_variant_names: &std::collections::HashSet<String>,
    list_drops: &mut std::collections::BTreeSet<String>,
    need_map_ss: &mut bool,
    need_list_str: &mut bool,
    need_matrix: &mut bool,
    need_list_matrix: &mut bool,
) -> String {
    use almide_lang::types::constructor::TypeConstructorId;
    let mut frees = String::new();
    for (i, ty) in field_tys.iter().enumerate() {
        let off = layout::slot_offset(i);
        match ty {
            Ty::String => {
                frees.push_str(&format!("    prim.rc_dec(prim.load64(h + {off}))\n"));
            }
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => {
                if let Some(rn) = recursive_aggregate_name(&a[0], rec_names) {
                    list_drops.insert(rn.clone());
                    let rn_fn = drop_fn_ident(&rn);
                    // The BINDING type must be valid Almide source: a NAMED element renders
                    // its name, an ANONYMOUS element its STRUCTURAL `{ k: T, … }` form — the
                    // synthesized `anonrec_<hash>` is a drop-fn identity, NOT a type name
                    // (writing `List[anonrec_…]` type-errored the whole generated batch:
                    // "undefined variable 'f0'" after the rejected let).
                    let src = aggregate_source_ty(&a[0]);
                    frees.push_str(&format!(
                        "    let f{i}: List[{src}] = prim.load_handle(h + {off})\n    __drop_list_{rn_fn}(f{i})\n"
                    ));
                } else if let Some(ev) = list_rich_variant_elem(ty, rec_variant_names) {
                    // `List[<rich variant>]` (`Global.init: List[Instr]`): each element is a
                    // recursive-drop variant block, freed per-element by `$__drop_list_<ev>` (→
                    // `$__drop_<ev>`, generated by `generate_variant_drop_sources`). A flat `rc_dec`
                    // of the list block would leak every element's nested children (its own List[Instr]).
                    let ev_fn = drop_fn_ident(&ev);
                    frees.push_str(&format!(
                        "    let f{i}: List[{ev}] = prim.load_handle(h + {off})\n    __drop_list_{ev_fn}(f{i})\n"
                    ));
                } else if matches!(&a[0], Ty::Matrix | Ty::Applied(TypeConstructorId::Matrix, _)) {
                    // `List[Matrix]` — each element is a matrix block whose slots hold owned
                    // row blocks: sweep TWO levels via `__drop_list_matrix` (each element
                    // through `__drop_matrix`, then the list). A flat `rc_dec` would leak
                    // every matrix AND its rows.
                    *need_matrix = true;
                    *need_list_matrix = true;
                    frees.push_str(&format!(
                        "    let f{i}: List[Matrix] = prim.load_handle(h + {off})\n    __drop_list_matrix(f{i})\n"
                    ));
                } else if matches!(&a[0],
                    Ty::Applied(TypeConstructorId::List, b) if b.len() == 1 && !is_heap_ty(&b[0]))
                {
                    // A matrix-shaped STRUCTURAL field (`List[List[scalar]]`): its slots hold
                    // owned flat row blocks — `__drop_matrix`'s per-row `rc_dec` sweep is its
                    // exact free (a flat `rc_dec` frees only the outer block, leaking rows).
                    *need_matrix = true;
                    frees.push_str(&format!(
                        "    let f{i}: Matrix = prim.load_handle(h + {off})\n    __drop_matrix(f{i})\n"
                    ));
                } else if matches!(a[0], Ty::String) || is_flat_variant_elem(&a[0], flat_variant_names) {
                    // `List[String]` OR `List[flat-variant]` (a nullary/scalar-only enum like
                    // `Capability`): each element is a single FLAT block, so `__drop_list_str` frees
                    // them per-element (`rc_dec` of each element handle + the list block). The flat
                    // variant element holds no inner handle, so the byte-identical String-list drop is
                    // its full free — a flat `rc_dec` of just the list block would LEAK each element.
                    *need_list_str = true;
                    frees.push_str(&format!(
                        "    let f{i}: List[String] = prim.load_handle(h + {off})\n    __drop_list_str(f{i})\n"
                    ));
                } else {
                    // List[scalar] or List[non-recursive heap]: flat free the block.
                    frees.push_str(&format!("    prim.rc_dec(prim.load64(h + {off}))\n"));
                }
            }
            // A `Matrix` field (the v1 value model: a List[List[Float]] block whose slots
            // hold owned flat row blocks — nn WhisperWeights.conv1_w): free each row + the
            // block via `__drop_matrix`. The previous flat `rc_dec` fallback leaked every row.
            Ty::Matrix | Ty::Applied(TypeConstructorId::Matrix, _) => {
                *need_matrix = true;
                frees.push_str(&format!(
                    "    let f{i}: Matrix = prim.load_handle(h + {off})\n    __drop_matrix(f{i})\n"
                ));
            }
            Ty::Applied(TypeConstructorId::Map, a)
                if a.len() == 2 && matches!(a[0], Ty::String) && matches!(a[1], Ty::String) =>
            {
                *need_map_ss = true;
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
                if let Some(rn) = recursive_aggregate_name(t, rec_names) {
                    let src = aggregate_source_ty(t);
                    let rn_fn = drop_fn_ident(&rn);
                    frees.push_str(&format!(
                        "    let f{i}: {src} = prim.load_handle(h + {off})\n    __drop_{rn_fn}(f{i})\n"
                    ));
                } else if is_heap_ty(t) {
                    // a non-recursive heap field (scalar-only nested record, Bytes, scalar map) — flat.
                    frees.push_str(&format!("    prim.rc_dec(prim.load64(h + {off}))\n"));
                }
                // a scalar field — skip (no free).
            }
        }
    }
    frees
}

/// Is `ty` a FLAT custom variant (in `flat_variant_names`) — a `List[ty]` element that frees as a
/// single block? `Named`/`UserDefined` only; `List`/`Map`/`Value`/record types never qualify.
fn is_flat_variant_elem(ty: &Ty, flat_variant_names: &std::collections::HashSet<String>) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    let n = match ty {
        Ty::Named(n, _) => n.as_str(),
        Ty::Applied(TypeConstructorId::UserDefined(n), _) => n.as_str(),
        _ => return false,
    };
    flat_variant_names.contains(n)
}

/// If `ty` is `List[V]` where `V` is a RICH (recursive-drop) variant (in `rec_variant_names`), return
/// `V`'s name — the element drop `$__drop_<V>` that `$__drop_list_<V>` calls per element. `None` for a
/// non-list, a scalar/String/flat-variant element list, or a record-element list (those route
/// elsewhere). Used by the variant-ctor field generator AND `record_drop_field_frees` so a `List[Instr]`
/// field (`Global.init`, `Block`'s payload) is freed recursively instead of leaking.
fn list_rich_variant_elem(
    ty: &Ty,
    rec_variant_names: &std::collections::HashSet<String>,
) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    let Ty::Applied(TypeConstructorId::List, a) = ty else { return None };
    if a.len() != 1 {
        return None;
    }
    let n = match &a[0] {
        Ty::Named(n, _) => n.as_str().to_string(),
        Ty::Applied(TypeConstructorId::UserDefined(n), _) => n.clone(),
        _ => return None,
    };
    rec_variant_names.contains(&n).then_some(n)
}

/// The ALMIDE SOURCE TYPE for a recursive-aggregate field (the `let fN: <ty> =` binding type in a
/// drop body). A NAMED aggregate renders to its name; an ANONYMOUS record renders to its structural
/// `{ k: T, … }` form (so a heap-nested anon-record field binds + recurses through `__drop_<hash>`).
fn aggregate_source_ty(ty: &Ty) -> String {
    use almide_lang::types::constructor::TypeConstructorId;
    match ty {
        Ty::Named(n, _) => n.as_str().to_string(),
        Ty::Applied(TypeConstructorId::UserDefined(n), _) => n.clone(),
        Ty::Record { fields } | Ty::OpenRecord { fields } => anon_record_source_ty(fields),
        _ => field_source_ty(ty),
    }
}

/// The ALMIDE SOURCE rendering of an anonymous record TYPE — `{ k0: T0, k1: T1 }` — used as the
/// synthesized `__drop_<hash>` parameter type and a nested anon-record field binding type. Field
/// types render via [`field_source_ty`] (the drop-relevant subset: Bytes/String/Int/.../named
/// records / `List[..]` / `Map[..]` / `Value` / nested anon records).
fn anon_record_source_ty(fields: &[(almide_lang::intern::Sym, Ty)]) -> String {
    let inner: Vec<String> = fields
        .iter()
        .map(|(k, t)| format!("{}: {}", k.as_str(), field_source_ty(t)))
        .collect();
    format!("{{ {} }}", inner.join(", "))
}

/// Render a record FIELD type back to Almide source for a drop binding/param. Total over the field
/// types a recursive-drop record can carry; an unhandled exotic type falls back to `Bytes` (a flat
/// heap block) ONLY as a defensive default — discovery (`anon_record_needs_recursive_drop`) never
/// synthesizes a drop for a shape whose fields it cannot classify, so this fallback is unreachable
/// for the registered shapes.
fn field_source_ty(ty: &Ty) -> String {
    use almide_lang::types::constructor::TypeConstructorId;
    match ty {
        Ty::Int | Ty::Int64 => "Int".to_string(),
        Ty::Int8 => "Int8".to_string(),
        Ty::Int16 => "Int16".to_string(),
        Ty::Int32 => "Int32".to_string(),
        Ty::UInt8 => "UInt8".to_string(),
        Ty::UInt16 => "UInt16".to_string(),
        Ty::UInt32 => "UInt32".to_string(),
        Ty::UInt64 => "UInt64".to_string(),
        Ty::Float | Ty::Float64 => "Float".to_string(),
        Ty::Float32 => "Float32".to_string(),
        Ty::Bool => "Bool".to_string(),
        Ty::String => "String".to_string(),
        Ty::Bytes => "Bytes".to_string(),
        Ty::Named(n, _) => n.as_str().to_string(),
        Ty::Applied(TypeConstructorId::UserDefined(n), _) => n.clone(),
        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => {
            format!("List[{}]", field_source_ty(&a[0]))
        }
        Ty::Applied(TypeConstructorId::Map, a) if a.len() == 2 => {
            format!("Map[{}, {}]", field_source_ty(&a[0]), field_source_ty(&a[1]))
        }
        t if is_value_ty(t) => "Value".to_string(),
        Ty::Record { fields } | Ty::OpenRecord { fields } => anon_record_source_ty(fields),
        Ty::Tuple(elems) => {
            let inner: Vec<String> = elems.iter().map(field_source_ty).collect();
            format!("({})", inner.join(", "))
        }
        // Defensive: a shape the synthesizer never registers (see doc). Bytes = a flat heap block.
        _ => "Bytes".to_string(),
    }
}

/// Walk the IR (every function's signature + body-expr types, every type decl's record fields) and
/// COLLECT the distinct ANONYMOUS record shapes that need a synthesized recursive drop — the input
/// to [`generate_record_drop_sources`]'s anon-drop loop. A shape qualifies iff at least one field
/// frees NON-flat (the flat one-level mask would leak — `anon_record_needs_recursive_drop`), where a
/// NAMED field record's recursiveness is resolved through the program's `rec_names`. Deduped by the
/// content-hash drop name. This is the discovery half; the generation half is the anon loop in
/// `generate_record_drop_sources`.
pub fn collect_recursive_anon_records(
    program: &almide_ir::IrProgram,
) -> Vec<Vec<(almide_lang::intern::Sym, Ty)>> {
    let mut all_decls: Vec<almide_ir::IrTypeDecl> = program.type_decls.clone();
    // A visitor that inspects every expression's type (every IrExpr carries its `ty`), collecting
    // the distinct anon record shapes that need a synthesized recursive drop (deduped by drop name).
    // RECURSES into a qualifying anon record's own anon-record FIELDS so a nested anon shape
    // (`{ st: { iv: Bytes } }`) gets its inner `__drop_anonrec_<hash>` generated too (the outer drop
    // body `let f: { iv: Bytes } = …; __drop_anonrec_<inner>(f)` would otherwise call a missing fn).
    struct TyCollector {
        seen: std::collections::HashSet<String>,
        out: Vec<Vec<(almide_lang::intern::Sym, Ty)>>,
    }
    impl TyCollector {
        fn consider(&mut self, ty: &Ty) {
            use almide_lang::types::constructor::TypeConstructorId;
            match ty {
                Ty::Record { fields } if anon_record_needs_recursive_drop(fields) => {
                    let name = anon_record_drop_name(fields);
                    if self.seen.insert(name) {
                        self.out.push(fields.clone());
                    }
                    // Recurse into field types so a nested anon record / a `List[anon]` element
                    // also registers its drop.
                    for (_, fty) in fields {
                        self.consider(fty);
                    }
                }
                Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => self.consider(&a[0]),
                Ty::Tuple(elems) => {
                    for e in elems {
                        self.consider(e);
                    }
                }
                _ => {}
            }
        }
    }
    impl almide_ir::visit::IrVisitor for TyCollector {
        fn visit_expr(&mut self, expr: &almide_ir::IrExpr) {
            self.consider(&expr.ty);
            almide_ir::visit::walk_expr(self, expr);
        }
    }

    let mut collector = TyCollector { seen: std::collections::HashSet::new(), out: Vec::new() };
    let funcs = program
        .functions
        .iter()
        .chain(program.modules.iter().flat_map(|m| m.functions.iter()));
    for f in funcs {
        collector.consider(&f.ret_ty);
        let param_tys: Vec<Ty> = f.params.iter().map(|p| p.ty.clone()).collect();
        for ty in &param_tys {
            collector.consider(ty);
        }
        almide_ir::visit::IrVisitor::visit_expr(&mut collector, &f.body);
    }
    collector.out
}

/// Does the program reference the `Result[Option[String], String]` shape anywhere (a function
/// signature or an expression type)? Gates `$__drop_opt_str` emission in
/// [`generate_record_drop_sources`] — the recursive-drop leaf `try_lower_result_option_scalar_str_ctor`
/// routes an `ok(some(<string>))` / `ok(none)` `Result[Option[String], String]` through
/// (`resrec:opt_str`). Only that shape needs the generated fn; a scalar Option leaf frees flat. Scans
/// the SAME positions as [`collect_recursive_anon_records`] (ret/param/body-expr types).
pub fn program_uses_result_option_str(program: &almide_ir::IrProgram) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    fn is_result_opt_str(ty: &Ty) -> bool {
        let Ty::Applied(TypeConstructorId::Result, a) = ty else { return false };
        if a.len() != 2 || !matches!(a[1], Ty::String) {
            return false;
        }
        matches!(&a[0], Ty::Applied(TypeConstructorId::Option, oa)
            if oa.len() == 1 && matches!(oa[0], Ty::String))
    }
    struct Finder {
        found: bool,
    }
    impl almide_ir::visit::IrVisitor for Finder {
        fn visit_expr(&mut self, expr: &almide_ir::IrExpr) {
            if is_result_opt_str(&expr.ty) {
                self.found = true;
            }
            almide_ir::visit::walk_expr(self, expr);
        }
    }
    let mut finder = Finder { found: false };
    let funcs = program
        .functions
        .iter()
        .chain(program.modules.iter().flat_map(|m| m.functions.iter()));
    for f in funcs {
        if is_result_opt_str(&f.ret_ty) || f.params.iter().any(|p| is_result_opt_str(&p.ty)) {
            return true;
        }
        almide_ir::visit::IrVisitor::visit_expr(&mut finder, &f.body);
        if finder.found {
            return true;
        }
    }
    false
}

/// Does the program create or carry FIRST-CLASS FUNCTION values (a `Lambda` expr or a
/// `Ty::Fn`-typed value anywhere)? Gates the injection of [`CLOSURE_DROP_SRC`] — a program
/// with no closures pays neither the second lowering pass nor the dead drop routine.
pub fn program_uses_closures(program: &almide_ir::IrProgram) -> bool {
    struct Finder {
        found: bool,
    }
    impl almide_ir::visit::IrVisitor for Finder {
        fn visit_expr(&mut self, expr: &almide_ir::IrExpr) {
            if matches!(expr.kind, almide_ir::IrExprKind::Lambda { .. })
                || matches!(expr.ty, Ty::Fn { .. })
            {
                self.found = true;
            }
            if !self.found {
                almide_ir::visit::walk_expr(self, expr);
            }
        }
    }
    let mut finder = Finder { found: false };
    let funcs = program
        .functions
        .iter()
        .chain(program.modules.iter().flat_map(|m| m.functions.iter()));
    for f in funcs {
        if matches!(f.ret_ty, Ty::Fn { .. }) || f.params.iter().any(|p| matches!(p.ty, Ty::Fn { .. }))
        {
            return true;
        }
        almide_ir::visit::IrVisitor::visit_expr(&mut finder, &f.body);
        if finder.found {
            return true;
        }
    }
    false
}

/// The ALMIDE SOURCE of the UNIFORM closure-block release `$__drop_closure` (the closures
/// machinery — injected by the render pipeline whenever the program carries first-class
/// function values). A closure block is SELF-DESCRIBING: slot 0 = fnidx (a table index —
/// NEVER dereferenced here), slot 1 = n_heap | (n_closure << 16), slots 2.. = captured
/// closures (freed by RECURSING into this very routine — the `compose` shape), then
/// captured heap values (each freed by ONE `rc_dec` — the lowering's capture gate admits
/// only one-level-exact kinds), then scalars (untouched). Any drop site can free any
/// closure value without knowing its captures (a call-result closure's layout is
/// unknowable at the caller). Like every generated `$__drop_*`, a trusted prim-only
/// routine (outside the witness surface), pinned by the closure leak-loop test.
pub const CLOSURE_DROP_SRC: &str = "\
fn __drop_closure(c: List[Int]) -> Unit = {
  let h = prim.handle(c)
  if prim.load32(h + 0) == 1 then {
    let hdr = prim.load64(h + 20)
    let nc = hdr / 65536
    let nh = hdr - nc * 65536
    __drop_closure_loop(h, nc, nh, 0)
  } else ()
  prim.rc_dec(h)
}
fn __drop_closure_loop(h: Int, nc: Int, nh: Int, i: Int) -> Unit =
  if i >= nc + nh then ()
  else {
    if i < nc then {
      let q: List[Int] = prim.load_handle(h + 28 + i * 8)
      __drop_closure(q)
    } else {
      prim.rc_dec(prim.load64(h + 28 + i * 8))
    }
    __drop_closure_loop(h, nc, nh, i + 1)
  }
";

/// Generate the ALMIDE SOURCE for each RECORD type's recursive drop `$__drop_<R>` (the records
/// counterpart of [`generate_variant_drop_sources`]). Records have NO tag — fields sit at
/// `slot_offset(i)`, freed per CONCRETE field type: `String → rc_dec`, `Map[String,String] →
/// __drop_map_ss`, `List[String] → __drop_list_str`, `List[<recursive record>] → __drop_list_<R>`,
/// a recursive record → `__drop_<R>`, a `Value → __drop_value`, a scalar-only nested aggregate or
/// `List[scalar]` → flat `rc_dec` of the block, a scalar → skip. Emits the needed `__drop_list_<R>`
/// loops + the generic `__drop_map_ss` / `__drop_list_str` helpers. Also emits a synthesized
/// `__drop_anonrec_<hash>` for each ANONYMOUS record shape in `anon_records` that needs the
/// recursive drop (a heap-nested anon record return — aes cfb8). All `__drop_`-prefixed ⇒ on the
/// `prim.rc_dec` whitelist + an empty ownership cert (a trusted free, leak-loop verified).
pub fn generate_record_drop_sources(
    type_decls: &[almide_ir::IrTypeDecl],
    anon_records: &[Vec<(almide_lang::intern::Sym, Ty)>],
    uses_result_opt_str: bool,
) -> String {
    use almide_ir::IrTypeDeclKind;
    let rec_names = recursive_record_drop_names(type_decls);
    let flat_variant_names = flat_variant_type_names(type_decls);
    // The RICH variant names — a record `List[<rich variant>]` field (`Global.init: List[Instr]`) routes
    // to `$__drop_list_<V>` (generated by `generate_variant_drop_sources`, appended to the same program).
    let variant_names = variant_type_names(type_decls);
    let all_record_names: std::collections::HashSet<String> = type_decls
        .iter()
        .filter(|d| matches!(&d.kind, IrTypeDeclKind::Record { .. }))
        .map(|d| d.name.as_str().to_string())
        .collect();
    let rec_variant_names: std::collections::HashSet<String> = type_decls
        .iter()
        .filter(|d| variant_needs_recursive_drop(d, &variant_names, &all_record_names))
        .map(|d| d.name.as_str().to_string())
        .collect();
    let mut out = String::new();
    let mut list_drops: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut need_map_ss = false;
    let mut need_list_str = false;
    let mut need_matrix = false;
    let mut need_list_matrix = false;
    for decl in type_decls {
        let IrTypeDeclKind::Record { fields } = &decl.kind else { continue };
        if !rec_names.contains(decl.name.as_str()) {
            continue;
        }
        let tname = decl.name.as_str();
        let fname = drop_fn_ident(tname);
        let field_tys: Vec<Ty> = fields.iter().map(|f| f.ty.clone()).collect();
        out.push_str(&format!("fn __drop_{fname}(e: {tname}) -> Unit = {{\n"));
        out.push_str("  let h = prim.handle(e)\n");
        out.push_str("  if prim.load32(h + 0) == 1 then {\n");
        out.push_str(&record_drop_field_frees(
            &field_tys,
            &rec_names,
            &flat_variant_names,
            &rec_variant_names,
            &mut list_drops,
            &mut need_map_ss,
            &mut need_list_str,
            &mut need_matrix,
            &mut need_list_matrix,
        ));
        out.push_str("  } else ()\n");
        out.push_str("  prim.rc_dec(h)\n");
        out.push_str("}\n");
    }
    // `$__drop_opt_<R>` for each recursive-drop record R — frees an `Option[R]` (the 0-or-1-element
    // layout) used by `Result[Option[R], String]` wrappers (`resrec:opt_<R>`, porta read_message's
    // `ok(none)` / `ok(r)` bases). The `match` drops the bound record `r` at the Some-arm end (routing
    // to `$__drop_<R>`); a None is a no-op; consuming `e` frees the Option block. Same per-R set as the
    // `$__drop_<R>` loop above, so an `$__drop_opt_<R>` is emitted only when its `$__drop_<R>` exists.
    for decl in type_decls {
        let IrTypeDeclKind::Record { .. } = &decl.kind else { continue };
        if !rec_names.contains(decl.name.as_str()) {
            continue;
        }
        let tname = decl.name.as_str();
        out.push_str(&format!(
            "fn __drop_opt_{tname}(e: Option[{tname}]) -> Unit = {{\n  match e {{\n    some(r) => (),\n    none => (),\n  }}\n}}\n"
        ));
    }
    // `$__drop_opt_str` — frees an `Option[String]` (the recursive-drop leaf of a `Result[Option[String],
    // String]`, the derived-Codec `__decode_option_string`). The `some(r)` arm binds the inner String
    // whose scope-end `rc_dec` frees it; consuming `e` frees the 0-or-1 Option block. Emitted ONLY when
    // the program constructs that shape (via `try_lower_result_option_scalar_str_ctor`'s `resrec:opt_str`),
    // so a program without it is not perturbed. (The scalar Option leaves — Int/Float/Bool — need no drop
    // fn: their `Result[Option[<scalar>], String]` frees flat via `DropListStr`.)
    if uses_result_opt_str {
        out.push_str(
            "fn __drop_opt_str(e: Option[String]) -> Unit = {\n  match e {\n    some(r) => (),\n    none => (),\n  }\n}\n",
        );
    }
    // `$__drop_tup_int_<R>` for each recursive-drop record R — frees a `(R, Int)` TUPLE
    // block (record handle @12 recursed via `$__drop_<R>`, the Int @20 is scalar), used
    // by `Result[(R, Int), String]` wrappers (`resrec:tup_int_<R>` — the gguf
    // parse_header `ok((GGUFHeader {…}, 24))` shape).
    for decl in type_decls {
        let IrTypeDeclKind::Record { .. } = &decl.kind else { continue };
        if !rec_names.contains(decl.name.as_str()) {
            continue;
        }
        let tname = decl.name.as_str();
        let fname = drop_fn_ident(tname);
        out.push_str(&format!(
            "fn __drop_tup_int_{fname}(e: ({tname}, Int)) -> Unit = {{
                 let h = prim.handle(e)
                 if prim.load32(h + 0) == 1 then {{
                     let r: {tname} = prim.load_handle(h + 12)
                     __drop_{fname}(r)
                 }} else ()
                 prim.rc_dec(h)
}}
"
        ));
    }
    // SYNTHESIZED recursive drops for the ANONYMOUS record return/binding shapes the corpus uses
    // (`{ data: Bytes, state: Cfb8State }` — aes cfb8). An anon record is NOT a `type` decl, so the
    // loop above never names it; it would otherwise drop via the flat one-level mask `DropListStr`,
    // which `rc_dec`s the `state` BLOCK but LEAKS the Bytes INSIDE Cfb8State. Each shape gets a
    // content-hashed `__drop_anonrec_<hash>` (dedup'd) with the SAME per-field-type recursion the
    // named generator emits — so the `state` field is freed through `__drop_Cfb8State`. The param is
    // the structural anon record type in source (`e: { data: Bytes, state: Cfb8State }`). Sorted by
    // name for host-determinism. (The discovery of WHICH anon shapes appear is the caller's; see
    // `generate_anon_record_drop_sources`.)
    let mut anon_sorted: Vec<&Vec<(almide_lang::intern::Sym, Ty)>> = anon_records.iter().collect();
    anon_sorted.sort_by_key(|fields| anon_record_drop_name(fields));
    anon_sorted.dedup_by_key(|fields| anon_record_drop_name(fields));
    for fields in anon_sorted {
        if !anon_record_needs_recursive_drop(fields) {
            continue;
        }
        let name = anon_record_drop_name(fields);
        let field_tys: Vec<Ty> = fields.iter().map(|(_, t)| t.clone()).collect();
        let param_ty = anon_record_source_ty(fields);
        out.push_str(&format!("fn __drop_{name}(e: {param_ty}) -> Unit = {{\n"));
        out.push_str("  let h = prim.handle(e)\n");
        out.push_str("  if prim.load32(h + 0) == 1 then {\n");
        out.push_str(&record_drop_field_frees(
            &field_tys,
            &rec_names,
            &flat_variant_names,
            &rec_variant_names,
            &mut list_drops,
            &mut need_map_ss,
            &mut need_list_str,
            &mut need_matrix,
            &mut need_list_matrix,
        ));
        out.push_str("  } else ()\n");
        out.push_str("  prim.rc_dec(h)\n");
        out.push_str("}\n");
    }
    // The SAME per-element list wrapper for each synthesized ANON-record drop — a
    // STRUCTURAL record-list literal (`take([{key: "x", val: "2"}])`, the checker
    // leaves the elements structural) routes to `list_anonrec_<hash>`; without this
    // wrapper the route referenced a missing `$__drop_list_anonrec_<hash>`.
    {
        let mut anon_sorted: Vec<&Vec<(almide_lang::intern::Sym, Ty)>> =
            anon_records.iter().collect();
        anon_sorted.sort_by_key(|fields| anon_record_drop_name(fields));
        anon_sorted.dedup_by_key(|fields| anon_record_drop_name(fields));
        for fields in anon_sorted {
            if !anon_record_needs_recursive_drop(fields) {
                continue;
            }
            let name = anon_record_drop_name(fields);
            let param_ty = anon_record_source_ty(fields);
            out.push_str(&format!(
                "fn __drop_list_{name}(xs: List[{param_ty}]) -> Unit = {{
                     let h = prim.handle(xs)
                     if prim.load32(h + 0) == 1 then __drop_list_{name}_loop(h, prim.load32(h + 4), 0) else ()
                     prim.rc_dec(h)
}}
                 fn __drop_list_{name}_loop(h: Int, n: Int, i: Int) -> Unit =
                     if i >= n then ()
                     else {{ let e: {param_ty} = prim.load_handle(h + 12 + i * 8)
         __drop_{name}(e)
         __drop_list_{name}_loop(h, n, i + 1) }}
"
            ));
        }
    }
    // A per-element-recursive `$__drop_list_<R>` for EVERY recursive-drop record R (not just the
    // field-referenced ones in `list_drops`) — so a standalone `List[R]` LITERAL value (`group([…])`)
    // routes its drop here too. Sorted for host-determinism.
    let _ = &list_drops; // (subsumed by rec_names below)
    let mut list_drop_names: Vec<&String> = rec_names.iter().collect();
    list_drop_names.sort();
    for rn in list_drop_names {
        // fn NAMES sanitize the module prefix; the `List[{rn}]` / `e: {rn}` type annotations keep
        // the dotted module-qualified name (a valid Almide type reference).
        let rn_fn = drop_fn_ident(rn);
        out.push_str(&format!(
            "fn __drop_list_{rn_fn}(xs: List[{rn}]) -> Unit = {{\n  \
               let h = prim.handle(xs)\n  \
               if prim.load32(h + 0) == 1 then __drop_list_{rn_fn}_loop(h, prim.load32(h + 4), 0) else ()\n  \
               prim.rc_dec(h)\n}}\n\
             fn __drop_list_{rn_fn}_loop(h: Int, n: Int, i: Int) -> Unit =\n  \
               if i >= n then ()\n  \
               else {{ let e: {rn} = prim.load_handle(h + 12 + i * 8)\n         __drop_{rn_fn}(e)\n         __drop_list_{rn_fn}_loop(h, n, i + 1) }}\n"
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
    if need_matrix {
        // The v1 Matrix free: at the block's last ref, `rc_dec` each owned flat row
        // (slot i64-widened handles @12 + i*8, count @4), then the block — the
        // `__drop_list_str` sweep typed over Matrix.
        out.push_str(
            "fn __drop_matrix(m: Matrix) -> Unit = {\n  \
               let h = prim.handle(m)\n  \
               if prim.load32(h + 0) == 1 then __drop_matrix_loop(h, prim.load32(h + 4), 0) else ()\n  \
               prim.rc_dec(h)\n}\n\
             fn __drop_matrix_loop(h: Int, n: Int, i: Int) -> Unit =\n  \
               if i >= n then ()\n  \
               else { prim.rc_dec(prim.load64(h + 12 + i * 8))\n         __drop_matrix_loop(h, n, i + 1) }\n",
        );
    }
    if need_list_matrix {
        // A `List[Matrix]` field: each element recurses through `__drop_matrix`, then
        // the list block — the two-level sweep `DropListListStr` performs for values.
        out.push_str(
            "fn __drop_list_matrix(xs: List[Matrix]) -> Unit = {\n  \
               let h = prim.handle(xs)\n  \
               if prim.load32(h + 0) == 1 then __drop_list_matrix_loop(h, prim.load32(h + 4), 0) else ()\n  \
               prim.rc_dec(h)\n}\n\
             fn __drop_list_matrix_loop(h: Int, n: Int, i: Int) -> Unit =\n  \
               if i >= n then ()\n  \
               else { let e: Matrix = prim.load_handle(h + 12 + i * 8)\n         __drop_matrix(e)\n         __drop_list_matrix_loop(h, n, i + 1) }\n",
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
    // A body-less `@extern(wasm, module, name)` function lowers to a thin host-IMPORT
    // call (the browser dom/fetch/timer/console stubs) — its behavior IS the host's, so
    // it CALLS the import, never fabricates a value. Gated STRICTLY on target == "wasm"
    // (a `rust`/`rs` extern has no wasm host → `None` → it keeps walling as before).
    if let Some(import_fn) = try_lower_extern_wasm(func)? {
        return Ok(vec![import_fn]);
    }
    let mut ctx = LowerCtx {
        globals: globals.clone(),
        global_inits: global_inits.clone(),
        fn_name: func.name.as_str().to_string(),
        record_layouts: record_layouts.clone(),
        variant_layouts: variant_layouts.clone(),
        // An EXPLICIT `Result`/`Option` declared return is a REAL heap value the caller inspects
        // (e.g. `fs.write -> Result[Unit, String]`), so a `Result[Unit, _]` tail must NOT be voided
        // — see `LowerCtx::decl_ret_is_result`. A declared-`Unit` effect fn (the synthetic Result)
        // keeps the void convention.
        decl_ret_is_result: matches!(
            &func.ret_ty,
            Ty::Applied(
                almide_lang::types::constructor::TypeConstructorId::Result
                    | almide_lang::types::constructor::TypeConstructorId::Option,
                _
            )
        ),
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
    crate::lower::dump_desugared_ir(func.name.as_str(), &func.body);
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
            crate::Capability::FsWrite,
            crate::Capability::Stdin,
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

// The `??`-operand admission gate (a free fn in the private `control` module) — re-exported so the
// `classify_corpus` caps counter consults the SAME predicate the lowering uses (no count drift).
pub use control::unwrap_or_operand_admitted;


#[cfg(test)]
mod tests;

include!("mod_p2.rs");
include!("mod_p3.rs");
include!("mod_p4.rs");
include!("mod_p5.rs");
include!("mod_p6.rs");
