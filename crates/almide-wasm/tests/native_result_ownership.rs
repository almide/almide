//! #2004 — the RESULT-OWNERSHIP MATRIX of the stdlib surface, generated
//! from the committed signature index (`docs/stdlib/*.md`, the same index
//! `scripts/check-interface-diff.sh` reads), never hand-picked: every
//! module fn whose result is a droppable block (Str, Bytes, List of
//! scalars) and whose parameters this harness can synthesise gets one
//! row that binds the result in a loop and measures the high-water mark
//! at N=1000 vs N=8000.
//!
//! A growing row is a finding: an arm that hands back a FRESH block
//! without stamping it owned (`NATIVE_FRESH_RESULTS`, calls_modules.rs —
//! the bind's borrow +1 then leaves it at rc 1 forever), or a leak
//! inside the arm / linked body (#2005). The Koka / Lean convention is
//! "every result is owned, views are the declared exception"; here the
//! leak-safe default is borrow, and this matrix is what keeps the fresh
//! declaration from drifting: an undeclared fresh arm is a red row, a
//! wrongly declared view is a cross-target divergence.
//!
//! Known growing rows live in `tests/golden/native-own-known-leaks.txt`
//! (shrink-only: a row may only shrink or vanish; `ALMIDE_UPDATE_NATIVE_OWN=1`
//! regenerates after a fix). Rows the harness cannot run — a parameter
//! type it cannot synthesise, a domain error at the synthesised
//! arguments, a wall — are reported, not counted.

mod harness;
use harness::run_wasm;

use std::collections::BTreeMap;

/// Modules whose fns run without a host (auto-imported, plus the three
/// pure ones needing an explicit import).
const MODULES: &[&str] = &[
    "list", "int", "float", "string", "bytes", "math", "map", "set", "value", "option", "result",
    "error", "datetime", "int8", "int16", "int32", "int64", "uint8", "uint16", "uint32", "uint64",
    "float32", "float64", "json", "url", "regex", "base64",
];
const NEEDS_IMPORT: &[&str] = &["json", "url", "regex", "base64"];

fn droppable(ret: &str) -> Option<&'static str> {
    match ret.trim() {
        "String" => Some("string.len(t)"),
        "Bytes" => Some("bytes.len(t)"),
        "List[Int]" | "List[Float]" | "List[Bool]" | "List[Int8]" | "List[Int16]"
        | "List[Int32]" | "List[Int64]" | "List[UInt8]" | "List[UInt16]" | "List[UInt32]"
        | "List[UInt64]" | "List[Float32]" | "List[Float64]" | "List[String]" | "List[Bytes]"
        | "List[List[Int]]" | "List[(Int, Int)]" | "List[Int?]" => Some("list.len(t)"),
        // Flat-payload blocks (#2010 stage 1): released by $dec_flat; the
        // bind keeps `t` alive, the reducer needs nothing from it.
        "Option[Int]" | "Int?" | "Option[Float]" | "Float?" | "Option[Bool]" | "Bool?" | "(Int, Int)"
        | "(Int, Bool)" | "(Float, Float)" | "(Int, Float)" | "Result[Int, Int]" => Some("1"),
        _ => None,
    }
}

/// A literal of the parameter's type, or None when the harness has no
/// synthesis for it (generics, maps, options, records).
fn synth(ty: &str) -> Option<&'static str> {
    Some(match ty.trim() {
        "String" => "\"abc\"",
        "Int" | "Int64" => "2",
        "Int8" | "Int16" | "Int32" | "UInt8" | "UInt16" | "UInt32" | "UInt64" => "2",
        "Float" | "Float64" | "Float32" => "1.5",
        "Bool" => "true",
        "Bytes" => "bytes.from_string(\"abc\")",
        "List[Int]" => "[1, 2, 3]",
        "List[Float]" => "[1.5, 2.5]",
        "List[Bool]" => "[true, false]",
        "List[String]" => "[\"a\", \"b\"]",
        "List[Bytes]" => "[bytes.from_string(\"a\")]",
        "(Int) -> Int" => "(x) => x + 1",
        "(Int) -> Bool" => "(x) => x > 1",
        "(Int) -> String" => "(x) => int.to_string(x)",
        "(Int, Int) -> Int" => "(a, b) => a + b",
        "(Int, Int) -> Bool" => "(a, b) => a < b",
        "(String) -> String" => "(s) => s",
        "(String) -> Bool" => "(s) => true",
        "(String) -> Int" => "(s) => 1",
        "(Float) -> Float" => "(x) => x",
        "(Float) -> Bool" => "(x) => x > 1.0",
        _ => return None,
    })
}

/// Every single-letter type parameter becomes `Int`.
fn instantiate(sig: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = sig.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let prev_word = i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_');
        let next_word = i + 1 < chars.len() && (chars[i + 1].is_alphanumeric() || chars[i + 1] == '_');
        if c.is_ascii_uppercase() && !prev_word && !next_word {
            out.push_str("Int");
        } else {
            out.push(c);
        }
        i += 1;
    }
    out
}

