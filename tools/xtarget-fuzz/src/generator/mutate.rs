//! Corpus mutation (the 30% path).
//!
//! We parse the `spec/*.almd` corpus with the real Almide parser, then
//! apply AST-level mutations that are *structurally type-preserving by
//! construction* — they swap a node for another node of the same
//! syntactic kind and literal type, so the mutant overwhelmingly still
//! type-checks. The oracle's `check` rung is the safety net: a mutant
//! that fails `check` is counted as a rejected mutation, not a finding
//! (a generator bug bucket the driver reports on).
//!
//! ## Mutation operators
//!
//! 1. **Literal perturbation** — replace an `Int`/`Float`/`String`/
//!    `Bool` literal with another value of the same type drawn from the
//!    divergence pools. This is the highest-value operator: it pushes
//!    existing well-typed programs onto the boundary inputs (multibyte
//!    strings, float edge cases, int extremes) without changing shape.
//! 2. **Equal-kind subexpression swap** — within one declaration, swap
//!    two leaf expressions of the *same* `ExprKind` discriminant (e.g.
//!    two string literals, two identifiers). Conservative but
//!    structure-preserving.
//! 3. **Statement duplication** — duplicate a `let`/`expr` statement in
//!    a block (re-binding is shadowing-safe in Almide). Stresses
//!    re-evaluation and ordering.
//!
//! The operator is chosen by a named weight table. After mutation the
//! AST is re-printed with the project formatter, so the mutant is always
//! syntactically canonical.

use almide::ast::{visit_exprs_mut, Decl, Expr, ExprKind, Program, Stmt};
use almide::fmt::format_program;

use crate::rng::SplitMix64;

use super::denylist::NamedDenylist;
use super::pools;
use super::{Generated, Origin, Signature};

/// A parsed corpus program available for mutation.
pub struct CorpusEntry {
    pub path: String,
    pub program: Program,
}

/// Corpus subdirectories to load, relative to the repo root.
///
/// We only mutate `main`-bearing programs (see `collect_almd`), so the
/// corpus leans on the directories that hold runnable entry points:
///   - `spec/wasm_cross` — the cross-target equivalence programs, each a
///     `main` designed to run identically on native and WASM. This is
///     the *ideal* mutation seed: already on the differential surface.
///   - `examples` — runnable showcase programs.
///   - the `spec/lang`/`stdlib`/`integration` dirs contribute the few
///     non-test `main` programs they contain.
const CORPUS_SUBDIRS: &[&str] = &[
    "spec/wasm_cross",
    "examples",
    "spec/lang",
    "spec/stdlib",
    "spec/integration",
];

/// Maximum corpus file size (bytes) we bother loading. Huge generated
/// fixtures mutate poorly (mostly noise) and slow startup; the spec
/// programs are all well under this.
const MAX_CORPUS_FILE_BYTES: u64 = 16 * 1024;

/// How many leading lines to scan for the `// wasm:skip` marker (matches
/// the project's own convention of placing it in the first few lines).
const WASM_SKIP_SCAN_LINES: usize = 3;

/// Opt-in env var: when set, `load_corpus` logs each admissible file the
/// effect/nondeterminism screen drops, with the denied surface that
/// tripped it. Diagnostic only — leaves selection behaviour unchanged.
const LOG_EXCLUDED_ENV: &str = "FUZZ_LOG_EXCLUDED";

/// Load and parse every eligible corpus file under `root`.
pub fn load_corpus(root: &std::path::Path) -> Vec<CorpusEntry> {
    // The set of package names an isolated single-file `almide check`
    // (the rung-a gate) can resolve: exactly the bundled stdlib modules
    // (`stdlib/*.almd`). A corpus file importing anything else — an
    // external package like `almai`, or a sibling module from a
    // multi-file project — cannot resolve when copied to a worker's
    // scratch dir alone, so EVERY mutation of it rejects at rung-a (pure
    // generator noise). We screen those out below.
    let resolvable = bundled_module_names(root);

    let mut out = Vec::new();
    for sub in CORPUS_SUBDIRS {
        let dir = root.join(sub);
        collect_almd(&dir, &resolvable, &mut out);
    }
    // Deterministic order: sort by path so a given (seed, index) always
    // maps to the same corpus entry across runs and machines.
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// The names of all bundled stdlib modules (`stdlib/<name>.almd`), the
/// only packages an isolated single-file check can resolve. Returns an
/// empty set if the directory cannot be read (in which case the import
/// screen below is a no-op and the rung-a gate still catches the
/// unresolvable imports as rejects).
fn bundled_module_names(root: &std::path::Path) -> std::collections::HashSet<String> {
    let mut names = std::collections::HashSet::new();
    if let Ok(entries) = std::fs::read_dir(root.join("stdlib")) {
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("almd") {
                if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                    names.insert(stem.to_string());
                }
            }
        }
    }
    names
}

