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
    /// Store the low `width` bytes (1/4/8) of an i64 value at a computed i64 address.
    Store { width: u8 },
    /// The `fd_write` WASI host call — `args = [fd, iov, count, nwritten]`, dst = the
    /// i64 errno. The ONLY sandbox exit; carries [`Capability::Stdout`].
    FdWrite,
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
            Op::Drop { v } => match release(&object_of, &mut rc, &mut dead, &borrowed, *v) {
                Ok(()) => {}
                Err(()) => violations.push(violation(i, *v, ViolationKind::DoubleFree)),
            },
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
            Op::Call { args, dst, result, .. } | Op::CallFn { args, dst, result, .. } => {
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
            Op::IntBinOp { .. } | Op::Prim { .. } | Op::IfThen { .. } | Op::Else { .. } | Op::EndIf { .. } => {}
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
