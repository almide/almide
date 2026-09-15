//! #2186 step 5: the native borrow-mode ORACLE. Every ownership fixture in
//! `tests/` pins one shape that once broke; this test generates the shapes.
//!
//! For each borrow-eligible parameter type it writes one program holding
//! every USE a body can make of a param (read it, return it, wrap it in a
//! list or a record, forward it to a borrowing and to a consuming callee,
//! capture it in a closure, compare it, concatenate it, bind it and read the
//! binding twice, branch on it, match it through an Option, loop over it,
//! project a field, spread it, mutate it through `mut`) crossed with every
//! CALL SITE shape (a literal, a `let` read twice, a loop, a borrowed param
//! forwarded twice, a `var` reassigned between two calls). Then it asserts
//! the property the fixtures each pin one point of: the program checks, the
//! native build succeeds (rustc accepts every borrow / clone / move the
//! passes chose), and native output equals wasm output byte for byte.
//!
//! A wrong borrow mode is a build failure here; a wrong clone or move is a
//! divergence. Either names the type, the use and the call shape.

use std::path::Path;
use std::process::Command;

/// One borrow-eligible type: how to spell it, two distinct literals, and
/// how to read a String out of a value (the printed witness).
struct TypeSpec {
    tag: &'static str,
    ty: &'static str,
    lit: &'static str,
    lit2: &'static str,
    /// A pure String-valued read of `p`.
    read: fn(&str) -> String,
    /// `p + p`, when the type concatenates.
    concat: bool,
    /// An in-place mutation of a `mut` param, when the type has one.
    mutate: Option<&'static str>,
    /// A `for x in p` body summing into an Int, when the type iterates.
    loop_body: Option<&'static str>,
    record: bool,
}

const TYPES: &[TypeSpec] = &[
    TypeSpec { tag: "str", ty: "String", lit: "\"ab\"", lit2: "\"cde\"", read: |p| format!("int.to_string(string.len({p}))"), concat: true, mutate: None, loop_body: None, record: false },
    TypeSpec { tag: "list", ty: "List[Int]", lit: "[1, 2]", lit2: "[3, 4, 5]", read: |p| format!("int.to_string(list.len({p}))"), concat: true, mutate: Some("list.push(p, 9)"), loop_body: Some("acc = acc + x"), record: false },
    TypeSpec { tag: "map", ty: "Map[String, Int]", lit: "[\"a\": 1]", lit2: "[\"b\": 2, \"c\": 3]", read: |p| format!("int.to_string(map.len({p}))"), concat: false, mutate: Some("map.insert(p, \"z\", 9)"), loop_body: None, record: false },
    TypeSpec { tag: "set", ty: "Set[Int]", lit: "set.from_list([1])", lit2: "set.from_list([2, 3])", read: |p| format!("int.to_string(set.len({p}))"), concat: false, mutate: None, loop_body: None, record: false },
    TypeSpec { tag: "bytes", ty: "Bytes", lit: "bytes.from_list([1])", lit2: "bytes.from_list([2, 3])", read: |p| format!("int.to_string(bytes.len({p}))"), concat: false, mutate: Some("bytes.push(p, 9)"), loop_body: None, record: false },
    TypeSpec { tag: "rec", ty: "Tok", lit: "{ text: \"ab\", n: 1 }", lit2: "{ text: \"cde\", n: 2 }", read: |p| format!("int.to_string(string.len({p}.text) + {p}.n)"), concat: false, mutate: None, loop_body: None, record: true },
];

/// One use of the param inside a body: the fn source and how the call's
/// result is shown as a String.
struct Shape {
    name: String,
    /// The fn definition, taking `p` and possibly `c: Bool`.
    def: String,
    /// Extra params after `p` at the call site.
    extra: &'static str,
    /// `show(expr)`: the String witness of the fn's result.
    show: fn(&TypeSpec, &str) -> String,
    /// The call site must pass a `var` (a `mut` param).
    needs_var: bool,
}

