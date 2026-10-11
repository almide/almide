//! The name-keyed property gate: a property that several stdlib functions
//! share is DECLARED once, at the functions (`@consume`, `@borrow_ref`,
//! `@reuse`, …), and every leg derives it from there. It is not answered by
//! listing the functions' names in the compiler.
//!
//! A name list is the shape a shortcut takes: `matches!(func, "set" | "swap")`,
//! `f == "take" || f == "drop"`, a `const X: &[&str]` of runtime symbols. Each
//! one holds a copy of a property that lives somewhere else. A function added
//! to the family later is missing from every copy nobody remembered, and the
//! miss is silent: a copy instead of an in-place write, or the two legs
//! disagreeing.
//!
//! This test parses the compiler's source (`crates/*/src`, `src/`; generated
//! files and `#[cfg(test)]` modules excluded) and finds every place that groups
//! two or more stdlib names (a function, a module, or an `almide_rt_*` symbol):
//!
//! * an or-pattern whose alternatives name them (`"a" | "b"`, in a `match` arm
//!   or a `matches!`);
//! * an `||` chain of comparisons against them (`f == "a" || f == "b"`);
//! * an array, slice or `vec!` whose elements name them.
//!
//! A single name per `match` arm is dispatch (`"set" => lower_list_set(..)`):
//! choosing a function's own lowering by its name is what dispatch is, so it is
//! not counted. A tuple `("list", "set")` names ONE function and is not counted
//! either.
//!
//! Every site found must be a row of `proofs/name-keyed-sites.toml`, keyed by
//! file, enclosing item, shape and the names it groups (no line numbers, so
//! edits elsewhere do not move it). The rows that predate the gate are
//! `legacy`, under a ceiling that only falls. A new site is refused unless its
//! row says `justified` and why the names cannot be a declaration. A row whose
//! site is gone must be removed.
//!
//! `ALMIDE_UPDATE_NAME_SITES=1` removes the stale rows, appends each new site
//! as `justified` with an empty `why`, lowers the ceiling to the legacy count,
//! and still fails until each `why` is written.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use syn::visit::{self, Visit};
use syn::{BinOp, Expr, ExprLit, Lit, Pat};

const LEDGER: &str = "proofs/name-keyed-sites.toml";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every stdlib function name (public or not), every stdlib module name.
struct Names {
    fns: BTreeSet<String>,
    modules: BTreeSet<String>,
}

impl Names {
    fn load() -> Names {
        let mut fns = BTreeSet::new();
        for e in std::fs::read_dir(root().join("stdlib")).expect("stdlib/") {
            let p = e.expect("entry").path();
            if p.extension().is_none_or(|x| x != "almd") {
                continue;
            }
            let src = std::fs::read_to_string(&p).expect("stdlib source");
            for line in src.lines() {
                let words: Vec<&str> = line.split_whitespace().collect();
                let Some(at) = words.iter().position(|w| *w == "fn") else {
                    continue;
                };
                if !words[..at]
                    .iter()
                    .all(|w| matches!(*w, "pub" | "effect" | "async" | "local"))
                {
                    continue;
                }
                let Some(name) = words.get(at + 1) else {
                    continue;
                };
                let name: String = name
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                if !name.is_empty() {
                    fns.insert(name);
                }
            }
        }
        let modules = std::fs::read_dir(root().join("docs/stdlib"))
            .expect("docs/stdlib/")
            .filter_map(|e| e.ok()?.path().file_stem()?.to_str().map(String::from))
            .filter(|m| m != "README")
            .collect();
        Names { fns, modules }
    }

    fn is_name(&self, s: &str) -> bool {
        if s.starts_with("almide_rt_") {
            return true;
        }
        if let Some((m, f)) = s.split_once('.') {
            return self.modules.contains(m) && self.fns.contains(f);
        }
        self.fns.contains(s) || self.modules.contains(s)
    }
}

fn lit_str(e: &Expr) -> Option<String> {
    match e {
        Expr::Lit(ExprLit {
            lit: Lit::Str(s), ..
        }) => Some(s.value()),
        Expr::Paren(p) => lit_str(&p.expr),
        Expr::Reference(r) => lit_str(&r.expr),
        _ => None,
    }
}

