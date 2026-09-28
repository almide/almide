//! #2893: the Bytes index syntax is C-067's CHECKED form on every leg — an
//! out-of-range read or write, at an index >= len or a negative one, aborts
//! with `Error: index out of bounds` and exit 1. The incumbent wasm leg lowered
//! the WRITE as C-229's total `bytes.set_at`, so it ran on as if the write had
//! not happened (exit 0). The cross-target harness votes the structural leg
//! only, so the forced incumbent (`ALMIDE_WASM_INCUMBENT=1`) is run here
//! against native and the default wasm route, and every leg is held to the
//! actual answer as well as to agreement — a shared wrong answer cannot pass.
use std::process::Command;

const ABORT: &str = "Error: index out of bounds";

const CASES: &[(&str, &str)] = &[
    ("spec/wasm_cross/bytes_index_bounds_write_past_end.almd", "[200, 7, 0]\n"),
    ("spec/wasm_cross/bytes_index_bounds_write_negative.almd", "[7, 9]\n"),
    ("spec/wasm_cross/bytes_index_bounds_read_past_end.almd", "3\n"),
    ("spec/wasm_cross/bytes_index_bounds_read_negative.almd", "1\n"),
];

#[test]
fn every_out_of_range_bytes_index_aborts_on_every_leg() {
    let bin =
        std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string());
    for (fixture, expected) in CASES {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(fixture);
        for leg in ["native", "wasm-default", "incumbent"] {
            let mut cmd = Command::new(&bin);
            cmd.arg("run").arg(&fixture).env_remove("ALMIDE_WASM_INCUMBENT");
            if leg != "native" {
                cmd.args(["--target", "wasm"]);
            }
            if leg == "incumbent" {
                cmd.env("ALMIDE_WASM_INCUMBENT", "1");
            }
            let out = cmd.output().unwrap();
            let stderr = String::from_utf8(out.stderr).unwrap();
            // The forced-leg notice is compiler routing information.
            let stderr = stderr
                .lines()
                .filter(|line| !line.starts_with("[almide] ALMIDE_WASM_INCUMBENT is set:"))
                .collect::<Vec<_>>()
                .join("\n");
            let context = format!("{}, {leg}: {stderr}", fixture.display());
            assert_eq!(out.status.code(), Some(1), "{context}");
            assert_eq!(stderr, ABORT, "{context}");
            assert_eq!(String::from_utf8(out.stdout).unwrap(), *expected, "{context}");
        }
    }
}
