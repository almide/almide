//! Whether a ledger rename keeps the fn's type (#4525).
//!
//! `removed_spellings::same_shape_rename` finds the renames: a removal whose
//! ledger rewrite keeps the call's argument list. Renaming the callee is the
//! whole edit only when the new fn also has the old fn's TYPE — the same
//! parameter types, return type and effect — so `almide fix` may apply the
//! rename unattended exactly then. `string.to_bytes(s)` → `bytes.from_string(s)`
//! keeps the argument text and changes the result from `List[Int]` to
//! `Bytes`; applied unattended, it moves the failure away from the line it
//! edited, or keeps compiling with another value flowing on.
//!
//! The types come from one place, the machine-owned stdlib signature index
//! (`docs/stdlib/*.md`, written by `tools/gen-stdlib-doc-index.py` from the
//! checker's interface JSON, so transparent aliases are already expanded).
//! An old fn is no longer in the index, so `tools/gen-removed-signatures.py`
//! records its last index line from the history beside the new fn's current
//! line, in `generated/removed-signatures.tsv`.
//! `tests/removed_signatures_test.rs` keeps that file whole and current.
//!
//! The comparison reads the new fn's generics and their protocol bounds from
//! its declaration in the bundled source: a generic parameter of the new fn
//! may stand for the old concrete type (`int.to_int8(x: Int)` →
//! `int8.from_wrapping[S: Integer](x: S)`) when the old type is a built-in
//! conformer of every bound, and the return type must then come out the same.

use std::collections::HashMap;
use std::sync::OnceLock;

const SIGNATURES: &str = include_str!("generated/removed-signatures.tsv");

/// One recorded rename: the old fn's last index line and the new fn's index
/// line now (`None` when the new fn has none, recorded as `-`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recorded {
    pub old: &'static str,
    pub new: Option<&'static str>,
}

fn table() -> &'static HashMap<&'static str, Recorded> {
    static TABLE: OnceLock<HashMap<&'static str, Recorded>> = OnceLock::new();
    TABLE.get_or_init(|| {
        SIGNATURES
            .lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
            .filter_map(|l| {
                let mut cols = l.split('\t');
                let (name, old, new) = (cols.next()?, cols.next()?, cols.next()?);
                Some((name, Recorded { old, new: (new != "-").then_some(new) }))
            })
            .collect()
    })
}

/// The recorded signatures of the rename of `old`, if the record has a row.
pub fn recorded(old: &str) -> Option<Recorded> {
    table().get(old).copied()
}

/// Every old name the record has a row for.
pub fn recorded_names() -> impl Iterator<Item = &'static str> {
    table().keys().copied()
}

/// A type as the signature index spells it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum T {
    /// `Int`, `List[A]`, `Matrix[Float32]`.
    App(String, Vec<T>),
    Tuple(Vec<T>),
    Fn(Vec<T>, Box<T>),
    Record(Vec<(String, T)>),
}

impl std::fmt::Display for T {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let list = |xs: &[T]| xs.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", ");
        match self {
            T::App(n, args) if args.is_empty() => write!(f, "{n}"),
            T::App(n, args) => write!(f, "{n}[{}]", list(args)),
            T::Tuple(xs) => write!(f, "({})", list(xs)),
            T::Fn(ps, r) => write!(f, "({}) -> {r}", list(ps)),
            T::Record(fs) => {
                let inner: Vec<String> = fs.iter().map(|(n, t)| format!("{n}: {t}")).collect();
                write!(f, "{{ {} }}", inner.join(", "))
            }
        }
    }
}

/// A parsed index line: `[effect ]module.fn(name: T, mut name: T) -> R`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Sig {
    effect: bool,
    params: Vec<(bool, T)>,
    ret: T,
}

struct P<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> P<'a> {
    fn ws(&mut self) {
        while self.s.get(self.i) == Some(&b' ') {
            self.i += 1;
        }
    }
    fn eat(&mut self, tok: &str) -> bool {
        self.ws();
        if self.s[self.i..].starts_with(tok.as_bytes()) {
            self.i += tok.len();
            true
        } else {
            false
        }
    }
    fn ident(&mut self) -> Option<String> {
        self.ws();
        let start = self.i;
        while self.s.get(self.i).is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_' || *c == b'.') {
            self.i += 1;
        }
        (self.i > start).then(|| String::from_utf8_lossy(&self.s[start..self.i]).into_owned())
    }
    fn list<X>(&mut self, close: &str, mut item: impl FnMut(&mut Self) -> Option<X>) -> Option<Vec<X>> {
        let mut out = Vec::new();
        if self.eat(close) {
            return Some(out);
        }
        loop {
            out.push(item(self)?);
            if self.eat(close) {
                return Some(out);
            }
            if !self.eat(",") {
                return None;
            }
        }
    }
    fn ty(&mut self) -> Option<T> {
        if self.eat("(") {
            let xs = self.list(")", |p| p.ty())?;
            return if self.eat("->") { Some(T::Fn(xs, Box::new(self.ty()?))) } else { Some(T::Tuple(xs)) };
        }
        if self.eat("{") {
            let fs = self.list("}", |p| {
                let n = p.ident()?;
                p.eat(":").then_some(())?;
                Some((n, p.ty()?))
            })?;
            return Some(T::Record(fs));
        }
        let name = self.ident()?;
        let args = if self.eat("[") { self.list("]", |p| p.ty())? } else { Vec::new() };
        Some(T::App(name, args))
    }
}

