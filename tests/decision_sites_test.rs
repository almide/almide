//! The decision-site detector: every place in the compiler, the CLI or the
//! runtime glue whose outcome is PASS or REFUSE for a capability, scope,
//! trust, purity, verification or ownership property is a row of
//! `proofs/decision-sites.toml` (the judge that reads the rows' evidence is
//! `tools/almide-gates decision-sites`; this test is its enumerator half).
//!
//! A site is found by what it EMITS, never by its name:
//!
//! * S1 — a string literal that is a diagnostic code whose row in
//!   `docs/diagnostics/codes.toml` says `decision = "<domain>"` (every code
//!   row must carry the column; `"no"` is a classification, a missing column
//!   is a failure);
//! * S2 — a construction of a refusal carrier the ledger's `[detector]`
//!   table declares (a refusal that never becomes a diagnostic code: a host
//!   errno, a scope error type);
//! * S3 — a string literal that is a registered switch
//!   (`almide_base::env::SWITCHES`) whose declared scope is `Gate` (it
//!   "bypasses or relaxes a check"), or any registered switch read inside an
//!   item that also holds an S1/S2 key (a switch read where a refusal is
//!   decided, whatever scope it declares). The other build-changing scopes
//!   (route, ablation, trap, instrument) are one decision, the switch lock,
//!   which refuses them by their declared scope (row `env.switch-lock`);
//! * S4 — a variant of a `clap` `Subcommand` enum: not a site but a ROUTE,
//!   each of which the ledger's `[routes]` table maps to a route column or
//!   to `n.a.: <why>`.
//!
//! Keys are `file::item::signal::payload` (no line numbers). A row owns the
//! keys found in its `items` (and the S3 keys of its `switches`); its `keys`
//! list must equal what is found there, so a new code in an old decision item
//! is a reviewed ledger diff. A key no row owns is a NEW decision site. A row
//! whose items, switches and files are all gone is stale.
//!
//! `ALMIDE_UPDATE_DECISION_SITES=1` rewrites each row's `keys`, appends every
//! unowned site as an UNCLASSIFIED row and a missing route as `unknown`, and
//! still fails until those are classified by hand. `ALMIDE_DECISION_SITES_TSV`
//! (a file path) writes the found keys for the Almide judge.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use syn::visit::{self, Visit};
use syn::{Expr, LitStr};

const LEDGER: &str = "proofs/decision-sites.toml";
const CODES: &str = "docs/diagnostics/codes.toml";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn is_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("cfg") && a.parse_args::<syn::Meta>().is_ok_and(|m| m.path().is_ident("test"))
    })
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
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

/// What the scan reads from the declarations it keys on.
struct Decls {
    /// code -> its `decision` column.
    codes: BTreeMap<String, String>,
    /// path suffixes whose construction is a refusal (`[detector] carriers`).
    carriers: Vec<String>,
}

fn is_code(s: &str) -> bool {
    s.len() == 4 && s.starts_with('E') && s[1..].chars().all(|c| c.is_ascii_digit())
}

struct Finder<'d> {
    decls: &'d Decls,
    file: String,
    items: Vec<String>,
    /// every item seen, `file::item`
    seen_items: BTreeSet<String>,
    /// `file::item` -> (signal, payload)
    keys: BTreeMap<String, BTreeSet<(String, String)>>,
    /// `file::item` -> switches read there whose scope is not `Gate`
    soft_switches: BTreeMap<String, BTreeSet<String>>,
    /// every registered switch spelled anywhere outside the registry
    switch_reads: BTreeSet<String>,
}

