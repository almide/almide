//! Security row 25: the switch lock (#4129) holds exactly the switches whose
//! DECLARED scope changes the build (`Scope::changes_the_build`). A switch
//! declared `debug` / `tool` / `runtime` / `harness` / `ci` is let through the
//! lock from any environment — so that declaration is a security claim, and
//! this file checks it instead of trusting it.
//!
//! For every registered switch the lock lets through, the compiler is run with
//! that switch ON (a flag set to `1`, a value switch set to `1`) and must
//! produce what it produces with no switch at all:
//!   - `almide emit --target rust` — the classic Rust emission, byte for byte;
//!   - `almide build --target wasm` — the module, byte for byte;
//!   - `almide build` (native) — the program's stdout, stderr and exit status,
//!     run with no `ALMIDE_*` in its own environment.
//! A switch that changes any of them is declared in the wrong scope: re-declare
//! it as one the lock holds (`instrument`, `ablation`, `route`, …).
//!
//! The control runs the other way: every `instrument` switch — declared to
//! change the artifact — must change one of the three, so a comparison that
//! cannot see a difference cannot pass.
//!
//! Negative control (2026-10-27): with `ALMIDE_ALLOC_COUNT` and the three wasm
//! allocator switches back in `harness`, this test names all four.

use almide_base::env::{Scope, Switch, SWITCHES};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const PROGRAM: &str = r#"type Shape =
  | Circle(Int)
  | Rect(Int, Int)

type Item = { name: String, qty: Int }

fn area(s: Shape) -> Int = match s {
  Circle(r) => 3 * r * r,
  Rect(w, h) => w * h,
}

effect fn main() -> Unit = {
  let shapes = [Circle(2), Rect(3, 4), Circle(1)]
  let total = shapes |> list.map((s) => area(s)) |> list.fold(0, (a, b) => a + b)
  println("total ${int.to_string(total)}")
  let items = [{ name: "a", qty: 2 }, { name: "bb", qty: 5 }]
  let names = items |> list.filter((i) => i.qty > 1) |> list.map((i) => i.name + "!") |> list.join(",")
  println(names)
  println(int.to_string(int.parse("42") ?? 0))
  var acc = ""
  for i in 0..5 {
    acc = acc + int.to_string(i)
  }
  println(acc)
}
"#;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

/// A command with no inherited `ALMIDE_*`, so only the switch under test is set.
fn clean(program: &Path) -> Command {
    let mut cmd = Command::new(program);
    for (k, _) in std::env::vars_os() {
        if k.to_string_lossy().starts_with("ALMIDE_") {
            cmd.env_remove(&k);
        }
    }
    cmd.stdin(Stdio::null());
    cmd
}

#[derive(PartialEq, Eq, Debug)]
struct Artifacts {
    rust: Vec<u8>,
    wasm: Vec<u8>,
    native_run: (Option<i32>, Vec<u8>, Vec<u8>),
}

