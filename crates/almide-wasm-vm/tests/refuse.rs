//! REQ-VM-2/3: everything outside the closed set is refused at load, before
//! any instruction runs — with the offset of the byte that decided it.

use almide_wasm_vm::decode;

fn refusal(wat: &str) -> String {
    let bytes = wat::parse_str(wat).expect("test module assembles");
    match decode(&bytes) {
        Ok(_) => panic!("accepted, but must be refused:\n{wat}"),
        Err(e) => e.to_string(),
    }
}

/// A minimal acceptable module with `body` as `_start`'s code and `extra`
/// module fields.
fn module(extra: &str, body: &str) -> String {
    format!("(module {extra} (memory 1) (func (export \"_start\") {body}))")
}

#[test]
fn the_minimal_module_loads() {
    let bytes = wat::parse_str(module("", "")).expect("assembles");
    assert!(decode(&bytes).is_ok());
}

#[test]
fn instructions_outside_the_set_are_refused() {
    let cases = [
        ("(drop (f32.add (f32.const 1) (f32.const 2)))", "f32.const"),
        ("(drop (i32.load16_s (i32.const 0)))", "i32.load16_s"),
        ("(drop (i32.div_s (i32.const 1) (i32.const 1)))", "i32.div_s"),
        ("(drop (i32.popcnt (i32.const 1)))", "i32.popcnt"),
        ("(drop (i32.trunc_sat_f64_u (f64.const 1)))", "i32.trunc_sat_f64_u"),
        ("(nop)", "nop"),
        ("(block (br_table 0 0 (i32.const 0)))", "br_table"),
    ];
    for (body, what) in cases {
        let e = refusal(&module("", body));
        assert!(e.contains("outside the instruction set"), "{what}: {e}");
    }
}

#[test]
fn features_outside_the_set_are_refused() {
    let cases = [
        ("(func $f (param f32))", "a type is outside i32/i64/f64"),
        ("(func $f (result i32 i32) (i32.const 0) (i32.const 0))", "multi-value"),
        ("(func $f (local f32))", "a local's type is outside"),
        ("(global f32 (f32.const 0))", "a global's type is outside"),
        ("(start 0)", "section 8 is not accepted"),
        ("(table 2 1 funcref)", "maximum is below its minimum"),
        ("(import \"wasi_snapshot_preview1\" \"path_open\" (func))", "not served by this VM"),
        ("(import \"env\" \"fd_write\" (func (param i32 i32 i32 i32) (result i32)))", "not served by this VM"),
        ("(import \"wasi_snapshot_preview1\" \"fd_write\" (func (param i32) (result i32)))", "WASI signature"),
        ("(data \"passive\")", "only active data segments"),
        ("(table 1 1 funcref) (elem func 0)", "element segments"),
    ];
    for (extra, why) in cases {
        let e = refusal(&module(extra, ""));
        assert!(e.contains(why), "{extra}: {e}");
    }
    let e = refusal("(module (memory 1) (memory 1) (func (export \"_start\")))");
    assert!(e.contains("exactly one memory") || e.contains("multi"), "{e}");
    let e = refusal("(module (memory 1 2 shared) (func (export \"_start\")))");
    assert!(e.contains("shared"), "{e}");
}

#[test]
fn block_types_outside_the_set_are_refused() {
    let e = refusal(&module("", "(drop (block (result f32) (f32.const 0)))"));
    assert!(e.contains("block type"), "{e}");
    let e = refusal(&module("(type $two (func (result i32 i32)))", "(block (type $two) (i32.const 0) (i32.const 0)) (drop) (drop)"));
    assert!(e.contains("multi-value"), "{e}");
}

#[test]
fn ill_typed_bodies_are_refused() {
    let cases = [
        "(drop (i32.add (i32.const 1) (i64.const 1)))",
        "(i32.const 1)",
        "(drop)",
        "(br 1)",
        "(local.get 0)",
        "(global.set 0 (i32.const 1))",
    ];
    for body in cases {
        let bytes = match wat::parse_str(module("(global i32 (i32.const 0))", body)) {
            Ok(b) => b,
            Err(_) => continue, // the text assembler caught it first
        };
        assert!(decode(&bytes).is_err(), "accepted an ill-typed body: {body}");
    }
}

#[test]
fn the_entry_point_is_required_and_takes_nothing() {
    let e = refusal("(module (func))");
    assert!(e.contains("_start"), "{e}");
    let e = refusal("(module (func (export \"_start\") (param i32)))");
    assert!(e.contains("_start"), "{e}");
}

#[test]
fn a_refusal_names_its_byte() {
    let bytes = wat::parse_str(module("", "(nop)")).expect("assembles");
    let Err(e) = decode(&bytes) else { panic!("nop must be refused") };
    let at = e.offset.expect("a body refusal has an offset");
    assert_eq!(bytes[at - 1], 0x01, "the offset points just past the refused opcode");
}

#[test]
fn truncated_and_garbage_input_is_refused_without_panicking() {
    let bytes =
        wat::parse_str(module("(global (mut i64) (i64.const 7))", "(global.set 0 (i64.const 1))")).expect("assembles");
    for n in 0..bytes.len() {
        let _ = decode(&bytes[..n]);
    }
    for i in 8..bytes.len() {
        let mut b = bytes.clone();
        b[i] ^= 0xff;
        let _ = decode(&b);
    }
}
