//! #4106 structural constant-time gate (C-421). `bytes.is_equal_constant_time`
//! / `string.is_equal_constant_time` promise no early exit on the first
//! differing byte. The cross fixture only shows the ANSWER is right; this gate
//! inspects the CODE both legs actually run, deterministically:
//!
//! - wasm: build a program that calls the compare and the same program
//!   without it; the functions only the first module carries are the
//!   compare's. A taint walk over each one marks every value derived from a
//!   byte load (`i32/i64.load8_*`), through the operand stack, locals, block
//!   results and callee return summaries, and fails if any `if` / `br_if` /
//!   `br_table` / `select` condition is tainted. Wide loads (`i32.load`,
//!   `i64.load`) are the block-header reads (len@4) and stay clean: the
//!   self-host floor reads payload bytes only through `prim.load8`.
//! - native: the runtime fns are cut out of runtime/rs/src/{bytes,string}.rs
//!   VERBATIM, compiled by rustc at opt-level 3 to LLVM IR, and the same taint
//!   walk runs over SSA (sources: `load i8`; through phis, casts, inline-asm
//!   barriers): no `br i1` / `switch` / `select i1` on a tainted value, and no
//!   call to `bcmp` / `memcmp`.
//!
//! NEGATIVE CONTROLS (same walks, must FAIL): on wasm, a user twin that exits
//! at the first differing byte and plain `==` on Bytes; on native, a twin with
//! `if a[i] != b[i] { return false }` and plain slice `==`. A gate whose
//! controls pass proves nothing, so each control is asserted to be caught.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn scratch(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("ct-gate-{name}"));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

// ── wasm leg ────────────────────────────────────────────────────────────────

fn build_wasm(dir: &Path, name: &str, src: &str) -> Vec<u8> {
    let p = dir.join(format!("{name}.almd"));
    let out = dir.join(format!("{name}.wasm"));
    std::fs::write(&p, src).unwrap();
    let st = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["build", p.to_str().unwrap(), "--target", "wasm", "-o", out.to_str().unwrap()])
        .output()
        .expect("run almide build");
    assert!(st.status.success(), "almide build {name} failed: {}", String::from_utf8_lossy(&st.stderr));
    std::fs::read(&out).unwrap()
}

struct Module {
    /// (param count, result count) per type index
    types: Vec<(usize, usize)>,
    /// type index per function index (imports first)
    func_types: Vec<u32>,
    n_imports: usize,
    /// raw body bytes per defined function
    bodies: Vec<Vec<u8>>,
    /// local count (params + declared) per defined function
    locals: Vec<usize>,
    /// exported function indices (the entry: its body differs between the
    /// builds only at the call site, and branching on the ANSWER is the point)
    exports: HashSet<u32>,
}

fn parse(bytes: &[u8]) -> Module {
    use wasmparser::{Parser, Payload, TypeRef};
    let mut m = Module { types: vec![], func_types: vec![], n_imports: 0, bodies: vec![], locals: vec![], exports: HashSet::new() };
    for p in Parser::new(0).parse_all(bytes) {
        match p.unwrap() {
            Payload::TypeSection(r) => {
                for rg in r {
                    for st in rg.unwrap().into_types() {
                        match &st.composite_type.inner {
                            wasmparser::CompositeInnerType::Func(f) => {
                                m.types.push((f.params().len(), f.results().len()))
                            }
                            _ => m.types.push((0, 0)),
                        }
                    }
                }
            }
            Payload::ImportSection(r) => {
                for imp in r.into_imports() {
                    if let TypeRef::Func(t) = imp.unwrap().ty {
                        m.func_types.push(t);
                        m.n_imports += 1;
                    }
                }
            }
            Payload::ExportSection(r) => {
                for e in r {
                    let e = e.unwrap();
                    if matches!(e.kind, wasmparser::ExternalKind::Func) && ENTRY_EXPORTS.contains(&e.name) {
                        m.exports.insert(e.index);
                    }
                }
            }
            Payload::FunctionSection(r) => {
                for t in r {
                    m.func_types.push(t.unwrap());
                }
            }
            Payload::CodeSectionEntry(body) => {
                let fidx = m.n_imports + m.bodies.len();
                let mut n = m.types[m.func_types[fidx] as usize].0;
                for l in body.get_locals_reader().unwrap() {
                    n += l.unwrap().0 as usize;
                }
                m.locals.push(n);
                let r = body.range();
                m.bodies.push(bytes[r.start as usize..r.end as usize].to_vec());
            }
            _ => {}
        }
    }
    m
}

