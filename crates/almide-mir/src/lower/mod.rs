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
    use std::collections::HashMap;
    // The main-side reference entry is SYNTHESIZED by the frontend with an UPPERCASED
    // name (`m.count` → a main var named "COUNT", `module_origin` set — the v0 Rust-const
    // naming convention, expressions.rs's cross-module top-let path). So the bridge keys
    // BOTH maps by the UPPERCASED module-side name: an all-caps `let SYSTEM` matched
    // before by accident; a lowercase `let title`/`var count` silently MISSED the bridge
    // and fell through to the raw numeric-id collision below (reading an UNRELATED
    // top-let's init — a confirmed silent wrong value, `let N = 7; var count = 0` printed
    // 7 for `m.count`; a heap-typed collider surfaced as invalid i64/i32 wasm instead).
    // MUTABILITY: only immutable `let`s are bridged — aliasing a `var` reference to its
    // INIT would const-fold reads across mutations (read-after-`bump()` returning 0).
    // A `var` reference instead has its collided raw entry REMOVED below, so it is
    // honestly UNBOUND → the reference site walls → `--verified` falls back to v0.
    // Keyed by (SOURCE MODULE, UPPERCASED NAME): the ref entry's `module_origin`
    // names which module it points at, so a name defined in TWO modules (view.ROW
    // and layout.ROW — the ceangal zip class) resolves per-module instead of
    // dropping as ambiguous. A bare-name fallback map keeps the pre-existing
    // behavior for refs whose module_origin the frontend left unset.
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
    // OVERRIDES an existing (module-raw, possibly colliding) entry — callers order the
    // composition as: module union → this bridge → main top-lets re-inserted last, so the
    // precedence is main > bridged-name > raw module id.
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

/// Does the BRIDGED module-side type agree with the main-side REFERENCE entry's
/// type, treating the reference side's `Unknown` as a wildcard STRUCTURALLY
/// (`Option[Unknown]` agrees with `Option[Cfg]` — the un-inferred synthesized
/// ref vs the refined module truth)? A concrete mismatch still refuses (the
/// honest unbound wall, never a wrong-typed alias).
fn bridged_ref_ty_agrees(bridged: &Ty, reference: &Ty) -> bool {
    match (bridged, reference) {
        (_, Ty::Unknown) => true,
        (Ty::Applied(a, xs), Ty::Applied(b, ys)) if a == b && xs.len() == ys.len() => {
            xs.iter().zip(ys).all(|(x, y)| bridged_ref_ty_agrees(x, y))
        }
        _ => bridged == reference,
    }
}

/// Refine an UNANNOTATED top-let's Unknown(-payload) type from its OPTION-ctor
/// initializer: `let MAYBE = some(Cfg { .. })` leaves the declared ty `Unknown`
/// (or the checker's partial `Option[Unknown]`) while the ctor's PAYLOAD expr
/// carries its real inferred type — `Option[payload.ty]` is the structural
/// truth, never a guess. Any other shape returns `None` (untouched).
pub fn refine_option_toplet_ty(ty: &Ty, init: &almide_ir::IrExpr) -> Option<Ty> {
    use almide_lang::types::constructor::TypeConstructorId;
    let unknown_payload = match ty {
        Ty::Unknown => true,
        Ty::Applied(TypeConstructorId::Option, a)
            if a.len() == 1 && matches!(a[0], Ty::Unknown) =>
        {
            true
        }
        _ => false,
    };
    if !unknown_payload {
        return None;
    }
    if let almide_ir::IrExprKind::OptionSome { expr } = &init.kind {
        if !matches!(expr.ty, Ty::Unknown) {
            return Some(Ty::option(expr.ty.clone()));
        }
    }
    None
}

/// Repair UNKNOWN expression types the frontend leaves on CROSS-MODULE global
/// references (`v.white` — the ref entry's ty is un-inferred, so the whole fn
/// trips the AllTypesConcrete precondition): a `Var` whose id the bridged
/// globals map types gets that type, and a member read off a now-typed
/// STRUCTURAL record object gets its field's type. Types come from the
/// authoritative top-let declaration — never guessed.
pub fn repair_unknown_global_ref_tys(
    func: &mut almide_ir::IrFunction,
    globals: &std::collections::HashMap<almide_ir::VarId, Ty>,
) {
    use almide_ir::{walk_expr_mut, IrMutVisitor};
    struct R<'a> {
        globals: &'a std::collections::HashMap<almide_ir::VarId, Ty>,
    }
    impl IrMutVisitor for R<'_> {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e); // children first — the object types before the member
            match &mut e.kind {
                IrExprKind::Var { id } if matches!(e.ty, Ty::Unknown) => {
                    if let Some(t) = self.globals.get(id) {
                        e.ty = t.clone();
                    }
                }
                IrExprKind::Member { object, field } if matches!(e.ty, Ty::Unknown) => {
                    if let Ty::Record { fields } = &object.ty {
                        if let Some((_, ft)) =
                            fields.iter().find(|(n, _)| n.as_str() == field.as_str())
                        {
                            e.ty = ft.clone();
                        }
                    }
                }
                // The PARENT of a repaired member read stays Unknown too — a BinOp is
                // TYPE-DISPATCHED (AddFloat vs AddInt), so its result type is intrinsic.
                IrExprKind::BinOp { op, .. } if matches!(e.ty, Ty::Unknown) => {
                    if let Some(t) = op.result_ty() {
                        e.ty = t;
                    }
                }
                _ => {}
            }
        }
    }
    let mut r = R { globals };
    r.visit_expr_mut(&mut func.body);
}

