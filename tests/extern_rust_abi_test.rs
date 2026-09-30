//! The `@extern(rust, …)` parameter ABI, gated as a matrix (#3045).
//!
//! A `Bytes` param used to fail rustc E0308: the call site passed
//! `&AlmideRcCow<Vec<u8>>` (borrow inference gave the hole body `Ref`) while
//! the wrapper took the value. The ABI is now one rule —
//! `extern_rust_borrow_mode` — read by the call sites and by the wrapper, and
//! this test holds every parameter type to it: ONE program declares an extern
//! per row of [`ROWS`], a `native/host.rs` implements each against the host
//! signature the docs promise (`docs/specs/module-system.md` §11.1), and the
//! program is built and run. A row that drifts fails to compile or prints the
//! wrong line.
//!
//! [`covering_rows`] is an exhaustive `match` over `Ty` with no wildcard: a
//! new type variant does not compile here until someone decides which row
//! covers it (or says why it is not an extern parameter type).

use std::path::Path;
use std::process::Command;

use almide::types::{Ty, TypeConstructorId};

struct Row {
    /// Row id: the extern fn is `f_<id>`, the host fn `host::f_<id>`.
    id: &'static str,
    /// The Almide parameter list (without parens) and return type.
    almide_params: &'static str,
    almide_ret: &'static str,
    /// The host fn's Rust parameter list and body — the promised ABI.
    host_params: &'static str,
    host_ret: &'static str,
    host_body: &'static str,
    /// Almide statements run in `main`; they print the expected line.
    call: &'static str,
    expected: &'static str,
}

