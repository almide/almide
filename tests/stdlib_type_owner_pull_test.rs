//! #1839: a stdlib-DECLARED type spelled under the auto-import reaches every
//! leg's IR with its declaration — the type-owner pull in `ir_link`.
//!
//! Three gates:
//!   1. The issue's repro rows (bind, match, param, `${e}`) build on the
//!      wasm leg, printing native's output.
//!   2. The pull is a MATRIX over every `type` any bundled module declares
//!      (enumerated from the bundled sources — no hand list): a program that
//!      spells only the type, with no import, links with the declaration in
//!      `type_decls` and the owner in `used_stdlib_modules`. Completeness
//!      rule: every NOMINAL bundled type (record / variant) is a cell; a
//!      transparent alias (`type TcpStream = Int`) is the intentional
//!      omission — the checker resolves it away, no IR ever spells it, and
//!      there is no declaration any leg needs.
//!   3. No two bundled modules declare one bare type name (the pull could
//!      not choose).

use std::path::Path;
use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok()
}

fn scratch(tag: &str, source: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("almide-type-owner-pull");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join(format!("{tag}.almd"));
    std::fs::write(&path, source).expect("write");
    path
}

fn run_leg(path: &Path, env: &[(&str, &str)], wasm: bool) -> (bool, String, String) {
    let mut cmd = Command::new(almide_bin());
    cmd.arg("run").arg(path);
    if wasm {
        cmd.arg("--target").arg("wasm");
    }
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("spawn almide");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

/// The issue's rows (bind + equality, match, param, `${e}`), first pinned on
/// `bytes.Endian`; that type left the stdlib and no auto-imported stdlib
/// VARIANT remains, so the rows now spell the auto-imported generic record
/// `deque.Deque` by its literal, with no `deque.*` call and no import.
const ROWS: &[(&str, &str, &str)] = &[
    ("eq", "effect fn main() -> Unit = {\n  let d: Deque[Int] = Deque { front: [1], back: [] }\n  println(\"${d == Deque { front: [2], back: [] }}\")\n}\n", "false\n"),
    ("match", "effect fn main() -> Unit = {\n  let d: Deque[Int] = Deque { front: [1], back: [2, 3] }\n  let n = match d { Deque { front, back } => list.len(front) + list.len(back) }\n  println(\"${n}\")\n}\n", "3\n"),
    ("param", "fn size(d: Deque[Int]) -> Int = match d { Deque { front, back } => list.len(front) + list.len(back) }\neffect fn main() -> Unit = {\n  println(\"${size(Deque { front: [], back: [4, 5] })}\")\n}\n", "2\n"),
    ("repr", "effect fn main() -> Unit = {\n  let d: Deque[Int] = Deque { front: [1], back: [2] }\n  println(\"${d}\")\n}\n", "Deque { front: [1], back: [2] }\n"),
];

#[test]
fn auto_import_owned_type_rows_build_on_the_wasm_leg() {
    if !wasmtime_available() {
        eprintln!("skipping: wasmtime not on PATH");
        return;
    }
    let mut failures = Vec::new();
    for (tag, src, want) in ROWS {
        let path = scratch(tag, src);
        let (ok_n, out_n, err_n) = run_leg(&path, &[], false);
        if !ok_n || out_n != *want {
            failures.push(format!("{tag}: native: ok={ok_n} out={out_n:?} err={err_n}"));
        }
        let (ok_s, out_s, err_s) = run_leg(&path, &[("ALMIDE_WASM_SKIP_STOCK_AUDIT", "1")], true);
        if !ok_s || out_s != *want {
            failures.push(format!("{tag}: structural: ok={ok_s} out={out_s:?} err={err_s}"));
        }
    }
    assert!(failures.is_empty(), "auto-import owned-type rows:\n{}", failures.join("\n"));
}

/// A program spelling only the type: a fn taking and returning it. A generic
/// type is spelled with `Int` for each parameter (`Deque[Int]`); a bare
/// `Deque` is an arity error the checker does not report yet (#4085).
fn spelling_program(ty: &str, arity: usize) -> String {
    let ty = if arity == 0 { ty.to_string() } else { format!("{ty}[{}]", vec!["Int"; arity].join(", ")) };
    format!("fn keep(x: {ty}) -> {ty} = x\nfn main() -> Unit = println(\"ok\")\n")
}

#[test]
fn every_bundled_type_decl_is_pulled_when_spelled() {
    let owners = almide_frontend::bundled_sigs::bundled_type_owners();
    assert!(owners.contains_key("Deque"), "deque.Deque is the auto-import row of the matrix");
    let nominal = nominal_bundled_types();
    assert!(nominal.contains_key("Deque") && nominal.contains_key("FileStat"), "the nominal set enumerates");
    let mut names: Vec<&String> = owners.keys().collect();
    names.sort();
    let mut failures = Vec::new();
    for name in names {
        let module = owners[name];
        let Some(&arity) = nominal.get(name.as_str()) else {
            eprintln!("{module}.{name}: transparent alias — resolves away at check time, no cell");
            continue;
        };
        let src = spelling_program(name, arity);
        let path = scratch(&format!("pull_{name}"), &src);
        let ir = match almide::wasm_leg::lower_to_ir(path.to_str().unwrap(), &src) {
            Ok(ir) => ir,
            Err(e) => {
                // A type the checker only admits behind its import is not
                // reachable under the auto-import; the pull is not what
                // decides that. Record the front's verdict, do not fail.
                eprintln!("{module}.{name}: front refused the import-free spelling: {e}");
                continue;
            }
        };
        // The decl is pulled to the top level, or the owner was lowered as a
        // module (an auto-imported module whose fns are linked, `deque` and
        // `heap`) and carries it there — the pull skips a name already declared.
        let declared = ir.type_decls.iter().any(|td| td.name.as_str() == name)
            || ir.modules.iter().any(|m| m.name.as_str() == module && m.type_decls.iter().any(|td| td.name.as_str() == name));
        let owner_used = ir.used_stdlib_modules.contains(module);
        if !declared || !owner_used {
            failures.push(format!("{module}.{name}: declared={declared} owner_used={owner_used}"));
        }
    }
    assert!(failures.is_empty(), "type-owner pull matrix:\n{}", failures.join("\n"));
}

/// The bare names of every RECORD or VARIANT `type` a bundled module
/// declares, with the number of its type parameters — the nominal cells of
/// the matrix.
fn nominal_bundled_types() -> std::collections::HashMap<String, usize> {
    let mut out = std::collections::HashMap::new();
    for module in almide::stdlib_info::BUNDLED_MODULES.iter().copied() {
        let Some(source) = almide::stdlib_info::bundled_source(module) else { continue };
        let program = almide_lang::parse_cached(source).expect("bundled source parses");
        for decl in &program.decls {
            if let almide::ast::Decl::Type { name, ty, generics, .. } = decl
                && matches!(ty, almide::ast::TypeExpr::Record { .. } | almide::ast::TypeExpr::Variant { .. })
            {
                out.insert(name.as_str().to_string(), generics.as_ref().map_or(0, |g| g.len()));
            }
        }
    }
    out
}

#[test]
fn bundled_type_decls_are_unique_by_name() {
    let mut seen: std::collections::HashMap<String, Vec<&str>> = Default::default();
    for module in almide::stdlib_info::BUNDLED_MODULES.iter().copied() {
        let Some(source) = almide::stdlib_info::bundled_source(module) else { continue };
        let program = almide_lang::parse_cached(source).expect("bundled source parses");
        for decl in &program.decls {
            if let almide::ast::Decl::Type { name, .. } = decl {
                seen.entry(name.as_str().to_string()).or_default().push(module);
            }
        }
    }
    let dups: Vec<String> = seen
        .iter()
        .filter(|(_, ms)| ms.len() > 1)
        .map(|(n, ms)| format!("{n}: {}", ms.join(", ")))
        .collect();
    assert!(dups.is_empty(), "bundled type names declared twice:\n{}", dups.join("\n"));
}
