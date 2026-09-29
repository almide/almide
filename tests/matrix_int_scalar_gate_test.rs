//! #2894 gate: `m * k` admits an `Int` scalar (the frontend's ScaleMatrix
//! dispatch), and every leg must scale by its value. Native handed the i64
//! to the f64 runtime scale and failed rustc, after `check` had passed. The
//! incumbent wasm leg, which the default route fell back to, handed it to
//! `matrix.scale` unconverted and scaled by its bit pattern read as a float.
//! Each cell runs natively, on the default wasm route and on the incumbent
//! leg, and must print the exact product.

use std::process::Command;

/// `ALMIDE_BIN` runs the cells against another build (A/B); the default is
/// this workspace's binary.
fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

#[test]
fn a_matrix_scaled_by_an_int_prints_the_product_on_every_leg() {
    // (the scaling expression over `a`, and the lists it must print)
    let cells = [
        ("2 * a", "[[2, 4], [6, 8]]"),
        ("a * 2", "[[2, 4], [6, 8]]"),
        ("a * k", "[[3, 6], [9, 12]]"),
        ("k * a", "[[3, 6], [9, 12]]"),
        ("a * (k - 4)", "[[-1, -2], [-3, -4]]"),
        // Float scalars, the control the Int cells are measured against.
        ("a * 0.5", "[[0.5, 1], [1.5, 2]]"),
        ("2.0 * a", "[[2, 4], [6, 8]]"),
    ];
    let mut failures = Vec::new();
    for (expr, want) in cells {
        let program = format!(
            "effect fn main() -> Unit = {{\n  let a = matrix.from_lists([[1.0, 2.0], [3.0, 4.0]])\n  let k = 3\n  let d = {expr}\n  println(\"${{matrix.to_lists(d)}}\")\n}}\n"
        );
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::write(root.path().join("t.almd"), &program).expect("write");
        let legs: [(&str, &[&str], bool); 3] = [
            ("native", &["run", "t.almd"], false),
            ("wasm", &["run", "t.almd", "--target", "wasm"], false),
            ("incumbent", &["run", "t.almd", "--target", "wasm"], true),
        ];
        for (leg, args, incumbent) in legs {
            let mut cmd = Command::new(almide());
            cmd.current_dir(root.path()).args(args);
            if incumbent {
                cmd.env("ALMIDE_WASM_INCUMBENT", "1");
            }
            let out = cmd.output().expect("run almide");
            let stdout = String::from_utf8_lossy(&out.stdout);
            if !out.status.success() || stdout != format!("{want}\n") {
                let err = String::from_utf8_lossy(&out.stderr);
                let first_error = err.lines().find(|l| l.starts_with("error")).unwrap_or("");
                failures.push(format!("`{expr}` on {leg}: {:?} {first_error}", stdout.chars().take(80).collect::<String>()));
            }
        }
    }
    assert!(failures.is_empty(), "{} cell(s) failed:\n{}", failures.len(), failures.join("\n"));
}
