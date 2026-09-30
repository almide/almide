//! Every stdlib in-place mutator, called with a record FIELD as its `mut`
//! receiver while each other argument reads the same record through a call,
//! builds and runs natively and answers exactly what the let-bound spelling
//! answers (#3050, #3049).
//!
//! The native call site takes `&mut r.buf`; a sibling `pos(&r)` evaluated
//! after it is rustc E0502, so the borrow-conflict hoist moves each such
//! sibling into a temporary first (`bytes.set_u8(s.buf, pos(s), 7)` was
//! E0502 on 0.64.0). With two or more hoisted siblings every temporary also
//! needs its own name — a shared `__hoist` handed every argument the last
//! one's value (#3049), which the ORACLE here catches: the same statements
//! with each argument bound by a `let` first must print the same bytes.
//!
//! The family is read from the stdlib source, not listed by hand: every
//! `fn NAME(mut recv: T, …)` in bytes/string/list/map whose receiver is
//! followed by at least one argument (the receiver-only `clear`/`pop` have
//! no sibling to hoist). A mutator added later joins the matrix by existing.
//! Two receiver places are driven: a local `var` record, and a `mut` record
//! parameter (the #3049 caller shape). Native only: the wasm leg walls a
//! Bytes-writer on a `mut` parameter's field (`bytes-set-nonvar`), and the
//! cells it serves are pinned cross-target by
//! spec/wasm_cross/mut_call_hoisted_args_distinct.almd.

use std::path::Path;
use std::process::Command;

struct Mutator {
    module: &'static str,
    name: String,
    /// Parameter types after the receiver, in order.
    params: Vec<String>,
}

/// The receiver field each module's mutators write.
fn field_of(module: &str) -> &'static str {
    match module {
        "bytes" => "buf",
        "string" => "s",
        "list" => "xs",
        "map" => "m",
        other => panic!("no receiver field for module {other}"),
    }
}

