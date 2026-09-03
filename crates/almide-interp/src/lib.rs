//! Almide IR tree-walking interpreter.
//!
//! Runs an `IrProgram` at the *pre-codegen* cut point — after
//! `lower_program → optimize_program → monomorphize → ir_link`, but before any
//! of `almide-codegen`'s target-lowering passes. At that point the IR is still
//! a faithful, target-agnostic spec: sugar is desugared, generics are
//! monomorphized, modules are flat, yet none of `ClosureConversion` /
//! `Perceus` / `StdlibLowering` / `IterChain` / … have run. The ~22 codegen-
//! inserted `IrExprKind` variants therefore CANNOT reach this interpreter; the
//! evaluator asserts them unreachable to document (and guard) the boundary.
//!
//! The interpreter is the third leg of the cross-target oracle: a fast,
//! in-process executable spec that can break ties between the native and WASM
//! backends and detect a both-wrong-the-same-way divergence the 2-way vote is
//! structurally blind to.
//!
//! Scope of THIS module set: the evaluator for every eval-able IR node, the
//! runtime/std dispatch bridge, the in-interp HOFs, fuel, and the total-op /
//! abort semantics. The 3-way harness is wired in a later phase.

mod vendored_libm;
mod bridge;
mod dispatch;
mod env;
mod eval;
mod hofs;
mod inplace;
mod stdlib_pool;
mod heap;
mod vfs;
mod value;

pub use value::{Closure, Value, VariantPayload};

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use almide_base::intern::Sym;
use almide_ir::{IrExpr, IrFunction, IrProgram};
use almide_lang::types::Ty;

/// The observable result of an interpreter run — the SAME 3-tuple shape as the
/// existing `run_native_capture` / `run_wasm_capture` harness helpers, plus a
/// classification so the gate can tell a real divergence from "the interp can't
/// run this fixture yet".
#[derive(Debug, Clone)]
pub struct RunOutcome {
    pub status: RunStatus,
    pub stdout: String,
    pub stderr: String,
}

