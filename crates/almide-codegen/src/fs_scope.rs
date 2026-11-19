//! Shared filesystem permission guards for both native code generators.
//! The scoped host implementation is trusted runtime code; caller ownership
//! witnesses retain the original named host-operation identities.
mod fs_scope_twin;

pub const SCOPE_MARKER: &str = "// __ALMIDE_FS_SCOPE_INSTALLED__";

/// One scope's guards and source-derived host twins. Roots are absolute,
/// resolved by the project loader. Only generated user calls are rewritten.
pub struct FsScope {
    pub prelude: String,
    guards: std::collections::HashMap<String, &'static str>,
    twinned: std::collections::BTreeSet<String>,
}

impl FsScope {
    /// `Err` names a string-taking fs runtime fn `calls` reaches that no
    /// declaration guards: a scoped build refuses it rather than call it
    /// unguarded (#4825).
    pub fn new(roots: &[String], rt_path: &str, rt_use: &str, calls: &str) -> Result<Self, String> {
        let reached: std::collections::HashSet<_> = crate::rust_idents::identifiers(calls).collect();
        let mut guards = fs_runtime_guards();
        let unguarded = unguarded(&reached, &guards);
        if !unguarded.is_empty() {
            return Err(format!(
                "[permissions] fs: the program reaches {} of the fs runtime, which no declaration marks with its path (@fs_path / @fs_glob / @fs_temp / @fs_root), so it cannot be bounded",
                unguarded.join(", ")
            ));
        }
        guards.retain(|name, _| reached.contains(name.as_str()));
        let entries = guards.iter().filter(|(_, g)| **g != "__almide_fs_root")
            .map(|(n, _)| n.clone()).collect();
        let (twin, twinned) = fs_scope_twin::scoped_twin(fs_runtime_source(), &entries, rt_use);
        let list: Vec<_> = roots.iter().map(|r| format!("{r:?}")).collect();
        let guards_src = FS_SCOPE_GUARDS.replace("__ALMIDE_RT::", rt_path);
        let prelude = format!("\n{SCOPE_MARKER}\nconst __ALMIDE_FS_SCOPE_ROOTS: &[&str] = &[{}];\n{guards_src}{twin}", list.join(", "));
        Ok(Self { prelude, guards, twinned })
    }

    /// Does `code` call anything the scope bounds — a guarded runtime fn, or
    /// a string-taking fs runtime fn (which [`FsScope::new`] then refuses)?
    pub fn applies_to(code: &str) -> bool {
        let guards = fs_runtime_guards();
        let fs_fns = fs_runtime_str_fns();
        crate::rust_idents::identifiers(code).any(|id| guards.contains_key(id) || fs_fns.iter().any(|f| f == id))
    }

    pub fn wrap_calls(&self, code: &str) -> String {
        wrap_fs_calls(code, &self.guards, &self.twinned)
    }
}

