//! The host-op ownership contract gate (#2739).
//!
//! `almide_mir::host_ops::HOST_OPS` admits a host op as an ORDINARY call: heap
//! arguments borrowed, a heap result fresh and owned. This test holds every row
//! to the two sources that make that true, so the table cannot drift from them:
//!
//! - the stdlib declaration: `@intrinsic("<symbol>")` sits on the row's
//!   `intrinsic_fn` in `stdlib/<module>.almd`, and a self-hosted wrapper's body
//!   calls that function;
//! - the native runtime body: `pub fn <symbol>(…)` in `runtime/rs/src/` takes
//!   no `&mut` and no owned heap value (each parameter is `&T`, a scalar, or a
//!   shared `Rc` closure handle the callee counts itself), and returns no
//!   reference.

use std::path::{Path, PathBuf};

use almide_mir::host_ops::HOST_OPS;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every `.rs` file under `runtime/rs/src`, concatenated.
fn runtime_source() -> String {
    let dir = root().join("runtime/rs/src");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .collect();
    files.sort();
    files.iter().map(|p| std::fs::read_to_string(p).expect("read runtime source")).collect::<Vec<_>>().join("\n")
}

/// Split `s` at commas that are not inside `<>`, `()` or `[]`.
fn split_top_level(s: &str) -> Vec<String> {
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
    out.push(cur);
    out.into_iter().map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect()
}

/// `(params, return type)` of `pub fn <symbol>(…) -> R {` in `src`.
fn runtime_signature(src: &str, symbol: &str) -> Option<(Vec<String>, Option<String>)> {
    let head = format!("pub fn {symbol}(");
    let start = src.find(&head)? + head.len();
    let rest = &src[start..];
    let mut depth = 1i32;
    let close = rest.char_indices().find_map(|(i, c)| {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        (depth == 0).then_some(i)
    })?;
    let params = split_top_level(&rest[..close]);
    let after = &rest[close + 1..];
    let body = after.find('{')?;
    let ret = after[..body].trim().strip_prefix("->").map(|r| r.trim().to_string());
    Some((params, ret))
}

const SCALARS: &[&str] = &["i64", "f64", "bool", "i32", "u8", "u32", "u64"];

/// Why `param` breaks the borrow contract, if it does.
fn param_breaks_borrow(param: &str) -> Option<String> {
    let ty = param.split_once(':').map(|(_, t)| t.trim()).unwrap_or(param);
    if ty.starts_with("&mut") {
        return Some(format!("`{param}` is written through (&mut) — not an ordinary borrowed argument"));
    }
    let shared_closure = ty.starts_with("std::rc::Rc<dyn Fn") || ty.starts_with("Rc<dyn Fn");
    if ty.starts_with('&') || SCALARS.contains(&ty) || shared_closure {
        return None;
    }
    Some(format!("`{param}` takes an owned value — the caller's argument would be moved, not borrowed"))
}

/// The attribute lines directly above `fn <name>(` in `src` (doc comments skipped).
fn attributes_of(src: &str, name: &str) -> Option<Vec<String>> {
    let lines: Vec<&str> = src.lines().collect();
    let decl = |l: &str| {
        let t = l.trim_start();
        let t = t.strip_prefix("effect ").unwrap_or(t);
        t.starts_with(&format!("fn {name}(")) || t.starts_with(&format!("fn {name}["))
    };
    let at = lines.iter().position(|l| decl(l))?;
    let mut attrs = Vec::new();
    for l in lines[..at].iter().rev() {
        let t = l.trim();
        if t.starts_with('@') {
            attrs.push(t.to_string());
        } else if t.starts_with("///") || t.starts_with("//") {
            continue;
        } else {
            break;
        }
    }
    Some(attrs)
}

/// The text of `fn <name>`'s declaration up to the next top-level declaration.
fn body_of<'a>(src: &'a str, name: &str) -> Option<&'a str> {
    let pos = src.find(&format!("fn {name}("))?;
    let rest = &src[pos..];
    let end = rest[1..]
        .find("\nfn ")
        .into_iter()
        .chain(rest[1..].find("\neffect fn "))
        .chain(rest[1..].find("\n@"))
        .min()
        .map_or(rest.len(), |e| e + 1);
    Some(&rest[..end])
}

#[test]
fn every_host_op_keeps_the_ordinary_call_contract() {
    let runtime = runtime_source();
    let mut faults = Vec::new();
    for op in HOST_OPS {
        let name = format!("{}.{}", op.module, op.func);
        let path = root().join(format!("stdlib/{}.almd", op.module));
        let stdlib = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let want = format!("@intrinsic(\"{}\")", op.symbol);
        match attributes_of(&stdlib, op.intrinsic_fn) {
            None => faults.push(format!("{name}: no `fn {}` in {}", op.intrinsic_fn, path.display())),
            Some(attrs) if !attrs.iter().any(|a| a == &want) => {
                faults.push(format!("{name}: `fn {}` does not carry {want} (has {attrs:?})", op.intrinsic_fn))
            }
            Some(_) => {}
        }
        if op.intrinsic_fn != op.func {
            match body_of(&stdlib, op.func) {
                Some(body) if body.contains(&format!("{}(", op.intrinsic_fn)) => {}
                _ => faults.push(format!("{name}: the wrapper does not call `{}`", op.intrinsic_fn)),
            }
        }
        let Some((params, ret)) = runtime_signature(&runtime, op.symbol) else {
            faults.push(format!("{name}: no `pub fn {}` in runtime/rs/src", op.symbol));
            continue;
        };
        faults.extend(params.iter().filter_map(|p| param_breaks_borrow(p)).map(|why| format!("{name} ({}): {why}", op.symbol)));
        if ret.as_deref().is_some_and(|r| r.starts_with('&')) {
            faults.push(format!("{name} ({}): returns a reference, not an owned result", op.symbol));
        }
    }
    assert!(faults.is_empty(), "host-op contract violations:\n  {}", faults.join("\n  "));
}

/// The gate's own negatives: the shapes it exists to refuse are refused.
#[test]
fn the_contract_check_refuses_what_it_should() {
    assert!(param_breaks_borrow("buf: &mut Vec<u8>").is_some());
    assert!(param_breaks_borrow("s: String").is_some());
    assert!(param_breaks_borrow("xs: Vec<i64>").is_some());
    assert!(param_breaks_borrow("s: &str").is_none());
    assert!(param_breaks_borrow("n: i64").is_none());
    assert!(param_breaks_borrow("f: std::rc::Rc<dyn Fn(String)>").is_none());
    let src = "pub fn almide_x(a: &str, m: &AlmideMap<String, String>, n: i64) -> Result<String, String> {\n}";
    let (params, ret) = runtime_signature(src, "almide_x").expect("parses");
    assert_eq!(params.len(), 3);
    assert_eq!(ret.as_deref(), Some("Result<String, String>"));
    let unit = runtime_signature("pub fn almide_y(c: &AlmideHttpCall) {\n}", "almide_y").expect("parses");
    assert_eq!(unit.1, None);
}
