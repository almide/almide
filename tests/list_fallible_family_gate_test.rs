//! The fallibility-polymorphic HOF family as an EXECUTABLE MATRIX (ADR-0006,
//! #3163).
//!
//! History: the `list.try_*` family went through the ADR's three states in
//! order — completeness matrix (v0.53.6) → freeze at seven (v0.54.0, D2) →
//! EMPTY (v0.56.0, D3) — leaving seven `__fallible_*` INTERNAL CARRIERS behind
//! the polymorphic core (`list.map(xs, (x) => f(x)!)!`). The rule ("the
//! callback is `!` ⇒ the HOF is `!`") then held for those seven cells only,
//! while the E005 hint said "the core list HOFs accept fallible callbacks", so
//! `list.all(xs, (x) => p(x)!)` was refused under a hint that said it was not
//! (#3163). A hand-kept list drifted from the prose about it.
//!
//! The matrix is now one table, `almide_lang::fallible_hofs`, and this gate
//! asserts the family's COMPLETENESS RULE against the stdlib sources:
//!
//!   - every public fn of list / map / set / option / result that takes a
//!     callback is classified exactly once — a FALLIBLE cell, or a
//!     TOTAL-ONLY cell with its reason (no unclassified HOF, no stale row);
//!   - every fallible cell's `__fallible_<hof>` carrier is declared in its
//!     module's source, and no carrier exists outside the table;
//!   - every fallible cell TYPE-CHECKS with a propagating callback as
//!     `Result[<the total form's type>, E]` (a probe per cell, run through the
//!     real checker — a new cell without a probe fails here);
//!   - every fallible cell RUNS in the cross-target matrix fixtures, which the
//!     native ⇄ wasm ⇄ interp gates judge;
//!   - the public surface carries NO `try_`-prefixed fn (ADR-0006 D3);
//!   - the diagnostics name the accepted cells from the same table.

use almide::canonicalize;
use almide::check::Checker;
use almide::diagnostic::Level;
use almide::lexer::Lexer;
use almide::parser::Parser;
use almide_lang::fallible_hofs::{FALLIBLE_HOFS, TOTAL_ONLY_HOFS};
use std::collections::BTreeSet;

const MODULES: &[&str] = &["list", "map", "set", "option", "result"];

fn module_src(module: &str) -> String {
    let path = format!("{}/stdlib/{module}.almd", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

/// `(name, takes_a_callback)` for every `fn` declared at the top of a module
/// source, INCLUDING the `__`-prefixed internals.
fn decls(module: &str) -> Vec<(String, bool)> {
    module_src(module)
        .lines()
        .filter_map(|l| {
            let rest = l.strip_prefix("fn ").or_else(|| l.strip_prefix("pub fn "))?;
            let name = rest.split(['(', '[']).next()?.trim().to_string();
            let params = rest.split_once('(')?.1;
            Some((name, has_fn_param(params)))
        })
        .collect()
}

/// A callback parameter is spelled `name: (..) -> ..` — a parenthesised type
/// followed by `->` (`(A?)?` alone is an Option, not a function).
fn has_fn_param(params: &str) -> bool {
    params.match_indices(": (").any(|(at, _)| {
        let mut depth = 0usize;
        for (i, c) in params[at + 2..].char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        return params[at + 2 + i + 1..].trim_start().starts_with("->");
                    }
                }
                _ => {}
            }
        }
        false
    })
}

