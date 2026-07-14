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
pub mod coown_names;
pub mod lower;
pub mod pipeline;
pub mod purity;
pub mod render_native;
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
    /// A `Bytes` CONSTANT — the raw bytes the EXECUTION render reproduces as a `[rc][len][cap]
    /// [bytes…]` block (physically identical to a `Str` block, but the bytes are arbitrary, not
    /// UTF-8: the aes S-box has 0x00–0xFF). The materialization of a const module-level Bytes
    /// global (`let SBOX = bytes.from_list([…])`) WITHOUT a runtime call — so the gate's IR-side
    /// call count stays exact (a computed init keeps walling). Cert: one `i`, init-agnostic.
    Bytes(Vec<u8>),
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
    /// `drop_list_str_str v` — release a `List[(String, String)]` whose element slots hold owned
    /// (String, String) TUPLE blocks (the `map.entries` / svg render_attrs shape). The render calls
    /// `$__drop_list_str_str`: at the list's last ref each tuple is freed at its own last ref (BOTH
    /// String slots rc_dec'd flat — vs `DropListStrValue`'s recursive `$__drop_value` 2nd slot), then
    /// the tuple, then the list block. Same single cert `d` as [`Op::Drop`]; the (String,String)
    /// counterpart of `DropListStrValue`.
    DropListStrStr { v: ValueId },
    /// `drop_list_int_str v` — release a `List[(Int, String)]` whose element slots hold owned
    /// `(Int, String)` TUPLE blocks (the `list.enumerate` / `[(1,"a"),…]` shape). At the list's last
    /// ref, for each element: free the tuple at ITS last ref — `rc_dec` ONLY the String slot1 @20 (the
    /// Int slot0 @12 is scalar) — then the tuple block; then the list block. A flat `DropListStr` would
    /// `rc_dec` each tuple HANDLE only, leaking the tuple's String + block. Inline (no helper — the
    /// prior routing emitted a call to a never-generated `$__drop_list_int_str` → invalid wat). Same
    /// single cert `d`; the per-tuple recursion is the trusted raw-handle routine (leak-loop verified).
    /// The (Int,String) counterpart of `DropListStrStr`.
    DropListIntStr { v: ValueId },
    /// `drop_list_str_int v` — release a `List[(String, Int)]` (the tokenizer
    /// vocab-pairs literal): per tuple rc_dec ONLY the String slot @12 (the Int
    /// @20 is scalar), then the tuple block, then the list block. The
    /// (String,Int) MIRROR of `DropListIntStr`.
    DropListStrInt { v: ValueId },
    /// `drop_result_lv v` — release a `value.as_array` Result `Result[List[Value], String]` (the
    /// cap-as-tag 1-slot block `[rc][len@4=1][cap@8][@12 payload][@16 tag]`). IFF the last reference
    /// (rc==1), the RENDER tag-dispatches on @16: Ok (0) frees the `List[Value]` payload @12
    /// RECURSIVELY (`$__drop_list_value`), Err (1) frees the String @12 (`rc_dec`); THEN the block.
    /// A flat `DropListStr` would only rc_dec @12 (the list block), LEAKING its element Values. Same
    /// cert event as [`Op::Drop`] (one `−1`/`d` on the Result object — its payload was `m`/consumed).
    DropResultListValue { v: ValueId },
    /// `drop_result_value v` — release a `Result[Value, String]` (the `ok(value.array(...))` shape),
    /// the cap-as-tag 1-slot block `[rc][len@4=1][cap@8][@12 payload][@16 tag]`. IFF the last ref
    /// (rc==1) the RENDER tag-dispatches on @16 (via self-hosted `$__drop_result_value`): Ok (0)
    /// frees the Value @12 RECURSIVELY (`$__drop_value` — a nested Array/Str payload frees too), Err
    /// (1) frees the String @12 (`rc_dec`); THEN the block. A flat `DropListStr` would only rc_dec
    /// @12, LEAKING the Ok Value's nested payload. Same single cert `d`; the Value-payload counterpart
    /// of `DropResultListValue`.
    DropResultValue { v: ValueId },
    /// `drop_result_str_int v` — release a `Result[(String, Int), String]` (toml `parse_key_part`'s
    /// `ok((slice, pos))` shape). The cap-as-tag 1-slot wrapper `[rc][len@4=1][cap@8][@12 payload]
    /// [@16 tag]`: IFF the last ref (rc==1) the RENDER tag-dispatches on @16 — Ok (0) frees the
    /// `(String, Int)` tuple @12 at ITS last ref (`rc_dec` the String slot0 @12 only; the Int slot1
    /// @20 is scalar), then the tuple block; Err (1) frees the String @12; THEN the wrapper block. A
    /// flat `DropListStr` would `rc_dec` @12 as a String, LEAKING the tuple's String (and freeing the
    /// tuple block as if it were the String). Same single cert `d`; the inline recursion is the
    /// trusted raw-handle routine (leak-loop verified). The tuple-payload counterpart of
    /// `DropResultValue`.
    DropResultStrInt { v: ValueId },
    /// `drop_result_value_int v` — release a `Result[(Value, Int), String]` (toml `parse_val`'s
    /// `ok((value.…, pos))` shape). Same cap-as-tag wrapper as `DropResultStrInt`, but the Ok tuple's
    /// slot0 is a dynamic `Value` (tag-dispatched, can hold a nested Array/Object), so the RENDER frees
    /// it RECURSIVELY via value_core's `$__drop_value_tuple` (Ok: at the tuple's last ref `$__drop_value`
    /// slot0 then the tuple block; Err: `rc_dec` the String @12); THEN the wrapper. A flat `DropListStr`
    /// would leak the Value's nested payload. Same single cert `d`; the recursion is the trusted
    /// value_core routine (leak-loop verified). value_core is always linked here (the Ok built a Value
    /// via a `value.*` ctor). The Value-tuple counterpart of `DropResultStrInt`.
    DropResultValueInt { v: ValueId },
    /// `drop_result_list_value_int v` — release a `Result[(List[Value], Int), String]` (toml
    /// `collect_array_items`'s `ok((items, np))`). Same cap-as-tag wrapper; the Ok tuple's slot0 is a
    /// `List[Value]`, freed RECURSIVELY via value_core's `$__drop_list_value_tuple` (Ok: at the tuple's
    /// last ref `$__drop_list_value` slot0 — each element Value freed by tag — then the tuple block;
    /// Err: `rc_dec` the String @12); THEN the wrapper. A flat `DropListStr` would leak the element
    /// Values. The List[Value]-tuple counterpart of `DropResultValueInt`.
    DropResultListValueInt { v: ValueId },
    /// `drop_result_list_str_int v` — release a `Result[(List[String], Int), String]` (toml
    /// `parse_key` / `parse_table_key`'s `ok((keys, pos))`). Same cap-as-tag wrapper, but the Ok
    /// tuple's slot0 is a `List[String]` handle: the RENDER frees it RECURSIVELY (a NESTED loop —
    /// at the tuple's last ref, at the List's last ref `rc_dec` each element String, then the List
    /// block, then the tuple block; Err: `rc_dec` the String @12), THEN the wrapper. A flat
    /// `DropListStr` would `rc_dec` the @12 tuple HANDLE only — leaking the List's element Strings
    /// AND the List block. Inline (no helper ⇒ no value_core link). Single cert `d`; the recursion
    /// is the trusted raw-handle routine (leak-loop verified). The List-tuple counterpart of
    /// `DropResultStrInt`.
    DropResultListStrInt { v: ValueId },
    /// `drop_result_list_str v` — release a `Result[List[String], String]` (the `fs.list_dir`
    /// `ok([name, …])` shape). Same cap-as-tag wrapper `[rc][len@4=1][cap@8=1][@12 payload]
    /// [@16 tag]` as `DropResultStrInt`, but the Ok payload @12 is a `List[String]` handle (no
    /// tuple layer — the DIRECT list, unlike `DropResultListStrInt`'s `(List[String], Int)`):
    /// the RENDER frees it RECURSIVELY (Ok: at the List's last ref `rc_dec` each element String,
    /// then the List block; Err: `rc_dec` the String @12), THEN the wrapper block. A flat
    /// `DropListStr` would `rc_dec` the @12 List HANDLE only — leaking the List's element
    /// Strings AND the List block. Inline (no helper). Single cert `d`; the recursion is the
    /// trusted raw-handle routine (leak-loop verified). The non-tuple counterpart of
    /// `DropResultListStrInt`.
    DropResultListStr { v: ValueId },
    /// `drop_list_list_str v` — release a `List[List[String]]` whose element slots hold owned
    /// `List[String]` blocks (the csv `rows` shape: a list of rows, each a list of cells). The render
    /// emits a NESTED loop: at the outer list's last ref (rc==1), for each element it frees the inner
    /// `List[String]` at ITS last ref (per-slot `rc_dec` of each String), then `rc_dec`s the inner
    /// block; THEN the outer block. A flat `DropListStr` would only `rc_dec` each inner-list HANDLE,
    /// LEAKING the cell Strings (the inner list's last-ref free never runs). Same single cert `d` as
    /// [`Op::Drop`]; the per-element recursion is the trusted routine (raw-handle, leak-loop verified).
    /// The list-of-lists counterpart of `DropListStr`.
    DropListListStr { v: ValueId },
    /// `drop_variant v : ty` — release a CUSTOM variant (user ADT) block whose ctor fields may be
    /// nested variant/heap handles (`Add(Expr, Expr)`). A flat `Drop`/`DropListStr` would `rc_dec`
    /// the block (and masked slots) WITHOUT recursively freeing a child variant's OWN nested fields
    /// — a LEAK. So the RENDER calls the GENERATED per-type recursive free `$__drop_<ty>` (the
    /// `$__drop_value` shape: at the last ref read the tag, recursively free each variant field +
    /// `rc_dec` each leaf field, then the block). Same single cert `d` as [`Op::Drop`]; the recursion
    /// is the trusted routine (the generated fn is `prim`-only ⇒ empty ownership cert, leak-loop
    /// verified). The custom-ADT counterpart of `DropValue` (ADT brick 5b).
    DropVariant { v: ValueId, ty: String },
    /// `drop_wrapper_rec v : drop_fn` — release an Option/Result WRAPPER block whose payload @12 is
    /// a heap RECORD (the `some({key, val})` / `ok({val, next})` shape). The wrapper is the same
    /// 1-slot block every other Option/Result materialization uses; a flat `DropListStr` would
    /// `rc_dec` the @12 record HANDLE only — freeing the record BLOCK but LEAKING its nested heap
    /// fields (String / List / Value), since `rc_dec` is one-level. So the RENDER recurses into the
    /// record via the generated `$__drop_<drop_fn>` (the same per-field recursive free a directly
    /// owned record uses — `record_drop_field_frees`), gated on the wrapper's last ref (rc==1), then
    /// `rc_dec`s the wrapper block. `is_result` selects the wrapper shape: `false` (Option) =
    /// 0-or-1-element DynListStr, recurse iff `len@4 > 0` (Some); `true` (Result) = cap-as-tag block,
    /// recurse iff `tag@16 == 0` (Ok-record), else `rc_dec` the @12 Err String. Same single cert `d`
    /// as [`Op::Drop`]; the recursion is the trusted generated routine (leak-loop verified). The
    /// record-payload counterpart of `DropResultValue` (whose Ok payload is a `Value`, not a record).
    DropWrapperRec { v: ValueId, drop_fn: String, is_result: bool },
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

    /// Call a host-provided WASM IMPORT — the body of an `@extern(wasm, module,
    /// name)` function. The renderer emits `(call $<import>)` and DECLARES the
    /// matching `(import "module" "name" (func …))` at module scope. This is
    /// FAITHFUL: the function's behavior IS the host's, so a call (not a fabricated
    /// value) is the only sound lowering — the wasm module is valid and a browser
    /// host satisfies the import (it does NOT instantiate under wasmtime, which has
    /// no such host; that is expected for a browser-targeted module).
    ///
    /// Ownership is exactly an [`Op::Call`]'s: heap-handle args are BORROWED
    /// (live-checked, refcount unchanged), and a heap `result` is a FRESH OWNED
    /// value (the host returns a pointer the caller now owns). `abi`/`result_abi`
    /// carry the import's wasm SIGNATURE valtypes (mapped from the declared Almide
    /// types: Int→i64, Float→f64, Bool→i32, String/heap→i32), parallel to `args`;
    /// the render coerces each i64-uniform MIR local to/from the import valtype.
    CallImport {
        dst: Option<ValueId>,
        module: String,
        name: String,
        args: Vec<CallArg>,
        abi: Vec<WasmAbi>,
        result: Option<Repr>,
        result_abi: Option<WasmAbi>,
    },

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
    /// Abort: write the String-block message to STDERR and proc_exit(1) — the
    /// self-host arm of the §13 termination convention (math.pow negative
    /// exponent, int.rotate nonpositive width). Never returns.
    Die,
    /// The `fd_write` WASI host call — `args = [fd, iov, count, nwritten]`, dst = the
    /// i64 errno. A sandbox exit; carries [`Capability::Stdout`].
    FdWrite,
    /// The `random_get` WASI host call — `args = [buf, buf_len]`, dst = the i64 errno;
    /// fills `buf_len` bytes at `buf` with host entropy. The second sandbox exit; reached
    /// only by the self-hosted `random.int`. Carries [`Capability::Entropy`] (the
    /// cap_witness counts it exactly like `FdWrite` → Stdout), so a function using it is
    /// caps-verified ONLY if it declares Entropy — never accept-but-unsafe.
    RandomGet,
    /// The `clock_time_get` WASI host call — `args = [clock_id, precision, time_ptr]`, dst =
    /// the i64 errno; writes the current clock value (nanoseconds) as an i64 at `time_ptr`.
    /// A SCALAR-dst sandbox exit (like [`RandomGet`] — NO heap result, NO ownership event),
    /// reached only by the self-hosted `env.unix_timestamp` (which reads `time_ptr` and
    /// divides by 1e9 to seconds). Carries [`Capability::Clock`] — a DISTINCT capability
    /// (a clock read is neither a filesystem nor an entropy effect; the cap_witness counts
    /// it exactly like `RandomGet` → Entropy), so a function using it is caps-verified ONLY
    /// if it declares Clock — never accept-but-unsafe. NON-DETERMINISTIC (no byte-match).
    ClockTimeGet,
    /// The `args_sizes_get` + `args_get` WASI host calls, packaged as ONE high-level
    /// HEAP-RESULT prim — no args, dst = a fresh OWNED `List[String]` of the program
    /// arguments `argv[1..]` (SKIP argv[0], matching native `env.args`). Each element
    /// is a canonical Almide String copied from the NUL-terminated argv C-string. The
    /// third sandbox exit, reached only by the self-hosted `env.args`. Carries
    /// [`Capability::CliArgs`] (the cap_witness counts it exactly like `RandomGet` →
    /// Entropy), so a function using it is caps-verified ONLY if it declares CliArgs —
    /// never accept-but-unsafe. Its dst is a heap Ptr (like `LoadHandle`), so the
    /// ownership certificate emits an `i` (alloc) for it, balanced by the caller's
    /// scope-end drop (a recursive `DropListStr` over the owned element Strings).
    ArgsGetList,
    /// The SAME WASI args floor as [`ArgsGetList`] but INCLUDING argv[0] (the program
    /// path) — `process.args()` = native `std::env::args()`. Renders as
    /// `(call $args_get_list (i32.const 0))` (the one parameterized bridge, skip=0);
    /// same fresh OWNED `List[String]` dst, same [`Capability::CliArgs`] accounting.
    ArgsGetListFull,
    /// The WASI `fd_read`-from-stdin line-read sequence, packaged as ONE high-level HEAP-RESULT
    /// prim — no args, dst = a fresh OWNED canonical `String` of ONE line of standard input.
    /// Reads fd 0 BYTE-BY-BYTE (so it never over-reads past the newline — a later
    /// `read_n_bytes` of the body still sees the right stream) until a `\n` (excluded from the
    /// result) or EOF, then strips a trailing `\r` (matching native
    /// `read_line().trim_end_matches('\n').trim_end_matches('\r')`). EOF with no bytes yields the
    /// empty String. Reached only by the self-hosted `io.read_line`. Carries [`Capability::Stdin`]
    /// — a DISTINCT capability (reading standard input is neither a write, a filesystem, an
    /// entropy, nor a clock effect; the cap_witness counts it exactly like `RandomGet` → Entropy),
    /// so a function using it is caps-verified ONLY if it declares Stdin — never accept-but-unsafe.
    /// NON-DETERMINISTIC (reads live stdin): no byte-match across runs unless stdin is fixed. Its
    /// dst is a heap Ptr (like [`ArgsGetList`]), so the ownership certificate emits an `i` (alloc)
    /// for it, balanced by the caller's scope-end flat `Drop` (a String owns no nested handles) or
    /// a heap-return move-out.
    ReadLine,
    /// `read_n_bytes(n)` — the WASI stdin-N-bytes floor (io.read_n_bytes), the SIBLING of
    /// [`PrimKind::ReadLine`]: `args = [n]` (an `Int`, the byte count), dst = a fresh OWNED `Bytes`
    /// block (the same byte-buffer block layout a `String` uses, built by the preamble `$read_n_bytes`
    /// via `$rtf_str`). Reads UP TO `n` bytes from fd 0 (stopping early at EOF). Carries
    /// Capability::Stdin (same DISTINCT cap as ReadLine). NON-DETERMINISTIC (live stdin): no byte-match.
    /// Its dst is a heap Ptr, so the ownership certificate emits an `i` (alloc) balanced by the caller's
    /// scope-end flat `Drop` (a Bytes owns no nested handles) or a heap-return move-out.
    ReadNBytes,
    /// The WASI `path_open` + `fd_read` file-read sequence, packaged as ONE high-level
    /// HEAP-RESULT prim — `args = [path]` (a BORROWED `String` handle), dst = a fresh
    /// OWNED `Result[String, String]`. Opens the file at `path` (relative to the first
    /// preopened dir, leading `/` stripped — the same absolute-path fallback the native
    /// emit's `__resolve_path` uses) and reads its bytes: on success builds `Ok(content)`
    /// where `content` is a canonical Almide String of the file bytes; on a path_open
    /// error builds `Err(<message>)`. The result block is the EXACT `materialize_result_str`
    /// layout — a 1-slot DynListStr `[rc][len@4=1][cap@8][@12 String handle][@16 tag]`
    /// (tag 0 = Ok, 1 = Err) — so the caller's `!`/`match`/`DropListStr` machinery handles
    /// it identically to a self-host-built `Result[String, String]`. The FOURTH sandbox
    /// exit, reached only by the self-hosted `fs.read_text`. Carries [`Capability::FsRead`]
    /// (the cap_witness counts it exactly like `ArgsGetList` → CliArgs), so a function using
    /// it is caps-verified ONLY if it declares FsRead — never accept-but-unsafe. Its dst is
    /// a heap Ptr (like `ArgsGetList`), so the ownership certificate emits an `i` (alloc) for
    /// it, balanced by the caller's scope-end drop (the flat `DropListStr` over the one owned
    /// payload String).
    ReadTextFile,
    /// The WASI `path_open(O_DIRECTORY)` + `fd_readdir` directory-listing sequence, packaged
    /// as ONE high-level HEAP-RESULT prim — `args = [path]` (a BORROWED `String` handle), dst
    /// = a fresh OWNED `Result[List[String], String]`. Opens the directory at `path` (same
    /// preopen-relative resolution as [`ReadTextFile`]) and reads its entries via an
    /// `fd_readdir` re-read-on-truncation loop, parsing each variable-length dirent record
    /// (`d_next u64 / d_ino u64 / d_namlen u32 / d_type u8 / name[d_namlen]`), SKIPPING `.`
    /// and `..` (WASI yields them, native `std::fs::read_dir` does not), then SORTING the names
    /// lexicographically (to byte-match the Rust runtime's `names.sort()`), and builds
    /// `Ok([name, …])` where the payload is a fresh owned `List[String]`. On a path_open error
    /// it builds `Err(<message>)`. The result block is the cap-as-tag layout `[rc][len@4=1]
    /// [cap@8=1][@12 List[String] handle][@16 tag]` (tag 0 = Ok, 1 = Err) — the SAME shape as
    /// [`ReadTextFile`], only the @12 payload is a nested `List[String]` (so the scope-end drop
    /// is the RECURSIVE [`StmtKind::DropResultListStr`], not the flat `DropListStr` that would
    /// leak the inner element Strings). The FIFTH sandbox exit, reached only by the self-hosted
    /// `fs.list_dir`. Carries [`Capability::FsRead`] (the cap_witness counts it exactly like
    /// [`ReadTextFile`] → FsRead), so a function using it is caps-verified ONLY if it declares
    /// FsRead — never accept-but-unsafe. Its dst is a heap Ptr (like [`ReadTextFile`]), so the
    /// ownership certificate emits an `i` (alloc) for it, balanced by the caller's scope-end
    /// recursive drop (or a heap-return move-out).
    ReadDir,
    /// The WASI `path_open(O_CREAT|O_TRUNC)` + `fd_write` file-WRITE sequence, packaged as ONE
    /// high-level HEAP-RESULT prim — `args = [path, content]` (both BORROWED `String` handles,
    /// the caller still owns them), dst = a fresh OWNED `Result[Unit, String]`. Opens (creating +
    /// truncating) the file at `path` (relative to the first preopened dir, leading `/` stripped —
    /// the same resolution [`ReadTextFile`] uses) and writes `content`'s bytes via `fd_write`: on
    /// success builds `Ok(())`, on a path_open / fd_write error builds `Err(<message>)`. The result
    /// block reuses the cap-as-tag layout `[rc][len@4][cap@8][@12][@16 tag]` (tag 0 = Ok, 1 = Err),
    /// but DIVERGES from [`ReadTextFile`] in the Ok arm: a `Unit` payload owns NO String, so Ok is
    /// built with `len@4 = 0` (and `@12 = 0`, `@16 = 0`) — EXACTLY the `materialize_result_ok`
    /// convention — so the caller's scope-end flat `DropListStr` frees NOTHING at @12 (it would
    /// trap on a null `rc_dec` if Ok carried a phantom `len = 1`). The Err arm sets `len@4 = 1`,
    /// `@12 = msg String`, `@16 tag = 1` (the flat `DropListStr` frees the one owned message). The
    /// FIFTH host-write sandbox exit, reached only by the self-hosted `fs.write`. Carries
    /// [`Capability::FsWrite`] — a DISTINCT capability from FsRead (a write is strictly greater
    /// authority), counted in cap_witness — so a function using it is caps-verified ONLY if it
    /// declares FsWrite; never accept-but-unsafe. Its dst is a heap Ptr (like [`ReadTextFile`]),
    /// so the ownership certificate emits an `i` (alloc) for it, balanced by the caller's scope-end
    /// flat `DropListStr` (sound for BOTH arms given the `len@4 = 0` Ok convention above).
    WriteTextFile,
    /// The WASI `path_create_directory` recursive-mkdir sequence, packaged as ONE high-level
    /// HEAP-RESULT prim — `args = [path]` (a BORROWED `String` handle, the caller still owns
    /// it), dst = a fresh OWNED `Result[Unit, String]`. Creates the directory at `path`
    /// (relative to the first preopened dir, leading `/` stripped — the same resolution
    /// [`WriteTextFile`] uses), creating each missing parent segment (so `a/b/c` makes all
    /// three); an existing dir (errno EEXIST = 20) counts as success. On success builds
    /// `Ok(())` (the `len@4 = 0` `materialize_result_ok` convention, IDENTICAL to
    /// [`WriteTextFile`]'s Ok arm), on a `path_create_directory` error builds
    /// `Err(<message>)` (`len@4 = 1`, `@12 = msg`, `@16 tag = 1`). A mkdir IS a filesystem
    /// WRITE, so it REUSES [`Capability::FsWrite`] (NOT a new capability — that would be a
    /// false distinction); counted in cap_witness exactly like [`WriteTextFile`]. Its dst is
    /// a heap Ptr, so the ownership certificate emits an `i` (alloc), balanced by the
    /// caller's scope-end flat `DropListStr` (sound for BOTH arms given the `len@4 = 0` Ok).
    MakeDir,
    /// The WASI `path_remove_directory` / `path_unlink_file` RECURSIVE-remove sequence, packaged
    /// as ONE high-level HEAP-RESULT prim — `args = [path]` (a BORROWED `String` handle, the
    /// caller still owns it), dst = a fresh OWNED `Result[Unit, String]`. Removes the tree rooted
    /// at `path` (relative to the first preopened dir, leading `/` stripped — the same resolution
    /// [`WriteTextFile`] uses): if `path` opens as a directory it RECURSIVELY removes every entry
    /// (a child directory via `path_remove_directory` after it is emptied, a child file via
    /// `path_unlink_file`) then removes the now-empty directory; if it is a file it is unlinked
    /// directly — matching native `fs.remove_all` (`remove_dir_all` for a dir, `remove_file`
    /// otherwise). On success builds `Ok(())` (the `len@4 = 0` `materialize_result_ok` convention,
    /// IDENTICAL to [`WriteTextFile`]'s Ok arm), on a removal error builds `Err(<message>)`
    /// (`len@4 = 1`, `@12 = msg`, `@16 tag = 1`). A remove IS a filesystem WRITE, so it REUSES
    /// [`Capability::FsWrite`] (NOT a new capability — that would be a false distinction); counted
    /// in cap_witness exactly like [`WriteTextFile`]. Its dst is a heap Ptr, so the ownership
    /// certificate emits an `i` (alloc), balanced by the caller's scope-end flat `DropListStr`
    /// (sound for BOTH arms given the `len@4 = 0` Ok).
    RemoveAll,
    /// The WASI `path_filestat_get` existence query, packaged as ONE high-level SCALAR prim —
    /// `args = [path]` (a BORROWED `String` handle, the caller still owns it), dst = a SCALAR
    /// `Bool` (an i64 0/1). Stats `path` (relative to the first preopened dir, leading `/`
    /// stripped — the same resolution [`ReadTextFile`] uses) and yields `1` if a file OR
    /// directory exists there (errno 0), `0` otherwise — matching native `fs.exists`
    /// (`std::path::Path::exists`). UNLIKE every other fs prim this is NOT a heap result: a stat
    /// allocates nothing, so its dst is a plain scalar (NO `materialize_result` block, NO
    /// scope-end drop, NO ownership-cert `i` — it falls in the scalar-result `_ => {}` arm).
    /// A stat IS a filesystem READ, so it REUSES [`Capability::FsRead`] (NOT a new capability —
    /// the SAME accounting as [`ReadTextFile`] → FsRead); counted in cap_witness. Reached only by
    /// the self-hosted `fs.exists`.
    PathExists,
    /// The WASI `path_filestat_get` FULL-stat query — `args = [bufaddr, path]` (a raw scratch
    /// ADDRESS the caller owns — the self-host's 64-byte Bytes data region — plus a BORROWED
    /// `String` handle), dst = the SCALAR errno (i64; 0 = the host wrote the 64-byte WASI
    /// filestat at `bufaddr`: filetype@16, size@32, mtim@48). The self-hosted `fs.stat` reads
    /// the fields off its own scratch via `prim.load*` and builds the FileStat record in
    /// ordinary Almide — the prim stays a thin syscall wrapper (no heap result, no ownership
    /// event; the same scalar-dst discipline as [`PathExists`]). A stat IS a filesystem READ,
    /// so it REUSES [`Capability::FsRead`] (counted in cap_witness). Reached only by the
    /// self-hosted `fs.stat`.
    PathFilestat,
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
    /// Reading host ENTROPY — the WASI `random_get` floor ([`PrimKind::RandomGet`]),
    /// reached by the self-hosted `random.int`. The second sandbox exit. A pure `fn`
    /// declares ∅, so it can NEVER reach entropy un-witnessed (the checker REJECTS
    /// `used ⊄ allowed`); only an `effect fn` (which declares the host caps) may.
    Entropy,
    /// Reading the program's COMMAND-LINE ARGUMENTS — the WASI `args_sizes_get` /
    /// `args_get` floor ([`PrimKind::ArgsGetList`]), reached by the self-hosted
    /// `env.args`. The third sandbox exit. Accounted exactly like Entropy/Stdout: a
    /// pure `fn` declares ∅ and so can NEVER read argv un-witnessed (the checker
    /// REJECTS `used ⊄ allowed`); only an `effect fn` (which declares the host caps) may.
    CliArgs,
    /// Reading a FILE from the host filesystem — the WASI `path_open` / `fd_read` floor
    /// ([`PrimKind::ReadTextFile`]), reached by the self-hosted `fs.read_text`. The fourth
    /// sandbox exit. Accounted exactly like CliArgs/Entropy/Stdout: a pure `fn` declares ∅
    /// and so can NEVER read a file un-witnessed (the checker REJECTS `used ⊄ allowed`);
    /// only an `effect fn` (which declares the host caps) may.
    FsRead,
    /// Writing a FILE to the host filesystem — the WASI `path_open(O_CREAT|O_TRUNC)` /
    /// `fd_write` floor ([`PrimKind::WriteTextFile`]), reached by the self-hosted `fs.write`.
    /// The fifth sandbox exit. A STRICTLY GREATER authority than [`Self::FsRead`] (a write
    /// creates/truncates host state), so it is a DISTINCT capability with its own id — never
    /// aliased to FsRead (conflating read and write would be a capability lie: a fn declaring
    /// only read could mutate the filesystem). Accounted exactly like FsRead: a pure `fn`
    /// declares ∅ and so can NEVER write a file un-witnessed (the checker REJECTS
    /// `used ⊄ allowed`); only an `effect fn` (which declares the host caps) may.
    FsWrite,
    /// Reading the host WALL CLOCK — the WASI `clock_time_get` floor
    /// ([`PrimKind::ClockTimeGet`]), reached by the self-hosted `env.unix_timestamp`. The
    /// sixth sandbox exit. A clock read is neither a filesystem effect nor an entropy draw,
    /// so it is a DISTINCT capability with its own id — never aliased to FsRead/FsWrite or
    /// Entropy. Accounted exactly like Entropy/FsRead: a pure `fn` declares ∅ and so can
    /// NEVER read the clock un-witnessed (the checker REJECTS `used ⊄ allowed`); only an
    /// `effect fn` (which declares the host caps) may.
    Clock,
    /// Reading STANDARD INPUT — the WASI `fd_read`-from-fd-0 floor ([`PrimKind::ReadLine`]),
    /// reached by the self-hosted `io.read_line`. The seventh sandbox exit. Reading stdin is
    /// neither a write, a filesystem read, an entropy draw, nor a clock read, so it is a DISTINCT
    /// capability with its own id — never aliased to FsRead/FsWrite/Entropy/Clock (a fn that
    /// consumes the operator's input stream is a real, separately-grantable authority). Accounted
    /// exactly like Entropy/FsRead: a pure `fn` declares ∅ and so can NEVER read stdin
    /// un-witnessed (the checker REJECTS `used ⊄ allowed`); only an `effect fn` (which declares
    /// the host caps) may.
    Stdin,
}

