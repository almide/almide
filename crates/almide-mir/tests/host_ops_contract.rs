//! The host-op contract gate (#2739, #3302).
//!
//! `almide_mir::host_ops::HOST_OPS` admits a host op as an ORDINARY call: heap
//! arguments borrowed, a heap result fresh and owned, its capabilities counted
//! at the call site. This test holds the table to the sources that make that
//! true, so it cannot drift from them:
//!
//! - the stdlib declaration: `@intrinsic("<symbol>")` sits on the row's
//!   `intrinsic_fn` in `stdlib/<module>.almd`, a self-hosted wrapper's body
//!   calls that function, and `returns_result` matches the declared return;
//! - the native runtime body: `pub fn <symbol>(…)` in `runtime/rs/src/` takes
//!   no `&mut` and no owned heap value outside the row's `by_value` copies
//!   (each parameter is `&T`, a scalar, or a closure the callee invokes), and
//!   returns no reference;
//! - the MATRIX: every `@intrinsic` of every capability module is a host op,
//!   pure, or walled — an unlisted capability-bearing intrinsic fails, the
//!   shape `env.millis` / `datetime.now` had before #3302 (pinned negative
//!   below) — and every stdlib module is either wholly pure or a capability
//!   module.

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

/// A runtime signature: its parameters, the generic parameters bound by a
/// closure trait (`F: Fn(A, String) -> A`), and its return type.
struct Sig {
    params: Vec<String>,
    closure_generics: Vec<String>,
    ret: Option<String>,
}

/// The text between `open` at `rest[0]` and its matching close.
fn balanced(rest: &str, open: char, close: char) -> Option<&str> {
    let mut depth = 0i32;
    for (i, c) in rest.char_indices() {
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Some(&rest[1..i]);
            }
        }
    }
    None
}

/// The signature of `pub fn <symbol>[<generics>](…) -> R [where …] {` in `src`.
fn runtime_signature(src: &str, symbol: &str) -> Option<Sig> {
    let head = format!("pub fn {symbol}");
    let start = src.match_indices(&head).map(|(i, _)| i + head.len()).find(|&i| {
        matches!(src[i..].chars().next(), Some('(') | Some('<'))
    })?;
    let mut rest = &src[start..];
    let mut closure_generics = Vec::new();
    if rest.starts_with('<') {
        let generics = balanced(rest, '<', '>')?;
        rest = &rest[generics.len() + 2..];
        closure_generics.extend(closure_bound_names(generics));
    }
    let params_text = balanced(rest, '(', ')')?;
    let after = &rest[params_text.len() + 2..];
    let body = after.find('{')?;
    let tail = &after[..body];
    let (ret_text, where_text) = tail.split_once("where").unwrap_or((tail, ""));
    closure_generics.extend(closure_bound_names(where_text));
    let ret = ret_text.trim().strip_prefix("->").map(|r| r.trim().to_string());
    Some(Sig { params: split_top_level(params_text), closure_generics, ret })
}

/// The names in a generics / where list bound by a closure trait (`F: Fn(..)`).
fn closure_bound_names(list: &str) -> Vec<String> {
    split_top_level(list)
        .into_iter()
        .filter_map(|g| {
            let (name, bound) = g.split_once(':')?;
            let bound = bound.trim();
            (bound.starts_with("Fn(") || bound.starts_with("FnMut(")).then(|| name.trim().to_string())
        })
        .collect()
}

const SCALARS: &[&str] = &["i64", "f64", "bool", "i32", "u8", "u32", "u64"];

