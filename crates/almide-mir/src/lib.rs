//! almide-mir — the Almide v1 Middle IR: the single source of truth for
//! ownership and layout.
//!
//! See docs/roadmap/active/v1-mir-architecture.md.
//!
//! # Constitution (§1)
//! Ownership and layout are decided ONCE, here. Renderers (Rust, wasm) only
//! translate the decision; they NEVER re-decide it. A renderer that recomputes
//! `dup`/`drop`/`borrow`/`Repr`/`MakeUnique` is a bug (the #643 class).
//!
//! # Flight-grade (§5)
//! This crate is the #529 WasmIR vehicle. The ownership model below is the
//! normative semantics (#563/#564); [`verify_ownership`] is the EXECUTABLE form
//! of the ownership invariant destined for Lean certification (#575/#576). To
//! stay auditable for DO-178C / DO-333 qualification this crate is:
//!   - `unsafe`-free (`#![forbid(unsafe_code)]`),
//!   - TOTAL — every `match` is exhaustive with no silent catch-all (a dropped
//!     case is a verification hole, the codegen-traversal-totality lesson),
//!   - free of unnamed magic numbers (scalar widths are named constants).
//!
//! This first brick is the data model + the ownership verifier. The
//! Core-IR→MIR lowering and the two renderers are subsequent bricks; they are
//! built fresh and judged against the existing compiler + the semantic-law
//! oracle (the v1 dual-oracle, §6).

#![forbid(unsafe_code)]

pub mod certificate;
pub mod lower;
pub mod purity;
pub mod render_rust;
pub mod render_wasm;
pub mod translation_validation;

use std::collections::{BTreeMap, BTreeSet};

// ───────────────────────────── Layout / Repr ──────────────────────────────

/// A scalar's byte width — a VALUE OBJECT, not a raw number. Magic widths are
/// structurally impossible: you write `ScalarWidth::Word`, never `4`. The byte
/// count is recovered via [`ScalarWidth::bytes`] where layout needs it (so the
/// relationship "Word = 4 bytes" lives in exactly one place).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum ScalarWidth {
    /// 1 byte (`Int8`/`UInt8`).
    Byte,
    /// 2 bytes (`Int16`/`UInt16`).
    Half,
    /// 4 bytes (`Int32`/`UInt32`/`Float32`, and `Bool`'s ABI slot).
    Word,
    /// 8 bytes (`Int`/`Int64`/`UInt64`/`Float`/`Float64`).
    Double,
}

impl ScalarWidth {
    /// The byte count — the ONLY place a `ScalarWidth` becomes a number.
    pub const fn bytes(self) -> u8 {
        match self {
            ScalarWidth::Byte => 1,
            ScalarWidth::Half => 2,
            ScalarWidth::Word => 4,
            ScalarWidth::Double => 8,
        }
    }
}

/// A value's runtime representation — the LAYOUT decision (§2.1), decided once.
///
/// `Scalar` values are `Copy` and carry no refcount (no `dup`/`drop`).
/// `Ptr`/`Boxed` values are reference-counted heap pointers; only these
/// participate in ownership accounting.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Repr {
    /// A `Copy` scalar (Int/Float/Bool/narrow ints) of a named [`ScalarWidth`].
    Scalar { width: ScalarWidth },
    /// A reference-counted heap pointer to a value laid out by `layout`.
    Ptr { layout: LayoutId },
    /// Like [`Repr::Ptr`] but BOXED for a recursive type. Renders as `Box<T>`
    /// on Rust and a bare pointer on wasm; reading THROUGH the box is a
    /// [`Op::Borrow`], never a consume (the #610 / gate shape-3 decision).
    Boxed { layout: LayoutId },
}

impl Repr {
    /// Heap-managed values carry a refcount and need `dup`/`drop`; scalars do
    /// not. This single predicate replaces the duplicated `is_heap_type`
    /// (pass_perceus + emit_wasm/statements, hand-copied today).
    pub fn is_heap(self) -> bool {
        matches!(self, Repr::Ptr { .. } | Repr::Boxed { .. })
    }
}

/// A handle into the layout registry (header size, field offsets, tag
/// placement, element stride). The inner id is PRIVATE so a bare `LayoutId(0)`
/// cannot be written anywhere — heap values get [`PLACEHOLDER_LAYOUT`] or a
/// registry-issued id (a later brick), never an ad-hoc number.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub struct LayoutId(u32);

impl LayoutId {
    /// Construct a layout id (only the layout registry should call this).
    pub(crate) const fn new(id: u32) -> Self {
        LayoutId(id)
    }
}

/// The layout id every heap value carries until the layout pass assigns real
/// ids (a later brick) — the single sanctioned placeholder.
pub const PLACEHOLDER_LAYOUT: LayoutId = LayoutId::new(0);

/// An SSA-like MIR value (a local). Identity is the id; its [`Repr`] is fixed
/// at definition and never re-decided downstream.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub struct ValueId(pub u32);

// ──────────────────────────── Ownership nodes ─────────────────────────────

