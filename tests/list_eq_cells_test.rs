//! The list `==` cells (#4006): `list.__eq_{int,float,bool,str}` in
//! stdlib/list.almd, each an `@intrinsic` over runtime/rs/src/list.rs and each
//! the generic the self-hosted `list.eq_*` body of its element type
//! specializes, so the certified native render serves a list `==` from the
//! declared floor.
//!
//! The family's completeness rule is the repr cells' (tests/list_repr_cells_test.rs):
//! one cell per lane-scalar element type (Int, Float, Bool) and String. A list
//! of String-bearing containers (`List[List[String]]`, `List[String?]`) has no
//! String-suffixed cell: its self-hosted body specializes the one generic cell
//! `list.__eq[A]`, so the element type is a type argument, not a `_str` twin
//! (#1460's ratchet, scripts/check-str-twin-ratchet.sh).

const CELLS: &[(&str, &str)] = &[
    // (cell, the self-hosted body that specializes it; its call name is `list.eq_<cell>`)
    ("int", "list_eq_int"),
    ("float", "list_eq_float"),
    ("bool", "list_eq_bool"),
    ("str", "list_eq_str"),
];

#[test]
fn every_cell_is_declared_specialized_and_on_the_native_floor() {
    let list = std::fs::read_to_string("stdlib/list.almd").unwrap();
    let rt = std::fs::read_to_string("runtime/rs/src/list.rs").unwrap();
    let core = std::fs::read_to_string("stdlib/value_core.almd").unwrap();
    for (cell, body) in CELLS {
        let symbol = format!("almide_rt_list_eq_{cell}");
        assert!(
            list.contains(&format!("@intrinsic(\"{symbol}\")\nfn __eq_{cell}(")),
            "list.__eq_{cell} is not declared over `{symbol}`"
        );
        assert!(rt.contains(&format!("pub fn {symbol}(")), "the runtime has no `{symbol}`");
        assert!(
            core.contains(&format!("@specializes(\"list.__eq_{cell}\")\nfn {body}(")),
            "`{body}` does not specialize list.__eq_{cell}"
        );
        for call in [format!("list.eq_{cell}"), format!("list.__eq_{cell}")] {
            assert_eq!(almide_mir::native_floor::refusal(&call), None, "`{call}` is not on the native floor");
        }
    }
}

/// The String-bearing nested bodies reach the native render through the one
/// generic cell, and no String-suffixed nested cell is declared.
#[test]
fn nested_string_bodies_specialize_the_generic_cell() {
    let list = std::fs::read_to_string("stdlib/list.almd").unwrap();
    let rt = std::fs::read_to_string("runtime/rs/src/list.rs").unwrap();
    let core = std::fs::read_to_string("stdlib/value_core.almd").unwrap();
    assert!(list.contains("@intrinsic(\"almide_rt_list_eq\")\nfn __eq[A](a: List[A], b: List[A]) -> Bool = _"));
    assert!(rt.contains("pub fn almide_rt_list_eq<A: PartialEq>(a: &[A], b: &[A]) -> bool"));
    for body in ["list_eq_list_str", "list_eq_opt_str"] {
        assert!(core.contains(&format!("@specializes(\"list.__eq\")\nfn {body}(")), "`{body}` does not specialize list.__eq");
    }
    for twin in ["fn __eq_list_str(", "fn __eq_opt_str("] {
        assert!(!list.contains(twin), "`{twin}` is a String-suffixed cell again");
    }
}

const PROGRAM: &str = r#"fn show(b: Bool) -> String = if b then "T" else "F"

fn main() -> Unit = {
  let nan = 0.0 / 0.0
  println(show([1, 2] == [1, 2]) + show([1, 2] == [1, 3]) + show([1] == [1, 2]))
  println(show([0.0] == [-0.0]) + show([nan] == [nan]) + show([1.5, 2.0] == [1.5, 2.0]))
  println(show([true] == [true]) + show([true] == [false]))
  println(show(["a", "b"] == ["a", "b"]) + show(["a"] == ["ab"]) + show(["ü"] == ["ü"]))
  println(show(list.__eq_int([1], [1])) + show(list.__eq_float([nan], [nan])) + show(list.__eq_bool([], [])) + show(list.__eq_str(["x"], ["y"])))
}
"#;

/// What v0, the certified native render and wasm print (measured on all three).
const EXPECTED: &str = "TFF\nTFT\nTF\nTFT\nTFTF\n";

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
    let dir = std::env::temp_dir().join(format!("list_eq_cells_{}", std::process::id()));
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