/// Recursively collect parseable `.almd` files into `out`.
fn collect_almd(
    dir: &std::path::Path,
    resolvable: &std::collections::HashSet<String>,
    out: &mut Vec<CorpusEntry>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_almd(&path, resolvable, out);
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("almd") {
            continue;
        }
        if std::fs::metadata(&path).map(|m| m.len()).unwrap_or(u64::MAX) > MAX_CORPUS_FILE_BYTES {
            continue;
        }
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        // Respect the project's own `// wasm:skip` marker: such files are
        // known/intentional cross-target divergences (or use features the
        // WASM target omits), so mutating them would resurface knowns.
        if src
            .lines()
            .take(WASM_SKIP_SCAN_LINES)
            .any(|l| l.contains("wasm:skip"))
        {
            continue;
        }
        let tokens = almide::lexer::Lexer::tokenize(&src);
        let mut parser = almide::parser::Parser::new(tokens);
        let Ok(mut program) = parser.parse() else {
            continue;
        };
        // The oracle runs every program via its `main`/`_start` entry —
        // both `almide run` (native) and the WASM `_start`. A corpus file
        // *without* a `main` (a pure `test`-block file) executes
        // differently on the two targets (native `run` is a no-op; the
        // WASM build runs the tests), which would manufacture spurious
        // "divergences". So we require a `main` AND strip all `test` /
        // `local test where` decls, leaving a program both targets run
        // identically.
        if !has_main(&program) {
            continue;
        }
        // Import-resolvability screen: a file that imports a package the
        // isolated single-file check cannot resolve — an external package
        // (`almai`, `extlib`) or a sibling module of a multi-file project
        // — fails rung-a on *every* mutation, contributing pure generator
        // noise to the reject rate. Drop those so only self-contained,
        // checkable programs reach the mutation pool. (`resolvable` is
        // empty only if `stdlib/` was unreadable, making this a no-op.)
        if !resolvable.is_empty() {
            if let Some(pkg) = unresolvable_import(&program, resolvable) {
                if std::env::var_os(LOG_EXCLUDED_ENV).is_some() {
                    eprintln!("EXCLUDED [import:{pkg}] {}", path.display());
                }
                continue;
            }
        }
        // Effect/nondeterminism screen (last gate before admission): a
        // corpus file referencing the effectful or nondeterministic
        // surface (`fs.*`, `process.*`, `http.*`, `net.*`, a clock, RNG,
        // `fan.race/any/timeout`, …) can run differently on each target —
        // or one target cannot perform the effect at all — so mutating it
        // would manufacture spurious "divergences". The synthesis path
        // denies the same surface; this mirrors that decision via the
        // shared `NamedDenylist`. Placed after the `main`/parse gates so
        // it only ever drops a file that would otherwise be admitted.
        if let Some(surface) = NamedDenylist::source_references_denied_surface(&src) {
            // Set `FUZZ_LOG_EXCLUDED=1` to audit which admissible corpus
            // files the effect screen drops (and which surface tripped
            // each) — used to verify the screen's reach without changing
            // its behaviour.
            if std::env::var_os(LOG_EXCLUDED_ENV).is_some() {
                eprintln!("EXCLUDED [{surface}] {}", path.display());
            }
            continue;
        }
        strip_tests(&mut program);
        out.push(CorpusEntry {
            path: path.to_string_lossy().into_owned(),
            program,
        });
    }
}

/// The first imported package name not in `resolvable`, or `None` if
/// every import resolves. An import's package is its first path segment
/// (`import almai.conv` ⇒ `almai`); a sub-module of a resolvable package
/// (`json.{…}`) still resolves through its head segment.
fn unresolvable_import(
    program: &Program,
    resolvable: &std::collections::HashSet<String>,
) -> Option<String> {
    // The parser collects `import` declarations into `program.imports`
    // (not `program.decls`), mirroring how the resolver reads them.
    for import in &program.imports {
        if let Decl::Import { path, .. } = import {
            if let Some(head) = path.first() {
                let pkg = head.as_str();
                if !resolvable.contains(pkg) {
                    return Some(pkg.to_string());
                }
            }
        }
    }
    None
}

/// Does the program declare a top-level `fn main`?
fn has_main(program: &Program) -> bool {
    program.decls.iter().any(|d| {
        matches!(d, Decl::Fn { name, .. } if name.as_str() == "main")
    })
}