/// Split a parameter list on top-level commas (types carry brackets).
fn split_params(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let (mut depth, mut cur) = (0i32, String::new());
    for c in s.chars() {
        match c {
            '[' | '(' => depth += 1,
            ']' | ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(cur.trim().to_string());
                cur.clear();
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

struct Row {
    name: String,
    module: &'static str,
    call: String,
    reduce: &'static str,
}

fn rows() -> (Vec<Row>, Vec<String>) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/stdlib");
    let mut rows = Vec::new();
    let mut skipped = Vec::new();
    for module in MODULES {
        let Ok(text) = std::fs::read_to_string(root.join(format!("{module}.md"))) else { continue };
        for line in text.lines() {
            let prefix = format!("{module}.");
            let Some(rest) = line.strip_prefix(&prefix) else { continue };
            let Some(open) = rest.find('(') else { continue };
            let Some(arrow) = rest.rfind(") -> ") else { continue };
            if arrow < open {
                continue;
            }
            let func = &rest[..open];
            if !func.chars().all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit()) {
                continue;
            }
            // A generic signature (`list.repeat(val: A, n: Int) -> List[A]`)
            // is measured at its Int instance — the structural leg's
            // droppable List is the scalar-element one, and a generic arm
            // that mis-declares its result hid behind the skip until the
            // list.repeat row of #2005.
            let params = instantiate(&rest[open + 1..arrow]);
            let ret = instantiate(&rest[arrow + 5..]);
            let (params, ret) = (params.as_str(), ret.as_str());
            let name = format!("{module}.{func}");
            let Some(reduce) = droppable(ret) else { continue };
            if params.contains("mut ") {
                skipped.push(format!("{name}: mut param"));
                continue;
            }
            let mut args = Vec::new();
            let mut bad = None;
            for p in split_params(params) {
                let ty = p.split_once(':').map(|(_, t)| t.trim()).unwrap_or(&p);
                match synth(ty) {
                    Some(a) => args.push(a),
                    None => {
                        bad = Some(ty.to_string());
                        break;
                    }
                }
            }
            if let Some(ty) = bad {
                skipped.push(format!("{name}: no synthesis for `{ty}`"));
                continue;
            }
            rows.push(Row { name, module, call: format!("{module}.{func}({})", args.join(", ")), reduce });
        }
    }
    (rows, skipped)
}

fn heap(n: u32, row: &Row) -> Result<u64, String> {
    let import = if NEEDS_IMPORT.contains(&row.module) { format!("import {}\n\n", row.module) } else { String::new() };
    let src = format!(
        r#"{import}effect fn main() -> Unit = {{
  var total = 0
  for i in 0..<{n} {{
    let t = {call}
    total = total + {reduce}
  }}
  println("${{total}}")
}}
"#,
        call = row.call,
        reduce = row.reduce
    );
    let ir = almide_spine::s5::lower_to_ir("native_own.almd", &src).map_err(|e| format!("front: {e}"))?;
    let bytes = almide_wasm::emit_program(&ir).map_err(|e| format!("wall: {e:?}"))?;
    let out = run_wasm(&bytes).map_err(|e| format!("run: {e}"))?;
    if out.exit != 0 {
        return Err(format!("exit {}: {}", out.exit, out.stderr.trim().lines().next().unwrap_or("")));
    }
    out.heap_end.ok_or_else(|| "no __heap".into())
}

fn ledger_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/native-own-known-leaks.txt")
}

fn read_ledger() -> BTreeMap<String, u64> {
    std::fs::read_to_string(ledger_path())
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| {
            let (k, v) = l.rsplit_once(' ')?;
            Some((k.trim().to_string(), v.trim().parse().ok()?))
        })
        .collect()
}

#[test]
fn stdlib_results_are_owned_by_their_binder() {
    let (rows, skipped) = rows();
    assert!(rows.len() >= 40, "the generated matrix collapsed ({} rows) — the signature index moved?", rows.len());
    let known = read_ledger();
    let mut growing: BTreeMap<String, u64> = BTreeMap::new();
    let mut unmeasured = Vec::new();
    let mut flat = 0usize;
    for row in &rows {
        match (heap(1000, row), heap(8000, row)) {
            (Ok(h1), Ok(h8)) => {
                let per_call = h8.saturating_sub(h1) / 7000;
                if h8 != h1 {
                    eprintln!("[native-own] GROWS {:<28} {per_call:>5} B/call", row.name);
                    growing.insert(row.name.clone(), per_call);
                } else {
                    flat += 1;
                }
            }
            (Err(e), _) | (_, Err(e)) => unmeasured.push(format!("{}: {e}", row.name)),
        }
    }
    eprintln!(
        "[native-own] {} rows: {flat} flat, {} growing, {} unmeasured, {} skipped",
        rows.len(),
        growing.len(),
        unmeasured.len(),
        skipped.len()
    );
    for u in &unmeasured {
        eprintln!("[native-own] unmeasured {u}");
    }
    if std::env::var("ALMIDE_UPDATE_NATIVE_OWN").is_ok() {
        let mut out = String::from(
            "# Stdlib fns whose bound result still grows the heap (B per call) — #2004 / #2005.\n# Shrink-only: a row may shrink or vanish, never grow or appear. ALMIDE_UPDATE_NATIVE_OWN=1 regenerates.\n",
        );
        for (k, v) in &growing {
            out.push_str(&format!("{k} {v}\n"));
        }
        std::fs::write(ledger_path(), out).expect("write ledger");
        return;
    }
    let mut bad = Vec::new();
    for (k, v) in &growing {
        match known.get(k) {
            Some(prev) if v <= prev => {}
            Some(prev) => bad.push(format!("{k}: {v} B/call, ledger says {prev}")),
            None => bad.push(format!("{k}: {v} B/call — NEW growing row (an unstamped fresh arm, or a leak inside it)")),
        }
    }
    assert!(
        bad.is_empty(),
        "stdlib results that grow the heap beyond the ledger:\n{}\n(a fix is ratified by ALMIDE_UPDATE_NATIVE_OWN=1 regeneration, never by hand)",
        bad.join("\n")
    );
}