/// The entry exports: their bodies differ between the builds only at the
/// call site, and branching on the ANSWER there is the point of the call. A
/// user fn the entry file also exports stays analyzed (the control twin is one).
const ENTRY_EXPORTS: &[&str] = &["_start", "main"];

fn op_name(op: &wasmparser::Operator) -> String {
    let d = format!("{op:?}");
    d.chars().take_while(|c| c.is_alphanumeric()).collect()
}

fn blockty_results(m: &Module, bt: &wasmparser::BlockType) -> usize {
    match bt {
        wasmparser::BlockType::Empty => 0,
        wasmparser::BlockType::Type(_) => 1,
        wasmparser::BlockType::FuncType(i) => m.types[*i as usize].1,
    }
}

struct Frame {
    base: usize,
    results: usize,
    acc: bool,
    unreachable: bool,
}

/// Taint walk over one defined function. Returns (does a tainted value reach
/// the return, the tainted branch sites). `summ` holds each function's
/// "returns a tainted value" so far (imports: clean unless an arg is tainted).
fn walk(m: &Module, def: usize, summ: &[bool]) -> (bool, Vec<String>) {
    let body = wasmparser::FunctionBody::new(wasmparser::BinaryReader::new(&m.bodies[def], 0));
    let mut tainted_locals: HashSet<u32> = HashSet::new();
    loop {
        let before = tainted_locals.len();
        let mut stack: Vec<bool> = Vec::new();
        let mut frames = vec![Frame { base: 0, results: m.types[m.func_types[m.n_imports + def] as usize].1, acc: false, unreachable: false }];
        let mut sites = Vec::new();
        let mut ret = false;
        let mut r = body.get_operators_reader().unwrap();
        let mut at = 0usize;
        while !r.eof() {
            let op = r.read().unwrap();
            at += 1;
            let name = op_name(&op);
            let base = frames.last().map_or(0, |f| f.base);
            let pop = |stack: &mut Vec<bool>| if stack.len() > base { stack.pop().unwrap() } else { false };
            use wasmparser::Operator as O;
            match &op {
                O::Block { blockty } | O::Loop { blockty } => {
                    let results = blockty_results(m, blockty);
                    frames.push(Frame { base: stack.len(), results, acc: false, unreachable: false });
                }
                O::If { blockty } => {
                    if pop(&mut stack) {
                        sites.push(format!("op#{at} if"));
                    }
                    let results = blockty_results(m, blockty);
                    frames.push(Frame { base: stack.len(), results, acc: false, unreachable: false });
                }
                O::Else => {
                    let f = frames.last_mut().unwrap();
                    if !f.unreachable {
                        let n = f.results.min(stack.len() - f.base);
                        f.acc |= stack[stack.len() - n..].iter().any(|t| *t);
                    }
                    stack.truncate(f.base);
                    f.unreachable = false;
                }
                O::End => {
                    let f = frames.pop().unwrap();
                    let mut acc = f.acc;
                    if !f.unreachable {
                        let n = f.results.min(stack.len() - f.base);
                        acc |= stack[stack.len() - n..].iter().any(|t| *t);
                    }
                    stack.truncate(f.base);
                    if frames.is_empty() {
                        ret |= acc;
                    } else {
                        stack.extend(std::iter::repeat_n(acc, f.results));
                    }
                }
                O::Br { .. } | O::Return | O::Unreachable => {
                    if matches!(op, O::Return) {
                        let n = frames[0].results.min(stack.len());
                        ret |= stack[stack.len() - n..].iter().any(|t| *t);
                    }
                    let f = frames.last_mut().unwrap();
                    stack.truncate(f.base);
                    f.unreachable = true;
                }
                O::BrIf { .. } => {
                    if pop(&mut stack) {
                        sites.push(format!("op#{at} br_if"));
                    }
                }
                O::BrTable { .. } => {
                    if pop(&mut stack) {
                        sites.push(format!("op#{at} br_table"));
                    }
                    let f = frames.last_mut().unwrap();
                    stack.truncate(f.base);
                    f.unreachable = true;
                }
                O::Select | O::TypedSelect { .. } => {
                    if pop(&mut stack) {
                        sites.push(format!("op#{at} select"));
                    }
                    let (b, a) = (pop(&mut stack), pop(&mut stack));
                    stack.push(a || b);
                }
                O::LocalGet { local_index } => stack.push(tainted_locals.contains(local_index)),
                O::LocalSet { local_index } => {
                    if pop(&mut stack) {
                        tainted_locals.insert(*local_index);
                    }
                }
                O::LocalTee { local_index } => {
                    let t = pop(&mut stack);
                    if t {
                        tainted_locals.insert(*local_index);
                    }
                    stack.push(t);
                }
                O::GlobalGet { .. } | O::MemorySize { .. } => stack.push(false),
                O::GlobalSet { .. } | O::Drop => {
                    pop(&mut stack);
                }
                O::Nop => {}
                O::Call { function_index } | O::ReturnCall { function_index } => {
                    let (np, nr) = m.types[m.func_types[*function_index as usize] as usize];
                    let mut any = false;
                    for _ in 0..np {
                        any |= pop(&mut stack);
                    }
                    let fi = *function_index as usize;
                    let callee = fi >= m.n_imports && summ[fi - m.n_imports];
                    if matches!(op, O::ReturnCall { .. }) {
                        ret |= any || callee;
                        let f = frames.last_mut().unwrap();
                        stack.truncate(f.base);
                        f.unreachable = true;
                    } else {
                        stack.extend(std::iter::repeat_n(any || callee, nr));
                    }
                }
                O::MemoryCopy { .. } | O::MemoryFill { .. } => {
                    for _ in 0..3 {
                        pop(&mut stack);
                    }
                }
                O::MemoryGrow { .. } => {
                    let t = pop(&mut stack);
                    stack.push(t);
                }
                _ if name.ends_with("Const") => stack.push(false),
                _ if name.contains("Load") => {
                    let addr = pop(&mut stack);
                    stack.push(addr || name.contains("Load8"));
                }
                _ if name.contains("Store") => {
                    pop(&mut stack);
                    pop(&mut stack);
                }
                _ if ["Eqz", "Clz", "Ctz", "Popcnt", "Wrap", "Extend", "Trunc", "Convert", "Demote", "Promote", "Reinterpret", "Neg", "Abs", "Sqrt", "Ceil", "Floor", "Nearest"]
                    .iter()
                    .any(|s| name.contains(s)) =>
                {
                    let t = pop(&mut stack);
                    stack.push(t);
                }
                _ if ["Add", "Sub", "Mul", "Div", "Rem", "And", "Or", "Xor", "Shl", "Shr", "Rotl", "Rotr", "Eq", "Ne", "Lt", "Gt", "Le", "Ge", "Min", "Max", "Copysign"]
                    .iter()
                    .any(|s| name.ends_with(s) || name.ends_with(&format!("{s}S")) || name.ends_with(&format!("{s}U"))) =>
                {
                    let (b, a) = (pop(&mut stack), pop(&mut stack));
                    stack.push(a || b);
                }
                _ => panic!("constant-time gate: operator {name} is outside the walk's table — extend it (do not skip)"),
            }
        }
        if tainted_locals.len() == before {
            return (ret, sites);
        }
    }
}

