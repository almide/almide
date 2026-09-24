//! A pad width past the wasm leg's i32 bound aborts rather than wrapping
//! (#2385).
//!
//! `string.pad_start`/`pad_end` narrowed their `Int` width with a bare
//! `i32_wrap_i64` in `Emitter::lower_string_pad`, so a width at or above 2^32
//! was taken MOD 2^32 and the leg ANSWERED with the remainder:
//!
//!     string.pad_start("ab", 4294967301, "x")   // 2^32 + 5
//!       native -> a string of 4294967301 characters
//!       wasm   -> "xxxab"
//!
//! No abort, no diagnostic, a different string. C-054 states that every
//! list/string op taking an `Int` count clamps on the FULL i64 BEFORE that
//! narrowing; this pair did not, and C-054's string fixture exercises both
//! functions but stops at width 5.
//!
//! WHY THIS IS A WASM-ONLY TEST AND NOT A CROSS-TARGET FIXTURE. The cell where
//! the wrap shows is one native can SATISFY: `2^32 + 5` is a four-gigabyte
//! string, which a machine with the room returns and a machine without it
//! aborts on. Running both legs at that width therefore measures the machine,
//! not the compiler. The wasm leg has no such freedom — its address space ends
//! at 4 GiB and the guard fires before any allocation — so the assertion is
//! deterministic on exactly one leg, and that is the leg the defect was on.
//! The both-legs-abort cell (`i64::MAX`) is pinned separately by
//! `spec/wasm_cross/string_pad_ceiling_oom.almd`, and the declared domain-edge
//! rows live in `proofs/domain-edges.toml`.

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

/// (exit code, stdout, stderr) from the wasm leg.
fn run_on_wasm(name: &str, src: &str) -> Option<(i32, String, String)> {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        eprintln!("skip: almide binary unavailable");
        return None;
    }
    let dir = std::env::temp_dir().join(format!("almide-issue2385-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let f = dir.join("m.almd");
    std::fs::write(&f, src).expect("write");
    let out = Command::new(almide_bin())
        .args(["run", "m.almd", "--target", "wasm"])
        .current_dir(&dir)
        .output()
        .expect("spawn almide run --target wasm");
    Some((
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
        String::from_utf8_lossy(&out.stderr).trim().to_string(),
    ))
}

fn program(width: &str, f: &str) -> String {
    format!(
        "fn main() -> Unit = {{\n  let r: String = string.{f}(\"ab\", {width}, \"x\")\n  \
         println(\"len=${{string.len(r)}}\")\n}}\n"
    )
}

#[test]
fn string_pad_width_past_the_i32_bound_aborts() {
    for f in ["pad_start", "pad_end"] {
        // 2^32 + 5. The wrap made this five.
        let Some((code, out, err)) = run_on_wasm(&format!("wrap-{f}"), &program("4294967301", f))
        else {
            return;
        };
        assert_ne!(
            out, "len=5",
            "{f} answered with the width taken mod 2^32 — the narrowing is unguarded again"
        );
        assert_eq!(code, 1, "{f} did not take C-197's abort: stdout={out:?} stderr={err:?}");
        assert!(
            err.contains("Error: out of memory"),
            "{f} aborted, but not on C-197's defined line: {err:?}"
        );
    }
}

/// 2^32 exactly wrapped to ZERO, which sized the block at nothing while the
/// tail still copied the source into it — the returned string reported length
/// 0 and had lost its own input. A separate cell from the one above because
/// the wrapped width lands BELOW the char count rather than above it, which is
/// the other side of this arm's `if`.
#[test]
fn a_width_that_wraps_below_the_char_count_aborts_too() {
    let Some((code, out, err)) = run_on_wasm("wrap-zero", &program("4294967296", "pad_start"))
    else {
        return;
    };
    assert_ne!(out, "len=0", "the width wrapped to zero and the result lost its input");
    assert_eq!(code, 1, "no abort: stdout={out:?} stderr={err:?}");
    assert!(err.contains("Error: out of memory"), "not the defined line: {err:?}");
}

/// The guard must not cost the ordinary case. A width the leg can serve is
/// still served, and one below the char count still returns the string
/// unchanged — the branch the guard was added next to.
#[test]
fn widths_the_leg_can_serve_are_unchanged() {
    let Some((code, out, err)) = run_on_wasm("in-range", &program("5", "pad_start")) else {
        return;
    };
    assert_eq!((code, out.as_str()), (0, "len=5"), "stderr={err:?}");

    let Some((code, out, err)) = run_on_wasm("narrower", &program("1", "pad_start")) else {
        return;
    };
    assert_eq!((code, out.as_str()), (0, "len=2"), "stderr={err:?}");
}
