//! #3085: no two live spellings for one stdlib operation.
//!
//! The cheatsheet promises "No synonyms", and the promise leaked twice in the
//! same place: #1735 deprecated `string.length` and left `list.length`, and
//! `matrix.concat_cols_many` / `matrix.row_dot` were "deprecated aliases" in
//! the docs for months with no marker in the source, so nothing warned. A rule
//! the author has to remember at an unrelated keystroke needs a gate; this is
//! it.
//!
//! **Rule.** Inside one public stdlib module, the fns that do the same thing
//! form an alias class, and at most ONE member of a class may be live — every
//! other member carries `@deprecated(use = "<module>.<live member>")`. Two fns
//! are aliases when their signatures are identical AND any of:
//!
//! 1. they bind the same `@intrinsic` symbol;
//! 2. they carry the same `@inline_rust` template;
//! 3. one's body is a bare forward to the other, passing its parameters
//!    through in order (`fn b(x) = a(x)`);
//! 4. their names are both in one synonym vocabulary (`len`/`length`/`size`),
//!    whatever the implementation — a synonym written as its own body is
//!    still a synonym.
//!
//! A deprecated member must name a replacement that exists in the same module
//! and is itself live, so the chain ends at the canonical spelling.
//!
//! **Allowlist.** `ALLOWED` is shrink-only: an entry that no longer trips the
//! rule fails the gate (delete it), and its length may not exceed
//! `ALLOWED_CEILING`, which may only go down. Each entry says why the second
//! live spelling is intended.

use std::collections::{BTreeMap, BTreeSet};

use almide::ast;
use almide::ast::ExprKind;

/// The synonym vocabularies of rule 4. `count` is deliberately absent: it is
/// count-by-predicate (`list.count(xs, f)`) or count-of-occurrences
/// (`string.count(s, sub)`) wherever it appears, never an element count.
const SYNONYM_VOCABULARIES: &[&[&str]] = &[&["len", "length", "size"]];

/// Modules that are not user surface.
const NOT_SURFACE: &[&str] = &["prim"];

/// `(module, fn)` pairs allowed to stay live although the rule groups them
/// with another live fn.
const ALLOWED: &[(&str, &str, &str)] = &[
    // The numeric conversion trio (CLAUDE.md "API families are extended by
    // matrix"): a lossy conversion has all three cells `to_T` /
    // `to_T_saturating` / `to_T_checked`. Float → integer `to_T` already
    // truncates and clamps, so the saturating cell forwards to it — the
    // family rule, not an accidental second name, is why both are live.
    ("float", "to_int8_saturating", "conversion trio: the saturating cell of a lossy Float -> Int8"),
    ("float", "to_int16_saturating", "conversion trio: the saturating cell of a lossy Float -> Int16"),
    ("float", "to_int32_saturating", "conversion trio: the saturating cell of a lossy Float -> Int32"),
    ("float", "to_int64_saturating", "conversion trio: the saturating cell of a lossy Float -> Int64"),
    ("float", "to_uint8_saturating", "conversion trio: the saturating cell of a lossy Float -> UInt8"),
    ("float", "to_uint16_saturating", "conversion trio: the saturating cell of a lossy Float -> UInt16"),
    ("float", "to_uint32_saturating", "conversion trio: the saturating cell of a lossy Float -> UInt32"),
    ("float", "to_uint64_saturating", "conversion trio: the saturating cell of a lossy Float -> UInt64"),
];

/// Shrink-only: lower it when an entry leaves `ALLOWED`, never raise it.
const ALLOWED_CEILING: usize = 8;

struct SurfaceFn {
    name: String,
    deprecated_use: Option<String>,
    deprecated: bool,
    intrinsic: Option<String>,
    inline_rust: Option<String>,
    forwards_to: Option<String>,
}

fn attr_string(attrs: &[ast::Attribute], name: &str, arg: Option<&str>) -> Option<String> {
    let attr = attrs.iter().find(|a| a.name.as_str() == name)?;
    attr.args.iter().find_map(|a| {
        let named_ok = match (arg, &a.name) {
            (None, _) => true,
            (Some(want), Some(n)) => n.as_str() == want,
            (Some(_), None) => false,
        };
        match (&a.value, named_ok) {
            (ast::AttrValue::String { value }, true) => Some(value.clone()),
            _ => None,
        }
    })
}

