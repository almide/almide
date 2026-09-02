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

/// The KNOWN wall shapes (#931): a coarse classification of the constructs
/// the verified renderer most often refuses. Each known shape carries a
/// plain-language headline and the documented rewrite (the CHEATSHEET idioms)
/// that takes the program back inside the subset — so the CLI can lead with
/// what the USER wrote and how to change it, keeping the compiler-internal
/// reason string for a trailing `note:`. `Other` is every wall with no
/// specific rewrite; it renders as before (reason as the headline).
/// `wall_shape_hints.rs` gates that every non-`Other` shape has both texts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WallShape {
    /// A `while` body reassigning a heap accumulator (`s = s + x`).
    WhileHeapAccumulator,
    /// A `match`/`if` producing a heap value, bound to a `let`/`var`.
    HeapResultBind,
    /// An Option/Result match bound to a `let`/`var` outside the executable
    /// (scalar-payload) subset.
    VariantValueMatch,
    /// A call argument (or scalar-call operand) whose shape the lowering
    /// cannot admit.
    CallArgument,
    /// A tail-position expression that is not a supported field/element
    /// extraction.
    TailExtraction,
    /// No specific rewrite known — the reason string is the whole story.
    Other,
}

impl WallShape {
    /// What the user wrote, in surface-language vocabulary — the diagnostic
    /// headline. `None` for [`WallShape::Other`].
    pub fn headline(&self) -> Option<&'static str> {
        match self {
            WallShape::WhileHeapAccumulator => Some(
                "this `while` body grows a heap value (String/List) across iterations \
                 — not yet in the verified wasm subset",
            ),
            WallShape::HeapResultBind => Some(
                "this `match`/`if` produces a heap value (String/List/record) and is \
                 bound to a let/var — not yet in the verified wasm subset",
            ),
            WallShape::VariantValueMatch => Some(
                "this Option/Result match binds a payload shape that is not yet in \
                 the verified wasm subset",
            ),
            WallShape::CallArgument => {
                Some("this call argument's shape is not yet in the verified wasm subset")
            }
            WallShape::TailExtraction => {
                Some("this return expression's shape is not yet in the verified wasm subset")
            }
            WallShape::Other => None,
        }
    }

    /// The documented rewrite that takes the program back inside the subset
    /// (the CHEATSHEET/CLAUDE.md idioms). `None` for [`WallShape::Other`].
    pub fn rewrite_hint(&self) -> Option<&'static str> {
        match self {
            WallShape::WhileHeapAccumulator => Some(
                "hoist the accumulator into a recursive helper fn (prefer recursion \
                 over var + while), or build the value with a list combinator \
                 (map / filter / join)",
            ),
            WallShape::HeapResultBind => Some(
                "move the `match`/`if` into tail position: return it from a small \
                 helper fn instead of binding it to a let/var",
            ),
            WallShape::VariantValueMatch => Some(
                "scalar payloads (Int/Float/Bool) execute directly; for heap payloads \
                 match in tail position via a helper fn, or collapse the value first \
                 with `??`",
            ),
            WallShape::CallArgument => {
                Some("hoist the argument into its own `let` binding and pass the name")
            }
            WallShape::TailExtraction => {
                Some("bind the expression to a `let` and return the binding")
            }
            WallShape::Other => None,
        }
    }
}

/// A lowering could not proceed because the input is outside this brick's
/// subset (or violates a precondition such as concrete types). Carrying the
/// reason keeps the pass TOTAL — no case is silently skipped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LowerError {
    Unsupported(String),
    // (impl below: `with_fn_context` — a DECORATOR appending the function name
    // to an existing spanless wall; spelled via `Self::` so the #931 ratchet's
    // textual scan keeps counting only real wall SITES.)
    /// A wall carrying the SOURCE SPAN of the construct that walled (#931):
    /// line/col/end_col in the lexer's 1-indexed char convention, straight off
    /// the nearest IR node. Construct via [`LowerError::at`] — every NEW wall
    /// site must use it (the spanless count is ratcheted by
    /// `spanless_wall_count_only_goes_down`), so the CLI can render the wall
    /// through the Diagnostic machinery with the source line and a caret
    /// instead of a bare sentence. Sites matching a known [`WallShape`]
    /// construct via [`LowerError::shaped`] instead, which additionally buys
    /// the per-shape headline and rewrite hint.
    UnsupportedAt {
        reason: String,
        span: almide_ir::Span,
        shape: WallShape,
    },
}