pub fn build_variant_layouts(type_decls: &[almide_ir::IrTypeDecl]) -> VariantLayouts {
    use almide_ir::{IrTypeDeclKind, IrVariantKind};
    let mut out = VariantLayouts::default();
    for decl in type_decls {
        // A plain RECORD's field defaults ride the same map, keyed by the record TYPE
        // name (`AllDefault()` — the paren-empty ctor fills them in
        // try_lower_record_construct; a variant record-ctor keys by CTOR name below).
        if let IrTypeDeclKind::Record { fields } = &decl.kind {
            for f in fields {
                if let Some(d) = &f.default {
                    out.ctor_field_defaults
                        .entry(decl.name.as_str().to_string())
                        .or_default()
                        .insert(f.name.as_str().to_string(), d.clone());
                }
            }
            continue;
        }
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
                    for f in fields {
                        if let Some(d) = &f.default {
                            out.ctor_field_defaults
                                .entry(case.name.as_str().to_string())
                                .or_default()
                                .insert(f.name.as_str().to_string(), d.clone());
                        }
                    }
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
    // (rc_dec), a List[scalar] (flat rc_dec), an Option[scalar] (flat rc_dec — the 0-or-1 block
    // owns no children), a List[<variant>] (per-element), a List[String] (per-element via the
    // generic `__drop_list_str` — each element is an OWNED String handle a flat rc_dec of just
    // the list block would leak), or a RECORD (recurse via `$__drop_<R>` / a scalar-only record's
    // flat rc_dec — see the drop generator's field loop).
    let supported_heap = |t: &Ty| -> bool {
        use almide_lang::types::constructor::TypeConstructorId;
        variant_field_name(t, variant_names).is_some()
            || matches!(t, Ty::Named(n, _) if record_names.contains(n.as_str()))
            || matches!(t, Ty::String)
            // A CLOSURE field: the generator's Fn arm frees the self-describing
            // closure block via `__drop_closure`.
            || matches!(t, Ty::Fn { .. })
            || matches!(t, Ty::Applied(TypeConstructorId::List, a)
                if a.len() == 1
                    && (!is_heap_ty(&a[0])
                        || matches!(a[0], Ty::String)
                        || variant_field_name(&a[0], variant_names).is_some()))
            || matches!(t, Ty::Applied(TypeConstructorId::Option, a)
                if a.len() == 1 && !is_heap_ty(&a[0]))
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

fn body_has_stmt_position_propagating_unwrap(body: &IrExpr) -> bool {
    fn stmt_is_propagating(kind: &IrStmtKind) -> bool {
        match kind {
            IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } => {
                matches!(&value.kind, IrExprKind::Unwrap { .. } | IrExprKind::Try { .. })
            }
            IrStmtKind::Expr { expr } => {
                matches!(&expr.kind, IrExprKind::Unwrap { .. } | IrExprKind::Try { .. })
            }
            _ => false,
        }
    }
    fn scan(e: &IrExpr) -> bool {
        match &e.kind {
            IrExprKind::Block { stmts, expr } => {
                stmts.iter().any(|s| stmt_is_propagating(&s.kind))
                    || expr.as_deref().is_some_and(scan)
            }
            IrExprKind::If { then, else_, .. } => scan(then) || scan(else_),
            IrExprKind::Match { arms, .. } => arms.iter().any(|a| scan(&a.body)),
            _ => false,
        }
    }
    scan(body)
}

/// Does `body`'s TAIL (recursing through Block/If/Match, the same control-flow-transparent
/// positions `body_has_stmt_position_propagating_unwrap` scans) end in a bare `!` over an
/// OPTION-typed operand? Such a tail can only compile correctly under a `Result[T, String]`
/// ABI: the desugar (`desugar_tail_effect_unwrap`'s bare-Unwrap case) turns it into
/// `match o { none => err("none"), some(v) => ok(v) }`, which constructs a real Result — under
/// a RAW scalar ABI there is no channel for the none case at all (the old pass-through returned
/// the raw Option handle, a confirmed silent wrong-value/invalid-wasm bug in BOTH the
/// declared-Result and the scalar-lifted case). So this is an AUTO_WRAP_ABI_FNS INCLUSION
/// criterion. Gated to OPTION operands only: a RESULT-typed tail-`!` operand (including a
/// never-err `self()!`/`f()!` — Result-typed at this pre-strip point) is repr-compatible with
/// the pass-through in every ABI (same block IS the propagated Result), so wrapping those would
/// only churn working fns (the yaml TCO cluster's tail self-calls among them).
/// Does `body` carry a TAIL/arm-position `Try`/`Unwrap` over a CAN-ERR Named callee
/// (`if n < 0 then fail("negative") else ... checked(n-1)` — every branch either
/// propagates the callee's Result verbatim or yields a raw scalar)? Such a fn's REAL
/// ABI must be Result (the err channel propagates), so it joins `AUTO_WRAP_ABI_FNS`:
/// the `body.ty` override then makes the SCALAR arms wrap (`0` → `ok(0)` via the
/// heap-result arm machinery) while the Try arms pass the callee's same-repr Result
/// through. Without this, `checked` classified can-err (post the Try fixpoint fix)
/// but its base arm still produced a raw i64 against the i32 Result ABI — the
/// effect_tco invalid-wasm divergence, second layer.
fn body_has_tail_position_canerr_try(
    body: &IrExpr,
    can_err: &std::collections::HashSet<String>,
) -> bool {
    fn scan(e: &IrExpr, can_err: &std::collections::HashSet<String>) -> bool {
        match &e.kind {
            IrExprKind::Unwrap { expr } | IrExprKind::Try { expr } => match &expr.kind {
                IrExprKind::Call { target: CallTarget::Named { name }, .. } => {
                    can_err.contains(name.as_str())
                }
                IrExprKind::Call { target: CallTarget::Module { .. }, .. }
                | IrExprKind::RuntimeCall { .. } => true,
                _ => false,
            },
            IrExprKind::Block { expr, .. } => expr.as_deref().is_some_and(|t| scan(t, can_err)),
            IrExprKind::If { then, else_, .. } => scan(then, can_err) || scan(else_, can_err),
            IrExprKind::Match { arms, .. } => arms.iter().any(|a| scan(&a.body, can_err)),
            _ => false,
        }
    }
    scan(body, can_err)
}

fn body_has_tail_position_option_unwrap(body: &IrExpr) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    fn scan(e: &IrExpr) -> bool {
        match &e.kind {
            IrExprKind::Unwrap { expr } => {
                matches!(&expr.ty, Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1)
            }
            IrExprKind::Block { expr, .. } => expr.as_deref().is_some_and(scan),
            IrExprKind::If { then, else_, .. } => scan(then) || scan(else_),
            IrExprKind::Match { arms, .. } => arms.iter().any(|a| scan(&a.body)),
            _ => false,
        }
    }
    scan(body)
}

/// Desugar `assert(cond)` / `assert_eq(a, b)` / `assert_ne(a, b)` (Unit-typed builtin
/// calls — the test-block floor, also legal in a main body) to the §13 controlled-halt
/// shape the SELF-HOST stdlib already proves out (math.pow's negative-exponent guard):
/// `if <cond> then () else prim.die(prim.handle("assertion failed…"))`. Everything
/// downstream is EXISTING machinery — the stmt-position Unit-`if` executes via
/// `try_lower_unit_if`, `==`/`!=` dispatch through the ordinary BinOp lowering (whatever
/// operand types that subset admits; the rest walls honestly), and `prim.die` is the
/// proven Die prim. Failure = message on stderr + exit 1 — the harness keys on the
/// non-zero exit, exactly like v0's trap. Applied desugar-before-both (same slot as
/// `desugar_heap_branches`), so every driver counts and lowers the SAME tree.
fn desugar_assert_calls(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::{walk_expr_mut, IrMutVisitor};
    use almide_lang::intern::sym;
    fn die_expr(msg: &str) -> IrExpr {
        die_on(IrExpr {
            kind: IrExprKind::LitStr { value: msg.to_string() },
            ty: Ty::String,
            span: None,
            def_id: None,
        })
    }
    /// die on an arbitrary String-typed message EXPRESSION (the computed 2-arg
    /// assert message: `assert(c, "got " + float.to_string(x))`).
    fn die_on(lit: IrExpr) -> IrExpr {
        let handle = IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Module { module: sym("prim"), func: sym("handle"), def_id: None },
                args: vec![lit],
                type_args: Vec::new(),
            },
            ty: Ty::Int,
            span: None,
            def_id: None,
        };
        IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Module { module: sym("prim"), func: sym("die"), def_id: None },
                args: vec![handle],
                type_args: Vec::new(),
            },
            ty: Ty::Unit,
            span: None,
            def_id: None,
        }
    }
    struct S {
        changed: bool,
    }
    impl IrMutVisitor for S {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let is_panic = matches!(&e.kind,
                IrExprKind::Call { target: CallTarget::Named { name }, args, .. }
                    if name.as_str() == "panic" && args.len() == 1
                        && matches!(args[0].ty, Ty::String));
            // `panic` types as the enclosing branch demands (Unit or Never) — it must
            // bypass the Unit gate below.
            if !is_panic && !matches!(e.ty, Ty::Unit) {
                return;
            }
            let IrExprKind::Call { target: CallTarget::Named { name }, args, .. } = &e.kind
            else {
                return;
            };
            // `panic(msg)` — an UNCONDITIONAL abort: die on "PANIC: " + msg (the v0
            // wasm form: prefix + message, then halt). The message expr is evaluated
            // only here (the abort path), like the computed assert message.
            if name.as_str() == "panic" && args.len() == 1 && matches!(args[0].ty, Ty::String)
            {
                let msg = args[0].clone();
                let text = match &msg.kind {
                    IrExprKind::LitStr { value } => {
                        die_expr(&format!("PANIC: {value}"))
                    }
                    _ => die_on(IrExpr {
                        kind: IrExprKind::BinOp {
                            op: almide_ir::BinOp::ConcatStr,
                            left: Box::new(IrExpr {
                                kind: IrExprKind::LitStr { value: "PANIC: ".to_string() },
                                ty: Ty::String,
                                span: None,
                                def_id: None,
                            }),
                            right: Box::new(msg),
                        },
                        ty: Ty::String,
                        span: None,
                        def_id: None,
                    }),
                };
                *e = text;
                self.changed = true;
                return;
            }
            let (cond, msg) = match (name.as_str(), args.as_slice()) {
                ("assert", [c]) if matches!(c.ty, Ty::Bool) => {
                    (c.clone(), None)
                }
                // The 2-arg form `assert(cond, msg)`: a LITERAL message folds into
                // the die text; a COMPUTED String message dies on the CONCAT
                // `"assertion failed: " + msg` (evaluated only on the failing path).
                ("assert", [c, m]) if matches!(c.ty, Ty::Bool) && matches!(m.ty, Ty::String) => {
                    (c.clone(), Some(m.clone()))
                }
                ("assert_eq", [a, b]) => (
                    IrExpr {
                        kind: IrExprKind::BinOp {
                            op: almide_ir::BinOp::Eq,
                            left: Box::new(a.clone()),
                            right: Box::new(b.clone()),
                        },
                        ty: Ty::Bool,
                        span: None,
                        def_id: None,
                    },
                    None,
                ),
                ("assert_ne", [a, b]) => (
                    IrExpr {
                        kind: IrExprKind::BinOp {
                            op: almide_ir::BinOp::Neq,
                            left: Box::new(a.clone()),
                            right: Box::new(b.clone()),
                        },
                        ty: Ty::Bool,
                        span: None,
                        def_id: None,
                    },
                    None,
                ),
                _ => return,
            };
            let default_text = match name.as_str() {
                "assert_eq" => "assertion failed: left == right",
                "assert_ne" => "assertion failed: left != right",
                _ => "assertion failed: assert(false)",
            };
            let die = match msg {
                None => die_expr(default_text),
                Some(m) => match &m.kind {
                    IrExprKind::LitStr { value } => {
                        die_expr(&format!("assertion failed: {value}"))
                    }
                    _ => die_on(IrExpr {
                        kind: IrExprKind::BinOp {
                            op: almide_ir::BinOp::ConcatStr,
                            left: Box::new(IrExpr {
                                kind: IrExprKind::LitStr {
                                    value: "assertion failed: ".to_string(),
                                },
                                ty: Ty::String,
                                span: None,
                                def_id: None,
                            }),
                            right: Box::new(m),
                        },
                        ty: Ty::String,
                        span: None,
                        def_id: None,
                    }),
                },
            };
            let unit = IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None };
            *e = IrExpr {
                kind: IrExprKind::If {
                    cond: Box::new(cond),
                    then: Box::new(unit),
                    else_: Box::new(die),
                },
                ty: Ty::Unit,
                span: e.span.clone(),
                def_id: e.def_id,
            };
            self.changed = true;
        }
    }
    let mut s = S { changed: false };
    let mut out = body.clone();
    s.visit_expr_mut(&mut out);
    s.changed.then_some(out)
}

