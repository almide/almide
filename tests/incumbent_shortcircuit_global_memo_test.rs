//! A module-level `let` first read on the RIGHT of a short-circuit `and`/`or`
//! must read its true value on the path where that right side never ran (#2521,
//! C-191's short-circuit cell).
//!
//! `a and b` lowers to `if a then b else false` on the incumbent v1 leg, so a
//! global materialized while lowering `b` has its defining op inside the THEN
//! block. The per-function memo cached that ValueId anyway — none of the three
//! "do not memoize here" depths is raised around a short-circuit operand — and a
//! later reader on the `else` path got a local that never initialized, which
//! wasm reads as 0. `f(0)` returned 0 where native returned 3000, with no trap
//! and a `verified` build; the heap twin additionally trapped in `rc_dec` at
//! teardown, dropping a handle from a block that never allocated.
//!
//! WHY THIS BINARY EXISTS ALONGSIDE spec/wasm_cross/global_in_shortcircuit_operand.almd:
//! the corpus gates run that fixture on the STRUCTURAL leg (and re-render it
//! through the v1 renderer for host determinism, which byte-compares the module
//! across hosts and never executes it). The defect is incumbent-only, so the
//! fixture alone would have stayed green through the whole regression. Here the
//! incumbent leg is run and compared against native, which is what fails.
//!
//! The reported route was `@export(wasm)` — the router sends an exported module
//! to the incumbent, which is why `almide run --target wasm` looked correct
//! while the exported `f` did not. Same MIR, so the leg is what to pin.
use std::process::Command;

/// The issue's first repro, plus the `or` mirror, the heap twin and a chain.
/// Every printed value is read on a path where the short-circuit's right side
/// was SKIPPED, except the controls that check the other side still works.
const SHORTCIRCUIT: &str = r#"let CAP = 3000

let TAG = "alpha"

fn gate(n: Int) -> Int = {
  let live = n > 0 and n < CAP
  if live then 1 else CAP
}

fn gate_or(n: Int) -> Int = {
  let live = n > 0 or n < CAP
  if live then CAP else 1
}

fn pick(flag: Bool, s: String) -> String =
  if flag and s == TAG then "hit" else TAG

fn chain(n: Int) -> Int = {
  let live = n > 0 and n > 1 and n < CAP
  if live then 1 else CAP
}

fn main() -> Unit = {
  println(int.to_string(gate(0)))
  println(int.to_string(gate(5)))
  println(int.to_string(gate_or(5)))
  println(int.to_string(gate_or(0)))
  println(pick(false, "alpha"))
  println(pick(true, "alpha"))
  println(int.to_string(chain(0)))
  println(int.to_string(chain(2)))
}
"#;

/// The issue's second repro: the short-circuit lives in a `while` CONDITION and
/// the tail reads the cap again after zero iterations, so both comparisons ran
/// against 0 and `f(0)` answered `CAP` — itself 0.
const WHILE_CONDITION: &str = r#"let CAP = 3000

fn backoff(n: Int) -> Int = {
  var d = 500
  while n > 0 and d < CAP {
    d = d * 2
  }
  if d > CAP then CAP else d
}

fn main() -> Unit = {
  println(int.to_string(backoff(0)))
  println(int.to_string(backoff(3)))
}
"#;

/// The control: a global read in a region that ALREADY raises one of the three
/// depths (a real `if` arm, a `match` arm, a loop body, a `??` right side) was
/// never affected, and must stay right. A fix that widened the refusal instead
/// of aiming it would still pass; a fix that narrowed it would not.
const ALREADY_GUARDED: &str = r#"let CAP = 3000

fn arm(n: Int) -> Int = {
  let x = if n > 0 then CAP else 0
  if x > 0 then 1 else CAP
}

fn armed_match(n: Int) -> Int = {
  let x = match n {
    1 => CAP,
    _ => 0,
  }
  if x > 0 then 1 else CAP
}

fn body(n: Int) -> Int = {
  var acc = 0
  var i = 0
  while i < n {
    acc = acc + CAP
    i = i + 1
  }
  if acc > 0 then 1 else CAP
}

fn fallback(n: Int) -> Int = {
  let o: Int? = if n > 0 then some(7) else none
  let x = o ?? CAP
  if x > 100 then 1 else CAP
}

fn main() -> Unit = {
  println(int.to_string(arm(0)))
  println(int.to_string(armed_match(0)))
  println(int.to_string(body(0)))
  println(int.to_string(fallback(5)))
}
"#;

fn almide() -> String {
    std::env::var("ALMIDE_BIN")
        .unwrap_or_else(|_| format!("{}/target/release/almide", env!("CARGO_MANIFEST_DIR")))
}

/// Run one source on all three legs and return (native, structural, incumbent)
/// stdout, each trimmed. A leg that fails to run returns its stderr instead, so
/// the assertion names what happened rather than comparing two empty strings —
/// the pre-fix heap twin trapped in `rc_dec`, which an stdout-only compare would
/// have read as "both printed nothing".
fn three_legs(src: &str) -> (String, String, String) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("main.almd");
    std::fs::write(&file, src).unwrap();
    let path = file.to_str().unwrap();
    let take = |out: std::process::Output| {
        if out.status.success() {
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        } else {
            format!(
                "<failed: {}>\n{}",
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            )
        }
    };
    let native = take(Command::new(almide()).args(["run", path]).output().unwrap());
    let structural = take(
        Command::new(almide()).args(["run", path, "--target", "wasm"]).output().unwrap(),
    );
    let incumbent = take(
        Command::new(almide())
            .args(["run", path, "--target", "wasm"])
            .env("ALMIDE_WASM_INCUMBENT", "1")
            .output()
            .unwrap(),
    );
    (native, structural, incumbent)
}

fn all_three_agree(src: &str, expected: &str) {
    let (native, structural, incumbent) = three_legs(src);
    assert_eq!(native, expected, "native is the oracle here and must print the stated values");
    assert_eq!(structural, native, "the structural wasm leg must agree with native");
    assert_eq!(
        incumbent, native,
        "the incumbent wasm leg must agree with native — a global read on a \
         short-circuit's right side must not be memoized for a path that skipped it"
    );
}

#[test]
fn a_global_read_on_a_short_circuit_right_side_is_true_on_the_skipped_path() {
    all_three_agree(SHORTCIRCUIT, "3000\n1\n3000\n3000\nalpha\nhit\n3000\n1");
}

#[test]
fn a_global_read_in_a_while_condition_survives_zero_iterations() {
    all_three_agree(WHILE_CONDITION, "500\n3000");
}

#[test]
fn a_global_read_in_an_already_guarded_region_still_reads_its_value() {
    all_three_agree(ALREADY_GUARDED, "3000\n3000\n3000\n3000");
}
