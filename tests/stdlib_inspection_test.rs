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
/// fingerprint of a stamp. The fingerprint covers a CLOSURE of source blocks,
/// followed from the function's declaration by the identifiers each block
/// names, to fixpoint — derived from the source, never from a list of names:
///
/// - Almide: the declaration block of `fn f` in `stdlib/<module>.almd`
///   (attributes, signature, a pure-Almide body), and every top-level item
///   (`fn`, `type`) of the SAME file a reached block references — calls
///   (`g(`), names a private `__` helper or a type anywhere (`|> __step`), bare
///   or qualified by the file's own module (`http.__with_path` inside
///   `http.almd`). So a rewritten private `__` helper voids every stamp that
///   reaches it, and only those. A call qualified by another module is that
///   module's own row; a parameter named like a function is not a reference.
/// - Native: for each `@intrinsic("sym")` a reached block declares, the
///   top-level `fn sym` and its `sym_owned` twin in `runtime/rs/src`, and every
///   top-level `fn` a reached body calls unqualified (not `.m(` / `T::m(`) and
///   `CONST` / `STATIC` it names, in the same file.
/// - Self-host: for each reached item `g` of `stdlib/<m>.almd` the wasm leg
///   links a pure-Almide body for (`self_host_registry.rs` rows, the
///   `self_host_fn_tables.rs` tables they name, and `@implements("m.g")`
///   declarations), that impl fn's block in its own file, and the items of
///   that file it reaches as above.
///
/// NOT covered: a dedicated lowering arm in crates/almide-wasm (e.g. the point
/// edits in `list_edit.rs`) cannot be cut per function; nor a runtime trait
/// method or macro-generated fn, a runtime helper in another file or passed
/// by name (`.map(helper)`), a non-`__` Almide fn passed by name, or a
/// private helper called through UFCS (`x.__h()`). That gap is the ledger
/// header's, closed by re-inspecting every stamped row on a schedule.
struct Sources {
    root: std::path::PathBuf,
    /// `stdlib/<stem>.almd` stem -> top-level item name -> block.
    almide: BTreeMap<String, BTreeMap<String, String>>,
    /// `runtime/rs/src/<file>` -> top-level item name -> block.
    rust: BTreeMap<String, BTreeMap<String, String>>,
    /// Call name (`m.f`, or a bare private `__f`) -> (stem, impl fn).
    self_host: BTreeMap<String, Vec<(String, String)>>,
}

/// One closure member: (source path, item name) -> its block.
type Closure = BTreeMap<(String, String), String>;

impl Sources {
    fn load() -> Sources {
        Sources::load_at(&root())
    }

    fn load_at(root: &std::path::Path) -> Sources {
        let read_dir = |dir: &str, ext: &str| -> Vec<(String, String)> {
            let mut files: Vec<_> = std::fs::read_dir(root.join(dir))
                .into_iter()
                .flatten()
                .filter_map(|e| Some(e.ok()?.path()))
                .filter(|p| p.extension().is_some_and(|x| x == ext))
                .collect();
            files.sort();
            files.iter().filter_map(|p| Some((p.file_name()?.to_str()?.to_string(), std::fs::read_to_string(p).ok()?))).collect()
        };
        let almide: BTreeMap<String, BTreeMap<String, String>> = read_dir("stdlib", "almd")
            .into_iter()
            .map(|(f, src)| (f.trim_end_matches(".almd").to_string(), almide_items(&src)))
            .collect();
        let rust = read_dir("runtime/rs/src", "rs").into_iter().map(|(f, src)| (format!("runtime/rs/src/{f}"), rust_items(&src))).collect();
        let mut self_host: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
        let read = |p: &str| std::fs::read_to_string(root.join(p)).unwrap_or_default();
        for (stem, imp, public) in registry(&read("crates/almide-types/src/self_host_registry.rs"), &read("crates/almide-types/src/self_host_fn_tables.rs")) {
            self_host.entry(public).or_default().push((stem, imp));
        }
        for (stem, items) in &almide {
            for (name, block) in items {
                for public in block.lines().filter_map(|l| l.strip_prefix("@implements(\"")?.split('"').next()) {
                    self_host.entry(public.to_string()).or_default().push((stem.clone(), name.clone()));
                }
            }
        }
        for v in self_host.values_mut() {
            v.sort();
            v.dedup();
        }
        Sources { root: root.to_path_buf(), almide, rust, self_host }
    }