fn shapes(t: &TypeSpec) -> Vec<Shape> {
    let r = t.read;
    let ty = t.ty;
    let tag = t.tag;
    let ident = |s: &str| s.to_string();
    let read_result: fn(&TypeSpec, &str) -> String = |t, e| (t.read)(e);
    let box_result: fn(&TypeSpec, &str) -> String = |t, e| (t.read)(&format!("{e}.v"));
    let list_result: fn(&TypeSpec, &str) -> String = |_, e| format!("int.to_string(list.len({e}))");
    let mut out = vec![
        Shape { name: "read".into(), def: format!("fn u_read_{tag}(p: {ty}) -> String = {}", r("p")), extra: "", show: ident_show, needs_var: false },
        Shape { name: "ret".into(), def: format!("fn u_ret_{tag}(p: {ty}) -> {ty} = p"), extra: "", show: read_result, needs_var: false },
        Shape { name: "list".into(), def: format!("fn u_list_{tag}(p: {ty}) -> List[{ty}] = [p]"), extra: "", show: list_result, needs_var: false },
        Shape { name: "rec".into(), def: format!("fn u_rec_{tag}(p: {ty}) -> Box_{tag} = {{ v: p }}"), extra: "", show: box_result, needs_var: false },
        Shape { name: "via".into(), def: format!("fn u_via_{tag}(p: {ty}) -> String = u_read_{tag}(p)"), extra: "", show: ident_show, needs_var: false },
        Shape { name: "viac".into(), def: format!("fn u_viac_{tag}(p: {ty}) -> String = int.to_string(list.len(u_list_{tag}(p)))"), extra: "", show: ident_show, needs_var: false },
        Shape { name: "cap".into(), def: format!("fn u_cap_{tag}(p: {ty}) -> String = list.join(list.map([1, 2], (i) => {}), \",\")", r("p")), extra: "", show: ident_show, needs_var: false },
        Shape { name: "eq".into(), def: format!("fn u_eq_{tag}(p: {ty}) -> String = if {} == p then \"eq\" else \"ne\"", t.lit2), extra: "", show: ident_show, needs_var: false },
        Shape { name: "eq2".into(), def: format!("fn u_eq2_{tag}(p: {ty}) -> String = if p == {} then \"eq\" else \"ne\"", t.lit), extra: "", show: ident_show, needs_var: false },
        Shape { name: "let".into(), def: format!("fn u_let_{tag}(p: {ty}) -> String = {{\n  let q = p\n  {} + {}\n}}", r("q"), r("q")), extra: "", show: ident_show, needs_var: false },
        Shape { name: "if".into(), def: format!("fn u_if_{tag}(p: {ty}, c: Bool) -> {ty} = if c then p else {}", t.lit2), extra: ", true", show: read_result, needs_var: false },
        Shape { name: "opt".into(), def: format!("fn u_opt_{tag}(p: {ty}, c: Bool) -> String = {{\n  let o: Option[{ty}] = if c then some(p) else none\n  match o {{\n    some(v) => {},\n    none => \"-\",\n  }}\n}}", r("v")), extra: ", true", show: ident_show, needs_var: false },
        Shape { name: "twice".into(), def: format!("fn u_twice_{tag}(p: {ty}) -> String = {} + u_read_{tag}(p)", r("p")), extra: "", show: ident_show, needs_var: false },
        Shape { name: "pair".into(), def: format!("fn u_pair_{tag}(p: {ty}, q: {ty}) -> String = {} + {}", r("p"), r("q")), extra: "", show: ident_show, needs_var: false },
    ];
    let _ = ident;
    if t.concat {
        out.push(Shape { name: "cat".into(), def: format!("fn u_cat_{tag}(p: {ty}) -> {ty} = p + p"), extra: "", show: read_result, needs_var: false });
    }
    if let Some(body) = t.loop_body {
        out.push(Shape { name: "loop".into(), def: format!("fn u_loop_{tag}(p: {ty}) -> String = {{\n  var acc = 0\n  for x in p {{\n    {body}\n  }}\n  int.to_string(acc)\n}}"), extra: "", show: ident_show, needs_var: false });
    }
    if let Some(m) = t.mutate {
        out.push(Shape { name: "mut".into(), def: format!("fn u_mut_{tag}(mut p: {ty}) -> Unit = {m}"), extra: "", show: unit_show, needs_var: true });
    }
    if t.record {
        out.push(Shape { name: "field".into(), def: format!("fn u_field_{tag}(p: {ty}) -> {ty} = {{ text: p.text, n: p.n + 1 }}"), extra: "", show: read_result, needs_var: false });
        out.push(Shape { name: "spread".into(), def: format!("fn u_spread_{tag}(p: {ty}) -> {ty} = {{ ...p, n: 9 }}"), extra: "", show: read_result, needs_var: false });
    }
    // The `pair` shape takes two params: its call sites pass `p` twice.
    out
}

fn ident_show(_: &TypeSpec, e: &str) -> String { e.to_string() }
fn unit_show(_: &TypeSpec, e: &str) -> String { format!("{{\n    {e}\n    \"unit\"\n  }}") }