fn parse_sig(line: &str) -> Option<Sig> {
    let (effect, rest) = match line.strip_prefix("effect ") {
        Some(r) => (true, r),
        None => (false, line),
    };
    let open = rest.find('(')?;
    let mut p = P { s: rest.as_bytes(), i: open + 1 };
    let params = p.list(")", |p| {
        let m = p.eat("mut ");
        p.ident()?;
        p.eat(":").then_some(())?;
        Some((m, p.ty()?))
    })?;
    p.eat("->").then_some(())?;
    let ret = p.ty()?;
    p.ws();
    (p.i == p.s.len()).then_some(Sig { effect, params, ret })
}

/// One-way match of the new fn's type `new` against the old `old`: a generic
/// of the new fn binds (consistently) when `bind` allows, everything else is
/// structural equality.
fn unify(new: &T, old: &T, generics: &[String], bind: bool, b: &mut HashMap<String, T>) -> bool {
    match (new, old) {
        (T::App(n, a), _) if a.is_empty() && generics.contains(n) => match b.get(n) {
            Some(bound) => bound == old,
            None if bind => {
                b.insert(n.clone(), old.clone());
                true
            }
            None => false,
        },
        (T::App(n, a), T::App(m, c)) => n == m && a.len() == c.len() && a.iter().zip(c).all(|(x, y)| unify(x, y, generics, bind, b)),
        (T::Tuple(a), T::Tuple(c)) => a.len() == c.len() && a.iter().zip(c).all(|(x, y)| unify(x, y, generics, bind, b)),
        (T::Fn(a, r), T::Fn(c, s)) => {
            a.len() == c.len() && a.iter().zip(c).all(|(x, y)| unify(x, y, generics, bind, b)) && unify(r, s, generics, bind, b)
        }
        (T::Record(a), T::Record(c)) => {
            a.len() == c.len() && a.iter().zip(c).all(|((n, x), (m, y))| n == m && unify(x, y, generics, bind, b))
        }
        _ => false,
    }
}

fn subst(t: &T, b: &HashMap<String, T>) -> T {
    match t {
        T::App(n, a) if a.is_empty() => b.get(n).cloned().unwrap_or_else(|| t.clone()),
        T::App(n, a) => T::App(n.clone(), a.iter().map(|x| subst(x, b)).collect()),
        T::Tuple(xs) => T::Tuple(xs.iter().map(|x| subst(x, b)).collect()),
        T::Fn(ps, r) => T::Fn(ps.iter().map(|x| subst(x, b)).collect(), Box::new(subst(r, b))),
        T::Record(fs) => T::Record(fs.iter().map(|(n, x)| (n.clone(), subst(x, b))).collect()),
    }
}

/// The generics of `name` and the protocol bounds on each, read from its
/// declaration in the bundled source (the signature table keeps the names
/// of the generics, not their bounds). A structural bound cannot be judged
/// from the index, so it is reported as a bound nothing satisfies.
fn declared_generics(name: &str) -> (Vec<String>, HashMap<String, Vec<String>>) {
    let Some((m, f)) = name.split_once('.') else { return Default::default() };
    let Some(program) = crate::stdlib::get_bundled_source(m).and_then(almide_lang::parse_cached) else {
        return Default::default();
    };
    let generics = program.decls.iter().find_map(|d| match d {
        almide_lang::ast::Decl::Fn { name, generics, .. } if name.as_str() == f => Some(generics.clone().unwrap_or_default()),
        _ => None,
    });
    let mut names = Vec::new();
    let mut bounds: HashMap<String, Vec<String>> = HashMap::new();
    for g in generics.into_iter().flatten() {
        let n = g.name.as_str().to_string();
        let entry = bounds.entry(n.clone()).or_default();
        entry.extend(g.bounds.iter().flatten().map(|p| p.as_str().to_string()));
        if g.structural_bound.is_some() {
            entry.push("{ … }".to_string());
        }
        names.push(n);
    }
    (names, bounds)
}

/// Does the type the index spells `ty` conform to protocol `proto`? A
/// primitive by the built-in table, a stdlib type by its own declaration
/// (`mod type SafeHtml: Body = String`). Anything else is not known to.
fn conforms(ty: &str, proto: &str) -> bool {
    if crate::canonicalize::protocols::is_builtin_conformer(ty, proto) {
        return true;
    }
    let Some(module) = crate::bundled_sigs::bundled_type_owner(ty) else { return false };
    let Some(source) = almide_lang::decl_index::types_source(module) else { return false };
    let Some(program) = almide_lang::parse_cached(source) else { return false };
    program.decls.iter().any(|d| {
        matches!(d, almide_lang::ast::Decl::Type { name, deriving: Some(ps), .. }
            if name.as_str() == ty && ps.iter().any(|p| p.as_str() == proto))
    })
}