    /// Every source block `module.func`'s behaviour is read from (see
    /// [`Sources`]); `None` when the module declares no such function.
    fn closure(&self, module: &str, func: &str) -> Option<Closure> {
        self.almide.get(module)?.get(func)?;
        let mut out = Closure::new();
        let mut work = vec![(format!("stdlib/{module}.almd"), func.to_string())];
        while let Some(key) = work.pop() {
            if out.contains_key(&key) {
                continue;
            }
            let (path, name) = &key;
            if let Some(stem) = path.strip_prefix("stdlib/").and_then(|p| p.strip_suffix(".almd")) {
                let Some(block) = self.almide.get(stem).and_then(|items| items.get(name)) else { continue };
                let items = &self.almide[stem];
                for (qual, id, called) in idents(block) {
                    // A call, a private `__` helper or a type: a parameter or
                    // local that shares a function's name is not a reference.
                    let referent = called || id.starts_with("__") || id.starts_with(|c: char| c.is_ascii_uppercase());
                    if referent && id != *name && items.contains_key(&id) && qual.as_deref().is_none_or(|q| q == stem) {
                        work.push((path.clone(), id));
                    }
                }
                for sym in block.lines().filter_map(|l| l.strip_prefix("@intrinsic(\"")?.split('"').next()) {
                    for s in [sym.to_string(), format!("{sym}_owned")] {
                        for (file, items) in &self.rust {
                            if items.contains_key(&s) {
                                work.push((file.clone(), s.clone()));
                            }
                        }
                    }
                }
                for call in [format!("{stem}.{name}"), name.clone()] {
                    for (s, imp) in self.self_host.get(&call).into_iter().flatten() {
                        work.push((format!("stdlib/{s}.almd"), imp.clone()));
                    }
                }
                out.insert(key.clone(), block.clone());
            } else {
                let Some(items) = self.rust.get(path) else { continue };
                let Some(block) = items.get(name) else { continue };
                for (qual, id, called) in idents(block) {
                    // A call, or a `CONST` / `STATIC`.
                    let referent = called || id.starts_with(|c: char| c.is_ascii_uppercase());
                    if referent && qual.is_none() && id != *name && items.contains_key(&id) {
                        work.push((path.clone(), id));
                    }
                }
                out.insert(key.clone(), block.clone());
            }
        }
        Some(out)
    }
}

/// The top-level items of an Almide source, name -> declaration block: the
/// attribute lines directly above (doc lines skipped), then every line to the
/// next blank line (stdlib bodies hold no blank line; a gate would notice the
/// fingerprint shrinking, not growing, if one did).
fn almide_items(src: &str) -> BTreeMap<String, String> {
    let lines: Vec<&str> = src.lines().collect();
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    for (at, line) in lines.iter().enumerate() {
        let mut l = *line;
        while let Some(r) = ["pub ", "effect ", "mod "].iter().find_map(|p| l.strip_prefix(p)) {
            l = r;
        }
        let Some(rest) = l.strip_prefix("fn ").or_else(|| l.strip_prefix("type ")) else { continue };
        let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        if name.is_empty() {
            continue;
        }
        let mut start = at;
        while start > 0 && (lines[start - 1].starts_with('@') || lines[start - 1].starts_with("///")) {
            start -= 1;
        }
        let end = (at..lines.len()).find(|&i| lines[i].trim().is_empty()).unwrap_or(lines.len());
        // Doc lines are prose: editing one does not change what was inspected.
        let block = lines[start..end].iter().filter(|l| !l.starts_with("///")).copied().collect::<Vec<_>>().join("\n");
        out.entry(name).and_modify(|b| b.push_str(&format!("\n{block}"))).or_insert(block);
    }
    out
}

