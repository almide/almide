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

/// A type variable anywhere in the interface type `t`.
fn has_type_var(t: &Value) -> bool {
    match t {
        Value::Object(o) => o.get("kind").and_then(|k| k.as_str()) == Some("type_var") || o.values().any(has_type_var),
        Value::Array(a) => a.iter().any(has_type_var),
        _ => false,
    }
}

/// `module.fn` -> (signature, class, generic) for every public stdlib function.
fn surface() -> BTreeMap<String, (String, &'static str, bool)> {
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
            let generic = has_type_var(&f["params"]) || has_type_var(&f["return"]);
            out.insert(format!("{m}.{name}"), (sig(f), class, generic));
        }
    }
    for (m, t) in CHECKER_SURFACE {
        for u in UNITS {
            out.insert(format!("{m}.{u}"), (format!("(n: Int) -> {t}"), "pure", false));
        }
    }
    out
}

const EVIDENCE: &[&str] = &["stamp", "order", "parity", "complexity", "diamond", "alloc"];

/// The criteria the harness measures once per instantiation of a generic.
const HEAP_EVIDENCE: &[&str] = &["complexity", "alloc", "diamond"];

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
    let mut stale_impl = Vec::new();
    let mut missing_impl = Vec::new();
    let fp = Sources::load();
    for r in &rows {
        let name = r.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        *seen.entry(name.clone()).or_default() += 1;
        let status = r.get("status").and_then(|v| v.as_str()).unwrap_or("");
        let row_sig = r.get("sig").and_then(|v| v.as_str()).unwrap_or("");
        match want.get(&name) {
            None => problems.push(format!("{name}: no such public function (stale row)")),
            Some((s, ..)) if s != row_sig => {
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
                // A generic function is measured with its type variables at
                // Int AND at String (the 10-16 ruling): the heap-element path
                // is a different lowering from Int's, and an Int-only stamp
                // never ran it (wasm list.flat_map was O(n^2) bytes there).
                if want.get(&name).is_some_and(|w| w.2) {
                    for k in HEAP_EVIDENCE {
                        if !r.get(*k).and_then(|v| v.as_str()).is_some_and(|s| s.contains("[vars=String]")) {
                            problems.push(format!("{name}: generic, but `{k}` has no [vars=String] measurement"));
                        }
                    }
                }
                // The stamp names the implementation it inspected: a change to
                // it voids the stamp, as a signature change does.
                let now = impl_fingerprint(&fp, &name);
                match (r.get("impl").and_then(|v| v.as_str()), now.as_deref()) {
                    (Some(was), Some(now)) if was != now => {
                        problems.push(format!("{name}: implementation changed since the inspection ({was} -> {now}) — re-inspect"));
                        stale_impl.push(name.clone());
                    }
                    (None, Some(now)) => {
                        problems.push(format!("{name}: inspected without `impl` (now {now})"));
                        missing_impl.push((name.clone(), now.to_string()));
                    }
                    _ => {}
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

    let bless = std::env::var("ALMIDE_UPDATE_INSPECTION").is_ok();
    if bless && !(missing.is_empty() && stale_sig.is_empty() && stale_impl.is_empty() && missing_impl.is_empty()) {
        let mut out = text.clone();
        // A fresh stamp gets the fingerprint of what was just inspected.
        for (n, now) in &missing_impl {
            let start = out.find(&format!("name = \"{n}\"")).expect("row");
            let at = start + out[start..].find("status = \"inspected\"\n").expect("status") + "status = \"inspected\"\n".len();
            out.insert_str(at, &format!("impl = \"{now}\"\n"));
        }
        // A changed implementation voids the stamp, as a changed signature does.
        for n in stale_sig.iter().chain(&stale_impl) {
            // A changed signature voids the stamp: the row is rewritten whole.
            let start = out.find(&format!("name = \"{n}\"")).expect("row");
            let head = out[..start].rfind("[[fn]]").expect("row head");
            let end = out[start..].find("\n[[fn]]").map(|e| start + e + 1).unwrap_or(out.len());
            let (s, class, _) = &want[n];
            out.replace_range(head..end, &row(n, s, class));
        }
        for n in &missing {
            let (s, class, _) = &want[*n];
            out.push_str(&row(n, s, class));
        }
        std::fs::write(&path, out).expect("bless");
    }
    assert!(problems.is_empty(), "stdlib inspection ledger:\n  {}", problems.join("\n  "));
}

fn row(name: &str, sig: &str, class: &str) -> String {
    format!("\n[[fn]]\nname = \"{name}\"\nsig = {}\nclass = \"{class}\"\nstatus = \"uninspected\"\n", toml::Value::from(sig))
}

/// The sources one stdlib function's behaviour is read from, for the `impl`
/// fingerprint of a stamp:
///
/// - its declaration block in `stdlib/<module>.almd` (attributes, signature,
///   and a pure-Almide body);
/// - the native runtime fn its `@intrinsic` names, and that fn's `_owned`
///   twin, in `runtime/rs/src`;
/// - the self-host source file the wasm leg links for it
///   (`self_host_registry.rs`).
///
/// NOT covered: a dedicated lowering arm in crates/almide-wasm (e.g. the point
/// edits in `list_edit.rs`) cannot be cut per function. That gap is the
/// ledger header's, closed by re-inspecting every stamped row on a schedule.
struct Sources {
    runtime: String,
    self_host: BTreeMap<String, String>,
}

impl Sources {
    fn load() -> Sources {
        let mut runtime = String::new();
        let mut files: Vec<_> = std::fs::read_dir(root().join("runtime/rs/src"))
            .expect("runtime/rs/src")
            .filter_map(|e| Some(e.ok()?.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "rs"))
            .collect();
        files.sort();
        for f in files {
            runtime.push_str(&std::fs::read_to_string(f).expect("runtime source"));
        }
        // `(crate::embedded::SRC_<STEM>, &[("impl", "module.fn"), ...])`.
        let reg = std::fs::read_to_string(root().join("crates/almide-types/src/self_host_registry.rs")).expect("registry");
        let mut self_host = BTreeMap::new();
        let mut stem = String::new();
        for line in reg.lines() {
            if let Some(i) = line.find("embedded::SRC_") {
                stem = line[i + "embedded::SRC_".len()..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect::<String>().to_lowercase();
            }
            for part in line.split("\", \"").skip(1) {
                let public: String = part.chars().take_while(|c| *c != '"').collect();
                if public.contains('.') && !stem.is_empty() {
                    self_host.entry(public).or_insert_with(|| stem.clone());
                }
            }
        }
        Sources { runtime, self_host }
    }

    /// `fn <sym>` through its closing brace, if the runtime defines it.
    fn runtime_fn(&self, sym: &str) -> Option<&str> {
        let at = [format!("fn {sym}<"), format!("fn {sym}(")].iter().find_map(|h| self.runtime.find(h.as_str()))?;
        let open = at + self.runtime[at..].find('{')?;
        let mut depth = 0usize;
        for (i, c) in self.runtime[open..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(&self.runtime[at..=open + i]);
                    }
                }
                _ => {}
            }
        }
        None
    }
}

/// The declaration block of `fn func` in `src`: the attribute lines directly
/// above it (doc lines skipped), then every line to the next blank line.
fn decl_block(src: &str, func: &str) -> Option<String> {
    let lines: Vec<&str> = src.lines().collect();
    let at = lines.iter().position(|l| {
        let l = l.trim_start_matches("pub ").trim_start_matches("effect ");
        l.strip_prefix("fn ").is_some_and(|r| r.strip_prefix(func).is_some_and(|t| t.starts_with('[') || t.starts_with('(')))
    })?;
    let mut start = at;
    while start > 0 && (lines[start - 1].starts_with('@') || lines[start - 1].starts_with("///")) {
        start -= 1;
    }
    let end = (at..lines.len()).find(|&i| lines[i].trim().is_empty()).unwrap_or(lines.len());
    // Doc lines are prose: editing one does not change what was inspected.
    Some(lines[start..end].iter().filter(|l| !l.starts_with("///")).copied().collect::<Vec<_>>().join("\n"))
}

/// The sources a checker-surface constructor is made of: it has no stdlib
/// declaration — the checker resolves it from the unit table and the
/// lowering erases it to `n * factor` — so a change to either voids its stamp.
const CHECKER_SURFACE_SOURCES: &[&str] = &["crates/almide-types/src/time_units.rs", "crates/almide-frontend/src/lower/calls_time.rs"];

/// FNV-1a 64 over the function's sources ([`Sources`]): stable across
/// toolchains, unlike `DefaultHasher`. `None` when the function has no
/// declaration to read.
fn impl_fingerprint(src: &Sources, name: &str) -> Option<String> {
    let (module, func) = name.split_once('.')?;
    if CHECKER_SURFACE.iter().any(|(m, _)| *m == module) {
        let parts: Vec<String> = CHECKER_SURFACE_SOURCES.iter().map(|p| std::fs::read_to_string(root().join(p)).unwrap_or_default()).collect();
        return Some(fnv1a(&parts));
    }
    let decl = decl_block(&std::fs::read_to_string(root().join(format!("stdlib/{module}.almd"))).ok()?, func)?;
    let mut parts = vec![decl.clone()];
    if let Some(sym) = decl.lines().find_map(|l| l.strip_prefix("@intrinsic(\"")?.split('"').next()) {
        for s in [sym.to_string(), format!("{sym}_owned")] {
            if let Some(body) = src.runtime_fn(&s) {
                parts.push(body.to_string());
            }
        }
    }
    if let Some(stem) = src.self_host.get(name) {
        parts.push(std::fs::read_to_string(root().join(format!("stdlib/{stem}.almd"))).unwrap_or_default());
    }
    Some(fnv1a(&parts))
}

fn fnv1a(parts: &[String]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in parts.join("\u{0}").bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("fnv1a:{h:016x}")
}
