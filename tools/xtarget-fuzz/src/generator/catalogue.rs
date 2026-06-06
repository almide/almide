//! The stdlib signature catalogue — the surface the type-directed
//! generator draws calls from.
//!
//! # Provenance
//!
//! Signatures are extracted from a **machine source**: the bundled
//! `stdlib/*.almd` declaration files (the same files the compiler's
//! `bundled_sigs` resolver parses). Each file declares its functions as
//! `fn name[generics](params) -> ret = _`, which the Almide parser
//! turns into `Decl::Fn` nodes we read directly. This guarantees the
//! catalogue tracks the real stdlib: a signature change in the compiler
//! is picked up the next time the fuzzer runs, with no hand-maintained
//! copy to drift.
//!
//! We embed the `.almd` sources with `include_str!` so the fuzzer is a
//! self-contained binary (no runtime dependence on a checkout layout) —
//! the paths are resolved at *build* time relative to this crate.
//!
//! # What we keep
//!
//! A signature is admitted only if every parameter and the return type
//! maps onto a `SigType` the generator understands (concrete or a
//! single-letter type variable). Effectful functions, mutating (`mut`)
//! parameters, and shapes outside the generator's universe (records,
//! variants, bytes/matrix, the integer-width family) are skipped — they
//! belong to the mutation path, not type-directed synthesis.
//!
//! # Weighting
//!
//! On top of the parsed surface we overlay a curated *weight table*
//! that biases selection toward the historic divergence clusters
//! (string/Unicode, float formatting, closures/HOFs). Functions absent
//! from the table get `DEFAULT_WEIGHT`. The weights are the only place
//! detection priorities live — the signatures themselves are neutral.

use std::collections::HashMap;

use almide::ast::{Decl, Program};
use almide::intern::Sym;

use super::denylist::NamedDenylist;
use super::sig_type::SigType;
use super::types::GenType;

/// A catalogued stdlib function the generator can call.
#[derive(Debug, Clone)]
pub struct Signature {
    /// Module name, e.g. `"string"`.
    pub module: &'static str,
    /// Function name, e.g. `"to_upper"`.
    pub func: String,
    /// Parameter names (in order), parallel to `params`. Used to detect
    /// count/size positions so the generator can feed them small,
    /// non-pathological values (a `repeat`/`pad` count of `u32::MAX`
    /// would exhaust memory, manufacturing a noise "hang").
    pub param_names: Vec<String>,
    /// Parameter types (in order). A `SigType::Var` marks a generic
    /// position the generator must instantiate consistently.
    pub params: Vec<SigType>,
    /// Return type (may reference the same type variables as `params`).
    pub ret: SigType,
    /// Selection weight (relative). Higher ⇒ generated more often.
    pub weight: u32,
}

impl Signature {
    /// The type-variable bindings the value grammar *pins* regardless of
    /// the goal: every `Map` key slot and `Result` error slot lowers to
    /// `String` in [`SigType::instantiate`]. Seeding these into the
    /// substitution before goal unification keeps the generator from
    /// binding a pinned variable to a non-`String` type it cannot
    /// actually construct a value of (see `gen_call`).
    pub fn forced_bindings(&self) -> HashMap<String, GenType> {
        let mut forced = HashMap::new();
        for p in &self.params {
            p.collect_forced_bindings(&mut forced);
        }
        self.ret.collect_forced_bindings(&mut forced);
        forced
    }
}

/// Bundled stdlib modules the generator draws from, paired with their
/// embedded source. Only the deterministic, side-effect-free, in-scope
/// modules are listed — no `fs`/`http`/`process`/`random`/`datetime`
/// (effects or non-determinism), no `bytes`/`matrix` (outside the
/// generator's value universe).
///
/// `(module_name, source_text)`.
const BUNDLED_MODULES: &[(&str, &str)] = &[
    ("string", include_str!("../../../../stdlib/string.almd")),
    ("int", include_str!("../../../../stdlib/int.almd")),
    ("float", include_str!("../../../../stdlib/float.almd")),
    ("list", include_str!("../../../../stdlib/list.almd")),
    ("map", include_str!("../../../../stdlib/map.almd")),
    ("set", include_str!("../../../../stdlib/set.almd")),
    ("option", include_str!("../../../../stdlib/option.almd")),
    ("result", include_str!("../../../../stdlib/result.almd")),
    ("math", include_str!("../../../../stdlib/math.almd")),
];

/// Default selection weight for a function not named in
/// [`WEIGHT_OVERRIDES`].
const DEFAULT_WEIGHT: u32 = 4;