/// `m[k]` over a `Map` (the frontend emits `MapAccess` ONLY for `obj.ty.is_map()`) →
/// `map.get(m, k)` — the ordinary self-host map lookup call (`Option[V]` result), which
/// the repr dispatch suffixes (`get_skv`/`get_str`/…) like every other map call site.
/// Applied desugar-before-both (same slot as `desugar_assert_calls`): the counted tree
/// and the lowering see the SAME Call node, so `mir == ir` holds for the one CallFn.
fn desugar_map_access_calls(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::{walk_expr_mut, IrMutVisitor};
    use almide_lang::intern::sym;
    struct S {
        changed: bool,
    }
    impl IrMutVisitor for S {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::MapAccess { object, key } = &e.kind else {
                return;
            };
            *e = IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Module {
                        module: sym("map"),
                        func: sym("get"),
                        def_id: None,
                    },
                    args: vec![(**object).clone(), (**key).clone()],
                    type_args: Vec::new(),
                },
                ty: e.ty.clone(),
                span: e.span.clone(),
                def_id: e.def_id,
            };
            self.changed = true;
        }
    }
    let mut s = S { changed: false };
    let mut out = body.clone();
    s.visit_expr_mut(&mut out);
    s.changed.then_some(out)
}

/// `buf[i]` over `Bytes` (a scalar `Int` element read) → `bytes.index(buf, i)` — the
/// CHECKED self-host byte read (aborts `Error: index out of bounds` + exit 1 exactly
/// like v0's `b[i]`; `bytes.read_u8`'s 0-for-OOB convention is a DIFFERENT api).
/// Same desugar-before-both slot as `desugar_map_access_calls`.
fn desugar_bytes_index_calls(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::{walk_expr_mut, IrMutVisitor};
    use almide_lang::intern::sym;
    struct S {
        changed: bool,
    }
    impl IrMutVisitor for S {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::IndexAccess { object, index } = &e.kind else {
                return;
            };
            if !matches!(object.ty, Ty::Bytes) || !matches!(e.ty, Ty::Int) {
                return;
            }
            *e = IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Module {
                        module: sym("bytes"),
                        func: sym("index"),
                        def_id: None,
                    },
                    args: vec![(**object).clone(), (**index).clone()],
                    type_args: Vec::new(),
                },
                ty: e.ty.clone(),
                span: e.span.clone(),
                def_id: e.def_id,
            };
            self.changed = true;
        }
    }
    let mut s = S { changed: false };
    let mut out = body.clone();
    s.visit_expr_mut(&mut out);
    s.changed.then_some(out)
}

