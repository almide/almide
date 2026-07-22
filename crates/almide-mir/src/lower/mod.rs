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

/// A FLAT scalar-slot heap block: an all-scalar tuple (`(Int, Int)`) or a
/// `List[<scalar>]` (`List[Int]`) — every slot in the block is a raw i64 value,
/// never a nested handle. Mirrors the `ListElemDrop::ScalarAggregate` gate in
/// `binds_p3.rs`. This is the exact shape B32's `__uh_eq` (list_hshare.almd)
/// compares correctly (length + raw-slot equality) — a String or any OTHER heap
/// element (record, nested heap list, Value) is NOT this shape, and must not be
/// routed to `__uh_eq`-based comparison (nor to the byte-level `__str_eq` String
/// family — the source of a CONFIRMED silent wrong-bytes bug when a tuple/nested-
/// list element was routed there: `__str_eq` misreads a slot-count `len` as a
/// BYTE count, comparing only the object's first `len` bytes — a false-positive
/// collision past the first ~2 bytes for any two elements sharing a leading Int).
pub fn is_flat_scalar_block_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    match ty {
        Ty::Tuple(tys) => !tys.is_empty() && tys.iter().all(|t| !is_heap_ty(t)),
        Ty::Applied(TypeConstructorId::List, b) => b.len() == 1 && !is_heap_ty(&b[0]),
        // An `Option[<scalar>]` is the SAME flat physics: len-as-tag (0 = none)
        // + one raw scalar slot — rc_dec is its full free and a slot-wise
        // content compare is exact (the C-149 nested-Option class).
        Ty::Applied(TypeConstructorId::Option, b) => b.len() == 1 && !is_heap_ty(&b[0]),
        _ => false,
    }
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

