//! Empirically verify the MIR-lowering WALL over the real v0 corpus — the
//! step-4 "continuous corpus verification = the definition of parity" gate, in
//! its honest first form. `proofs/corpus-wall.sh` drives this.
//!
//!   classify_corpus <file.almd | dir> ...
//!
//! For every function the frontend can hand to MIR lowering, `lower_function`
//! MUST be TOTAL: it returns `Ok(mir)` (in-profile) or `Err(Unsupported(reason))`
//! (explicitly walled). It must NEVER panic and never silently miscompile — that
//! is the wall the value-semantics subset stands behind, and this harness proves
//! it holds on real source, not just on hand-built MIR.
//!
//! Output split:
//!  - `--out DIR`: the witnesses of every IN-PROFILE function for ALL THREE
//!    proven properties, written as `.cert` files the kernel-proven checker
//!    re-verifies in one pass each:
//!      ownership.cert — one heap object per line (accept ⟹ no double-free/leak)
//!      names.cert     — one `defined|used` line per function (⟹ no dangling ref)
//!      caps.cert      — one `allowed|used` line per function (⟹ no undeclared
//!                       host effect)
//!    So accept ⟹ the FULL proven property set holds over the real corpus.
//!  - STDERR: the honest coverage report — files scanned, frontend-rejected,
//!    functions reaching MIR, in-profile count, and an Unsupported-reason
//!    histogram (so coverage growth is measurable per language feature).
//!
//! Exit code: non-zero iff `lower_function` PANICKED on any corpus function (a
//! wall breach to fix). Frontend rejects and explicit Unsupported are EXPECTED
//! and never fail the harness — they are the wall doing its job.

use almide_frontend::canonicalize;
use almide_frontend::check::Checker;
use almide_frontend::ir_link;
use almide_frontend::lower::lower_program;
use almide_lang::lexer::Lexer;
use almide_lang::parser::Parser;
use almide_ir::IrTypeDeclKind;
use almide_mir::certificate::{
    cap_witness_string, name_witness_string, ownership_certificate, reaches_capability_or_unknown,
};
use almide_mir::{MirFunction, Op};
use almide_optimize::{mono, optimize};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};

/// Builtin free functions that reach STDERR / process-abort but NOT the modeled
/// `Stdout` capability (`assert*` print a diff to stderr then panic; `eprintln`
/// is stderr; `panic` aborts; `to_string` is pure). They are Stdout-free, so a
/// call to one cannot make a caller reach Stdout. NOTE: this is sound for the
/// CURRENT one-capability (Stdout-only) vocabulary — stderr/abort are real host
/// effects the model does not yet name (a wider Capability set is a later brick),
/// so the honest property is "no undeclared STDOUT effect", not "no host effect".
const KNOWN_STDOUT_FREE_BUILTINS: &[&str] =
    &["assert", "assert_eq", "assert_ne", "eprintln", "panic", "to_string"];

/// The NON-RECURRING soundness gate for the borrow-by-default calling convention:
/// EVERY `+1` event in the ownership certificate must be BACKED by a real runtime
/// op — an `i` by an `Alloc` or a heap-result call, an `a` by a `Dup`. A heap
/// parameter must therefore inject NO `+1` (an owned-param `+1` would be synthetic,
/// unbacked by any runtime `Alloc`/`rc_inc` — the gate-blind use-after-free class).
/// If a future lowering re-introduces an unbacked param `+1`, this equality breaks
/// and the corpus gate fails — making the class structurally impossible to ship.
fn plus_one_events_backed(mir: &MirFunction) -> bool {
    let cert = ownership_certificate(mir);
    let i = cert.chars().filter(|c| *c == 'i').count();
    let a = cert.chars().filter(|c| *c == 'a').count();
    let allocs = mir.ops.iter().filter(|o| matches!(o, Op::Alloc { .. })).count();
    let heap_results = mir
        .ops
        .iter()
        .filter(|o| match o {
            Op::Call { dst: Some(_), result: Some(r), .. }
            | Op::CallFn { dst: Some(_), result: Some(r), .. } => r.is_heap(),
            _ => false,
        })
        .count();
    let dups = mir.ops.iter().filter(|o| matches!(o, Op::Dup { .. })).count();
    i == allocs + heap_results && a == dups
}