/// A float-family BinOp over MATRIX operands (`a * b` / `a + b` / `a - b` on Matrix —
/// the numeric-protocol operators) → the registered `matrix.mul`/`add`/`sub` module
/// call. The scalar-binop path had NO operand gate on the arithmetic arms, so `a * b`
/// lowered as an f64 multiply of the two BLOCK HANDLES — a silent garbage Matrix on
/// the verified default (matrix_test's `*` row). Same desugar-before-both slot as
/// `desugar_map_access_calls` (the rewrite adds ONE counted Module call).
fn desugar_matrix_binops(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::{walk_expr_mut, IrMutVisitor};
    use almide_lang::intern::sym;
    struct S {
        changed: bool,
    }
    impl IrMutVisitor for S {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::BinOp { op, left, right } = &e.kind else { return };
            let is_matrix = |t: &Ty| {
                matches!(t, Ty::Matrix)
                    || matches!(t, Ty::Applied(
                        almide_lang::types::constructor::TypeConstructorId::Matrix, _))
            };
            // `m * k` / `k * m` (ScaleMatrix — one Matrix, one scalar) → matrix.scale
            // with the Matrix normalized to the FIRST arg (the self-host's signature).
            if matches!(op, almide_ir::BinOp::ScaleMatrix) {
                let (m, k) = if is_matrix(&left.ty) {
                    ((**left).clone(), (**right).clone())
                } else {
                    ((**right).clone(), (**left).clone())
                };
                e.kind = IrExprKind::Call {
                    target: CallTarget::Module {
                        module: sym("matrix"),
                        func: sym("scale"),
                        def_id: None,
                    },
                    args: vec![m, k],
                    type_args: Vec::new(),
                };
                self.changed = true;
                return;
            }
            if !is_matrix(&left.ty) || !is_matrix(&right.ty) {
                return;
            }
            // The frontend's dispatch: `a * b` (both Matrix) → MulMatrix; `m * k` →
            // ScaleMatrix (handled by the two-typed arm below); `a + b`/`a - b` fall
            // through the NUMERIC arms as AddInt/SubInt (neither operand is Float),
            // so those are matched here by the MATRIX operand types, not the op class.
            let func = match op {
                almide_ir::BinOp::MulMatrix => "mul",
                almide_ir::BinOp::AddMatrix => "add",
                almide_ir::BinOp::SubMatrix => "sub",
                almide_ir::BinOp::AddInt | almide_ir::BinOp::AddFloat => "add",
                almide_ir::BinOp::SubInt | almide_ir::BinOp::SubFloat => "sub",
                almide_ir::BinOp::DivInt | almide_ir::BinOp::DivFloat => "div",
                almide_ir::BinOp::MulInt | almide_ir::BinOp::MulFloat => "mul",
                _ => return,
            };
            e.kind = IrExprKind::Call {
                target: CallTarget::Module {
                    module: sym("matrix"),
                    func: sym(func),
                    def_id: None,
                },
                args: vec![(**left).clone(), (**right).clone()],
                type_args: Vec::new(),
            };
            self.changed = true;
        }
    }
    let mut s = S { changed: false };
    let mut out = body.clone();
    s.visit_expr_mut(&mut out);
    s.changed.then_some(out)
}

