//! The `@export(c)` ABI (#4447, #4448, C-452), gated as a matrix: every row
//! of the C ABI table (`almide_lang::types::extern_abi`) × {fn, effect fn},
//! called by a C host whose prototypes are written here by hand — never
//! derived from the table — so a wrapper that drifts from the C types it
//! promises reads back a wrong value.
//!
//! The wrapper used to keep its own type match: every return outside `Int`,
//! `Bool`, `Float`, `Unit` and `Handle` was cast `as i32`, and so was every
//! effect fn's return (lifted to `Result[T, String]` by then). An effect fn's
//! `Float` came back as an `int32_t`, an `Int64` was narrowed, an effect fn
//! returning `Unit` or `String` died at rustc (E0605), and a `String`
//! parameter became a Rust `String` inside an `extern "C"` fn — undefined
//! behaviour for a C caller passing `const char*`. `check` ran no E090 on the
//! signature. `Int` itself crossed as a C `int` (dialect epoch 15 made it an
//! `int64_t`, in `@extern(c)` too).
//!
//! The C host is the package's own `native/host.rs`: it declares each export
//! in an `extern "C"` block and calls it from `cf_*` driver fns, which the
//! Almide `main` calls through `@extern(c)`. No C toolchain is needed.

use std::path::Path;
use std::process::Command;

fn run_package(tag: &str, almd: &str, host: &str, args: &[&str]) -> (Option<i32>, String, String) {
    run_package_with(tag, almd, host, args, &[])
}