impl Finder<'_> {
    fn here(&self) -> String {
        let item = if self.items.is_empty() { "<file>".to_string() } else { self.items.join("::") };
        format!("{}::{}", self.file, item)
    }

    fn scoped(&mut self, name: String, f: impl FnOnce(&mut Self)) {
        self.items.push(name);
        let h = self.here();
        self.seen_items.insert(h);
        f(self);
        self.items.pop();
    }

    fn literal(&mut self, s: &str) {
        let here = self.here();
        if is_code(s) {
            if let Some(d) = self.decls.codes.get(s)
                && d != "no"
            {
                self.keys.entry(here).or_default().insert(("S1".into(), s.to_string()));
            }
            return;
        }
        // The registry's own declaration is not a read.
        if self.file == "crates/almide-base/src/env.rs" && self.items.first().is_some_and(|i| i == "SWITCHES") {
            return;
        }
        if let Some(sw) = almide_base::env::switch(s) {
            self.switch_reads.insert(s.to_string());
            if sw.scope == almide_base::env::Scope::Gate {
                self.keys.entry(here).or_default().insert(("S3".into(), s.to_string()));
            } else {
                self.soft_switches.entry(here).or_default().insert(s.to_string());
            }
        }
    }

    fn path(&mut self, p: &syn::Path) {
        let segs: Vec<String> = p.segments.iter().map(|s| s.ident.to_string()).collect();
        let text = segs.join("::");
        for c in &self.decls.carriers {
            let hit = text == *c || text.ends_with(&format!("::{c}")) || text.starts_with(&format!("{c}::")) || text.contains(&format!("::{c}::"));
            if hit {
                let here = self.here();
                self.keys.entry(here).or_default().insert(("S2".into(), c.clone()));
            }
        }
    }

    /// Literals and paths inside a macro body (`format!`, `vec!`, `println!`, ...).
    fn macro_body(&mut self, m: &syn::Macro) {
        let parsed = m.parse_body_with(syn::punctuated::Punctuated::<Expr, syn::Token![,]>::parse_terminated);
        if let Ok(exprs) = parsed {
            for e in exprs.iter() {
                self.visit_expr(e);
            }
        }
    }
}

fn impl_name(i: &syn::ItemImpl) -> String {
    match &*i.self_ty {
        syn::Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()).unwrap_or_default(),
        _ => "<impl>".to_string(),
    }
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
    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        let name = impl_name(i);
        self.items.push(name);
        visit::visit_item_impl(self, i);
        self.items.pop();
    }
    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        if is_cfg_test(&f.attrs) {
            return;
        }
        self.scoped(f.sig.ident.to_string(), |s| visit::visit_impl_item_fn(s, f));
    }
    fn visit_item_const(&mut self, c: &'ast syn::ItemConst) {
        self.scoped(c.ident.to_string(), |s| visit::visit_item_const(s, c));
    }
    fn visit_item_static(&mut self, c: &'ast syn::ItemStatic) {
        self.scoped(c.ident.to_string(), |s| visit::visit_item_static(s, c));
    }
    fn visit_item_trait(&mut self, t: &'ast syn::ItemTrait) {
        self.scoped(t.ident.to_string(), |s| visit::visit_item_trait(s, t));
    }
    fn visit_lit_str(&mut self, l: &'ast LitStr) {
        self.literal(&l.value());
    }
    // Only an EXPRESSION path constructs a carrier; naming it in a type does not.
    fn visit_expr_path(&mut self, p: &'ast syn::ExprPath) {
        self.path(&p.path);
        visit::visit_expr_path(self, p);
    }
    fn visit_expr_struct(&mut self, s: &'ast syn::ExprStruct) {
        self.path(&s.path);
        visit::visit_expr_struct(self, s);
    }
    fn visit_pat(&mut self, p: &'ast syn::Pat) {
        // A pattern MATCHES a carrier, it does not construct one; its literals
        // (a code matched in an arm) are not emitted either.
        let _ = p;
    }
    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        self.macro_body(m);
    }
}

struct Scan {
    /// `file::item` -> keys
    keys: BTreeMap<String, BTreeSet<String>>,
    items: BTreeSet<String>,
    switch_reads: BTreeSet<String>,
}