/// `buf[i] = v` over `Bytes` — the WRITE-side twin of `desugar_bytes_index_calls` —
/// → statement `bytes.set_at(buf, i, v)`, the CHECKED packed-byte store self-host
/// (whose receiver rides the #794 COW discipline: local var → MakeUnique, mut param
/// → write-through). Without this rewrite `IndexAssign` lowers as a uniform 8-byte
/// SLOT store (`+12+i*8` — never where `bytes.index` reads `+12+i`, and past a
/// packed block's end for i>3): `buf[2] = 0x42` silently vanished on the verified
/// default while corrupting the neighboring heap block. Bytes receivers are known
/// by TYPE: `Bytes`-typed params plus `Bind`s with `ty: Bytes`, seen in statement
/// order (VarIds are function-unique, so no scoping ambiguity).
fn desugar_bytes_index_assign(body: &IrExpr, params: &[IrParam]) -> Option<IrExpr> {
    use almide_ir::{walk_stmt_mut, IrMutVisitor};
    use almide_lang::intern::sym;
    struct S {
        bytes_vars: HashSet<VarId>,
        changed: bool,
    }
    impl IrMutVisitor for S {
        fn visit_stmt_mut(&mut self, stmt: &mut IrStmt) {
            walk_stmt_mut(self, stmt);
            if let IrStmtKind::Bind { var, ty: Ty::Bytes, .. } = &stmt.kind {
                self.bytes_vars.insert(*var);
                return;
            }
            let IrStmtKind::IndexAssign { target, index, value } = &stmt.kind else {
                return;
            };
            if !self.bytes_vars.contains(target) {
                return;
            }
            let recv = IrExpr {
                kind: IrExprKind::Var { id: *target },
                ty: Ty::Bytes,
                span: index.span.clone(),
                def_id: None,
            };
            let call = IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Module {
                        module: sym("bytes"),
                        func: sym("set_at"),
                        def_id: None,
                    },
                    args: vec![recv, index.clone(), value.clone()],
                    type_args: Vec::new(),
                },
                ty: Ty::Unit,
                span: index.span.clone(),
                def_id: None,
            };
            stmt.kind = IrStmtKind::Expr { expr: call };
            self.changed = true;
        }
    }
    let mut s = S {
        bytes_vars: params.iter().filter(|p| matches!(p.ty, Ty::Bytes)).map(|p| p.var).collect(),
        changed: false,
    };
    let mut out = body.clone();
    s.visit_expr_mut(&mut out);
    s.changed.then_some(out)
}