/// Remove `test` and `test where` declarations so both targets execute
/// only `main` (and its supporting decls).
fn strip_tests(program: &mut Program) {
    program
        .decls
        .retain(|d| !matches!(d, Decl::Test { .. } | Decl::TestWhereDef { .. }));
}

/// Relative weights for the mutation operators.
const W_PERTURB_LITERAL: u32 = 6;
const W_SWAP_EQUAL_KIND: u32 = 2;
const W_DUPLICATE_STMT: u32 = 2;

/// Mutate one randomly chosen corpus entry. Returns `None` if the chosen
/// entry has no mutable site for the picked operator (the caller falls
/// back to synthesis).
pub fn mutate_one(
    rng: &mut SplitMix64,
    corpus: &[CorpusEntry],
    _catalogue: &[Signature],
) -> Option<Generated> {
    if corpus.is_empty() {
        return None;
    }
    let entry = &corpus[rng.below(corpus.len() as u32) as usize];
    let mut program = entry.program.clone();

    let op_weights = [W_PERTURB_LITERAL, W_SWAP_EQUAL_KIND, W_DUPLICATE_STMT];
    let mutated = match rng.pick_weighted(&op_weights) {
        0 => perturb_literal(rng, &mut program),
        1 => swap_equal_kind(rng, &mut program),
        _ => duplicate_stmt(rng, &mut program),
    };
    if !mutated {
        return None;
    }

    let source = format_program(&program);
    Some(Generated {
        source,
        origin: Origin::Mutation {
            corpus_file: entry.path.clone(),
        },
    })
}

/// Replace one literal with another of the same type from the pools.
/// Returns `true` if a literal was perturbed.
fn perturb_literal(rng: &mut SplitMix64, program: &mut Program) -> bool {
    // Collect mutable literal sites via a first pass, then mutate a
    // single chosen one in a second pass (so the choice is uniform over
    // all sites without materializing pointers).
    let mut count = 0u32;
    visit_exprs_mut(program, &mut |e| {
        if is_perturbable_literal(&e.kind) {
            count += 1;
        }
    });
    if count == 0 {
        return false;
    }
    let target = rng.below(count);

    let mut seen = 0u32;
    let mut done = false;
    // `visit_exprs_mut` does not let us early-return, so we gate the
    // mutation on the target index and a `done` flag.
    let new_int = *rng.pick(pools::INT_POOL);
    let new_float = *rng.pick(pools::FLOAT_POOL);
    let new_string = (*rng.pick(pools::STRING_POOL)).to_string();
    let new_bool = *rng.pick(pools::BOOL_POOL);

    visit_exprs_mut(program, &mut |e| {
        if done || !is_perturbable_literal(&e.kind) {
            return;
        }
        if seen == target {
            apply_literal(
                &mut e.kind,
                new_int,
                new_float,
                &new_string,
                new_bool,
            );
            done = true;
        }
        seen += 1;
    });
    done
}

/// Whether an expression is a literal we can perturb in place.
fn is_perturbable_literal(kind: &ExprKind) -> bool {
    matches!(
        kind,
        ExprKind::Int { .. }
            | ExprKind::Float { .. }
            | ExprKind::String { .. }
            | ExprKind::Bool { .. }
    )
}

/// Overwrite a literal node with a new same-type value.
fn apply_literal(kind: &mut ExprKind, i: i64, f: f64, s: &str, b: bool) {
    match kind {
        ExprKind::Int { value, raw } => {
            *value = serde_json::Value::from(i);
            *raw = i.to_string();
        }
        ExprKind::Float { value } => *value = f,
        ExprKind::String { value } => *value = s.to_string(),
        ExprKind::Bool { value } => *value = b,
        _ => {}
    }
}