impl RunOutcome {
    /// The process-style exit code the harness compares (0 = clean, 1 = abort).
    pub fn exit_code(&self) -> i32 {
        match self.status {
            RunStatus::Ok => 0,
            RunStatus::Aborted => 1,
            RunStatus::Exited(code) => code,
            // Distinguished markers: the gate excludes these from the 3-way
            // assert rather than emitting a bogus third vote.
            RunStatus::Unsupported(_) => -2,
            RunStatus::FuelExhausted => -3,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RunStatus {
    /// Program completed; `main` returned normally.
    Ok,
    /// Program aborted with a runtime error (div-by-zero, OOB, unhandled
    /// error reaching `main`, panic, failed assert). `stderr` carries the
    /// `Error: <msg>` line and `exit_code()` is 1 — matching both backends.
    Aborted,
    /// A capability the interpreter does not implement (a non-deterministic or
    /// out-of-scope intrinsic). NOT a bug — the gate skips this fixture.
    Unsupported(String),
    /// The fuel / recursion-depth budget was exhausted. NOT a hang or panic —
    /// a clean distinguished outcome for the future fuzz oracle.
    FuelExhausted,
    /// An explicit `process.exit(n)` with a NON-ZERO, NON-ONE code. Both
    /// backends exit with exactly `n`, so the third vote has to carry it: this
    /// used to collapse into `Aborted`, whose `exit_code()` is a flat 1, and
    /// the 3-way gate then read a `process.exit(3)` fixture as
    /// `interp=1 native=3 wasm=3` — a BOTH-BACKENDS-WRONG banner raised by the
    /// ORACLE's own lossy encoding, not by any disagreement in the program
    /// (#1124's fixture). `Ok` and `Aborted` still cover 0 and 1 so every
    /// existing match arm keeps its meaning.
    Exited(i32),
}

/// The interpreter over a fully-linked `IrProgram`.
pub struct Interpreter<'a> {
    pub(crate) program: &'a IrProgram,
    /// The run's argv tail (argv[1..] — what `prim.args_get_list` answers).
    /// Defaults to empty: the oracle harness runs every fixture without
    /// arguments on all three legs, so the empty vec IS the parity value.
    pub(crate) args: Vec<String>,
    /// Top-level functions indexed by name for O(1) call dispatch. Holds
    /// user fns, monomorphized specializations, and any almide-bodied stdlib
    /// fns that were lowered into the program.
    pub(crate) fns: HashMap<Sym, &'a IrFunction>,
    /// `(module, func)` -> almide-bodied stdlib IrFunction, when present.
    /// Populated from `program.modules` (pre-`ir_link`) and from any function
    /// whose name encodes a module path. Used by tier-(i) dispatch.
    pub(crate) module_fns: HashMap<(Sym, Sym), &'a IrFunction>,
    /// The sandboxed fs overlay (#1218) — writes land here, never on disk;
    /// reads fall back to the real filesystem read-only. See `vfs.rs`.
    pub(crate) vfs: vfs::Vfs,
    /// The block heap (#1226): the arena a self-hosted stdlib body's
    /// `prim.handle`/`prim.load*`/`prim.store*` resolve against. Per
    /// interpreter, like `vfs` — `cargo test` runs the gates in parallel
    /// threads, and a shared arena would let one fixture observe another's
    /// blocks.
    pub(crate) heap: heap::Heap,
    /// Names that resolved to POOL bodies (stdlib self-host, not shadowed by
    /// a program fn). The pool tier is address-uniform (#1226 slice 2): a
    /// heap value inside it IS its block address, and only the boundary back
    /// into fixture-tier code rebuilds addresses into `Value`s —
    /// `pool_depth` tracks that boundary. Syncing pool-INTERNAL calls was a
    /// proven wrong vote in both directions: an eager rebuild snapshots a
    /// fresh-alloc-still-to-be-filled (`set_union`'s `__set_alloc` came back
    /// as eight zeros), and no rebuild at all leaks addresses into native
    /// ops (`regex_split`'s pieces printed as integers).
    pub(crate) pool_fns: HashSet<Sym>,
    pub(crate) pool_depth: u32,
    /// The STATIC type of `prim.handle`'s argument at the current call site,
    /// stashed by `eval_module_call` and consumed by `heap_prim_handle`. This
    /// is what disambiguates a byte block from a slot block: the VALUE
    /// `[1,2,3]` is one interp `List` whether the source typed it `Bytes` or
    /// `List[Int]`, but the two spell different memory — 3 payload bytes vs
    /// 3 i64 slots — and a body's `load64` on the wrong one reads garbage
    /// (list_chunk_windows printed 2^56 for 3).
    pub(crate) handle_arg_ty: Option<Ty>,
    /// Field-declaration lists (decl order + default exprs) for record types
    /// and record-variant ctors, keyed by type/ctor name — the interp-side
    /// twin of codegen's `default_fields` pass: a record literal that OMITS a
    /// defaulted field (`maybe: Bool? = none`) must still construct it, or
    /// every later `.maybe` access aborts where both backends read the
    /// default (codec_empty_and_bool, surfaced by #1226 slice 2).
    pub(crate) record_decls: HashMap<Sym, &'a [almide_ir::IrFieldDecl]>,
    /// Named record types keyed by their SORTED field-name set, mapping to
    /// `(type name, declaration-order field names)`. Lets the repr recover the
    /// nominal name + declaration order for a record LITERAL whose inferred type
    /// is structural (`Ty::Record`) rather than `Ty::Named` — e.g. nested list
    /// elements `[{ val: 2, kids: [] }]` whose element type was inferred
    /// structurally. This mirrors the codegen walker's
    /// `ctx.ann.named_records.get(&sorted_names)` lookup
    /// (walker/expressions.rs:520) so `${value}` renders `RNode { .. }` (decl
    /// order), not the anonymous `{ .. }` (sorted) the structural type would
    /// otherwise imply. A field-name set shared by two record types is
    /// ambiguous and intentionally NOT indexed (the structural type is then
    /// treated as a true anonymous record).
    pub(crate) named_records: HashMap<Vec<Sym>, (Sym, Vec<Sym>)>,
    /// Variant constructor registry: case name → `(type name, ctor kind)`,
    /// built once from `program.type_decls`. The old per-call linear scan of
    /// every type decl ran for EVERY Named call (user fn calls included)
    /// before the fn-table lookup. First declaration wins on a shared case
    /// name, exactly like the scan it replaces.
    pub(crate) variant_ctors: HashMap<Sym, (Sym, dispatch::CtorKind)>,
    /// The opaque NEWTYPE decls (`mod type SafeHtml = String`, `local type
    /// JsonPath = Int`) under the identity their ctor call and ctor pattern
    /// carry into the IR — bare for a bundled module's own or the entry
    /// program's plain one, `self.Value` for the entry program's shadow of a
    /// stdlib-owned name, `m.Token` for a module's (#1835). Both backends
    /// ERASE the newtype: native renders the ctor call as the tuple-struct
    /// construction and the pattern as its destructure, the wasm leg lowers
    /// the value to its payload outright — so the interp's ctor arm is an
    /// identity on the argument and its pattern arm a pass-through to the
    /// payload sub-pattern. The same filter as codegen's
    /// `collect_newtype_ctors` (pass_builtin_lowering.rs), minus the dotted
    /// restriction it needs only because bare names never reach its flatten.
    pub(crate) newtype_ctors: HashSet<Sym>,
    /// The global scope holding evaluated top-level lets. Every top-level fn
    /// call and `FnRef` closure parents off this so globals are visible from
    /// nested calls (not just from `main`'s body). Seeded once, lazily.
    /// SPACE 0 of the spaced-global model (#1602) — the program root's frame.
    pub(crate) globals: env::Scope,
    /// Per-MODULE global frames (#1602): space i+1 = `program.modules[i]`.
    /// Separately-lowered modules each restart `VarId`s at 0, so one shared
    /// `VarId`-keyed frame let module A's global overwrite module B's (the
    /// last-wins `by_var` collision). Each module's top-lets now live in its
    /// own frame, and a lowered fn's hop frame parents off ITS module's
    /// frame (`space_scope`), so a body's `VarId`s only ever resolve against
    /// the table they index. Cross-space reads are pre-bound by alias at
    /// init (see `ensure_globals`); a MUTABLE cross-space alias abstains.
    pub(crate) module_globals: Vec<env::Scope>,
    /// `&IrFunction` address → the space whose `VarId`s its body indexes
    /// (0 = root, i+1 = `modules[i]`). Pool fns are absent (their bodies are
    /// self-contained — the pool has no top-lets) and default to space 0.
    pub(crate) fn_space: HashMap<usize, u32>,
    /// The space of the lowered fn currently EXECUTING (set per trampoline
    /// hop, restored on return; 0 = the program root, i+1 = `modules[i]`).
    /// A module body's bare sibling call (`to_string(a)` inside `html.concat`)
    /// resolves against this module first — the scope the checker bound it
    /// in — so two loaded modules sharing a fn name are not ambiguous (#1844).
    pub(crate) cur_space: Cell<u32>,
    pub(crate) globals_ready: Cell<bool>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    /// Decremented per eval step; 0 → `FuelExhausted`.
    pub(crate) fuel: Cell<u64>,
    /// Current call-stack depth, bounded to avoid a native stack overflow on
    /// adversarial deep recursion.
    pub(crate) depth: Cell<u32>,
    /// ADR-0001 deterministic meter (fan.bounded / fan.race). Mirrors the MIR
    /// W1 charge placement EXACTLY — user-fn entries, loop-head checks
    /// (while: n+1 condition evaluations; for-in: n iterations + 1 exit
    /// check), and closure invocations (a lifted lambda's entry charge on the
    /// backends) — and the renderers' budget arithmetic (min-cap entry, lazy
    /// verdict, streaming exit). Counts DOWN from i64::MAX like both legs.
    pub(crate) det_fuel: Cell<i64>,
    /// Entry units of the OPEN region, -1 when none is open — the C-320 reap
    /// sentinel: a cut leaves it armed, and the exhausted read performs the
    /// missed exit bookkeeping before answering (see `budget_prim_rt`).
    pub(crate) det_entry: Cell<i64>,
    pub(crate) det_verdict: Cell<i64>,
    pub(crate) det_spend: Cell<i64>,
    /// True while evaluating a USER fn body. Charges apply only there: the
    /// stdlib pool bodies are unmetered on every leg (both backends meter
    /// user functions only), so a pool fn's internal loops must not charge.
    pub(crate) det_in_user: Cell<bool>,
    /// Open metered regions (budget_enter +1 / budget_exit -1): the strict
    /// cut (T1-1) fires only inside a region — outside one, fuel below zero
    /// is impossible in budget mode and irrelevant in probe mode.
    pub(crate) det_region_depth: Cell<u32>,
    /// C-320: saved-fuel stack, pushed by budget_enter, popped by
    /// budget_exit — the repair pops what a skipped exit left behind.
    /// The open region's saved outer fuel — the reap's restore source (the
    /// exit prim's `saved` operand dies with the cut frame).
    pub(crate) det_saved: Cell<i64>,
    /// The user program's own fn names — captured BEFORE the stdlib pool is
    /// layered into `fns`, so the meter can tell the two apart at call time.
    pub(crate) user_fn_names: HashSet<Sym>,
    /// ENTRY-CHARGE-EXEMPT user fns — the mirror of the backends' region
    /// inliner (lazily built; see `det_entry_exempt`). W1's charge sites are
    /// defined over the SHARED MIR (ADR-0001 「charge site」), and the MIR
    /// inlines loop-free non-recursive user fns into their callers, deleting
    /// the entry charge a per-call AST mirror would count: with `compute.h(0)`
    /// both backends admitted `{ work() }` (zero surviving charges) while this
    /// meter voted exhaust (xtarget-fuzz seed=20260817 index=578).
    det_exempt: std::cell::RefCell<Option<HashSet<Sym>>>,
    /// T5-1 wall-deadline mirror (fan.timeout): absolute deadline (ns since
    /// interp start; i64::MAX = none), hit flag, persisted verdict, and the
    /// wall-check ordinal (the ω of T5-2). Replay/record ride the same env
    /// contract as the backends (`ALMIDE_OMEGA` / `ALMIDE_OMEGA_RECORD`).
    pub(crate) t_deadline: Cell<i64>,
    pub(crate) t_hit: Cell<bool>,
    pub(crate) t_verdict: Cell<i64>,
    pub(crate) t_ord: Cell<i64>,
    pub(crate) t_start: std::time::Instant,
}

/// Default fuel budget — high enough for any real corpus program, low enough to
/// bound an adversarial loop. Roughly 100M eval steps.
pub const DEFAULT_FUEL: u64 = 100_000_000;
/// Recursion-depth ceiling (interp call frames, not Rust frames per se). This is
/// a *semantic* fuel-like bound on call nesting: a clean `FuelExhausted` once a
/// program nests calls this deep, never a native stack overflow. The native
/// stack is decoupled from this number by running the evaluator on a dedicated
/// [`INTERP_STACK_SIZE`]-byte thread (see [`Interpreter::run_main`]) so the
/// guard is host-stack-independent.
///
/// Sizing (empirically measured — `crates/almide-interp/examples/depth_probe*`):
/// a worst-case interp call frame costs ~48 KiB of native stack in an
/// unoptimized (cargo-test `debug`) build — the `eval_expr → eval_call →
/// call_function → eval_expr …` chain is not inlined. So `MAX_DEPTH` frames need
/// `MAX_DEPTH × 48 KiB` of stack. With `INTERP_STACK_SIZE = 256 MiB`:
///   256 MiB / 48 KiB ≈ 5460 frames fit; MAX_DEPTH = 4000 leaves a ~1.37×
///   safety factor (4000 × 48 KiB ≈ 187 MiB < 256 MiB). Both bounds verified by
///   the probe: 4000 frames survive a 192 MiB stack, 5500 survive 256 MiB.
pub const MAX_DEPTH: u32 = 4_000;

/// Dedicated-thread stack size for the evaluator. Decouples [`MAX_DEPTH`] from
/// the caller's thread stack so the recursion bound is host-independent: a
/// program that exhausts [`MAX_DEPTH`] reports a clean `FuelExhausted` whether it
/// runs on a 2 MiB cargo-test worker thread, an 8 MiB main thread, or anywhere
/// else. 256 MiB is *reserved* address space, not committed memory — thread
/// stacks are demand-paged, so only the pages actually touched by the deepest
/// recursion a given run reaches are ever backed by RAM. Sized for `MAX_DEPTH`
/// debug-build frames with margin (see the `MAX_DEPTH` sizing note).
pub const INTERP_STACK_SIZE: usize = 256 * 1024 * 1024;

/// Internal control-flow signal threaded out of `eval`. A `Value` result is the
/// normal case; the others unwind to the nearest handler (loop / function).
pub(crate) enum Flow {
    /// Normal completion with a value.
    Value(Value),
    /// `break` — unwinds to the enclosing loop.
    Break,
    /// `continue` — unwinds to the enclosing loop.
    Continue,
    /// A function-level early return (the value of a `?`/`!` short-circuit, a
    /// `Guard` else, or an explicit return-position). Carries the value the
    /// function should yield.
    Return(Value),
    /// A runtime abort (`Error: <msg>` → stderr, exit 1). Propagates straight
    /// to the top.
    Abort(String),
    /// An explicit `process.exit(n)` — terminate with code n, printing NOTHING
    /// extra (the ALS-T18 assert desugar eprintlns its own line first).
    /// Modeled as Ok for n == 0, Aborted (exit 1) otherwise — the two codes
    /// the deterministic corpus uses.
    Exit(i64),
    /// Out of fuel / too deep. Propagates straight to the top.
    Fuel,
    /// An out-of-scope capability. Propagates straight to the top.
    Unsupported(String),
}

impl Flow {
    pub(crate) fn val(v: Value) -> Flow {
        Flow::Value(v)
    }
}

/// Index named record types by their sorted field-name set, so a record VALUE
/// can recover the nominal name its repr prints.
///
/// A field-name set shared by two distinct record types is ambiguous → drop it
/// (sentinel-marked in `ambiguous`, which also stops a later decl from
/// re-adding it), so the repr falls back to anonymous-record rendering rather
/// than guessing a name.
fn index_named_records(program: &IrProgram) -> HashMap<Vec<Sym>, (Sym, Vec<Sym>)> {
    let mut named_records: HashMap<Vec<Sym>, (Sym, Vec<Sym>)> = HashMap::new();
    let mut ambiguous: HashSet<Vec<Sym>> = HashSet::new();
    let record_decls = program
        .type_decls
        .iter()
        .chain(program.modules.iter().flat_map(|m| m.type_decls.iter()));
    for decl in record_decls {
        let almide_ir::IrTypeDeclKind::Record { fields } = &decl.kind else { continue };
        let decl_order: Vec<Sym> = fields.iter().map(|f| f.name).collect();
        let mut key = decl_order.clone();
        key.sort();
        if ambiguous.contains(&key) {
            continue;
        }
        match named_records.get(&key) {
            // Two record types with identical field-name sets: ambiguous.
            Some(prev) if prev.0 != decl.name => {
                named_records.remove(&key);
                ambiguous.insert(key);
            }
            Some(_) => {}
            None => {
                named_records.insert(key, (decl.name, decl_order));
            }
        }
    }
    named_records
}

/// Variant constructor registry: case name → `(type name, ctor kind)`.
/// Mirrors the linear scan `variant_ctor` used to run per Named call:
/// `program.type_decls` only (module decls were never scanned), in decl
/// order, first declaration of a shared case name wins (`or_insert`).
fn index_variant_ctors(program: &IrProgram) -> HashMap<Sym, (Sym, dispatch::CtorKind)> {
    use almide_ir::{IrTypeDeclKind, IrVariantKind};
    let mut out: HashMap<Sym, (Sym, dispatch::CtorKind)> = HashMap::new();
    for td in &program.type_decls {
        let IrTypeDeclKind::Variant { cases, .. } = &td.kind else { continue };
        for case in cases {
            let kind = match case.kind {
                IrVariantKind::Unit => dispatch::CtorKind::Unit,
                IrVariantKind::Tuple { .. } => dispatch::CtorKind::Tuple,
                IrVariantKind::Record { .. } => dispatch::CtorKind::Record,
            };
            out.entry(case.name).or_insert((td.name, kind));
        }
    }
    out
}

/// The opaque-newtype decl names (program + modules) — a non-public `Alias`
/// whose target is not a fn type, the decls native renders as `pub struct
/// N(T)`. A PUBLIC alias is transparent (no ctor exists), and a fn-typed
/// alias names a signature, not a wrapper.
fn index_newtype_ctors(program: &IrProgram) -> HashSet<Sym> {
    use almide_ir::{IrTypeDeclKind, IrVisibility};
    program
        .type_decls
        .iter()
        .chain(program.modules.iter().flat_map(|m| m.type_decls.iter()))
        .filter(|td| matches!(td.visibility, IrVisibility::Mod | IrVisibility::Private))
        .filter(|td| matches!(&td.kind, IrTypeDeclKind::Alias { target } if !matches!(target, Ty::Fn { .. })))
        .map(|td| td.name)
        .collect()
}

impl<'a> Interpreter<'a> {
    pub fn new(program: &'a IrProgram) -> Self {
        let mut fns = HashMap::new();
        for f in &program.functions {
            fns.insert(f.name, f);
        }
        // The deterministic meter's user-fn set: program fns + user-module fns,
        // captured before the pool layers in (pool bodies are unmetered).
        let mut user_fn_names: HashSet<Sym> = fns.keys().copied().collect();
        for m in &program.modules {
            for f in &m.functions {
                user_fn_names.insert(f.name);
            }
        }
        // Layer in the self-hosted stdlib bodies (lowered once, process-wide) so
        // a stdlib call the interp-native surfaces don't cover evaluates the SAME
        // definition both backends run — see `stdlib_pool`. Program fns are
        // inserted first and `or_insert` keeps them authoritative, so a fixture
        // fn can never be shadowed by a pool body; the pool's own intra-source
        // helper calls (`__sext`) resolve through this same table, and `__`
        // names cannot collide with user code (#868 rejects the prefix).
        let mut pool_fns: HashSet<Sym> = HashSet::new();
        for f in stdlib_pool::pool().fns.values() {
            if let std::collections::hash_map::Entry::Vacant(e) = fns.entry(f.name) {
                e.insert(f);
                pool_fns.insert(f.name);
            }
        }
        let mut module_fns = HashMap::new();
        let mut fn_space: HashMap<usize, u32> = HashMap::new();
        for f in &program.functions {
            fn_space.insert(f as *const IrFunction as usize, 0);
        }
        for (i, m) in program.modules.iter().enumerate() {
            for f in &m.functions {
                module_fns.insert((m.name, f.name), f);
                fn_space.insert(f as *const IrFunction as usize, i as u32 + 1);
            }
        }
        // A module's `__`-prefixed PRIVATE helpers also join the flat table:
        // the module's own bodies call them as bare Named targets (`flag` →
        // `__flag_at`), and #868 rejects the prefix in user code, so a program
        // fn can never collide. Only a name defined in EXACTLY ONE module is
        // indexed — a shared helper name across two modules stays module-keyed
        // and abstains honestly rather than resolving from the wrong source
        // (the #1087 class).
        {
            let mut count: HashMap<Sym, u32> = HashMap::new();
            for m in &program.modules {
                for f in &m.functions {
                    if f.name.as_str().starts_with("__") {
                        *count.entry(f.name).or_insert(0) += 1;
                    }
                }
            }
            for m in &program.modules {
                for f in &m.functions {
                    if f.name.as_str().starts_with("__") && count.get(&f.name) == Some(&1) {
                        fns.entry(f.name).or_insert(f);
                    }
                }
            }
        }
        let named_records = index_named_records(program);
        let variant_ctors = index_variant_ctors(program);
        let newtype_ctors = index_newtype_ctors(program);
        let mut record_decls: HashMap<Sym, &'a [almide_ir::IrFieldDecl]> = HashMap::new();
        for td in program
            .type_decls
            .iter()
            .chain(program.modules.iter().flat_map(|m| m.type_decls.iter()))
        {
            match &td.kind {
                almide_ir::IrTypeDeclKind::Record { fields } => {
                    record_decls.entry(td.name).or_insert(fields);
                }
                almide_ir::IrTypeDeclKind::Variant { cases, .. } => {
                    for c in cases {
                        if let almide_ir::IrVariantKind::Record { fields } = &c.kind {
                            record_decls.entry(c.name).or_insert(fields);
                        }
                    }
                }
                _ => {}
            }
        }

        Interpreter {
            program,
            args: Vec::new(),
            vfs: vfs::Vfs::new(),
            heap: heap::Heap::new(),
            fns,
            module_fns,
            pool_fns,
            pool_depth: 0,
            handle_arg_ty: None,
            record_decls,
            named_records,
            variant_ctors,
            newtype_ctors,
            globals: env::Scope::root(),
            module_globals: program.modules.iter().map(|_| env::Scope::root()).collect(),
            fn_space,
            cur_space: Cell::new(0),
            globals_ready: Cell::new(false),
            stdout: String::new(),
            stderr: String::new(),
            fuel: Cell::new(DEFAULT_FUEL),
            depth: Cell::new(0),
            det_fuel: Cell::new(i64::MAX),
            det_entry: Cell::new(-1),
            det_verdict: Cell::new(0),
            det_spend: Cell::new(0),
            det_in_user: Cell::new(false),
            det_region_depth: Cell::new(0),
            det_saved: Cell::new(0),
            user_fn_names,
            det_exempt: std::cell::RefCell::new(None),
            t_deadline: Cell::new(i64::MAX),
            t_hit: Cell::new(false),
            t_verdict: Cell::new(0),
            t_ord: Cell::new(0),
            t_start: std::time::Instant::now(),
        }
    }

    /// T5-1: monotonic ns since interp start (0 in replay mode — the baked
    /// ordinal decides instead, mirroring both backends).
    pub(crate) fn wall_now_ns(&self) -> i64 {
        self.t_start.elapsed().as_nanos() as i64
    }

    /// T5-2: the replay ordinal (env, same contract as the compile-time bake).
    pub(crate) fn omega_replay() -> i64 {
        std::env::var("ALMIDE_OMEGA").ok().and_then(|v| v.parse().ok()).unwrap_or(-1)
    }

    /// T5-1: the wall-deadline check at a charge site — ordinal + replay or
    /// live clock, exactly the backends' `__wall_hit`.
    pub(crate) fn wall_hit(&self) -> bool {
        if self.t_deadline.get() == i64::MAX {
            return false;
        }
        if self.t_hit.get() {
            return true;
        }
        self.t_ord.set(self.t_ord.get() + 1);
        let omega = Self::omega_replay();
        if omega >= 0 {
            if self.t_ord.get() >= omega {
                self.t_hit.set(true);
            }
            return self.t_hit.get();
        }
        if self.wall_now_ns() >= self.t_deadline.get() {
            self.t_hit.set(true);
        }
        self.t_hit.get()
    }

    /// One deterministic charge unit, if the meter applies here (inside a
    /// user fn). Wrapping like both renderers.
    #[inline]
    pub(crate) fn det_charge(&self) {
        if self.det_in_user.get() {
            self.det_fuel.set(self.det_fuel.get().wrapping_sub(1));
        }
    }

    /// True when `name`'s ENTRY charge is exempt — the fn is loop-free
    /// (no While/ForIn anywhere in its body) and on no call cycle, exactly
    /// the class the shared-MIR inliner folds into its callers, entry charge
    /// and all. Direct or MUTUAL recursion stays charged: TCO rewrites it
    /// into a loop whose head charges survive, so the per-call entry this
    /// mirror counts stays the aligned approximation. A loop-free fn CALLING
    /// a loop-carrying one is exempt itself while the callee keeps charging
    /// (inline-the-wrapper, keep-the-worker). Built once, lazily, from the
    /// user-fn call graph.
    fn det_entry_exempt(&self, name: Sym) -> bool {
        let mut cache = self.det_exempt.borrow_mut();
        let set = cache.get_or_insert_with(|| {
            use almide_ir::visit::{walk_expr, IrVisitor};
            use almide_ir::{CallTarget, IrExpr, IrExprKind};
            struct Scan {
                has_loop: bool,
                calls: HashSet<Sym>,
            }
            impl IrVisitor for Scan {
                fn visit_expr(&mut self, e: &IrExpr) {
                    match &e.kind {
                        IrExprKind::While { .. } | IrExprKind::ForIn { .. } => {
                            self.has_loop = true
                        }
                        IrExprKind::Call { target, .. } => match target {
                            CallTarget::Named { name } => {
                                self.calls.insert(*name);
                            }
                            CallTarget::Module { func, .. } => {
                                self.calls.insert(*func);
                            }
                            _ => {}
                        },
                        _ => {}
                    }
                    walk_expr(self, e);
                }
            }
            let user_bodies: Vec<&IrFunction> = self
                .fns
                .values()
                .copied()
                .filter(|f| self.user_fn_names.contains(&f.name))
                .chain(self.module_fns.values().copied())
                .collect();
            let mut loopy: HashSet<Sym> = HashSet::new();
            let mut edges: HashMap<Sym, HashSet<Sym>> = HashMap::new();
            for f in &user_bodies {
                let mut s = Scan { has_loop: false, calls: HashSet::new() };
                s.visit_expr(&f.body);
                if s.has_loop {
                    loopy.insert(f.name);
                }
                s.calls.retain(|c| self.user_fn_names.contains(c));
                edges.entry(f.name).or_default().extend(s.calls);
            }
            // On-a-cycle = the fn reaches itself through user-fn edges.
            let reaches_self = |start: Sym| -> bool {
                let mut seen: HashSet<Sym> = HashSet::new();
                let mut stack: Vec<Sym> = edges.get(&start).into_iter().flatten().copied().collect();
                while let Some(n) = stack.pop() {
                    if n == start {
                        return true;
                    }
                    if seen.insert(n) {
                        stack.extend(edges.get(&n).into_iter().flatten().copied());
                    }
                }
                false
            };
            user_bodies
                .iter()
                .map(|f| f.name)
                .filter(|n| !loopy.contains(n) && !reaches_self(*n))
                .collect::<HashSet<Sym>>()
        });
        set.contains(&name)
    }

    /// T1-1 strict cut: inside an open metered region with the meter below
    /// zero, execution returns from the current fn with a dummy value (never
    /// observed — the region verdict is already Err). Mirrors the backends'
    /// check-and-return at charge sites.
    #[inline]
    pub(crate) fn det_cut(&self) -> bool {
        self.det_region_depth.get() > 0 && (self.det_fuel.get() < 0 || self.wall_hit())
    }

    /// Override the fuel budget (for tests / the fuzz oracle).
    pub fn with_fuel(mut self, fuel: u64) -> Self {
        self.fuel = Cell::new(fuel);
        self
    }

    /// Supply the run's argv tail (argv[1..]). The oracle harness never sets
    /// this — fixtures run argument-less on all three legs — but a caller
    /// embedding the interp can inject real args.
    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }
}

include!("lib_run.rs");
