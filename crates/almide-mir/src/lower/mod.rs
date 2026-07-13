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
) {
    use std::collections::HashMap;
    let mut by_name: HashMap<&str, Option<(Ty, &almide_ir::IrExpr)>> = HashMap::new();
    for m in &ir.modules {
        for tl in &m.top_lets {
            let Some(info) = m.var_table.entries.get(tl.var.0 as usize) else { continue };
            by_name
                .entry(info.name.as_str())
                .and_modify(|e| *e = Option::None) // second definition ⇒ ambiguous, drop
                .or_insert(Some((tl.ty.clone(), &tl.value)));
        }
    }
    // OVERRIDES an existing (module-raw, possibly colliding) entry — callers order the
    // composition as: module union → this bridge → main top-lets re-inserted last, so the
    // precedence is main > bridged-name > raw module id.
    for (i, info) in ir.var_table.entries.iter().enumerate() {
        let id = almide_ir::VarId(i as u32);
        if let Some(Some((ty, init))) = by_name.get(info.name.as_str()) {
            if *ty == info.ty {
                globals.insert(id, ty.clone());
                global_inits.insert(id, (*init).clone());
            }
        }
    }
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
    // owns no children), a List[<variant>] (per-element), or a RECORD (recurse via `$__drop_<R>`
    // / a scalar-only record's flat rc_dec — see the drop generator's field loop).
    let supported_heap = |t: &Ty| -> bool {
        use almide_lang::types::constructor::TypeConstructorId;
        variant_field_name(t, variant_names).is_some()
            || matches!(t, Ty::Named(n, _) if record_names.contains(n.as_str()))
            || matches!(t, Ty::String)
            || matches!(t, Ty::Applied(TypeConstructorId::List, a)
                if a.len() == 1
                    && (!is_heap_ty(&a[0])
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
    crate::lower::dump_desugared_ir(func.name.as_str(), &func.body, variant_layouts, record_layouts);
    let pre_tco = desugar_heap_branches(&func.body, variant_layouts);
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

include!("drop_sources.rs");
include!("repr_sources.rs");
include!("newtype_erase.rs");
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