const ROWS: &[Row] = &[
    Row { id: "int", almide_params: "v: Int", almide_ret: "Int", host_params: "v: i64", host_ret: "i64", host_body: "v + 1",
          call: "println(int.to_string(f_int(41)))", expected: "42" },
    Row { id: "int8", almide_params: "v: Int8", almide_ret: "Int", host_params: "v: i8", host_ret: "i64", host_body: "v as i64",
          call: "let a_int8: Int8 = -3\n  println(int.to_string(f_int8(a_int8)))", expected: "-3" },
    Row { id: "int16", almide_params: "v: Int16", almide_ret: "Int", host_params: "v: i16", host_ret: "i64", host_body: "v as i64",
          call: "let a_int16: Int16 = 300\n  println(int.to_string(f_int16(a_int16)))", expected: "300" },
    Row { id: "int32", almide_params: "v: Int32", almide_ret: "Int", host_params: "v: i32", host_ret: "i64", host_body: "v as i64",
          call: "let a_int32: Int32 = 70000\n  println(int.to_string(f_int32(a_int32)))", expected: "70000" },
    Row { id: "int64", almide_params: "v: Int64", almide_ret: "Int", host_params: "v: i64", host_ret: "i64", host_body: "v",
          call: "let a_int64: Int64 = 5\n  println(int.to_string(f_int64(a_int64)))", expected: "5" },
    Row { id: "uint8", almide_params: "v: UInt8", almide_ret: "Int", host_params: "v: u8", host_ret: "i64", host_body: "v as i64",
          call: "let a_uint8: UInt8 = 200\n  println(int.to_string(f_uint8(a_uint8)))", expected: "200" },
    Row { id: "uint16", almide_params: "v: UInt16", almide_ret: "Int", host_params: "v: u16", host_ret: "i64", host_body: "v as i64",
          call: "let a_uint16: UInt16 = 60000\n  println(int.to_string(f_uint16(a_uint16)))", expected: "60000" },
    Row { id: "uint32", almide_params: "v: UInt32", almide_ret: "Int", host_params: "v: u32", host_ret: "i64", host_body: "v as i64",
          call: "let a_uint32: UInt32 = 7\n  println(int.to_string(f_uint32(a_uint32)))", expected: "7" },
    Row { id: "uint64", almide_params: "v: UInt64", almide_ret: "Int", host_params: "v: u64", host_ret: "i64", host_body: "v as i64",
          call: "let a_uint64: UInt64 = 8\n  println(int.to_string(f_uint64(a_uint64)))", expected: "8" },
    Row { id: "float", almide_params: "v: Float", almide_ret: "Float", host_params: "v: f64", host_ret: "f64", host_body: "v * 2.0",
          call: "println(float.to_string(f_float(1.25)))", expected: "2.5" },
    Row { id: "float32", almide_params: "v: Float32", almide_ret: "Int", host_params: "v: f32", host_ret: "i64", host_body: "(v * 2.0) as i64",
          call: "let a_f32: Float32 = 1.5\n  println(int.to_string(f_float32(a_f32)))", expected: "3" },
    Row { id: "float64", almide_params: "v: Float64", almide_ret: "Int", host_params: "v: f64", host_ret: "i64", host_body: "v as i64",
          call: "let a_f64: Float64 = 9.0\n  println(int.to_string(f_float64(a_f64)))", expected: "9" },
    Row { id: "bool", almide_params: "v: Bool", almide_ret: "Bool", host_params: "v: bool", host_ret: "bool", host_body: "!v",
          call: "println(\"${f_bool(false)}\")", expected: "true" },
    Row { id: "unit", almide_params: "v: Unit", almide_ret: "Int", host_params: "_v: ()", host_ret: "i64", host_body: "11",
          call: "println(int.to_string(f_unit(())))", expected: "11" },
    Row { id: "string", almide_params: "v: String", almide_ret: "Int", host_params: "v: &str", host_ret: "i64", host_body: "v.len() as i64",
          call: "let a_s = \"hello\"\n  println(int.to_string(f_string(a_s)))\n  println(int.to_string(f_string(\"ab\" + a_s)))", expected: "5\n7" },
    // The #3045 row: a let-bound value and a temporary, both borrowed as a slice.
    Row { id: "bytes", almide_params: "v: Bytes", almide_ret: "Int", host_params: "v: &[u8]", host_ret: "i64", host_body: "v.iter().map(|b| *b as i64).sum()",
          call: "let a_b = bytes.from_string(\"AB\")\n  println(int.to_string(f_bytes(a_b)))\n  println(int.to_string(f_bytes(bytes.from_string(\"C\"))))\n  println(int.to_string(bytes.len(a_b)))",
          expected: "131\n67\n2" },
    Row { id: "matrix", almide_params: "v: Matrix", almide_ret: "Int", host_params: "v: &AlmideMatrix", host_ret: "i64", host_body: "(v.rows * 10 + v.cols) as i64",
          call: "let a_m = matrix.zeros(2, 3)\n  println(int.to_string(f_matrix(a_m)))\n  println(int.to_string(f_matrix(matrix.zeros(4, 1))))\n  println(int.to_string(matrix.rows(a_m)))",
          expected: "23\n41\n2" },
    Row { id: "list", almide_params: "v: List[Int]", almide_ret: "Int", host_params: "v: &[i64]", host_ret: "i64", host_body: "v.iter().sum()",
          call: "let a_l = [1, 2, 3]\n  println(int.to_string(f_list(a_l)))\n  println(int.to_string(list.len(a_l)))", expected: "6\n3" },
    Row { id: "list_str", almide_params: "v: List[String]", almide_ret: "Int", host_params: "v: &[String]", host_ret: "i64", host_body: "v.iter().map(|s| s.len() as i64).sum()",
          call: "println(int.to_string(f_list_str([\"ab\", \"c\"])))", expected: "3" },
    Row { id: "map", almide_params: "v: Map[String, Int]", almide_ret: "Int", host_params: "v: &AlmideMap<String, i64>", host_ret: "i64", host_body: "v.len() as i64",
          call: "let a_map = [\"a\": 1, \"b\": 2]\n  println(int.to_string(f_map(a_map)))\n  println(int.to_string(map.len(a_map)))", expected: "2\n2" },
    Row { id: "set", almide_params: "v: Set[Int]", almide_ret: "Int", host_params: "v: &AlmideSet<i64>", host_ret: "i64", host_body: "v.len() as i64",
          call: "println(int.to_string(f_set(set.from_list([1, 2, 2, 3]))))", expected: "3" },
    Row { id: "record", almide_params: "v: Pt", almide_ret: "Int", host_params: "v: &crate::Pt", host_ret: "i64", host_body: "v.x * 10 + v.y",
          call: "let a_p = Pt { x: 1, y: 2 }\n  println(int.to_string(f_record(a_p)))\n  println(int.to_string(a_p.x))", expected: "12\n1" },
    Row { id: "variant", almide_params: "v: Sh", almide_ret: "Int", host_params: "v: crate::Sh", host_ret: "i64",
          host_body: "match v { crate::Sh::Circle(r) => r, crate::Sh::Sq(s) => s * s }",
          call: "println(int.to_string(f_variant(Sq(3))))", expected: "9" },
    Row { id: "option", almide_params: "v: Option[String]", almide_ret: "Int", host_params: "v: Option<String>", host_ret: "i64", host_body: "v.map_or(-1, |s| s.len() as i64)",
          call: "println(int.to_string(f_option(some(\"abc\"))))\n  println(int.to_string(f_option(none)))", expected: "3\n-1" },
    Row { id: "result", almide_params: "v: Result[Int, String]", almide_ret: "Int", host_params: "v: Result<i64, String>", host_ret: "i64", host_body: "v.unwrap_or(-1)",
          call: "println(int.to_string(f_result(ok(4))))\n  println(int.to_string(f_result(err(\"no\"))))", expected: "4\n-1" },
    Row { id: "tuple", almide_params: "v: (Int, String)", almide_ret: "Int", host_params: "v: (i64, String)", host_ret: "i64", host_body: "v.0 + v.1.len() as i64",
          call: "println(int.to_string(f_tuple((10, \"abc\"))))", expected: "13" },
    Row { id: "fn", almide_params: "v: (Int) -> Int", almide_ret: "Int", host_params: "v: std::rc::Rc<dyn Fn(i64) -> i64>", host_ret: "i64", host_body: "v(20)",
          call: "let a_k = 2\n  println(int.to_string(f_fn((x) => x + a_k)))", expected: "22" },
    Row { id: "mut_list", almide_params: "mut v: List[Int]", almide_ret: "Unit", host_params: "v: &mut Vec<i64>", host_ret: "()", host_body: "v.push(9)",
          call: "var a_ml = [1]\n  f_mut_list(a_ml)\n  println(int.to_string(list.len(a_ml)))", expected: "2" },
    Row { id: "mut_bytes", almide_params: "mut v: Bytes", almide_ret: "Unit", host_params: "v: &mut Vec<u8>", host_ret: "()", host_body: "v.push(b'!')",
          call: "var a_mb = bytes.from_string(\"x\")\n  let a_mb0 = a_mb\n  f_mut_bytes(a_mb)\n  println(int.to_string(bytes.len(a_mb)))\n  println(int.to_string(bytes.len(a_mb0)))",
          expected: "2\n1" },
    Row { id: "two", almide_params: "a: Bytes, b: String, c: Int", almide_ret: "Int", host_params: "a: &[u8], b: &str, c: i64", host_ret: "i64", host_body: "a.len() as i64 + b.len() as i64 + c",
          call: "println(int.to_string(f_two(bytes.from_string(\"ab\"), \"cde\", 100)))", expected: "105" },
    // Returns: a raw `Vec<u8>` / `AlmideMatrix` is accepted (`.into()`).
    Row { id: "ret_bytes", almide_params: "n: Int", almide_ret: "Bytes", host_params: "n: i64", host_ret: "Vec<u8>", host_body: "vec![7u8; n as usize]",
          call: "let a_rb = f_ret_bytes(3)\n  println(int.to_string(bytes.len(a_rb)))", expected: "3" },
    Row { id: "ret_matrix", almide_params: "n: Int", almide_ret: "Matrix", host_params: "n: i64", host_ret: "AlmideMatrix", host_body: "AlmideMatrix { rows: n as usize, cols: 1, data: vec![0.0; n as usize] }",
          call: "println(int.to_string(matrix.rows(f_ret_matrix(5))))", expected: "5" },
];

