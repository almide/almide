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
    format!("(module {extra} (memory (export \"memory\") 1) (func (export \"_start\") {body}))")
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

/// A module assembled by hand, for the encodings the text assembler never
/// writes: one `() -> ()` function `_start` with `body` as its code, one
/// exported memory, and `extra` sections spliced in before the code section.
fn raw(body: &[u8], exports: &[(&str, u8, u8)], extra: &[(u8, Vec<u8>)]) -> Vec<u8> {
    let mut entry = vec![0];
    entry.extend_from_slice(body);
    raw_entry(&entry, exports, extra)
}

/// As `raw`, with the code entry (its local declarations, then its code)
/// given whole.
fn raw_entry(entry: &[u8], exports: &[(&str, u8, u8)], extra: &[(u8, Vec<u8>)]) -> Vec<u8> {
    fn section(id: u8, payload: &[u8]) -> Vec<u8> {
        let mut s = vec![id, payload.len() as u8];
        s.extend_from_slice(payload);
        s
    }
    let mut m = b"\0asm\x01\0\0\0".to_vec();
    m.extend(section(1, &[1, 0x60, 0, 0]));
    m.extend(section(3, &[1, 0]));
    m.extend(section(5, &[1, 0, 1]));
    let mut ex = vec![exports.len() as u8];
    for (name, kind, index) in exports {
        ex.push(name.len() as u8);
        ex.extend_from_slice(name.as_bytes());
        ex.extend_from_slice(&[*kind, *index]);
    }
    m.extend(section(7, &ex));
    for (id, payload) in extra {
        m.extend(section(*id, payload));
    }
    let mut code = vec![1, entry.len() as u8];
    code.extend_from_slice(entry);
    m.extend(section(10, &code));
    m
}

const COMMAND: &[(&str, u8, u8)] = &[("_start", 0, 0), ("memory", 2, 0)];

#[test]
fn encodings_are_read_exactly() {
    assert!(decode(&raw(&[0x0B], COMMAND, &[])).is_ok(), "the hand-built baseline loads");
    // `block` whose type is spelled 0xC0 0x7F (-64 as a two-byte s33) is not
    // the one-byte 0x40 the binary format requires for an empty block type
    assert!(decode(&raw(&[0x02, 0xC0, 0x7F, 0x0B, 0x0B], COMMAND, &[])).is_err());
    assert!(decode(&raw(&[0x02, 0x40, 0x0B, 0x0B], COMMAND, &[])).is_ok());
    // a memory index of 0 in a two-byte LEB128 spelling is still memory 0
    assert!(decode(&raw(&[0x3F, 0x80, 0x00, 0x1A, 0x0B], COMMAND, &[])).is_ok());
}

#[test]
fn exports_are_checked() {
    let e = decode(&raw(&[0x0B], &[("_start", 0, 0), ("memory", 2, 0), ("f", 0, 7)], &[])).err();
    assert!(e.is_some_and(|e| e.reason.contains("does not exist")), "an export past the function space");
    let e = decode(&raw(&[0x0B], &[("_start", 0, 0), ("memory", 2, 0), ("_start", 0, 0)], &[])).err();
    assert!(e.is_some_and(|e| e.reason.contains("twice")), "a duplicate export name");
    let e = decode(&raw(&[0x0B], &[("_start", 0, 0)], &[])).err();
    assert!(e.is_some_and(|e| e.reason.contains("exports it as `memory`")), "a WASI command exports its memory");
}

#[test]
fn a_data_count_must_match_the_data() {
    let e = decode(&raw(&[0x0B], COMMAND, &[(12, vec![1])])).err();
    assert!(e.is_some_and(|e| e.reason.contains("data count")), "a count of 1 with no data section");
    assert!(decode(&raw(&[0x0B], COMMAND, &[(12, vec![0])])).is_ok());
}

#[test]
fn an_element_segment_needs_a_table() {
    let e = refusal(&module("(elem (i32.const 0))", ""));
    assert!(e.contains("needs a table") || e.contains("table"), "{e}");
}

#[test]
fn forged_sizes_are_refused_before_they_allocate() {
    let params = "i32 ".repeat(1001);
    let e = refusal(&module(&format!("(type (func (param {params})))"), ""));
    assert!(e.contains("params"), "{e}");
    let e = refusal(&module("(table 2000000 funcref)", ""));
    assert!(e.contains("table of more than"), "{e}");
    // two local runs: 1 × i32, then 50,000 × i64 (LEB 0xD0 0x86 0x03) — one
    // past the cap, refused without expanding either run
    let e = decode(&raw_entry(&[2, 1, 0x7F, 0xD0, 0x86, 0x03, 0x7E, 0x0B], COMMAND, &[])).err();
    assert!(e.is_some_and(|e| e.reason.contains("locals")));
    let fits = decode(&raw_entry(&[2, 1, 0x7F, 0xCF, 0x86, 0x03, 0x7E, 0x0B], COMMAND, &[]));
    assert!(fits.is_ok_and(|m| m.bodies[0].locals == 50_000), "exactly the cap loads, counted, not expanded");
}
