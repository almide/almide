//! `almide verify` is a subprocess shim over the independent `almide-verify`
//! binary, with NO linked fallback (#2152).
//!
//! Each case runs a copy of the almide under test from a scratch directory
//! whose contents — and an empty PATH — decide whether a verifier exists:
//!
//! - absent: a named `error[verifier-missing]` and exit 127, in both the
//!   produce-then-verify form and the forwarding form; `--emit` still writes
//!   the bundle, which the verifier's own parser reads and judges;
//! - present: arguments, standard streams and the exit status pass through,
//!   and a real program's bundle is judged by the real verifier — a
//!   capability past the manifest comes back as a REJECT and exit 1;
//! - the bundle is the STRUCTURAL leg's (#4011): the build `almide build
//!   --target wasm` ships, with its bounds witness, and pinned by SHA-256 to
//!   the built file when one is present — a stale file is refused.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn almide() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_almide"))
}

fn exe(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

fn fixture(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("proofs/fixtures").join(name).display().to_string()
}

#[path = "support/verifier.rs"]
#[allow(dead_code)] // `path_with_verifier` is the route matrix's
mod support_verifier;
use support_verifier::verifier;

/// One stage at a time. macOS validates a freshly created executable on its
/// first exec (AMFI + the syspolicy provenance scan), and concurrent first
/// execs of new binary paths were observed to SIGKILL one of them ("no CMS
/// blob … Unrecoverable CT signature issue" in the kernel log) — a host
/// property, not the shim's. Staging serially avoids that race.
static STAGING: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A scratch directory next to the almide under test holding a COPY of
/// `almide` (a copy, not a hard link: a hard link shares the vnode whose
/// signature the kernel validates) and, when `with_verifier`, of
/// `almide-verify`.
struct Stage {
    dir: PathBuf,
    empty_path: PathBuf,
    _serial: std::sync::MutexGuard<'static, ()>,
}

impl Stage {
    fn new(tag: &str, with_verifier: bool) -> Stage {
        let serial = STAGING.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = almide().with_file_name(format!("verify-shim-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let empty_path = dir.join("empty-path");
        std::fs::create_dir_all(&empty_path).expect("scratch dir");
        let place = |from: &Path, name: &str| {
            std::fs::copy(from, dir.join(exe(name))).expect("copy binary");
        };
        place(&almide(), "almide");
        if with_verifier {
            place(&verifier(), "almide-verify");
        }
        Stage { dir, empty_path, _serial: serial }
    }

    /// Run the staged almide with PATH holding nothing, from the scratch dir
    /// (no `almide.toml` in reach). A SIGKILL on a new binary's first exec on
    /// macOS (see STAGING) is retried once; any other outcome is returned.
    fn verify(&self, args: &[&str]) -> Output {
        let run = || {
            Command::new(self.dir.join(exe("almide")))
                .arg("verify")
                .args(args)
                .env("PATH", &self.empty_path)
                .current_dir(&self.dir)
                .output()
                .expect("staged almide runs")
        };
        let first = run();
        if cfg!(target_os = "macos") && killed_by_sigkill(&first) {
            return run();
        }
        first
    }
}

#[cfg(unix)]
fn killed_by_sigkill(o: &Output) -> bool {
    use std::os::unix::process::ExitStatusExt;
    o.status.signal() == Some(9)
}

#[cfg(not(unix))]
fn killed_by_sigkill(_: &Output) -> bool {
    false
}

impl Drop for Stage {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn without_the_verifier_the_shim_fails_by_name_and_has_no_fallback() {
    let stage = Stage::new("absent", false);
    for args in [vec![fixture("heap_arg_call.almd")], vec!["--version".to_string()]] {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let o = stage.verify(&args);
        assert_eq!(o.status.code(), Some(127), "args {args:?}: {}", text(&o.stderr));
        let err = text(&o.stderr);
        assert!(err.contains("error[verifier-missing]") && err.contains("almide-verify"), "{err}");
        assert!(o.stdout.is_empty(), "no verdict may be printed without the verifier: {}", text(&o.stdout));
    }
}

#[test]
fn emit_writes_the_bundle_even_without_the_verifier() {
    let stage = Stage::new("emit", false);
    let out = stage.dir.join("heap_arg_call.bundle");
    let o = stage.verify(&[&fixture("heap_arg_call.almd"), "--emit", &out.display().to_string()]);
    assert_eq!(o.status.code(), Some(127), "{:?}: {}", o.status, text(&o.stderr));
    let bundle = almide_verify::bundle::parse(&std::fs::read(&out).expect("bundle written"))
        .expect("the producer writes a bundle the verifier's parser reads");
    assert!(bundle.metadata.iter().any(|(k, v)| k == "producer" && v.starts_with("almide ")));
    assert!(bundle.artifact.is_none(), "no built file in reach: nothing to pin");
    let (verdicts, _) = almide_verify::bundle::verify(&bundle);
    let main_names: Vec<_> = verdicts.iter().filter(|v| v.function == "main").map(|v| v.property.name()).collect();
    assert_eq!(main_names, ["ownership", "caps"], "per-function witnesses for main");
    for program_level in ["call-modes", "bounds", "init", "fields", "caps-transitive"] {
        assert!(verdicts.iter().any(|v| v.property.name() == program_level), "the program-level {program_level} witness");
    }
    assert!(verdicts.iter().any(|v| v.property.name() == "names"), "the names witnesses of the shipped bytes");
    assert!(verdicts.iter().all(|v| v.accepted), "every heap_arg_call witness is accepted: {verdicts:?}");
}

#[test]
fn mir_emits_the_certificates_of_the_mir_the_native_route_ships() {
    // `--mir` (#4909): the native route's own certificates of the MIR it
    // renders from — a version-5 bundle with one `native-calls` witness and
    // the bounds witnesses — judged by the real verifier (the control).
    let stage = Stage::new("mir", true);
    let out = stage.dir.join("heap_arg_call.bundle");
    let o = stage.verify(&["--mir", &fixture("heap_arg_call.almd"), "--emit", &out.display().to_string()]);
    assert_eq!(o.status.code(), Some(0), "{:?}: {}{}", o.status, text(&o.stdout), text(&o.stderr));
    let bundle = almide_verify::bundle::parse(&std::fs::read(&out).expect("bundle written")).expect("a bundle");
    let (verdicts, _) = almide_verify::bundle::verify(&bundle);
    assert_eq!(verdicts.iter().filter(|v| v.property.name() == "native-calls").count(), 1, "{verdicts:?}");
    assert!(verdicts.iter().any(|v| v.property.name() == "bounds" && v.function == "main"), "{verdicts:?}");
    assert!(!verdicts.iter().any(|v| v.property.name() == "call-modes"), "no v1 witness over MIR no route ships");
    assert!(verdicts.iter().all(|v| v.accepted), "{verdicts:?}");
}

#[test]
fn a_program_the_native_route_walls_is_reported_uncertified() {
    // #4909: a shape the native render does not serve walls the whole
    // program (here a C Region return, `@region(len, release)`: foreign
    // storage no native form holds). The bundle names the wall and
    // holds no witness, and the command fails — nothing falls back to
    // another producer.
    let stage = Stage::new("mir-wall", true);
    let src = stage.dir.join("walled.almd");
    std::fs::write(
        &src,
        "import ffi\n\n@extern(c, \"c\", \"malloc\")\n@region(len = n, release = \"free\")\nfn c_malloc(n: UInt64) -> Region = _\n\neffect fn main() -> Unit = println(\"walled ${ffi.len(c_malloc(8))}\")\n",
    )
    .expect("source");
    let out = stage.dir.join("walled.bundle");
    let o = stage.verify(&["--mir", &src.display().to_string(), "--emit", &out.display().to_string()]);
    assert_eq!(o.status.code(), Some(1), "{:?}: {}", o.status, text(&o.stderr));
    assert!(text(&o.stderr).contains("is uncertified — the native route walls it: native:"), "{}", text(&o.stderr));
    let written = String::from_utf8(std::fs::read(&out).expect("bundle written")).expect("utf-8");
    assert!(written.lines().any(|l| l.starts_with("uncertified (program) native:")), "{written}");
    assert!(!written.lines().any(|l| l.starts_with("witness ")), "{written}");
}

#[test]
fn an_extern_with_no_native_binding_is_refused_before_the_render() {
    // C-461: a host import declared only for the JS host, with no Almide
    // body, has nothing a native binary could run. The native route refuses
    // the program with E081 before it renders anything: no bundle is written.
    let stage = Stage::new("mir-unbound", true);
    let src = stage.dir.join("unbound.almd");
    std::fs::write(
        &src,
        "@extern(wasm, \"js\", \"js_half\")\nfn js_half(x: Float) -> Float\n\nfn main() -> Unit = println(float.to_string(js_half(5.0)))\n",
    )
    .expect("source");
    let out = stage.dir.join("unbound.bundle");
    let o = stage.verify(&["--mir", &src.display().to_string(), "--emit", &out.display().to_string()]);
    assert_eq!(o.status.code(), Some(1), "{:?}: {}", o.status, text(&o.stderr));
    assert!(text(&o.stderr).contains("error[E081]: `js_half` has no implementation on the native target"), "{}", text(&o.stderr));
    assert!(!out.exists(), "no bundle for a refused program");
}

#[test]
fn with_the_verifier_arguments_streams_and_exit_status_pass_through() {
    let stage = Stage::new("present", true);
    let o = stage.verify(&["--version"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    assert!(text(&o.stdout).starts_with("almide-verify "), "the verifier's own version line: {}", text(&o.stdout));

    let leak = stage.dir.join("leak.cert");
    std::fs::write(&leak, "idd\n").expect("witness file");
    let o = stage.verify(&["ownership", &leak.display().to_string()]);
    assert_eq!((o.status.code(), text(&o.stdout).trim().to_string()), (Some(1), "REJECT".to_string()));
}

#[test]
fn a_real_program_is_judged_by_the_real_verifier() {
    let stage = Stage::new("program", true);
    let o = stage.verify(&[&fixture("heap_arg_call.almd")]);
    let out = text(&o.stdout);
    assert!(out.contains("ACCEPT  call-modes") && out.contains("ACCEPT  bounds"), "{out}\n{}", text(&o.stderr));
    assert!(!out.contains("REJECT"), "{out}");

    // A manifest that grants only `Rand`: main's console output is `IO.stdout`
    // (#4155), past what the manifest allows, so the program is refused end to
    // end, exit 1. Since #4155 the build route enforces `[permissions]` itself,
    // so the refusal comes before the certifier runs; the shim's own
    // REJECT pass-through is pinned by the leak witness above.
    std::fs::write(stage.dir.join("almide.toml"), "[package]\nname = \"m\"\nversion = \"0.1.0\"\n\n[permissions]\nallow = [\"Rand\"]\n")
        .expect("manifest");
    std::fs::write(stage.dir.join("app.almd"), "effect fn main() -> Unit = {\n  println(\"hi\")\n}\n").expect("source");
    let o = stage.verify(&["app.almd"]);
    let (out, err) = (text(&o.stdout), text(&o.stderr));
    assert_eq!(o.status.code(), Some(1), "{out}\n{err}");
    assert!(
        err.contains("IO.stdout is not in [permissions].allow") || out.contains("REJECT  caps"),
        "a too-small grant must be refused by the build or the certifier:\n{out}\n{err}"
    );
}

#[test]
fn the_bundle_is_pinned_to_the_built_file_and_a_stale_file_is_refused() {
    let stage = Stage::new("pin", true);
    std::fs::copy(fixture("heap_arg_call.almd"), stage.dir.join("app.almd")).expect("source");
    let built = Command::new(stage.dir.join(exe("almide")))
        .args(["build", "app.almd", "--target", "wasm"])
        .current_dir(&stage.dir)
        .output()
        .expect("staged almide builds");
    assert!(built.status.success(), "{}", text(&built.stderr));
    let o = stage.verify(&["app.almd"]);
    let out = text(&o.stdout);
    assert_eq!(o.status.code(), Some(0), "{out}\n{}", text(&o.stderr));
    assert!(out.contains("ARTIFACT") && out.contains("app.wasm: MATCH"), "the verdict names the built file: {out}");

    // One byte more: the file is no longer what this build produces.
    let wasm = stage.dir.join("app.wasm");
    let mut bytes = std::fs::read(&wasm).expect("built file");
    bytes.push(0);
    std::fs::write(&wasm, bytes).expect("tamper");
    let o = stage.verify(&["app.almd"]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    assert!(text(&o.stderr).contains("is not the module"), "{}", text(&o.stderr));
    assert!(o.stdout.is_empty(), "no verdict on a stale file: {}", text(&o.stdout));
}

/// A plain `fn` calling a pure-declared `@extern` builds with the verifier
/// installed (the checker admits it, and the certificate agrees), and its
/// capability report still lists FOREIGN: foreign code is reachable, never
/// hidden. `spec/wasm_host_js/unmarked_promise.almd` was refused by its own
/// build gate before (`REJECT caps greet`).
#[test]
fn a_plain_fn_calling_a_pure_extern_builds_under_the_verifier_and_reports_foreign() {
    let stage = Stage::new("pure-extern", true);
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("spec/wasm_host_js/unmarked_promise.almd");
    std::fs::copy(&src, stage.dir.join("app.almd")).expect("source");
    let built = Command::new(stage.dir.join(exe("almide")))
        .args(["build", "app.almd", "--target", "wasm", "--host", "js", "-o", "js_out.wasm"])
        .env("PATH", &stage.empty_path)
        .current_dir(&stage.dir)
        .output()
        .expect("staged almide builds");
    assert!(built.status.success(), "{}\n{}", text(&built.stdout), text(&built.stderr));
    assert!(stage.dir.join("js_out.wasm").is_file(), "the gate wrote the artifact");

    let out = stage.dir.join("app.bundle");
    let o = stage.verify(&["app.almd", "--emit", &out.display().to_string()]);
    assert_eq!(o.status.code(), Some(0), "{}\n{}", text(&o.stdout), text(&o.stderr));
    let bundle = almide_verify::bundle::parse(&std::fs::read(&out).expect("bundle written")).expect("a bundle");
    let foreign = 8.to_string();
    let caps = |f: &str| {
        let w = bundle
            .witnesses
            .iter()
            .find(|w| w.function == f && w.property.name() == "caps")
            .unwrap_or_else(|| panic!("{f}'s caps witness"));
        text(&w.bytes)
    };
    for f in ["greet", "poke", "read"] {
        let w = caps(f);
        let reach: Vec<&str> = w.trim().split('|').nth(1).unwrap_or("").split(' ').collect();
        assert!(reach.contains(&foreign.as_str()), "{f} reaches foreign code and says so: {w}");
    }
}