/// How a freshly [`Op::Alloc`]'d value is initialized — the COMPUTATION the
/// ownership skeleton carries. The value-semantics subset only needs integer
/// lists; richer initializers arrive with later bricks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Init {
    /// No concrete initializer — an ownership-only skeleton (not renderable to a
    /// running program; used by the ownership-shape tests).
    Opaque,
    /// A `List[Int]` literal.
    IntList(Vec<i64>),
    /// A string literal's UTF-8 bytes — real DATA the EXECUTION render needs to
    /// reproduce the value (the ownership cert is unaffected: an `Alloc` is one `i`
    /// regardless of content). The un-defer of string data, the first ③ slice.
    Str(String),
    /// A DYNAMICALLY-sized, runtime-allocated String of `len` bytes (a ValueId) — an
    /// OWNED, rc=1, empty-data block the caller fills via `prim.store8`. The ownership
    /// cert is the SAME one `i` as any `Alloc` (init-agnostic), so NO checker change: it
    /// is a fresh owned object, moved out / dropped like a literal. This is the primitive
    /// the self-hosted `int.to_string` (and string-builders) allocate their result with.
    DynStr { len: ValueId },
    /// A materialized `Some(payload)` — Option modeled as a 0-or-1-element LIST block
    /// (the proven list layout `[rc][len@4][cap@8][data@12]`): `Some(x)` is a 1-element
    /// list (len=1, `data[0]`=x), `None` is the 0-element list (`Init::Opaque`, len=0).
    /// The tag IS the length, so a variant `match` reads `len` and extracts `data[0]`.
    /// SCALAR payload only (a heap payload would alias the element — a later refinement).
    /// The ownership cert is the SAME one `i` as any `Alloc` (init-agnostic), so NO
    /// checker change: a fresh owned object, moved out / dropped like a literal.
    OptSome { payload: ValueId },
    /// A materialized `None` — the 0-element Option (len=0, the tag), but allocated with the
    /// SAME physical size as `OptSome` (cap=1 + headroom). Sizing it identically to `Some`
    /// is what lets the size-bucketed `$alloc` free-list REUSE a block between `Some` and
    /// `None` results (a closure returning `(Int) -> Option[Int]` alternates them — distinct
    /// sizes would fragment the head-only free-list and grow memory). len=0 still reads as
    /// `None`; the spare slot is unused. Init-agnostic `i` cert (no checker change).
    OptNone,
    /// A DYNAMICALLY-sized, runtime-allocated `List[Int]` of `len` (a ValueId) i64-element
    /// slots — an OWNED, rc=1 block (len = cap = `len`, `LIST_HEADER + len*ELEM_SIZE`
    /// bytes), filled by the caller via `prim.store64`. The list-building sibling of
    /// `DynStr`; the ownership cert is the SAME one `i` as any `Alloc` (init-agnostic), so
    /// NO checker change. List[Int] elements are i64 values (no nested heap ownership).
    DynList { len: ValueId },
    /// A DYNAMICALLY-sized OWNED `List[String]` of `len` slots — physically identical to
    /// `DynList` (the slots hold i64-widened String handles), but the value is tracked as a
    /// NESTED-OWNERSHIP list: each element handle stored into it is `Consume`d (owned by the
    /// list), and a scope-end drop is an [`Op::DropListStr`] (recursive free), not a flat
    /// `Drop`. The ownership cert is the SAME one `i` as any `Alloc` (init-agnostic). This is
    /// the Machinery-2 allocation for string.split / lines / chars and List[String] results.
    DynListStr { len: ValueId },
}