/// The guard macros and the scope cell `inject_fs_scope` splices in. The
/// scope's own items (`fs_scope_*`, `fs_scoped_*`, `almide_sfs`) are the
/// runtime's: fs.rs includes them through almide-rt-core fs_beneath_core.rs.
const FS_SCOPE_GUARDS: &str = r#"static __ALMIDE_FS_SCOPE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
fn __almide_fs_scope_check(path: &str) -> Result<(), String> {
    __ALMIDE_FS_SCOPE.get_or_init(|| {
        let raw: Vec<String> = __ALMIDE_FS_SCOPE_ROOTS.iter().map(|r| r.to_string()).collect();
        __ALMIDE_RT::fs_scoped_install(Some(&raw));
    });
    if __ALMIDE_RT::fs_scoped_admits(path) { Ok(()) } else { Err(__ALMIDE_RT::fs_scope_refusal(path)) }
}
fn __almide_fs_fix<T, E: std::ops::Deref<Target = str> + From<String>>(r: Result<T, E>, path: &str) -> Result<T, E> {
    match r { Err(e) if __ALMIDE_RT::fs_scoped_refused_in(&e) => Err(E::from(__ALMIDE_RT::fs_scope_refusal(path))), r => r }
}
macro_rules! __almide_fs_path { ($f:expr, $p:expr $(, $a:expr)* $(,)?) => {{ let __almide_p = $p; let __almide_s = String::from({ let path: &str = &__almide_p; path }); match __almide_fs_scope_check(&__almide_s) { Ok(()) => __almide_fs_fix(($f)(__almide_p $(, $a)*), &__almide_s), Err(e) => Err(e.into()) } }}; }
macro_rules! __almide_fs_bool { ($f:expr, $p:expr $(,)?) => {{ let __almide_p = $p; if __almide_fs_scope_check(&*__almide_p).is_ok() { ($f)(__almide_p) } else { false } }}; }
macro_rules! __almide_fs_two { ($f:expr, $p:expr, $q:expr $(,)?) => {{ let (__almide_p, __almide_q) = ($p, $q); let __almide_s = String::from({ let path: &str = &__almide_p; path }); match __almide_fs_scope_check(&__almide_s).and_then(|()| __almide_fs_scope_check(&*__almide_q)) { Ok(()) => __almide_fs_fix(($f)(__almide_p, __almide_q), &__almide_s), Err(e) => Err(e.into()) } }}; }
macro_rules! __almide_fs_glob { ($f:expr, $p:expr $(,)?) => {{ let __almide_p = $p; let __almide_s = __ALMIDE_RT::fs_scope_glob_base(&*__almide_p); match __almide_fs_scope_check(&__almide_s) { Ok(()) => __almide_fs_fix(($f)(__almide_p), &__almide_s), Err(e) => Err(e.into()) } }}; }
macro_rules! __almide_fs_root { ($f:expr, $p:expr $(, $a:expr)* $(,)?) => {{ let __almide_p = $p; let __almide_s = String::from((&*__almide_p).split('\0').next().unwrap_or("")); match __almide_fs_scope_check(&__almide_s) { Ok(()) => __almide_fs_fix(($f)(__almide_p $(, $a)*), &__almide_s), Err(e) => Err(e.into()) } }}; }
macro_rules! __almide_fs_temp { ($f:expr, $p:expr $(,)?) => {{ let __almide_p = $p; let __almide_s = std::env::temp_dir().to_string_lossy().into_owned(); match __almide_fs_scope_check(&__almide_s) { Ok(()) => __almide_fs_fix(($f)(__almide_p), &__almide_s), Err(e) => Err(e.into()) } }}; }
"#;

/// The fs runtime module's embedded source (includes inlined).
pub(crate) fn fs_runtime_source() -> &'static str {
    crate::generated::rust_runtime::RUST_RUNTIME_MODULES.iter().find(|(m, _)| *m == "fs").map_or("", |(_, s)| *s)
}

/// The guard macro of each runtime symbol that takes a path, read off the
/// bundled stdlib's declarations (#4825): an `@intrinsic` fn marked
/// `@fs_path(p)` (a path the call touches; `@fs_path(src, dst)` two),
/// `@fs_glob(p)` (a pattern, judged by its literal base), `@fs_temp(p)` (a
/// call that creates in the temp dir) or `@fs_root(p)` (a rooted call's
/// root). The marked parameters lead the signature in that order — a guard
/// takes the path first. A runtime helper's `Send`-free twin
/// (`runtime_fn_modes::send_free_twin`, where the lowering routes a
/// non-`Send` value) takes its helper's guard. A malformed mark panics
/// (`fs_runtime_guards_from` says why): a mark that cannot be read guards
/// nothing, so it must not build.
pub(crate) fn fs_runtime_guards() -> std::collections::HashMap<String, &'static str> {
    static GUARDS: std::sync::OnceLock<std::collections::HashMap<String, &'static str>> = std::sync::OnceLock::new();
    GUARDS
        .get_or_init(|| {
            let sources: Vec<(&str, &str)> = almide_lang::stdlib_info::BUNDLED_MODULES
                .iter()
                .filter_map(|&m| almide_lang::stdlib_info::bundled_source(m).map(|s| (m, s)))
                .collect();
            fs_runtime_guards_from(&sources).unwrap_or_else(|e| panic!("[permissions] fs guards: {e}"))
        })
        .clone()
}