fn scan(decls: &Decls) -> Scan {
    let mut files = Vec::new();
    rust_files(&root().join("src"), &mut files);
    rust_files(&root().join("runtime/rs/src"), &mut files);
    for e in std::fs::read_dir(root().join("crates")).expect("crates/").flatten() {
        rust_files(&e.path().join("src"), &mut files);
    }
    files.sort();
    let mut keys: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut items = BTreeSet::new();
    let mut switch_reads = BTreeSet::new();
    for f in files {
        let src = std::fs::read_to_string(&f).expect("source");
        let ast = syn::parse_file(&src).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        let rel = f.strip_prefix(root()).expect("under root").to_string_lossy().replace('\\', "/");
        let mut finder = Finder {
            decls,
            file: rel,
            items: Vec::new(),
            seen_items: BTreeSet::new(),
            keys: BTreeMap::new(),
            soft_switches: BTreeMap::new(),
            switch_reads: BTreeSet::new(),
        };
        finder.visit_file(&ast);
        // A switch of any scope read where a refusal is decided is a way to turn it off.
        for (item, sws) in &finder.soft_switches {
            if finder.keys.get(item).is_some_and(|k| k.iter().any(|(s, _)| s == "S1" || s == "S2")) {
                for sw in sws {
                    finder.keys.get_mut(item).unwrap().insert(("S3".into(), sw.clone()));
                }
            }
        }
        for (item, ks) in finder.keys {
            let e = keys.entry(item.clone()).or_default();
            for (sig, p) in ks {
                e.insert(format!("{item}::{sig}::{p}"));
            }
        }
        items.extend(finder.seen_items);
        switch_reads.extend(finder.switch_reads);
    }
    Scan { keys, items, switch_reads }
}

/// Every variant of a `#[derive(Subcommand)]` enum: `Enum::Variant`.
fn routes() -> BTreeSet<String> {
    let mut files = Vec::new();
    rust_files(&root().join("src"), &mut files);
    let mut out = BTreeSet::new();
    for f in files {
        let src = std::fs::read_to_string(&f).expect("source");
        let ast = syn::parse_file(&src).expect("parses");
        for item in ast.items {
            let syn::Item::Enum(e) = item else { continue };
            let derives_subcommand = e.attrs.iter().any(|a| {
                a.path().is_ident("derive")
                    && a.parse_args_with(syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated)
                        .is_ok_and(|ps| ps.iter().any(|p| p.segments.last().is_some_and(|s| s.ident == "Subcommand")))
            });
            if derives_subcommand {
                for v in &e.variants {
                    out.insert(format!("{}::{}", e.ident, v.ident));
                }
            }
        }
    }
    out
}