/// One MIR statement. Ownership is EXPLICIT: a heap value's refcount is changed
/// only by [`Op::Alloc`]/[`Op::Dup`] (+1) and [`Op::Drop`]/[`Op::Consume`]
/// (−1). The renderers SPELL these (`__rc_inc`/`.clone()`, `__rc_dec`/scope
/// drop, ptr-transfer/move); they never compute where they go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    /// `dst = alloc(repr, init)` — a fresh owned heap value with refcount 1. The
    /// only +1 besides [`Op::Dup`]. `repr` must be a heap repr.
    Alloc { dst: ValueId, repr: Repr, init: Init },
    /// `dst = <scalar>` — a `Copy` value whose CONTENT is DEFERRED (a placeholder;
    /// no refcount, no ownership). Renders to nothing — the local stays the wasm
    /// zero default. Used where the scalar value is not yet computed by lowering.
    Const { dst: ValueId },
    /// `dst = <int literal>` — a materialized integer constant (`Copy`, no
    /// ownership). The value-carrying counterpart of [`Op::Const`]: renders to
    /// `(local.set $dst (i64.const value))`. Lets a self-hosted runtime fn compute
    /// real addresses/lengths (the scalar-value foundation for the prim floor).
    ConstInt { dst: ValueId, value: i64 },
    /// `dst = dup src` — `dst` is a NEW handle (a distinct variable) denoting
    /// the SAME heap OBJECT as `src`, acquiring one additional owned reference
    /// (+1 on the object). The single decision for "this binding aliases a
    /// still-live value" (Rust `let dst = src.clone()`, wasm `__rc_inc`).
    /// Handle ≠ object: `src` and `dst` are distinct [`ValueId`]s (so a renderer
    /// can name two variables) that share one refcounted object.
    Dup { dst: ValueId, src: ValueId },
    /// `drop v` — release one owned reference (−1); at 0 the value is freed
    /// (Rust scope-end drop, wasm `__rc_dec`).
    Drop { v: ValueId },
    /// `drop_list_str v` — release a `List[String]` (a list whose i64 slots hold OWNED
    /// String handles): a RECURSIVE drop. Same cert event as [`Op::Drop`] (one `−1`/`d` on
    /// the LIST object — the elements were already accounted as `m`/consumed when stored into
    /// it), but the RENDER, IFF this is the last reference (rc==1), first `rc_dec`s each
    /// element handle, THEN `rc_dec`s the list (so a shared list's aliases don't free the
    /// elements early). The nested-ownership counterpart of `Drop` for Machinery 2.
    DropListStr { v: ValueId },
    /// `drop_value v` — release a dynamic `Value` (the Codec data model). A scalar Value
    /// (Null/Bool/Int/Float, tag < 4) owns NO heap payload, so this frees just the block; a
    /// heap-payload Value (Str/Array/Object, tag ≥ 4) owns ONE handle at +12, freed first IFF
    /// this is the last reference (rc==1). Same cert event as [`Op::Drop`] (one `−1`/`d` on the
    /// Value object — the payload was accounted `m`/consumed when stored into it). The
    /// RUNTIME-TAG-DISPATCHED counterpart of `Drop` for the Value type.
    DropValue { v: ValueId },
    /// `drop_list_value v` — release a `List[Value]` (a list whose i64 slots hold OWNED dynamic
    /// `Value` handles, each itself possibly a heap-payload Str/Array). A flat `DropListStr` would
    /// `rc_dec` each slot's Value block WITHOUT freeing that Value's own nested payload (its String,
    /// or an Array's element Values) — a LEAK. So the RENDER, IFF this is the last reference (rc==1),
    /// calls the recursive `$__drop_value` on each element (which tag-dispatches), THEN frees the
    /// list block. Same cert event as [`Op::Drop`] (one `−1`/`d` on the LIST object — the element
    /// Values were accounted `m`/consumed when stored). The Value-element counterpart of
    /// `DropListStr` (which is for String elements, whose `rc_dec` IS their full free).
    DropListValue { v: ValueId },
    /// `drop_list_str_value v` — release a `List[(String, Value)]` whose element slots hold owned
    /// (String, Value) TUPLE blocks (the yaml `pairs` shape). The render calls the recursive
    /// `$__drop_list_str_value`: at the list's last ref each tuple is freed at its own last ref (its
    /// String slot rc_dec'd flat, its Value slot freed recursively via `$__drop_value`), then the tuple,
    /// then the list block. Same single cert `d` as [`Op::Drop`]; the per-tuple recursion is the trusted
    /// routine (empty cert, leak-loop verified). The TUPLE-element counterpart of `DropListValue`.
    DropListStrValue { v: ValueId },
    /// `drop_result_lv v` — release a `value.as_array` Result `Result[List[Value], String]` (the
    /// cap-as-tag 1-slot block `[rc][len@4=1][cap@8][@12 payload][@16 tag]`). IFF the last reference
    /// (rc==1), the RENDER tag-dispatches on @16: Ok (0) frees the `List[Value]` payload @12
    /// RECURSIVELY (`$__drop_list_value`), Err (1) frees the String @12 (`rc_dec`); THEN the block.
    /// A flat `DropListStr` would only rc_dec @12 (the list block), LEAKING its element Values. Same
    /// cert event as [`Op::Drop`] (one `−1`/`d` on the Result object — its payload was `m`/consumed).
    DropResultListValue { v: ValueId },
    /// `drop_variant v : ty` — release a CUSTOM variant (user ADT) block whose ctor fields may be
    /// nested variant/heap handles (`Add(Expr, Expr)`). A flat `Drop`/`DropListStr` would `rc_dec`
    /// the block (and masked slots) WITHOUT recursively freeing a child variant's OWN nested fields
    /// — a LEAK. So the RENDER calls the GENERATED per-type recursive free `$__drop_<ty>` (the
    /// `$__drop_value` shape: at the last ref read the tag, recursively free each variant field +
    /// `rc_dec` each leaf field, then the block). Same single cert `d` as [`Op::Drop`]; the recursion
    /// is the trusted routine (the generated fn is `prim`-only ⇒ empty ownership cert, leak-loop
    /// verified). The custom-ADT counterpart of `DropValue` (ADT brick 5b).
    DropVariant { v: ValueId, ty: String },
    /// `consume v` — transfer v's reference OUT (into a container, a return, or
    /// a callee that takes ownership). v is dead here; the reference lives on
    /// elsewhere. Renders as a move (Rust) / ptr-transfer with no inc (wasm).
    Consume { v: ValueId },
    /// `borrow v` — read v without changing its refcount (Rust `&v`, wasm a
    /// pointer load). Reading through a [`Repr::Boxed`] is this, not a consume.
    Borrow { v: ValueId },
    /// `make_unique v` — ensure v is uniquely owned before an in-place
    /// mutation (clone-on-shared). Renders as `.clone()`-on-alias (Rust) /
    /// `__cow_check` (wasm). The AliasCow / gate shape-5 decision.
    MakeUnique { v: ValueId },
    /// `dst = pure(uses…)` — a computation that BORROWS its inputs and defines
    /// a scalar `dst` (e.g. `list.len`). Heap results are produced by
    /// [`Op::Alloc`]. Keeps the op set total without a catch-all.
    Pure { dst: ValueId, uses: Vec<ValueId> },

    /// Call a (self-hosted) RUNTIME function — the boundary between the tiny MIR
    /// PRIMITIVE set (alloc/load/store/Dup/Drop/…) the renderers hand-map, and
    /// everything else, which is a runtime function (§4.1). The renderers emit a
    /// call; the function's BODY is provided by the runtime (today a bootstrap
    /// hand-written one, ultimately Almide compiled through this same path). A
    /// renderer never re-implements a runtime operation inline — that is the
    /// discipline that keeps the hand-written wasm surface tiny.
    Call { dst: Option<ValueId>, func: RtFn, args: Vec<CallArg>, result: Option<Repr> },

    /// Call a USER/runtime MIR function by name (the mechanism that lets the
    /// runtime be self-hosted: a runtime fn is just a [`MirFunction`] called
    /// here). `dst` binds the result; `result` is its [`Repr`] — `Some(heap)`
    /// marks a FRESH OWNED heap value (the callee allocated it and moved it out
    /// to the caller, who now owns it: a +1, like [`Op::Alloc`]). This is the
    /// callee's RETURN-mode signature, read at the call site WITHOUT opening the
    /// callee (the compositionality lever for ownership).
    CallFn { dst: Option<ValueId>, name: String, args: Vec<CallArg>, result: Option<Repr> },

    /// Call a CLOSURE VALUE indirectly: `dst = (table[table_idx])(args)` — the
    /// function-value invocation `(f)(x)` the higher-order self-host (list.map/filter/
    /// fold) needs, lowered to wasm `call_indirect`. `table_idx` is a scalar (the closure
    /// value = a function-table index). SOUNDNESS-CRITICAL for caps: the callee is an
    /// UNANALYZABLE closure, so [`crate::certificate::cap_witness`] treats this op as
    /// reaching EVERY capability (conservative `used ⊇ all`) — a fn with a `CallIndirect`
    /// is therefore caps-VERIFIED only if it DECLARES the cap, never silently (a closure
    /// that reaches Stdout could otherwise pass un-witnessed = accept-but-unsafe). Args are
    /// borrowed/moved like a `CallFn`; a heap result is a fresh owned value.
    CallIndirect { dst: Option<ValueId>, table_idx: ValueId, args: Vec<CallArg>, result: Option<Repr> },

    /// `dst = the function-table slot of the lifted function `name`` — a scalar index
    /// (carried in the i64-uniform value) used as a `CallIndirect.table_idx`. The render
    /// resolves `name` to its position in the module function table. This materializes a
    /// lifted lambda's value (the closures-machinery binding for `let f = (x) => …`). No
    /// ownership (a scalar constant); no capability (the dispatch site taints, not this).
    FuncRef { dst: ValueId, name: String },

    /// `dst = a <op> b` on scalars (no ownership) — the arithmetic runtime
    /// functions need.
    IntBinOp { dst: ValueId, op: IntOp, a: ValueId, b: ValueId },

    /// A PRIMITIVE FLOOR op — raw memory / host access the self-hosted runtime needs,
    /// below the language (`prim.load32`/`prim.store32`/`prim.fd_write`/…). The
    /// renderers hand-map it INLINE (no preamble `(func …)`), and it is a CLOSED set
    /// accounted as the trusted floor (like the RC primitives), small/total enough to
    /// prove faithful to the wasm spec. The MIR is i64-uniform; the i32 wasm memory
    /// boundary wraps/extends at the op. `args` are scalar/handle inputs; `dst` binds
    /// a scalar result (loads, fd_write, handle→address). No ownership: scalars carry
    /// none and a handle arg is BORROWED (read, no refcount change).
    /// [`PrimKind::FdWrite`] reaches [`Capability::Stdout`] (the only sandbox exit).
    Prim { kind: PrimKind, dst: Option<ValueId>, args: Vec<ValueId> },

    /// Structured control flow as FLAT MARKERS. `IfThen` begins an `if` on a Bool
    /// scalar `cond` (i64 0/1); the ops up to [`Op::Else`] are the THEN arm, the ops
    /// up to [`Op::EndIf`] are the ELSE arm. Only the TAKEN arm executes (the render
    /// emits a wasm `if`/`else`), but BOTH arms are PER-ARM-BALANCED by the lowering,
    /// so the cert processes the arm ops FLAT — the same sound linearization it already
    /// proves; the markers themselves carry no ownership. A scalar result `dst` is
    /// bound from `then_val` / `else_val` (the arm values left on the wasm stack).
    IfThen { cond: ValueId, dst: Option<ValueId> },
    /// Separates the THEN arm from the ELSE arm; `val` is the THEN arm's result value
    /// (left on the wasm stack) for a scalar `if`.
    Else { val: Option<ValueId> },
    /// Closes the `if`; `val` is the ELSE arm's result value for a scalar `if`.
    EndIf { val: Option<ValueId> },

    /// A loop as FLAT MARKERS (scalar-state loops). `LoopStart` opens a wasm
    /// `(block (loop`; the cond is evaluated INSIDE the loop and [`Op::LoopBreakUnless`]
    /// exits when it is false (a `br_if` of the outer block on `i64.eqz cond`); the body
    /// ops follow; [`Op::LoopEnd`] closes with a back-edge (`br` the loop). The markers
    /// carry NO ownership and the body ops are PER-ITERATION-BALANCED by the lowering, so
    /// the cert verifies ONE balanced iteration — sound for ANY N runtime iterations (each
    /// is the same balanced episode, exactly the existing model-one-iteration argument).
    /// Restricted to scalar state: a mutable loop var is a stable i64 local reassigned via
    /// [`Op::SetLocal`].
    LoopStart,
    /// Inside a loop: exit when the Bool scalar `cond` (i64 0/1) is false.
    LoopBreakUnless { cond: ValueId },
    /// Closes the loop with a back-edge to its top.
    LoopEnd,
    /// Reassign a mutable SCALAR local: `local := src` (a stable i64 wasm local re-written
    /// — the loop-carried state). No ownership (scalar copy); `local` was already defined
    /// by its `var` bind, `src` is the freshly computed value.
    SetLocal { local: ValueId, src: ValueId },
}