fn show(o: &Output) -> String {
    format!(
        "exit {:?}\nstdout:\n{}\nstderr:\n{}",
        o.status.code(),
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

/// Compile PROGRAM in a fresh directory with `switch` (if any) set on the
/// compiler only. `Err` carries why a step failed.
fn artifacts(tag: &str, switch: Option<(&str, &str)>) -> Result<Artifacts, String> {
    let dir: PathBuf = std::env::temp_dir().join(format!("almide-switch-scope-{}-{}", std::process::id(), tag));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.almd"), PROGRAM).unwrap();
    let compiler = |args: &[&str]| {
        let mut c = clean(Path::new(almide()));
        c.args(args).current_dir(&dir);
        if let Some((k, v)) = switch {
            c.env(k, v);
        }
        c.output().expect("spawn almide")
    };

    let rust = compiler(&["emit", "a.almd", "--target", "rust"]);
    if !rust.status.success() {
        return Err(format!("emit --target rust failed:\n{}", show(&rust)));
    }
    let wasm_out = compiler(&["build", "a.almd", "--target", "wasm", "-o", "a.wasm"]);
    if !wasm_out.status.success() {
        return Err(format!("build --target wasm failed:\n{}", show(&wasm_out)));
    }
    let wasm = std::fs::read(dir.join("a.wasm")).map_err(|e| format!("no a.wasm: {e}"))?;
    let native = compiler(&["build", "a.almd", "-o", "a.bin"]);
    if !native.status.success() {
        return Err(format!("native build failed:\n{}", show(&native)));
    }
    let run = clean(&dir.join("a.bin")).current_dir(&dir).output().expect("run the native program");
    let _ = std::fs::remove_dir_all(&dir);
    Ok(Artifacts { rust: rust.stdout, wasm, native_run: (run.status.code(), run.stdout, run.stderr) })
}

/// The value a switch under test is set to: ON for a flag, a present value
/// for a value switch.
const PROBE: &str = "1";

/// Which of the three artifacts differ from `base`.
fn differs(base: &Artifacts, got: &Artifacts) -> Vec<&'static str> {
    let mut d = Vec::new();
    if base.rust != got.rust {
        d.push("the emitted Rust");
    }
    if base.wasm != got.wasm {
        d.push("the wasm module");
    }
    if base.native_run != got.native_run {
        d.push("the native program's output");
    }
    d
}

/// Run `f` over `items` on a few threads, keeping the input order.
fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let n = std::thread::available_parallelism().map_or(4, |n| n.get()).min(8);
    let chunk = items.len().div_ceil(n).max(1);
    std::thread::scope(|s| {
        let hs: Vec<_> = items.chunks(chunk).map(|c| s.spawn(|| c.iter().map(&f).collect::<Vec<R>>())).collect();
        hs.into_iter().flat_map(|h| h.join().unwrap()).collect()
    })
}

#[test]
fn a_switch_the_lock_lets_through_does_not_change_the_build() {
    let base = artifacts("base", None).unwrap_or_else(|e| panic!("baseline: {e}"));
    let again = artifacts("base-again", None).unwrap_or_else(|e| panic!("baseline rerun: {e}"));
    assert_eq!(differs(&base, &again), Vec::<&str>::new(), "the baseline is not reproducible, so nothing below can be compared");

    let unlocked: Vec<&Switch> = SWITCHES.iter().filter(|s| !s.scope.changes_the_build()).collect();
    assert!(unlocked.len() > 50, "the roster shrank to {} unlocked switches — is this the right registry?", unlocked.len());
    let verdicts = par_map(&unlocked, |sw| (sw, artifacts(sw.name, Some((sw.name, PROBE)))));
    let mut wrong = Vec::new();
    for (sw, got) in verdicts {
        match got {
            Ok(got) => {
                let d = differs(&base, &got);
                if !d.is_empty() {
                    wrong.push(format!("{} (declared `{}`) changes {}", sw.name, sw.scope.label(), d.join(" and ")));
                }
            }
            Err(e) => wrong.push(format!("{} (declared `{}`) changes whether the build succeeds: {e}", sw.name, sw.scope.label())),
        }
    }
    assert!(
        wrong.is_empty(),
        "switch(es) the lock lets through change the build — re-declare each in a scope the lock holds \
         (crates/almide-base/src/env.rs):\n  {}",
        wrong.join("\n  ")
    );
}

#[test]
fn every_instrument_switch_changes_the_build() {
    let base = artifacts("ctl-base", None).unwrap_or_else(|e| panic!("baseline: {e}"));
    let inst: Vec<&Switch> = SWITCHES.iter().filter(|s| s.scope == Scope::Instrument).collect();
    assert!(!inst.is_empty(), "no `instrument` switch is registered, so this control controls nothing");
    let verdicts = par_map(&inst, |sw| (sw, artifacts(&format!("ctl-{}", sw.name), Some((sw.name, PROBE)))));
    for (sw, got) in verdicts {
        let got = got.unwrap_or_else(|e| panic!("{}: {e}", sw.name));
        assert!(
            !differs(&base, &got).is_empty(),
            "{} is declared `instrument` but changes none of the emitted Rust, the wasm module or the native \
             program's output — either the comparison is blind or the declaration is wrong",
            sw.name
        );
    }
}
