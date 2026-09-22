//! Family gate for #2497: every stdlib fn whose declared return carries a
//! `Bytes` (an RcCow-represented type) inside a `Result` must be bindable
//! through `!` — `let b = f(...)!` — and build natively.
//!
//! `almide check` was green and rustc red for `http.get_bytes` /
//! `http.request_bytes`: the binding is typed `AlmideRcCow<Vec<u8>>` (the
//! Almide `Bytes` shape), the intrinsic returns `Result<Vec<u8>, String>`,
//! and the #617 glue that wraps a raw runtime result decided "is this a
//! native runtime symbol" by the `almide_rt_` prefix alone. The two http
//! intrinsics are spelled `almide_http_*` (the older runtime spelling, shared
//! with json and regex), so the `?` handed the binding a bare `Vec<u8>`
//! (E0308). Every `almide_rt_` sibling in the table below already glued.
//!
//! The table is the closed set of PUBLIC stdlib fns declared
//! `-> Result[..Bytes.., _]` (`grep -n '-> Result\[.*Bytes' stdlib/<module>.almd`,
//! `prim` excluded as the trust-spine internal). A new one belongs in a row.
//! Compile-only: nothing here may reach the network or the filesystem — the
//! program is built, never run.

use std::path::Path;
use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let release = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if release.exists() {
        return release.to_str().unwrap().to_string();
    }
    let debug = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug/almide");
    if debug.exists() {
        return debug.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

/// One row: the fn under test, the modules it needs imported, and the call
/// as written by a user (arguments are placeholders that type-check; the
/// program never runs).
struct Row {
    tag: &'static str,
    imports: &'static [&'static str],
    call: &'static str,
}

const ROWS: &[Row] = &[
    Row { tag: "base64.decode", imports: &["base64"], call: "base64.decode(\"AQI=\")" },
    Row { tag: "base64.decode_url", imports: &["base64"], call: "base64.decode_url(\"AQI\")" },
    Row { tag: "hex.decode", imports: &["hex"], call: "hex.decode(\"0102\")" },
    Row { tag: "fs.read_bytes_raw", imports: &["fs"], call: "fs.read_bytes_raw(\"/nonexistent/2497\")" },
    Row { tag: "http.get_bytes", imports: &["http"], call: "http.get_bytes(\"http://127.0.0.1:9/\")" },
    Row {
        tag: "http.request_bytes",
        imports: &["http"],
        call: "http.request_bytes(\"GET\", \"http://127.0.0.1:9/\", \"\", [:])",
    },
    Row { tag: "net.tcp_read", imports: &["net"], call: "net.tcp_read(0, 1)" },
    Row { tag: "net.tcp_read_exact", imports: &["net"], call: "net.tcp_read_exact(0, 1)" },
    Row { tag: "net.tcp_read_timeout", imports: &["net"], call: "net.tcp_read_timeout(0, 1, 1)" },
    Row { tag: "zlib.compress", imports: &["zlib"], call: "zlib.compress(bytes.from_list([1, 2]))" },
    Row { tag: "zlib.compress_level", imports: &["zlib"], call: "zlib.compress_level(bytes.from_list([1, 2]), 9)" },
    Row { tag: "zlib.decompress", imports: &["zlib"], call: "zlib.decompress(bytes.from_list([1, 2]))" },
    Row { tag: "zlib.deflate", imports: &["zlib"], call: "zlib.deflate(bytes.from_list([1, 2]))" },
    Row { tag: "zlib.deflate_level", imports: &["zlib"], call: "zlib.deflate_level(bytes.from_list([1, 2]), 9)" },
    Row { tag: "zlib.inflate", imports: &["zlib"], call: "zlib.inflate(bytes.from_list([1, 2]))" },
    Row { tag: "zlib.gzip", imports: &["zlib"], call: "zlib.gzip(bytes.from_list([1, 2]))" },
    Row { tag: "zlib.gunzip", imports: &["zlib"], call: "zlib.gunzip(bytes.from_list([1, 2]))" },
];

/// `fs.read_bytes_raw_if_exists` carries the Bytes one level deeper
/// (`Result[Bytes?, String]`) — the glue's Option arm.
const OPTION_ROW: Row = Row {
    tag: "fs.read_bytes_raw_if_exists",
    imports: &["fs"],
    call: "fs.read_bytes_raw_if_exists(\"/nonexistent/2497\")",
};