/// The path marks [`fs_runtime_guards`] reads.
pub const FS_PATH_MARKS: &[&str] = &["fs_path", "fs_glob", "fs_temp", "fs_root"];

/// [`fs_runtime_guards`] over `sources` (`(module, text)`).
pub fn fs_runtime_guards_from(sources: &[(&str, &str)]) -> Result<std::collections::HashMap<String, &'static str>, String> {
    use almide_lang::ast::{AttrValue, Decl, TypeExpr};
    let mut out: std::collections::HashMap<String, &'static str> = std::collections::HashMap::new();
    for (module, text) in sources {
        let tokens = almide_lang::lexer::Lexer::tokenize(text);
        let program = almide_lang::parser::Parser::new(tokens).parse().map_err(|_| format!("bundled module `{module}` does not parse"))?;
        for d in &program.decls {
            let Decl::Fn { name, attrs, params, return_type, .. } = d else { continue };
            let marks: Vec<_> = attrs.iter().filter(|a| FS_PATH_MARKS.contains(&a.name.as_str())).collect();
            let mark = match marks.as_slice() {
                [] => continue,
                [m] => *m,
                _ => return Err(format!("{module}.{name} carries {} path marks; one says how its path is judged", marks.len())),
            };
            let at = |what: &str| format!("{module}.{name} `@{}`: {what}", mark.name.as_str());
            let symbol = attrs
                .iter()
                .find(|a| a.name.as_str() == "intrinsic")
                .and_then(|a| a.args.first())
                .and_then(|a| match &a.value {
                    AttrValue::String { value } => Some(value.clone()),
                    _ => None,
                })
                .ok_or_else(|| at("marks a fn that is not an `@intrinsic`"))?;
            let named: Vec<String> = mark
                .args
                .iter()
                .map(|a| match &a.value {
                    AttrValue::Ident { name } => Ok(name.as_str().to_string()),
                    _ => Err(at("names its path parameters by identifier")),
                })
                .collect::<Result<_, _>>()?;
            let leading: Vec<&str> = params.iter().take(named.len()).map(|p| p.name.as_str()).collect();
            if named.is_empty() || !named.iter().map(String::as_str).eq(leading.iter().copied()) {
                return Err(at(&format!("names {named:?}, but a guard takes the leading parameters {leading:?}")));
            }
            let returns_bool = matches!(return_type, TypeExpr::Simple { name } if name.as_str() == "Bool");
            let guard = match (mark.name.as_str(), named.len()) {
                ("fs_path", 1) if returns_bool => "__almide_fs_bool",
                ("fs_path", 1) => "__almide_fs_path",
                ("fs_path", 2) if !returns_bool => "__almide_fs_two",
                ("fs_glob", 1) if params.len() == 1 => "__almide_fs_glob",
                ("fs_temp", 1) if params.len() == 1 => "__almide_fs_temp",
                ("fs_root", 1) => "__almide_fs_root",
                _ => return Err(at("has no guard of that shape")),
            };
            let mut entries = vec![symbol.clone()];
            entries.extend(crate::generated::runtime_fn_modes::send_free_twin(&symbol).map(str::to_string));
            for e in entries {
                if out.insert(e.clone(), guard).is_some_and(|g| g != guard) {
                    return Err(format!("`{e}` is marked twice with different guards"));
                }
            }
        }
    }
    Ok(out)
}

/// The string-taking fs runtime fns `reached` calls that `guards` leaves
/// unguarded.
fn unguarded(reached: &std::collections::HashSet<&str>, guards: &std::collections::HashMap<String, &'static str>) -> Vec<String> {
    fs_runtime_str_fns().into_iter().filter(|f| reached.contains(f.as_str()) && !guards.contains_key(f)).collect()
}