/// The closed set of primitive-floor operations (the trusted, wasm-spec-faithful
/// surface the self-hosted runtime is written over).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimKind {
    /// Reinterpret a heap handle (i32 pointer) as an i64 address value — the
    /// String/List→Int bridge so all address math is `Int` `IntBinOp`.
    Handle,
    /// Load `width` bytes (1/4/8) at a computed i64 address, zero-extended to i64.
    Load { width: u8 },
    /// Load a 4-byte i32 HANDLE at a computed i64 address — UNLIKE `Load { width: 4 }`, the
    /// result keeps the `Ptr` (i32) repr (no i64 zero-extend), so it IS a heap handle a caller
    /// can pass to a String/List consumer. The bridge for extracting a heap element from a slot
    /// (a `match Some(s)` payload / a `List[String]` element). A borrowed alias — no ownership.
    LoadHandle,
    /// Store the low `width` bytes (1/4/8) of an i64 value at a computed i64 address.
    Store { width: u8 },
    /// Bounds-checked element ADDRESS for a direct `xs[i]` index — `args = [list_handle,
    /// index]` (both i64-uniform: the handle reinterpreted to an address, the index a scalar
    /// i64), dst = the i64 element-slot address `list + LIST_HEADER + idx*ELEM_SIZE`. Renders
    /// `(call $elem_addr ...)` (the SAME preamble helper v0's `$list_get`/`$list_set` use), so a
    /// negative or `>= cap` index TRAPS (the controlled-halt bounds wall) instead of reading
    /// outside the block — v0's `a[i]` likewise halts on OOB (it prints `index out of bounds`
    /// and exits 1; this traps). For an in-bounds index the loaded element byte-matches v0. A
    /// scalar address computation, no ownership (a no-op in verify_ownership like every Prim).
    ElemAddr,
    /// The `fd_write` WASI host call — `args = [fd, iov, count, nwritten]`, dst = the
    /// i64 errno. The ONLY sandbox exit; carries [`Capability::Stdout`].
    FdWrite,
    /// Release one reference of a RAW heap handle (`(call $rc_dec …)`), the inverse of [`RcInc`].
    /// The MECHANISM the self-hosted recursive `value.__drop_value` frees a dynamic Value tree with
    /// (the §4.1-compliant alternative to a hand-written WAT drop): it operates on raw Int handles,
    /// so its ownership cert is EMPTY (a `Prim` is a no-op in verify_ownership) — like `string_eq`.
    /// REUSES the proven `$rc_dec` (no new WAT func). args = [addr], no dst (Unit). TRUSTED like the
    /// inline DropListStr's per-element rc_dec — its leak/double-free safety is the differential
    /// test's burden (a value.stringify round-trip), NOT the ownership cert. Use is contained to the
    /// drop routine.
    RcDec,
    /// Acquire one reference of a RAW heap handle (`(call $rc_inc …)`) — the self-host `value.array`
    /// SHALLOW-COPIES a `List[Value]` by `rc_inc`-ing each element into a new owned list (matching
    /// v0's `items.clone()` observably) so the borrowed `items` param is untouched. args = [addr],
    /// no dst. REUSES the proven `$rc_inc`. Cert no-op (raw handle), trusted like RcDec.
    RcInc,
    /// The FLOAT floor: a `Float` scalar is the i64-uniform value holding the f64 BITS, so
    /// every float op `reinterpret`s i64→f64, computes, and `reinterpret`s back (a compare /
    /// to-int yields a real i64). Scalar, no ownership — the cert is untouched (these are
    /// `Op::Prim`, no-ops in verify_ownership). This opens the whole `float.*` / `math.*`
    /// f64 category for self-host over `prim.fabs` / `prim.fadd` / `prim.f2i` / etc.
    FloatUn(FUnOp),
    FloatBin(FBinOp),
    FloatCmp(FCmpOp),
    /// `i64.trunc_sat_f64_s(reinterpret(x))` — Float → Int (saturating truncate, v0's `as i64`).
    FloatToInt,
    /// `reinterpret(f64.convert_i64_s(x))` — Int → Float.
    IntToFloat,
    /// IDENTITY — the raw f64↔i64 BIT reinterpret (`float.to_bits` / `int.bits_to_float`):
    /// the i64-uniform value ALREADY holds the f64 bits, so this is a no-op pass-through.
    FloatBits,
    /// `f32.demote_f64` — Float (f64) → Float32. The narrower f32 value is held as its 32-bit
    /// pattern in the LOW half of the i64 slot (`i32.reinterpret_f32` then zero-extend). Rounds to
    /// nearest, matching Rust's `n as f32`.
    F32Demote,
    /// `f64.promote_f32` — Float32 → Float (f64). Reads the low-32 f32 pattern (`i32.wrap_i64`
    /// then `f32.reinterpret_i32`) and widens exactly.
    F32Promote,
    /// `f32.convert_i64_s` — Int → Float32 directly (single rounding), matching Rust's `n as f32`.
    /// Result is the f32 pattern in the low half of the i64 slot.
    IntToF32,
    /// IDENTITY — Float32 → its 32-bit pattern as an Int. A Float32 value ALREADY holds the f32
    /// bits in the low 32 of the i64 slot (high 32 zero, from F32Demote/IntToF32's zero-extend), so
    /// this is a type-only reinterpret (no-op pass-through), like FloatBits for f64.
    F32Bits,
}