/// Swap two leaf expressions of the same `ExprKind` discriminant within
/// the program. Conservative (type-preserving for same-typed leaves).
fn swap_equal_kind(rng: &mut SplitMix64, program: &mut Program) -> bool {
    // Gather (index, discriminant) of swappable leaves.
    let mut leaves: Vec<(u32, u8)> = Vec::new();
    let mut idx = 0u32;
    visit_exprs_mut(program, &mut |e| {
        if let Some(d) = leaf_discriminant(&e.kind) {
            leaves.push((idx, d));
        }
        idx += 1;
    });
    if leaves.len() < 2 {
        return false;
    }
    // Find two leaves sharing a discriminant.
    let first = leaves[rng.below(leaves.len() as u32) as usize];
    let partners: Vec<u32> = leaves
        .iter()
        .filter(|(i, d)| *d == first.1 && *i != first.0)
        .map(|(i, _)| *i)
        .collect();
    if partners.is_empty() {
        return false;
    }
    let second = partners[rng.below(partners.len() as u32) as usize];

    // Extract both kinds, then swap them in a second pass.
    let mut kinds: Vec<ExprKind> = Vec::new();
    let mut grab_idx = 0u32;
    visit_exprs_mut(program, &mut |e| {
        if grab_idx == first.0 || grab_idx == second {
            kinds.push(e.kind.clone());
        }
        grab_idx += 1;
    });
    if kinds.len() != 2 {
        return false;
    }
    let (a_kind, b_kind) = (kinds[0].clone(), kinds[1].clone());
    let mut set_idx = 0u32;
    visit_exprs_mut(program, &mut |e| {
        if set_idx == first.0 {
            e.kind = b_kind.clone();
        } else if set_idx == second {
            e.kind = a_kind.clone();
        }
        set_idx += 1;
    });
    true
}

/// Leaf-expression discriminant for the swap operator. Returns `None`
/// for non-leaf or unsafe-to-swap kinds.
///
/// Only *literal* leaves are swappable. Identifiers are deliberately
/// excluded: a same-kind ident swap is not type-/scope-preserving —
/// two `Ident` nodes can name a parameter, a local, a for-loop binding,
/// or a module, in different scopes and of different types. Swapping
/// them across those boundaries produces ill-typed or unbound-variable
/// mutants (a parameter `b` swapped into another fn where only `a` is in
/// scope, a `List[String]` var swapped where a `String` is expected, or
/// a module name `list`/`float`/`println` pasted into value position).
/// Those are generator defects, not compiler findings, so we keep the
/// swap operator restricted to literals, where same-discriminant always
/// means same-type and scope is irrelevant.
fn leaf_discriminant(kind: &ExprKind) -> Option<u8> {
    match kind {
        ExprKind::Int { .. } => Some(1),
        ExprKind::Float { .. } => Some(2),
        ExprKind::String { .. } => Some(3),
        ExprKind::Bool { .. } => Some(4),
        _ => None,
    }
}

/// Duplicate one statement inside a fn-body block. In Almide re-binding
/// a `let` shadows, so duplicating a `let` or expression statement keeps
/// the program well-formed.
///
/// Done in two passes to avoid aliasing: first *count* the eligible
/// blocks, then descend a second time and mutate the chosen one by
/// ordinal. This keeps everything in safe `&mut` borrows.
fn duplicate_stmt(rng: &mut SplitMix64, program: &mut Program) -> bool {
    let mut block_count = 0u32;
    for decl in program.decls.iter() {
        if let Decl::Fn { body: Some(expr), .. } = decl {
            count_blocks(expr, &mut block_count);
        }
    }
    if block_count == 0 {
        return false;
    }
    let target_block = rng.below(block_count);
    let dup_choice = rng.next_u64(); // resolved against the block's len below

    let mut ordinal = 0u32;
    let mut done = false;
    for decl in program.decls.iter_mut() {
        if done {
            break;
        }
        if let Decl::Fn { body: Some(expr), .. } = decl {
            mutate_block(expr, target_block, dup_choice, &mut ordinal, &mut done);
        }
    }
    done
}

/// Count block-statement vectors reachable from an expression.
fn count_blocks(expr: &Expr, count: &mut u32) {
    if let ExprKind::Block { expr: tail, .. } = &expr.kind {
        *count += 1;
        if let Some(t) = tail {
            count_blocks(t, count);
        }
    }
}

/// Descend to the `target`-th block and duplicate one eligible statement
/// in it. `ordinal` tracks the running block index; `done` short-
/// circuits once the mutation lands.
fn mutate_block(expr: &mut Expr, target: u32, dup_choice: u64, ordinal: &mut u32, done: &mut bool) {
    if *done {
        return;
    }
    if let ExprKind::Block { stmts, expr: tail } = &mut expr.kind {
        if *ordinal == target {
            if !stmts.is_empty() {
                let dup_at = (dup_choice % stmts.len() as u64) as usize;
                // Restrict to side-effect-light statements: duplicating
                // an assignment could change observable state in a way
                // that diverges legitimately (not our target).
                if matches!(stmts[dup_at], Stmt::Let { .. } | Stmt::Expr { .. }) {
                    let cloned = stmts[dup_at].clone();
                    stmts.insert(dup_at, cloned);
                    *done = true;
                }
            }
            *ordinal += 1;
            return;
        }
        *ordinal += 1;
        if let Some(t) = tail {
            mutate_block(t, target, dup_choice, ordinal, done);
        }
    }
}