/// The functions `with` carries that `without` does not (by body bytes), and
/// the tainted branch sites in each.
fn wasm_findings(with: &[u8], without: &[u8]) -> (usize, Vec<String>) {
    let (mw, mo) = (parse(with), parse(without));
    let base: HashSet<&Vec<u8>> = mo.bodies.iter().collect();
    // return-taint summaries to a fixpoint over the whole module
    let mut summ = vec![false; mw.bodies.len()];
    loop {
        let next: Vec<bool> = (0..mw.bodies.len()).map(|d| walk(&mw, d, &summ).0).collect();
        if next == summ {
            break;
        }
        summ = next;
    }
    let mut n = 0;
    let mut out = Vec::new();
    for d in 0..mw.bodies.len() {
        if base.contains(&mw.bodies[d]) || mw.exports.contains(&((mw.n_imports + d) as u32)) {
            continue;
        }
        n += 1;
        if std::env::var("CT_DEBUG").is_ok() {
            eprintln!("analyzed func {} exports={:?}", mw.n_imports + d, mw.exports);
        }
        for s in walk(&mw, d, &summ).1 {
            out.push(format!("func {}: {s}", mw.n_imports + d));
        }
    }
    (n, out)
}

/// One program shape for every build, so the module diff isolates the call:
/// `x`/`y` are Strings built at run time, `a`/`b` Bytes, `cond` is the only
/// thing that changes.
fn program(cond: &str, extra: &str) -> String {
    format!(
        "{extra}fn main() -> Unit = {{\n  let a = bytes.from_string(\"abcdef\")\n  let b = bytes.from_string(\"abcdeg\")\n  let x = bytes.to_string_lossy(a)\n  let y = bytes.to_string_lossy(b)\n  println(if string.len(x) == string.len(y) and {cond} then \"eq\" else \"ne\")\n}}\n"
    )
}