/// Which row(s) exercise each `Ty` variant as an extern parameter — or why it
/// cannot be one. Exhaustive on purpose (no `_`): a new variant is a compile
/// error here until the ABI has a decision for it.
fn covering_rows(ty: &Ty) -> Result<&'static [&'static str], &'static str> {
    match ty {
        Ty::Int => Ok(&["int"]),
        Ty::Int8 => Ok(&["int8"]),
        Ty::Int16 => Ok(&["int16"]),
        Ty::Int32 => Ok(&["int32"]),
        Ty::Int64 => Ok(&["int64"]),
        Ty::UInt8 => Ok(&["uint8"]),
        Ty::UInt16 => Ok(&["uint16"]),
        Ty::UInt32 => Ok(&["uint32"]),
        Ty::UInt64 => Ok(&["uint64"]),
        Ty::Float => Ok(&["float"]),
        Ty::Float32 => Ok(&["float32"]),
        Ty::Float64 => Ok(&["float64"]),
        Ty::Bool => Ok(&["bool"]),
        Ty::Unit => Ok(&["unit"]),
        Ty::String => Ok(&["string"]),
        Ty::Bytes => Ok(&["bytes", "mut_bytes", "two", "ret_bytes"]),
        Ty::Matrix => Ok(&["matrix", "ret_matrix"]),
        Ty::Applied(id, _) => match id {
            TypeConstructorId::List => Ok(&["list", "list_str", "mut_list"]),
            TypeConstructorId::Map => Ok(&["map"]),
            TypeConstructorId::Set => Ok(&["set"]),
            TypeConstructorId::Option => Ok(&["option"]),
            TypeConstructorId::Result => Ok(&["result"]),
            TypeConstructorId::Tuple => Ok(&["tuple"]),
            TypeConstructorId::Matrix => Ok(&["matrix"]),
            TypeConstructorId::Int | TypeConstructorId::Float | TypeConstructorId::String
            | TypeConstructorId::Bool | TypeConstructorId::Unit | TypeConstructorId::Bytes =>
                Err("nullary constructor ids resolve to the plain Ty variants above"),
            TypeConstructorId::UserDefined(_) => Ok(&["record", "variant"]),
        },
        Ty::Tuple(_) => Ok(&["tuple"]),
        Ty::Named(..) => Ok(&["record", "variant"]),
        Ty::Record { .. } => Ok(&["record"]),
        Ty::Variant { .. } => Ok(&["variant"]),
        Ty::Fn { .. } => Ok(&["fn"]),
        Ty::RawPtr => Err("`@extern(c)` only — a C pointer, rendered by render_extern_c"),
        Ty::OpenRecord { .. } => Err("an open-record param is destructured into fields before codegen"),
        Ty::Union(_) => Err("inline unions lower to a generated enum — a Named type (the variant row)"),
        Ty::TypeVar(_) => Err("generic externs are not analysed; the param stays owned as written"),
        Ty::Never | Ty::Unknown => Err("not a value type"),
        Ty::ConstParam { .. } | Ty::ConstValue { .. } => Err("a const generic, not a parameter's type"),
    }
}

