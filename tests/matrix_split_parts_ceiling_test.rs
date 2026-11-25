//! #4914, C-161: `matrix.split_cols_even(m, n)` answers n parts of rows(m)
//! rows each, so its part count is a row count and n x rows(m) row slots take
//! the shared matrix ceiling (2^28) on every leg: over it the call aborts in
//! the T6 form (`Error: matrix dimensions too large`, exit 1) before anything
//! is allocated. Before, native allocated the slots (n = 3e8 over 2 rows:
//! 11.7 GB, exit 0) or died in a raw abort (n = 2^40, SIGABRT) where wasm
//! answered `Error: out of memory`. Within the ceiling the documented answer
//! stands: n parts cols/n wide, 0 wide once n > cols.

use std::process::{Command, Output};

/// `ALMIDE_CONTROL_BIN` points the assertions at another build (the negative
/// control: a compiler without the part-count guard passes them not).
fn almide(dir: &std::path::Path, args: &[&str]) -> Output {
    let bin = std::env::var("ALMIDE_CONTROL_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string());
    Command::new(bin).args(args).current_dir(dir).output().expect("almide runs")
}

fn legs(program: &str) -> Vec<(String, Output)> {
    let td = tempfile::TempDir::new().unwrap();
    std::fs::write(td.path().join("main.almd"), program).unwrap();
    [&["run", "main.almd"][..], &["run", "main.almd", "--target", "wasm"]]
        .iter()
        .map(|args| (args.join(" "), almide(td.path(), args)))
        .collect()
}

fn split(rows: i64, cols: i64, n: &str) -> String {
    format!(
        "effect fn main() -> Unit = {{\n  let parts = matrix.split_cols_even(matrix.zeros({rows}, {cols}), {n})\n  println(\"parts=${{list.len(parts)}}\")\n}}\n"
    )
}

#[test]
fn novel_a_part_count_over_the_row_slot_ceiling_aborts_alike_on_both_legs() {
    // A fresh row count (3) just past the ceiling by division, and a huge n.
    for program in [split(3, 5, "89478486"), split(2, 3, "1099511627776")] {
        for (leg, o) in legs(&program) {
            let err = String::from_utf8_lossy(&o.stderr);
            let out = String::from_utf8_lossy(&o.stdout);
            assert_eq!(o.status.code(), Some(1), "`almide {leg}`:\n{program}\nstdout: {out}\nstderr: {err}");
            assert!(err.contains("Error: matrix dimensions too large"), "`almide {leg}`:\n{program}\n{err}");
            assert!(!out.contains("parts="), "`almide {leg}` answered:\n{out}");
        }
    }
}

/// The control: the same rows at the ceiling's edge from below and a part
/// count past the width keep their documented answer on both legs.
#[test]
fn a_part_count_within_the_ceiling_keeps_its_answer_on_both_legs() {
    for (program, want) in [(split(3, 5, "7"), "parts=7"), (split(2, 4, "2"), "parts=2"), (split(0, 4, "9"), "parts=0")] {
        for (leg, o) in legs(&program) {
            let out = String::from_utf8_lossy(&o.stdout);
            assert!(o.status.success() && out.contains(want), "`almide {leg}`:\n{program}\n{out}{}", String::from_utf8_lossy(&o.stderr));
        }
    }
}
