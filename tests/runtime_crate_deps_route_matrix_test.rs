//! Every build route declares the crates a program's runtime needs (#3346).
//!
//! Three runtime modules are written against crates.io crates: `http` and `sse`
//! (rustls and its root stores) and `zlib` (flate2). The bin route chose an
//! HTTP manifest template and appended `flate2`; the cdylib route wrote only
//! `[native-deps]`, so `almide build --cdylib` of a program using `zlib`
//! failed with E0433 on `flate2` while the same program built as a binary.
//! Both routes now derive the set from one table,
//! `almide_codegen::RUNTIME_MODULE_CRATES`.
//!
//! The matrix: each dependency-bearing module × each native build route —
//! a binary, `--cdylib`, `--repr-c`, `--repr-c --cdylib`, and `almide test`.
//! A cell passes when the route builds (and, for the test route, when the
//! test passes). No program reaches the network: http calls go to a closed
//! local port and fail fast.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

/// A program per dependency-bearing module: `main` for the build routes and a
/// `test` block for the test route, each calling into the module.
fn program(module: &str) -> String {
    let call = match module {
        "zlib" => "zlib.deflate(bytes.from_string(\"hello hello hello\"))",
        "http" => "http.get(\"http://127.0.0.1:1/\")",
        "sse" => "http.openai_streaming_call(\"http://127.0.0.1:1\", \"k\", \"{}\", (t) => println(t))",
        other => panic!("no program for {other}"),
    };
    let import = if module == "zlib" { "zlib" } else { "http" };
    format!(
        "import {import}\n\n\
         effect fn probe() -> String = match {call} {{\n  ok(_) => \"ok\",\n  err(_) => \"err\",\n}}\n\n\
         effect fn main() -> Unit = println(probe()!)\n\n\
         test \"the {module} runtime links\" {{\n  let r = probe()!\n  assert(r == \"ok\" or r == \"err\")\n}}\n"
    )
}

fn scratch(module: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-runtime-crate-matrix-{}-{module}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn run(dir: &Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(almide())
        .args(args)
        .current_dir(dir)
        .env_remove("CARGO_TARGET_DIR")
        .output()
        .expect("spawn almide");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.success(), text)
}

/// Build `module`'s program through every route; report every failing cell.
fn route_matrix(module: &str) {
    let dir = scratch(module);
    std::fs::write(dir.join("p.almd"), program(module)).expect("write program");
    let routes: [(&str, &[&str]); 5] = [
        ("bin", &["build", "p.almd", "-o", "p_bin"]),
        ("cdylib", &["build", "p.almd", "--cdylib", "-o", "p_cdylib"]),
        ("repr-c", &["build", "p.almd", "--repr-c", "-o", "p_reprc"]),
        ("repr-c cdylib", &["build", "p.almd", "--repr-c", "--cdylib", "-o", "p_reprc_cdylib"]),
        ("test", &["test", "p.almd"]),
    ];
    let failures: Vec<String> = routes
        .iter()
        .filter_map(|(route, args)| {
            let (ok, text) = run(&dir, args);
            (!ok).then(|| format!("--- {module} × {route} (`almide {}`):\n{text}", args.join(" ")))
        })
        .collect();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(failures.is_empty(), "{} route(s) failed:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn zlib_builds_on_every_route() {
    route_matrix("zlib");
}

#[test]
fn http_builds_on_every_route() {
    route_matrix("http");
}

#[test]
fn sse_builds_on_every_route() {
    route_matrix("sse");
}

/// The table covers every runtime module whose source names a crate the
/// runtime crate's own manifest declares, and spells each crate as that
/// manifest does — the generated projects compile the same source.
#[test]
fn the_table_matches_the_runtime_manifest() {
    let manifest = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("runtime/rs/Cargo.toml"))
        .expect("runtime/rs/Cargo.toml");
    for (module, crates) in almide_codegen::RUNTIME_MODULE_CRATES {
        for (name, spec) in crates.iter() {
            let line = if spec.starts_with('{') { format!("{name} = {spec}") } else { format!("{name} = \"{spec}\"") };
            if *name == "flate2" {
                continue; // the runtime crate builds zlib only inside generated projects
            }
            assert!(manifest.lines().any(|l| l.trim() == line), "{module}: `{line}` is not runtime/rs/Cargo.toml's spelling");
        }
    }
    let src = |f: &str| std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("runtime/rs/src").join(f)).unwrap();
    assert!(src("zlib.rs").contains("flate2"), "the zlib runtime module no longer names flate2 — update RUNTIME_MODULE_CRATES");
    assert!(src("http.rs").contains("rustls"), "the http runtime module no longer names rustls — update RUNTIME_MODULE_CRATES");
}

/// The dependency set, per module, from the generated code alone: the `--cdylib`
/// and bin routes both call this.
#[test]
fn runtime_crate_deps_per_module() {
    let names = |code: &str| almide_codegen::runtime_crate_deps(code).into_iter().map(|(n, _)| n).collect::<Vec<_>>();
    assert_eq!(names("fn f() { almide_rt_zlib_deflate(x) }"), ["flate2"]);
    assert_eq!(names("fn f() { almide_rt_http_get(x) }"), ["rustls", "webpki-roots", "rustls-native-certs"]);
    assert_eq!(names("fn f() { almide_rt_sse_openai_chat(x); almide_rt_http_get(y) }"), ["rustls", "webpki-roots", "rustls-native-certs"]);
    assert!(names("fn f() { almide_rt_list_len(x) }").is_empty());
}
