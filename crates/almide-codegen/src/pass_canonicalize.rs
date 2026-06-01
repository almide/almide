//! CanonicalizePass: establish a host-deterministic canonical order for the
//! emit-affecting collections, so the emitted WASM module is a pure function of
//! the program's *content* rather than of upstream pass/iteration order.
//!
//! # Why (Determinism / Purity Belt, L3)
//!
//! The compiler runs compiled to wasm32-unknown-unknown in the browser
//! playground. A function's WASM index is its position in `program.functions`
//! (and, per module, `module.functions`); if that order ever derives from a
//! `HashMap` iteration or other host-dependent source, the in-browser compiler
//! emits a module that diverges from — or traps relative to — the native one.
//! Individual leaks were patched at the source (mono → `BTreeMap`, the emit-site
//! sorts), but that is whack-a-mole: it cannot prove the *next* reordering pass
//! is caught. This pass + the [`Canonical`](crate::Canonical) type-state make
//! "output order is a function of content" true *by construction* and
//! unbypassable: emit accepts only a canonicalized program, so any future pass
//! that reorders functions in host-dependent order is re-normalized here before
//! a single byte is produced. It is the order-determinism analogue of how
//! `Verified` gates emit on RC balance.
//!
//! # What is (and is NOT) reordered
//!
//! Reordered — safe because functions are resolved by *name* at emit (WASM func
//! indices and the Rust fn list both follow this Vec), so permuting them is
//! semantics-preserving:
//!   * `program.functions`
//!   * each `program.modules[_].functions`
//!
//! Deliberately left alone — their order carries meaning and is already
//! host-deterministic (source order):
//!   * `program.modules` order and every `*.top_lets`: top-level `let` init runs
//!     in sequence and a later binding may observe an earlier one, so the order
//!     is semantic, not cosmetic.
//!   * `type_decls`: variant tag / type-registration order. Reordering risks tag
//!     drift; source order is already deterministic.

use almide_ir::*;
use super::pass::{NanoPass, PassResult, Postcondition, Target};

#[derive(Debug)]
pub struct CanonicalizePass;

/// Total, content-derived sort key for a function's emit position.
///
/// `is_test` first keeps test functions grouped after non-test; the name is the
/// interned identifier's *content* (`Sym`'s order is content-based, but we take
/// the string explicitly so the key is unambiguously total and readable). The
/// sort is *stable*, so on the degenerate case of a true name collision the
/// elements keep their (already deterministic) upstream order.
fn fn_key(f: &IrFunction) -> (bool, String) {
    (f.is_test, f.name.as_str().to_string())
}

/// True iff `funcs` is already in non-descending [`fn_key`] order. Used both as
/// the [`Canonical`](crate::Canonical) certificate's postcondition and as the
/// pass postcondition. Cheap (one linear scan), total, and idempotent — a sorted
/// slice stays sorted, so `canonicalize` is a projection.
fn is_sorted(funcs: &[IrFunction]) -> bool {
    funcs.windows(2).all(|w| fn_key(&w[0]) <= fn_key(&w[1]))
}

/// Establish canonical function order in place. Idempotent.
pub fn canonicalize(program: &mut IrProgram) {
    program.functions.sort_by_key(fn_key);
    for module in &mut program.modules {
        module.functions.sort_by_key(fn_key);
    }
}

/// The canonical-form predicate the [`Canonical`](crate::Canonical) certificate
/// asserts: every emit-ordered function Vec is in [`fn_key`] order.
pub fn is_canonical(program: &IrProgram) -> bool {
    is_sorted(&program.functions)
        && program.modules.iter().all(|m| is_sorted(&m.functions))
}

/// Postcondition probe (free fn so it coerces to the `fn` pointer
/// `Postcondition::Custom` expects).
fn check_canonical(program: &IrProgram) -> Vec<String> {
    if is_canonical(program) {
        vec![]
    } else {
        vec!["CanonicalizePass: program.functions / module.functions are not in \
              canonical (is_test, name) order — emit order would be \
              host-nondeterministic".to_string()]
    }
}

impl NanoPass for CanonicalizePass {
    fn name(&self) -> &str { "Canonicalize" }
    // WASM-only: the `Canonical` gate guards WASM emit (mirroring `Verified`'s
    // scope). The Rust target flattens modules and has its own determinism
    // handling; bringing it under canonical form is future work.
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Wasm]) }
    fn depends_on(&self) -> Vec<&'static str> { vec![] }
    fn postconditions(&self) -> Vec<Postcondition> {
        vec![Postcondition::Custom(check_canonical)]
    }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        canonicalize(&mut program);
        PassResult { program, changed: true }
    }
}