/// The names a pattern spells anywhere inside it.
fn pat_names(p: &Pat, names: &Names, out: &mut BTreeSet<String>) {
    match p {
        Pat::Lit(ExprLit {
            lit: Lit::Str(s), ..
        }) if names.is_name(&s.value()) => {
            out.insert(s.value());
        }
        Pat::Or(o) => o.cases.iter().for_each(|c| pat_names(c, names, out)),
        Pat::Tuple(t) => t.elems.iter().for_each(|c| pat_names(c, names, out)),
        Pat::TupleStruct(t) => t.elems.iter().for_each(|c| pat_names(c, names, out)),
        Pat::Slice(s) => s.elems.iter().for_each(|c| pat_names(c, names, out)),
        Pat::Paren(x) => pat_names(&x.pat, names, out),
        Pat::Reference(x) => pat_names(&x.pat, names, out),
        Pat::Ident(i) => {
            if let Some((_, sub)) = &i.subpat {
                pat_names(sub, names, out);
            }
        }
        _ => {}
    }
}

/// The names of every or-pattern in `p` whose alternatives name two or more
/// stdlib names between them; an or found is not searched further.
fn or_groups(p: &Pat, names: &Names, out: &mut Vec<BTreeSet<String>>) {
    match p {
        Pat::Or(o) => {
            let mut all = BTreeSet::new();
            let mut cases = 0;
            for c in &o.cases {
                let mut one = BTreeSet::new();
                pat_names(c, names, &mut one);
                if !one.is_empty() {
                    cases += 1;
                }
                all.extend(one);
            }
            if cases >= 2 {
                out.push(all);
            } else {
                o.cases.iter().for_each(|c| or_groups(c, names, out));
            }
        }
        Pat::Tuple(t) => t.elems.iter().for_each(|c| or_groups(c, names, out)),
        Pat::TupleStruct(t) => t.elems.iter().for_each(|c| or_groups(c, names, out)),
        Pat::Slice(s) => s.elems.iter().for_each(|c| or_groups(c, names, out)),
        Pat::Paren(x) => or_groups(&x.pat, names, out),
        Pat::Reference(x) => or_groups(&x.pat, names, out),
        Pat::Ident(i) => {
            if let Some((_, sub)) = &i.subpat {
                or_groups(sub, names, out);
            }
        }
        _ => {}
    }
}

/// The names an element of a table spells: a literal, or literals in a tuple.
fn elem_names(e: &Expr, names: &Names, out: &mut BTreeSet<String>) {
    if let Some(s) = lit_str(e) {
        if names.is_name(&s) {
            out.insert(s);
        }
        return;
    }
    match e {
        Expr::Tuple(t) => t.elems.iter().for_each(|x| elem_names(x, names, out)),
        Expr::Paren(p) => elem_names(&p.expr, names, out),
        _ => {}
    }
}

/// `x == "a"` (either side, through `.as_str()` and `&`): the name compared.
fn compared_name(e: &Expr, names: &Names) -> Option<String> {
    let Expr::Binary(b) = e else { return None };
    if !matches!(b.op, BinOp::Eq(_)) {
        return None;
    }
    lit_str(&b.right)
        .or_else(|| lit_str(&b.left))
        .filter(|s| names.is_name(s))
}

fn or_leaves<'e>(e: &'e Expr, out: &mut Vec<&'e Expr>) {
    match e {
        Expr::Binary(b) if matches!(b.op, BinOp::Or(_)) => {
            or_leaves(&b.left, out);
            or_leaves(&b.right, out);
        }
        Expr::Paren(p) if matches!(&*p.expr, Expr::Binary(b) if matches!(b.op, BinOp::Or(_))) => {
            or_leaves(&p.expr, out)
        }
        _ => out.push(e),
    }
}

struct Finder<'n> {
    names: &'n Names,
    file: String,
    items: Vec<String>,
    /// `file::item::shape` -> every stdlib name the item groups in that shape.
    sites: BTreeMap<String, BTreeSet<String>>,
}

