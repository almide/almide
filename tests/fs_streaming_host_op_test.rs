//! `fold_lines` / `for_each_line` walk real lines on BOTH legs (#2090).
//!
//! #2090 gave these two their own host ops (51/52) so the wasm leg could name
//! `fs.fold_lines` where it used to borrow `fs.read_lines`' op. Routing them to
//! `fs_dispatch_r2` was right; letting them fall through to its `_` arm was not.
//! That arm is the raw-BYTES reader, so the guest — which decodes u32-LE
//! length-prefixed FRAMES — walked nothing:
//!
//! ```text
//! fs.fold_lines(csv, 0, (acc, line) => acc + parse_row(line))  =>  0   (want 6)
//! ```
//!
//! No error, no wall, no failing assertion: an empty walk returns `init`, which
//! is a perfectly good Int. `almide test` stayed 440/440 green.
//!
//! What DID catch it was a `docs/stdlib/fs.md` run fence, and the reason the
//! suite missed it is worth pinning: `spec/stdlib/fs_streaming_test.almd` asserts
//! the same sum, but with an INLINE lambda, and that shape does not take the
//! structural op path. The doc fence used a NAMED function. So this test uses a
//! named function deliberately — the callback shape is the routing input.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

const PROG: &str = "import fs\n\
     fn parse_row(line: String) -> Int = int.parse(line) ?? 0\n\
     fn width(line: String) -> Int = string.len(line)\n\
     effect fn main() -> Unit = {\n\
    \x20 let dir = fs.create_temp_dir(\"fs-stream-\")!\n\
    \x20 fs.write(\"${dir}/d.csv\", \"1\\n2\\n3\\n\")!\n\
    \x20 println(int.to_string(fs.fold_lines(\"${dir}/d.csv\", 0, (a, l) => a + parse_row(l))!))\n\
    \x20 println(int.to_string(fs.fold_lines(\"${dir}/d.csv\", 0, (a, l) => a + width(l))!))\n\
    \x20 fs.remove_all(dir)!\n\
     }\n";

fn run(dir: &std::path::Path, target: Option<&str>) -> String {
    let file = dir.join("stream.almd");
    std::fs::write(&file, PROG).expect("write fixture");
    let mut cmd = Command::new(almide());
    cmd.arg("run").arg(&file);
    if let Some(t) = target {
        cmd.args(["--target", t]);
    }
    let out = cmd.output().expect("run almide");
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// 1+2+3 = 6 and the three one-character lines total 3. An empty walk answers
/// `init` (0) on both counts, which is exactly the shape that hid the bug.
#[test]
fn a_named_callback_folds_every_line_on_both_legs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let native = run(dir.path(), None);
    assert!(
        native.contains("6") && native.contains("3"),
        "native fold_lines did not walk the lines:\n{native}"
    );

    let wasm = run(dir.path(), Some("wasm"));
    assert_eq!(
        native.trim(),
        wasm.trim(),
        "fold_lines diverged between the legs — an empty walk returns `init`, so \
         this reads as a plausible number rather than a failure"
    );
}
