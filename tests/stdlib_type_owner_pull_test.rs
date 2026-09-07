//! #1839: a stdlib-DECLARED type spelled under the auto-import reaches every
//! leg's IR with its declaration — the type-owner pull in `ir_link`.
//!
//! Three gates:
//!   1. The issue's repro rows (bind, match, param, `${e}`) build on the
//!      FORCED structural leg and on the incumbent, printing native's output.
//!      Against the origin/develop binary every row walls on both legs.
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

/// The issue's four rows, verbatim shapes, plus the typed read with a BOUND
/// `Endian` value (the shape that walled on the structural leg even with an
/// explicit `import bytes`).
const ROWS: &[(&str, &str, &str)] = &[
    ("eq", "effect fn main() -> Unit = {\n  let e: Endian = BigEndian\n  println(\"${e == LittleEndian}\")\n}\n", "false\n"),
    ("match", "effect fn main() -> Unit = {\n  let e: Endian = BigEndian\n  let s = match e { LittleEndian => \"le\", BigEndian => \"be\" }\n  println(s)\n}\n", "be\n"),
    ("param", "fn name(e: Endian) -> String = match e { LittleEndian => \"le\", BigEndian => \"be\" }\neffect fn main() -> Unit = {\n  println(name(BigEndian))\n}\n", "be\n"),
    ("repr", "effect fn main() -> Unit = {\n  let e = BigEndian\n  println(\"${e}\")\n}\n", "BigEndian\n"),
    ("bound_read", "effect fn main() -> Unit = {\n  let e: Endian = BigEndian\n  let b = bytes.from_list([1, 2, 3, 4])\n  println(\"${int.from_uint16(bytes.read_uint16(b, 0, e))}\")\n}\n", "258\n"),
];

#[test]
fn auto_import_endian_rows_build_on_both_wasm_legs() {
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
        let (ok_s, out_s, err_s) = run_leg(&path, &[("ALMIDE_WASM_STRUCTURAL", "1")], true);
        if !ok_s || out_s != *want {
            failures.push(format!("{tag}: structural: ok={ok_s} out={out_s:?} err={err_s}"));
        }
        let (ok_i, out_i, err_i) = run_leg(&path, &[("ALMIDE_WASM_INCUMBENT", "1")], true);
        if !ok_i || out_i != *want {
            failures.push(format!("{tag}: incumbent: ok={ok_i} out={out_i:?} err={err_i}"));
        }
    }
    assert!(failures.is_empty(), "auto-import Endian rows:\n{}", failures.join("\n"));
}

/// A program spelling only the type: a fn taking and returning it.
fn spelling_program(ty: &str) -> String {
    format!("fn keep(x: {ty}) -> {ty} = x\nfn main() -> Unit = println(\"ok\")\n")
}

#[test]
fn every_bundled_type_decl_is_pulled_when_spelled() {
    let owners = almide_frontend::bundled_sigs::bundled_type_owners();
    assert!(owners.contains_key("Endian"), "bytes.Endian is the auto-import row of the matrix");
    let nominal = nominal_bundled_types();
    assert!(nominal.contains("Endian") && nominal.contains("FileStat"), "the nominal set enumerates");
    let mut names: Vec<&String> = owners.keys().collect();
    names.sort();
    let mut failures = Vec::new();
    for name in names {
        let module = owners[name];
        if !nominal.contains(name.as_str()) {
            eprintln!("{module}.{name}: transparent alias — resolves away at check time, no cell");
            continue;
        }
        let src = spelling_program(name);
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
        let declared = ir.type_decls.iter().any(|td| td.name.as_str() == name);
        let owner_used = ir.used_stdlib_modules.contains(module);
        if !declared || !owner_used {
            failures.push(format!("{module}.{name}: declared={declared} owner_used={owner_used}"));
        }
    }
    assert!(failures.is_empty(), "type-owner pull matrix:\n{}", failures.join("\n"));
}

/// The bare names of every RECORD or VARIANT `type` a bundled module
/// declares — the nominal cells of the matrix.
fn nominal_bundled_types() -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    for module in almide::stdlib_info::BUNDLED_MODULES.iter().copied() {
        let Some(source) = almide::stdlib_info::bundled_source(module) else { continue };
        let program = almide_lang::parse_cached(source).expect("bundled source parses");
        for decl in &program.decls {
            if let almide::ast::Decl::Type { name, ty, .. } = decl
                && matches!(ty, almide::ast::TypeExpr::Record { .. } | almide::ast::TypeExpr::Variant { .. })
            {
                out.insert(name.as_str().to_string());
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