#[test]
fn wasm_constant_time_compare_has_no_data_dependent_branch() {
    let dir = scratch("wasm");
    let base = build_wasm(&dir, "baseline", &program("bytes.len(a) == bytes.len(b)", ""));
    for (name, call) in [("bytes", "bytes.is_equal_constant_time(a, b)"), ("string", "string.is_equal_constant_time(x, y)")] {
        let with = build_wasm(&dir, name, &program(call, ""));
        let (n, findings) = wasm_findings(&with, &base);
        assert!(n > 0, "{name}: the compare added no function — the gate is not looking at it");
        assert!(findings.is_empty(), "{name}.is_equal_constant_time branches on a byte value on wasm:\n{}", findings.join("\n"));
    }
}

#[test]
fn control_wasm_early_exit_twins_are_caught() {
    let dir = scratch("wasm-ctl");
    let base = build_wasm(&dir, "baseline", &program("bytes.len(a) == bytes.len(b)", ""));
    let twin = "fn early(a: Bytes, b: Bytes, i: Int) -> Bool =\n  if i >= bytes.len(a) then true\n  else if bytes.get_or(a, i, 0) != bytes.get_or(b, i, 0) then false\n  else early(a, b, i + 1)\n\n";
    for (name, call, extra) in [("early_twin", "early(a, b, 0)", twin), ("plain_eq", "a == b", ""), ("plain_eq_string", "x == y", "")] {
        let with = build_wasm(&dir, name, &program(call, extra));
        let (n, findings) = wasm_findings(&with, &base);
        assert!(n > 0, "control {name}: nothing added");
        assert!(!findings.is_empty(), "control {name}: an early-exit compare was NOT caught — the wasm walk is blind");
        eprintln!("control {name} caught: {findings:?}");
    }
}

// ── native leg ──────────────────────────────────────────────────────────────

/// Cut `pub fn <name>` out of a runtime source verbatim (to its closing `}` at
/// column 0).
fn cut_fn(file: &str, name: &str) -> String {
    let src = std::fs::read_to_string(root().join(file)).unwrap();
    let start = src.find(&format!("pub fn {name}(")).unwrap_or_else(|| panic!("{name} not in {file}"));
    let end = start + src[start..].find("\n}\n").expect("closing brace") + 3;
    src[start..end].to_string()
}

fn llvm_ir(dir: &Path, src: &str) -> String {
    let lib = dir.join("ct.rs");
    std::fs::write(&lib, src).unwrap();
    let out = dir.join("ct.ll");
    let st = Command::new("rustc")
        .args(["--crate-type=lib", "--edition=2021", "-C", "opt-level=3", "-C", "panic=abort", "-C", "debuginfo=0", "--emit=llvm-ir", "-o"])
        .arg(&out)
        .arg(&lib)
        .output()
        .expect("run rustc");
    assert!(st.status.success(), "rustc failed: {}", String::from_utf8_lossy(&st.stderr));
    std::fs::read_to_string(out).unwrap()
}