/// The program for a set of rows: every call bound through `!` in one
/// effect `main`, each binding read once so nothing is unused.
fn program(rows: &[&Row]) -> String {
    let mut imports: Vec<&str> = rows.iter().flat_map(|r| r.imports.iter().copied()).collect();
    imports.sort();
    imports.dedup();
    let mut src = String::new();
    for m in imports {
        src.push_str(&format!("import {m}\n"));
    }
    src.push_str("\neffect fn main() -> Unit = {\n");
    for (i, r) in rows.iter().enumerate() {
        if r.tag == OPTION_ROW.tag {
            src.push_str(&format!("  let b{i} = {}!\n  println(\"${{option.is_some(b{i})}}\")\n", r.call));
        } else {
            src.push_str(&format!("  let b{i} = {}!\n  println(int.to_string(bytes.len(b{i})))\n", r.call));
        }
    }
    src.push_str("}\n");
    src
}

/// Build (never run) `src`; returns the combined output on failure.
fn native_build(tag: &str, src: &str) -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("almide-2497-{}-{}", tag.replace('.', "_"), std::process::id()));
    std::fs::create_dir_all(&dir).expect("mk temp dir");
    let path = dir.join("p.almd");
    let bin = dir.join("p");
    std::fs::write(&path, src).expect("write source");
    let out = Command::new(almide_bin())
        .arg("build")
        .arg(&path)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("spawn almide build");
    let _ = std::fs::remove_dir_all(&dir);
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ))
    }
}

#[test]
fn every_result_bytes_fn_binds_through_bang_and_builds_natively() {
    let all: Vec<&Row> = ROWS.iter().chain(std::iter::once(&OPTION_ROW)).collect();
    // One build for the whole family on the green path; only a failure pays
    // for the per-row builds that name the culprits.
    if native_build("family", &program(&all)).is_ok() {
        return;
    }
    let mut failed = Vec::new();
    for r in &all {
        if let Err(out) = native_build(r.tag, &program(&[r])) {
            failed.push(format!("--- {}\n{out}", r.tag));
        }
    }
    assert!(
        !failed.is_empty(),
        "the combined program failed to build but every row builds alone — a cross-row interaction"
    );
    panic!(
        "`let b = f(...)!` over a Result[Bytes, _] stdlib fn must build natively (#2497); {} row(s) failed:\n{}",
        failed.len(),
        failed.join("\n")
    );
}

/// The exact #2497 repro, with the generated line that carried the E0308
/// pinned by shape: the `?`-unwrapped intrinsic result is wrapped into the
/// binding's `AlmideRcCow` before the binding sees it.
#[test]
fn http_get_bytes_bang_binding_is_glued_into_the_rc_cow_shape() {
    let src = "import http\n\neffect fn main() -> Unit = {\n  let b = http.get_bytes(\"http://127.0.0.1:9/\")!\n  println(int.to_string(bytes.len(b)))\n}\n";
    let dir = std::env::temp_dir().join(format!("almide-2497-emit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mk temp dir");
    let path = dir.join("p.almd");
    std::fs::write(&path, src).expect("write source");
    let out = Command::new(almide_bin())
        .arg(&path)
        .args(["--target", "rust"])
        .output()
        .expect("spawn almide emit");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(out.status.success(), "emit failed:\n{}", String::from_utf8_lossy(&out.stderr));
    let rust = String::from_utf8_lossy(&out.stdout);
    // The runtime's own `pub fn almide_http_get_bytes(` is spliced above the
    // program; the binding is the `let` that CALLS it.
    let bind = rust
        .lines()
        .find(|l| l.trim_start().starts_with("let ") && l.contains("almide_http_get_bytes("))
        .unwrap_or_else(|| panic!("no binding line calls almide_http_get_bytes:\n{rust}"));
    assert!(
        bind.contains("AlmideRcCow::from") || bind.contains("AlmideRcCow::new"),
        "the `?`-unwrapped Vec<u8> must be glued into the AlmideRcCow binding (#2497), got:\n{bind}"
    );
}
