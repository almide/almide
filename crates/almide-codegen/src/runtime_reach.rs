//! What a runtime fn reaches, read off the embedded runtime source (C-460's
//! scan, #4918's library): each runtime fn's body, followed through the fns
//! its own module defines, against a set of std calls. Over-approximate by
//! construction: a helper name several fns share is all of them.
//!
//! #4918 reads it for the `rustc-wasip1` leg: `--target wasi` compiles the
//! native runtime whole for wasm32-wasip1, whose std implements a few calls as
//! an abort (no process id, no threads). A stdlib fn whose `@intrinsic`
//! reaches one is refused on that leg whether or not
//! proofs/target-availability.toml has a row for it — the row only words the
//! reason; an undeclared fn is not admitted.

use std::collections::{BTreeMap, HashMap, HashSet};

/// Each `fn` of `text` with a body, name -> its bodies joined.
pub fn fn_bodies(text: &str) -> HashMap<String, String> {
    let b = text.as_bytes();
    let mut out: HashMap<String, String> = HashMap::new();
    let mut i = 0;
    while let Some(at) = text[i..].find("fn ").map(|k| k + i) {
        i = at + 3;
        if at > 0 && (b[at - 1].is_ascii_alphanumeric() || b[at - 1] == b'_') { continue; }
        let name: String = text[i..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        if name.is_empty() { continue; }
        let (mut j, mut depth) = (i + name.len(), 0i32);
        while j < b.len() && !(depth == 0 && (b[j] == b'{' || b[j] == b';')) {
            match b[j] { b'(' | b'[' => depth += 1, b')' | b']' => depth -= 1, _ => {} }
            j += 1;
        }
        if j >= b.len() || b[j] == b';' { continue; }
        let (mut k, mut braces) = (j, 0i32);
        while k < b.len() {
            match b[k] { b'{' => braces += 1, b'}' => { braces -= 1; if braces == 0 { break; } } _ => {} }
            k += 1;
        }
        out.entry(name).or_default().push_str(&text[j..k.min(b.len() - 1) + 1]);
    }
    out
}

/// The names `body` calls: `f(`, `.f(`, `m::f(`, `f::<..>(`.
pub fn called_names(body: &str) -> impl Iterator<Item = &str> {
    crate::rust_idents::identifiers(body).filter(move |id| {
        let end = id.as_ptr() as usize - body.as_ptr() as usize + id.len();
        let rest = &body[end..];
        rest.starts_with('(') || rest.starts_with("::<")
    })
}

/// The fns of `modules` (`(module, text)`) whose body — or a fn of the same
/// module it calls, transitively — contains one of `apis`, with its module.
pub fn reaching_fns<'a>(modules: &[(&'a str, &str)], apis: &[&str]) -> BTreeMap<String, &'a str> {
    let mut out = BTreeMap::new();
    for (module, text) in modules {
        let bodies = fn_bodies(text);
        for name in bodies.keys() {
            let mut seen = HashSet::new();
            let mut stack = vec![name.as_str()];
            while let Some(n) = stack.pop() {
                if !seen.insert(n) { continue; }
                let Some(b) = bodies.get(n) else { continue };
                if apis.iter().any(|t| b.contains(t)) {
                    out.insert(name.clone(), *module);
                    break;
                }
                stack.extend(called_names(b).filter(|c| bodies.contains_key(*c)));
            }
        }
    }
    out
}

/// The std calls wasm32-wasip1's std implements as an abort: `process::id`
/// panics ("no pids on this platform") and `thread::spawn` unwraps a spawn
/// the platform always refuses. (A call that returns an `Unsupported` error
/// instead is served: the program sees the err, as on any host that refuses.)
pub const WASIP1_ABORTING_CALLS: &[&str] = &["std::process::id(", "std::thread::spawn("];

/// `module.fn` -> its `@intrinsic` symbol, for every stdlib declaration in
/// `sources` whose runtime symbol `reached` holds.
pub fn intrinsic_fns_reaching(sources: &[(&str, &str)], reached: &BTreeMap<String, &str>) -> BTreeMap<String, String> {
    use almide_lang::ast::{AttrValue, Decl};
    let mut out = BTreeMap::new();
    for (module, text) in sources {
        let Ok(program) = almide_lang::parser::Parser::new(almide_lang::lexer::Lexer::tokenize(text)).parse() else {
            panic!("bundled module `{module}` does not parse; its intrinsics cannot be read")
        };
        for d in &program.decls {
            let Decl::Fn { name, attrs, .. } = d else { continue };
            let Some(symbol) = attrs.iter().find(|a| a.name.as_str() == "intrinsic").and_then(|a| match a.args.first().map(|x| &x.value) {
                Some(AttrValue::String { value }) => Some(value.as_str()),
                _ => None,
            }) else {
                continue;
            };
            if reached.contains_key(symbol) {
                out.insert(format!("{module}.{name}"), symbol.to_string());
            }
        }
    }
    out
}

/// The stdlib fns `--target wasi` cannot serve, derived: `module.fn` -> the
/// runtime symbol whose body reaches a [`WASIP1_ABORTING_CALLS`] call.
pub fn wasip1_aborting_fns() -> &'static BTreeMap<String, String> {
    static FNS: std::sync::OnceLock<BTreeMap<String, String>> = std::sync::OnceLock::new();
    FNS.get_or_init(|| {
        let reached = reaching_fns(crate::generated::rust_runtime::RUST_RUNTIME_MODULES, WASIP1_ABORTING_CALLS);
        let sources: Vec<(&str, &str)> = almide_lang::stdlib_info::BUNDLED_MODULES
            .iter()
            .filter_map(|&m| almide_lang::stdlib_info::bundled_source(m).map(|s| (m, s)))
            .collect();
        intrinsic_fns_reaching(&sources, &reached)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Calibration: the one fn the availability table declares on the
    /// `rustc-wasip1` leg today is found by the derivation.
    #[test]
    fn the_derivation_finds_process_pid() {
        let fns = wasip1_aborting_fns();
        assert_eq!(fns.get("process.pid").map(String::as_str), Some("almide_rt_process_pid"), "{fns:?}");
    }

    /// The control: an intrinsic that reaches an aborting call through a
    /// helper of its module is found; a pure one and an `Unsupported`-err one
    /// are not.
    #[test]
    fn an_intrinsic_reaching_an_aborting_call_through_a_helper_is_found() {
        let rt = "pub fn almide_rt_x_a() -> i64 { helper() }\nfn helper() -> i64 { std::process::id() as i64 }\n\
                  pub fn almide_rt_x_b(n: i64) -> i64 { n + 1 }\n\
                  pub fn almide_rt_x_c() -> bool { std::net::TcpStream::connect(\"a:1\").is_ok() }\n\
                  pub fn almide_rt_x_d() { std::thread::spawn(|| ()); }\n";
        let reached = reaching_fns(&[("x", rt)], WASIP1_ABORTING_CALLS);
        let src = "@intrinsic(\"almide_rt_x_a\")\nfn a() -> Int = _\n@intrinsic(\"almide_rt_x_b\")\nfn b(n: Int) -> Int = _\n\
                   @intrinsic(\"almide_rt_x_c\")\nfn c() -> Bool = _\n@intrinsic(\"almide_rt_x_d\")\nfn d() -> Unit = _\n";
        let fns = intrinsic_fns_reaching(&[("x", src)], &reached);
        assert_eq!(fns.keys().map(String::as_str).collect::<Vec<_>>(), vec!["x.a", "x.d"]);
    }
}