/// The body lines of `define ... @name(` in an LLVM IR text.
fn define_body<'a>(ir: &'a str, name: &str) -> Vec<&'a str> {
    let mut lines = ir.lines().skip_while(|l| !(l.starts_with("define") && l.contains(&format!("@{name}("))));
    let head = lines.next().unwrap_or_else(|| panic!("no define for {name}"));
    let _ = head;
    lines.take_while(|l| *l != "}").collect()
}

fn ssa_names(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let j = i + 1 + s[i + 1..].find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.')).unwrap_or(s.len() - i - 1);
            out.push(s[i..j].to_string());
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

/// Tainted conditional sites + forbidden calls in one function's IR.
fn native_findings(body: &[&str]) -> Vec<String> {
    let mut tainted: HashSet<String> = HashSet::new();
    let mut defs: HashMap<String, (bool, Vec<String>)> = HashMap::new();
    for l in body {
        let t = l.trim();
        if let Some((lhs, rhs)) = t.split_once(" = ") {
            if lhs.starts_with('%') {
                let src = rhs.starts_with("load i8") || rhs.starts_with("load volatile i8");
                defs.insert(lhs.to_string(), (src, ssa_names(rhs)));
            }
        }
    }
    loop {
        let before = tainted.len();
        for (name, (src, uses)) in &defs {
            if *src || uses.iter().any(|u| tainted.contains(u)) {
                tainted.insert(name.clone());
            }
        }
        if tainted.len() == before {
            break;
        }
    }
    let mut out = Vec::new();
    for l in body {
        let t = l.trim();
        let cond = if let Some(r) = t.strip_prefix("br i1 ") {
            ssa_names(r).into_iter().next()
        } else if let Some(r) = t.strip_prefix("switch ") {
            ssa_names(r).into_iter().next()
        } else if let Some((_, r)) = t.split_once("select i1 ") {
            ssa_names(r).into_iter().next()
        } else {
            None
        };
        if cond.is_some_and(|c| tainted.contains(&c)) {
            out.push(format!("tainted condition: {t}"));
        }
        if t.contains("@bcmp(") || t.contains("@memcmp(") {
            out.push(format!("library compare: {t}"));
        }
    }
    out
}

#[test]
fn native_constant_time_compare_has_no_data_dependent_branch() {
    let dir = scratch("native");
    let bytes_fn = cut_fn("runtime/rs/src/bytes.rs", "almide_rt_bytes_is_equal_constant_time");
    let string_fn = cut_fn("runtime/rs/src/string.rs", "almide_rt_string_is_equal_constant_time");
    assert!(bytes_fn.contains("black_box") && string_fn.contains("black_box"), "the runtime fns lost their barrier");
    let ir = llvm_ir(&dir, &format!("#[no_mangle]\n{bytes_fn}\n#[no_mangle]\n{string_fn}\n"));
    for name in ["almide_rt_bytes_is_equal_constant_time", "almide_rt_string_is_equal_constant_time"] {
        let body = define_body(&ir, name);
        assert!(body.iter().any(|l| l.contains("load i8") || l.contains("load volatile i8")), "{name}: no byte load in the IR — the walk is not looking at the loop");
        let f = native_findings(&body);
        assert!(f.is_empty(), "{name} at opt-level 3 has a data-dependent branch or a library compare:\n{}", f.join("\n"));
    }
}

#[test]
fn control_native_early_exit_twins_are_caught() {
    let dir = scratch("native-ctl");
    let src = r#"
#[no_mangle]
pub fn twin_early(a: &Vec<u8>, b: &Vec<u8>) -> bool {
    if a.len() != b.len() { return false; }
    for i in 0..a.len() {
        if a[i] != b[i] { return false; }
    }
    true
}
#[no_mangle]
pub fn twin_plain(a: &Vec<u8>, b: &Vec<u8>) -> bool { a == b }
"#;
    let ir = llvm_ir(&dir, src);
    for name in ["twin_early", "twin_plain"] {
        let f = native_findings(&define_body(&ir, name));
        assert!(!f.is_empty(), "control {name}: an early-exit compare was NOT caught — the native walk is blind");
        eprintln!("control {name} caught: {f:?}");
    }
}