#[test]
fn every_callback_hof_is_classified_exactly_once() {
    let fallible: BTreeSet<(&str, &str)> = FALLIBLE_HOFS.iter().copied().collect();
    let total_only: BTreeSet<(&str, &str)> = TOTAL_ONLY_HOFS.iter().map(|&(m, h, _)| (m, h)).collect();
    assert_eq!(fallible.len(), FALLIBLE_HOFS.len(), "a FALLIBLE_HOFS row is duplicated");
    for &(m, h, reason) in TOTAL_ONLY_HOFS {
        assert!(!reason.trim().is_empty(), "{m}.{h} is TOTAL-ONLY without a reason");
        assert!(!fallible.contains(&(m, h)), "{m}.{h} is both FALLIBLE and TOTAL-ONLY");
    }
    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
    for &module in MODULES {
        for (name, takes_callback) in decls(module) {
            if name.starts_with("__") || !takes_callback {
                continue;
            }
            assert!(
                fallible.contains(&(module, name.as_str())) || total_only.contains(&(module, name.as_str())),
                "`{module}.{name}` takes a callback but is in neither table of \
                 almide_types::fallible_hofs — give it a `__fallible_{name}` carrier and a \
                 FALLIBLE_HOFS row (ADR-0006: the callback is `!` ⇒ the HOF is `!`), or a \
                 TOTAL_ONLY_HOFS row that says why the fallible form does not apply"
            );
            seen.insert((module.to_string(), name));
        }
    }
    for &(m, h) in fallible.iter().chain(total_only.iter()) {
        if m == "fs" {
            continue;
        }
        assert!(
            seen.contains(&(m.to_string(), h.to_string())) || (m == "list" && h == "each"),
            "the matrix names `{m}.{h}`, which stdlib/{m}.almd does not declare as a callback-taking fn"
        );
    }
}

#[test]
fn every_fallible_cell_has_its_carrier_and_no_carrier_is_orphaned() {
    for &module in MODULES {
        let declared: BTreeSet<String> = decls(module).into_iter().map(|(n, _)| n).collect();
        for &(m, h) in FALLIBLE_HOFS.iter().filter(|(m, _)| *m == module) {
            assert!(
                declared.contains(&format!("__fallible_{h}")),
                "`{m}.__fallible_{h}` is missing from stdlib/{m}.almd — the fallible-callback \
                 normalization routes `{m}.{h}(.., (x) => f(x)!)` to it"
            );
        }
        for name in declared.iter().filter(|n| n.starts_with("__fallible_")) {
            let hof = &name["__fallible_".len()..];
            assert!(
                almide_lang::fallible_hofs::is_fallible_hof(module, hof),
                "stdlib/{module}.almd declares `{name}`, but `{module}.{hof}` is not a FALLIBLE_HOFS \
                 cell — nothing instantiates it"
            );
        }
    }
}

#[test]
fn the_public_try_family_is_empty() {
    for &module in MODULES {
        for (name, _) in decls(module) {
            assert!(
                !name.starts_with("try_"),
                "`{module}.{name}` resurrects the removed try_ family (ADR-0006 D3): a \
                 combinator's fallible form is the polymorphic instantiation \
                 (`{module}.<hof>(.., (x) => f(x)!)!`), never a named twin"
            );
        }
    }
}

// ── the executable half: one probe per cell, through the real checker ──

const HELPERS: &str = r#"
fn p(x: Int) -> Bool! = if x < 0 then err("neg") else ok(x > 1)
fn k(x: Int) -> Int! = if x < 0 then err("neg") else ok(x % 2)
fn a2(a: Int, x: Int) -> Int! = if x < 0 then err("neg") else ok(a + x)
fn kv(k: String, v: Int) -> Bool! = if v < 0 then err("neg " + k) else ok(v > 1)
fn a3(a: Int, k: String, v: Int) -> Int! = if v < 0 then err("neg " + k) else ok(a + v)
fn o(x: Int) -> Int?! = if x < 0 then err("neg") else ok(some(x))
fn t() -> Int! = ok(1)
fn ot() -> Int?! = ok(some(1))
fn seen(x: Int) -> Unit! = if x < 0 then err("neg") else ok(())
"#;