/// The public fns of the fs runtime module that take a string operand
/// (`&str`): `every_string_taking_fs_runtime_fn_is_guarded` holds each to a
/// guard, so a new one cannot run unscoped (#4825).
pub(crate) fn fs_runtime_str_fns() -> Vec<String> {
    let mut out = Vec::new();
    for line in fs_runtime_source().lines() {
        let Some(rest) = line.strip_prefix("pub fn ") else { continue };
        let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        let tail = &rest[name.len()..];
        // The parameter list: from the first `(` at generic depth 0.
        let mut depth = 0i32;
        let mut params = None;
        for (i, c) in tail.char_indices() {
            match c {
                '<' => depth += 1,
                '>' if i > 0 && tail.as_bytes()[i - 1] != b'-' => depth -= 1,
                '(' if depth == 0 => {
                    params = Some(&tail[i..]);
                    break;
                }
                _ => {}
            }
        }
        let close = params.and_then(|p| {
            let mut d = 0i32;
            p.char_indices().find_map(|(i, c)| {
                match c {
                    '(' => d += 1,
                    ')' => d -= 1,
                    _ => {}
                }
                (d == 0).then_some(&p[..=i])
            })
        });
        if close.is_some_and(|p| p.contains("&str")) {
            out.push(name);
        }
    }
    out
}