/// `fn b(x, y) = a(x, y)` / `= module.a(x, y)` → `Some("a")`.
fn forward_target(module: &str, params: &[ast::Param], body: Option<&ast::Expr>) -> Option<String> {
    let ExprKind::Call { callee, args, named_args, .. } = &body?.kind else { return None };
    if !named_args.is_empty() || args.len() != params.len() {
        return None;
    }
    let target = match &callee.kind {
        ExprKind::Ident { name, .. } => name.to_string(),
        ExprKind::Member { object, field } => match &object.kind {
            ExprKind::Ident { name, .. } if name.as_str() == module => field.to_string(),
            _ => return None,
        },
        _ => return None,
    };
    let passes_through = args.iter().zip(params).all(|(a, p)| {
        matches!(&a.kind, ExprKind::Ident { name, .. } if *name == p.name)
    });
    passes_through.then_some(target)
}

fn surface(module: &str) -> Vec<SurfaceFn> {
    let src = almide::stdlib::get_bundled_source(module)
        .unwrap_or_else(|| panic!("no bundled source for stdlib module {module}"));
    let program = almide_lang::parse_cached(src)
        .unwrap_or_else(|| panic!("stdlib module {module} does not parse"));
    program
        .decls
        .iter()
        .filter_map(|d| match d {
            ast::Decl::Fn { name, visibility, attrs, params, body, .. }
                if matches!(visibility, ast::Visibility::Public) && !name.as_str().starts_with("__") =>
            {
                Some(SurfaceFn {
                    name: name.to_string(),
                    deprecated: attrs.iter().any(|a| a.name.as_str() == "deprecated"),
                    deprecated_use: attr_string(attrs, "deprecated", Some("use")),
                    intrinsic: attr_string(attrs, "intrinsic", None),
                    inline_rust: attr_string(attrs, "inline_rust", None),
                    forwards_to: forward_target(module, params, body.as_ref()),
                })
            }
            _ => None,
        })
        .collect()
}

fn same_signature(module: &str, a: &str, b: &str) -> bool {
    let (Some(x), Some(y)) = (almide::stdlib::lookup_sig(module, a), almide::stdlib::lookup_sig(module, b)) else {
        return false;
    };
    let tys = |s: &almide_frontend::types::FnSig| s.params.iter().map(|(_, t)| t.clone()).collect::<Vec<_>>();
    tys(&x) == tys(&y) && x.ret == y.ret && x.is_effect == y.is_effect
}

/// Alias classes of one module, as sets of fn names (only classes of 2+).
fn alias_classes(module: &str, fns: &[SurfaceFn]) -> Vec<BTreeSet<String>> {
    // Union-find over indices.
    let mut parent: Vec<usize> = (0..fns.len()).collect();
    fn find(p: &mut [usize], i: usize) -> usize {
        if p[i] != i {
            let r = find(p, p[i]);
            p[i] = r;
        }
        p[i]
    }
    let index: BTreeMap<&str, usize> = fns.iter().enumerate().map(|(i, f)| (f.name.as_str(), i)).collect();
    let link = |p: &mut [usize], i: usize, j: usize| {
        if i != j && same_signature(module, &fns[i].name, &fns[j].name) {
            let (a, b) = (find(p, i), find(p, j));
            p[a] = b;
        }
    };
    for i in 0..fns.len() {
        for j in (i + 1)..fns.len() {
            let (a, b) = (&fns[i], &fns[j]);
            let shared = |x: &Option<String>, y: &Option<String>| x.is_some() && x == y;
            let vocab = SYNONYM_VOCABULARIES
                .iter()
                .any(|v| v.contains(&a.name.as_str()) && v.contains(&b.name.as_str()));
            if shared(&a.intrinsic, &b.intrinsic) || shared(&a.inline_rust, &b.inline_rust) || vocab {
                link(&mut parent, i, j);
            }
        }
        if let Some(t) = fns[i].forwards_to.as_deref().and_then(|t| index.get(t)) {
            link(&mut parent, i, *t);
        }
    }
    let mut classes: BTreeMap<usize, BTreeSet<String>> = BTreeMap::new();
    for i in 0..fns.len() {
        let r = find(&mut parent, i);
        classes.entry(r).or_default().insert(fns[i].name.clone());
    }
    classes.into_values().filter(|c| c.len() > 1).collect()
}