/// `(module, hof, call, the TOTAL form's result type)`: the call's type must
/// check as `Result[<type>, String]`.
const PROBES: &[(&str, &str, &str, &str)] = &[
    ("list", "map", "list.map([1, 2], (x) => k(x)!)", "List[Int]"),
    ("list", "filter", "list.filter([1, 2], (x) => p(x)!)", "List[Int]"),
    ("list", "flat_map", "list.flat_map([1, 2], (x) => [k(x)!])", "List[Int]"),
    ("list", "filter_map", "list.filter_map([1, 2], (x) => o(x)!)", "List[Int]"),
    ("list", "fold", "list.fold([1, 2], 0, (a, x) => a2(a, x)!)", "Int"),
    ("list", "find", "list.find([1, 2], (x) => p(x)!)", "Int?"),
    ("list", "each", "list.each([1, 2], (x) => seen(x)!)", "Unit"),
    ("list", "any", "list.any([1, 2], (x) => p(x)!)", "Bool"),
    ("list", "all", "list.all([1, 2], (x) => p(x)!)", "Bool"),
    ("list", "count", "list.count([1, 2], (x) => p(x)!)", "Int"),
    ("list", "find_index", "list.find_index([1, 2], (x) => p(x)!)", "Int?"),
    ("list", "partition", "list.partition([1, 2], (x) => p(x)!)", "(List[Int], List[Int])"),
    ("list", "take_while", "list.take_while([1, 2], (x) => p(x)!)", "List[Int]"),
    ("list", "drop_while", "list.drop_while([1, 2], (x) => p(x)!)", "List[Int]"),
    ("list", "reduce", "list.reduce([1, 2], (a, x) => a2(a, x)!)", "Int?"),
    ("list", "scan", "list.scan([1, 2], 0, (a, x) => a2(a, x)!)", "List[Int]"),
    ("list", "sort_by", "list.sort_by([1, 2], (x) => k(x)!)", "List[Int]"),
    ("list", "group_by", "list.group_by([1, 2], (x) => k(x)!)", "Map[Int, List[Int]]"),
    ("list", "unique_by", "list.unique_by([1, 2], (x) => k(x)!)", "List[Int]"),
    ("list", "zip_with", "list.zip_with([1, 2], [3, 4], (a, x) => a2(a, x)!)", "List[Int]"),
    ("list", "update", "list.update([1, 2], 0, (x) => k(x)!)", "List[Int]"),
    ("list", "iterate", "list.iterate(1, (x) => k(x)!, 3)", "List[Int]"),
    ("map", "map", r#"map.map(map.from_list([("a", 1)]), (v) => k(v)!)"#, "Map[String, Int]"),
    ("map", "filter", r#"map.filter(map.from_list([("a", 1)]), (q, v) => kv(q, v)!)"#, "Map[String, Int]"),
    ("map", "fold", r#"map.fold(map.from_list([("a", 1)]), 0, (a, q, v) => a3(a, q, v)!)"#, "Int"),
    ("map", "any", r#"map.any(map.from_list([("a", 1)]), (q, v) => kv(q, v)!)"#, "Bool"),
    ("map", "all", r#"map.all(map.from_list([("a", 1)]), (q, v) => kv(q, v)!)"#, "Bool"),
    ("map", "count", r#"map.count(map.from_list([("a", 1)]), (q, v) => kv(q, v)!)"#, "Int"),
    ("map", "find", r#"map.find(map.from_list([("a", 1)]), (q, v) => kv(q, v)!)"#, "(String, Int)?"),
    ("map", "update", r#"map.update(map.from_list([("a", 1)]), "a", (v) => k(v)!)"#, "Map[String, Int]"),
    ("map", "upsert", r#"map.upsert(map.from_list([("a", 1)]), "b", 0, (v) => k(v)!)"#, "Map[String, Int]"),
    ("set", "map", "set.map(set.from_list([1, 2]), (x) => k(x)!)", "Set[Int]"),
    ("set", "filter", "set.filter(set.from_list([1, 2]), (x) => p(x)!)", "Set[Int]"),
    ("set", "fold", "set.fold(set.from_list([1, 2]), 0, (a, x) => a2(a, x)!)", "Int"),
    ("set", "any", "set.any(set.from_list([1, 2]), (x) => p(x)!)", "Bool"),
    ("set", "all", "set.all(set.from_list([1, 2]), (x) => p(x)!)", "Bool"),
    ("option", "map", "option.map(some(1), (x) => k(x)!)", "Int?"),
    ("option", "flat_map", "option.flat_map(some(1), (x) => o(x)!)", "Int?"),
    ("option", "filter", "option.filter(some(1), (x) => p(x)!)", "Int?"),
    ("option", "unwrap_or_else", "option.unwrap_or_else(some(1), () => t()!)", "Int"),
    ("option", "or_else", "option.or_else(some(1), () => ot()!)", "Int?"),
];

fn errors_of(src: &str) -> Vec<String> {
    let tokens = Lexer::tokenize(src);
    let mut prog = Parser::new(tokens).parse().expect("probe parses");
    let canon = canonicalize::canonicalize_program(&prog, std::iter::empty());
    let mut checker = Checker::from_env(canon.env);
    checker.diagnostics = canon.diagnostics;
    checker
        .infer_program(&mut prog)
        .into_iter()
        .filter(|d| d.level == Level::Error)
        .map(|d| format!("{} | hint: {}", d.message, d.hint))
        .collect()
}

#[test]
fn every_fallible_cell_checks_as_the_fallible_form() {
    for &(m, h) in FALLIBLE_HOFS {
        let probe = PROBES.iter().find(|&&(pm, ph, _, _)| pm == m && ph == h);
        let Some(&(_, _, call, ty)) = probe else {
            panic!("no probe for the fallible cell `{m}.{h}` — add one to PROBES")
        };
        let src = format!("{HELPERS}\nfn probe() -> Result[{ty}, String] = {call}\n\nfn main() -> Unit = ()\n");
        let errs = errors_of(&src);
        assert!(errs.is_empty(), "`{m}.{h}` with a fallible callback does not check as Result[{ty}, String]:\n{errs:#?}");
    }
    assert_eq!(PROBES.len(), FALLIBLE_HOFS.len(), "a probe names a cell the matrix does not have");
}

#[test]
fn every_fallible_cell_runs_in_the_cross_target_matrix() {
    let dir = format!("{}/spec/wasm_cross", env!("CARGO_MANIFEST_DIR"));
    let mut text = String::new();
    for f in ["fallible_hof_release_matrix.almd", "fallible_hof_matrix_list.almd", "fallible_hof_matrix_containers.almd"] {
        text += &std::fs::read_to_string(format!("{dir}/{f}")).unwrap_or_else(|e| panic!("read {f}: {e}"));
    }
    for &(m, h) in FALLIBLE_HOFS {
        assert!(
            text.contains(&format!("{m}.{h}(")),
            "`{m}.{h}` with a fallible callback is not exercised by the cross-target matrix \
             fixtures (spec/wasm_cross/fallible_hof_matrix_*.almd) — the native ⇄ wasm ⇄ interp \
             gates never run it"
        );
    }
}

#[test]
fn the_e005_hint_names_the_cells_it_accepts() {
    // A user HOF's plain slot handed a fallible callback: the one place the
    // hint has to say which HOFs DO accept one. It names them from the table.
    let src = "fn p(x: Int) -> Bool! = ok(x > 1)\n\
               fn keep(xs: List[Int], f: (Int) -> Bool) -> List[Int] = list.filter(xs, f)\n\
               fn probe() -> List[Int] = keep([1], (x) => p(x)!)\n\
               fn main() -> Unit = ()\n";
    let errs = errors_of(src);
    let hint = errs.iter().find(|e| e.contains("The callback is FALLIBLE")).unwrap_or_else(|| panic!("no fallible-callback hint in {errs:#?}"));
    for &(m, h) in FALLIBLE_HOFS {
        let group = format!("{m}.{{");
        let at = hint.find(&group).unwrap_or_else(|| panic!("the hint does not name module `{m}`: {hint}"));
        let names = &hint[at + group.len()..];
        let names = &names[..names.find('}').unwrap()];
        assert!(names.split(", ").any(|n| n == h), "the hint does not name `{m}.{h}`: {hint}");
    }
}
