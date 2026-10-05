//! `map.get(m, k) ?? d` fuses into one `map_get_or(m, &k, d)` call only when
//! evaluating `d` early, inside the call, is unobservable (#3409).
//!
//! The fusion (`try_fuse_map_get_or` in pass_peephole.rs) runs after clone
//! insertion and turns the lazy fallback into an eager sibling argument. Two
//! things go wrong when `d` is not inert:
//!
//! - **ownership**: `map.get(m, key) ?? key` became `map_get_or(m, &key, key)`,
//!   the key borrowed and moved in one argument list — rustc E0505. Unfused,
//!   the lookup's borrow ends before the fallback moves.
//! - **laziness**: `??` evaluates its fallback only on `none`
//!   (docs/specs/language.md), but the fused call evaluated
//!   `map.get(m, "a") ?? noisy("a")` on a hit too.
//!
//! So only a literal, or a variable the lookup's arguments do not mention
//! (or a `Copy` scalar), fuses. The positive cell pins that the common
//! `?? 0` / `?? ""` shape keeps the fusion.
//!
//! Skips cleanly when the `almide` binary is unavailable (CI builds it in the
//! build step; locally run `cargo build --release` first).

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

fn tool_available() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
}

const PROGRAM: &str = r#"fn same_key(m: Map[String, String], key: String) -> String = map.get(m, key) ?? key

fn lit(m: Map[String, Int], key: String) -> Int = map.get(m, key) ?? 0

fn other_var(m: Map[String, String], key: String, d: String) -> String = map.get(m, key) ?? d

fn int_key(m: Map[Int, Int], k: Int) -> Int = map.get(m, k) ?? k

fn noisy(k: String) -> String = {
  println("fallback for " + k)
  k
}

fn effectful(m: Map[String, String]) -> String = map.get(m, "a") ?? noisy("a")

fn main() -> Unit = {
  let m: Map[String, String] = ["a": "alpha"]
  let c: Map[String, Int] = ["a": 1]
  println(same_key(m, "a") + " " + same_key(m, "b"))
  println(int.to_string(lit(c, "a") + lit(c, "b")))
  println(other_var(m, "a", "d") + " " + other_var(m, "b", "d"))
  println(int.to_string(int_key([1: 10], 1) + int_key([1: 10], 2)))
  println(effectful(m))
}
"#;

fn with_program<T>(tag: &str, f: impl FnOnce(&Path) -> T) -> T {
    let dir = std::env::temp_dir().join(format!("almide-map-get-or-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("prog.almd");
    std::fs::write(&src, PROGRAM).unwrap();
    let out = f(&src);
    std::fs::remove_dir_all(&dir).ok();
    out
}

fn emitted() -> String {
    with_program("emit", |src| {
        let output = Command::new(almide_bin())
            .args([src.to_str().unwrap(), "--target", "rust"])
            .output()
            .expect("failed to spawn almide");
        assert!(output.status.success(), "--target rust emit failed:\n{}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8_lossy(&output.stdout).to_string()
    })
}

/// The body of `pub fn <name>(` up to the first column-zero `}`.
fn body<'a>(rust: &'a str, name: &str) -> &'a str {
    let needle = format!("pub fn {name}(");
    let start = rust.find(&needle).unwrap_or_else(|| panic!("emitted Rust has no `{needle}`"));
    let rest = &rust[start..];
    let end = rest.find("\n}").map(|i| i + 2).unwrap_or(rest.len());
    &rest[..end]
}

#[test]
fn an_inert_fallback_still_fuses() {
    if !tool_available() { return; }
    let rust = emitted();
    for name in ["lit", "other_var", "int_key"] {
        let b = body(&rust, name);
        assert!(b.contains("almide_rt_map_get_or("), "`{name}` has an inert fallback and must fuse:\n{b}");
    }
}

#[test]
fn a_fallback_that_moves_a_borrowed_arg_or_has_an_effect_stays_lazy() {
    if !tool_available() { return; }
    let rust = emitted();
    for name in ["same_key", "effectful"] {
        let b = body(&rust, name);
        assert!(!b.contains("almide_rt_map_get_or("), "`{name}` must keep `??` lazy, not fuse:\n{b}");
    }
}

#[test]
fn the_program_runs_with_a_lazy_fallback() {
    if !tool_available() { return; }
    let output = with_program("run", |src| {
        Command::new(almide_bin()).args(["run", src.to_str().unwrap()]).output().expect("spawn almide run")
    });
    assert!(output.status.success(), "run failed:\n{}", String::from_utf8_lossy(&output.stderr));
    // `effectful` hits, so `noisy` never runs and prints nothing.
    assert_eq!(String::from_utf8_lossy(&output.stdout), "alpha b\n1\nalpha d\n12\nalpha\n");
}