/// `xs[a..b]` over a SCALAR-element list: the frontend struck the range slice
/// directly to `RuntimeCall{almide_rt_list_slice}` (expressions.rs), which the
/// v1 bind path can only defer to an EMPTY Opaque — `sub[0]` then walls. But
/// `almide_rt_list_slice` IS `list.slice`, and `list.slice` is SELF-HOSTED
/// (list_take_drop.almd) — rewrite the RuntimeCall back to the Module call so
/// it rides `lower_pure_module_value_call` and materializes a REAL list.
/// Same desugar-before-both slot as `desugar_map_access_calls`. Gated to a
/// `List[scalar]` result — the registered self-host is the scalar-element
/// `list_slice`; a heap-element slice keeps the (walling) deferred path.
/// `buf[a..b]` over `Bytes` (`RuntimeCall{almide_rt_bytes_slice}`) is the same
/// deferred-Opaque hole with a WORSE failure (the empty defer READS as len 0 —
/// `bytes.len(sub)` returned 0 silently) — rewrite to the self-hosted
/// `bytes.slice(b, start, end)` (bytes_core.almd, v0-clamping semantics).
fn desugar_list_slice_calls(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::{walk_expr_mut, IrMutVisitor};
    use almide_lang::intern::sym;
    struct S {
        changed: bool,
    }
    impl IrMutVisitor for S {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::RuntimeCall { symbol, args } = &e.kind else {
                return;
            };
            if args.len() != 3 {
                return;
            }
            let (module, func) = match symbol.as_str() {
                "almide_rt_list_slice" if crate::lower::is_scalar_elem_list_ty(&e.ty) => {
                    ("list", "slice")
                }
                "almide_rt_bytes_slice" if matches!(e.ty, Ty::Bytes) => ("bytes", "slice"),
                _ => return,
            };
            *e = IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Module {
                        module: sym(module),
                        func: sym(func),
                        def_id: None,
                    },
                    args: args.clone(),
                    type_args: Vec::new(),
                },
                ty: e.ty.clone(),
                span: e.span.clone(),
                def_id: e.def_id,
            };
            self.changed = true;
        }
    }
    let mut s = S { changed: false };
    let mut out = body.clone();
    s.visit_expr_mut(&mut out);
    s.changed.then_some(out)
}

/// `p?.f` → `match p { some(__x) => some(__x.f), none => none }` — a PURE desugar
/// into the proven Option-match rails (variant-seeded subjects, payload binds,
/// heap-result arms), replacing the deferred-Opaque the OptionalChain node fell
/// to (its bound var then misread as `none`/garbage in any comparison — the
/// unwrap_operators optional-chain walls). Same desugar-before-both slot as
/// `desugar_map_access_calls`; the rewrite adds NO calls (Match/Member/Some are
/// call-free), so both counters see the identical call multiset. Fresh payload
/// vars mint past `max_var_id` (the desugar_unwrap discipline).
fn desugar_optional_chain(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::{walk_expr_mut, IrMutVisitor};
    use almide_lang::types::constructor::TypeConstructorId;
    struct S {
        changed: bool,
        next_var: u32,
    }
    impl IrMutVisitor for S {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::OptionalChain { expr, field } = &e.kind else {
                return;
            };
            let Ty::Applied(TypeConstructorId::Option, a) = &expr.ty else {
                return;
            };
            if a.len() != 1 {
                return;
            }
            let payload_ty = a[0].clone();
            let x = VarId(self.next_var);
            self.next_var += 1;
            let mk = |kind: IrExprKind, ty: Ty| IrExpr { kind, ty, span: e.span.clone(), def_id: None };
            let field_ty = match &e.ty {
                Ty::Applied(TypeConstructorId::Option, fa) if fa.len() == 1 => fa[0].clone(),
                _ => return,
            };
            let x_read = mk(IrExprKind::Var { id: x }, payload_ty.clone());
            let member =
                mk(IrExprKind::Member { object: Box::new(x_read), field: *field }, field_ty);
            let some_body = mk(IrExprKind::OptionSome { expr: Box::new(member) }, e.ty.clone());
            let none_body = mk(IrExprKind::OptionNone, e.ty.clone());
            let arms = vec![
                almide_ir::IrMatchArm {
                    pattern: almide_ir::IrPattern::Some {
                        inner: Box::new(almide_ir::IrPattern::Bind { var: x, ty: payload_ty }),
                    },
                    guard: None,
                    body: some_body,
                },
                almide_ir::IrMatchArm { pattern: almide_ir::IrPattern::None, guard: None, body: none_body },
            ];
            // ANF-lift a non-Var subject (`match f() {…}` → `{ let __s = f(); match __s {…} }`):
            // the LET-BOUND Named call is what seeds the Option read-shape
            // (`materialized_options`), so the match branches on a TRACKED subject.
            let (stmts, subject) = if matches!(&expr.kind, IrExprKind::Var { .. }) {
                (Vec::new(), expr.clone())
            } else {
                let s_var = VarId(self.next_var);
                self.next_var += 1;
                let bind = IrStmt {
                    kind: IrStmtKind::Bind {
                        var: s_var,
                        mutability: almide_ir::Mutability::Let,
                        ty: expr.ty.clone(),
                        value: (**expr).clone(),
                    },
                    span: e.span.clone(),
                };
                let subj = mk(IrExprKind::Var { id: s_var }, expr.ty.clone());
                (vec![bind], Box::new(subj))
            };
            let match_expr = mk(IrExprKind::Match { subject, arms }, e.ty.clone());
            *e = if stmts.is_empty() {
                match_expr
            } else {
                mk(IrExprKind::Block { stmts, expr: Some(Box::new(match_expr)) }, e.ty.clone())
            };
            self.changed = true;
        }
    }
    let mut s = S { changed: false, next_var: crate::lower::max_var_id(body) + 1 };
    let mut out = body.clone();
    s.visit_expr_mut(&mut out);
    s.changed.then_some(out)
}

