//! The bundled functions the compiler calls on its own are the ones
//! `bundled_reach::COMPILER_INSERTED` and `TEST_SYNTHESIZED` list (#4328).
//!
//! A bundled module keeps only the functions a program reaches, and reach is
//! read off the spelled program. A call the lowering builds itself
//! (`testing.__assert_aborts_check`, `value.__keys`) is spelled nowhere, so
//! it stays live only through the tables. This scans the lowering, the
//! driver's test-runner synthesis and the wasm emitter for every literal
//! `module.__name` call they construct into a bundled module, and refuses one
//! the tables do not list: a new synthesized call would otherwise surface as
//! a missing function in a program that happens not to name it.

use std::collections::BTreeSet;

fn lowering_sources() -> Vec<(String, String)> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("crates/almide-frontend/src/lower");
    let runner = root.join("crates/almide-driver/src/test_runner.rs");
    let mut out = vec![(runner.display().to_string(), std::fs::read_to_string(&runner).expect("test_runner.rs"))];
    for dir in [dir, root.join("crates/almide-wasm/src")] {
        for entry in std::fs::read_dir(&dir).expect("source dir") {
            let path = entry.expect("entry").path();
            if path.extension().is_some_and(|e| e == "rs") {
                out.push((path.display().to_string(), std::fs::read_to_string(&path).expect("read")));
            }
        }
    }
    out
}

/// Every (module, `__fn`) a lowering source builds a call to, in the three
/// spellings the lowering uses.
fn synthesized_calls(text: &str) -> BTreeSet<(String, String)> {
    let mut out = BTreeSet::new();
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let quoted = |s: &str| -> Option<String> {
        let s = s.strip_prefix('"')?;
        Some(s[..s.find('"')?].to_string())
    };
    // `testing_call(ctx, "__f", ...)`
    for (i, _) in flat.match_indices("testing_call(ctx, ") {
        if let Some(f) = quoted(&flat[i + "testing_call(ctx, ".len()..]) {
            out.insert(("testing".to_string(), f));
        }
    }
    // `module_call(sym("m"), "__f", ...)` (the driver's test runner)
    for (i, _) in flat.match_indices("module_call(sym(") {
        let rest = &flat[i + "module_call(sym(".len()..];
        if let Some(m) = quoted(rest) {
            let after = &rest[m.len() + 2..];
            if let Some(f) = after.strip_prefix("), ").and_then(quoted) {
                out.insert((m, f));
            }
        }
    }
    // `call_mod_("m", "__f", ...)`
    for (i, _) in flat.match_indices("call_mod_(") {
        let rest = &flat[i + "call_mod_(".len()..];
        if let Some(m) = quoted(rest) {
            let after = &rest[m.len() + 2..];
            if let Some(f) = after.strip_prefix(", ").and_then(quoted) {
                out.insert((m, f));
            }
        }
    }
    // `CallTarget::Module { module: sym("m"), func: sym("__f"), ... }`, the
    // `sym` spelled bare or path-qualified (`almide_base::intern::sym`).
    let flat = flat.replace("almide_base::intern::sym(", "sym(").replace("almide_lang::intern::sym(", "sym(");
    for (i, _) in flat.match_indices("module: sym(") {
        let rest = &flat[i + "module: sym(".len()..];
        if let Some(m) = quoted(rest) {
            let after = &rest[m.len() + 2..];
            if let Some(f) = after.strip_prefix("), func: sym(").and_then(quoted) {
                out.insert((m, f));
            }
        }
    }
    // Only an Almide-bodied function of a bundled module can be pruned.
    out.retain(|(m, f)| f.starts_with("__") && almide_lang::stdlib_info::bundled_source(m).is_some());
    out
}

#[test]
fn every_synthesized_internal_call_is_in_the_compiler_inserted_table() {
    use almide_frontend::bundled_reach::{COMPILER_INSERTED, TEST_SYNTHESIZED};
    let table: BTreeSet<(String, String)> = COMPILER_INSERTED
        .iter()
        .chain(TEST_SYNTHESIZED)
        .map(|(m, f)| (m.to_string(), f.to_string()))
        .collect();
    let mut found = BTreeSet::new();
    let mut missing = Vec::new();
    for (path, text) in lowering_sources() {
        for call in synthesized_calls(&text) {
            if !table.contains(&call) {
                missing.push(format!("{}.{} (built in {path})", call.0, call.1));
            }
            found.insert(call);
        }
    }
    assert!(
        missing.is_empty(),
        "the compiler builds calls to bundled functions that bundled_reach::COMPILER_INSERTED / TEST_SYNTHESIZED do not list, \
         so a program that does not spell them loses their bodies:\n  {}",
        missing.join("\n  ")
    );
    // The scan must see the calls the table was written from; a scan that
    // finds nothing passes vacuously.
    for known in [("testing", "__assert_aborts_check"), ("testing", "__abort_site_selected"), ("value", "__keys"), ("string", "__err_text")] {
        assert!(
            found.contains(&(known.0.to_string(), known.1.to_string())),
            "the scan no longer finds {}.{} — fix the scan before trusting it",
            known.0,
            known.1
        );
    }
}

#[test]
fn every_table_row_is_a_function_of_its_bundled_module() {
    use almide_frontend::bundled_reach::{COMPILER_INSERTED, TEST_SYNTHESIZED};
    for (m, f) in COMPILER_INSERTED.iter().chain(TEST_SYNTHESIZED) {
        let src = almide_lang::stdlib_info::bundled_source(m).unwrap_or_else(|| panic!("{m} is not a bundled module"));
        let prog = almide_lang::parse_cached(src).expect("bundled source parses");
        let declared = prog.decls.iter().any(|d| matches!(d, almide_lang::ast::Decl::Fn { name, .. } if name.as_str() == *f));
        assert!(declared, "COMPILER_INSERTED lists {m}.{f}, which {m} does not declare");
    }
}

/// The wasm emitter resolves some surfaces itself, by a literal key no IR call
/// names (`resolve_qualified("float.to_string")` inside the JSON serializer);
/// the wasm link loads their self-host bodies only through
/// `almide_wasm::EMITTER_IMPLIED_SURFACES` (#4458). A literal key no row
/// resolves would be missing from every program that does not happen to
/// demand it some other way.
#[test]
fn every_literal_surface_the_wasm_emitter_resolves_is_implied() {
    let resolved: BTreeSet<&str> = almide_wasm::EMITTER_IMPLIED_SURFACES.iter().map(|(_, r)| *r).collect();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/almide-wasm/src");
    let mut keys = BTreeSet::new();
    for entry in std::fs::read_dir(&dir).expect("almide-wasm/src") {
        let path = entry.expect("entry").path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read");
        for (i, _) in text.match_indices("resolve_qualified(\"") {
            let rest = &text[i + "resolve_qualified(\"".len()..];
            keys.insert((path.file_name().unwrap().to_string_lossy().to_string(), rest[..rest.find('"').unwrap()].to_string()));
        }
    }
    assert!(keys.len() >= 3, "the scan found {} literal keys: the emitter's spelling moved", keys.len());
    let missing: Vec<String> =
        keys.iter().filter(|(_, k)| !resolved.contains(k.as_str())).map(|(f, k)| format!("{f}: {k}")).collect();
    assert!(missing.is_empty(), "literal surfaces the wasm emitter resolves with no EMITTER_IMPLIED_SURFACES row: {missing:?}");
}
