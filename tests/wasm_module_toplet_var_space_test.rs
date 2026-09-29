//! #2807: an imported module's top-level let initializer lowers inside `main`'s
//! prologue, but its VarIds index the MODULE's VarTable, which restarts at 0
//! like every other. The structural wasm leg looked a Var up in `main`'s
//! locals before the module's globals, so a module initializer reading a
//! sibling top-let (`let NAMES = ["a", DEV]`) read whichever `main` local
//! happened to carry the same VarId:
//!
//! - a local of another type walled the build
//!   (`ty-mismatch:List(ETy(2))-vs-Scalar(Str)`, the reporter's package), and
//!   the router handed the program to the incumbent;
//! - a local of the SAME type built, and read the still-unset local: wasm
//!   printed `a43` where native printed `a43dev`.
//!
//! A module initializer that lowers inline is bind-free, so it reads globals
//! only; `main`'s VarId-keyed tables are hidden while it lowers.
//!
//! The same package also showed the wall's `-->` line naming the WRONG file:
//! the module's line was printed as an entry-file line of `main`. A wall in a
//! top-let initializer is now reported as that top-let, in its own module.
//!
//! Module projects cannot be `spec/wasm_cross` fixtures (single files), so the
//! cross-target evidence lives here (C-077).

use std::path::Path;
use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

const TOML: &str = "[package]\nname = \"spacepkg\"\nversion = \"0.1.0\"\n";

fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("almide.toml"), TOML).unwrap();
    for (rel, src) in files {
        let p = dir.path().join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, src).unwrap();
    }
    dir
}

fn native_stdout(dir: &Path) -> String {
    let r = Command::new(almide())
        .current_dir(dir)
        .args(["run", "src/main.almd"])
        .output()
        .expect("native run");
    assert!(r.status.success(), "native run failed:\n{}", String::from_utf8_lossy(&r.stderr));
    String::from_utf8_lossy(&r.stdout).into_owned()
}

/// Build for wasm (structural leg forced when `structural`); `Err(stderr)` on
/// a failed build, else the wasmtime stdout (`None` without wasmtime).
fn wasm_stdout(dir: &Path, structural: bool) -> Result<Option<String>, String> {
    let mut cmd = Command::new(almide());
    cmd.current_dir(dir).args(["build", "src/main.almd", "--target", "wasm", "-o", "out.wasm"]);
    if structural {
        cmd.env("ALMIDE_WASM_SKIP_STOCK_AUDIT", "1");
    }
    let b = cmd.output().expect("wasm build");
    if !b.status.success() {
        return Err(String::from_utf8_lossy(&b.stderr).into_owned());
    }
    let Ok(w) = Command::new("wasmtime").current_dir(dir).arg("out.wasm").output() else {
        return Ok(None);
    };
    assert!(w.status.success(), "wasm run failed:\n{}", String::from_utf8_lossy(&w.stderr));
    Ok(Some(String::from_utf8_lossy(&w.stdout).into_owned()))
}

fn assert_same_on_both(dir: &Path, structural: bool) {
    let native = native_stdout(dir);
    match wasm_stdout(dir, structural) {
        Err(stderr) => panic!("wasm build failed (structural forced: {structural}):\n{stderr}"),
        Ok(Some(wasm)) => assert_eq!(native, wasm, "native and wasm disagree (structural forced: {structural})"),
        Ok(None) => {}
    }
}

const FIXES: &str = "let DEV = \"dev\"

let NAMES: List[String] = [\"a\", DEV]

fn who_of(id: String) -> String = NAMES.join(id)
";

/// Same-typed collision: `main`'s String local and the module's `DEV` share a
/// VarId. Before the fix this BUILT on the structural leg and printed `a43`.
#[test]
fn a_module_initializer_reads_its_own_global_not_a_main_local() {
    let main = "import self.fixes

effect fn main() -> Unit = {
  let s = int.to_string(41 + list.len(fixes.who_of(\"-\").split(\"-\")))
  println(fixes.who_of(s))
}
";
    let dir = project(&[("src/fixes.almd", FIXES), ("src/main.almd", main)]);
    assert_eq!(native_stdout(dir.path()), "a43dev\n");
    assert_same_on_both(dir.path(), false);
    assert_same_on_both(dir.path(), true);
}

/// The reporter's shape: `main` binds `process.args()` (a List) and the
/// module's record catalog names a String constant. Walled the structural leg
/// `ty-mismatch:List(ETy(..))-vs-Scalar(Str)` before the fix.
#[test]
fn a_module_record_catalog_naming_a_constant_builds_on_the_structural_leg() {
    let fixes = "let DEV = \"dev\"

type Fix = { id: String, who: String }

let FIXES: List[Fix] = [
  Fix { id: \"SEC-01\", who: DEV },
  Fix { id: \"SEC-02\", who: \"x\" }
]

let NAMES: List[String] = [\"a\", DEV]

fn who_of(id: String) -> String = {
  let hit = FIXES |> list.find((f) => f.id == id)
  match hit {
    some(f) => f.who + \"/\" + NAMES.join(\",\"),
    none => \"?\",
  }
}
";
    let main = "import process
import self.fixes

effect fn main() -> Unit = {
  let argv = process.args()
  println(fixes.who_of(\"SEC-01\"))
  println(fixes.who_of(\"SEC-02\"))
  println(int.to_string(list.len(argv)))
}
";
    let dir = project(&[("src/fixes.almd", fixes), ("src/main.almd", main)]);
    assert_eq!(native_stdout(dir.path()), "dev/a,dev\nx/a,dev\n1\n");
    assert_same_on_both(dir.path(), true);
}

/// A declined shape (a `continue` in a VALUE-position block, `expr:Continue`
/// today — the statement-position forms lower since #2745; swap in any other
/// decline if it starts lowering: the assertion is the location).
const DECLINES: &str = "// line 1

let COUNT: Int = {
  var n = 0
  for x in [1, 0, 2] {
    let k = {
      if x <= 0 then continue
      1
    }
    n = n + k
  }
  n
}

fn keep(xs: List[Int]) -> Int = {
  var n = 0
  for x in xs {
    let k = {
      if x <= 0 then continue
      1
    }
    n = n + k
  }
  n
}
";

fn structural_wall_site(main: &str) -> String {
    let dir = project(&[("src/fixes.almd", DECLINES), ("src/main.almd", main)]);
    let stderr = wasm_stdout(dir.path(), true).expect_err("the shape must wall the structural leg");
    stderr
        .lines()
        .find(|l| l.trim_start().starts_with("-->"))
        .unwrap_or_else(|| panic!("no `-->` site line; stderr:\n{stderr}"))
        .trim()
        .to_string()
}

#[test]
fn a_wall_in_a_module_top_let_names_that_top_let_and_module() {
    let site = structural_wall_site(
        "import self.fixes\n\neffect fn main() -> Unit = println(int.to_string(fixes.COUNT))\n",
    );
    assert_eq!(site, "--> in top-level let `fixes.COUNT` (module `fixes`, line 6)");
}

#[test]
fn a_wall_in_a_module_fn_names_that_fn_and_module() {
    let site = structural_wall_site(
        "import self.fixes\n\neffect fn main() -> Unit = println(int.to_string(fixes.keep([1, 0])))\n",
    );
    assert_eq!(site, "--> in fn `fixes.keep` (module `fixes`, line 18)");
}