/// How main hands the value to each use: the emitted statements produce a
/// String named `line`.
fn call_sites(t: &TypeSpec, s: &Shape) -> Vec<(String, String)> {
    let f = |arg: &str| {
        let extra = if s.name == "pair" { format!(", {arg}") } else { s.extra.to_string() };
        format!("u_{}_{}({arg}{extra})", s.name, t.tag)
    };
    let show = |arg: &str| (s.show)(t, &f(arg));
    let mut out = Vec::new();
    if s.needs_var {
        // A `mut` param takes a `var`: mutate, read, mutate again.
        out.push(("var".into(), format!("var x = {}\n  {}\n  let a = {}\n  {}\n  let line = a + {}", t.lit, f("x"), (t.read)("x"), f("x"), (t.read)("x"))));
        return out;
    }
    out.push(("lit".into(), format!("let line = {}", show(t.lit))));
    out.push(("twice".into(), format!("let x = {}\n  let line = {} + {}", t.lit, show("x"), show("x"))));
    out.push(("loop".into(), format!("let x = {}\n  var acc = \"\"\n  for _i in list.range(0, 2) {{\n    acc = acc + {}\n  }}\n  let line = acc", t.lit, show("x"))));
    out.push(("param".into(), format!("let line = c_{}_{}({})", s.name, t.tag, t.lit)));
    out.push(("mutated".into(), format!("var x = {}\n  let a = {}\n  x = {}\n  let line = a + {}", t.lit, show("x"), t.lit2, show("x"))));
    out
}

fn program(t: &TypeSpec) -> String {
    let mut src = String::new();
    src.push_str("// generated by tests/native_borrow_oracle_test.rs — every use × every call site of one param type\n");
    if t.record {
        src.push_str("type Tok = { text: String, n: Int }\n");
    }
    src.push_str(&format!("type Box_{} = {{ v: {} }}\n", t.tag, t.ty));
    let shapes = shapes(t);
    for s in &shapes {
        src.push_str(&s.def);
        src.push('\n');
    }
    // The forwarding callers: a borrowed param handed to the use twice.
    for s in shapes.iter().filter(|s| !s.needs_var) {
        let extra = if s.name == "pair" { ", q".to_string() } else { s.extra.to_string() };
        let call = format!("u_{}_{}(q{extra})", s.name, t.tag);
        let shown = (s.show)(t, &call);
        src.push_str(&format!("fn c_{}_{}(q: {}) -> String = {} + {}\n", s.name, t.tag, t.ty, shown, shown));
    }
    src.push_str("fn main() -> Unit = {\n");
    for s in &shapes {
        for (site, body) in call_sites(t, s) {
            src.push_str(&format!("  {{\n  {}\n  println(\"{}/{}: \" + line)\n  }}\n", body.replace('\n', "\n  "), s.name, site));
        }
    }
    src.push_str("}\n");
    src
}

fn run(almide: &str, args: &[&str], src: &Path) -> (bool, String, String) {
    let out = Command::new(almide).args(args).arg(src).output().expect("almide");
    (out.status.success(), String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn every_use_and_call_shape_builds_natively_and_agrees_with_wasm() {
    let almide = env!("CARGO_BIN_EXE_almide");
    let dir = std::env::temp_dir().join(format!("almide-borrow-oracle-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut failures = Vec::new();
    for t in TYPES {
        let src = dir.join(format!("oracle_{}.almd", t.tag));
        let text = program(t);
        std::fs::write(&src, &text).unwrap();
        let (checked, _, check_err) = run(almide, &["check"], &src);
        if !checked {
            failures.push(format!("[{}] the generated program does not check — generator bug:\n{check_err}\n--- program ---\n{text}", t.tag));
            continue;
        }
        let (native_ok, native_out, native_err) = run(almide, &["run"], &src);
        if !native_ok {
            let rustc: Vec<&str> = native_err.lines().filter(|l| l.contains("error[") || l.contains("^^^") || l.starts_with("  -->") || l.contains("|")).take(40).collect();
            failures.push(format!("[{}] the native build failed — a borrow / clone / move the passes chose is one rustc refuses:\n{}\n--- program ---\n{text}", t.tag, rustc.join("\n")));
            continue;
        }
        let (wasm_ok, wasm_out, wasm_err) = run(almide, &["run", "--target", "wasm"], &src);
        if !wasm_ok {
            failures.push(format!("[{}] the wasm leg failed:\n{wasm_err}", t.tag));
            continue;
        }
        if native_out != wasm_out {
            let diff: Vec<String> = native_out.lines().zip(wasm_out.lines()).filter(|(a, b)| a != b).map(|(a, b)| format!("  native: {a}\n  wasm:   {b}")).collect();
            failures.push(format!("[{}] native and wasm disagree — a clone or move changed a value:\n{}", t.tag, diff.join("\n")));
        }
        let lines = native_out.lines().count();
        assert!(lines >= 60, "[{}] the program printed only {lines} lines — the generator lost its shapes", t.tag);
    }
    if !almide_base::env::flag("ALMIDE_ORACLE_KEEP") {
        let _ = std::fs::remove_dir_all(&dir);
    } else {
        eprintln!("generated programs kept under {}", dir.display());
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
