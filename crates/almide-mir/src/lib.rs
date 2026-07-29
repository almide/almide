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

/// The compiler's debug-trace channels.
pub(crate) mod trace;

pub mod alias_safety;
pub mod certificate;
pub mod concat_to_append;
pub mod coown_names;
pub mod region_alloc;
pub mod region_compact;
pub mod scalar_call_inline;
pub mod lower;
pub mod pipeline;
pub mod purity;
pub(crate) mod mir_wellformed;
pub mod render_native;
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
    ///
    /// On a VALUE path this means "the lowering could not build this content",
    /// and admitting it is the accept-but-wrong reservoir F2 (#810) is about: an
    /// empty block the program then reads as `0` / `None` / `[]`. It must WALL.
    /// It used to also mean "a genuinely empty container", which is the opposite
    /// — that meaning is now [`Init::Empty`], so the two can be told apart by
    /// the type instead of by each consumer re-deriving it from the expression
    /// shape (the `!elements.is_empty()` guards that used to approximate it).
    Opaque,
    /// A genuinely EMPTY heap block — the `[:]` / empty-container case, where the
    /// empty block IS the correct content and the program goes on to fill it
    /// (`map.from_list` allocates empty, then inserts).
    ///
    /// Physically identical to [`Init::Opaque`]: both render as an empty block,
    /// and the ownership cert is the same init-agnostic one `i`. The distinction
    /// is entirely about INTENT, and it exists so that "correct and empty" and
    /// "unbuildable" stop sharing a spelling. A corpus census (#810) found this
    /// is the ONLY shape that reached the terminal deferred bind — all 18
    /// corpus programs that a blanket wall broke were this case, 10 directly and
    /// 8 through `map.from_list`'s own empty-then-insert body.
    Empty,
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
    /// recurse iff `tag@16 == 0` (Ok-record), else `rc_dec` the @12 Err String. `err_rec` (Result
    /// only) INVERTS the tag dispatch for the heap-Ok × variant-Err class (`Result[String,
    /// MathError]` — `reserr:<V>`): recurse iff `tag@16 == 1` (Err-variant, via `$__drop_<V>`),
    /// else flat `rc_dec` the @12 Ok payload. Same single cert `d` as [`Op::Drop`]; the recursion
    /// is the trusted generated routine (leak-loop verified). The record-payload counterpart of
    /// `DropResultValue` (whose Ok payload is a `Value`, not a record).
    DropWrapperRec { v: ValueId, drop_fn: String, is_result: bool, err_rec: bool },
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

    /// A SCALAR-element list LITERAL materialized as ONE target-neutral op (rung 4 of
    /// the native trust-spine ladder — the shared-MIR list design): `dst` = a fresh
    /// OWNED `List[<scalar>]` block whose slots hold `elems` (raw i64 slot values,
    /// `len == cap == elems.len()`). render_wasm expands it to EXACTLY the
    /// `Alloc{DynList}` + per-slot-store sequence the inline builder emitted before;
    /// render_native maps it to `vec![…]`. Certificate/ownership: ONE `i`
    /// (alloc-class) on `dst` — the identical event stream the replaced `Alloc`
    /// produced, so the kernel checker sees no new vocabulary.
    ListLit { dst: ValueId, elems: Vec<ValueId> },

    /// `dst = list[idx]` for a SCALAR element — the bounds-checked element load
    /// (idx < 0 or >= cap TRAPs, matching native's halt). Replaces the inline
    /// `Handle` + `ElemAddr` + `Load{8}` sequence one-for-one; ownership-NEUTRAL
    /// (the list handle is borrowed/live-checked, the scalar result carries none).
    ListGetScalar { dst: ValueId, list: ValueId, idx: ValueId },

    /// `list[idx] = val` for a SCALAR element — the bounds-checked element store
    /// (COW is the caller's existing `MakeUnique` BEFORE this op). Replaces the
    /// inline `Handle` + `ElemAddr` + `Store{8}` sequence one-for-one;
    /// ownership-NEUTRAL like the load.
    ListSetScalar { list: ValueId, idx: ValueId, val: ValueId },

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

include!("lib_b.rs");
include!("lib_c.rs");
