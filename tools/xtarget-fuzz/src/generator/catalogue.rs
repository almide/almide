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

use almide::ast::{Decl, ExprKind, Program};
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
    /// The one `mut` parameter, if any: the call needs a `var` there, and a
    /// `Unit` return means the mutated value is the call's observable
    /// result ([`Signature::effective_ret`]).
    pub mut_param: Option<usize>,
    /// An `effect fn` from a module the sandbox makes reproducible
    /// ([`SANDBOXED_EFFECT_MODULES`]): the program's `main` becomes an
    /// `effect fn`, and its arguments name things the sandbox holds.
    pub effect: bool,
    /// An effect fn with an Almide body: its call is lifted to
    /// `Result[T, String]` (the checker's view; the ledger writes the
    /// signature `-> T ! String`). An `= _` intrinsic keeps its declared type.
    pub lifted: bool,
}

/// Effect modules whose functions are reproducible inside the fuzzer's
/// sandbox (`oracle/sandbox.rs`: a seeded tree, fixed argv, env and stdin),
/// so their effect functions are catalogued (tier E1,
/// docs/roadmap/active/effect-inspection-harness.md). Every other effect
/// function stays out.
pub const SANDBOXED_EFFECT_MODULES: &[&str] = &["fs", "env", "io", "args"];