fn every_ty_variant() -> Vec<Ty> {
    let s = almide::intern::sym("T");
    vec![
        Ty::Int, Ty::Int8, Ty::Int16, Ty::Int32, Ty::Int64, Ty::UInt8, Ty::UInt16, Ty::UInt32, Ty::UInt64,
        Ty::Float, Ty::Float32, Ty::Float64, Ty::Bool, Ty::Unit, Ty::String, Ty::Bytes, Ty::Matrix,
        Ty::Applied(TypeConstructorId::List, vec![Ty::Int]),
        Ty::Applied(TypeConstructorId::Map, vec![Ty::String, Ty::Int]),
        Ty::Applied(TypeConstructorId::Set, vec![Ty::Int]),
        Ty::Applied(TypeConstructorId::Option, vec![Ty::Int]),
        Ty::Applied(TypeConstructorId::Result, vec![Ty::Int, Ty::String]),
        Ty::Applied(TypeConstructorId::Matrix, vec![Ty::Float]),
        Ty::Tuple(vec![Ty::Int, Ty::String]),
        Ty::Named(s, vec![]),
        Ty::Record { fields: vec![] },
        Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Int), is_effect: false },
    ]
}

#[test]
fn every_parameter_type_names_rows_that_exist() {
    for ty in every_ty_variant() {
        let rows = covering_rows(&ty).unwrap_or_else(|why| panic!("{ty:?} is a parameter type: {why}"));
        for id in rows {
            assert!(ROWS.iter().any(|r| r.id == *id), "{ty:?} names row `{id}`, which is not in ROWS");
        }
    }
}

fn program(rows: &[Row]) -> (String, String) {
    let mut almd = String::from(
        "import bytes\n\ntype Pt = { x: Int, y: Int }\ntype Sh = | Circle(Int) | Sq(Int)\n\n",
    );
    let mut host = String::from("#![allow(dead_code, unused_imports)]\nuse crate::*;\n\n");
    for r in rows {
        almd.push_str(&format!(
            "@extern(rust, \"crate::host\", \"f_{id}\")\nfn f_{id}({p}) -> {ret} = _\n\n",
            id = r.id, p = r.almide_params, ret = r.almide_ret
        ));
        host.push_str(&format!(
            "pub fn f_{id}({p}) -> {ret} {{ {body} }}\n",
            id = r.id, p = r.host_params, ret = r.host_ret, body = r.host_body
        ));
    }
    almd.push_str("effect fn main() -> Unit = {\n");
    for r in rows {
        almd.push_str(&format!("  {}\n", r.call));
    }
    almd.push_str("}\n");
    (almd, host)
}

#[test]
fn every_row_compiles_against_its_documented_host_signature_and_runs() {
    let dir = std::env::temp_dir().join(format!("almide_extern_rust_abi_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("native")).unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("almide.toml"), "[package]\nname = \"extern_abi\"\n").unwrap();
    let (almd, host) = program(ROWS);
    std::fs::write(dir.join("src/main.almd"), &almd).unwrap();
    std::fs::write(dir.join("native/host.rs"), &host).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .current_dir(&dir)
        .env("ALMIDE_RUN_PROJECT_DIR", dir.join(".run"))
        .arg("run")
        .arg(Path::new("src/main.almd"))
        .output()
        .expect("spawn almide run");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "the extern ABI matrix failed to build or run\n--- stderr ---\n{stderr}\n--- program ---\n{almd}\n--- host ---\n{host}"
    );
    let expected: String = ROWS.iter().map(|r| format!("{}\n", r.expected)).collect();
    assert_eq!(stdout, expected, "rows printed the wrong values\n--- program ---\n{almd}");
    let _ = std::fs::remove_dir_all(&dir);
}
