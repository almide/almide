//! Generate `src/generated/capability_bounds.rs`: which type parameters of a
//! native-intrinsic stdlib function need equality or ordering.
//!
//! The requirement is already declared, once, where the native leg needs it:
//! the runtime fn's generic bounds (`almide_rt_list_dedup<T: Clone +
//! PartialEq>`). An element type that cannot meet the bound — a function, or
//! a record holding one (`http.HttpRoute`) — passed `almide check` and then
//! failed rustc (`can't compare HttpRoute with HttpRoute`). The checker reads
//! this table to refuse such a call at check time, for every intrinsic that
//! carries the bound, instead of keeping a list of function names.
//!
//! A text scan, as the other tables here are: the `@intrinsic("sym")` line
//! directly above a stdlib `fn name[A, B](..)`, and the generic list of
//! `fn sym<..>` in `runtime/rs/src/*.rs`. Generics pair up by POSITION (the
//! runtime fns are written with the stdlib signature's generics first, in its
//! order); a bounded runtime generic past the stdlib's list must be a
//! `?Sized` lookup key (`map.get`'s `Q`), and anything else fails the build.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

struct Row {
    module: String,
    func: String,
    generic: usize,
    cap: &'static str,
}

/// The generic names of `fn name[A, B](...)` (empty when it has none).
fn stdlib_fn(line: &str) -> Option<(String, Vec<String>)> {
    let after = line.trim().strip_prefix("effect ").unwrap_or(line.trim());
    let after = after.strip_prefix("fn ")?;
    let name: String = after.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    let rest = &after[name.len()..];
    let generics = match rest.strip_prefix('[') {
        Some(g) => top_level_split(&g[..g.find(']')?]).into_iter().map(|p| head(&p)).collect(),
        None => Vec::new(),
    };
    Some((name, generics))
}

/// `A: Ord + Clone` -> `A`.
fn head(param: &str) -> String {
    param.split(':').next().unwrap_or("").trim().to_string()
}

fn top_level_split(s: &str) -> Vec<String> {
    let (mut depth, mut cur, mut out) = (0i32, String::new(), Vec::new());
    for c in s.chars() {
        match c {
            '<' | '(' | '[' => depth += 1,
            '>' | ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                out.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out.into_iter().map(|p| p.trim().to_string()).collect()
}

/// The generic params of `fn sym<..>(` in the runtime source, lifetimes
/// dropped. `None` when the runtime fn is not generic or not found.
fn runtime_generics(runtime: &str, sym: &str) -> Option<Vec<String>> {
    let at = runtime.find(&format!("fn {sym}<"))?;
    let open = at + "fn ".len() + sym.len() + 1;
    let mut depth = 1i32;
    let mut end = open;
    for (i, c) in runtime[open..].char_indices() {
        match c {
            '<' => depth += 1,
            '>' if runtime[open..open + i].ends_with('-') => {}
            '>' => {
                depth -= 1;
                if depth == 0 {
                    end = open + i;
                    break;
                }
            }
            _ => {}
        }
    }
    Some(top_level_split(&runtime[open..end]).into_iter().filter(|p| !p.starts_with('\'')).collect())
}

/// The capabilities a runtime bound list demands of its type.
fn caps(bounds: &str) -> Vec<&'static str> {
    let words: Vec<&str> = bounds
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty())
        .collect();
    let mut out = Vec::new();
    if words.iter().any(|w| matches!(*w, "PartialEq" | "Eq")) {
        out.push("Eq");
    }
    if words.iter().any(|w| matches!(*w, "Ord" | "PartialOrd")) {
        out.push("Ord");
    }
    out
}

fn scan(path: &Path, runtime: &str) -> Vec<Row> {
    let module = path.file_stem().expect("stem").to_string_lossy().to_string();
    let src = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mut out = Vec::new();
    let mut intrinsic: Option<String> = None;
    for line in src.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("@intrinsic(") {
            intrinsic = Some(rest.trim_end_matches(')').trim().trim_matches('"').to_string());
            continue;
        }
        if t.starts_with('@') || t.starts_with("//") {
            continue;
        }
        let Some(sym) = intrinsic.take() else { continue };
        let Some((func, generics)) = stdlib_fn(t) else { continue };
        let Some(rt) = runtime_generics(runtime, &sym) else { continue };
        for (i, param) in rt.iter().enumerate() {
            let bounds = param.split_once(':').map(|(_, b)| b).unwrap_or("");
            for cap in caps(bounds) {
                if i >= generics.len() {
                    assert!(
                        bounds.contains("?Sized"),
                        "{module}.{func}: runtime `{sym}` bounds its generic #{i} ({param}) by {cap}, \
                         and the stdlib signature has no generic there — put the stdlib's generics first, in order"
                    );
                    continue;
                }
                out.push(Row { module: module.clone(), func: func.clone(), generic: i, cap });
            }
        }
    }
    out
}

pub fn generate(root: &Path, files: &[PathBuf], out_path: &Path) {
    let runtime: String = std::fs::read_dir(root.join("runtime/rs/src"))
        .map(|rd| {
            let mut paths: Vec<PathBuf> = rd
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "rs"))
                .collect();
            paths.sort();
            paths.iter().map(|p| std::fs::read_to_string(p).unwrap_or_default()).collect()
        })
        .unwrap_or_default();
    if runtime.is_empty() {
        // No runtime source beside the checkout: keep the committed table.
        return;
    }
    let mut rows: Vec<Row> = files.iter().flat_map(|f| scan(f, &runtime)).collect();
    rows.sort_by(|a, b| (&a.module, &a.func, a.generic, a.cap).cmp(&(&b.module, &b.func, b.generic, b.cap)));
    rows.dedup_by(|a, b| (&a.module, &a.func, a.generic, a.cap) == (&b.module, &b.func, b.generic, b.cap));
    let mut out = String::from(
        "// @generated by crates/almide-types/buildscript/capability_bounds.rs from the runtime's generic bounds — do not edit.\n\n",
    );
    out.push_str("pub const BOUNDS: &[Bound] = &[\n");
    for r in &rows {
        writeln!(
            out,
            "    Bound {{ module: {:?}, func: {:?}, generic: {}, cap: Cap::{} }},",
            r.module, r.func, r.generic, r.cap
        )
        .expect("write");
    }
    out.push_str("];\n");
    super::in_checkout::write_if_changed(out_path, &out);
}