/// How the type of `to` differs from that of `from`, in one line for the
/// hint, or `None` when the two fns have the same type and renaming the
/// callee is the whole edit.
pub fn type_change(from: &str, to: &str) -> Option<String> {
    // NEGATIVE CONTROL (#4525): the type comparison reverted — every rename keeping the argument text is machine-applied again.
    if std::env::var_os("ALMIDE_NEGCTL_NEVER").is_none() {
        return None;
    }
    let Some(rec) = recorded(from) else {
        return Some(format!("the type of `{from}` is not recorded, so the rename is not known to keep it"));
    };
    let Some(new_line) = rec.new else {
        return Some(format!("`{to}` has no stdlib signature to compare `{from}` with"));
    };
    let (Some(old), Some(new)) = (parse_sig(rec.old), parse_sig(new_line)) else {
        return Some(format!("the signatures of `{from}` and `{to}` could not be compared"));
    };
    let (generics, bounds) = declared_generics(to);
    let mut b = HashMap::new();
    let params_match = old.params.len() == new.params.len()
        && new.params.iter().zip(&old.params).all(|((nm, nt), (om, ot))| nm == om && unify(nt, ot, &generics, true, &mut b));
    let mut parts = Vec::new();
    if new.effect != old.effect {
        parts.push(if new.effect { "is an effect fn".to_string() } else { "is not an effect fn".to_string() });
    }
    let show = |ps: &[(bool, T)], b: &HashMap<String, T>| {
        ps.iter().map(|(m, t)| format!("{}{}", if *m { "mut " } else { "" }, subst(t, b))).collect::<Vec<_>>().join(", ")
    };
    if !params_match {
        parts.push(format!("takes ({}), not ({})", show(&new.params, &HashMap::new()), show(&old.params, &HashMap::new())));
    } else {
        // Every generic the parameters bound must admit the old type.
        let mut unmet: Vec<String> = Vec::new();
        let mut bound: Vec<(&String, &T)> = b.iter().collect();
        bound.sort_by(|x, y| x.0.cmp(y.0));
        for (g, t) in bound {
            for p in bounds.get(g).into_iter().flatten() {
                if !conforms(&t.to_string(), p) {
                    unmet.push(format!("`{g}: {p}`, which {t} is not"));
                }
            }
        }
        if !unmet.is_empty() {
            parts.push(format!("needs {}", unmet.join(" and ")));
        }
    }
    // The return type may only use generics the parameters bound: one that
    // only the return names is inferred from the caller's context instead.
    if !(params_match && unify(&new.ret, &old.ret, &generics, false, &mut b)) {
        parts.push(format!("returns {}, not {}", if params_match { subst(&new.ret, &b) } else { new.ret.clone() }, old.ret));
    }
    (!parts.is_empty()).then(|| format!("`{to}` {}: adapt the callers", parts.join(" and ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_lines_parse() {
        let s = parse_sig("list.fold(xs: List[A], init: B, f: (B, A) -> B) -> B").unwrap();
        assert_eq!(s.params.len(), 3);
        assert_eq!(s.params[2].1.to_string(), "(B, A) -> B");
        let s = parse_sig("effect bytes.read_bool_at(mut b: Bytes, pos: Int) -> (Int, Option[Bool])").unwrap();
        assert!(s.effect && s.params[0].0);
        assert_eq!(s.ret.to_string(), "(Int, Option[Bool])");
        assert!(parse_sig("env.args() -> List[String]").is_some_and(|s| s.params.is_empty()));
        assert!(parse_sig("map.entries(m)").is_none(), "a call, not an index line");
    }

    #[test]
    fn the_same_type_through_a_bounded_generic_is_kept() {
        // `int8.from_wrapping[S: Integer](x: S) -> Int8` stands for the old
        // `int.to_int8(x: Int) -> Int8`.
        assert_eq!(type_change("int.to_int8", "int8.from_wrapping"), None);
        assert_eq!(type_change("bytes.cmp", "bytes.compare"), None);
        // A stdlib type that declares the bound (`SafeHtml: Body`).
        assert_eq!(type_change("http.html", "http.response"), None);
    }

    #[test]
    fn a_bound_the_old_type_does_not_meet_is_named() {
        assert_eq!(
            type_change("net.tcp_close", "io.close").as_deref(),
            Some("`io.close` needs `H: Close`, which Int is not: adapt the callers")
        );
    }

    #[test]
    fn a_changed_return_type_is_named() {
        assert_eq!(
            type_change("string.to_bytes", "bytes.from_string").as_deref(),
            Some("`bytes.from_string` returns Bytes, not List[Int]: adapt the callers")
        );
    }
}