impl Capability {
    /// The stable registry id — the ONLY place a `Capability` becomes a number.
    /// proofs/CapabilityBound.v's checker is GENERIC over `list nat` (a `subset_check`,
    /// no per-capability enumeration), so it needs no edit to admit a new id — only
    /// this mapping must stay injective + stable (Stdout = 0, Entropy = 1, CliArgs = 2,
    /// FsRead = 3, FsWrite = 4, Clock = 5, Stdin = 6).
    pub const fn id(self) -> u32 {
        match self {
            Capability::Stdout => 0,
            Capability::Entropy => 1,
            Capability::CliArgs => 2,
            Capability::FsRead => 3,
            Capability::FsWrite => 4,
            Capability::Clock => 5,
            Capability::Stdin => 6,
        }
    }
}

/// A wasm IMPORT-signature value type — the host-facing valtype an
/// [`Op::CallImport`] argument/result is mapped to from its declared Almide type
/// (Int→`I64`, Float→`F64`, Bool→`I32`, String/heap pointer→`I32`). The MIR is
/// i64-uniform for scalars (a Float local holds the f64 BITS) and i32 for heap
/// handles, so the render coerces each local to/from this valtype at the call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WasmAbi {
    /// A 64-bit integer — the MIR scalar local passes through directly.
    I64,
    /// A 64-bit float — the MIR i64 local holds its bits; reinterpret around the call.
    F64,
    /// A 32-bit integer — a heap pointer (MIR i32, direct) or a Bool (MIR i64, wrapped).
    I32,
}