impl Finder<'_> {
    fn report(&mut self, shape: &str, group: BTreeSet<String>) {
        let item = if self.items.is_empty() {
            "<file>".to_string()
        } else {
            self.items.join("::")
        };
        self.sites
            .entry(format!("{}::{}::{}", self.file, item, shape))
            .or_default()
            .extend(group);
    }

    fn table(&mut self, elems: &[&Expr]) {
        let mut group = BTreeSet::new();
        let mut hits = 0;
        for e in elems {
            let mut one = BTreeSet::new();
            elem_names(e, self.names, &mut one);
            if !one.is_empty() {
                hits += 1;
            }
            group.extend(one);
        }
        if hits >= 2 {
            self.report("table", group);
        }
    }

    fn pattern(&mut self, p: &Pat) {
        let mut groups = Vec::new();
        or_groups(p, self.names, &mut groups);
        for g in groups {
            self.report("or-pattern", g);
        }
    }

    fn scoped(&mut self, name: String, f: impl FnOnce(&mut Self)) {
        self.items.push(name);
        f(self);
        self.items.pop();
    }
}

fn is_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("cfg")
            && a.parse_args::<syn::Meta>()
                .is_ok_and(|m| m.path().is_ident("test"))
    })
}

impl<'ast> Visit<'ast> for Finder<'_> {
    fn visit_item_mod(&mut self, m: &'ast syn::ItemMod) {
        if is_cfg_test(&m.attrs) {
            return;
        }
        self.scoped(m.ident.to_string(), |s| visit::visit_item_mod(s, m));
    }

    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        if is_cfg_test(&f.attrs) {
            return;
        }
        self.scoped(f.sig.ident.to_string(), |s| visit::visit_item_fn(s, f));
    }

    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        self.scoped(f.sig.ident.to_string(), |s| visit::visit_impl_item_fn(s, f));
    }

    fn visit_item_const(&mut self, c: &'ast syn::ItemConst) {
        self.scoped(c.ident.to_string(), |s| visit::visit_item_const(s, c));
    }

    fn visit_item_static(&mut self, c: &'ast syn::ItemStatic) {
        self.scoped(c.ident.to_string(), |s| visit::visit_item_static(s, c));
    }

    fn visit_arm(&mut self, a: &'ast syn::Arm) {
        self.pattern(&a.pat);
        visit::visit_arm(self, a);
    }

    fn visit_expr_array(&mut self, a: &'ast syn::ExprArray) {
        let elems: Vec<&Expr> = a.elems.iter().collect();
        self.table(&elems);
        visit::visit_expr_array(self, a);
    }

    fn visit_expr_binary(&mut self, b: &'ast syn::ExprBinary) {
        if !matches!(b.op, BinOp::Or(_)) {
            return visit::visit_expr_binary(self, b);
        }
        let mut leaves = Vec::new();
        or_leaves(&b.left, &mut leaves);
        or_leaves(&b.right, &mut leaves);
        let group: BTreeSet<String> = leaves
            .iter()
            .filter_map(|l| compared_name(l, self.names))
            .collect();
        if leaves
            .iter()
            .filter(|l| compared_name(l, self.names).is_some())
            .count()
            >= 2
        {
            self.report("or-chain", group);
        }
        for l in leaves {
            self.visit_expr(l);
        }
    }

    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        let last = m
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_default();
        match last.as_str() {
            "matches" => {
                let parsed = m.parse_body_with(|input: syn::parse::ParseStream| {
                    let e: Expr = input.parse()?;
                    input.parse::<syn::Token![,]>()?;
                    let p = Pat::parse_multi_with_leading_vert(input)?;
                    // An `if` guard may follow; its tokens are not searched.
                    input.step(|c| {
                        let mut rest = *c;
                        while let Some((_, next)) = rest.token_tree() {
                            rest = next;
                        }
                        Ok(((), rest))
                    })?;
                    Ok((e, p))
                });
                if let Ok((e, p)) = parsed {
                    self.pattern(&p);
                    self.visit_expr(&e);
                }
            }
            "vec" => {
                let parsed = m.parse_body_with(
                    syn::punctuated::Punctuated::<Expr, syn::Token![,]>::parse_terminated,
                );
                if let Ok(elems) = parsed {
                    let elems: Vec<&Expr> = elems.iter().collect();
                    self.table(&elems);
                    elems.into_iter().for_each(|e| self.visit_expr(e));
                }
            }
            _ => {}
        }
    }
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if p.file_name().is_some_and(|n| n == "generated") {
                continue;
            }
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// Every site and the names it groups.
fn sites(names: &Names) -> BTreeMap<String, BTreeSet<String>> {
    let mut files = Vec::new();
    rust_files(&root().join("src"), &mut files);
    for e in std::fs::read_dir(root().join("crates"))
        .expect("crates/")
        .flatten()
    {
        rust_files(&e.path().join("src"), &mut files);
    }
    files.sort();
    let mut out = BTreeMap::new();
    for f in files {
        let src = std::fs::read_to_string(&f).expect("source");
        let ast = syn::parse_file(&src).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        let rel = f
            .strip_prefix(root())
            .expect("under root")
            .to_string_lossy()
            .replace('\\', "/");
        let mut finder = Finder {
            names,
            file: rel,
            items: Vec::new(),
            sites: BTreeMap::new(),
        };
        finder.visit_file(&ast);
        out.extend(finder.sites);
    }
    out
}

