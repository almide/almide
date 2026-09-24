//! #2465: an alias bound BEFORE an in-place clear keeps the pre-clear value on
//! the INCUMBENT wasm leg too. The cross-target harness votes the structural
//! leg only, so the forced incumbent (`ALMIDE_WASM_INCUMBENT=1`) is run here
//! against native and the default wasm route, and every leg is held to the
//! actual answer as well as agreement — a shared wrong answer cannot pass.
//! The field fixture is served by the incumbent on the default route as well
//! (the structural leg refuses `string-clear-nonvar`), so for it the two wasm
//! legs are the same brick reached two ways.
use std::process::Command;

const CASES: &[(&str, &str)] = &[
    (
        "spec/wasm_cross/inplace_clear_alias.almd",
        "A.string=[] [hello]\n\
         A.bytes=0 3 3\n\
         A.list=0 3\n\
         A.map=0 1\n\
         B=[] [again] 0 2 0 2 0 1\n",
    ),
    (
        "spec/wasm_cross/inplace_clear_alias_field.almd",
        "C.r=[] 0 0 0 r2=[hi] 2 3 1\n\
         D.r=[] 0 0 0 r3=[x] 1 1 1\n",
    ),
];

#[test]
fn every_clear_alias_shape_agrees_on_every_leg() {
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
            assert!(out.status.success(), "{context}");
            assert_eq!(stderr, "", "{context}");
            assert_eq!(String::from_utf8(out.stdout).unwrap(), *expected, "{context}");
        }
    }
}
