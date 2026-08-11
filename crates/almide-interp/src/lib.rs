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
    /// The global scope holding evaluated top-level lets. Every top-level fn
    /// call and `FnRef` closure parents off this so globals are visible from
    /// nested calls (not just from `main`'s body). Seeded once, lazily.
    pub(crate) globals: env::Scope,
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
    /// The user program's own fn names — captured BEFORE the stdlib pool is
    /// layered into `fns`, so the meter can tell the two apart at call time.
    pub(crate) user_fn_names: HashSet<Sym>,
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
        for f in stdlib_pool::pool().fns.values() {
            fns.entry(f.name).or_insert(f);
        }
        let mut module_fns = HashMap::new();
        for m in &program.modules {
            for f in &m.functions {
                module_fns.insert((m.name, f.name), f);
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

        Interpreter {
            program,
            args: Vec::new(),
            vfs: vfs::Vfs::new(),
            fns,
            module_fns,
            named_records,
            globals: env::Scope::root(),
            globals_ready: Cell::new(false),
            stdout: String::new(),
            stderr: String::new(),
            fuel: Cell::new(DEFAULT_FUEL),
            depth: Cell::new(0),
            det_fuel: Cell::new(i64::MAX),
            det_entry: Cell::new(0),
            det_verdict: Cell::new(0),
            det_spend: Cell::new(0),
            det_in_user: Cell::new(false),
            det_region_depth: Cell::new(0),
            user_fn_names,
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

    /// Run the program's `main` entry point and return the observable outcome.
    ///
    /// The evaluation runs on a dedicated [`INTERP_STACK_SIZE`]-byte thread so
    /// the [`MAX_DEPTH`] recursion bound is decoupled from the *caller's* thread
    /// stack: a deeply-recursive program reports a clean `FuelExhausted` instead
    /// of a native stack overflow whether it is driven from a 2 MiB cargo-test
    /// worker, the 8 MiB main thread, or any other host stack. Only the
    /// `Send + Sync` `&IrProgram` crosses into the thread and the `Send`
    /// `RunOutcome` crosses back — the `Rc`/`Cell` evaluator state never leaves
    /// it. A `std::thread::scope` borrows the program in place (no `'static`
    /// requirement) and joins before returning, so the borrow is sound.
    ///
    /// The fuel budget is captured *before* spawning (the `Interpreter` itself is
    /// not `Send`); the worker rebuilds a fresh interpreter over the same program
    /// inside the big-stack thread — `Interpreter::new` only indexes the program,
    /// so this is cheap and observationally identical.
    pub fn run_main(self) -> RunOutcome {
        let program: &'a IrProgram = self.program;
        let fuel = self.fuel.get();
        std::thread::scope(|scope| {
            std::thread::Builder::new()
                .name("almide-interp".to_string())
                // The whole point: a big, KNOWN stack so MAX_DEPTH — not the
                // host thread's stack — is the binding recursion bound.
                .stack_size(INTERP_STACK_SIZE)
                .spawn_scoped(scope, move || {
                    Interpreter::new(program).with_fuel(fuel).run_main_on_stack()
                })
                .expect("failed to spawn almide-interp worker thread")
                .join()
                // A panic inside the evaluator is a genuine interpreter bug, not
                // an out-of-scope skip — re-raise it so it is never swallowed.
                .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
        })
    }

    /// The actual `main`-driving logic. Runs on the big-stack worker thread
    /// spawned by [`run_main`](Self::run_main). Never call this directly off the
    /// dedicated thread — it carries the deep recursion that [`run_main`]'s stack
    /// is sized for.
    fn run_main_on_stack(mut self) -> RunOutcome {
        let main = match self.fns.get(&almide_base::intern::sym("main")) {
            Some(f) => *f,
            None => {
                // No entry point: a program of pure definitions. Treat as a
                // clean no-op run (matches `almide run` on a fn-only file,
                // which also produces no output).
                return RunOutcome {
                    status: RunStatus::Ok,
                    stdout: self.stdout,
                    stderr: self.stderr,
                };
            }
        };

        // Seed top-level lets into the shared global scope before main runs.
        if let Err(flow) = self.ensure_globals() {
            return self.outcome_from_flow(flow);
        }
        let root = self.globals.clone();

        match self.call_function(main, Vec::new(), &root) {
            // A `main` body whose value is an unhandled `Err`/`None` terminates
            // the program with `Error: <inner>` + exit 1 (the unhandled-main-
            // error termination contract). An `Ok`/`Some`/other value is a
            // clean exit. `call_function` already collapses a body-level
            // `Return` into `Value`.
            Flow::Value(v) => match unhandled_main_error(&v) {
                Some(msg) => self.outcome_from_flow(Flow::Abort(msg)),
                None => RunOutcome {
                    status: RunStatus::Ok,
                    stdout: self.stdout,
                    stderr: self.stderr,
                },
            },
            other => self.outcome_from_flow(other),
        }
    }

    /// Evaluate top-level lets into the shared `globals` scope exactly once.
    /// Idempotent: a second call is a no-op (so nested calls that trigger it do
    /// not re-run any effectful top-let).
    pub(crate) fn ensure_globals(&mut self) -> Result<(), Flow> {
        if self.globals_ready.get() {
            return Ok(());
        }
        // Mark ready up front so a top-let that calls a fn (which itself wants
        // globals) does not recurse into re-seeding.
        self.globals_ready.set(true);
        let globals = self.globals.clone();
        // DEPENDENCY-ORDERED init (#632, C-007): a top-let whose initializer reads a
        // LATER-declared global (directly or through a fn it calls — `BANNER =
        // make_banner()` reading `APP_NAME`) must see it already bound. Both backends
        // interprocedurally topo-sort the declaration order (`dependency_init_order`);
        // evaluating in bare declaration order here left the forward-referenced global
        // unbound (`unbound variable APP_NAME`) — a WRONG third vote vs the native==wasm
        // consensus. Reuse the SAME ordering utility so the interp matches by construction.
        use almide_ir::top_let_storage::{
            build_global_tables, dependency_init_order, top_let_inputs,
        };
        let mut inputs = Vec::new();
        for tl in &self.program.top_lets {
            inputs.push(top_let_inputs(tl));
        }
        for m in &self.program.modules {
            for tl in &m.top_lets {
                inputs.push(top_let_inputs(tl));
            }
        }
        let (_globals_info, alias, _offenders) =
            build_global_tables(&inputs, &self.program.var_table);
        let order = dependency_init_order(self.program, &alias);
        // Index every top-let (root + modules) by its VarId so the sorted order can
        // fetch its initializer. A VarId in `order` but absent here (unreachable) is
        // skipped; a top-let absent from `order` (defensive) falls back to decl order.
        let mut by_var: std::collections::HashMap<almide_ir::VarId, &almide_ir::IrExpr> =
            std::collections::HashMap::new();
        for tl in &self.program.top_lets {
            by_var.insert(tl.var, &tl.value);
        }
        for m in &self.program.modules {
            for tl in &m.top_lets {
                by_var.insert(tl.var, &tl.value);
            }
        }
        let mut seen: std::collections::HashSet<almide_ir::VarId> =
            std::collections::HashSet::new();
        let ordered: Vec<(almide_ir::VarId, &almide_ir::IrExpr)> = order
            .iter()
            .filter_map(|v| by_var.get(v).map(|e| (*v, *e)))
            .chain(
                // Any top-let the sort omitted (a self-referential cycle the topo-sort
                // dropped) is appended in declaration order — never silently unbound.
                self.program
                    .top_lets
                    .iter()
                    .map(|tl| (tl.var, &tl.value))
                    .chain(self.program.modules.iter().flat_map(|m| {
                        m.top_lets.iter().map(|tl| (tl.var, &tl.value))
                    })),
            )
            .filter(|(v, _)| seen.insert(*v))
            .collect();
        for (var, value) in ordered {
            match self.eval_expr(value, &globals) {
                Flow::Value(v) => globals.bind(var, v),
                other => return Err(other),
            }
        }
        Ok(())
    }

    fn outcome_from_flow(&self, flow: Flow) -> RunOutcome {
        match flow {
            Flow::Value(_) | Flow::Return(_) | Flow::Break | Flow::Continue => RunOutcome {
                status: RunStatus::Ok,
                stdout: self.stdout.clone(),
                stderr: self.stderr.clone(),
            },
            Flow::Abort(msg) => {
                let mut stderr = self.stderr.clone();
                // The unhandled-error / abort termination contract: a single
                // `Error: <msg>` line on stderr, exit 1 (matches both backends'
                // main-error termination).
                stderr.push_str(&format!("Error: {}\n", msg));
                RunOutcome {
                    status: RunStatus::Aborted,
                    stdout: self.stdout.clone(),
                    stderr,
                }
            }
            Flow::Exit(code) => RunOutcome {
                status: match code {
                    0 => RunStatus::Ok,
                    1 => RunStatus::Aborted,
                    n => RunStatus::Exited(n as i32),
                },
                stdout: self.stdout.clone(),
                stderr: self.stderr.clone(),
            },
            Flow::Fuel => RunOutcome {
                status: RunStatus::FuelExhausted,
                stdout: self.stdout.clone(),
                stderr: self.stderr.clone(),
            },
            Flow::Unsupported(what) => RunOutcome {
                status: RunStatus::Unsupported(what),
                stdout: self.stdout.clone(),
                stderr: self.stderr.clone(),
            },
        }
    }

    /// Burn one unit of fuel; returns `Err(Flow::Fuel)` when exhausted.
    pub(crate) fn step(&self) -> Result<(), Flow> {
        let f = self.fuel.get();
        if f == 0 {
            return Err(Flow::Fuel);
        }
        self.fuel.set(f - 1);
        Ok(())
    }

    /// Bind a function's params and evaluate its body in a fresh frame parented
    /// at the program root scope (`base`). Top-level fns do not close over the
    /// caller's locals — only over top-level lets — so `base` is the root.
    pub(crate) fn call_function(
        &mut self,
        func: &'a IrFunction,
        args: Vec<Value>,
        base: &env::Scope,
    ) -> Flow {
        self.call_function_keeping_frame(func, args, base).0
    }

    /// [`Self::call_function`], but the callee's FRAME survives the call so the
    /// caller can read the final values of `mut` parameters back out — the
    /// copy-out half of the backends' mut-param lowering (C-132: the callee
    /// returns the buffer and every call position writes it back). See
    /// `eval_named_call`'s write-back (#1022).
    pub(crate) fn call_function_keeping_frame(
        &mut self,
        func: &'a IrFunction,
        args: Vec<Value>,
        base: &env::Scope,
    ) -> (Flow, env::Scope) {
        self.run_callable(TailCallee::Fn(func), args, base)
    }

    /// The tail-call trampoline: the shared engine under every function and
    /// closure application.
    ///
    /// The backends convert tail calls to loops (tco_rewrite; C-178's
    /// `return_call`/`return_call_indirect` on wasm), so `sum_to(1_000_000, 0)`
    /// runs in O(1) stack there — while a frame-per-call evaluator dies at
    /// [`MAX_DEPTH`] and abstains on programs the contracts DEFINE as
    /// constant-stack. This loop is the interp's mirror of that guarantee:
    /// each hop binds a fresh frame, walks the body's TAIL SPINE
    /// (`eval_body_spine`: Block tails, If branches, the effect `Try{Call}`
    /// wrapper), and when the spine ends in a call to a lowered fn or a
    /// closure, RE-ENTERS with the callee instead of recursing.
    ///
    /// What is preserved exactly:
    ///   - resolution order — the spine walker re-checks builtins / variant
    ///     ctors / Endian before the fn table, same as `eval_named_call`;
    ///   - the deterministic meter — the per-hop entry charge fires each
    ///     transfer, matching the backends' charge-inside-the-loop placement;
    ///   - `Try`/`Unwrap` — a transfer through the effect wrapper records the
    ///     marker's node type (run-length compressed), and the final base
    ///     value is folded through the SAME normalization `eval_try_unwrap`
    ///     applies, innermost-first, so N∘…∘N is computed, not approximated;
    ///   - mut-param copy-out — a callee with a `mut` param DECLINES the
    ///     transfer (evaluated via the plain nested path), because copy-out
    ///     needs the caller's lvalue, which a transferred frame no longer has.
    fn run_callable(
        &mut self,
        mut callee: TailCallee<'a>,
        mut args: Vec<Value>,
        base: &env::Scope,
    ) -> (Flow, env::Scope) {
        let d = self.depth.get();
        if d >= MAX_DEPTH {
            return (Flow::Fuel, base.child());
        }
        self.depth.set(d + 1);
        let det_was_user = self.det_in_user.get();

        // First hop's frame — what mut-param copy-out reads. Meaningful only
        // when no transfer happened, and a transfer implies no mut params.
        let mut first_frame: Option<env::Scope> = None;
        // (marker node type, run length) — pending `Try` normalizations.
        let mut pending: Vec<(Ty, u32)> = Vec::new();

        let result = 'tramp: loop {
            // Per-hop entry charge (the backends charge inside the loop).
            match &callee {
                TailCallee::Fn(f) => {
                    let det_is_user = self.user_fn_names.contains(&f.name);
                    self.det_in_user.set(det_is_user);
                    if det_is_user {
                        self.det_fuel.set(self.det_fuel.get().wrapping_sub(1));
                        if self.det_cut() {
                            break 'tramp Flow::Value(Value::Int(0));
                        }
                    }
                }
                TailCallee::Clo(_) => {
                    self.det_charge();
                    if self.det_cut() {
                        break 'tramp Flow::Value(Value::Int(0));
                    }
                }
            }

            // Bind the hop's frame. A closure hop keeps its body Rc alive for
            // the duration of the spine walk.
            let (frame, fn_body, clo_body): (env::Scope, Option<&'a IrExpr>, Option<Rc<Closure>>) =
                match &callee {
                    TailCallee::Fn(f) => {
                        let fr = base.child();
                        for (param, arg) in f.params.iter().zip(args.drain(..)) {
                            fr.bind(param.var, arg);
                        }
                        (fr, Some(&f.body), None)
                    }
                    TailCallee::Clo(c) => {
                        let fr = c.captured.child();
                        for (param, arg) in c.params.iter().zip(args.drain(..)) {
                            fr.bind(*param, arg);
                        }
                        (fr, None, Some(Rc::clone(c)))
                    }
                };
            if first_frame.is_none() {
                first_frame = Some(frame.clone());
            }
            let outcome = match (&fn_body, &clo_body) {
                (Some(b), _) => self.eval_body_spine(b, &frame),
                (_, Some(c)) => self.eval_body_spine(&c.body, &frame),
                _ => unreachable!("one body source is always set"),
            };
            match outcome {
                SpineOutcome::Done(flow) => break 'tramp flow,
                SpineOutcome::Transfer { next, next_args, try_marker } => {
                    if let Some(ty) = try_marker {
                        match pending.last_mut() {
                            Some((last, n)) if *last == ty => *n += 1,
                            _ => pending.push((ty, 1)),
                        }
                    }
                    callee = next;
                    args = next_args;
                }
            }
        };

        // A function-body `Return` resolves to the returned value at the fn
        // boundary; then the pending Try normalizations fold over the value,
        // innermost-first — exactly what the nested evaluation would compute.
        let mut result = match result {
            Flow::Return(v) | Flow::Value(v) => Flow::Value(v),
            other => other,
        };
        'fold: for (ty, n) in pending.iter().rev() {
            for _ in 0..*n {
                let Flow::Value(v) = result else { break 'fold };
                result = match self.try_unwrap_value(v, ty) {
                    // `Return(x)` here means "that level's fn returns x" —
                    // which is the next level's call VALUE.
                    Flow::Return(v) | Flow::Value(v) => Flow::Value(v),
                    other => other,
                };
            }
        }
        self.det_in_user.set(det_was_user);
        self.depth.set(self.depth.get() - 1);
        (result, first_frame.unwrap_or_else(|| base.child()))
    }

    /// Apply a closure value to arguments. Used by the in-interp HOFs and by
    /// `Computed` call targets. Rides the same trampoline as named fns, so a
    /// lambda whose tail is a call (C-178's indirect-recursion cycle) costs
    /// O(1) evaluator depth per chain, matching the backends' `return_call`.
    pub(crate) fn apply_closure(&mut self, clo: &Rc<Closure>, args: Vec<Value>) -> Flow {
        let root = self.root_scope();
        self.run_callable(TailCallee::Clo(Rc::clone(clo)), args, &root).0
    }
}

/// A tail-transferable callee: a lowered named function (program-lifetime
/// borrow) or a closure value (owned Rc). What [`Interpreter::run_callable`]
/// loops over.
pub(crate) enum TailCallee<'a> {
    Fn(&'a IrFunction),
    Clo(Rc<Closure>),
}

/// One hop's verdict from the tail-spine walker.
pub(crate) enum SpineOutcome<'a> {
    /// The body does not end in a transferable call — this flow is the hop's
    /// result (a `Return` is resolved at the engine's fn boundary).
    Done(Flow),
    /// The body's tail is a call to `next` — re-enter the trampoline.
    /// `try_marker` carries the `Try`/`Unwrap` marker node's type when the
    /// tail was the effect wrapper `Try{Call}`, so the engine can fold the
    /// normalization over the final value.
    Transfer { next: TailCallee<'a>, next_args: Vec<Value>, try_marker: Option<Ty> },
}

/// If `main`'s result value is an unhandled error, return the message that the
/// program should terminate with (`Error: <msg>`). An `Err(e)` yields `e`
/// displayed bare (a String error prints raw, matching native
/// `Error: invalid digit found in string`); a `None` yields a generic message.
/// Any other value (incl. `Ok`/`Some`/`Unit`) is a clean exit (`None`).
fn unhandled_main_error(v: &Value) -> Option<String> {
    match v {
        Value::Result(Err(e)) => Some(e.display_bare()),
        Value::Option(None) => Some("called `Option::unwrap()` on a `None` value".to_string()),
        _ => None,
    }
}

/// Convenience: build an interpreter for `program` and run `main`.
pub fn interpret(program: &IrProgram) -> RunOutcome {
    Interpreter::new(program).run_main()
}