/// The i64-uniform bit pattern of a float literal: a `Float32`-typed literal carries the
/// LOW-32 f32 pattern (the F32Demote/IntToF32 convention — see `PrimKind::F32Bin`),
/// everything else the f64 bits. Emitting f64 bits for a Float32 made every downstream
/// f32-family op (arith, compare, to_string) read garbage.
pub(crate) fn float_lit_bits(value: f64, ty: &Ty) -> i64 {
    if matches!(ty, Ty::Float32) {
        (value as f32).to_bits() as i64
    } else {
        value.to_bits() as i64
    }
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
        // `string.from_codepoint(<int literal>)` (`let NL = string.from_codepoint(10)` —
        // the stringify-escape test globals) CONST-FOLDS to its one-char string at
        // lowering time: zero calls injected, so the count gate stays exact. An invalid
        // codepoint keeps walling (never a wrong byte).
        IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
            if module.as_str() == "string"
                && func.as_str() == "from_codepoint"
                && args.len() == 1 =>
        {
            let IrExprKind::LitInt { value } = &args[0].kind else { return None };
            u32::try_from(*value)
                .ok()
                .and_then(char::from_u32)
                .map(|c| crate::Init::Str(c.to_string()))
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
        // A RawPtr is a RAW linear-memory ADDRESS carried in the uniform i64 scalar
        // slot (the same value `prim.handle` yields; on wasm it is an i32 offset the
        // consuming prim wraps). The bytes_rawptr bridge (#440) reads/writes THROUGH
        // it via the self-hosted prim loops — never a tracked heap handle.
        Ty::RawPtr => ScalarWidth::Double,
        // Unit/Never/Const* are not values that get a scalar slot here.
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

/// A sized-int WIDENING conversion call (`int8.to_int64(x)`, `uint32.to_int64(x)`, …)
/// whose runtime is the IDENTITY on the canonical-i64 slot value: every integer width
/// lives sign-/zero-extended in one i64 (the `Ty` docs + `extern_wasm_abi` pin this),
/// and the Rust runtime is `n as i64` over that already-canonical value (`u64 as i64`
/// is the same bit-reinterpret the slot already holds). Returns the operand expr when
/// the shape applies — the lowering forwards the operand's value with NO call, and
/// `count_ir_calls` skips the node by the SAME predicate (mir == ir by construction).
pub fn identity_int_widening_call(e: &IrExpr) -> Option<&IrExpr> {
    let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &e.kind
    else {
        return None;
    };
    if args.len() != 1 || func.as_str() != "to_int64" {
        return None;
    }
    if !matches!(
        module.as_str(),
        "int" | "int8" | "int16" | "int32" | "int64" | "uint8" | "uint16" | "uint32" | "uint64"
    ) {
        return None;
    }
    let arg_int = matches!(
        args[0].ty,
        Ty::Int
            | Ty::Int8
            | Ty::Int16
            | Ty::Int32
            | Ty::Int64
            | Ty::UInt8
            | Ty::UInt16
            | Ty::UInt32
            | Ty::UInt64
    );
    arg_int.then(|| &args[0])
}

/// A `float.from_int(x)` call over an `Int` — the sitofp floor (#806 step 2):
/// the lowering emits ONE `PrimKind::F64FromInt` (a `f64.convert_i64_s` in the
/// render, `as f64` natively) instead of the self-host runtime CALL, and
/// `count_ir_calls` skips the node by this SAME predicate (`mir == ir` by
/// construction). Returns the operand expr when the shape applies.
pub fn float_from_int_prim_call(e: &IrExpr) -> Option<&IrExpr> {
    let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &e.kind
    else {
        return None;
    };
    (module.as_str() == "float"
        && func.as_str() == "from_int"
        && args.len() == 1
        && matches!(args[0].ty, Ty::Int))
    .then(|| &args[0])
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
/// Does `e` contain ANY call node (Named/Module/Method/Computed Call, RuntimeCall,
/// TailCall)? Used to gate synthesized-expr admissions (a default-field fill) whose calls
/// the counted IR would not see (the caps `mir == ir` invariant).
pub fn expr_contains_call(e: &almide_ir::IrExpr) -> bool {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct C(bool);
    impl IrVisitor for C {
        fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
            if matches!(
                e.kind,
                almide_ir::IrExprKind::Call { .. }
                    | almide_ir::IrExprKind::RuntimeCall { .. }
                    | almide_ir::IrExprKind::TailCall { .. }
            ) {
                self.0 = true;
            }
            walk_expr(self, e);
        }
    }
    let mut c = C(false);
    almide_ir::visit::IrVisitor::visit_expr(&mut c, e);
    c.0
}

/// CROSS-MODULE top-let NAME BRIDGE: the main program references `toplib.SYSTEM` through a
/// MAIN-side VarId (per-module VarId regions — no IR-level flatten), while the globals union
/// keys the MODULE-side id — so the reference was "unbound" (or COLLIDED with an unrelated
/// module id, resolving to the wrong init). Alias every main-side var-table id whose NAME +
/// TYPE match a module top-let's onto that top-let's (ty, init). By-NAME, so an AMBIGUOUS
/// name (a top-let in two modules) is skipped — those references stay walled; a same-named
/// function LOCAL is harmless (locals resolve through `value_of` first — the globals map is
/// only consulted for otherwise-unbound ids). Registration only: the reference site still
/// materializes through the CONST-init machinery (count-exact, unchanged certs).
pub fn bridge_cross_module_toplets(
    ir: &almide_ir::IrProgram,
    globals: &mut std::collections::HashMap<almide_ir::VarId, Ty>,
    global_inits: &mut std::collections::HashMap<almide_ir::VarId, almide_ir::IrExpr>,
    // #782: main-side synthesized ref VarId → module-side MUTABLE var VarId.
    // With the v0 fallback retired, a mutable cross-module reference must LOWER
    // instead of walling: the caller aliases the main-side id onto the module
    // var's linear-memory slot, so reads and assigns route through the SAME
    // storage the owning module's fns use (no const-fold hazard — the slot is
    // real storage, not an init alias).
    mutable_aliases: &mut std::collections::HashMap<almide_ir::VarId, almide_ir::VarId>,
) {
    // Sequential-phase split (codopsy8 complexity sweep): phase 1 builds the by-name/
    // by-bare lookup maps (self-contained — never touches globals/global_inits/
    // mutable_aliases); phase 2 reads those FINISHED, read-only maps to populate the 3
    // output maps. Pure text-move, no logic change.
    let (by_name, by_bare) = bridge_cross_module_toplets_build_lookup(ir);
    bridge_cross_module_toplets_apply(ir, &by_name, &by_bare, globals, global_inits, mutable_aliases);
}

/// Extracted from `bridge_cross_module_toplets` (codopsy8 complexity sweep, phase 1 of
/// 2): the by-name/by-bare lookup maps of every module top-let. Verbatim.
///
/// The main-side reference entry is SYNTHESIZED by the frontend with an UPPERCASED
/// name (`m.count` → a main var named "COUNT", `module_origin` set — the v0 Rust-const
/// naming convention, expressions.rs's cross-module top-let path). So the bridge keys
/// BOTH maps by the UPPERCASED module-side name: an all-caps `let SYSTEM` matched
/// before by accident; a lowercase `let title`/`var count` silently MISSED the bridge
/// and fell through to the raw numeric-id collision below (reading an UNRELATED
/// top-let's init — a confirmed silent wrong value, `let N = 7; var count = 0` printed
/// 7 for `m.count`; a heap-typed collider surfaced as invalid i64/i32 wasm instead).
/// MUTABILITY: only immutable `let`s are bridged — aliasing a `var` reference to its
/// INIT would const-fold reads across mutations (read-after-`bump()` returning 0).
/// A `var` reference instead has its collided raw entry REMOVED below, so it is
/// honestly UNBOUND → the reference site walls → `--verified` falls back to v0.
/// Keyed by (SOURCE MODULE, UPPERCASED NAME): the ref entry's `module_origin`
/// names which module it points at, so a name defined in TWO modules (view.ROW
/// and layout.ROW — the ceangal zip class) resolves per-module instead of
/// dropping as ambiguous. A bare-name fallback map keeps the pre-existing
/// behavior for refs whose module_origin the frontend left unset.
#[allow(clippy::type_complexity)]
fn bridge_cross_module_toplets_build_lookup(
    ir: &almide_ir::IrProgram,
) -> (
    std::collections::HashMap<(String, String), Option<(Ty, &almide_ir::IrExpr, bool, almide_ir::VarId)>>,
    std::collections::HashMap<String, Option<(Ty, &almide_ir::IrExpr, bool, almide_ir::VarId)>>,
) {
    use std::collections::HashMap;
    let mut by_name: HashMap<(String, String), Option<(Ty, &almide_ir::IrExpr, bool, almide_ir::VarId)>> =
        HashMap::new();
    let mut by_bare: HashMap<String, Option<(Ty, &almide_ir::IrExpr, bool, almide_ir::VarId)>> = HashMap::new();
    for m in &ir.modules {
        // In-module alias chains (`let white = _white`) leave the alias tl's ty
        // UN-INFERRED — chase to the referent so the bridge carries the REAL
        // (ty, init) and the reader materializes the record directly (the ceangal
        // theme `v.white` class). Bounded hops; a non-Var / cross-module init stops.
        let local: HashMap<u32, (&Ty, &almide_ir::IrExpr)> =
            m.top_lets.iter().map(|t| (t.var.0, (&t.ty, &t.value))).collect();
        for tl in &m.top_lets {
            let Some(info) = m.var_table.entries.get(tl.var.0 as usize) else { continue };
            let mutable = matches!(info.mutability, almide_ir::Mutability::Var);
            let (mut ty, mut init) = (&tl.ty, &tl.value);
            let mut hops = 0;
            // Chase Var inits REGARDLESS of the alias's own ty — the init expr is
            // about to cross regions, and any surviving REGION-LOCAL Var id inside
            // it would capture an unrelated main-side id (a silent wrong-global
            // read when that id's init is const; probe-confirmed as VarId(7)).
            while hops < 4 {
                let almide_ir::IrExprKind::Var { id } = &init.kind else { break };
                let Some((t2, i2)) = local.get(&id.0) else { break };
                if matches!(ty, Ty::Unknown) {
                    ty = t2;
                }
                init = i2;
                hops += 1;
            }
            // An UNANNOTATED module top-let leaves tl.ty Unknown even after the
            // alias chase — the INIT expression's checker-inferred ty is the
            // referent's real type (`let _white = { r: 1.0, … }` infers the record).
            if matches!(ty, Ty::Unknown) && !matches!(init.ty, Ty::Unknown) {
                ty = &init.ty;
            }
            // An OPTION-ctor init whose OWN node ty is also un-inferred (`let MAYBE =
            // some(Cfg { .. })` — the crossmod option_record_toplet): synthesize
            // `Option[payload.ty]` from the payload's inferred type.
            let refined_opt;
            if let Some(r) = refine_option_toplet_ty(ty, init) {
                refined_opt = r;
                ty = &refined_opt;
            }
            // A chased init that STILL references region-local vars (a call init
            // over a sibling const, a nested alias past the hop bound) must NOT
            // cross: the ids would misresolve in the main region. Drop the name
            // (honest unbound wall) rather than ship a capturing expr.
            fn expr_has_var(e: &almide_ir::IrExpr) -> bool {
                use almide_ir::visit::{walk_expr, IrVisitor};
                struct V(bool);
                impl IrVisitor for V {
                    fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
                        if matches!(e.kind, almide_ir::IrExprKind::Var { .. }) {
                            self.0 = true;
                        }
                        walk_expr(self, e);
                    }
                }
                let mut v = V(false);
                v.visit_expr(e);
                v.0
            }
            let entry = if !mutable && expr_has_var(init) {
                Option::None
            } else {
                Some((ty.clone(), init, mutable, tl.var))
            };
            by_name
                .entry((m.name.as_str().to_string(), info.name.as_str().to_uppercase()))
                .and_modify(|e| *e = Option::None) // second definition ⇒ ambiguous, drop
                .or_insert(entry.clone());
            by_bare
                .entry(info.name.as_str().to_uppercase())
                .and_modify(|e| *e = Option::None) // cross-module name collision ⇒ ambiguous
                .or_insert(entry);
        }
    }
    (by_name, by_bare)
}

/// Extracted from `bridge_cross_module_toplets` (codopsy8 complexity sweep, phase 2 of
/// 2): reads the (already-finished, read-only) `by_name`/`by_bare` lookup maps from
/// phase 1 to populate `globals`/`global_inits`/`mutable_aliases`. OVERRIDES an existing
/// (module-raw, possibly colliding) entry — callers order the composition as: module
/// union → this bridge → main top-lets re-inserted last, so the precedence is main >
/// bridged-name > raw module id. Verbatim.
#[allow(clippy::type_complexity)]
fn bridge_cross_module_toplets_apply(
    ir: &almide_ir::IrProgram,
    by_name: &std::collections::HashMap<(String, String), Option<(Ty, &almide_ir::IrExpr, bool, almide_ir::VarId)>>,
    by_bare: &std::collections::HashMap<String, Option<(Ty, &almide_ir::IrExpr, bool, almide_ir::VarId)>>,
    globals: &mut std::collections::HashMap<almide_ir::VarId, Ty>,
    global_inits: &mut std::collections::HashMap<almide_ir::VarId, almide_ir::IrExpr>,
    mutable_aliases: &mut std::collections::HashMap<almide_ir::VarId, almide_ir::VarId>,
) {
    for (i, info) in ir.var_table.entries.iter().enumerate() {
        let id = almide_ir::VarId(i as u32);
        // Only the frontend-synthesized cross-module reference entries participate
        // (module_origin set) — a main-local name that happens to match a module
        // top-let must not be rebound.
        if info.module_origin.is_none() {
            continue;
        }
        let looked_up = info
            .module_origin
            .as_ref()
            .and_then(|mo| by_name.get(&(mo.clone(), info.name.as_str().to_uppercase())))
            .or_else(|| by_bare.get(&info.name.as_str().to_uppercase()));
        match looked_up {
            // An UNKNOWN-typed reference entry (the frontend leaves an alias-let's
            // synthesized ref un-inferred — `let white = _white` read as `v.white`,
            // the ceangal theme class) takes the MODULE side's type: the name is
            // unique (the ambiguity arm below dropped collisions), so the module
            // top-let IS the referent. A concretely-typed ref still must agree.
            Some(Some((ty, init, mutable, _mod_id)))
                if !mutable && bridged_ref_ty_agrees(ty, &info.ty) =>
            {
                globals.insert(id, ty.clone());
                global_inits.insert(id, (*init).clone());
            }
            // #782: a MUTABLE cross-module reference aliases onto the module
            // var's storage slot instead of walling (the v0 fallback that used
            // to absorb it is retired). The init is never shipped — only the
            // slot identity — so the const-fold hazard that justified the old
            // exclusion cannot occur.
            Some(Some((ty, _init, true, mod_id)))
                if bridged_ref_ty_agrees(ty, &info.ty) =>
            {
                mutable_aliases.insert(id, *mod_id);
                globals.remove(&id);
                global_inits.remove(&id);
            }
            _ => {
                // Unmatched reference: purge any raw module-id numeric collision
                // so the reference is honestly unbound (a diagnosed wall), never
                // an unrelated init.
                globals.remove(&id);
                global_inits.remove(&id);
            }
        }
    }
}
include!("mod_b.rs");
include!("mod_c.rs");