/// The `Result[Unit, E]` this fn's ABI promises when its body's effective TAIL is Unit-typed
/// (descending Block chains; an absent tail is Unit) — `None` when the tail carries a real
/// value or the fn is not Result-ABI. Declared `Result[Unit, E]` keeps its own `E`; a
/// declared-Unit AUTO_WRAP lift synthesizes `Result[Unit, String]` (the same type the
/// `owned_body` override stamps). Declared-Option and declared-Unit-non-AUTO_WRAP fns
/// (including a void-convention main) are excluded by construction.
fn unit_tail_result_abi_ty(func: &IrFunction, body: &IrExpr) -> Option<Ty> {
    use almide_lang::types::constructor::TypeConstructorId;
    fn tail_is_unit(e: &IrExpr) -> bool {
        match &e.kind {
            IrExprKind::Block { expr: Some(t), .. } => tail_is_unit(t),
            IrExprKind::Block { expr: None, .. } => true,
            _ => matches!(e.ty, Ty::Unit),
        }
    }
    let result_ty = match &func.ret_ty {
        Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 && matches!(a[0], Ty::Unit) => {
            func.ret_ty.clone()
        }
        // A LIFTED (declared-Unit effect) fn whose CALLERS keep the Result expectation:
        // the AUTO_WRAP set, or any CAN-ERR lifted fn (∉ NEVER_ERR — e.g. an argument-
        // position `!` errs without tripping the stmt/tail AUTO_WRAP heuristics, so the
        // caller's `Try` is never stripped and it `local.set`s the promised handle).
        // The def must return that handle: same registry, same verdict, by construction.
        // `main` keeps the exit-code void convention (its caller is `_start`, not a
        // registry-classified call site).
        Ty::Unit
            if func.is_effect
                && func.name.as_str() != "main"
                && (crate::lower::AUTO_WRAP_ABI_FNS
                    .with(|s| s.borrow().contains(func.name.as_str()))
                    || !crate::lower::NEVER_ERR_LIFTED_FNS
                        .with(|s| s.borrow().contains(func.name.as_str()))) =>
        {
            Ty::result(Ty::Unit, Ty::String)
        }
        _ => return None,
    };
    tail_is_unit(body).then_some(result_ty)
}

