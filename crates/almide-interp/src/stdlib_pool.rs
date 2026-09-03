//! The self-hosted stdlib bodies, lowered once and shared by every interpreter.
//!
//! The stdlib is self-hosted: `int.to_int8` is not a Rust intrinsic but pure
//! Almide (`stdlib/int_sized.almd` — `prim.band` + a sign-extend helper), and the
//! WASM leg links exactly those bodies into every build via
//! `almide_lang::self_host_registry`. This module lowers the SAME sources through
//! the SAME frontend, so the third oracle can evaluate the same definition both
//! backends execute instead of hand-mirroring it in `bridge.rs` — the
//! restate-a-subset drift class (an `Int | Bool` here, an identity-conversion
//! there) that keeps producing acceptance gaps elsewhere in the compiler.
//!
//! What this deliberately does NOT change:
//!
//! - **Evaluability is decided per fn, at call time, by what the body touches.**
//!   There is no hand-maintained "these modules are scalar" list. A body that
//!   reaches a heap/effect prim (`prim.handle`, `prim.store8`, `prim.fd_write`…)
//!   hits the bridge's honest `None` and the call abstains with that prim named —
//!   a skip, never a guess. The scalar prim floor in `bridge.rs` is the whole
//!   vocabulary a body may consume.
//! - **The interp-native surfaces still win.** Containers are modeled as
//!   `Value::List`/`Value::Map`, not linear memory, so `hofs.rs` remains the
//!   faithful implementation for them; the pool is a FALLBACK tier consulted only
//!   after the existing dispatch misses. Flipping specific bridge arms over to
//!   their pool bodies is follow-up work, judged arm by arm by the 3-way gate.
//!
//! Lowering stops at `lower_program` — no optimize/mono/ir_link. The pool is a
//! declaration harvest with no `main`, so DCE would empty it; the registry impl
//! fns are non-generic by construction (the wasm leg links them unmonomorphized);
//! and `lower_program` output is exactly the pre-codegen node set the evaluator
//! admits. Sources that fail to lower under this recipe (an effect-heavy module
//! wanting an import the empty recipe does not load) are recorded and their fns
//! simply stay uncovered — identical to the pre-pool state, never worse.

use std::collections::HashMap;
use std::sync::OnceLock;

use almide_base::intern::{sym, Sym};
use almide_frontend::canonicalize;
use almide_frontend::check::Checker;
use almide_frontend::lower::lower_program;
use almide_ir::IrFunction;
use almide_lang::lexer::Lexer;
use almide_lang::parser::Parser;

pub(crate) struct StdlibPool {
    /// Every impl fn from every registry source that lowered cleanly, keyed by
    /// its flat impl name (`int_to_int8`). Helpers (`__sext`) ride along so
    /// intra-source calls resolve through the interpreter's layered fn table.
    pub(crate) fns: HashMap<Sym, IrFunction>,
    /// `("int", "to_int8")` → `int_to_int8` — the registry's mapping, split at
    /// the dot. Present even when the impl fn failed to lower (the dispatch
    /// tier then misses on the fn lookup and falls through to `Unsupported`,
    /// exactly as before the pool existed).
    pub(crate) call_map: HashMap<(Sym, Sym), Sym>,
    /// impl name → `(module, func)` — `call_map` reversed (first writer wins;
    /// the registry is injective in practice).
    pub(crate) impl_to_call: HashMap<Sym, (Sym, Sym)>,
}

/// The pool, built on first use and shared for the process lifetime. A single
/// harness run interprets ~130 fixtures; lowering the registry once instead of
/// per fixture is what makes the pool affordable.
pub(crate) fn pool() -> &'static StdlibPool {
    static POOL: OnceLock<StdlibPool> = OnceLock::new();
    POOL.get_or_init(build)
}

/// The impl fn a `module.func` call resolves to, when the registry names one.
pub(crate) fn impl_fn(module: Sym, func: Sym) -> Option<Sym> {
    pool().call_map.get(&(module, func)).copied()
}

/// The reverse: the `(module, func)` a registry IMPL name stands for. Lets a
/// NAMED call of an impl (`string_slice` — how a lowered MODULE body spells
/// `string.slice`) take the same bridge-first resolution a module-spelled call
/// takes, instead of running the pool body straight into its heap prims.
pub(crate) fn module_of_impl(impl_name: Sym) -> Option<(Sym, Sym)> {
    pool().impl_to_call.get(&impl_name).copied()
}

fn build() -> StdlibPool {
    let mut fns: HashMap<Sym, IrFunction> = HashMap::new();
    let mut call_map: HashMap<(Sym, Sym), Sym> = HashMap::new();
    let mut impl_to_call: HashMap<Sym, (Sym, Sym)> = HashMap::new();
    for (source, entries) in almide_lang::self_host_registry::self_host_runtime() {
        for (impl_name, call_name) in entries.iter() {
            if let Some((m, f)) = call_name.split_once('.') {
                call_map.insert((sym(m), sym(f)), sym(impl_name));
                impl_to_call.entry(sym(impl_name)).or_insert((sym(m), sym(f)));
            }
        }
        if let Some(lowered) = lower_registry_source(source, owning_module(entries)) {
            for f in lowered {
                // First writer wins: registry sources are name-disjoint (the wasm
                // leg splices them into one namespace), so a collision would be a
                // registry bug — keeping the first is deterministic either way.
                fns.entry(f.name).or_insert(f);
            }
        }
    }
    StdlibPool { fns, call_map, impl_to_call }
}

/// The bundled module a registry source belongs to: the ONE module every
/// entry names (`SRC_BYTES_TYPED` → `bytes`). A source serving several
/// modules (`SRC_CLOCK_NOW` → `datetime` + `env`) has no single owner and
/// lowers as a plain entry program, as every source did before.
fn owning_module(entries: &[(&str, &'static str)]) -> Option<&'static str> {
    let mut modules = entries.iter().filter_map(|(_, call)| call.split_once('.').map(|(m, _)| m));
    let first = modules.next()?;
    modules.all(|m| m == first).then_some(first)
}

/// Parse + check + lower ONE registry source, `None` when any stage declines.
/// The `catch_unwind` mirrors the harness's fixture lowering: a frontend
/// `assert` on an out-of-scope construct must degrade to "this source is
/// uncovered", never poison the whole pool.
///
/// The source is checked AS its owning module (`canonicalize_program_in`,
/// the driver's bundled-entry recipe, #1837): an unprefixed type it declares
/// (`type Endian` in the bytes source) keeps the stdlib's bare key instead of
/// the `self.`-qualified shadow key an entry program's declaration takes —
/// the same identity leak one level down that #1837 closed for the driver.
fn lower_registry_source(source: &str, module: Option<&str>) -> Option<Vec<IrFunction>> {
    let src = source.to_string();
    let module = module.map(str::to_string);
    std::panic::catch_unwind(move || {
        let tokens = Lexer::tokenize(&src);
        let mut parser = Parser::new(tokens);
        let mut prog = parser.parse().ok()?;
        if !parser.errors.is_empty() {
            return None;
        }
        let canon =
            canonicalize::canonicalize_program_in(&prog, std::iter::empty(), module.as_deref());
        let mut checker = Checker::from_env(canon.env);
        let diags = checker.infer_program(&mut prog);
        if diags.iter().any(|d| d.level == almide_frontend::diagnostic::Level::Error) {
            return None;
        }
        let ir = lower_program(&prog, &checker.env, &checker.type_map);
        Some(ir.functions)
    })
    .ok()
    .flatten()
}
