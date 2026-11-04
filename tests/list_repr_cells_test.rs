//! The `${xs}` repr cells (#4006): `list.__to_string_{int,float,bool,str}` in
//! stdlib/list.almd, each an `@intrinsic` over runtime/rs/src/list.rs and each
//! the generic the self-hosted `list.to_string*` body of its element type
//! specializes, so the certified native render serves an interpolated list
//! from the declared floor.
//!
//! The family's completeness rule: one cell per lane-scalar element type (Int,
//! Float, Bool) and String — the element types whose list is a flat native
//! `Vec`. A list of any other element (a nested list, an Option, a tuple)
//! renders through its own self-hosted body and has no cell, on purpose.

const CELLS: &[(&str, &str, &str)] = &[
    // (cell, the self-hosted body that specializes it, its call name)
    ("int", "list_to_string", "list.to_string"),
    ("float", "list_to_string_f", "list.to_string_f"),
    ("bool", "list_to_string_b", "list.to_string_b"),
    ("str", "list_to_string_s", "list.to_string_s"),
];

#[test]
fn every_cell_is_declared_specialized_and_on_the_native_floor() {
    let list = std::fs::read_to_string("stdlib/list.almd").unwrap();
    let rt = std::fs::read_to_string("runtime/rs/src/list.rs").unwrap();
    for (cell, body, call) in CELLS {
        let symbol = format!("almide_rt_list_to_string_{cell}");
        assert!(
            list.contains(&format!("@intrinsic(\"{symbol}\")\nfn __to_string_{cell}(")),
            "list.__to_string_{cell} is not declared over `{symbol}`"
        );
        assert!(rt.contains(&format!("pub fn {symbol}(")), "the runtime has no `{symbol}`");
        let src = std::fs::read_dir("stdlib")
            .unwrap()
            .filter_map(|e| std::fs::read_to_string(e.ok()?.path()).ok())
            .find(|s| s.contains(&format!("\nfn {body}(")))
            .unwrap_or_else(|| panic!("no self-hosted `{body}`"));
        assert!(
            src.contains(&format!("@specializes(\"list.__to_string_{cell}\")\nfn {body}(")),
            "`{body}` does not specialize list.__to_string_{cell}"
        );
        assert_eq!(almide_mir::native_floor::refusal(call), None, "`{call}` is not on the native floor");
        assert_eq!(
            almide_mir::native_floor::refusal(&format!("list.__to_string_{cell}")),
            None,
            "`list.__to_string_{cell}` is not on the native floor"
        );
    }
}

const PROGRAM: &str = r#"fn main() -> Unit = {
  let a = [1, -2, 9223372036854775807]
  let f = [1.0, 2.5, -0.0, 0.1, 100.0]
  let b = [true, false]
  let s = ["a", "q\"t", "back\\slash", "nl\nx", "tab\tx", "cr\rx", "ü"]
  let e: List[Int] = []
  println("${a} ${f} ${b} ${s} ${e}")
  println(list.__to_string_int(a))
  println(list.__to_string_float(f))
  println(list.__to_string_bool(b))
  println(list.__to_string_str(s))
  println(list.__to_string_int(e))
}
"#;

/// What v0, the certified native render and wasm print (measured on all three).
const EXPECTED: &str = "[1, -2, 9223372036854775807] [1, 2.5, -0, 0.1, 100] [true, false] [\"a\", \"q\\\"t\", \"back\\\\slash\", \"nl\\nx\", \"tab\\tx\", \"cr\\rx\", \"ü\"] []\n\
[1, -2, 9223372036854775807]\n\
[1, 2.5, -0, 0.1, 100]\n\
[true, false]\n\
[\"a\", \"q\\\"t\", \"back\\\\slash\", \"nl\\nx\", \"tab\\tx\", \"cr\\rx\", \"ü\"]\n\
[]\n";

#[test]
fn the_interp_oracle_answers_every_cell_as_the_backends_do() {
    let ir = almide::wasm_leg::lower_to_ir("spec/wasm_cross/probe.almd", PROGRAM).expect("front failed");
    let out = almide_interp::Interpreter::new(&ir).run_main();
    assert!(matches!(out.status, almide_interp::RunStatus::Ok), "the interp did not answer: {:?}", out.status);
    assert_eq!(out.stdout, EXPECTED);
}

#[test]
fn both_backends_print_every_cell_identically() {
    let bin = env!("CARGO_BIN_EXE_almide");
    let dir = std::env::temp_dir().join(format!("list_repr_cells_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("cells.almd");
    std::fs::write(&src, PROGRAM).unwrap();
    let native = std::process::Command::new(bin).arg("run").arg(&src).output().unwrap();
    assert!(native.status.success(), "native: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout), EXPECTED, "native");
    let wasm = std::process::Command::new(bin).args(["run", "--target", "wasm"]).arg(&src).output().unwrap();
    if String::from_utf8_lossy(&wasm.stderr).contains("wasmtime") && !wasm.status.success() {
        return; // no wasm runner on this host
    }
    assert!(wasm.status.success(), "wasm: {}", String::from_utf8_lossy(&wasm.stderr));
    assert_eq!(String::from_utf8_lossy(&wasm.stdout), EXPECTED, "wasm");
}