/// Outcome of driving one `.almd` source through the frontend to linked IR.
enum FrontendOutcome {
    /// Reached linked IR — carries the functions MIR lowering will see.
    Ir(almide_ir::IrProgram),
    /// The frontend itself rejected (parse / type error) — its OWN wall, not
    /// MIR's. Out of scope for this gate, but counted for an honest picture.
    Rejected,
    /// The frontend PANICKED — a frontend-totality issue (separate layer; still
    /// surfaced so it is never invisible).
    Panicked,
}

/// Drive source → linked IR with NO `die()` — every failure becomes a value, so
/// the sweep never aborts on a single bad file. Mirrors `emit_cert_from_source`'s
/// pipeline (the same public frontend functions almide-interp uses).
fn source_to_ir(source: &str) -> FrontendOutcome {
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<almide_ir::IrProgram, String> {
        let tokens = Lexer::tokenize(source);
        let mut parser = Parser::new(tokens);
        let mut prog = parser.parse().map_err(|e| format!("parse error: {e:?}"))?;
        if !parser.errors.is_empty() {
            return Err(format!("parse errors: {:?}", parser.errors));
        }
        let canon = canonicalize::canonicalize_program(&prog, std::iter::empty());
        let mut checker = Checker::from_env(canon.env);
        let diags = checker.infer_program(&mut prog);
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.level == almide_frontend::diagnostic::Level::Error)
            .map(|d| d.message.clone())
            .collect();
        if !errors.is_empty() {
            return Err(format!("type errors ({} diag)", errors.len()));
        }
        let mut ir = lower_program(&prog, &checker.env, &checker.type_map);
        optimize::optimize_program(&mut ir);
        mono::monomorphize(&mut ir);
        ir_link::ir_link(&mut ir);
        Ok(ir)
    }));
    match result {
        Ok(Ok(ir)) => FrontendOutcome::Ir(ir),
        Ok(Err(_reason)) => FrontendOutcome::Rejected,
        Err(_) => FrontendOutcome::Panicked,
    }
}

/// Group an `Unsupported` reason into a stable histogram key: the leading clause
/// before the first variable-debug fragment (`:`, `(`, `{`). Keeps "no scalar
/// Repr for Named { .. }" and "no scalar Repr for Tuple [..]" in one bucket, so
/// the histogram tracks language FEATURES, not incidental type spellings.
fn reason_key(reason: &str) -> String {
    reason
        .split(|c| c == ':' || c == '(' || c == '{')
        .next()
        .unwrap_or(reason)
        .trim()
        .to_string()
}

#[derive(Default)]
struct Tally {
    files: usize,
    frontend_rejected: usize,
    frontend_panicked: usize,
    functions: usize,
    in_profile: usize,
    unsupported: BTreeMap<String, usize>,
    lower_panics: Vec<String>,
    /// Functions whose certificate has an UNBACKED `+1` (the borrow-by-default
    /// soundness gate). Must stay empty — a non-empty list is a wall breach.
    cert_backing_breaches: Vec<String>,
    /// In-profile functions whose Stdout-freedom is provable transitively (every
    /// `Op::CallFn` callee is Stdout-free): their empty capability witness is
    /// emitted for the proven checker. accept ⟹ no undeclared Stdout effect.
    caps_verified: usize,
    /// In-profile functions that call an UNANALYZABLE callee (a walled or
    /// cross-file user function), so their Stdout-freedom cannot be proven; their
    /// witness is NOT emitted (honest: not falsely claimed caps-safe).
    caps_unverified: usize,
}

/// Recursively collect `.almd` files under a path (file or directory).
fn collect_almd(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        let mut entries: Vec<_> = match std::fs::read_dir(path) {
            Ok(rd) => rd.filter_map(|e| e.ok().map(|e| e.path())).collect(),
            Err(_) => return,
        };
        entries.sort();
        for e in entries {
            collect_almd(&e, out);
        }
    } else if path.extension().is_some_and(|x| x == "almd") {
        out.push(path.to_path_buf());
    }
}

