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
//!   capability violation in source comes back as a REJECT and exit 1.

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

/// The real verifier binary. The workspace's default members build it next
/// to almide (and the test archive carries it); a `cargo test -p almide` run
/// that did not, builds it into a private target directory — never the one
/// this test process is running from, whose lock the outer cargo may hold.
fn verifier() -> PathBuf {
    let beside = almide().with_file_name(exe("almide-verify"));
    if beside.is_file() {
        return beside;
    }
    let target = std::env::temp_dir().join("almide-verify-shim-target");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = Command::new(cargo)
        .args(["build", "-q", "-p", "almide-verify", "--target-dir"])
        .arg(&target)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status()
        .expect("cargo runs");
    assert!(status.success(), "building almide-verify for the shim test failed");
    target.join("debug").join(exe("almide-verify"))
}

/// A scratch directory next to the almide under test (same filesystem, so
/// the binary can be hard-linked rather than copied), holding `almide` and,
/// when `with_verifier`, `almide-verify`.
struct Stage {
    dir: PathBuf,
    empty_path: PathBuf,
}

impl Stage {
    fn new(tag: &str, with_verifier: bool) -> Stage {
        let dir = almide().with_file_name(format!("verify-shim-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let empty_path = dir.join("empty-path");
        std::fs::create_dir_all(&empty_path).expect("scratch dir");
        let place = |from: &Path, name: &str| {
            let to = dir.join(exe(name));
            if std::fs::hard_link(from, &to).is_err() {
                std::fs::copy(from, &to).expect("copy binary");
            }
        };
        place(&almide(), "almide");
        if with_verifier {
            place(&verifier(), "almide-verify");
        }
        Stage { dir, empty_path }
    }

    /// Run the staged almide with PATH holding nothing, from the scratch dir
    /// (no `almide.toml` in reach).
    fn verify(&self, args: &[&str]) -> Output {
        Command::new(self.dir.join(exe("almide")))
            .arg("verify")
            .args(args)
            .env("PATH", &self.empty_path)
            .current_dir(&self.dir)
            .output()
            .expect("staged almide runs")
    }
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
    assert_eq!(o.status.code(), Some(127), "{}", text(&o.stderr));
    let bundle = almide_verify::bundle::parse(&std::fs::read(&out).expect("bundle written"))
        .expect("the producer writes a bundle the verifier's parser reads");
    assert!(bundle.metadata.iter().any(|(k, v)| k == "producer" && v.starts_with("almide ")));
    let (verdicts, _) = almide_verify::bundle::verify(&bundle);
    let main_names: Vec<_> = verdicts.iter().filter(|v| v.function == "main").map(|v| v.property.name()).collect();
    assert_eq!(main_names, ["ownership", "names", "caps"], "per-function witnesses for main");
    assert!(verdicts.iter().any(|v| v.property.name() == "call-modes"), "the program-level call-mode witness");
    assert!(verdicts.iter().all(|v| v.accepted), "every heap_arg_call witness is accepted: {verdicts:?}");
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
    assert!(out.contains("ACCEPT  call-modes"), "{out}\n{}", text(&o.stderr));
    assert!(!out.contains("REJECT"), "{out}");

    // print_str's main reaches Stdout without declaring it: the sandbox
    // promise, caught on real source, comes back through the shim as exit 1.
    let o = stage.verify(&[&fixture("print_str.almd")]);
    let out = text(&o.stdout);
    assert_eq!(o.status.code(), Some(1), "{out}\n{}", text(&o.stderr));
    assert!(out.contains("REJECT  caps") && out.contains("-> REJECTED"), "{out}");
}