/// The top-level `fn` / `const` / `static` items of a Rust source, name ->
/// text from the item's first line through its closing `}` or `;` (cfg twins
/// of one name are joined).
fn rust_items(src: &str) -> BTreeMap<String, String> {
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    let mut offset = 0;
    for line in src.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        let mut l = line;
        // An attribute on the item's own line: `#[inline(always)] pub fn ...`.
        while let Some(r) = l.strip_prefix("#[") {
            let mut depth = 1;
            let Some(close) = r.char_indices().find_map(|(i, c)| {
                depth += match c {
                    '[' => 1,
                    ']' => -1,
                    _ => 0,
                };
                (depth == 0).then_some(i)
            }) else {
                break;
            };
            l = r[close + 1..].trim_start();
        }
        while let Some(r) = ["pub(crate) ", "pub(super) ", "pub "].iter().find_map(|p| l.strip_prefix(p)) {
            l = r;
        }
        let data = l.starts_with("const ") || l.starts_with("static ");
        while let Some(r) = ["const ", "static ", "mut ", "unsafe ", "async ", "extern \"C\" "].iter().find_map(|p| l.strip_prefix(p)) {
            l = r;
        }
        let is_fn = l.starts_with("fn ");
        if !(is_fn || data) {
            continue;
        }
        let name: String = l.trim_start_matches("fn ").chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        if name.is_empty() {
            continue;
        }
        let Some(end) = rust_item_end(src, start, is_fn) else { continue };
        let text = &src[start..end];
        out.entry(name).and_modify(|b| b.push_str(&format!("\n{text}"))).or_insert_with(|| text.to_string());
    }
    out
}

/// The end of the Rust item starting at `from`: past the `}` closing a fn's
/// body, or the `;` ending a bodiless fn or a `const` / `static`, skipping
/// comments, strings and char literals.
fn rust_item_end(src: &str, from: usize, is_fn: bool) -> Option<usize> {
    let b = src.as_bytes();
    let is_id = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let (mut i, mut depth) = (from, 0i32);
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => i += src[i..].find('\n').unwrap_or(src.len() - i),
            b'/' if b.get(i + 1) == Some(&b'*') => i += src[i..].find("*/")? + 1,
            b'r' if matches!(b.get(i + 1), Some(b'"' | b'#')) && (i == 0 || !is_id(b[i - 1])) => {
                let hashes = src[i + 1..].bytes().take_while(|c| *c == b'#').count();
                if b.get(i + 1 + hashes) == Some(&b'"') {
                    let close = format!("\"{}", "#".repeat(hashes));
                    i += 2 + hashes + src[i + 2 + hashes..].find(&close)? + close.len() - 1;
                }
            }
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
            }
            b'\'' if b.get(i + 1) == Some(&b'\\') => i += 3 + src.get(i + 3..)?.find('\'')?,
            b'\'' if b.get(i + 2) == Some(&b'\'') => i += 2,
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' => depth -= 1,
            b'}' => {
                depth -= 1;
                if depth == 0 && is_fn {
                    return Some(i + 1);
                }
            }
            b';' if depth == 0 => return Some(i + 1),
            _ => {}
        }
        i += 1;
    }
    None
}

/// The identifiers of a block, each with the identifier qualifying it
/// (`q.id`), or `Some("")` after `::` / a non-identifier `.` receiver, and
/// whether it is called (`id(` or a turbofish `id::<`).
fn idents(block: &str) -> Vec<(Option<String>, String, bool)> {
    let b = block.as_bytes();
    let is_id = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if !(b[i].is_ascii_alphabetic() || b[i] == b'_') || (i > 0 && is_id(b[i - 1])) {
            i += 1;
            continue;
        }
        let start = i;
        while i < b.len() && is_id(b[i]) {
            i += 1;
        }
        let qual = if start >= 2 && &b[start - 2..start] == b"::" {
            Some(String::new())
        } else if start >= 1 && b[start - 1] == b'.' {
            let q = block[..start - 1].bytes().rev().take_while(|c| is_id(*c)).count();
            Some(block[start - 1 - q..start - 1].to_string())
        } else {
            None
        };
        let called = block[i..].starts_with('(') || block[i..].starts_with("::<");
        out.push((qual, block[start..i].to_string(), called));
    }
    out
}

/// `(stem, impl fn, call name)` for each row of the self-host registry: each
/// `crate::embedded::SRC_<STEM>` and the `("impl", "call")` pairs after it,
/// including those of a `self_host_fn_tables.rs` table it names.
fn registry(reg: &str, tables: &str) -> Vec<(String, String, String)> {
    let mut consts = BTreeMap::new();
    for part in tables.split("const ").skip(1) {
        let name: String = part.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        consts.insert(name, part.split("];").next().unwrap_or("").to_string());
    }
    let mut out = Vec::new();
    for part in reg.split("embedded::SRC_").skip(1) {
        let stem: String = part.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect::<String>().to_lowercase();
        let mut text = part.to_string();
        for (qual, id, _) in idents(part) {
            if let (None, Some(body)) = (qual, consts.get(&id)) {
                text.push_str(body);
            }
        }
        for (imp, call) in pairs(&text) {
            out.push((stem.clone(), imp, call));
        }
    }
    out
}

