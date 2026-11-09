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
    pub fn new(roots: &[String], rt_path: &str, rt_use: &str, calls: &str) -> Self {
        let reached: std::collections::HashSet<_> = crate::rust_idents::identifiers(calls).collect();
        let mut guards = fs_runtime_guards();
        guards.retain(|name, _| reached.contains(name.as_str()));
        let entries = guards.iter().filter(|(_, g)| **g != "__almide_fs_root")
            .map(|(n, _)| n.clone()).collect();
        let (twin, twinned) = fs_scope_twin::scoped_twin(fs_runtime_source(), &entries, rt_use);
        let list: Vec<_> = roots.iter().map(|r| format!("{r:?}")).collect();
        let guards_src = FS_SCOPE_GUARDS.replace("__ALMIDE_RT::", rt_path);
        let prelude = format!("\n{SCOPE_MARKER}\nconst __ALMIDE_FS_SCOPE_ROOTS: &[&str] = &[{}];\n{guards_src}{twin}", list.join(", "));
        Self { prelude, guards, twinned }
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

/// Each public `almide_rt_fs_*` fn of the runtime with the guard macro its
/// signature calls for (`None`: it takes no path).
pub(crate) fn fs_runtime_guards() -> std::collections::HashMap<String, &'static str> {
    let src = fs_runtime_source();
    let mut out = std::collections::HashMap::new();
    for line in src.lines() {
        let Some(rest) = line.strip_prefix("pub fn almide_rt_fs_") else { continue };
        let name_len = rest.find(['<', '(']).unwrap_or(rest.len());
        let name = format!("almide_rt_fs_{}", &rest[..name_len]);
        let Some((params, ret)) = fs_signature(&rest[name_len..]) else { continue };
        let guard = match params.as_slice() {
            [] => continue,
            [p, ..] if ret.starts_with("bool") && (p == "path" || p == "dir") => "__almide_fs_bool",
            [s, d] if s == "src" && d == "dst" => "__almide_fs_two",
            [p] if p == "pattern" => "__almide_fs_glob",
            [p] if p == "prefix" => "__almide_fs_temp",
            // The rooted calls' `<root>\0<segments>` operand (#4084): the
            // root is judged; the descriptor walk keeps the rest under it.
            [p, ..] if p == "at" => "__almide_fs_root",
            [p, ..] if p == "path" || p == "dir" => "__almide_fs_path",
            _ => continue,
        };
        out.insert(name, guard);
    }
    out
}

/// The parameter names and the return text of a signature tail
/// `<generics>(params) -> Ret {`.
fn fs_signature(tail: &str) -> Option<(Vec<String>, String)> {
    let mut depth = 0i32;
    let mut start = None;
    for (i, c) in tail.char_indices() {
        match c {
            '<' | '(' | '[' => {
                if c == '(' && depth == 0 && start.is_none() {
                    start = Some(i + 1);
                }
                depth += 1;
            }
            '>' if i > 0 && tail.as_bytes()[i - 1] == b'-' => {}
            '>' | ')' | ']' => {
                depth -= 1;
                if c == ')' && depth == 0 && start.is_some() {
                    let params = &tail[start?..i];
                    let ret = tail[i + 1..].trim_start().strip_prefix("->").unwrap_or("").trim_start().to_string();
                    let mut names = Vec::new();
                    let (mut d, mut seg) = (0i32, String::new());
                    for ch in params.chars() {
                        match ch {
                            '<' | '(' | '[' => d += 1,
                            '>' | ')' | ']' => d -= 1,
                            _ => {}
                        }
                        if ch == ',' && d == 0 {
                            names.push(std::mem::take(&mut seg));
                        } else {
                            seg.push(ch);
                        }
                    }
                    names.push(seg);
                    let names = names
                        .iter()
                        .filter_map(|p| p.split(':').next().map(str::trim))
                        .filter(|n| !n.is_empty())
                        .map(str::to_string)
                        .collect();
                    return Some((names, ret));
                }
            }
            _ => {}
        }
    }
    None
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
}