impl WasmAbi {
    /// The WAT valtype keyword for an import signature.
    pub fn wat(self) -> &'static str {
        match self {
            WasmAbi::I64 => "i64",
            WasmAbi::F64 => "f64",
            WasmAbi::I32 => "i32",
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
    /// `pub fn` names to expose as wasm `(export …)` directives (#457 — module-export
    /// roots the v0 emitter also exports). Populated by the pipeline from the MAIN
    /// program's `IrVisibility::Public` non-test functions; empty everywhere else.
    pub exports: Vec<String>,
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
    /// The two arms of an `IfThen`/`Else`/`EndIf` branch leave an object at
    /// DIFFERENT owner counts — whichever way the branch goes at runtime, the
    /// later accounting is wrong for the other path (a path-dependent leak or
    /// double-free). Mirrors the proven checker's `CBranch` agreement rule.
    BranchDisagreement,
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

    // BRANCH JOIN (mirrors the proven checker's `CBranch` rule): each arm of an
    // `IfThen`/`Else`/`EndIf` runs from the SAME entry state, and the arms must
    // AGREE on every object's leaving count (the net may be nonzero — a
    // heap-result branch nets +1 through either arm). Folding the arms FLAT
    // (the old model) counted BOTH arms' events, silently accepting cross-arm
    // compensation — a `Consume` in one arm "balancing" the other arm's missing
    // release, i.e. a path-dependent leak/double-free.
    struct BranchFrame {
        entry_rc: BTreeMap<ValueId, i64>,
        entry_dead: BTreeMap<ValueId, bool>,
        then_exit: Option<(BTreeMap<ValueId, i64>, BTreeMap<ValueId, bool>)>,
    }
    let mut branches: Vec<BranchFrame> = Vec::new();

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
            | Op::DropListStrStr { v }
            | Op::DropListIntStr { v }
            | Op::DropListStrInt { v }
            | Op::DropResultListValue { v }
            | Op::DropResultValue { v }
            | Op::DropResultStrInt { v }
            | Op::DropResultValueInt { v }
            | Op::DropResultListValueInt { v }
            | Op::DropResultListStrInt { v }
            | Op::DropResultListStr { v }
            | Op::DropListListStr { v }
            | Op::DropVariant { v, .. }
            | Op::DropWrapperRec { v, .. } => {
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
            // A CallImport (a host wasm import) has the SAME ownership shape: heap-handle
            // args are BORROWED, a heap result is a FRESH OWNED value (the host returns a
            // pointer the caller now owns). Its scalar args carry no ownership.
            | Op::CallImport { args, dst, result, .. }
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
            // The if-markers carry no ownership of their own, but they scope the
            // BRANCH JOIN: both arms run from the entry state and must agree.
            Op::IfThen { .. } => {
                branches.push(BranchFrame {
                    entry_rc: rc.clone(),
                    entry_dead: dead.clone(),
                    then_exit: None,
                });
            }
            Op::Else { .. } => {
                if let Some(fr) = branches.last_mut() {
                    fr.then_exit = Some((rc.clone(), dead.clone()));
                    rc = fr.entry_rc.clone();
                    dead = fr.entry_dead.clone();
                }
            }
            Op::EndIf { .. } => {
                if let Some(fr) = branches.pop() {
                    let (then_rc, then_dead) = match fr.then_exit {
                        Some(t) => t,
                        // No Else marker: everything since IfThen was the then arm;
                        // the else arm is empty (= the entry state).
                        None => {
                            let cur = (rc.clone(), dead.clone());
                            rc = fr.entry_rc.clone();
                            dead = fr.entry_dead.clone();
                            cur
                        }
                    };
                    // Agreement per object (absent = 0 owned refs).
                    let keys: BTreeSet<ValueId> =
                        then_rc.keys().chain(rc.keys()).copied().collect();
                    for k in keys {
                        let a = then_rc.get(&k).copied().unwrap_or(0);
                        let b = rc.get(&k).copied().unwrap_or(0);
                        if a != b {
                            violations.push(violation(i, k, ViolationKind::BranchDisagreement));
                        }
                    }
                    // Continue with the JOIN: pointwise max keeps the run stable
                    // after a reported disagreement (no cascading underflows); on
                    // agreement it is the common value. A handle dead on EITHER
                    // path is unusable after the merge.
                    for (k, v) in then_rc {
                        let e = rc.entry(k).or_insert(0);
                        if v > *e {
                            *e = v;
                        }
                    }
                    for (k, d) in then_dead {
                        let e = dead.entry(k).or_insert(d);
                        *e = *e || d;
                    }
                }
            }
            // Scalar arithmetic — no ownership.
            // A scalar arithmetic op and a primitive-floor op carry no ownership: a
            // scalar result is Copy and a `Prim` handle arg is BORROWED (read only).
            Op::IntBinOp { .. }
            // Loop markers carry no ownership; the body ops between them are
            // per-iteration-balanced (verified flat, one iteration).
            | Op::LoopStart
            | Op::LoopBreakUnless { .. }
            | Op::LoopEnd => {}
            // VALUE-RC modeling (柱C extension) — bring the Value refcount ops out of the prim blind
            // spot for the NAMEABLE case: prim.handle(v) carries its source object in args[0], so the
            // rc events on it verify against the same rc machine. load64-fed handles have no carrier
            // and stay unmodeled (the differential-test floor). MIRRORED in ownership_certificate.
            Op::Prim { kind, dst, args } => match kind {
                PrimKind::Handle => {
                    if let (Some(d), Some(&o)) =
                        (dst.as_ref(), args.first().and_then(|a| object_of.get(a)))
                    {
                        object_of.insert(*d, o);
                    }
                }
                PrimKind::RcInc => {
                    if let Some(&o) = args.first().and_then(|a| object_of.get(a)) {
                        *rc.entry(o).or_insert(0) += 1;
                    }
                }
                PrimKind::RcDec => {
                    if let Some(&o) = args.first().and_then(|a| object_of.get(a)) {
                        if rc.get(&o).copied().unwrap_or(0) >= 1 {
                            *rc.entry(o).or_insert(0) -= 1;
                        }
                    }
                }
                _ => {}
            },
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

include!("lib_p2.rs");