fn str_list(v: Option<&toml::Value>) -> Vec<String> {
    v.and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

struct Row {
    id: String,
    items: Vec<String>,
    switches: Vec<String>,
    files: Vec<String>,
    keys: Vec<String>,
}

fn key_switch(k: &str) -> Option<&str> {
    let mut parts = k.rsplitn(3, "::");
    let payload = parts.next()?;
    (parts.next()? == "S3").then_some(payload)
}

fn key_item(k: &str) -> &str {
    // file::item...::Sx::payload
    let mut it = k.rsplitn(3, "::");
    it.next();
    it.next();
    it.next().unwrap_or("")
}

#[test]
fn every_decision_site_is_ledgered() {
    let codes_text = std::fs::read_to_string(root().join(CODES)).expect("codes.toml");
    let codes_tbl: toml::Table = toml::from_str(&codes_text).expect("codes.toml parses");
    let mut problems = Vec::new();
    let mut codes = BTreeMap::new();
    for (code, row) in &codes_tbl {
        match row.get("decision").and_then(|v| v.as_str()) {
            Some(d) if !d.trim().is_empty() => {
                codes.insert(code.clone(), d.to_string());
            }
            _ => problems.push(format!(
                "{CODES} [{code}]: no `decision` column — classify the code: the domain it refuses for (`decision = \"effect\"`, ...) or `decision = \"no\"`"
            )),
        }
    }

    let path = root().join(LEDGER);
    let text = std::fs::read_to_string(&path).expect("ledger");
    let ledger: toml::Table = toml::from_str(&text).expect("ledger parses");
    let detector = ledger.get("detector").and_then(|v| v.as_table()).expect("[detector] table");
    let decls = Decls { codes, carriers: str_list(detector.get("carriers")) };
    assert!(!decls.carriers.is_empty(), "[detector] carriers is empty");

    let Scan { keys: found, items, switch_reads } = scan(&decls);
    let n_keys: usize = found.values().map(|k| k.len()).sum();
    assert!(n_keys > 50, "only {n_keys} decision keys found — the scan is broken");

    let rows: Vec<Row> = ledger
        .get("site")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .map(|r| Row {
            id: r.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            items: str_list(r.get("items")),
            switches: str_list(r.get("switches")),
            files: str_list(r.get("files")),
            keys: str_list(r.get("keys")),
        })
        .collect();

    let all_keys: BTreeSet<&String> = found.values().flatten().collect();
    let mut owned: BTreeSet<String> = BTreeSet::new();
    let mut want_keys: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut ids = BTreeSet::new();
    for r in &rows {
        if !ids.insert(r.id.clone()) {
            problems.push(format!("{}: duplicate id", r.id));
        }
        let live_items: Vec<&String> = r.items.iter().filter(|i| items.contains(*i)).collect();
        let live_files: Vec<&String> = r.files.iter().filter(|f| root().join(f).exists()).collect();
        for i in r.items.iter().filter(|i| !items.contains(*i)) {
            problems.push(format!("{}: item {i} is gone — point the row at where the decision lives now", r.id));
        }
        for f in r.files.iter().filter(|f| !root().join(f).exists()) {
            problems.push(format!("{}: file {f} is gone", r.id));
        }
        let mut mine: BTreeSet<String> = BTreeSet::new();
        for i in &r.items {
            if let Some(ks) = found.get(i) {
                mine.extend(ks.iter().cloned());
            }
        }
        for k in &all_keys {
            if key_switch(k).is_some_and(|s| r.switches.iter().any(|w| w == s)) {
                mine.insert((*k).clone());
            }
        }
        for s in &r.switches {
            if almide_base::env::switch(s).is_none() {
                problems.push(format!("{}: switch {s} is not registered", r.id));
            }
        }
        let live_switches = r.switches.iter().filter(|s| switch_reads.contains(*s)).count();
        if live_items.is_empty() && live_files.is_empty() && mine.is_empty() && live_switches == 0 {
            problems.push(format!("{}: stale — none of its items, files or switch reads exists", r.id));
        }
        let have: BTreeSet<String> = r.keys.iter().cloned().collect();
        if have != mine {
            let added: Vec<&String> = mine.difference(&have).collect();
            let gone: Vec<&String> = have.difference(&mine).collect();
            problems.push(format!(
                "{}: keys changed (new {added:?}, gone {gone:?}) — a decision changed in this row's items; review it and update `keys`",
                r.id
            ));
        }
        owned.extend(mine.iter().cloned());
        want_keys.insert(r.id.clone(), mine.into_iter().collect());
    }

    // Keys nobody owns: new decision sites.
    // Grouped by what they emit (`S1::E006`): one decision usually spans several items.
    let mut new_sites: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for k in all_keys.iter().filter(|k| !owned.contains(**k)) {
        let mut parts = k.rsplitn(3, "::");
        let payload = parts.next().unwrap_or("");
        let sig = parts.next().unwrap_or("");
        new_sites.entry(format!("{sig}.{payload}")).or_default().push((*k).clone());
    }
    for (what, ks) in &new_sites {
        problems.push(format!(
            "{what}: NEW decision site(s) {} — add a row (from, unknown, routes, tests, control) to {LEDGER}",
            ks.join(", ")
        ));
    }

    // S4: every route of the CLI is a column or says why not.
    let route_map = ledger.get("routes").and_then(|v| v.as_table()).cloned().unwrap_or_default();
    let cli = routes();
    assert!(cli.len() > 20, "only {} CLI routes found", cli.len());
    let mut missing_routes = Vec::new();
    for r in &cli {
        match route_map.get(r).and_then(|v| v.as_str()) {
            None => {
                missing_routes.push(r.clone());
                problems.push(format!("[routes] {r}: a CLI route the ledger does not map — name its column or `n.a.: <why>`"));
            }
            Some(v) if v == "unknown" || v.trim().is_empty() => {
                problems.push(format!("[routes] {r} = {v:?}: decide its column or `n.a.: <why>`"))
            }
            Some(_) => {}
        }
    }
    for r in route_map.keys().filter(|r| !cli.contains(*r)) {
        problems.push(format!("[routes] {r}: no such CLI route any more"));
    }

    if let Ok(out) = std::env::var("ALMIDE_DECISION_SITES_TSV")
        && !out.trim().is_empty()
    {
        let mut tsv = String::from("key\titem\towner\n");
        for k in &all_keys {
            let owner: Vec<&String> = want_keys.iter().filter(|(_, ks)| ks.contains(k)).map(|(id, _)| id).collect();
            tsv.push_str(&format!(
                "{k}\t{}\t{}\n",
                key_item(k),
                owner.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(",")
            ));
        }
        std::fs::write(&out, tsv).expect("write tsv");
        let all_items: Vec<&str> = items.iter().map(String::as_str).collect();
        std::fs::write(format!("{out}.items"), all_items.join("\n")).expect("write items");
    }

    if update() {
        let mut out = rewrite_keys(&text, &want_keys);
        for r in &missing_routes {
            out = add_route(&out, r);
        }
        for (what, ks) in &new_sites {
            out.push_str(&new_row(what, ks));
        }
        std::fs::write(&path, out).expect("update ledger");
    }
    assert!(problems.is_empty(), "decision sites:\n  {}", problems.join("\n  "));
}

fn update() -> bool {
    std::env::var("ALMIDE_UPDATE_DECISION_SITES").is_ok_and(|v| almide_base::env::is_on(std::ffi::OsStr::new(&v)))
}

fn toml_list(v: &[String]) -> String {
    if v.is_empty() {
        return "[]".to_string();
    }
    let inner: Vec<String> = v.iter().map(|s| toml::Value::from(s.as_str()).to_string()).collect();
    format!("[\n  {},\n]", inner.join(",\n  "))
}

/// Replace (or add) the `keys = [...]` of each row, keeping the rest of the text.
fn rewrite_keys(text: &str, want: &BTreeMap<String, Vec<String>>) -> String {
    let mut out = String::new();
    let blocks: Vec<&str> = text.split("\n[[site]]\n").collect();
    out.push_str(blocks[0]);
    for b in &blocks[1..] {
        out.push_str("\n[[site]]\n");
        let id = b
            .lines()
            .find_map(|l| l.strip_prefix("id = "))
            .and_then(|v| toml::from_str::<toml::Table>(&format!("v = {v}")).ok())
            .and_then(|t| t.get("v").and_then(|v| v.as_str()).map(str::to_string))
            .unwrap_or_default();
        let Some(keys) = want.get(&id) else {
            out.push_str(b);
            continue;
        };
        let new_keys = format!("keys = {}", toml_list(keys));
        let mut lines: Vec<String> = Vec::new();
        let mut it = b.lines().peekable();
        let mut replaced = false;
        while let Some(l) = it.next() {
            if l.starts_with("keys = ") {
                // Skip a multi-line array.
                if l.trim_end().ends_with('[') || (l.contains('[') && !l.contains(']')) {
                    for m in it.by_ref() {
                        if m.trim_start().starts_with(']') {
                            break;
                        }
                    }
                }
                lines.push(new_keys.clone());
                replaced = true;
            } else {
                lines.push(l.to_string());
                if l.starts_with("id = ") && !replaced && !b.contains("\nkeys = ") {
                    lines.push(new_keys.clone());
                    replaced = true;
                }
            }
        }
        out.push_str(&lines.join("\n"));
        if b.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

fn add_route(text: &str, route: &str) -> String {
    text.replacen("\n[routes]\n", &format!("\n[routes]\n{} = \"unknown\"\n", toml::Value::from(route)), 1)
}

fn new_row(what: &str, keys: &[String]) -> String {
    let id = format!("unclassified.{}", what.to_lowercase().replace('_', "-"));
    let mut items: Vec<String> = keys.iter().map(|k| key_item(k).to_string()).collect();
    items.dedup();
    format!(
        "\n[[site]]\nid = {}\nitems = {}\nkeys = {}\ndecides = \"\"\nfrom = \"\"\nunknown = \"\"\nstatus = \"UNCLASSIFIED\"\nreasons = [\"detector: found by the first run, not yet read\"]\n",
        toml::Value::from(id.as_str()),
        toml_list(&items),
        toml_list(keys)
    )
}