fn surface_modules() -> Vec<&'static str> {
    almide::stdlib::BUNDLED_MODULES.iter().copied().filter(|m| !NOT_SURFACE.contains(m)).collect()
}

#[test]
fn every_alias_class_has_one_live_spelling() {
    let allowed: BTreeSet<(&str, &str)> = ALLOWED.iter().map(|(m, f, _)| (*m, *f)).collect();
    let mut violations = Vec::new();
    let mut allowed_hit: BTreeSet<(&str, &str)> = BTreeSet::new();
    for module in surface_modules() {
        let fns = surface(module);
        for class in alias_classes(module, &fns) {
            let live: Vec<&String> = class
                .iter()
                .filter(|n| !fns.iter().any(|f| &f.name == *n && f.deprecated))
                .collect();
            let unexcused: Vec<&String> = live
                .iter()
                .copied()
                .filter(|n| {
                    let key = (module, n.as_str());
                    let hit = allowed.iter().find(|a| **a == key).copied();
                    if let Some(k) = hit {
                        allowed_hit.insert(k);
                    }
                    hit.is_none()
                })
                .collect();
            if unexcused.len() > 1 {
                violations.push(format!(
                    "{module}: {:?} are aliases (same signature and same intrinsic / template / forward / \
                     synonym name) and {:?} are all live — keep one and mark the rest \
                     `@deprecated(since = <current epoch>, use = \"{module}.<the one>\")`",
                    class, unexcused
                ));
            }
        }
    }
    assert!(violations.is_empty(), "unmarked stdlib synonyms:\n  {}", violations.join("\n  "));
    let stale: Vec<_> = allowed.difference(&allowed_hit).collect();
    assert!(
        stale.is_empty(),
        "ALLOWED entries that no longer trip the rule (delete them and lower ALLOWED_CEILING): {stale:?}"
    );
    assert!(
        ALLOWED.len() <= ALLOWED_CEILING,
        "ALLOWED grew to {} past its shrink-only ceiling {ALLOWED_CEILING}",
        ALLOWED.len()
    );
}

#[test]
fn every_deprecated_alias_names_a_live_replacement_in_its_module() {
    let mut bad = Vec::new();
    for module in surface_modules() {
        let fns = surface(module);
        for f in fns.iter().filter(|f| f.deprecated) {
            let Some(target) = f.deprecated_use.as_deref() else { continue };
            // Operator replacements (`use = "??"`) are not fn names.
            let Some((m, name)) = target.split_once('.') else { continue };
            if m != module {
                continue;
            }
            match fns.iter().find(|g| g.name == name) {
                None => bad.push(format!("{module}.{}: use = {target:?} does not exist", f.name)),
                Some(g) if g.deprecated => {
                    bad.push(format!("{module}.{}: use = {target:?} is itself deprecated", f.name))
                }
                Some(_) => {}
            }
        }
    }
    assert!(bad.is_empty(), "deprecation chains that do not end at a live fn:\n  {}", bad.join("\n  "));
}

/// The gate must see what it guards: if the parse or the classes silently
/// came back empty, every assertion above would pass on nothing.
#[test]
fn the_gate_sees_the_known_deprecated_aliases() {
    for (module, alias, canonical) in [
        ("list", "length", "len"),
        ("string", "length", "len"),
        ("matrix", "concat_cols_many", "concat_cols"),
        ("matrix", "row_dot", "dot_row"),
    ] {
        let fns = surface(module);
        let classes = alias_classes(module, &fns);
        assert!(
            classes.iter().any(|c| c.contains(alias) && c.contains(canonical)),
            "{module}.{alias} and {module}.{canonical} are not grouped as aliases — the gate is blind"
        );
        let f = fns.iter().find(|f| f.name == alias).expect("alias declared");
        assert_eq!(
            f.deprecated_use.as_deref(),
            Some(format!("{module}.{canonical}").as_str()),
            "{module}.{alias} must be @deprecated(use = \"{module}.{canonical}\")"
        );
    }
}