/// Split a parameter list at top-level commas (`Map[K, V]` stays whole).
fn split_params(list: &str) -> Vec<String> {
    let (mut out, mut depth, mut cur) = (Vec::new(), 0i32, String::new());
    for c in list.chars() {
        match c {
            '[' | '(' => { depth += 1; cur.push(c); }
            ']' | ')' => { depth -= 1; cur.push(c); }
            ',' if depth == 0 => { out.push(cur.trim().to_string()); cur.clear(); }
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() { out.push(cur.trim().to_string()); }
    out
}

fn mutators() -> Vec<Mutator> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib");
    let mut out = Vec::new();
    for module in ["bytes", "string", "list", "map"] {
        let src = std::fs::read_to_string(root.join(format!("{module}.almd"))).expect("stdlib source");
        for line in src.lines() {
            let t = line.trim_start().trim_start_matches("pub ");
            let Some(rest) = t.strip_prefix("fn ") else { continue };
            let Some(open) = rest.find('(') else { continue };
            let name = rest[..open].split('[').next().unwrap_or("").to_string();
            let after = &rest[open + 1..];
            if !after.starts_with("mut ") { continue; }
            let Some(close) = after.rfind(") ->") else { continue };
            let params = split_params(&after[..close]);
            let rest_types: Vec<String> = params[1..]
                .iter()
                .map(|p| p.split_once(':').map_or(p.as_str(), |(_, ty)| ty).trim().to_string())
                .collect();
            if rest_types.is_empty() { continue; }
            out.push(Mutator { module, name, params: rest_types });
        }
    }
    out
}

/// An argument of type `ty` that READS the record `r` through a call — the
/// shape that conflicts with the `&mut r.<field>` receiver.
fn arg_for(module: &str, ty: &str) -> String {
    match (module, ty) {
        (_, "Int") | ("list", "A") | ("map", "V") => "pos(r)".into(),
        (_, "Float") => "flt(r)".into(),
        (_, "Float32") => "f32_of(r)".into(),
        (_, "Int32") => "i32_of(r)".into(),
        (_, "UInt16") => "u16_of(r)".into(),
        (_, "UInt32") => "u32_of(r)".into(),
        (_, "String") | ("map", "K") => "key(r)".into(),
        (_, "Bool") => "flag(r)".into(),
        (_, "Bytes") => "src(r)".into(),
        (_, "Endian") => "endian_of(r)".into(),
        (m, t) => panic!("no sibling-read argument for {m}.* parameter type {t} — extend arg_for"),
    }
}

/// One statement per mutator. `hoisted`: the arguments inline, as a user
/// writes them. Otherwise each argument is `let`-bound first, in order — the
/// spelling that never needed the hoist, and the oracle.
fn statements(ms: &[Mutator], hoisted: bool, indent: &str) -> String {
    let mut out = String::new();
    for (i, m) in ms.iter().enumerate() {
        let args: Vec<String> = m.params.iter().map(|t| arg_for(m.module, t)).collect();
        let recv = format!("r.{}", field_of(m.module));
        if hoisted {
            out.push_str(&format!("{indent}{}.{}({recv}, {})\n", m.module, m.name, args.join(", ")));
        } else {
            let names: Vec<String> = (0..args.len()).map(|j| format!("a{i}_{j}")).collect();
            for (n, a) in names.iter().zip(&args) {
                out.push_str(&format!("{indent}let {n} = {a}\n"));
            }
            out.push_str(&format!("{indent}{}.{}({recv}, {})\n", m.module, m.name, names.join(", ")));
        }
        // Advance the read so consecutive calls see different argument values.
        out.push_str(&format!("{indent}r.n = (r.n + 1) % 5\n"));
    }
    out
}

fn program(ms: &[Mutator], hoisted: bool) -> String {
    format!(
        r#"type R = {{ buf: Bytes, s: String, xs: List[Int], m: Map[String, Int], n: Int }}

fn pos(r: R) -> Int = r.n + 1
fn flt(r: R) -> Float = float.from_int(r.n) + 0.5
fn f32_of(r: R) -> Float32 = float.to_float32(float.from_int(r.n) + 1.5)
fn i32_of(r: R) -> Int32 = int.to_int32(r.n * 3 - 4)
fn u16_of(r: R) -> UInt16 = int.to_uint16(r.n * 100 + 9)
fn u32_of(r: R) -> UInt32 = int.to_uint32(r.n * 65536 + 11)
fn key(r: R) -> String = "k${{int.to_string(r.n)}}"
fn flag(r: R) -> Bool = r.n % 2 == 0
fn src(r: R) -> Bytes = bytes.from_list([r.n, 2, 3, 4])
fn endian_of(r: R) -> Endian = if r.n % 2 == 0 then LittleEndian else BigEndian

fn show(r: R) -> String = "${{bytes.to_list(r.buf)}} ${{r.s}} ${{r.xs}} ${{r.m}} ${{r.n}}"

fn drive(mut r: R) -> Unit = {{
{in_param}}}

effect fn main() -> Unit = {{
  var r = R {{ buf: bytes.new(40), s: "s", xs: [0], m: [:], n: 0 }}
{in_var}  println(show(r))
  drive(r)
  println(show(r))
}}
"#,
        in_param = statements(ms, hoisted, "  "),
        in_var = statements(ms, hoisted, "  "),
    )
}

fn run_native(src: &str, dir: &Path, label: &str) -> String {
    let file = dir.join(format!("{label}.almd"));
    std::fs::write(&file, src).expect("write program");
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["run", file.to_str().expect("path")])
        .output()
        .expect("spawn almide");
    assert!(
        out.status.success(),
        "{label}: native build/run failed ({:?}):\n{}\n--- program ---\n{src}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn every_in_place_mutator_with_a_field_receiver_evaluates_sibling_reads_before_the_borrow() {
    let ms = mutators();
    // The matrix is only as good as its row count: the byte writers alone are
    // ~50 fns, and push/insert anchor the list/map/string rows.
    assert!(ms.len() >= 50, "found only {} mutators — the signature scan broke", ms.len());
    for (m, f) in [("bytes", "set_u8"), ("bytes", "copy_within"), ("string", "push"), ("list", "push"), ("map", "insert")] {
        assert!(ms.iter().any(|x| x.module == m && x.name == f), "{m}.{f} missing from the matrix");
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let hoisted = run_native(&program(&ms, true), dir.path(), "hoisted");
    let bound = run_native(&program(&ms, false), dir.path(), "let_bound");
    assert!(hoisted.lines().count() == 2, "expected two state lines, got:\n{hoisted}");
    assert_eq!(hoisted, bound, "an inline sibling read answered differently from its let-bound spelling");
}