/// [`run_package`] with arguments after the source path too
/// (`almide src/main.almd --target rust`).
fn run_package_with(tag: &str, almd: &str, host: &str, args: &[&str], after: &[&str]) -> (Option<i32>, String, String) {
    let dir = std::env::temp_dir().join(format!("almide_export_c_abi_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("native")).unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("almide.toml"), "[package]\nname = \"export_c_abi\"\n").unwrap();
    std::fs::write(dir.join("src/main.almd"), almd).unwrap();
    std::fs::write(dir.join("native/host.rs"), host).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .current_dir(&dir)
        .env("ALMIDE_RUN_PROJECT_DIR", dir.join(".run"))
        .args(args)
        .arg(Path::new("src/main.almd"))
        .args(after)
        .output()
        .expect("spawn almide");
    let _ = std::fs::remove_dir_all(&dir);
    (out.status.code(), String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

/// One row of the C table, exported as `x_<id>(v: T) -> T` and as
/// `xe_<id>(v: T, fail: Bool) -> T` (an effect fn that errs when `fail`).
struct Row {
    id: &'static str,
    almide_ty: &'static str,
    /// The C type the host's prototype spells — by hand, not from the table.
    c_ty: &'static str,
    /// The Almide body over `v`.
    body: &'static str,
    /// The argument the host passes.
    arg: &'static str,
    /// How `main` prints the value the host got back (`r`).
    print: &'static str,
    expected: &'static str,
}

const ROWS: &[Row] = &[
    // `Int` is an `int64_t`: a value past 32 bits crosses whole, both ways.
    Row { id: "int", almide_ty: "Int", c_ty: "i64", body: "v + 1", arg: "5000000000", print: "int.to_string(r)", expected: "5000000001" },
    Row { id: "int_neg", almide_ty: "Int", c_ty: "i64", body: "v - 1", arg: "-4294967296", print: "int.to_string(r)", expected: "-4294967297" },
    Row { id: "int8", almide_ty: "Int8", c_ty: "i8", body: "v", arg: "-100", print: "int.to_string(int.from(r))", expected: "-100" },
    Row { id: "int16", almide_ty: "Int16", c_ty: "i16", body: "v", arg: "-30000", print: "int.to_string(int.from(r))", expected: "-30000" },
    Row { id: "int32", almide_ty: "Int32", c_ty: "i32", body: "v", arg: "-2000000000", print: "int.to_string(int.from(r))", expected: "-2000000000" },
    Row { id: "int64", almide_ty: "Int64", c_ty: "i64", body: "v", arg: "-9000000000000000000", print: "int.to_string(r)", expected: "-9000000000000000000" },
    Row { id: "uint8", almide_ty: "UInt8", c_ty: "u8", body: "v", arg: "250", print: "int.to_string(int.from(r))", expected: "250" },
    Row { id: "uint16", almide_ty: "UInt16", c_ty: "u16", body: "v", arg: "65000", print: "int.to_string(int.from(r))", expected: "65000" },
    Row { id: "uint32", almide_ty: "UInt32", c_ty: "u32", body: "v", arg: "4000000000", print: "int.to_string(int.from(r))", expected: "4000000000" },
    // 18e18 is past i64::MAX; its two's-complement Int is 18e18 - 2^64.
    Row { id: "uint64", almide_ty: "UInt64", c_ty: "u64", body: "v", arg: "18000000000000000000", print: "int.to_string(int.from_wrapping(r))", expected: "-446744073709551616" },
    Row { id: "float", almide_ty: "Float", c_ty: "f64", body: "v / 2.0", arg: "3.0", print: "float.to_string(r)", expected: "1.5" },
    Row { id: "float32", almide_ty: "Float32", c_ty: "f32", body: "v", arg: "1.5", print: "float.to_string(float.from(r))", expected: "1.5" },
    Row { id: "float64", almide_ty: "Float64", c_ty: "f64", body: "v", arg: "2.25", print: "float.to_string(r)", expected: "2.25" },
    // `Bool` is an `int`: any nonzero is true going in, 1 coming out.
    Row { id: "bool", almide_ty: "Bool", c_ty: "i32", body: "v", arg: "7", print: "\"${r}\"", expected: "true" },
    Row { id: "bool_f", almide_ty: "Bool", c_ty: "i32", body: "v", arg: "0", print: "\"${r}\"", expected: "false" },
];

/// The matrix program and its host: every row as a fn and an effect fn, a
/// `Unit` return in both forms, and a `Handle` that comes back as the same
/// pointer.
fn matrix() -> (String, String, String) {
    let mut almd = String::from(
        "local fn gate(fail: Bool) -> Result[Int, String] = if fail then err(\"refused\") else ok(0)\n\n",
    );
    let mut decls = String::new();
    let mut drivers = String::new();
    let mut main = String::from("effect fn main() -> Unit = {\n");
    let mut expected = String::new();
    for r in ROWS {
        let (id, t, c) = (r.id, r.almide_ty, r.c_ty);
        almd.push_str(&format!(
            "@export(c, \"x_{id}\")\nfn x_{id}(v: {t}) -> {t} = {body}\n\n\
             @export(c, \"xe_{id}\")\neffect fn xe_{id}(v: {t}, fail: Bool) -> {t} = {{\n  let _g = gate(fail)!\n  {body}\n}}\n\n\
             @extern(c, \"c\", \"cf_{id}\")\nfn c_{id}() -> {t} = _\n\n\
             @extern(c, \"c\", \"cf_e_{id}\")\nfn c_e_{id}() -> {t} = _\n\n",
            body = r.body,
        ));
        decls.push_str(&format!("    fn x_{id}(v: {c}) -> {c};\n    fn xe_{id}(v: {c}, fail: i32) -> {c};\n"));
        drivers.push_str(&format!(
            "#[no_mangle]\npub extern \"C\" fn cf_{id}() -> {c} {{ unsafe {{ x_{id}({arg}) }} }}\n\
             #[no_mangle]\npub extern \"C\" fn cf_e_{id}() -> {c} {{ unsafe {{ xe_{id}({arg}, 0) }} }}\n",
            arg = r.arg,
        ));
        for (call, label) in [(format!("c_{id}()"), id.to_string()), (format!("c_e_{id}()"), format!("{id} effect"))] {
            main.push_str(&format!("  let r_{n} = {call}\n  println(\"{label} \" + {p})\n", n = label.replace(' ', "_"), p = r.print.replace("(r)", &format!("(r_{})", label.replace(' ', "_"))).replace("${r}", &format!("${{r_{}}}", label.replace(' ', "_")))));
            expected.push_str(&format!("{label} {}\n", r.expected));
        }
    }
    // Unit: `void` in both forms; the fn prints, the host returns nothing.
    almd.push_str(concat!(
        "@export(c, \"x_unit\")\nfn x_unit(v: Int) -> Unit = println(\"unit ${v}\")\n\n",
        "@export(c, \"xe_unit\")\neffect fn xe_unit(v: Int, fail: Bool) -> Unit = {\n  let _g = gate(fail)!\n  println(\"unit effect ${v}\")\n}\n\n",
        "@extern(c, \"c\", \"cf_unit\")\nfn c_unit() -> Unit = _\n\n",
        // Handle: the same pointer comes back, in both forms.
        "@export(c, \"x_handle\")\nfn x_handle(v: Handle) -> Handle = v\n\n",
        "@export(c, \"xe_handle\")\neffect fn xe_handle(v: Handle, fail: Bool) -> Handle = {\n  let _g = gate(fail)!\n  v\n}\n\n",
        "@extern(c, \"c\", \"cf_handle\")\nfn c_handle() -> Int = _\n\n",
    ));
    decls.push_str(concat!(
        "    fn x_unit(v: i64);\n    fn xe_unit(v: i64, fail: i32);\n",
        "    fn x_handle(v: *mut u8) -> *mut u8;\n    fn xe_handle(v: *mut u8, fail: i32) -> *mut u8;\n",
    ));
    drivers.push_str(concat!(
        "#[no_mangle]\npub extern \"C\" fn cf_unit() { unsafe { x_unit(5000000000); xe_unit(-5000000000, 0) } }\n",
        "#[no_mangle]\npub extern \"C\" fn cf_handle() -> i64 {\n",
        "    let mut cell = 7u8;\n    let p: *mut u8 = &mut cell;\n",
        "    unsafe { (x_handle(p) == p && xe_handle(p, 0) == p) as i64 }\n}\n",
    ));
    main.push_str("  c_unit()\n  println(\"handle \" + int.to_string(c_handle()))\n}\n");
    expected.push_str("unit 5000000000\nunit effect -5000000000\nhandle 1\n");
    almd.push_str(&main);
    let host = format!(
        "#![allow(dead_code, unused_imports, clippy::missing_safety_doc)]\n\nextern \"C\" {{\n{decls}}}\n\n{drivers}"
    );
    (almd, host, expected)
}

/// Every row × {fn, effect fn} round-trips through hand-written C prototypes.
#[test]
#[cfg(unix)]
fn every_export_c_row_crosses_at_its_c_type_as_fn_and_effect_fn() {
    let (almd, host, expected) = matrix();
    let (code, stdout, stderr) = run_package("matrix", &almd, &host, &["run"]);
    assert_eq!(code, Some(0), "the @export(c) matrix failed\n--- stderr ---\n{stderr}\n--- program ---\n{almd}\n--- host ---\n{host}");
    assert_eq!(stdout, expected, "--- stderr ---\n{stderr}\n--- program ---\n{almd}\n--- host ---\n{host}");
}

/// The same matrix builds as a C library (`--cdylib`, the route #4448 found
/// dying at rustc on an effect fn's `Unit` return), and the wrapper the
/// standard codegen emits spells the C types of the table.
#[test]
#[cfg(unix)]
fn the_matrix_builds_as_a_cdylib_and_spells_the_table_types() {
    let (almd, host, _) = matrix();
    let (code, _, stderr) = run_package("cdylib", &almd, &host, &["build", "--cdylib", "-o", "libx"]);
    assert_eq!(code, Some(0), "--cdylib failed\n--- stderr ---\n{stderr}");
    let (code, rust, stderr) = run_package_with("emit", &almd, &host, &[], &["--target", "rust"]);
    assert_eq!(code, Some(0), "--target rust failed\n--- stderr ---\n{stderr}");
    for want in [
        "pub extern \"C\" fn __c_x_int(v: i64) -> i64",
        "pub extern \"C\" fn __c_xe_float(v: f64, fail: i32) -> f64",
        "pub extern \"C\" fn __c_xe_int64(v: i64, fail: i32) -> i64",
        "pub extern \"C\" fn __c_xe_uint64(v: u64, fail: i32) -> u64",
        "pub extern \"C\" fn __c_xe_float32(v: f32, fail: i32) -> f32",
        "pub extern \"C\" fn __c_xe_unit(v: i64, fail: i32) -> ()",
    ] {
        assert!(rust.contains(want), "no `{want}` in the emitted Rust");
    }
    // Each `#[export_name]` wrapper, up to its closing brace: none narrows
    // a value with `as i32` (the old catch-all), and every export has one.
    let wrappers: Vec<&str> = rust.split("#[export_name").skip(1).map(|w| w.split("\n}\n").next().unwrap_or(w)).collect();
    assert_eq!(wrappers.len(), ROWS.len() * 2 + 4, "one C wrapper per export");
    for w in &wrappers {
        assert!(!w.contains("as i32"), "a wrapper still narrows with `as i32`:\n{w}");
    }
}

/// C-452: an effect export's err has no C form. The message goes to stderr
/// and the process exits 1 before the caller regains control: the host's
/// line after the call never prints, nor does `main`'s.
#[test]
#[cfg(unix)]
fn an_effect_export_err_writes_its_message_and_exits_1() {
    let almd = concat!(
        "local fn check(n: Int) -> Result[Int, String] = if n > 100 then err(\"over 100: ${n}\") else ok(n)\n\n",
        "@export(c, \"xe_half\")\neffect fn xe_half(n: Int) -> Float = float.from(check(n)!) / 2.0\n\n",
        "@extern(c, \"c\", \"cf_half\")\nfn c_half(n: Int) -> Float = _\n\n",
        "effect fn main() -> Unit = {\n",
        "  println(\"ok \" + float.to_string(c_half(3)))\n",
        "  println(\"err \" + float.to_string(c_half(200)))\n",
        "  println(\"after\")\n",
        "}\n",
    );
    let host = concat!(
        "#![allow(dead_code, clippy::missing_safety_doc)]\n",
        "extern \"C\" { fn xe_half(n: i64) -> f64; }\n",
        "#[no_mangle]\npub extern \"C\" fn cf_half(n: i64) -> f64 {\n",
        "    let r = unsafe { xe_half(n) };\n    eprintln!(\"host regained control\");\n    r\n}\n",
    );
    let (code, stdout, stderr) = run_package("err", almd, host, &["run"]);
    assert_eq!(code, Some(1), "--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}");
    assert_eq!(stdout, "ok 1.5\n", "--- stderr ---\n{stderr}");
    assert!(stderr.contains("over 100: 200\n"), "--- stderr ---\n{stderr}");
    assert_eq!(stderr.matches("host regained control").count(), 1, "the host ran on after the err\n{stderr}");
}

/// C-452's judge fixture: the program calls its own exports through
/// `@extern(c)` bindings to the same symbols, then an effect export errs.
#[test]
#[cfg(unix)]
fn the_c452_fixture_round_trips_its_exports_and_exits_1_on_err() {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/spec/native_only/export_c_scalar_abi.almd");
    let dir = tempfile::tempdir().expect("tempdir");
    let bin = dir.path().join("export_c_scalar_abi");
    let build = Command::new(env!("CARGO_BIN_EXE_almide")).arg("build").arg(fixture).arg("-o").arg(&bin).output().expect("build");
    assert!(build.status.success(), "build failed: {}", String::from_utf8_lossy(&build.stderr));
    let out = Command::new(&bin).output().expect("run the fixture");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "stdout: {stdout}\nstderr: {stderr}");
    assert_eq!(
        stdout,
        concat!(
            "wide 5000000001\n",
            "wide_e 4294967303\n",
            "i64 -9000000000000000000\n",
            "u64 -446744073709551616\n",
            "f32 1.5\n",
            "f64 2.25\n",
            "half_e 1.5\n",
            "even_e true false\n",
            "note 7\n",
            "half_e of 200 errs:\n",
        ),
        "stderr: {stderr}"
    );
    assert_eq!(stderr, "over 100: 200\n", "stdout: {stdout}");
}

/// E090 at `check`: a parameter or return with no C form is refused, one
/// diagnostic each, as a fn and as an effect fn, and the declared return of
/// an effect fn is what is judged. A `String` parameter is refused rather
/// than compiled into an `extern "C"` fn taking a Rust `String`.
#[test]
fn export_c_types_without_a_c_form_are_e090() {
    const NO_C_FORM: &[&str] = &[
        "String", "Bytes", "List[Int]", "Map[String, Int]", "Set[Int]", "Option[Int]", "Result[Int, String]",
        "(Int, String)", "Pt", "Sh", "(Int) -> Int",
    ];
    let mut almd = String::from("type Pt = { x: Int, y: Int }\ntype Sh = | Circle(Int) | Sq(Int)\n\n");
    let mut want = Vec::new();
    for (i, t) in NO_C_FORM.iter().enumerate() {
        for (k, eff) in [("p", ""), ("pe", "effect ")] {
            almd.push_str(&format!("@export(c, \"{k}{i}\")\n{eff}fn {k}{i}(v: {t}) -> Int = 0\n\n"));
            want.push(format!("@export(c) fn '{k}{i}': parameter 'v' has type"));
        }
        // A fn's return, and an effect fn's declared (ok) return.
        let body = match *t {
            "String" => "\"\"",
            "Bytes" => "bytes.new()",
            "List[Int]" => "[]",
            "Map[String, Int]" => "map.new()",
            "Set[Int]" => "set.new()",
            "Option[Int]" => "none",
            "Result[Int, String]" => "ok(0)",
            "(Int, String)" => "(0, \"\")",
            "Pt" => "Pt { x: 0, y: 0 }",
            "Sh" => "Circle(0)",
            _ => "(x) => x",
        };
        for (k, eff) in [("r", ""), ("re", "effect ")] {
            almd.push_str(&format!("@export(c, \"{k}{i}\")\n{eff}fn {k}{i}(n: Int) -> {t} = {body}\n\n"));
            want.push(format!("@export(c) fn '{k}{i}': return type"));
        }
    }
    almd.push_str("@export(c, \"m\")\nfn m(mut n: Int) -> Int = n\n\n");
    want.push("@export(c) fn 'm': `mut` parameter 'n'".to_string());
    almd.push_str("effect fn main() -> Unit = println(\"x\")\n");
    let (code, stdout, stderr) = run_package("e090", &almd, "", &["check"]);
    let out = format!("{stdout}{stderr}");
    assert_ne!(code, Some(0), "check accepted @export(c) types with no C form:\n{almd}");
    for w in &want {
        assert!(out.contains(w.as_str()), "no E090 for `{w}`\n--- check output ---\n{out}\n--- program ---\n{almd}");
    }
    assert_eq!(out.matches("error[E090]").count(), want.len(), "one E090 per declaration\n--- check output ---\n{out}");
    // The String parameter names the shape a C caller can pass instead.
    assert!(out.contains("a C caller's `const char*` has no length and no owner"), "{out}");
    // Every row the table accepts checks clean.
    let (almd_ok, host, _) = matrix();
    let (code, stdout, stderr) = run_package("e090_clean", &almd_ok, &host, &["check"]);
    assert_eq!(code, Some(0), "check refused the accepted @export(c) rows:\n{stdout}{stderr}");
}

/// The v1 native render (`try_render_rust_source`) has no `@export(c)`
/// wrapper — its program's `exports` are empty — so it walls a program that
/// declares one, and the build takes the standard codegen, which renders the
/// wrapper. The same program without the attribute renders.
#[test]
fn the_v1_native_render_walls_an_export_c_program() {
    let plain = "fn wide(v: Int) -> Int = v + 1\n\neffect fn main() -> Unit = println(int.to_string(wide(2)))\n";
    let exported = format!("@export(c, \"wide\")\n{plain}");
    assert!(
        almide_mir::pipeline::try_render_rust_source(plain).is_ok(),
        "the control program walled, so the wall below would prove nothing"
    );
    let wall = almide_mir::pipeline::try_render_rust_source(&exported).expect_err("the v1 render accepted an @export(c) program");
    assert!(wall.to_string().contains("fn `wide` is @export(c)"), "{wall}");
}