/// A unary f64 op (the value is the f64 bits in an i64; render reinterprets around it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FUnOp {
    Abs,
    Sqrt,
    Floor,
    Ceil,
    Neg,
}

/// A binary f64 op.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FBinOp {
    Add,
    Sub,
    Mul,
    Div,
    Min,
    Max,
    /// `f64.copysign(a, b)` — magnitude of `a` with the sign bit of `b` (the basis for an
    /// exact `f64::signum`: `copysign(1.0, x)`, with NaN handled by the caller).
    CopySign,
}

/// An f64 comparison — yields an i64 0/1 (the Bool / `if` condition).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FCmpOp {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

/// A scalar integer binary operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntOp {
    Add,
    Sub,
    Mul,
    /// Signed division — traps on divide-by-zero (matching v0's checked `DivInt`).
    Div,
    /// Signed remainder — traps on divide-by-zero (matching v0's checked `ModInt`).
    Mod,
    // Comparisons: produce a Bool scalar (i64 0/1) — the `if` condition. Signed.
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    // Bitwise i64 ops (the int.band/bor/bxor/bshl/bshr floor). Scalar, no ownership.
    And,
    Or,
    Xor,
    Shl,
    /// Arithmetic (sign-extending) shift right, matching v0's `>>` on `i64`.
    Shr,
    /// LOGICAL (zero-filling) shift right (`i64.shr_u`) — for unsigned/bit-width ops like
    /// int.rotate_* which shift the value as a u64. The shift amount is wasm-masked to 0..63.
    ShrU,
}

/// A runtime function the MIR can call. An enum (not a string) so the renderer
/// mapping is TOTAL and the runtime surface is a closed, auditable set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RtFn {
    /// `list[index] = value` in place (after a [`Op::MakeUnique`]).
    ListSet,
    /// push a value onto a list in place (after a [`Op::MakeUnique`]); the
    /// result is rebound to `dst` (the buffer may move).
    ListPush,
    /// `println` a list as `label=e0,e1,…`.
    PrintList,
    /// `println` a scalar integer.
    PrintInt,
    /// `println` a heap string (the value-semantics subset's string print). A
    /// WITNESS-LEVEL primitive today: it carries the ownership (borrows the
    /// string handle) and capability ([`Capability::Stdout`]) facts the proven
    /// checker re-verifies, but the renderers do NOT lower it yet — strings are
    /// `Init::Opaque` skeletons in this subset (no content bytes), so a faithful
    /// `print_str` render awaits the string-content lowering brick. Until then a
    /// renderer asked to emit it refuses LOUDLY (the catch-all panic), never
    /// silently — the flight-grade totality rule.
    PrintStr,
}

impl RtFn {
    /// The host [`Capability`] this runtime function reaches, if any. Pure heap
    /// ops touch no host effect; the print ops reach [`Capability::Stdout`]. This
    /// is the SINGLE mapping the capability witness derives "used capabilities"
    /// from — exhaustive, so a new effectful runtime fn cannot silently escape
    /// the sandbox accounting.
    pub const fn capability(self) -> Option<Capability> {
        match self {
            RtFn::ListSet | RtFn::ListPush => None,
            RtFn::PrintList | RtFn::PrintInt | RtFn::PrintStr => Some(Capability::Stdout),
        }
    }
}

/// A host CAPABILITY a function may reach — the unit of the sandbox promise
/// (the 4th flight-grade property, proofs/CapabilityBound.v: a program reaches
/// ONLY the capabilities it declares). A VALUE OBJECT, not a raw id: you write
/// `Capability::Stdout`, never `0`. The stable registry id the proven checker
/// compares is recovered via [`Capability::id`], so the "Stdout = 0" mapping
/// lives in exactly ONE place and MUST match the Coq capability registry. The
/// set is closed and grows only as the runtime gains host effects (fs, net, …).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub enum Capability {
    /// Writing to standard output (the only host effect the current MIR subset
    /// reaches, via [`RtFn::PrintInt`] / [`RtFn::PrintList`]).
    Stdout,
}

impl Capability {
    /// The stable registry id — the ONLY place a `Capability` becomes a number.
    /// MUST agree with proofs/CapabilityBound.v's registry (Stdout = 0).
    pub const fn id(self) -> u32 {
        match self {
            Capability::Stdout => 0,
        }
    }
}

/// An argument to a runtime [`Op::Call`] / user [`Op::CallFn`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallArg {
    /// A heap handle (borrowed by the call — live-checked, refcount unchanged).
    Handle(ValueId),
    /// A scalar value (a `ValueId` of scalar Repr — no ownership).
    Scalar(ValueId),
    /// An immediate integer (index / value).
    Imm(i64),
    /// An immediate string (a print label).
    Label(String),
}

/// A function parameter: a value the caller supplies, with its [`Repr`]. A heap
/// param is BORROWED (the v1 calling convention): the CALLER retains ownership
/// and releases it; the callee gets a live handle but no owned reference. So a
/// param contributes NO `+1` to the ownership certificate — an owned-param `+1`
/// would be synthetic (no runtime `Alloc`/`rc_inc` backs it), the gate-blind
/// use-after-free class. A body that needs to consume or return a param must
/// first `Dup` it (acquire its own reference). A scalar param carries no
/// ownership. (Per-param move-mode signatures are a later refinement.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MirParam {
    pub value: ValueId,
    pub repr: Repr,
}

