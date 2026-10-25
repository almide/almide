//! The classified fs errnos (C-273, owner ruling 2026-10-17): ENOTEMPTY,
//! ENAMETOOLONG and EXDEV differ between the compared hosts in number or text,
//! so no leg answers the host's `Display` for them — every leg answers ONE
//! errno-free text from `almide_base::fs_errno`. Before, `fs.remove` of a
//! non-empty directory read `Directory not empty (os error 66)` natively on
//! macOS, `(os error 39)` on Linux, and `filesystem operation failed (wasi
//! errno 55)` from the stock artifact.
//!
//! The probe runs on the four legs the CLI drives: native (`almide run`), the
//! embedded host (`almide run --target wasm`), the stock p1 artifact
//! (`almide build --target wasm` under wasmtime) and the p3 component
//! (`ALMIDE_COMPONENT_P3=1 almide build --component` under wasmtime 46+), and
//! every leg must print the expected bytes. EXDEV needs a second filesystem,
//! which no CI runner provides, so it is pinned by the table's unit tests
//! (`crates/almide-base/tests/fs_errno_table.rs`), not here.
use std::path::{Path, PathBuf};
use std::process::Command;

const PROBE: &str = r#"import fs

fn show(r: Result[Unit, String]) -> String = match r {
  ok(_) => "ok",
  err(m) => m,
}

effect fn main() -> Unit = {
  fs.mkdir_p("full/sub")!
  fs.write_text("full/sub/f.txt", "x")!
  fs.mkdir_p("other")!
  fs.write_text("other/g.txt", "y")!
  println(show(fs.remove("full")))
  println(show(fs.rename("other", "full")))
  let long = string.repeat("n", 300)
  println(show(fs.write_text(long, "z")))
  println(match fs.read_text(long) { ok(v) => "ok(${v})", err(m) => m })
  println(show(fs.mkdir_p("full/" + long)))
}
"#;

fn expected() -> String {
    let long = "n".repeat(300);
    [
        "fs.remove(\"full\"): Directory not empty".to_string(),
        "fs.rename(\"other\", \"full\"): Directory not empty".to_string(),
        format!("fs.write_text(\"{long}\"): File name too long"),
        format!("fs.read_text(\"{long}\"): File name too long"),
        format!("fs.mkdir_p(\"full/{long}\"): File name too long"),
    ]
    .join("\n")
        + "\n"
}

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().expect("utf8 path").to_string();
    }
    "almide".to_string()
}

/// A fresh cwd per leg (each leg creates the same tree) with the probe in it.
fn leg_dir(scratch: &Path, leg: &str) -> PathBuf {
    let d = scratch.join(leg);
    std::fs::create_dir_all(&d).expect("mkdir");
    std::fs::write(d.join("probe.almd"), PROBE).expect("write");
    d
}

fn stdout_of(mut c: Command, what: &str) -> String {
    let o = c.output().unwrap_or_else(|e| panic!("{what}: {e}"));
    assert!(o.status.success(), "{what} failed:\n{}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).expect("utf8")
}

fn run_cli(dir: &Path, args: &[&str]) -> String {
    let mut c = Command::new(almide_bin());
    c.arg("run").arg("probe.almd").args(args).current_dir(dir).env("PWD", dir);
    stdout_of(c, &format!("almide run {args:?}"))
}

/// The installed wasmtime's major version, if there is one.
fn wasmtime_major() -> Option<u32> {
    let o = Command::new("wasmtime").arg("--version").output().ok()?;
    let v = String::from_utf8_lossy(&o.stdout).to_string();
    v.split_whitespace().nth(1)?.split('.').next()?.parse().ok()
}

/// Build the probe as a stock p1 module (or, `p3`, a WASI 0.3 component) and
/// run it under wasmtime from `dir`.
fn run_wasmtime(dir: &Path, p3: bool) -> String {
    let wasm = dir.join("probe.wasm");
    let mut build = Command::new(almide_bin());
    build.args(["build", "probe.almd", "--target", "wasm", "-o"]).arg(&wasm).current_dir(dir);
    if p3 {
        build.arg("--component").env("ALMIDE_COMPONENT_P3", "1");
    } else {
        build.env_remove("ALMIDE_COMPONENT_P3");
    }
    let _ = stdout_of(build, "almide build");
    let mut run = Command::new("wasmtime");
    run.args(["run", "--dir=/", "-S", "inherit-env=y"]).arg(&wasm).current_dir(dir).env("PWD", dir);
    stdout_of(run, "wasmtime run")
}

#[test]
fn the_classified_errnos_read_the_same_errno_free_text_on_every_leg() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let scratch = tempfile::tempdir().expect("tempdir");
    let want = expected();
    assert_eq!(run_cli(&leg_dir(scratch.path(), "native"), &[]), want, "native");
    assert_eq!(run_cli(&leg_dir(scratch.path(), "embedded"), &["--target", "wasm"]), want, "the embedded host");
    let Some(major) = wasmtime_major() else {
        assert!(std::env::var_os("ALMIDE_EXPECT_TOOLS").is_none(), "ALMIDE_EXPECT_TOOLS: wasmtime is not on PATH");
        return;
    };
    assert_eq!(run_wasmtime(&leg_dir(scratch.path(), "p1"), false), want, "the stock p1 artifact");
    if major >= 46 {
        assert_eq!(run_wasmtime(&leg_dir(scratch.path(), "p3"), true), want, "the p3 component");
    }
}

/// The fifth leg, the interp: its writes land in the per-interpreter VFS
/// overlay, which must refuse what the host refuses — a non-empty directory
/// removed or renamed onto, a path component past NAME_MAX — in the same words.
#[test]
fn the_interp_vfs_answers_the_same_classified_texts() {
    let scratch = tempfile::tempdir().expect("tempdir");
    let entry = leg_dir(scratch.path(), "interp").join("probe.almd");
    let ir = almide::wasm_leg::lower_to_ir(entry.to_str().expect("utf8"), PROBE).expect("front failed");
    let out = almide_interp::Interpreter::new(&ir).run_main();
    assert_eq!(out.status, almide_interp::RunStatus::Ok, "interp: stderr=<{}>", out.stderr);
    assert_eq!(out.stdout, expected(), "the interp VFS");
}