/// Why parameter `param` breaks the borrow contract, if it does. `by_value`:
/// the row declares this parameter handed over as a copy.
fn param_breaks_borrow(param: &str, closure_generics: &[String], by_value: bool) -> Option<String> {
    let ty = param.split_once(':').map(|(_, t)| t.trim()).unwrap_or(param);
    if ty.starts_with("&mut") {
        return Some(format!("`{param}` is written through (&mut) — not an ordinary borrowed argument"));
    }
    let shared_closure = ty.starts_with("std::rc::Rc<dyn Fn") || ty.starts_with("Rc<dyn Fn");
    let generic_closure = closure_generics.iter().any(|g| g == ty) || ty.starts_with("impl Fn");
    if ty.starts_with('&') || SCALARS.contains(&ty) || shared_closure || generic_closure || by_value {
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

/// The declared return type of `fn <name>(…) -> R = …` in `src` (the text
/// between the parameter list's `->` and the body's `=`, across lines).
fn declared_return(src: &str, name: &str) -> Option<String> {
    let head = format!("fn {name}");
    let at = src.match_indices(&head).map(|(i, _)| i).find(|&i| {
        (i == 0 || matches!(src.as_bytes()[i - 1], b'\n' | b' '))
            && matches!(src[i + head.len()..].chars().next(), Some('(') | Some('['))
    })?;
    let mut rest = &src[at + head.len()..];
    if rest.starts_with('[') {
        let generics = balanced(rest, '[', ']')?;
        rest = &rest[generics.len() + 2..];
    }
    let params = balanced(rest, '(', ')')?;
    let after = &rest[params.len() + 2..];
    let eq = after.find(" =")?;
    after[..eq].trim().strip_prefix("->").map(|r| r.trim().to_string())
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
        match declared_return(&stdlib, op.func) {
            None => faults.push(format!("{name}: no declaration of `fn {}` to read its return type", op.func)),
            Some(ret) if (ret.starts_with("Result[") && !ret.ends_with('?')) != op.returns_result => faults.push(format!(
                "{name}: returns_result is {} but the declaration returns `{ret}`",
                op.returns_result
            )),
            Some(_) => {}
        }
        if op.intrinsic_fn != op.func {
            match body_of(&stdlib, op.func) {
                Some(body) if body.contains(&format!("{}(", op.intrinsic_fn)) => {}
                _ => faults.push(format!("{name}: the wrapper does not call `{}`", op.intrinsic_fn)),
            }
        }
        let Some(sig) = runtime_signature(&runtime, op.symbol) else {
            faults.push(format!("{name}: no `pub fn {}` in runtime/rs/src", op.symbol));
            continue;
        };
        for (i, p) in sig.params.iter().enumerate() {
            if let Some(why) = param_breaks_borrow(p, &sig.closure_generics, op.by_value.contains(&i)) {
                faults.push(format!("{name} ({}): {why}", op.symbol));
            }
        }
        faults.extend(
            op.by_value
                .iter()
                .filter(|i| **i >= sig.params.len())
                .map(|i| format!("{name}: by_value names parameter {i}, the signature has {}", sig.params.len())),
        );
        if sig.ret.as_deref().is_some_and(|r| r.starts_with('&') || r == "!") {
            faults.push(format!("{name} ({}): returns a reference (or never returns), not an owned result", op.symbol));
        }
    }
    assert!(faults.is_empty(), "host-op contract violations:\n  {}", faults.join("\n  "));
}

/// The gate's own negatives: the shapes it exists to refuse are refused.
#[test]
fn the_contract_check_refuses_what_it_should() {
    assert!(param_breaks_borrow("buf: &mut Vec<u8>", &[], false).is_some());
    assert!(param_breaks_borrow("s: String", &[], false).is_some());
    assert!(param_breaks_borrow("xs: Vec<i64>", &[], false).is_some());
    assert!(param_breaks_borrow("init: A", &[], false).is_some());
    assert!(param_breaks_borrow("init: A", &[], true).is_none());
    assert!(param_breaks_borrow("s: &str", &[], false).is_none());
    assert!(param_breaks_borrow("n: i64", &[], false).is_none());
    assert!(param_breaks_borrow("f: std::rc::Rc<dyn Fn(String)>", &[], false).is_none());
    assert!(param_breaks_borrow("f: F", &["F".to_string()], false).is_none());
    let src = "pub fn almide_x(a: &str, m: &AlmideMap<String, String>, n: i64) -> Result<String, String> {\n}";
    let sig = runtime_signature(src, "almide_x").expect("parses");
    assert_eq!(sig.params.len(), 3);
    assert_eq!(sig.ret.as_deref(), Some("Result<String, String>"));
    let unit = runtime_signature("pub fn almide_y(c: &AlmideHttpCall) {\n}", "almide_y").expect("parses");
    assert_eq!(unit.ret, None);
    let generic = runtime_signature(
        "pub fn almide_z<A: Clone, F: Fn(A, String) -> A>(p: &str, init: A, f: F) -> Result<A, String> {\n}",
        "almide_z",
    )
    .expect("parses");
    assert_eq!(generic.closure_generics, vec!["F".to_string()]);
    assert_eq!(generic.params.len(), 3);
    // A prefix of another symbol is not that symbol.
    assert!(runtime_signature("pub fn almide_xy(a: &str) {\n}", "almide_x").is_none());
}

/// The capability-module intrinsics of one module source that the matrix does
/// not classify (`host_ops::classify` → `Unclassified`).
fn matrix_faults(module: &str, src: &str) -> Vec<String> {
    let lines: Vec<&str> = src.lines().collect();
    let mut faults = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        let t = l.strip_prefix("effect ").unwrap_or(l);
        let Some(rest) = t.strip_prefix("fn ") else { continue };
        let func: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        let intrinsic = lines[..i]
            .iter()
            .rev()
            .take_while(|p| p.starts_with('@') || p.starts_with("//"))
            .any(|p| p.starts_with("@intrinsic"));
        if intrinsic && almide_mir::host_ops::classify(module, &func) == almide_mir::host_ops::Cell::Unclassified {
            faults.push(format!("{module}.{func}: a capability-module intrinsic that is neither a host op, pure, nor walled"));
        }
    }
    faults
}

