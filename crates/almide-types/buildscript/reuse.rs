//! Generate `src/generated/reuse.rs`: the `@reuse(p)` declarations of the
//! stdlib, as one table both legs read (ruling 2026-10-11).
//!
//! `@reuse(p)` on a stdlib function says: when the argument for `p` is at
//! its last use, the result may take over its storage. The native leg
//! realises that either because the runtime fn already takes `p` by value
//! (the declaration also says `@consume(p)`) or through the runtime's
//! `<intrinsic>_owned` twin. A declaration with neither fails the build here,
//! naming the twin to write. The wasm leg reads the same table to decide
//! which reads a dying var may move into.
//!
//! This is a text scan, as the embedded table above is: the attribute lines
//! directly above a `fn`, the fn's name and its parameter names. The scan is
//! checked against the real parser by `crates/almide-codegen/tests/reuse_decl_test.rs`.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

struct Decl {
    module: String,
    func: String,
    param: usize,
    pname: String,
    arity: usize,
    intrinsic: String,
    consumes: bool,
}

/// `@name(a, b)` -> ("name", ["a", "b"]); `@name("s")` -> ("name", ["s"]).
fn attr(line: &str) -> Option<(&str, Vec<String>)> {
    let rest = line.trim().strip_prefix('@')?;
    let (name, args) = match rest.split_once('(') {
        Some((n, a)) => (n.trim(), a.rsplit_once(')').map(|(a, _)| a).unwrap_or(a)),
        None => (rest.trim(), ""),
    };
    let args = args
        .split(',')
        .map(|a| a.trim().trim_matches('"').to_string())
        .filter(|a| !a.is_empty())
        .collect();
    Some((name, args))
}

/// The parameter names of `fn name[..](p: T, q: (A) -> B) -> R = …`.
fn params(sig: &str) -> Option<(String, Vec<String>)> {
    let after = sig.trim().strip_prefix("fn ")?;
    let name: String = after
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    let open = after.find('(')?;
    let (mut depth, mut cur, mut out) = (0i32, String::new(), Vec::new());
    for c in after[open + 1..].chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' if depth > 0 => depth -= 1,
            ')' => break,
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
    let names = out
        .iter()
        .map(|p| p.split(':').next().unwrap_or("").trim().to_string())
        .collect();
    Some((name, names))
}

fn scan(path: &Path) -> Vec<Decl> {
    let module = path
        .file_stem()
        .expect("stem")
        .to_string_lossy()
        .to_string();
    let src =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mut out = Vec::new();
    let mut attrs: Vec<(String, Vec<String>)> = Vec::new();
    for line in src.lines() {
        let t = line.trim();
        if let Some((n, a)) = t.starts_with('@').then(|| attr(t)).flatten() {
            attrs.push((n.to_string(), a));
            continue;
        }
        if t.starts_with("///") || t.starts_with("//") {
            continue;
        }
        let attrs_here = std::mem::take(&mut attrs);
        let Some(reuse) = attrs_here.iter().find(|(n, _)| n == "reuse") else {
            continue;
        };
        let (func, names) = params(t)
            .unwrap_or_else(|| panic!("{}: @reuse is not above a `fn`: {t}", path.display()));
        let [p] = &reuse.1[..] else {
            panic!("{module}.{func}: @reuse takes exactly one parameter name")
        };
        let param = names
            .iter()
            .position(|n| n == p)
            .unwrap_or_else(|| panic!("{module}.{func}: @reuse({p}) names no parameter"));
        let intrinsic = attrs_here
            .iter()
            .find(|(n, _)| n == "intrinsic")
            .and_then(|(_, a)| a.first().cloned())
            .unwrap_or_else(|| {
                panic!("{module}.{func}: @reuse needs an @intrinsic (the native leg's realisation)")
            });
        let consumes = attrs_here
            .iter()
            .any(|(n, a)| n == "consume" && a.iter().any(|x| x == p));
        out.push(Decl {
            module: module.clone(),
            func,
            param,
            pname: p.clone(),
            arity: names.len(),
            intrinsic,
            consumes,
        });
    }
    out
}

pub fn generate(root: &Path, files: &[PathBuf], out_path: &Path) {
    let runtime: String = std::fs::read_dir(root.join("runtime/rs/src"))
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.path().extension().is_some_and(|x| x == "rs"))
                .map(|e| std::fs::read_to_string(e.path()).unwrap_or_default())
                .collect()
        })
        .unwrap_or_default();
    println!(
        "cargo:rerun-if-changed={}",
        root.join("runtime/rs/src").display()
    );
    let mut decls: Vec<Decl> = files.iter().flat_map(|f| scan(f)).collect();
    decls.sort_by(|a, b| (&a.module, &a.func).cmp(&(&b.module, &b.func)));
    let mut out = String::from(
        "// @generated by crates/almide-types/buildscript/reuse.rs from the stdlib's @reuse declarations — do not edit.\n\n",
    );
    out.push_str("pub const REUSE: &[Reuse] = &[\n");
    for d in &decls {
        let twin = if d.consumes {
            "None".to_string()
        } else {
            let twin = format!("{}_owned", d.intrinsic);
            if !runtime.is_empty()
                && !runtime.contains(&format!("fn {twin}<"))
                && !runtime.contains(&format!("fn {twin}("))
            {
                panic!(
                    "{}.{}: @reuse({}) has no native realisation — write `{twin}` in runtime/rs/src (taking the source by value), or declare @consume({}) if the runtime fn already takes it by value",
                    d.module, d.func, d.pname, d.pname
                );
            }
            format!("Some({twin:?})")
        };
        writeln!(
            out,
            "    Reuse {{ module: {:?}, func: {:?}, param: {}, arity: {}, intrinsic: {:?}, owned_twin: {twin} }},",
            d.module, d.func, d.param, d.arity, d.intrinsic
        )
        .expect("write");
    }
    out.push_str("];\n");
    super::in_checkout::write_if_changed(out_path, &out);
}
