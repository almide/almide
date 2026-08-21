//! A trailing Unit expression in an `effect fn` that calls through a generic
//! protocol bound must still be lifted to `Ok(())` (#1546).
//!
//! mono's `fix_body_match_ty` overwrote `body.ty` with the declared `ret_ty`.
//! An effect fn's body is not Ok-lifted at that point — ResultPropagation runs
//! downstream of mono — so the body then CLAIMED to be a Result without wrapping
//! one, and ResultPropagation's "already Result, nothing to repair" gate skipped
//! the lift. rustc rejected the generated code with E0308 "expected
//! `Result<(), String>`, found `()`". Only programs containing a generic bound
//! were affected: without one, mono leaves the body ty alone and the gate fires.
//!
//! Lives at the compiler level (not spec/) because the lift is a RUST-target
//! property — `wrap_non_result` is `matches!(target, Target::Rust)` — so a corpus
//! file would run on the wasm leg and assert nothing.

use std::io::Write;
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

/// Compile + run `src` on the Rust target; assert it prints `expected`.
fn run_prints(name: &str, src: &str, expected: &str) {
    let dir = std::env::temp_dir().join(format!("almd_1546_{name}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.almd");
    let mut f = std::fs::File::create(&file).unwrap();
    f.write_all(src.as_bytes()).unwrap();
    drop(f);
    let out = Command::new(almide_bin())
        .args(["run", file.to_str().unwrap()])
        .output()
        .expect("failed to spawn almide");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    std::fs::remove_dir_all(&dir).ok();
    assert!(
        out.status.success(),
        "[{name}] almide run failed (the #1546 missing-ok-lift shape?):\n{stderr}"
    );
    assert_eq!(stdout.trim_end(), expected, "[{name}] wrong output");
}

const DECLS: &str = r#"type Priced = { total: Int }
protocol Rate {
  fn pct(r: Self) -> Int
}
type FlatRate: Rate = { pct_value: Int }
fn FlatRate.pct(r: FlatRate) -> Int = r.pct_value
fn discounted[R: Rate](r: R, p: Priced) -> Int = p.total - p.total * r.pct() / 100
"#;

#[test]
fn trailing_call_tail_lifts_with_a_generic_bound_in_the_body() {
    run_prints(
        "tail_call",
        &format!(
            r#"{DECLS}
effect fn main() -> Result[Unit, String] = {{
  let r: FlatRate = {{ pct_value: 10 }}
  println("total: ${{int.to_string(discounted(r, {{ total: 2000 }}))}}")
}}
"#
        ),
        "total: 1800",
    );
}

#[test]
fn trailing_tail_lifts_when_the_bounded_call_is_let_bound() {
    // The trigger is the monomorphized call being present in the body at all —
    // not the tail expression referencing it.
    run_prints(
        "let_bound",
        &format!(
            r#"{DECLS}
effect fn main() -> Result[Unit, String] = {{
  let r: FlatRate = {{ pct_value: 10 }}
  let n = discounted(r, {{ total: 2000 }})
  println("total: ${{int.to_string(n)}}")
}}
"#
        ),
        "total: 1800",
    );
}
