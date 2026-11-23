//! C-460 / C-393: which runtime calls `[permissions] fs` judges, read off the
//! declarations. A call whose declaration carries an `FS` leaf is judged; one
//! no path mark guards is refused; a guarded fn the scope cannot see called
//! is refused; and every `FS` call makes the scope apply. The declarations
//! here are fresh (a module, symbols and parameter names no stdlib file
//! has), so a rule keyed on today's names fails them.

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
    let src = "@intrinsic(\"almide_rt_novelmod_touch\")\n@category(\"FS.write\")\neffect fn touch(n: Int) -> Result[Unit, String] = _\n\n\
               @intrinsic(\"almide_rt_novelmod_peek\")\n@category(\"Net.fetch\", \"FS.read\")\neffect fn peek(at: String) -> Result[Int, String] = _\n\n\
               @intrinsic(\"almide_rt_novelmod_seen\")\n@fs_path(at)\n@category(\"FS.read\")\neffect fn seen(at: String) -> Bool = _\n\n\
               @intrinsic(\"almide_rt_novelmod_tick\")\n@category(\"Time\")\neffect fn tick() -> Int = _\n";
    let sources = [("unix", src)];
    let syms = fs_category_symbols_from(&sources).expect("declarations read");
    assert!(!syms.contains("almide_rt_novelmod_tick"), "a Time call is not a file call: {syms:?}");
    let reached: HashSet<&str> = ["almide_rt_novelmod_touch", "almide_rt_novelmod_peek", "almide_rt_novelmod_seen", "almide_rt_novelmod_tick"].into_iter().collect();
    assert_eq!(
        fs_scope_refusals(&sources, &reached).unwrap(),
        vec!["almide_rt_novelmod_peek".to_string(), "almide_rt_novelmod_touch".to_string()],
        "the unmarked FS calls are refused, the marked one and the Time call are not"
    );
    // A module whose row is FS makes an undeclared intrinsic an FS call.
    let fs_row = [("fs", "@intrinsic(\"almide_rt_fs_novelpeek\")\neffect fn novelpeek(n: Int) -> Int = _\n")];
    let reached: HashSet<&str> = ["almide_rt_fs_novelpeek"].into_iter().collect();
    assert_eq!(fs_scope_refusals(&fs_row, &reached).unwrap(), vec!["almide_rt_fs_novelpeek".to_string()]);
    // The bundled stdlib has none: every FS call carries its mark.
    let all: HashSet<String> = fs_category_symbols_from(&bundled()).unwrap();
    assert!(all.len() > 30, "{all:?}");
    let reached: HashSet<&str> = all.iter().map(String::as_str).collect();
    assert_eq!(fs_scope_refusals(&bundled(), &reached).unwrap(), Vec::<String>::new());
    assert_eq!(fs_runtime_guards_from(&sources).unwrap().len(), 1);
}

#[test]
fn novel_a_guarded_fn_named_as_a_value_refuses_the_scope() {
    let roots = vec!["/novel-root-4825".to_string()];
    for calls in [
        "let f = almide_rt_fs_write;\nf(p, c)",
        "let g = almide_rt_fs_exists as fn(&str) -> bool;",
        "xs.iter().map(almide_rt_unix___os_listen)",
    ] {
        let scope = FsScope::new(&roots, "", "", calls).expect("a scope");
        let r = scope.wrap_calls(calls);
        assert!(r.as_ref().is_err_and(|e| e.contains("named other than as a direct call")), "{calls}: {r:?}");
    }
    let calls = "let r = almide_rt_fs_write(p, c);";
    let scope = FsScope::new(&roots, "", "", calls).unwrap();
    assert!(scope.wrap_calls(calls).is_ok_and(|w| w.contains("__almide_fs_path!(")));
}

#[test]
fn novel_every_fs_category_call_makes_the_scope_apply() {
    let syms = fs_category_symbols_from(&bundled()).unwrap();
    assert!(syms.len() > 30, "{syms:?}");
    for s in &syms {
        assert!(FsScope::applies_to(&format!("let r = {s}(p);")), "`{s}` does not make the scope apply");
    }
    assert!(!FsScope::applies_to("let r = almide_rt_string_len(p);"));
}