/// A MIR function: params, a flat ownership-explicit op sequence, and an
/// optional returned value (moved out — a [`Op::Consume`] of `ret` is implied at
/// the boundary).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct MirFunction {
    pub name: String,
    pub params: Vec<MirParam>,
    pub ops: Vec<Op>,
    pub ret: Option<ValueId>,
    /// The host [`Capability`]s this function is PERMITTED to reach (its effect
    /// signature, lowered). The capability witness checks the capabilities the
    /// body actually uses against this declared bound — accept ⟹ no undeclared
    /// host effect (proofs/CapabilityBound.v). Empty = a pure/sandboxed function.
    pub declared_caps: Vec<Capability>,
    /// RENDER-ONLY side table: a value → the i64-SLOT INDICES that hold an OWNED heap
    /// handle, for a MIXED scalar+heap record/tuple block (e.g. `R { name: String, n: Int }`
    /// = `[0]`). It refines the recursive free of an [`Op::DropListStr`] on such a value:
    /// instead of the uniform "free EVERY slot" loop (correct only for a homogeneous
    /// `List[String]`), the render frees exactly these slots, then the block. A value
    /// ABSENT from this table keeps the uniform-loop behavior (`List[String]` / all-heap
    /// aggregate). This carries NO ownership semantics — the certificate sees a `DropListStr`
    /// as the SAME single `d` regardless (each heap field was already accounted `m`/consumed
    /// at its move-in store), exactly as for `List[String]`. So it is a pure rendering
    /// refinement (like the `DropValue` tag dispatch) — NOT a new op or certificate event.
    pub heap_slot_masks: BTreeMap<ValueId, Vec<usize>>,
}

/// A whole MIR program.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct MirProgram {
    pub functions: Vec<MirFunction>,
}

// ─────────────────────────── Ownership verifier ───────────────────────────
//
// The executable ownership invariant (#575/#576). A symbolic refcount
// interpretation over the ops: every heap value's owner count must return to 0
// (every reference dropped or moved out), never go negative (double-free), and
// never be used after it reaches 0 / is moved (use-after-free / -move).

/// What an ownership violation is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ViolationKind {
    /// A `drop` of a value whose owner count is already 0.
    DoubleFree,
    /// A `dup`/`borrow`/`make_unique`/`pure`-use of a freed value.
    UseAfterFree,
    /// A `consume` of a value already moved out (count 0).
    UseAfterMove,
    /// A heap value still owned (count > 0) at function end.
    Leak,
}

/// A located ownership violation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Violation {
    /// Index into `func.ops`; equals `ops.len()` for an end-of-function leak.
    pub op_index: usize,
    pub value: ValueId,
    pub kind: ViolationKind,
}