/// `{ stmts…; unit_tail }` → `{ stmts…; unit_tail; ok(()) }` — the old Unit tail becomes a
/// statement (the standard stmt-position effect shape), and the fn returns the real ok-Unit
/// Result block its ABI classification promises. Only the TOP-level Block is flattened; a
/// non-Block unit body becomes the single statement.
fn wrap_unit_body_in_ok(body: &IrExpr, result_ty: Ty) -> IrExpr {
    let (mut stmts, old_tail) = match &body.kind {
        IrExprKind::Block { stmts, expr } => (stmts.clone(), expr.as_deref().cloned()),
        _ => (Vec::new(), Some(body.clone())),
    };
    if let Some(t) = old_tail {
        stmts.push(IrStmt { kind: IrStmtKind::Expr { expr: t }, span: None });
    }
    let ok_unit = IrExpr {
        kind: IrExprKind::ResultOk {
            expr: Box::new(IrExpr {
                kind: IrExprKind::Unit,
                ty: Ty::Unit,
                span: None,
                def_id: None,
            }),
        },
        ty: result_ty.clone(),
        span: None,
        def_id: None,
    };
    IrExpr {
        kind: IrExprKind::Block { stmts, expr: Some(Box::new(ok_unit)) },
        ty: result_ty,
        span: body.span.clone(),
        def_id: body.def_id,
    }
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
    // A `mut` param's write-back rides v0's tuple-return + place-writeback
    // convention (C-131/C-132). The v1 lower has NO move-mode calling convention
    // yet: a mutation through the borrowed param COWs a copy and silently DROPS
    // the caller-visible write (`push9(v, 20)` left `v` unchanged on the verified
    // default while v0 pushed — the #790 mut_list_param row, main-reachable).
    // WALL the fn — v0 emits the correct convention on both targets.
    if !func.mutated_params.is_empty() {
        return Err(LowerError::Unsupported(format!(
            "fn `{}` mutates its `mut` param(s) — the move-mode write-back \
             convention (C-132) not in this brick",
            func.name
        )));
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
        // STRICTLY-Result declared return (Option excluded — see the field doc) OR an
        // auto-wrapped scalar ABI: the bare-tail-Option-`!` desugar's gate.
        ret_is_result_abi: matches!(
            &func.ret_ty,
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, _)
        ) || crate::lower::AUTO_WRAP_ABI_FNS
            .with(|s| s.borrow().contains(func.name.as_str())),
        // The fn's effective err type — declared `Result[_, E]`'s E, `String` for the lifted
        // synthetic Result, None for a declared Option (its `!` pass-through is repr-identical).
        decl_fn_err: match &func.ret_ty {
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, a)
                if a.len() == 2 =>
            {
                Some(a[1].clone())
            }
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, _) => None,
            _ => Some(Ty::String),
        },
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
    let owned_body;
    let func_body: &IrExpr = if crate::lower::AUTO_WRAP_ABI_FNS
        .with(|s| s.borrow().contains(func.name.as_str()))
    {
        owned_body = IrExpr { ty: Ty::result(func.ret_ty.clone(), Ty::String), ..func.body.clone() };
        &owned_body
    } else {
        &func.body
    };
    // assert/assert_eq/assert_ne → the controlled-halt `if`/die shape (see
    // `desugar_assert_calls`). Desugar-before-both: every downstream consumer
    // (counting, TCO, lowering) sees the same tree.
    let assert_body;
    let func_body: &IrExpr = if let Some(rewritten) = desugar_assert_calls(func_body) {
        assert_body = rewritten;
        &assert_body
    } else {
        func_body
    };
    // `m[k]` → `map.get(m, k)` (see `desugar_map_access_calls`) — same
    // desugar-before-both slot.
    let map_access_body;
    let func_body: &IrExpr = if let Some(rewritten) = desugar_map_access_calls(func_body) {
        map_access_body = rewritten;
        &map_access_body
    } else {
        func_body
    };
    // `buf[i]` over Bytes → `bytes.index(buf, i)` (see `desugar_bytes_index_calls`).
    let bytes_index_body;
    let func_body: &IrExpr = if let Some(rewritten) = desugar_bytes_index_calls(func_body) {
        bytes_index_body = rewritten;
        &bytes_index_body
    } else {
        func_body
    };
    // Matrix `a * b`/`+`/`-` → matrix.mul/add/sub (see `desugar_matrix_binops`) —
    // same desugar-before-both slot.
    let matrix_binop_body;
    let func_body: &IrExpr = if let Some(rewritten) = desugar_matrix_binops(func_body) {
        matrix_binop_body = rewritten;
        &matrix_binop_body
    } else {
        func_body
    };
    // The C-127 piped HOF chain (`… |> option.map(λ) |> option.unwrap_or(d)`) →
    // its source-`let` decomposed form (see `desugar_hof_chain_anf`) — same
    // desugar-before-both slot.
    let hof_chain_body;
    let func_body: &IrExpr = if let Some(rewritten) = desugar_hof_chain_anf(func_body) {
        hof_chain_body = rewritten;
        &hof_chain_body
    } else {
        func_body
    };
    // `buf[i] = v` over Bytes → `bytes.set_at(buf, i, v)` (see
    // `desugar_bytes_index_assign`) — same desugar-before-both slot.
    let bytes_index_assign_body;
    let func_body: &IrExpr =
        if let Some(rewritten) = desugar_bytes_index_assign(func_body, &func.params) {
            bytes_index_assign_body = rewritten;
            &bytes_index_assign_body
        } else {
            func_body
        };
    // `xs[a..b]` slice RuntimeCall → `list.slice(xs, a, b)` (see `desugar_list_slice_calls`).
    let list_slice_body;
    let func_body: &IrExpr = if let Some(rewritten) = desugar_list_slice_calls(func_body) {
        list_slice_body = rewritten;
        &list_slice_body
    } else {
        func_body
    };
    // `p?.f` → the some/none match (see `desugar_optional_chain`).
    let opt_chain_body;
    let func_body: &IrExpr = if let Some(rewritten) = desugar_optional_chain(func_body) {
        opt_chain_body = rewritten;
        &opt_chain_body
    } else {
        func_body
    };
    // A RESULT-ABI fn (declared `Result[Unit, E]`, or a declared-Unit AUTO_WRAP lift) whose
    // effective TAIL is Unit-typed produces NO value on the unit path — the never-err strips
    // reduce a lifted tail call to a raw Unit effect call, and a declared-Result effect fn can
    // end on a bare effect stmt. But every CALL SITE consults the same name-keyed ABI
    // registries and `local.set`s the expected Result handle over the void callee — invalid
    // wasm (the #786 class: def and call sites disagree on the ABI). Materialize the missing
    // value: `body_unit` → `{ body_unit; ok(()) }`, so the def returns the real Result block
    // its classification promises (the proven alloc(i) + move-out(m) tail). A declared-Unit
    // main is NEITHER case (both gates miss), so the exit-code void convention is untouched.
    let ok_wrapped_body;
    let func_body: &IrExpr = if let Some(result_ty) = unit_tail_result_abi_ty(func, func_body) {
        ok_wrapped_body = wrap_unit_body_in_ok(func_body, result_ty);
        &ok_wrapped_body
    } else {
        func_body
    };
    crate::lower::dump_desugared_ir(func.name.as_str(), func_body, variant_layouts, record_layouts);
    let pre_tco = desugar_heap_branches(func_body, variant_layouts);
    let body_ref: &IrExpr = pre_tco.as_ref().unwrap_or(func_body);
    let tco_body = try_tco_rewrite(&ctx.fn_name, &func.params, body_ref);
    let final_body = tco_body.as_ref().unwrap_or(body_ref);
    // SHARED-CELL pre-scan (closures Rung 6, cells.rs): over the FINAL lowered tree,
    // so bind/read/write/capture all classify the same vars as cells. A pure scan —
    // no rewrite, so the counted tree is untouched.
    ctx.cell_vars = collect_cell_vars(final_body, &ctx.globals, &func.params);
    let ret = ctx.lower_body_into(final_body)?;
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
    // The synthesized recursive-eq helpers ride the same rail as lifted lambdas
    // (extra cluster functions; per-parent names, so no cross-fn collision).
    all.extend(std::mem::take(&mut ctx.synth_eq_fns));
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

include!("drop_sources.rs");
include!("repr_sources.rs");
include!("newtype_erase.rs");
include!("record_defaults.rs");
include!("desugar_guard.rs");
include!("cells.rs");
include!("mod_p2.rs");
include!("mod_p3.rs");
include!("mod_p4.rs");
include!("mod_p5.rs");
// The desugar family (formerly one 4.8k-line mod_p6.rs), split by concern:
include!("desugar.rs");
include!("desugar_unwrap.rs");
include!("desugar_loop.rs");
include!("desugar_branch.rs");
include!("desugar_fan.rs");
include!("desugar_match.rs");
include!("desugar_match_subject.rs");
include!("synth_eq.rs");
