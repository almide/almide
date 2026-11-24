//! #4918: on `--target wasi` (the rustc-wasip1 leg) a stdlib fn whose
//! runtime reaches a call wasm32-wasip1's std aborts on is refused whether or
//! not proofs/target-availability.toml declares it — derived from the runtime
//! source (`almide::codegen::runtime_reach`), not from a list of fn names.
//!
//! Novel: a fresh runtime symbol reaching `std::process::id` through a fresh
//! helper, declared by a fresh module's fresh fn, is found; the control twin
//! (the same shapes with the aborting call replaced by an `Unsupported`-err
//! call) is not. The check-route test: a program reaching the derived fn is
//! refused by `check --target wasi` with the derived reason.

#[path = "support/novel.rs"]
mod novel;

use almide::codegen::runtime_reach::{intrinsic_fns_reaching, reaching_fns, wasip1_aborting_fns, WASIP1_ABORTING_CALLS};

fn shapes(aborting: &str) -> (String, String, String, String) {
    let sym = novel::runtime_symbol();
    let helper = novel::ident("helper");
    let module = novel::module();
    let func = novel::ident("f");
    let rt = format!("pub fn {sym}() -> i64 {{ {helper}() }}\nfn {helper}() -> i64 {{ {aborting} }}\n");
    let src = format!("@intrinsic(\"{sym}\")\neffect fn {func}() -> Int = _\n");
    (rt, src, module, format!("{func}"))
}

#[test]
fn novel_a_fresh_intrinsic_reaching_an_aborting_call_is_derived_unavailable() {
    for call in WASIP1_ABORTING_CALLS {
        let (rt, src, module, func) = shapes(&format!("{call}) as i64"));
        let reached = reaching_fns(&[("rtmod", rt.as_str())], WASIP1_ABORTING_CALLS);
        let fns = intrinsic_fns_reaching(&[(module.as_str(), src.as_str())], &reached);
        assert!(fns.contains_key(&format!("{module}.{func}")), "{call}: a fresh fn reaching it was not derived: {fns:?}");
    }
    // The control: an err-returning platform call is served, not derived.
    let (rt, src, module, _) = shapes("std::net::TcpStream::connect(\"a:1\").map_or(0, |_| 1)");
    let reached = reaching_fns(&[("rtmod", rt.as_str())], WASIP1_ABORTING_CALLS);
    assert!(intrinsic_fns_reaching(&[(module.as_str(), src.as_str())], &reached).is_empty());
}

/// The public call that reaches each derived fn: a private leaf is never
/// called by a program, so the route is exercised through the stdlib fn that
/// wraps it. A derived fn with no row here fails the test — the table cannot
/// fall behind the derivation.
const PUBLIC_CALLS: &[(&str, &str, &str)] = &[
    ("process.pid", "process", "let _ = process.pid()"),
    ("unix.__os_shm_create", "unix", "let _ = unix.shm_create(4096)!"),
];

/// Every derived fn is refused by `check --target wasi` through its public
/// call, on the real binary.
#[test]
fn every_derived_fn_is_refused_by_check_on_the_wasi_route() {
    let fns = wasip1_aborting_fns();
    assert!(!fns.is_empty(), "the derivation found nothing: the runtime scan went blind");
    for (key, symbol) in fns {
        let Some((_, module, call)) = PUBLIC_CALLS.iter().find(|(k, _, _)| k == key) else {
            panic!("derived `{key}` ({symbol}) has no public call in PUBLIC_CALLS: add the stdlib fn that reaches it");
        };
        let t = check_wasi(&format!("import {module}\n\neffect fn main() -> Unit = {{\n  {call}\n  println(\"x\")\n}}\n"));
        assert!(t.0 == false && t.1.contains("error[E081]") && t.1.contains("--target wasi"), "{key} ({symbol}) not refused through `{call}`:\n{}", t.1);
    }
}

/// Calibration (#4918): the subprocess family starts its child before any pipe
/// thread, and wasm32-wasip1 refuses that spawn with an err the program sees,
/// so exec / output are served on this route, not refused.
#[test]
fn exec_and_output_are_not_refused_on_the_wasi_route() {
    let fns = wasip1_aborting_fns();
    for f in ["process.exec", "process.output", "process.__os_output", "process.run", "process.__os_run"] {
        assert!(!fns.contains_key(f), "{f} derived as aborting: {fns:?}");
    }
    let (ok, text) = check_wasi("import process\n\neffect fn main() -> Unit = {\n  let r = process.exec(\"true\", [])!\n  let o = process.output(process.ProcessCommand { cmd: \"true\", args: [] })!\n  println(\"${r}${o.code}\")\n}\n");
    assert!(ok, "exec / output refused on the wasi route:\n{text}");
}

/// `almide check p.almd --target wasi` on `src`: (success, stdout + stderr).
fn check_wasi(src: &str) -> (bool, String) {
    let d = std::env::temp_dir().join(format!("almide-4918-{}-{}", novel::ident("d"), std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("p.almd"), src).unwrap();
    let o = std::process::Command::new(env!("CARGO_BIN_EXE_almide")).args(["check", "p.almd", "--target", "wasi"]).current_dir(&d).output().unwrap();
    let _ = std::fs::remove_dir_all(&d);
    (o.status.success(), format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)))
}