impl Signature {
    /// The type a call yields to the generator: a `Unit`-returning mutator
    /// yields its mutated `var`; a bodied effect function ([`Self::lifted`])
    /// yields `Result[T, String]` (its declared `T` unless that is already a
    /// Result); every other function its declared return.
    pub fn effective_ret(&self) -> SigType {
        match self.mut_param {
            Some(m) if self.ret == SigType::Unit => return self.params[m].clone(),
            _ => {}
        }
        match &self.ret {
            SigType::Result(..) => self.ret.clone(),
            r if self.lifted => SigType::Result(Box::new(r.clone()), Box::new(SigType::String)),
            r => r.clone(),
        }
    }

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
/// (effects or non-determinism). The fixed-width numbers, `bytes`, `matrix`
/// and `value` are in: the value grammar builds them (`GenType::Sized`,
/// `Bytes`, `Matrix`, `Value`).
///
/// `base64` / `hex` / `hash` / `zlib` / `path` / `url` / `regex` are pure but
/// not auto-imported: a program that calls one starts with `import <module>`
/// ([`imports_for`]); so is `json`, whose functions over a `Value` or a
/// `String` the grammar reaches. `error` and `value` are auto-imported.
///
/// `(module_name, source_text, needs_import)`.
const BUNDLED_MODULES: &[(&str, &str, bool)] = &[
    ("string", include_str!("../../../../stdlib/string.almd"), false),
    ("int", include_str!("../../../../stdlib/int.almd"), false),
    ("float", include_str!("../../../../stdlib/float.almd"), false),
    ("list", include_str!("../../../../stdlib/list.almd"), false),
    ("map", include_str!("../../../../stdlib/map.almd"), false),
    ("set", include_str!("../../../../stdlib/set.almd"), false),
    ("option", include_str!("../../../../stdlib/option.almd"), false),
    ("result", include_str!("../../../../stdlib/result.almd"), false),
    ("math", include_str!("../../../../stdlib/math.almd"), false),
    ("int8", include_str!("../../../../stdlib/int8.almd"), false),
    ("int16", include_str!("../../../../stdlib/int16.almd"), false),
    ("int32", include_str!("../../../../stdlib/int32.almd"), false),
    ("int64", include_str!("../../../../stdlib/int64.almd"), false),
    ("uint8", include_str!("../../../../stdlib/uint8.almd"), false),
    ("uint16", include_str!("../../../../stdlib/uint16.almd"), false),
    ("uint32", include_str!("../../../../stdlib/uint32.almd"), false),
    ("uint64", include_str!("../../../../stdlib/uint64.almd"), false),
    ("float32", include_str!("../../../../stdlib/float32.almd"), false),
    ("float64", include_str!("../../../../stdlib/float64.almd"), false),
    ("bytes", include_str!("../../../../stdlib/bytes.almd"), false),
    ("matrix", include_str!("../../../../stdlib/matrix.almd"), false),
    ("error", include_str!("../../../../stdlib/error.almd"), false),
    ("value", include_str!("../../../../stdlib/value.almd"), false),
    ("base64", include_str!("../../../../stdlib/base64.almd"), true),
    ("hex", include_str!("../../../../stdlib/hex.almd"), true),
    ("hash", include_str!("../../../../stdlib/hash.almd"), true),
    ("zlib", include_str!("../../../../stdlib/zlib.almd"), true),
    ("path", include_str!("../../../../stdlib/path.almd"), true),
    ("url", include_str!("../../../../stdlib/url.almd"), true),
    ("regex", include_str!("../../../../stdlib/regex.almd"), true),
    ("json", include_str!("../../../../stdlib/json.almd"), true),
    ("fs", include_str!("../../../../stdlib/fs.almd"), true),
    ("env", include_str!("../../../../stdlib/env.almd"), true),
    ("io", include_str!("../../../../stdlib/io.almd"), true),
    ("args", include_str!("../../../../stdlib/args.almd"), true),
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
/// usable signatures, attach weights. Done once at startup. A function no
/// wasm build path serves is left out: a program that calls it fails the
/// wasm build with the documented E081 every time, which is a finding about
/// the generator, not about either leg.
pub fn build() -> Vec<Signature> {
    let unavailable = wasm_unavailable(TARGET_AVAILABILITY);
    let mut out = Vec::new();
    for &(module, source, _) in BUNDLED_MODULES {
        let program = match parse(source) {
            Some(p) => p,
            None => continue, // a malformed bundled file is a stdlib bug, not ours
        };
        for decl in &program.decls {
            if let Some(sig) = signature_from_decl(module, decl) {
                if !unavailable.contains(&format!("{}.{}", sig.module, sig.func)) {
                    out.push(sig);
                }
            }
        }
    }
    out
}

/// The `import` lines a generated program needs: one per catalogue module
/// that is not auto-imported and that `src` calls (`<module>.` after a
/// non-identifier character).
pub fn imports_for(src: &str) -> String {
    let mut out = String::new();
    for &(module, _, needs_import) in BUNDLED_MODULES {
        if !needs_import {
            continue;
        }
        let call = format!("{module}.");
        let used = src.match_indices(&call).any(|(i, _)| {
            !src[..i].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.')
        });
        if used {
            out.push_str(&format!("import {module}\n"));
        }
    }
    out
}

/// The declared availability matrix (#1423), the same file the E081
/// check-time diagnostic reads.
const TARGET_AVAILABILITY: &str = include_str!("../../../../proofs/target-availability.toml");

/// `module.fn` of every row whose `legs` include `stock-p1`: the default
/// `build --target wasm` serves it on no path (E081).
fn wasm_unavailable(toml: &str) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    let mut current: Option<String> = None;
    for line in toml.lines().map(str::trim) {
        if let Some(v) = line.strip_prefix("fn = ") {
            current = Some(v.trim_matches('"').to_string());
        } else if line.starts_with("legs = ") && line.contains("\"stock-p1\"") {
            if let Some(f) = current.take() {
                out.insert(f);
            }
        } else if line.starts_with('[') {
            current = None;
        }
    }
    out
}

#[cfg(test)]
mod availability_tests {
    use super::*;

    #[test]
    fn native_only_functions_are_left_out_of_the_catalogue() {
        let unavailable = wasm_unavailable(TARGET_AVAILABILITY);
        assert!(unavailable.contains("matrix.linear_q1_0_row_no_bias"));
        let cat = build();
        assert!(!cat.iter().any(|s| unavailable.contains(&format!("{}.{}", s.module, s.func))));
        assert!(cat.iter().any(|s| s.module == "matrix" && s.func == "dot_row"), "a served matrix fn stays");
    }