struct Row {
    id: String,
    class: String,
    names: usize,
    why: String,
}

fn rows(text: &str) -> Vec<Row> {
    let ledger: toml::Table = toml::from_str(text).expect("ledger parses");
    let s = |r: &toml::Value, k: &str| r.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    ledger
        .get("site")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .map(|r| Row {
            id: s(r, "id"),
            class: s(r, "class"),
            names: r.get("names").and_then(|v| v.as_integer()).unwrap_or(0) as usize,
            why: s(r, "why"),
        })
        .collect()
}

#[test]
fn every_name_keyed_property_site_is_ledgered() {
    let names = Names::load();
    assert!(
        names.fns.len() > 500 && names.modules.len() > 20,
        "stdlib names not found"
    );
    let found = sites(&names);
    let path = root().join(LEDGER);
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let rows = rows(&text);

    let mut problems = Vec::new();
    let mut listed = BTreeSet::new();
    for r in &rows {
        let now = found.get(&r.id).map(|g| g.len());
        match (r.class.as_str(), now) {
            (_, None) => problems.push(format!("{}: no such site any more — remove the row", r.id)),
            ("legacy", Some(n)) if n > r.names => problems.push(format!(
                "{}: groups {n} stdlib names, the ledger allows {} — a legacy site may only shrink. \
                 Declare the property at the functions instead of adding names here",
                r.id, r.names
            )),
            ("legacy", _) => {}
            ("justified", _) if r.why.trim().is_empty() => {
                problems.push(format!("{}: `justified` needs a `why` — why can these names not be a declaration?", r.id))
            }
            ("justified", _) => {}
            (other, _) => problems.push(format!("{}: class {other:?} (legacy | justified)", r.id)),
        }
        listed.insert(r.id.clone());
    }
    let new: Vec<&String> = found.keys().filter(|s| !listed.contains(*s)).collect();
    for s in &new {
        problems.push(format!(
            "{s}: a NEW name-keyed site ({}). Declare the property at the functions (an attribute \
             such as @consume / @borrow_ref / @reuse, read by every leg) instead of listing their \
             names; if a name list is genuinely right here, add a `justified` row saying why",
            found[*s].iter().cloned().collect::<Vec<_>>().join(", ")
        ));
    }

    if bless() {
        let mut out = String::from(HEADER);
        for r in &rows {
            let Some(g) = found.get(&r.id) else { continue };
            // A legacy row's allowance follows the site down, never up.
            let n = if r.class == "legacy" {
                g.len().min(r.names)
            } else {
                g.len()
            };
            out.push_str(&row_text(&r.id, &r.class, n, &r.why));
        }
        for s in &new {
            out.push_str(&row_text(s, "justified", found[*s].len(), ""));
        }
        std::fs::write(&path, out).expect("bless");
    }
    assert!(
        problems.is_empty(),
        "name-keyed property sites:\n  {}",
        problems.join("\n  ")
    );
}

const HEADER: &str = "\
# Name-keyed property sites (tests/name_keyed_sites_test.rs): items in the
# compiler that group stdlib names instead of reading a declaration.
# `names` is how many stdlib names the item groups. A `legacy` row predates
# the gate (2026-10-11): its count may only fall, and the row goes when the
# site does. A new site needs `class = \"justified\"` and a `why`.
";

fn row_text(id: &str, class: &str, names: usize, why: &str) -> String {
    let mut t = format!(
        "\n[[site]]\nid = {}\nclass = {}\nnames = {names}\n",
        toml::Value::from(id),
        toml::Value::from(class)
    );
    if class != "legacy" {
        t.push_str(&format!("why = {}\n", toml::Value::from(why)));
    }
    t
}

fn bless() -> bool {
    std::env::var("ALMIDE_UPDATE_NAME_SITES").is_ok_and(|v| !v.is_empty() && v != "0")
}
