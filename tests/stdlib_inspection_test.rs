//! The stdlib inspection ledger (`proofs/stdlib-inspection.toml`): one row per
//! public stdlib function, and an `inspected` stamp only on a row whose
//! evidence is filled in. The rules are in the ledger's header.
//!
//! This test holds the ledger to the stdlib as it is: every public function
//! (from `almide compile <module> --json`, the same extraction the docs-gen
//! count uses, plus the checker-surface `compute` / `duration` constructors)
//! has exactly one row, no row names a function that no longer exists, and a
//! row's `sig` is the function's current signature, so a signature change
//! clears the stamp and the function goes back to the queue. The number of
//! uninspected rows may only fall (`uninspected_ceiling`).
//!
//! `ALMIDE_UPDATE_INSPECTION=1` appends a skeleton row for each function the
//! ledger lacks and rewrites stale signatures as uninspected, then fails, so
//! the change is reviewed rather than silently accepted.
use std::collections::BTreeMap;
use std::process::Command;

use serde_json::Value;

const LEDGER: &str = "proofs/stdlib-inspection.toml";

/// Checker-surface constructors with no stdlib module (docs/stdlib/compute.md,
/// docs/stdlib/duration.md): `ns / us / ms / s / min / h`, each `(Int) -> T`.
const CHECKER_SURFACE: &[(&str, &str)] = &[("compute", "Compute"), ("duration", "Duration")];
const UNITS: &[&str] = &["ns", "us", "ms", "s", "min", "h"];

fn root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn ty(t: &Value) -> String {
    let inner = |k: &str| ty(&t[k]);
    match t["kind"].as_str().unwrap_or("?") {
        "int" => "Int".into(),
        "float" => "Float".into(),
        "bool" => "Bool".into(),
        "string" => "String".into(),
        "unit" => "Unit".into(),
        "bytes" => "Bytes".into(),
        "matrix" => "Matrix".into(),
        "type_var" => t["name"].as_str().unwrap_or("?").into(),
        "named" => {
            let name = t["name"].as_str().unwrap_or("?").to_string();
            match t["args"].as_array() {
                Some(a) if !a.is_empty() => format!("{name}[{}]", a.iter().map(ty).collect::<Vec<_>>().join(", ")),
                _ => name,
            }
        }
        "list" => format!("List[{}]", inner("inner")),
        "set" => format!("Set[{}]", inner("inner")),
        "option" => format!("Option[{}]", inner("inner")),
        "map" => format!("Map[{}, {}]", inner("key"), inner("value")),
        "result" => format!("Result[{}, {}]", inner("ok"), inner("err")),
        "tuple" => format!(
            "({})",
            t["elements"].as_array().map(|a| a.iter().map(ty).collect::<Vec<_>>().join(", ")).unwrap_or_default()
        ),
        "fn" => format!(
            "({}) -> {}",
            t["params"].as_array().map(|a| a.iter().map(ty).collect::<Vec<_>>().join(", ")).unwrap_or_default(),
            inner("return")
        ),
        other => format!("<{other}>"),
    }
}

fn sig(f: &Value) -> String {
    let params = f["params"]
        .as_array()
        .map(|a| a.iter().map(|p| format!("{}: {}", p["name"].as_str().unwrap_or("_"), ty(&p["type"]))).collect::<Vec<_>>().join(", "))
        .unwrap_or_default();
    let effect = if f["effect"].as_bool() == Some(true) { "effect " } else { "" };
    let err = if f["error"].is_object() { format!(" ! {}", ty(&f["error"])) } else { String::new() };
    format!("{effect}({params}) -> {}{err}", ty(&f["return"]))
}