    #[test]
    fn value_and_json_functions_over_the_grammar_are_catalogued() {
        let names: Vec<String> = build().iter().map(|s| format!("{}.{}", s.module, s.func)).collect();
        for want in ["value.field", "value.pick", "value.to_camel_case", "value.stringify", "json.parse", "json.stringify", "json.to_map"] {
            assert!(names.iter().any(|n| n == want), "{want} is not in the catalogue");
        }
        let src = "fn main() -> Unit = {\n  let s = json.stringify(value.null())\n}\n";
        assert_eq!(imports_for(src), "import json\n");
    }

    #[test]
    fn private_helpers_are_left_out_of_the_catalogue() {
        let private: Vec<String> = build().iter().filter(|s| s.func.starts_with("__")).map(|s| format!("{}.{}", s.module, s.func)).collect();
        assert!(private.is_empty(), "private helpers in the catalogue: {private:?}");
    }

    #[test]
    fn a_program_calling_an_import_only_module_gets_its_import() {
        let src = "fn main() -> Unit = {\n  let a = base64.encode(bytes.new(2))\n  let b = regex.is_match(\"a\", \"a\")\n}\n";
        assert_eq!(imports_for(src), "import base64\nimport regex\n");
        // auto-imported modules and look-alike identifiers need none
        assert_eq!(imports_for("let x = error.chain(\"a\", \"b\")\nlet y = mypath.x"), "");
        // A match inside a string literal still imports: an unused import is
        // only a warning, while a call inside an interpolation
        // (`"${base64.encode(b)}"`) that the scan skipped would be an E003.
        assert_eq!(imports_for("let z = string.len(\"url.\")"), "import url\n");
        let cat = build();
        for m in ["base64", "hex", "hash", "zlib", "path", "url", "regex", "error"] {
            assert!(cat.iter().any(|s| s.module == m), "{m} has a synthesizable function in the catalogue");
        }
    }

    #[test]
    fn only_stock_p1_rows_count() {
        let t = "[[unavailable]]\nfn = \"a.x\"\nlegs = [\"structural\"]\n[[unavailable]]\nfn = \"a.y\"\nlegs = [\"structural\", \"stock-p1\"]\n";
        let u = wasm_unavailable(t);
        assert!(!u.contains("a.x") && u.contains("a.y"));
    }
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
        body,
        params,
        return_type,
        ..
    } = decl
    else {
        return None;
    };

    // Effect functions only from the sandboxed modules: a clock, a socket or
    // a process is not reproducible across legs.
    let effect = effect.unwrap_or(false);
    if effect && !SANDBOXED_EFFECT_MODULES.contains(&module) {
        return None;
    }

    let func = sym_str(*name).to_string();

    // A `__`-prefixed fn is a module's private helper (the stdlib inspection
    // ledger's rule, tests/stdlib_inspection_test.rs): not public surface, so
    // a program calling it is a finding about the generator.
    if func.starts_with("__") {
        return None;
    }

    // Skip non-deterministic functions (the `effect` flag misses these).
    // Sourced from the shared `NamedDenylist` so the synthesis and
    // mutation paths deny the same surface.
    if NamedDenylist::is_denied_function(module, &func) {
        return None;
    }

    // A `mut` parameter needs a `var` at the call: the generator hoists one
    // (term.rs). Two `mut` parameters are left to the mutation path.
    let muts: Vec<usize> = params.iter().enumerate().filter(|(_, p)| p.is_mut).map(|(i, _)| i).collect();
    if muts.len() > 1 {
        return None;
    }
    let mut_param = muts.first().copied();
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
        mut_param,
        effect,
        lifted: effect && !matches!(body.as_ref().map(|b| &b.kind), None | Some(ExprKind::Hole)),
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
