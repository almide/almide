//! #2239: a `fan` arm holding a value's LAST occurrence moves it into its
//! `__fan_cap_*` binding instead of cloning it — the lambda capture-move rule
//! (#2231), keyed on the arm's identity from the use walk. What is asserted is
//! the emitted bind of each capture, on one program carrying every shape:
//!
//! - a value two arms name: the first arm clones, the second (last) moves;
//! - a value an arm names that is used again AFTER the fan: the arm clones;
//! - a value only one arm names: that arm moves it.
//!
//! The ablation `ALMIDE_CAPTURE_MOVE_OFF=1` empties the rule's use table, so
//! under it every bind clones again — the control that shows the moves come
//! from the rule and not from the emitter.
use std::path::Path;
use std::process::Command;

const PROGRAM: &str = r#"effect fn shout(s: String) -> Result[String, String] = ok(string.to_upper(s))

effect fn size(xs: List[Int]) -> Result[Int, String] = ok(list.len(xs))

effect fn main() -> Unit = {
  let reply = "hello"
  let (a, b) = fan {
    shout(reply)
    shout(reply + "!")
  }
  let xs = [1, 2, 3]
  let (n, m) = fan {
    size(xs)
    size([9])
  }
  println("${a} ${b} ${n} ${m} ${list.len(xs)}")
  let word = "solo"
  let (w, _z) = fan {
    shout(word)
    size([1])
  }
  println(w)
}
"#;

fn emit(dir: &Path, env: &[(&str, &str)]) -> Vec<String> {
    let file = dir.join("fan_move.almd");
    std::fs::write(&file, PROGRAM).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_almide"));
    cmd.arg(&file).args(["--target", "rust"]);
    for (k, v) in env { cmd.env(k, v); }
    let out = cmd.output().expect("almide");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    // The capture binds only — not the `thread::scope` line that names them.
    String::from_utf8_lossy(&out.stdout).lines()
        .map(|l| l.trim())
        .filter(|l| l.strip_prefix("let ").map(|r| r.strip_prefix("mut ").unwrap_or(r)).is_some_and(|r| r.starts_with("__fan_cap_")))
        .map(|l| l.to_string())
        .collect()
}

/// `(captured var, moved?)` per `__fan_cap_*` bind, in emission order.
fn binds(lines: &[String]) -> Vec<(String, bool)> {
    lines.iter().map(|l| {
        let rhs = l.split('=').nth(1).unwrap().trim().trim_end_matches(';');
        let var = rhs.trim_end_matches(".clone()").to_string();
        (var, !rhs.ends_with(".clone()"))
    }).collect()
}

#[test]
fn the_arm_holding_the_last_occurrence_moves_and_every_other_capture_clones() {
    let dir = tempfile::tempdir().unwrap();
    let lines = emit(dir.path(), &[]);
    let got = binds(&lines);
    let want = vec![
        ("reply".to_string(), false), // first arm: `reply` is named again by the second arm
        ("reply".to_string(), true),  // second arm: its last occurrence
        ("xs".to_string(), false),    // used after the fan
        ("word".to_string(), true),   // only this arm names it
    ];
    assert_eq!(got, want, "emitted binds:\n{}", lines.join("\n"));
}

#[test]
fn with_the_rule_ablated_every_fan_capture_clones() {
    let dir = tempfile::tempdir().unwrap();
    // The certifier would refuse the ablated build (C3, its debug default is
    // `fail`), which is the point of the ablation; switch it off to read the emit.
    let lines = emit(dir.path(), &[("ALMIDE_CAPTURE_MOVE_OFF", "1"), ("ALMIDE_CERTIFY_OWNERSHIP", "off")]);
    let got = binds(&lines);
    assert_eq!(got.len(), 4, "{}", lines.join("\n"));
    assert!(got.iter().all(|(_, moved)| !moved), "a bind moved under the ablation:\n{}", lines.join("\n"));
}

#[test]
fn the_program_prints_the_same_on_both_targets() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("fan_move.almd");
    std::fs::write(&file, PROGRAM).unwrap();
    let mut outs = Vec::new();
    for target in ["rust", "wasm"] {
        let out = Command::new(env!("CARGO_BIN_EXE_almide"))
            .arg("run").arg(&file).args(["--target", target])
            .output().expect("almide run");
        assert!(out.status.success(), "{target}: {}", String::from_utf8_lossy(&out.stderr));
        outs.push(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    assert_eq!(outs[0], "HELLO HELLO! 3 1 3\nSOLO\n");
    assert_eq!(outs[0], outs[1]);
}