/// Verify the ownership invariant for one function.
///
/// Returns `Ok(())` if the MIR is balanced (the by-construction guarantee the
/// renderers rely on), or every violation found (deterministic order). This is
/// the MIR-level analogue of the Perceus belt's IR check, but it is the SINGLE
/// source — there is no second hand-written copy in a renderer to drift from.
pub fn verify_ownership(func: &MirFunction) -> Result<(), Vec<Violation>> {
    // Handle ≠ object. Each known heap HANDLE (ValueId) maps to its OBJECT (the
    // `Alloc`'d representative ValueId); the refcount is per OBJECT. A handle is
    // also tracked LIVE/dead, so a use of a handle after its own drop/consume is
    // caught even when the object lives on through a sibling handle.
    let mut object_of: BTreeMap<ValueId, ValueId> = BTreeMap::new();
    let mut rc: BTreeMap<ValueId, i64> = BTreeMap::new(); // keyed by object — OUR (callee's) owned refs
    let mut dead: BTreeMap<ValueId, bool> = BTreeMap::new(); // keyed by handle
    let mut violations: Vec<Violation> = Vec::new();

    // Heap params are BORROWED by default (the v1 calling convention): the CALLER
    // owns the reference and releases it at its own scope end; the callee gets a
    // LIVE handle but holds NO owned reference of its own (its rc starts at 0).
    // This is the exact dual of the certificate omitting the param's `i` event —
    // an owned-param `+1` would be SYNTHETIC (no `Alloc`/`rc_inc` backs it), the
    // gate-blind use-after-free class. A body that wants to consume or return a
    // param must first `Dup` it (acquire its own ref); a release with rc 0 (the
    // `borrowed` object, never `Dup`'d) fails — exactly the cert's `d`/`m` at
    // rc 0, which the proven checker faults.
    let mut borrowed: BTreeSet<ValueId> = BTreeSet::new();
    for p in &func.params {
        if p.repr.is_heap() {
            object_of.insert(p.value, p.value);
            dead.insert(p.value, false);
            borrowed.insert(p.value);
        }
    }

    for (i, op) in func.ops.iter().enumerate() {
        match op {
            Op::Alloc { dst, repr, .. } => {
                debug_assert!(repr.is_heap(), "Alloc of a non-heap repr is malformed MIR");
                object_of.insert(*dst, *dst);
                rc.insert(*dst, 1);
                dead.insert(*dst, false);
            }
            Op::Const { dst: _ } | Op::ConstInt { .. } => {
                // A scalar — no ownership accounting.
            }
            Op::FuncRef { .. } => {
                // A function-table slot index — a scalar constant, no ownership.
            }
            Op::Dup { dst, src } => {
                if let Some(o) = live_object(&object_of, &rc, &dead, &borrowed, *src) {
                    // Acquire OUR own reference. A `Dup` of a borrowed param has no
                    // prior rc entry (we owned none) — start it at 0, then +1.
                    *rc.entry(o).or_insert(0) += 1;
                    object_of.insert(*dst, o);
                    dead.insert(*dst, false);
                } else {
                    violations.push(violation(i, *src, ViolationKind::UseAfterFree));
                }
            }
            // A `DropListStr`/`DropListValue` releases the LIST object exactly like a `Drop` (the
            // recursive element free is a RENDER concern, gated on rc==1; the cert sees one −1 on the
            // list — its elements were `Consume`d into it when stored).
            Op::Drop { v }
            | Op::DropListStr { v }
            | Op::DropValue { v }
            | Op::DropListValue { v }
            | Op::DropListStrValue { v }
            | Op::DropResultListValue { v }
            | Op::DropVariant { v, .. } => {
                match release(&object_of, &mut rc, &mut dead, &borrowed, *v) {
                    Ok(()) => {}
                    Err(()) => violations.push(violation(i, *v, ViolationKind::DoubleFree)),
                }
            }
            Op::Consume { v } => match release(&object_of, &mut rc, &mut dead, &borrowed, *v) {
                Ok(()) => {}
                Err(()) => violations.push(violation(i, *v, ViolationKind::UseAfterMove)),
            },
            Op::Borrow { v } | Op::MakeUnique { v } => {
                if live_object(&object_of, &rc, &dead, &borrowed, *v).is_none() {
                    violations.push(violation(i, *v, ViolationKind::UseAfterFree));
                }
            }
            Op::Pure { dst: _, uses } => {
                for v in uses {
                    // Only heap handles are accountable; scalar uses are absent
                    // from `object_of` and correctly skipped.
                    if object_of.contains_key(v)
                        && live_object(&object_of, &rc, &dead, &borrowed, *v).is_none()
                    {
                        violations.push(violation(i, *v, ViolationKind::UseAfterFree));
                    }
                }
            }
            // A runtime/user call BORROWS its heap-handle args (live-checked, no
            // refcount change). Immediate/label args carry no ownership. A call
            // whose `result` is a heap repr returns a FRESH OWNED value (the
            // callee allocated and moved it out — the return-mode signature): the
            // `dst` becomes a new owned object, like Alloc.
            Op::Call { args, dst, result, .. }
            | Op::CallFn { args, dst, result, .. }
            // A CallIndirect has the same ownership shape as a CallFn: its heap-arg handles
            // must be live, a heap result is a FRESH OWNED value. The `table_idx` is a
            // scalar closure value (no ownership).
            | Op::CallIndirect { args, dst, result, .. } => {
                for a in args {
                    if let CallArg::Handle(v) = a {
                        if live_object(&object_of, &rc, &dead, &borrowed, *v).is_none() {
                            violations.push(violation(i, *v, ViolationKind::UseAfterFree));
                        }
                    }
                }
                if let (Some(d), Some(r)) = (dst, result) {
                    if r.is_heap() {
                        object_of.insert(*d, *d);
                        rc.insert(*d, 1);
                        dead.insert(*d, false);
                    }
                }
            }
            // Scalar arithmetic — no ownership.
            // A scalar arithmetic op and a primitive-floor op carry no ownership: a
            // scalar result is Copy and a `Prim` handle arg is BORROWED (read only).
            // The if-markers carry no ownership either — the arm OPS (flat between the
            // markers) are processed normally, per-arm-balanced by the lowering.
            Op::IntBinOp { .. }
            | Op::Prim { .. }
            | Op::IfThen { .. }
            | Op::Else { .. }
            | Op::EndIf { .. }
            // Loop markers carry no ownership; the body ops between them are
            // per-iteration-balanced (verified flat, one iteration).
            | Op::LoopStart
            | Op::LoopBreakUnless { .. }
            | Op::LoopEnd => {}
            // `SetLocal` into a HEAP slot is a loop-carried REBIND (`acc = acc + [x]`):
            // the slot now aliases the source's object. The slot's OLD object was
            // released by a preceding `Drop` in the loop body, so rebinding makes the
            // slot LIVE again (= the new object), preserving the per-iteration invariant
            // (slot owns exactly one ref at the body's start and end) — exactly the
            // soundness condition proved in OwnershipChecker.v's `check_line_unroll_sound`
            // (a rc-preserving loop body is leak/double-free-free for any iteration
            // count). For a SCALAR src (the scalar-TCO loop var) `object_of` has no
            // entry, so this is a no-op, as before.
            Op::SetLocal { local, src } => {
                if let Some(o) = object_of.get(src).copied() {
                    object_of.insert(*local, o);
                    dead.insert(*local, false);
                }
            }
        }
    }

    // A heap return value is MOVED OUT to the caller. It must be a reference WE
    // own (an `Alloc`/call-result, or a `Dup` we acquired): releasing it transfers
    // our reference out. Returning a BORROWED param we never acquired (rc 0) would
    // give the caller a SECOND owner of the caller's own reference — a double-free.
    // `release` fails there (rc 0) and we record it, the dual of the cert's `m` at
    // rc 0 which the proven checker faults.
    if let Some(r) = func.ret {
        if object_of.contains_key(&r)
            && release(&object_of, &mut rc, &mut dead, &borrowed, r).is_err()
        {
            violations.push(violation(func.ops.len(), r, ViolationKind::UseAfterMove));
        }
    }

    // Leak check: every object's references must have left (dropped or moved).
    for (o, c) in &rc {
        if *c > 0 {
            violations.push(violation(func.ops.len(), *o, ViolationKind::Leak));
        }
    }

    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

fn violation(op_index: usize, value: ValueId, kind: ViolationKind) -> Violation {
    Violation { op_index, value, kind }
}

/// The object a handle denotes, iff the handle is live. A handle is live when it
/// is not yet dropped AND either WE hold a reference to its object (rc ≥ 1) OR
/// the object is a `borrowed` param the CALLER keeps alive for the call's
/// duration (a borrow is always valid against the caller's reference, even when
/// our own count is 0). `None` = dead/unknown handle, or a non-borrowed object
/// whose references have all left.
fn live_object(
    object_of: &BTreeMap<ValueId, ValueId>,
    rc: &BTreeMap<ValueId, i64>,
    dead: &BTreeMap<ValueId, bool>,
    borrowed: &BTreeSet<ValueId>,
    v: ValueId,
) -> Option<ValueId> {
    if dead.get(&v).copied().unwrap_or(true) {
        return None; // unknown handle or already dropped/consumed
    }
    let o = *object_of.get(&v)?;
    if borrowed.contains(&o) || rc.get(&o).copied().unwrap_or(0) >= 1 {
        Some(o)
    } else {
        None
    }
}

/// Release one reference held by handle `v` (drop or consume): mark the handle
/// dead and decrement OUR object's refcount. `Err(())` if `v` is not live, OR if
/// we hold no reference of our own to release (rc 0 — e.g. a `borrowed` param we
/// never `Dup`'d): freeing a reference we do not own is a double-free against the
/// caller, so it is rejected rather than silently underflowed.
fn release(
    object_of: &BTreeMap<ValueId, ValueId>,
    rc: &mut BTreeMap<ValueId, i64>,
    dead: &mut BTreeMap<ValueId, bool>,
    borrowed: &BTreeSet<ValueId>,
    v: ValueId,
) -> Result<(), ()> {
    match live_object(object_of, rc, dead, borrowed, v) {
        Some(o) if rc.get(&o).copied().unwrap_or(0) >= 1 => {
            *rc.get_mut(&o).expect("a held reference has a refcount") -= 1;
            dead.insert(v, true);
            Ok(())
        }
        _ => Err(()),
    }
}

// ──────────────────────────────── tests ───────────────────────────────────
//
// The Phase 0 decision gate (research/spike/v1-mir/) proved, in a standalone
// spike, that one ownership decision per shape renders faithfully to both
// idioms. Here those five shapes are encoded as REAL MIR and checked by the
// REAL verifier: the balanced skeleton verifies clean, and a renderer-style
// "re-decision" (a dropped Drop, a deep free, a consume-on-call) is caught.

#[cfg(test)]
mod tests {
    use super::*;

    fn v(n: u32) -> ValueId {
        ValueId(n)
    }
    fn heap() -> Repr {
        Repr::Ptr { layout: PLACEHOLDER_LAYOUT }
    }
    fn func(ops: Vec<Op>) -> MirFunction {
        MirFunction { name: "shape".into(), ops, ..Default::default() }
    }

    // Shape 2 — list_get_643: a per-iteration heap temp `t` is alloc'd and
    // consumed (pushed into `out`); the alias `nx` is dup'd and dropped at
    // scope end. The leak of a per-iteration temp is exactly #643's class.
    fn shape_643() -> MirFunction {
        let (nx, t) = (v(0), v(1));
        func(vec![
            Op::Alloc { dst: nx, repr: heap(), init: Init::Opaque }, // nx acquires its own ref (alias-inc)
            Op::Alloc { dst: t, repr: heap(), init: Init::Opaque },  // the slice|>join temp
            Op::Consume { v: t },                // pushed into `out` (moved)
            Op::Borrow { v: nx },                // used
            Op::Drop { v: nx },                  // scope end
        ])
    }

    // Shape 1 — alias_return: move the payload OUT (consume), free the shell
    // ONLY. A renderer that deep-frees the returned payload double-frees.
    fn shape_alias_return() -> MirFunction {
        let (payload, shell) = (v(0), v(1));
        func(vec![
            Op::Alloc { dst: payload, repr: heap(), init: Init::Opaque },
            Op::Alloc { dst: shell, repr: heap(), init: Init::Opaque },
            Op::Consume { v: payload }, // transferred to the caller (returned)
            Op::Drop { v: shell },      // free the Option shell only
        ])
    }

    // Shape 3 — boxed_pattern_610: read through the box is a Borrow (no
    // dup/drop of the child); the Leaf payload is a Scalar. One Drop of the node.
    fn shape_boxed_pattern() -> MirFunction {
        let (node, a) = (v(0), v(1));
        func(vec![
            Op::Alloc { dst: node, repr: heap(), init: Init::Opaque },
            Op::Const { dst: a },         // scalar leaf payload (Borrow-through-box copy)
            Op::Borrow { v: node },       // the nested read
            Op::Pure { dst: v(2), uses: vec![a, node] }, // e.g. a + node-tag use
            Op::Drop { v: node },         // scope end
        ])
    }

    // Shape 4 — closure_capture: capture = dup into env (a new handle `env`
    // sharing x's object); each call borrows the env handle; env-drop and the
    // original drop release the two refs. Read-only, callable twice (Fn).
    fn shape_closure_capture() -> MirFunction {
        let (x, env) = (v(0), v(1));
        func(vec![
            Op::Alloc { dst: x, repr: heap(), init: Init::Opaque },
            Op::Dup { dst: env, src: x }, // capture into the closure env
            Op::Borrow { v: env },        // call 1
            Op::Borrow { v: env },        // call 2
            Op::Drop { v: env },          // closure/env drop
            Op::Drop { v: x },            // original drop
        ])
    }

    // Shape 5 — alias_cow: `b` aliases `a` (a new handle sharing a's object),
    // MakeUnique before the in-place mutate, both handles dropped. (The AliasCow
    // *value* bug is wrong-output with the refcount BALANCED — caught by the
    // semantic-law oracle, finding #3 — so the ownership skeleton here is,
    // correctly, balanced.)
    fn shape_alias_cow() -> MirFunction {
        let (a, b) = (v(0), v(1));
        func(vec![
            Op::Alloc { dst: a, repr: heap(), init: Init::Opaque },
            Op::Dup { dst: b, src: a }, // b aliases a (object now shared, rc 2)
            Op::MakeUnique { v: a },    // clone-on-shared before mutating
            Op::Drop { v: a },          // a
            Op::Drop { v: b },          // b
        ])
    }

    #[test]
    fn all_five_gate_shapes_verify_balanced() {
        for f in [
            shape_643(),
            shape_alias_return(),
            shape_boxed_pattern(),
            shape_closure_capture(),
            shape_alias_cow(),
        ] {
            assert_eq!(verify_ownership(&f), Ok(()), "shape `{}` must verify clean", f.name);
        }
    }

    #[test]
    fn dropped_drop_is_caught_as_leak() {
        // #643 with the per-iteration alias Drop omitted (the renderer-side leak).
        let mut f = shape_643();
        f.ops.retain(|op| !matches!(op, Op::Drop { .. }));
        let errs = verify_ownership(&f).unwrap_err();
        assert!(errs.iter().any(|e| e.kind == ViolationKind::Leak && e.value == ValueId(0)));
    }

    #[test]
    fn deep_free_of_a_moved_payload_is_caught() {
        // alias_return where the renderer ALSO frees the returned payload.
        let mut f = shape_alias_return();
        f.ops.push(Op::Drop { v: ValueId(0) }); // drop after consume
        let errs = verify_ownership(&f).unwrap_err();
        assert!(errs.iter().any(|e| e.kind == ViolationKind::DoubleFree && e.value == ValueId(0)));
    }

    #[test]
    fn capture_consumed_on_call_over_releases() {
        // closure_capture mis-modeled: a call CONSUMES the env capture handle,
        // so the 2nd call uses an already-moved handle (the re-decision is caught
        // — UseAfterMove here; the point is it does not pass silently).
        let (x, env) = (ValueId(0), ValueId(1));
        let f = func(vec![
            Op::Alloc { dst: x, repr: heap(), init: Init::Opaque },
            Op::Dup { dst: env, src: x },
            Op::Consume { v: env }, // call 1 wrongly consumes the capture
            Op::Consume { v: env }, // call 2 — env already moved
            Op::Drop { v: x },
        ]);
        let errs = verify_ownership(&f).unwrap_err();
        assert!(errs
            .iter()
            .any(|e| matches!(e.kind, ViolationKind::UseAfterMove | ViolationKind::DoubleFree)));
    }

    #[test]
    fn use_after_free_is_caught() {
        let x = ValueId(0);
        let f = func(vec![
            Op::Alloc { dst: x, repr: heap(), init: Init::Opaque },
            Op::Drop { v: x },   // freed
            Op::Borrow { v: x }, // used after free
        ]);
        let errs = verify_ownership(&f).unwrap_err();
        assert!(errs.iter().any(|e| e.kind == ViolationKind::UseAfterFree));
    }

    #[test]
    fn scalars_need_no_ownership() {
        // A Const used by a Pure must not be flagged (no refcount on scalars).
        let f = func(vec![
            Op::Const { dst: ValueId(0) },
            Op::Pure { dst: ValueId(1), uses: vec![ValueId(0)] },
        ]);
        assert_eq!(verify_ownership(&f), Ok(()));
    }

    #[test]
    fn repr_heap_predicate() {
        assert!(heap().is_heap());
        assert!(Repr::Boxed { layout: PLACEHOLDER_LAYOUT }.is_heap());
        assert!(!Repr::Scalar { width: ScalarWidth::Double }.is_heap());
    }
}