impl LowerError {
    /// Append the walling FUNCTION's name to a spanless reason — a decorator
    /// on an existing wall, not a new wall site. A reason already carrying a
    /// location (" in `") passes through untouched.
    pub fn with_fn_context(self, fn_name: &str) -> Self {
        match self {
            Self::Unsupported(msg) if !msg.contains(" in `") => {
                Self::Unsupported(format!("{msg} (fn `{fn_name}`)"))
            }
            other => other,
        }
    }

    /// Span-carrying wall constructor — pass the nearest IR node's span.
    /// Falls back to the spanless form when the node carries none, so callers
    /// never have to branch.
    pub fn at(span: Option<almide_ir::Span>, reason: impl Into<String>) -> LowerError {
        match span {
            Some(span) => LowerError::UnsupportedAt {
                reason: reason.into(),
                span,
                shape: WallShape::Other,
            },
            None => LowerError::Unsupported(reason.into()),
        }
    }

    /// Shape-carrying wall constructor (#931): [`LowerError::at`] plus the
    /// known [`WallShape`], so the CLI headlines the construct and hints its
    /// documented rewrite. Spanless input falls back exactly as `at` does —
    /// the shape is dropped, because the hint machinery renders only through
    /// the span path.
    pub fn shaped(
        span: Option<almide_ir::Span>,
        shape: WallShape,
        reason: impl Into<String>,
    ) -> LowerError {
        match span {
            Some(span) => LowerError::UnsupportedAt {
                reason: reason.into(),
                span,
                shape,
            },
            None => LowerError::at(None, reason),
        }
    }

    /// The wall reason, span or not — what every ledger/notice prints.
    pub fn reason(&self) -> &str {
        match self {
            LowerError::Unsupported(reason) => reason,
            LowerError::UnsupportedAt { reason, .. } => reason,
        }
    }

    /// The source span, when the construction site had one to give.
    pub fn span(&self) -> Option<almide_ir::Span> {
        match self {
            LowerError::Unsupported(_) => None,
            LowerError::UnsupportedAt { span, .. } => Some(*span),
        }
    }

    /// The wall's shape — [`WallShape::Other`] when the site recorded none
    /// (every spanless wall, and every `at`-constructed site).
    pub fn shape(&self) -> WallShape {
        match self {
            LowerError::UnsupportedAt { shape, .. } => *shape,
            _ => WallShape::Other,
        }
    }
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
        f.write_str(self.reason())
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
    let IrExprKind::LitInt { value } = &args[0].kind else {
        return None;
    };
    u32::try_from(*value)
        .ok()
        .and_then(char::from_u32)
        .map(|c| crate::Init::Str(c.to_string()))
}

fn const_fold_bytes_from_list(args: &[IrExpr]) -> Option<crate::Init> {
    let IrExprKind::List { elements } = &args[0].kind else {
        return None;
    };
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
        IrExprKind::Call {
            target: CallTarget::Module { module, func, .. },
            args,
            ..
        } if is_string_from_codepoint_call(module.as_str(), func.as_str(), args.len()) => {
            const_fold_string_from_codepoint(args)
        }
        IrExprKind::Call {
            target: CallTarget::Module { module, func, .. },
            args,
            ..
        } if is_bytes_from_list_call(module.as_str(), func.as_str(), args.len()) => {
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
        return Ok(Repr::Ptr {
            layout: PLACEHOLDER_LAYOUT,
        });
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
        Ty::Int
        | Ty::Int8
        | Ty::Int16
        | Ty::Int32
        | Ty::Int64
        | Ty::UInt8
        | Ty::UInt16
        | Ty::UInt32
        | Ty::UInt64 => Some(WasmAbi::I64),
        Ty::Float | Ty::Float32 | Ty::Float64 => Some(WasmAbi::F64),
        Ty::Bool => Some(WasmAbi::I32),
        // A String / list / map / any heap value crosses the boundary as an i32 POINTER.
        _ if is_heap_ty(ty) => Some(WasmAbi::I32),
        _ => None,
    }
}

include!("mod_b.rs");
include!("mod_c.rs");
include!("mod_d.rs");
include!("crossmod_toplet_bridge.rs");
include!("continuation_lift.rs");