/// Curated weight overrides — the detection-priority table.
///
/// `(module, function, weight)`. Weights are relative; the divergence
/// clusters that have historically hidden native↔WASM bugs are boosted
/// so the campaign spends its budget where the payoff is highest:
///
/// - **string/Unicode** case-fold & offset ops: the known ASCII-only
///   WASM case bug lives here, and any new string op is high-risk.
/// - **float formatting** (`to_string`, `to_fixed`, `parse`): Dragon4 /
///   dec2flt clusters.
/// - **closures/HOFs** (`map`, `filter`, `fold`, …): the `fan.map`-class
///   boxing bugs surface through inline lambdas.
const WEIGHT_OVERRIDES: &[(&str, &str, u32)] = &[
    // ── string / Unicode (highest priority) ──
    ("string", "to_upper", 12),
    ("string", "to_lower", 12),
    ("string", "capitalize", 12),
    ("string", "reverse", 9),
    ("string", "slice", 9),
    ("string", "chars", 9),
    ("string", "len", 8),
    ("string", "index_of", 8),
    ("string", "codepoint", 8),
    ("string", "from_codepoint", 8),
    ("string", "count", 7),
    ("string", "replace", 7),
    ("string", "split", 7),
    ("string", "pad_start", 6),
    ("string", "pad_end", 6),
    ("string", "trim", 6),
    // ── float formatting / rounding ──
    ("float", "to_string", 11),
    ("float", "to_fixed", 10),
    ("float", "parse", 9),
    ("float", "round", 7),
    ("float", "sqrt", 6),
    ("float", "abs", 5),
    // ── int width / conversion ──
    ("int", "to_string", 7),
    ("int", "parse", 7),
    ("int", "to_hex", 6),
    ("int", "to_float", 6),
    ("int", "abs", 5),
    // ── higher-order functions (closure surface) ──
    ("list", "map", 9),
    ("list", "filter", 8),
    ("list", "fold", 8),
    ("list", "filter_map", 8),
    ("list", "flat_map", 7),
    ("list", "find", 6),
    ("list", "sort", 6),
    ("list", "sort_by", 6),
    ("option", "map", 7),
    ("option", "filter", 6),
    ("result", "map", 7),
    ("map", "map", 6),
    ("set", "map", 6),
];

/// Build the full catalogue: parse every bundled module, admit the
/// usable signatures, attach weights. Done once at startup.
pub fn build() -> Vec<Signature> {
    let mut out = Vec::new();
    for &(module, source) in BUNDLED_MODULES {
        let program = match parse(source) {
            Some(p) => p,
            None => continue, // a malformed bundled file is a stdlib bug, not ours
        };
        for decl in &program.decls {
            if let Some(sig) = signature_from_decl(module, decl) {
                out.push(sig);
            }
        }
    }
    out
}

/// Parse `.almd` source into a `Program`, tolerating the parser's
/// `Result` contract. Returns `None` on hard parse failure.
fn parse(source: &str) -> Option<Program> {
    let tokens = almide::lexer::Lexer::tokenize(source);
    let mut parser = almide::parser::Parser::new(tokens);
    parser.parse().ok()
}

/// Convert one declaration into a catalogue `Signature`, or `None` if
/// it is not a usable, deterministic, in-universe function.
fn signature_from_decl(module: &'static str, decl: &Decl) -> Option<Signature> {
    let Decl::Fn {
        name,
        effect,
        params,
        return_type,
        ..
    } = decl
    else {
        return None;
    };

    // Skip effectful functions — they can read clocks/files/etc.
    if effect.unwrap_or(false) {
        return None;
    }

    let func = sym_str(*name).to_string();

    // Skip non-deterministic functions (the `effect` flag misses these).
    // Sourced from the shared `NamedDenylist` so the synthesis and
    // mutation paths deny the same surface.
    if NamedDenylist::is_denied_function(module, &func) {
        return None;
    }

    // Skip `mut`-parameter functions: they require a `var` binding and
    // model in-place mutation, which the type-directed value grammar
    // does not produce. (They reach the corpus via mutation instead.)
    if params.iter().any(|p| p.is_mut) {
        return None;
    }
    // Skip functions with default parameters in non-trailing form is
    // unnecessary — Almide allows omitting trailing defaults — but we
    // require *every* parameter to be a synthesizable type regardless.
    let mut param_types = Vec::with_capacity(params.len());
    let mut param_names = Vec::with_capacity(params.len());
    for p in params {
        let st = SigType::from_type_expr(&p.ty)?;
        param_types.push(st);
        param_names.push(sym_str(p.name).to_string());
    }
    let ret = SigType::from_type_expr(return_type)?;

    let weight = weight_for(module, &func);

    Some(Signature {
        module,
        func,
        param_names,
        params: param_types,
        ret,
        weight,
    })
}

/// Look up the curated weight for a function, falling back to
/// [`DEFAULT_WEIGHT`].
fn weight_for(module: &str, func: &str) -> u32 {
    WEIGHT_OVERRIDES
        .iter()
        .find(|(m, f, _)| *m == module && *f == func)
        .map(|(_, _, w)| *w)
        .unwrap_or(DEFAULT_WEIGHT)
}

/// Resolve an interned symbol to its string. Centralized so the call
/// sites read cleanly.
fn sym_str(s: Sym) -> &'static str {
    s.as_str()
}