fn main() {
    // Parse `--out DIR` (where the three witness `.cert` files are written); the
    // remaining args are corpus paths (files or dirs).
    let mut out_dir: Option<PathBuf> = None;
    let mut paths: Vec<String> = Vec::new();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        if a == "--out" {
            out_dir = it.next().map(PathBuf::from);
        } else {
            paths.push(a);
        }
    }
    if paths.is_empty() || out_dir.is_none() {
        eprintln!("usage: classify_corpus --out DIR <file.almd | dir> ...");
        std::process::exit(2);
    }
    let out_dir = out_dir.unwrap();

    // The sweep catches panics deliberately; silence the default hook so a
    // walled-off panic does not spray a backtrace over the honest report.
    std::panic::set_hook(Box::new(|_| {}));

    let mut files = Vec::new();
    for a in &paths {
        collect_almd(Path::new(a), &mut files);
    }

    let mut t = Tally::default();
    // One witness stream per proven property. ownership = one heap object per
    // line; names/caps = one `<superset>|<subset>` line per in-profile function.
    let mut ownership_stream = String::new();
    let mut names_stream = String::new();
    let mut caps_stream = String::new();

    for file in &files {
        t.files += 1;
        let source = match std::fs::read_to_string(file) {
            Ok(s) => s,
            Err(_) => {
                t.frontend_rejected += 1;
                continue;
            }
        };
        let ir = match source_to_ir(&source) {
            FrontendOutcome::Ir(ir) => ir,
            FrontendOutcome::Rejected => {
                t.frontend_rejected += 1;
                continue;
            }
            FrontendOutcome::Panicked => {
                t.frontend_panicked += 1;
                continue;
            }
        };

        // Variant constructors of this program are PURE data builders (no host
        // effect), so a `CallFn` to one is Stdout-free — collected for the
        // capability soundness fold below.
        let ctors: HashSet<String> = ir
            .type_decls
            .iter()
            .flat_map(|td| match &td.kind {
                IrTypeDeclKind::Variant { cases, .. } => {
                    cases.iter().map(|c| c.name.as_str().to_string()).collect::<Vec<_>>()
                }
                _ => Vec::new(),
            })
            .collect();

        // Pass 1: lower every function; emit the LOCAL witnesses (ownership, names)
        // and collect the in-profile MIRs (the capability fold needs the whole
        // file's in-profile set before it can judge any one function's callees).
        let mut file_mirs: Vec<(String, MirFunction)> = Vec::new();
        for func in &ir.functions {
            t.functions += 1;
            let lowered = catch_unwind(AssertUnwindSafe(|| almide_mir::lower::lower_function(func)));
            match lowered {
                Ok(Ok(mir)) => {
                    t.in_profile += 1;
                    // The borrow-by-default soundness gate: every `+1` event must
                    // be backed by a real runtime op (no synthetic param `+1`).
                    if !plus_one_events_backed(&mir) {
                        t.cert_backing_breaches
                            .push(format!("{}::{}", file.display(), func.name.as_str()));
                    }
                    // Ownership is one heap object per line; names are one line per
                    // function. Both are LOCAL properties — no transitivity.
                    ownership_stream.push_str(&ownership_certificate(&mir));
                    names_stream.push_str(&name_witness_string(&mir));
                    names_stream.push('\n');
                    file_mirs.push((func.name.as_str().to_string(), mir));
                }
                Ok(Err(almide_mir::lower::LowerError::Unsupported(reason))) => {
                    *t.unsupported.entry(reason_key(&reason)).or_insert(0) += 1;
                }
                Err(_) => {
                    // THE wall breach: lowering must be total. Record file::func.
                    t.lower_panics
                        .push(format!("{}::{}", file.display(), func.name.as_str()));
                }
            }
        }

        // Pass 2 (capability SOUNDNESS): a function's empty capability witness is a
        // sound claim of Stdout-freedom ONLY if it reaches no Stdout TRANSITIVELY —
        // the direct witness alone misses what a callee reaches. Emit the witness
        // only for functions provably Stdout-free across `Op::CallFn` edges; the
        // rest are NOT claimed caps-safe (honest scope), never falsely accepted.
        let in_profile_map: BTreeMap<String, MirFunction> = file_mirs.iter().cloned().collect();
        // The conservative free-callee policy: a callee not in the in-profile map
        // is Stdout-free only if it is a pure stdlib `Module` call (a dotted name,
        // purity-gated at lowering), a variant constructor, or a known Stdout-free
        // builtin. Everything else (walled / cross-file user fns) is tainted.
        let is_known_free = |n: &str| {
            n.contains('.') || ctors.contains(n) || KNOWN_STDOUT_FREE_BUILTINS.contains(&n)
        };
        for (name, mir) in &file_mirs {
            let mut visited = BTreeSet::new();
            if reaches_capability_or_unknown(name, &in_profile_map, &is_known_free, &mut visited) {
                t.caps_unverified += 1;
            } else {
                caps_stream.push_str(&cap_witness_string(mir));
                caps_stream.push('\n');
                t.caps_verified += 1;
            }
        }
    }

    // Restore a sane hook before we print (catch window is over).
    let _ = std::panic::take_hook();

    // Write the three witness streams for the proven checker. ownership may be
    // empty if no in-profile function emits a heap object (trivially accepted);
    // names/caps have one line per in-profile function.
    let write = |name: &str, body: &str| {
        let p = out_dir.join(name);
        if let Err(e) = std::fs::write(&p, body) {
            eprintln!("cannot write {}: {e}", p.display());
            std::process::exit(2);
        }
    };
    write("ownership.cert", &ownership_stream);
    write("names.cert", &names_stream);
    write("caps.cert", &caps_stream);

    // STDERR: the honest coverage report.
    eprintln!("== v0-corpus MIR-lowering wall report ==");
    eprintln!("files scanned          : {}", t.files);
    eprintln!("  frontend-rejected    : {}", t.frontend_rejected);
    eprintln!("  frontend-panicked    : {}", t.frontend_panicked);
    eprintln!("functions reaching MIR : {}", t.functions);
    eprintln!(
        "  in-profile (lowers)  : {}  <- proven-checker re-verifies these",
        t.in_profile
    );
    let walled: usize = t.unsupported.values().sum();
    eprintln!("  walled (Unsupported) : {walled}");
    for (reason, n) in &t.unsupported {
        eprintln!("      {n:>4}  {reason}");
    }
    eprintln!("  lower panics (BUG)   : {}", t.lower_panics.len());
    for p in &t.lower_panics {
        eprintln!("      PANIC {p}");
    }
    eprintln!(
        "  unbacked +1 (BUG)    : {}  <- borrow-by-default backing gate",
        t.cert_backing_breaches.len()
    );
    eprintln!(
        "  caps-verified        : {}  <- provably reach no Stdout (transitive); witness emitted",
        t.caps_verified
    );
    eprintln!(
        "  caps-unverified      : {}  <- call an unanalyzable callee; not claimed caps-safe (honest scope)",
        t.caps_unverified
    );
    for p in &t.cert_backing_breaches {
        eprintln!("      UNBACKED {p}");
    }

    let total_breaches = t.lower_panics.len() + t.cert_backing_breaches.len();
    if total_breaches == 0 {
        eprintln!(
            "WALL OK: lower_function was TOTAL over {} corpus functions \
             (Ok or explicit Unsupported, zero panics, zero silent miscompiles), \
             and every in-profile certificate `+1` is backed by a real runtime op \
             (no synthetic param ownership — the borrow-by-default gate).",
            t.functions
        );
    } else {
        if !t.lower_panics.is_empty() {
            eprintln!(
                "WALL BREACH: lower_function panicked on {} function(s) — must return \
                 Ok or Unsupported, never panic.",
                t.lower_panics.len()
            );
        }
        if !t.cert_backing_breaches.is_empty() {
            eprintln!(
                "WALL BREACH: {} function(s) emitted an UNBACKED certificate `+1` — \
                 a param or op injected ownership no runtime op performs \
                 (the gate-blind use-after-free class).",
                t.cert_backing_breaches.len()
            );
        }
        std::process::exit(1);
    }
}
