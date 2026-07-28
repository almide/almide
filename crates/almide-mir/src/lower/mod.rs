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

/// The USER-FACING rendering: the reason, bare. The `Debug` form wraps it in
/// `Unsupported("…")` — and because walls nest (a fn's wall is quoted inside
/// the program's), Debug-formatting at each level compounded into
/// `Unsupported("…: Unsupported(\"…\")")` with escaped quotes, the worst
/// diagnostic in the compiler (#931). Every layer that shows a wall to a HUMAN
/// — the fn-wall ledger, the CLI's wall error, the native-fallback notice —
/// formats through THIS, so the reason reads as one sentence at any depth.
impl std::fmt::Display for LowerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LowerError::Unsupported(reason) => f.write_str(reason),
        }
    }
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
/// THE definition lives beside `Ty` (`almide_types::types::heap`, #926) as an
/// exhaustive no-wildcard match, so a new `Ty` variant must be classified before
/// the workspace compiles; this re-export keeps the SoT reading
/// (`crate::lower::is_heap_ty`) both renderers and the optimizer share.
pub use almide_lang::types::is_heap_ty;

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
// Pure guards, no recursion — named so the two Module-call arms of
// `const_global_init` below read as one condition instead of three inlined
// clauses each (same "extract the boolean, keep the shape" refactor as the
// guard predicates in `lower/binds_p4_b_b.rs`).
fn is_string_from_codepoint_call(module: &str, func: &str, arg_count: usize) -> bool {
    module == "string" && func == "from_codepoint" && arg_count == 1
}
fn is_bytes_from_list_call(module: &str, func: &str, arg_count: usize) -> bool {
    module == "bytes" && func == "from_list" && arg_count == 1
}

/// `string.from_codepoint(<int literal>)` (`let NL = string.from_codepoint(10)` —
/// the stringify-escape test globals) CONST-FOLDS to its one-char string at
/// lowering time: zero calls injected, so the count gate stays exact. An invalid
/// codepoint keeps walling (never a wrong byte).
fn const_fold_string_from_codepoint(args: &[IrExpr]) -> Option<crate::Init> {
    let IrExprKind::LitInt { value } = &args[0].kind else { return None };
    u32::try_from(*value)
        .ok()
        .and_then(char::from_u32)
        .map(|c| crate::Init::Str(c.to_string()))
}

fn const_fold_bytes_from_list(args: &[IrExpr]) -> Option<crate::Init> {
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
            if is_string_from_codepoint_call(module.as_str(), func.as_str(), args.len()) =>
        {
            const_fold_string_from_codepoint(args)
        }
        IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
            if is_bytes_from_list_call(module.as_str(), func.as_str(), args.len()) =>
        {
            const_fold_bytes_from_list(args)
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
/// wrong value (the `prim.handle(<literal>)` → address-0 class). Set by the
/// render/output entrypoints (render_program); the classifier and the in-process
/// unit tests keep the permissive caps-counting behavior.
///
/// THREAD-LOCAL, for the same reason [`mod_p2::NEVER_ERR_LIFTED_FNS`] is:
/// lowering runs single-threaded per program, so per-thread state carries the
/// mode faithfully on the real render path. A process-global `AtomicBool` did
/// not: `cargo test` runs a crate's unit tests as many threads in ONE process,
/// so a test that enabled strict mode leaked it into every test that ran after
/// it on any thread. `lower::tests::scalar_loop_with_break_walls` asserts a
/// break/continue wall and intermittently saw a STRICT-mode wall instead —
/// order-dependent, invisible when the test ran alone, and the doc-comment above
/// claimed the opposite ("the in-process unit tests keep the permissive
/// behavior") while nothing enforced it.
pub fn set_strict_values(on: bool) {
    STRICT_VALUES.with(|s| s.set(on));
}

/// Enable strict mode for a scope and restore the previous value on drop.
///
/// Prefer this over the bare setter. Thread-locality alone is not enough: it
/// isolates the PARALLEL test runner (libtest gives each test its own thread),
/// but `--test-threads=1` runs every test on the same thread, where an
/// unrestored setter leaks exactly as the process-global did. It also matters
/// outside tests — a long-lived process that compiles more than one program
/// (the playground, the language server) would otherwise have the first strict
/// render pin the mode for everything after it.
#[must_use = "the guard restores the previous mode when dropped; binding it to `_` restores immediately"]
pub struct StrictValuesGuard(bool);

impl StrictValuesGuard {
    pub fn set(on: bool) -> Self {
        let prev = strict_values();
        set_strict_values(on);
        Self(prev)
    }
}

impl Drop for StrictValuesGuard {
    fn drop(&mut self) {
        set_strict_values(self.0);
    }
}

thread_local! {
    static STRICT_VALUES: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub(crate) fn strict_values() -> bool {
    STRICT_VALUES.with(|s| s.get())
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

include!("mod_b.rs");
include!("mod_c.rs");
include!("crossmod_toplet_bridge.rs");