/// Every `("a", "b")` in `s`.
fn pairs(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(i) = rest.find("(\"") {
        rest = &rest[i + 2..];
        let Some((a, tail)) = rest.split_once('"') else { break };
        let Some(tail) = tail.trim_start().strip_prefix(',').map(str::trim_start).and_then(|t| t.strip_prefix('"')) else { continue };
        let Some((b, tail)) = tail.split_once('"') else { break };
        if tail.trim_start().starts_with(')') {
            out.push((a.to_string(), b.to_string()));
        }
    }
    out
}

/// The sources a checker-surface constructor is made of: it has no stdlib
/// declaration — the checker resolves it from the unit table and the
/// lowering erases it to `n * factor` — so a change to either voids its stamp.
const CHECKER_SURFACE_SOURCES: &[&str] = &["crates/almide-types/src/time_units.rs", "crates/almide-frontend/src/lower/calls_time.rs"];

/// FNV-1a 64 over the function's sources ([`Sources::closure`]): stable
/// across toolchains, unlike `DefaultHasher`. `None` when the function has
/// no declaration to read.
fn impl_fingerprint(src: &Sources, name: &str) -> Option<String> {
    let (module, func) = name.split_once('.')?;
    if CHECKER_SURFACE.iter().any(|(m, _)| *m == module) {
        let parts: Vec<String> = CHECKER_SURFACE_SOURCES.iter().map(|p| std::fs::read_to_string(src.root.join(p)).unwrap_or_default()).collect();
        return Some(fnv1a(&parts));
    }
    let parts: Vec<String> = src.closure(module, func)?.into_iter().map(|((path, item), block)| format!("{path}::{item}\n{block}")).collect();
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

/// Editing one source block changes the fingerprint of exactly the public
/// functions whose closure reaches it: through a chain of same-file private
/// helpers, a self-qualified call, an `@implements` body and its helpers, and
/// a native intrinsic's private Rust helper — and of no other function.
#[test]
fn a_helper_edit_voids_exactly_the_functions_that_reach_it() {
    const MODULE: &str = "\
fn a(x: Int) -> Int = __h1(x) + 1

fn b(x: Int) -> Int = __h2(x)

fn c(x: Int, len: Int) -> Int = x * 3 + len

fn d(x: Int) -> Int = m.__h1(x)

/// e's native body.
@intrinsic(\"almide_rt_m_e\")
fn e(x: Int) -> Int = _

@intrinsic(\"almide_rt_m_f\")
fn f(x: Int) -> Int = _

fn g(xs: List[Int]) -> Int = list.len(xs) + __lone_twin(xs)

fn len(xs: List[Int]) -> Int = 0

fn h(xs: List[Int]) -> List[Int] = list.map(xs, __inc)

fn __inc(x: Int) -> Int = x + 1

/// The shared helper.
fn __h1(x: Int) -> Int = __h2(x) + 1

fn __h2(x: Int) -> Int = x * 2

fn __lone(x: Int) -> Int = x

fn __lone_twin(xs: List[Int]) -> Int = 0
";
    const IMPL: &str = "\
@implements(\"m.f\")
fn m_f_impl(x: Int) -> Int = __g(x)

fn __g(x: Int) -> Int = x + 7

fn __unused(x: Int) -> Int = x
";
    const RUNTIME: &str = "\
fn helper(x: i64) -> i64 {
    x + LIMIT
}

const LIMIT: i64 = 1;

pub fn almide_rt_m_e(x: i64) -> i64 {
    // a brace in a char and in a string must not end the body: '{' \"{\"
    let _ = ('{', \"{\", b'\\'');
    helper(x)
}

fn other(x: i64) -> i64 { let s = \"}\"; x }

pub fn almide_rt_m_f(x: i64) -> i64 { x.other() }
";
    let fns = ["a", "b", "c", "d", "e", "f", "g", "h"];
    let fps = |files: &[(&str, &str)]| -> BTreeMap<&str, String> {
        let dir = tempfile::tempdir().expect("tempdir");
        for (p, text) in files {
            let path = dir.path().join(p);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        let src = Sources::load_at(dir.path());
        fns.iter().map(|f| (*f, impl_fingerprint(&src, &format!("m.{f}")).expect("declared"))).collect()
    };
    let tree = |module: &str, imp: &str, rt: &str| vec![("stdlib/m.almd", module.to_string()), ("stdlib/m_impl.almd", imp.to_string()), ("runtime/rs/src/m.rs", rt.to_string())];
    let run = |t: Vec<(&str, String)>| fps(&t.iter().map(|(p, s)| (*p, s.as_str())).collect::<Vec<_>>());
    let base = run(tree(MODULE, IMPL, RUNTIME));
    let edits: &[(&str, (&str, &str, &str), &[&str])] = &[
        ("__h2", ("stdlib/m.almd", "x * 2", "x * 4"), &["a", "b", "d"]),
        ("__h1", ("stdlib/m.almd", "__h2(x) + 1", "__h2(x) + 2"), &["a", "d"]),
        ("__lone", ("stdlib/m.almd", "fn __lone(x: Int) -> Int = x", "fn __lone(x: Int) -> Int = x + 1"), &[]),
        ("__lone_twin", ("stdlib/m.almd", "fn __lone_twin(xs: List[Int]) -> Int = 0", "fn __lone_twin(xs: List[Int]) -> Int = 1"), &["g"]),
        ("len", ("stdlib/m.almd", "fn len(xs: List[Int]) -> Int = 0", "fn len(xs: List[Int]) -> Int = 2"), &[]),
        ("__inc", ("stdlib/m.almd", "x + 1\n", "x + 2\n"), &["h"]),
        ("a doc line", ("stdlib/m.almd", "/// The shared helper.", "/// The shared helper, reworded."), &[]),
        ("__g", ("stdlib/m_impl.almd", "x + 7", "x + 8"), &["f"]),
        ("__unused", ("stdlib/m_impl.almd", "fn __unused(x: Int) -> Int = x", "fn __unused(x: Int) -> Int = x - 1"), &[]),
        ("helper", ("runtime/rs/src/m.rs", "x + LIMIT", "x + LIMIT + 1"), &["e"]),
        ("LIMIT", ("runtime/rs/src/m.rs", "LIMIT: i64 = 1", "LIMIT: i64 = 2"), &["e"]),
        ("other", ("runtime/rs/src/m.rs", "x }\n\npub", "x + 1 }\n\npub"), &[]),
    ];
    for (what, (file, from, to), want) in edits {
        let mut t = tree(MODULE, IMPL, RUNTIME);
        let entry = t.iter_mut().find(|(p, _)| p == file).unwrap();
        assert!(entry.1.contains(from), "{what}: edit anchor {from:?} not in {file}");
        entry.1 = entry.1.replacen(from, to, 1);
        let now = run(t);
        let changed: Vec<&str> = fns.iter().copied().filter(|f| base[f] != now[f]).collect();
        assert_eq!(changed, *want, "editing {what} changed the fingerprints of {changed:?}");
    }
}

/// A function's wasm lowering is part of what a stamp inspected: editing the
/// emitter arm that dispatches on the function's name, or a lowering helper
/// only stdlib arms reach, changes the fingerprint of exactly the functions
/// that arm serves (#3796). Shared emitter infrastructure (reached from
/// general expression lowering too) and other modules' arms move nothing.
#[test]
fn an_emitter_arm_edit_voids_exactly_the_functions_it_serves() {
    const MODULE: &str = "\
fn a(x: Int) -> Int = x + 1

fn b(x: Int, y: Int) -> Int = x + y

fn c(x: Int) -> Int = x * 2

fn d(x: Int) -> Int = x * 3

fn e(x: Int, y: Int) -> Int = x - y
";
    const EMITTER: &str = r#"
impl Emitter<'_> {
    pub(crate) fn lower_expr(&mut self, e: &IrExpr) -> ArmResult {
        self.lower_arg(e)?;
        match &e.kind {
            IrExprKind::Call { target, args, .. } => self.lower_module_call(target, args),
            _ => Ok(None),
        }
    }

    fn lower_module_call(&mut self, target: &CallTarget, args: &[IrExpr]) -> ArmResult {
        match target {
            CallTarget::Module { module, func, .. } if module.as_str() == "m" && func.as_str() == "c" => {
                self.lower_arg(&args[0])?;
                self.emit_c_kernel()
            }
            CallTarget::Module { module, func, .. } if module.as_str() == "m" => {
                self.lower_m_call(func.as_str(), args)
            }
            CallTarget::Module { module, func, .. } if module.as_str() == "n" => {
                self.lower_n_call(func.as_str(), args)
            }
            _ => self.lower_linked_call(args),
        }
    }

    fn lower_m_call(&mut self, func: &str, args: &[IrExpr]) -> ArmResult {
        match (func, args) {
            ("a", [x]) => self.lower_m_a(x),
            ("b" | "e", [x, y]) => self.lower_m_pair(func == "b", x, y),
            _ => self.lower_linked_call(args),
        }
    }

    fn lower_n_call(&mut self, func: &str, args: &[IrExpr]) -> ArmResult {
        match (func, args) {
            ("a", [x]) => self.lower_arg(x),
            _ => self.lower_linked_call(args),
        }
    }

    fn lower_m_a(&mut self, x: &IrExpr) -> ArmResult {
        self.lower_arg(x)?;
        self.zip_kernel(1)
    }

    fn lower_m_pair(&mut self, is_b: bool, x: &IrExpr, y: &IrExpr) -> ArmResult {
        self.lower_arg(x)?;
        self.lower_arg(y)?;
        self.zip_kernel(if is_b { 2 } else { 3 })
    }

    fn zip_kernel(&mut self, k: i32) -> ArmResult {
        self.f.instructions().i32_const(k);
        Ok(None)
    }

    fn emit_c_kernel(&mut self) -> ArmResult {
        self.f.instructions().i32_const(7);
        Ok(None)
    }

    fn lower_arg(&mut self, e: &IrExpr) -> ArmResult {
        self.f.instructions().i32_const(0);
        Ok(None)
    }

    fn lower_linked_call(&mut self, args: &[IrExpr]) -> ArmResult {
        Ok(None)
    }
}
"#;
    let fns = ["a", "b", "c", "d", "e"];
    let run = |emitter: &str| -> BTreeMap<&str, String> {
        let dir = tempfile::tempdir().expect("tempdir");
        for (p, text) in [("stdlib/m.almd", MODULE), ("crates/almide-wasm/src/calls.rs", emitter)] {
            let path = dir.path().join(p);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        let src = Sources::load_at(dir.path());
        fns.iter().map(|f| (*f, impl_fingerprint(&src, &format!("m.{f}")).expect("declared"))).collect()
    };
    let base = run(EMITTER);
    let edits: &[(&str, &str, &str, &[&str])] = &[
        ("the a arm", "(\"a\", [x]) => self.lower_m_a(x)", "(\"a\", [x]) => self.lower_m_a(x).map(|l| l)", &["a"]),
        ("a's own helper", "self.zip_kernel(1)", "self.zip_kernel(4)", &["a"]),
        ("the b|e arm", "self.lower_m_pair(func == \"b\", x, y)", "self.lower_m_pair(func != \"e\", x, y)", &["b", "e"]),
        ("the b|e helper", "if is_b { 2 } else { 3 }", "if is_b { 5 } else { 3 }", &["b", "e"]),
        ("a helper every m arm shares", "i32_const(k)", "i32_const(k + 1)", &["a", "b", "e"]),
        ("the m.c guard arm", "self.emit_c_kernel()\n", "self.emit_c_kernel().map(|l| l)\n", &["c"]),
        ("c's own helper", "i32_const(7)", "i32_const(8)", &["c"]),
        ("shared infrastructure", "i32_const(0)", "i32_const(1)", &[]),
        ("another module's arm of the same name", "(\"a\", [x]) => self.lower_arg(x)", "(\"a\", [x]) => self.lower_arg(x).map(|l| l)", &[]),
        (
            "the fallback",
            "fn lower_linked_call(&mut self, args: &[IrExpr]) -> ArmResult {\n        Ok(None)",
            "fn lower_linked_call(&mut self, args: &[IrExpr]) -> ArmResult {\n        Ok(Some(None))",
            &[],
        ),
    ];
    for (what, from, to, want) in edits {
        assert!(EMITTER.contains(from), "{what}: edit anchor {from:?} not in the emitter");
        let now = run(&EMITTER.replacen(from, to, 1));
        let changed: Vec<&str> = fns.iter().copied().filter(|f| base[f] != now[f]).collect();
        assert_eq!(changed, *want, "editing {what} changed the fingerprints of {changed:?}");
    }
}
