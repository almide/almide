//! C-460 / C-393: which runtime calls `[permissions] fs` judges, read off the
//! declarations. A call whose declaration carries an `FS` leaf is judged; one
//! no path mark guards is refused; a guarded fn the scope cannot see called
//! is refused; and every `FS` call makes the scope apply. The declarations
//! here are fresh (a module, symbols and parameter names no stdlib file
//! has), so a rule keyed on today's names fails them.

#[path = "../../../tests/support/novel.rs"]
mod novel;

use almide_codegen::fs_scope::{fs_category_symbols_from, fs_runtime_guards_from, fs_scope_refusals, FsScope};
use std::collections::HashSet;

fn bundled() -> Vec<(&'static str, &'static str)> {
    almide_lang::stdlib_info::BUNDLED_MODULES
        .iter()
        .filter_map(|&m| almide_lang::stdlib_info::bundled_source(m).map(|s| (m, s)))
        .collect()
}

#[test]
fn novel_an_fs_category_call_with_no_path_mark_is_refused() {
    let [touch, peek, seen, tick] = std::array::from_fn(|_| novel::runtime_symbol());
    let [ft, fp, fs, fk] = std::array::from_fn(|_| novel::ident("f"));
    let (n, at) = (novel::ident("n"), novel::ident("at"));
    let src = format!(
        "@intrinsic(\"{touch}\")\n@category(\"FS.write\")\neffect fn {ft}({n}: Int) -> Result[Unit, String] = _\n\n\
         @intrinsic(\"{peek}\")\n@category(\"Net.fetch\", \"FS.read\")\neffect fn {fp}({at}: String) -> Result[Int, String] = _\n\n\
         @intrinsic(\"{seen}\")\n@fs_path({at})\n@category(\"FS.read\")\neffect fn {fs}({at}: String) -> Bool = _\n\n\
         @intrinsic(\"{tick}\")\n@category(\"Time\")\neffect fn {fk}() -> Int = _\n"
    );
    // A module the stdlib has (its row is read for an undeclared fn); the
    // declarations, symbols and parameter names in it are fresh.
    let sources = [("unix", src.as_str())];
    let syms = fs_category_symbols_from(&sources).expect("declarations read");
    assert!(!syms.contains(&tick), "a Time call is not a file call: {syms:?}");
    let reached: HashSet<&str> = [touch.as_str(), peek.as_str(), seen.as_str(), tick.as_str()].into_iter().collect();
    let mut refused = vec![peek.clone(), touch.clone()];
    refused.sort();
    assert_eq!(
        fs_scope_refusals(&sources, &reached).expect("the fs scope reads"),
        refused,
        "the unmarked FS calls are refused, the marked one and the Time call are not"
    );
    // A module whose row is FS makes an undeclared intrinsic an FS call.
    let (sym, f) = (format!("almide_rt_fs_{}", novel::ident("peek")), novel::ident("f"));
    let fs_src = format!("@intrinsic(\"{sym}\")\neffect fn {f}(n: Int) -> Int = _\n");
    let fs_row = [("fs", fs_src.as_str())];
    let reached: HashSet<&str> = [sym.as_str()].into_iter().collect();
    assert_eq!(fs_scope_refusals(&fs_row, &reached).expect("the fs scope reads"), vec![sym.clone()]);
    // The bundled stdlib has none: every FS call carries its mark.
    let all: HashSet<String> = fs_category_symbols_from(&bundled()).expect("the fs category reads");
    assert!(all.len() > 30, "{all:?}");
    let reached: HashSet<&str> = all.iter().map(String::as_str).collect();
    assert_eq!(fs_scope_refusals(&bundled(), &reached).expect("the fs scope reads"), Vec::<String>::new());
    assert_eq!(fs_runtime_guards_from(&sources).expect("the runtime guards read").len(), 1);
}

#[test]
fn novel_a_guarded_fn_named_as_a_value_refuses_the_scope() {
    let roots = vec![format!("/{}", novel::path_segment())];
    let (f, g, xs) = (novel::ident("f"), novel::ident("g"), novel::ident("xs"));
    for calls in [
        format!("let {f} = almide_rt_fs_write;\n{f}(p, c)"),
        format!("let {g} = almide_rt_fs_exists as fn(&str) -> bool;"),
        format!("{xs}.iter().map(almide_rt_unix___os_listen)"),
    ] {
        let calls = calls.as_str();
        let scope = FsScope::new(&roots, "", "", calls).expect("a scope");
        let r = scope.wrap_calls(calls);
        assert!(r.as_ref().is_err_and(|e| e.contains("named other than as a direct call")), "{calls}: {r:?}");
    }
    let calls = "let r = almide_rt_fs_write(p, c);";
    let scope = FsScope::new(&roots, "", "", calls).expect("the scope builds");
    assert!(scope.wrap_calls(calls).is_ok_and(|w| w.contains("__almide_fs_path!(")));
}

#[test]
fn novel_every_fs_category_call_makes_the_scope_apply() {
    let syms = fs_category_symbols_from(&bundled()).expect("the fs category reads");
    assert!(syms.len() > 30, "{syms:?}");
    for s in &syms {
        assert!(FsScope::applies_to(&format!("let r = {s}(p);")), "`{s}` does not make the scope apply");
    }
    assert!(!FsScope::applies_to(&format!("let r = {}(p);", novel::runtime_symbol())));
}