/// `code` with every call `almide_rt_fs_<f>[::<..>](` whose fn has a guard
/// rewritten to `<guard>!(almide_rt_fs_<f>[::<..>], ` — the twin's
/// `__almide_scoped_fs::almide_rt_fs_<f>` when `twinned` holds it.
fn wrap_fs_calls(code: &str, guards: &std::collections::HashMap<String, &'static str>, twinned: &std::collections::BTreeSet<String>) -> String {
    let mut out = String::with_capacity(code.len() + 256);
    let mut copied = 0;
    // Literal/comment contents are not calls, even when they spell a host
    // symbol followed by `(`. Reuse the runtime-reference token scanner.
    for ident in crate::rust_idents::identifiers(code).filter(|id| guards.contains_key(*id)) {
        let i = ident.as_ptr() as usize - code.as_ptr() as usize;
        let before = &code[copied..i];
        let after = &code[i..];
        let ident_len = ident.len();
        // An optional turbofish, then the call's `(`.
        let mut j = ident_len;
        if after[j..].starts_with("::<") {
            let mut depth = 0i32;
            for (k, c) in after[j + 2..].char_indices() {
                match c {
                    '<' => depth += 1,
                    '>' => {
                        depth -= 1;
                        if depth == 0 {
                            j = j + 2 + k + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
        out.push_str(before);
        match guards.get(ident) {
            Some(g) if !before.ends_with("fn ") && after[j..].starts_with('(') => {
                let twin = if twinned.contains(ident) { format!("{}::", fs_scope_twin::TWIN_MOD) } else { String::new() };
                out.push_str(&format!("{g}!({twin}{}, ", &after[..j]));
                copied = i + j + 1;
            }
            _ => {
                out.push_str(ident);
                copied = i + ident_len;
            }
        }
    }
    out.push_str(&code[copied..]);
    out
}


#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scope_guards_do_not_rewrite_literals_comments_or_definitions() {
        let guards = fs_runtime_guards();
        let src = r##"let a = "almide_rt_fs_write(x)";
let b = r#"almide_rt_fs_write(x)"#;
// almide_rt_fs_write(x)
/* almide_rt_fs_write(x) */
fn almide_rt_fs_write(x: &str) {}
almide_rt_fs_write(x, y)
"##;
        let wrapped = wrap_fs_calls(src, &guards, &Default::default());
        assert_eq!(wrapped, src.strip_suffix("almide_rt_fs_write(x, y)\n").unwrap().to_string()
            + "__almide_fs_path!(almide_rt_fs_write, x, y)\n");
    }

    /// #4825: the guard comes from the mark on the declaration, whatever the
    /// parameter is called (the runtime-name rule left `file`, `from`/`to`
    /// and `p` unguarded).
    #[test]
    fn a_path_parameter_is_guarded_by_its_mark_not_its_name() {
        let src = "@intrinsic(\"almide_rt_fs_novel\")\n@fs_path(file)\neffect fn novel(file: String, n: Int) -> Result[Unit, String] = _\n\n\
                   @intrinsic(\"almide_rt_fs_novel_two\")\n@fs_path(from, to)\neffect fn novel_two(from: String, to: String) -> Result[Unit, String] = _\n\n\
                   @intrinsic(\"almide_rt_fs_novel_q\")\n@fs_path(p)\neffect fn novel_q(p: String) -> Bool = _\n\n\
                   @intrinsic(\"almide_rt_fs_novel_g\")\n@fs_glob(pat)\neffect fn novel_g(pat: String) -> Result[List[String], String] = _\n";
        let g = fs_runtime_guards_from(&[("fixture", src)]).expect("marks read");
        assert_eq!(g.get("almide_rt_fs_novel"), Some(&"__almide_fs_path"));
        assert_eq!(g.get("almide_rt_fs_novel_two"), Some(&"__almide_fs_two"));
        assert_eq!(g.get("almide_rt_fs_novel_q"), Some(&"__almide_fs_bool"));
        assert_eq!(g.get("almide_rt_fs_novel_g"), Some(&"__almide_fs_glob"));
    }

    /// A mark that cannot say how the path is judged is refused, never read
    /// as "no path".
    #[test]
    fn a_mark_that_cannot_be_read_is_refused() {
        for src in [
            "@intrinsic(\"x\")\n@fs_path(p)\neffect fn f(n: Int, p: String) -> Result[Unit, String] = _\n",
            "@fs_path(p)\neffect fn f(p: String) -> Result[Unit, String] = _\n",
            "@intrinsic(\"x\")\n@fs_path(p)\n@fs_glob(p)\neffect fn f(p: String) -> Result[Unit, String] = _\n",
            "@intrinsic(\"x\")\n@fs_temp(p)\neffect fn f(p: String, q: String) -> Result[Unit, String] = _\n",
        ] {
            assert!(fs_runtime_guards_from(&[("fixture", src)]).is_err(), "accepted: {src}");
        }
    }

    /// Every fs runtime fn that takes a string is guarded through some
    /// declaration's mark (the fail-closed rule's other half).
    #[test]
    fn every_string_taking_fs_runtime_fn_is_guarded() {
        let guards = fs_runtime_guards();
        let fns = fs_runtime_str_fns();
        assert!(fns.len() > 30, "the fs runtime's string-taking fns were not found: {fns:?}");
        let missing: Vec<_> = fns.iter().filter(|f| !guards.contains_key(*f)).collect();
        assert!(missing.is_empty(), "fs runtime fns no declaration marks with its path: {missing:?}");
    }

    /// #4832: the unix socket leaves are bounded by their path.
    #[test]
    fn the_unix_socket_paths_are_guarded() {
        let guards = fs_runtime_guards();
        for s in ["almide_rt_unix___os_connect", "almide_rt_unix___os_listen"] {
            assert_eq!(guards.get(s), Some(&"__almide_fs_path"), "{s}");
        }
    }

    /// The negative control: a reached string-taking fs runtime fn the
    /// guards miss refuses the scope (it used to be called unguarded).
    #[test]
    fn an_unguarded_fs_runtime_fn_refuses_the_scope() {
        let mut guards = fs_runtime_guards();
        guards.remove("almide_rt_fs_read_text");
        let reached: std::collections::HashSet<&str> = ["almide_rt_fs_read_text", "almide_rt_fs_write"].into_iter().collect();
        assert_eq!(unguarded(&reached, &guards), vec!["almide_rt_fs_read_text".to_string()]);
        assert!(unguarded(&reached, &fs_runtime_guards()).is_empty());
    }
}