/// `module.fn` -> (signature, class) for every public stdlib function.
fn surface() -> BTreeMap<String, (String, &'static str)> {
    let mut out = BTreeMap::new();
    let mut modules: Vec<String> = std::fs::read_dir(root().join("docs/stdlib"))
        .expect("docs/stdlib")
        .filter_map(|e| e.ok()?.path().file_stem()?.to_str().map(String::from))
        .filter(|m| m != "README" && !CHECKER_SURFACE.iter().any(|(c, _)| c == m))
        .collect();
    modules.sort();
    modules.dedup();
    for m in &modules {
        let o = Command::new(env!("CARGO_BIN_EXE_almide"))
            .args(["compile", m, "--json"])
            .current_dir(root())
            .output()
            .expect("almide compile");
        if !o.status.success() {
            continue; // a docs page that is not a module (index pages); the count gate owns that list
        }
        let v: Value = serde_json::from_slice(&o.stdout).expect("interface json");
        for f in v["functions"].as_array().into_iter().flatten() {
            let name = f["name"].as_str().unwrap_or("");
            if name.starts_with("__") {
                continue;
            }
            let class = if f["effect"].as_bool() == Some(true) { "effect" } else { "pure" };
            out.insert(format!("{m}.{name}"), (sig(f), class));
        }
    }
    for (m, t) in CHECKER_SURFACE {
        for u in UNITS {
            out.insert(format!("{m}.{u}"), (format!("(n: Int) -> {t}"), "pure"));
        }
    }
    out
}

const EVIDENCE: &[&str] = &["stamp", "order", "parity", "complexity", "diamond", "alloc"];

#[test]
fn every_public_stdlib_function_has_one_ledger_row_and_stamps_carry_evidence() {
    let path = root().join(LEDGER);
    let text = std::fs::read_to_string(&path).expect("ledger");
    let ledger: toml::Table = toml::from_str(&text).expect("ledger parses");
    let rows = ledger.get("fn").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let want = surface();
    assert!(want.len() > 900, "suspiciously small surface: {}", want.len());

    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut problems = Vec::new();
    let mut uninspected = 0usize;
    let mut stale_sig = Vec::new();
    for r in &rows {
        let name = r.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        *seen.entry(name.clone()).or_default() += 1;
        let status = r.get("status").and_then(|v| v.as_str()).unwrap_or("");
        let row_sig = r.get("sig").and_then(|v| v.as_str()).unwrap_or("");
        match want.get(&name) {
            None => problems.push(format!("{name}: no such public function (stale row)")),
            Some((s, _)) if s != row_sig => {
                problems.push(format!("{name}: sig {row_sig:?} is now {s:?} — re-inspect"));
                stale_sig.push(name.clone());
            }
            _ => {}
        }
        match status {
            "uninspected" => uninspected += 1,
            "inspected" => {
                for k in EVIDENCE {
                    if r.get(*k).and_then(|v| v.as_str()).is_none_or(|s| s.trim().is_empty()) {
                        problems.push(format!("{name}: inspected without `{k}`"));
                    }
                }
            }
            other => problems.push(format!("{name}: status {other:?} (inspected | uninspected)")),
        }
    }
    for (n, c) in &seen {
        if *c > 1 {
            problems.push(format!("{n}: {c} rows"));
        }
    }
    let missing: Vec<&String> = want.keys().filter(|n| !seen.contains_key(*n)).collect();
    for n in &missing {
        problems.push(format!("{n}: no row"));
    }
    let ceiling: usize = text
        .lines()
        .find_map(|l| l.strip_prefix("# uninspected_ceiling = ")?.trim().trim_matches('"').parse().ok())
        .expect("# uninspected_ceiling = \"N\" in the header");
    if uninspected > ceiling {
        problems.push(format!("{uninspected} uninspected rows over the ceiling {ceiling} — the ceiling only falls"));
    }

    if std::env::var("ALMIDE_UPDATE_INSPECTION").is_ok() && (!missing.is_empty() || !stale_sig.is_empty()) {
        let mut out = text.clone();
        for n in &stale_sig {
            // A changed signature voids the stamp: the row is rewritten whole.
            let start = out.find(&format!("name = \"{n}\"")).expect("row");
            let head = out[..start].rfind("[[fn]]").expect("row head");
            let end = out[start..].find("\n[[fn]]").map(|e| start + e + 1).unwrap_or(out.len());
            let (s, class) = &want[n];
            out.replace_range(head..end, &row(n, s, class));
        }
        for n in &missing {
            let (s, class) = &want[*n];
            out.push_str(&row(n, s, class));
        }
        std::fs::write(&path, out).expect("bless");
    }
    assert!(problems.is_empty(), "stdlib inspection ledger:\n  {}", problems.join("\n  "));
}

fn row(name: &str, sig: &str, class: &str) -> String {
    format!("\n[[fn]]\nname = \"{name}\"\nsig = {}\nclass = \"{class}\"\nstatus = \"uninspected\"\n", toml::Value::from(sig))
}