fn stdlib_source(module: &str) -> String {
    let path = root().join(format!("stdlib/{module}.almd"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The runtime surface as a matrix (#3302): every `@intrinsic` of every
/// capability module is a host op (capabilities counted), pure, or walled.
#[test]
fn every_capability_module_intrinsic_is_classified() {
    let faults: Vec<String> = almide_mir::host_ops::CAP_MODULES
        .iter()
        .flat_map(|m| matrix_faults(m, &stdlib_source(m)))
        .collect();
    assert!(faults.is_empty(), "unclassified capability-bearing intrinsics:\n  {}", faults.join("\n  "));
}

/// The negative: a capability-bearing intrinsic the table does not list fails
/// the matrix — the shape `env.millis` had before #3302.
#[test]
fn an_unlisted_capability_bearing_intrinsic_fails_the_matrix() {
    let src = "/// Reads the clock.\n@intrinsic(\"almide_rt_env_fake_clock\")\neffect fn fake_clock() -> Int = _\n";
    let faults = matrix_faults("env", src);
    assert_eq!(faults.len(), 1, "{faults:?}");
    assert!(faults[0].starts_with("env.fake_clock"));
    // A listed row in the same shape passes.
    let listed = "@intrinsic(\"almide_rt_env_millis\")\neffect fn millis() -> Int = _\n";
    assert!(matrix_faults("env", listed).is_empty());
}

/// Every stdlib module is either wholly pure or a capability module, never both.
#[test]
fn stdlib_modules_partition_into_pure_and_capability() {
    use almide_lang::stdlib_info::{BUNDLED_MODULES, STDLIB_MODULES};
    let pure = almide_mir::purity::PURE_MODULES;
    let cap = almide_mir::host_ops::CAP_MODULES;
    for m in STDLIB_MODULES.iter().chain(BUNDLED_MODULES).filter(|m| **m != "prim") {
        assert!(pure.contains(m) != cap.contains(m), "module `{m}` must be in exactly one of PURE_MODULES / CAP_MODULES");
    }
}

/// A named WALLED cell names a real declaration (no stale rows).
#[test]
fn walled_cells_name_real_declarations() {
    for (module, func, _) in almide_mir::host_ops::WALLED.iter().filter(|(_, f, _)| *f != "*") {
        let src = stdlib_source(module);
        assert!(attributes_of(&src, func).is_some(), "WALLED names {module}.{func}, which is not declared");
    }
}

/// End to end (#3302): the witness producer `almide verify` runs counts the
/// clock reads of `env.millis` / `datetime.now` in the caps witness of the
/// function that makes them — `<declared>|<used>`, Clock is id 5.
#[test]
fn the_caps_witness_of_a_clock_reader_names_the_clock() {
    let src = r#"import env
effect fn main() -> Unit = {
  let a = env.millis()
  let b = datetime.now()
  if a >= 0 and b >= 0 then println("ok") else println("no")
}
"#;
    let w = almide_mir::pipeline::program_witnesses(src, &[], None).expect("the program lowers");
    let main = w.functions.iter().find(|f| f.name == "main").expect("main is certified, not walled");
    let used = main.caps.split_once('|').map(|(_, u)| u).unwrap_or("");
    assert!(used.split_whitespace().any(|id| id == "5"), "Clock (5) missing from main's used caps: `{}`", main.caps);
}
