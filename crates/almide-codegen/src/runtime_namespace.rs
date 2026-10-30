//! The runtime's namespace in the generated Rust crate (#4345).
//!
//! The native runtime is spliced into the user's crate as source. Spliced
//! flat, every helper it defines — `mk` in matrix.rs, `sha256`, `request`,
//! `json_quote`, the `FsBeneathError` of a shared core — sat at the crate
//! root beside the user's items: a user `fn mk` in a program that reached
//! `matrix` was rustc E0428 after a green `almide check`. Renaming helpers one
//! by one (#1821, #3487, #4282) never closes that, because the next helper
//! reopens it.
//!
//! So the runtime modules' bodies live in ONE private module,
//! [`RUNTIME_MODULE`], which sees the crate root through `use super::*` —
//! inside it everything resolves exactly as it did flat (the runtime's own
//! items shadow anything the glob brings in) — and the crate root gets back
//! only the items whose names lie in the runtime's namespace,
//! `almide_ir::is_runtime_item_name`: one `pub use` each. A helper outside
//! that namespace is unreachable from user code, and no user item can collide
//! with what the root does hold. The guard test below holds the WHOLE
//! crate-root preamble (prelude, re-exports, the kernel) to the same
//! predicate, so a new prelude item cannot reopen the class either.

use crate::rust_idents;

/// The private module the spliced runtime modules' bodies live in.
pub(crate) const RUNTIME_MODULE: &str = "almide_runtime";

/// An item's declared visibility, as far as a re-export cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Vis {
    Private,
    Crate,
    Pub,
}

/// A name a top-level item (or a top-level `use`) binds in its module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TopItem {
    pub name: String,
    pub vis: Vis,
    /// The item's `#[cfg(…)]` attributes, verbatim: its re-export must exist
    /// exactly when it does.
    pub cfgs: Vec<String>,
}

type Tok<'a> = (usize, &'a str);

/// The names Rust source `src` binds at its top level: items (also those of
/// an `extern { … }` block and the statics of a `thread_local!`), the names
/// a `use` binds, and the public items a `use local_mod::*` glob brings in.
/// Impl blocks and other macro invocations bind nothing here.
pub(crate) fn top_level_items(src: &str) -> Vec<TopItem> {
    let mut lexer = rust_idents::tokens(src);
    let toks: Vec<Tok> = std::iter::from_fn(|| lexer.next_token()).collect();
    Scan { src, toks: &toks }.items(0, toks.len())
}

/// The re-exports that give the crate root back the runtime-namespace items
/// of `bodies` (the spliced module bodies, which live in [`RUNTIME_MODULE`]).
/// Private items cannot be re-exported and are not: nothing outside the
/// runtime names them.
pub(crate) fn runtime_reexports(bodies: &str) -> String {
    let mut seen = std::collections::HashSet::new();
    let mut out = String::new();
    for it in top_level_items(bodies) {
        let vis = match it.vis {
            Vis::Pub => "pub use",
            Vis::Crate => "pub(crate) use",
            Vis::Private => continue,
        };
        if !almide_ir::is_runtime_item_name(&it.name) || !seen.insert((it.name.clone(), it.cfgs.clone())) {
            continue;
        }
        for cfg in &it.cfgs {
            out.push_str(cfg);
            out.push('\n');
        }
        out.push_str(&format!("{vis} {RUNTIME_MODULE}::{};\n", it.name));
    }
    out
}

/// What a scan of one module level collects.
#[derive(Default)]
struct Acc {
    out: Vec<TopItem>,
    /// Each local `mod`'s items, for a `use m::*` glob.
    mods: Vec<(String, Vec<TopItem>)>,
    globs: Vec<(String, Vis, Vec<String>)>,
    /// Each local `macro_rules!`: [`Scan::macro_items`].
    macros: Vec<(String, Vec<(usize, Vis)>)>,
    /// Top-level invocations of a macro that is not local.
    unresolved: Vec<String>,
}

/// The macros invoked at the top level of `src` whose items the scanner
/// cannot see (not a local `macro_rules!`, not `thread_local!`).
pub(crate) fn unresolved_top_level_macros(src: &str) -> Vec<String> {
    let mut lexer = rust_idents::tokens(src);
    let toks: Vec<Tok> = std::iter::from_fn(|| lexer.next_token()).collect();
    Scan { src, toks: &toks }.scan(0, toks.len()).unresolved
}

/// The `macro_rules!` macros `src` defines at its top level, each with the
/// number of items its transcriber declares from a `$name` argument.
pub(crate) fn top_level_macros(src: &str) -> Vec<(String, usize)> {
    let mut lexer = rust_idents::tokens(src);
    let toks: Vec<Tok> = std::iter::from_fn(|| lexer.next_token()).collect();
    Scan { src, toks: &toks }.scan(0, toks.len()).macros.into_iter().map(|(m, d)| (m, d.len())).collect()
}

struct Scan<'s, 't> {
    src: &'s str,
    toks: &'t [Tok<'s>],
}

impl<'s> Scan<'s, '_> {
    fn tok(&self, i: usize) -> &'s str {
        self.toks.get(i).map_or("", |t| t.1)
    }

    fn is_ident(&self, i: usize) -> bool {
        self.tok(i).starts_with(|c: char| c == '_' || c.is_alphabetic())
    }

    /// The index just past the group opened at `open` (`(`, `[` or `{`).
    fn group_end(&self, open: usize, end: usize) -> usize {
        let mut depth = 0usize;
        for i in open..end {
            match self.tok(i) {
                "(" | "[" | "{" => depth += 1,
                ")" | "]" | "}" => {
                    if depth <= 1 {
                        return i + 1;
                    }
                    depth -= 1;
                }
                _ => {}
            }
        }
        end
    }

    /// The index just past the item starting at `from`: its `;` at depth 0,
    /// or the close of its first top-level brace block.
    fn item_end(&self, from: usize, end: usize) -> usize {
        let mut i = from;
        while i < end {
            match self.tok(i) {
                ";" => return i + 1,
                "{" => return self.group_end(i, end),
                "(" | "[" => i = self.group_end(i, end),
                _ => i += 1,
            }
        }
        end
    }

    /// The first `{` at depth 0 in `from..end`.
    fn body_open(&self, from: usize, end: usize) -> Option<usize> {
        let mut i = from;
        while i < end {
            match self.tok(i) {
                "{" => return Some(i),
                ";" => return None,
                "(" | "[" => i = self.group_end(i, end),
                _ => i += 1,
            }
        }
        None
    }

    /// `pub`, `pub(crate)`, `pub(super)`, `pub(in …)` or nothing at `i`.
    fn visibility(&self, i: usize, end: usize) -> (Vis, usize) {
        if self.tok(i) != "pub" {
            return (Vis::Private, i);
        }
        if self.tok(i + 1) == "(" {
            return (Vis::Crate, self.group_end(i + 1, end));
        }
        (Vis::Pub, i + 1)
    }

    /// Skip the qualifiers between a visibility and the item keyword
    /// (`unsafe`, `async`, `const fn`, `extern "C" fn`; the ABI string is not
    /// a token).
    fn qualifiers(&self, mut j: usize) -> usize {
        loop {
            match (self.tok(j), self.tok(j + 1)) {
                ("unsafe" | "async" | "default" | "safe", _) => j += 1,
                ("const", "fn" | "unsafe" | "async" | "extern") => j += 1,
                ("extern", next) if next != "{" && next != "crate" => j += 1,
                _ => return j,
            }
        }
    }

    fn items(&self, start: usize, end: usize) -> Vec<TopItem> {
        self.scan(start, end).out
    }

    fn scan(&self, start: usize, end: usize) -> Acc {
        let mut acc = Acc::default();
        let mut cfgs: Vec<String> = Vec::new();
        let mut i = start;
        while i < end {
            if self.tok(i) == "#" {
                let open = if self.tok(i + 1) == "!" { i + 2 } else { i + 1 };
                let close = self.group_end(open, end);
                if self.tok(open + 1) == "cfg" {
                    let (a, b) = (self.toks[i].0, self.toks[close - 1].0 + 1);
                    cfgs.push(self.src[a..b].to_string());
                }
                i = close;
                continue;
            }
            let (vis, j) = self.visibility(i, end);
            let j = self.qualifiers(j);
            let next = self.item(j, end, vis, &cfgs, &mut acc);
            cfgs.clear();
            i = next.max(i + 1);
        }
        for (path, vis, cfgs) in std::mem::take(&mut acc.globs) {
            let Some((_, inner)) = acc.mods.iter().find(|(m, _)| *m == path) else { continue };
            let found: Vec<TopItem> = inner
                .iter()
                .filter(|it| it.vis != Vis::Private)
                .map(|it| {
                    let vis = if vis == Vis::Pub && it.vis == Vis::Pub { Vis::Pub } else { vis.min_crate() };
                    TopItem { name: it.name.clone(), vis, cfgs: [cfgs.clone(), it.cfgs.clone()].concat() }
                })
                .collect();
            acc.out.extend(found);
        }
        acc
    }

    /// Record what the item at `j` binds; the index past it.
    fn item(&self, j: usize, end: usize, vis: Vis, cfgs: &[String], acc: &mut Acc) -> usize {
        let item_end = self.item_end(j, end);
        match (self.tok(j), self.tok(j + 1)) {
            ("fn" | "struct" | "enum" | "union" | "trait" | "type" | "const" | "static" | "mod", _) => {
                let k = if self.tok(j + 1) == "mut" { j + 2 } else { j + 1 };
                let name = self.tok(k);
                if self.tok(j) == "mod" {
                    if let Some(open) = self.body_open(k, end) {
                        let close = self.group_end(open, end);
                        acc.mods.push((name.to_string(), self.items(open + 1, close - 1)));
                    }
                }
                if self.is_ident(k) && name != "_" {
                    acc.out.push(TopItem { name: name.to_string(), vis, cfgs: cfgs.to_vec() });
                }
                item_end
            }
            ("use", _) => {
                self.use_names(j + 1, item_end, vis, cfgs, acc);
                item_end
            }
            ("extern", "{") => {
                let close = self.group_end(j + 1, end);
                for mut it in self.items(j + 2, close - 1) {
                    it.cfgs = [cfgs.to_vec(), it.cfgs].concat();
                    acc.out.push(it);
                }
                close
            }
            ("thread_local", "!") => self.thread_local(j, end, cfgs, acc),
            ("macro_rules", "!") => {
                let def = self.macro_items(j + 3, item_end);
                acc.macros.push((self.tok(j + 2).to_string(), def));
                item_end
            }
            (name, "!") if self.is_ident(j) => {
                self.macro_call(name, j + 2, item_end, cfgs, acc);
                item_end
            }
            _ => item_end,
        }
    }

    fn thread_local(&self, j: usize, end: usize, cfgs: &[String], acc: &mut Acc) -> usize {
        let close = self.group_end(j + 2, end);
        let mut k = j + 3;
        while k < close {
            let (svis, s) = self.visibility(k, close);
            if self.tok(s) == "static" && self.is_ident(s + 1) {
                acc.out.push(TopItem { name: self.tok(s + 1).to_string(), vis: svis, cfgs: cfgs.to_vec() });
            }
            k = self.item_end(s, close).max(k + 1);
        }
        close
    }

    /// A local `macro_rules!` body (`from..end`): for each item its
    /// transcriber declares under a `$param` name (`pub fn $name(…)`), the
    /// position of that `ident` fragment among the matcher's fragments, and
    /// the item's visibility.
    fn macro_items(&self, from: usize, end: usize) -> Vec<(usize, Vis)> {
        let mut params: Vec<&str> = Vec::new();
        let mut out = Vec::new();
        for i in from..end {
            if self.tok(i) == "$" && self.tok(i + 2) == ":" && self.is_ident(i + 1) {
                params.push(self.tok(i + 1));
            }
            let declares = matches!(self.tok(i), "fn" | "struct" | "enum" | "const" | "static" | "type");
            if declares && self.tok(i + 1) == "$" {
                if let Some(pos) = params.iter().position(|p| *p == self.tok(i + 2)) {
                    let vis = if i > 0 && self.tok(i - 1) == "pub" { Vis::Pub } else { Vis::Private };
                    out.push((pos, vis));
                }
            }
        }
        out
    }

    /// A top-level invocation of macro `name` whose arguments are
    /// `from..end`: the items a local `macro_rules!` declares from them. An
    /// invocation of any other macro is recorded as unresolved, so the guard
    /// test can refuse an item the scanner cannot see.
    fn macro_call(&self, name: &str, from: usize, end: usize, cfgs: &[String], acc: &mut Acc) {
        let Some((_, def)) = acc.macros.iter().find(|(m, _)| m == name) else {
            acc.unresolved.push(name.to_string());
            return;
        };
        let close = self.group_end(from, end);
        let mut args: Vec<Vec<&str>> = vec![Vec::new()];
        let mut i = from + 1;
        while i + 1 < close {
            match self.tok(i) {
                "," => args.push(Vec::new()),
                "(" | "[" | "{" => {
                    let e = self.group_end(i, close);
                    if let Some(a) = args.last_mut() {
                        a.extend(self.toks[i..e].iter().map(|t| t.1));
                    }
                    i = e;
                    continue;
                }
                t => {
                    if let Some(a) = args.last_mut() {
                        a.push(t);
                    }
                }
            }
            i += 1;
        }
        let found: Vec<TopItem> = def
            .iter()
            .filter_map(|(pos, vis)| match args.get(*pos).map(Vec::as_slice) {
                Some([one]) => Some(TopItem { name: (*one).to_string(), vis: *vis, cfgs: cfgs.to_vec() }),
                _ => None,
            })
            .collect();
        acc.out.extend(found);
    }

    /// The names the use tree in `from..end` (up to its `;`) binds: each
    /// leaf's last segment or its `as` name; a `path::*` glob is resolved
    /// against this module's own `mod`s once they are all known.
    fn use_names(
        &self,
        from: usize,
        end: usize,
        vis: Vis,
        cfgs: &[String],
        acc: &mut Acc,
    ) {
        let leaf_end = |i: usize| matches!(self.tok(i), "," | "{" | "}" | ";" | "");
        let mut i = from;
        let mut leaf: Vec<&str> = Vec::new();
        while i < end {
            if !leaf_end(i) {
                leaf.push(self.tok(i));
                i += 1;
                continue;
            }
            let opens_group = self.tok(i) == "{";
            if let Some(name) = Self::leaf_name(&leaf, opens_group) {
                if name == "*" {
                    let path: Vec<&str> = leaf.iter().copied().filter(|t| *t != ":" && *t != "*").collect();
                    if let [single] = path.as_slice() {
                        acc.globs.push(((*single).to_string(), vis, cfgs.to_vec()));
                    }
                } else if name != "_" && name != "self" {
                    acc.out.push(TopItem { name, vis, cfgs: cfgs.to_vec() });
                }
            }
            leaf.clear();
            i += 1;
        }
    }

    fn leaf_name(leaf: &[&str], opens_group: bool) -> Option<String> {
        if opens_group || leaf.is_empty() {
            return None;
        }
        if let Some(p) = leaf.iter().position(|t| *t == "as") {
            return leaf.get(p + 1).map(|s| (*s).to_string());
        }
        leaf.last().filter(|t| **t != ":").map(|s| (*s).to_string())
    }
}

impl Vis {
    /// A visibility no wider than `pub(crate)`.
    fn min_crate(self) -> Vis {
        match self {
            Vis::Pub | Vis::Crate => Vis::Crate,
            Vis::Private => Vis::Private,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(src: &str) -> Vec<(String, Vis)> {
        top_level_items(src).into_iter().map(|it| (it.name, it.vis)).collect()
    }

    #[test]
    fn the_scanner_sees_top_level_items_only() {
        let src = r#"
            use std::io::Read as _;
            pub use a::{b, c as d};
            pub fn mk(x: i64) -> i64 { let s = "fn hidden() {}"; let c = '{'; x }
            /* fn commented() {} */
            pub(crate) const fn almide_x<const N: usize>() -> usize { N }
            struct S { f: i64 }
            impl S { pub fn method(&self) {} }
            static mut ALMIDE_Y: i64 = 0;
            const Z: S = S { f: 1 };
            extern "C" { fn signal(s: i32) -> i32; }
            thread_local! { pub static ALMIDE_T: i64 = 0; static U: i64 = 1; }
            macro_rules! m { () => { fn in_macro() {} } }
            mod inner { pub fn almide_rt_in() {} fn private_in() {} }
            pub use inner::*;
            #[cfg(unix)]
            pub fn almide_unix_only() {}
            pub unsafe extern "C" fn almide_c() {}
        "#;
        let got = names(src);
        let want: Vec<(&str, Vis)> = vec![
            ("b", Vis::Pub), ("d", Vis::Pub), ("mk", Vis::Pub), ("almide_x", Vis::Crate), ("S", Vis::Private),
            ("ALMIDE_Y", Vis::Private), ("Z", Vis::Private), ("signal", Vis::Private), ("ALMIDE_T", Vis::Pub),
            ("U", Vis::Private), ("inner", Vis::Private), ("almide_unix_only", Vis::Pub), ("almide_c", Vis::Pub),
            ("almide_rt_in", Vis::Pub),
        ];
        assert_eq!(got, want.into_iter().map(|(n, v)| (n.to_string(), v)).collect::<Vec<_>>());
        let unix = top_level_items(src).into_iter().find(|it| it.name == "almide_unix_only").unwrap();
        assert_eq!(unix.cfgs, vec!["#[cfg(unix)]".to_string()]);
    }

    #[test]
    fn only_runtime_namespace_items_are_reexported() {
        let re = runtime_reexports(
            "pub fn mk() {}\npub fn almide_rt_matrix_mul() {}\nfn almide_private() {}\n#[cfg(windows)]\npub fn almide_win() {}\npub struct AlmideMatrix;\npub fn request() {}\n",
        );
        assert_eq!(
            re,
            "pub use almide_runtime::almide_rt_matrix_mul;\n#[cfg(windows)]\npub use almide_runtime::almide_win;\npub use almide_runtime::AlmideMatrix;\n"
        );
    }

    /// THE guard (#4345): every name the generated crate's root holds before
    /// the user's code — the prelude, the hoisted `use`s, the runtime module,
    /// its re-exports, the kernel — lies in the runtime's namespace
    /// (`almide_ir::is_runtime_item_name`), so no user item can share it. Both
    /// renders: the inline one (every runtime module, the private prelude) and
    /// the rlib one (`emit_runtime_crate`, the `pub` prelude).
    #[test]
    fn no_crate_root_runtime_name_is_a_user_spelling() {
        let all: std::collections::HashSet<&str> =
            crate::generated::rust_runtime::RUST_RUNTIME_MODULES.iter().map(|(m, _)| *m).collect();
        let inline = format!(
            "{}{}\n{}",
            crate::rust_runtime_prelude(false),
            crate::rust_runtime_modules(&all),
            crate::generated::rust_runtime::ALMIDE_KERNEL_INLINE
        );
        for (render, root) in [("inline", inline), ("rlib", crate::emit_runtime_crate())] {
            let bad: Vec<String> = top_level_items(&root)
                .into_iter()
                .filter(|it| !almide_ir::is_runtime_item_name(&it.name))
                .map(|it| it.name)
                .collect();
            assert!(bad.is_empty(), "{render} runtime puts names a user item can spell at the crate root: {bad:?}");
        }
    }

    /// Every top-level macro invocation in the runtime and the prelude is one
    /// the scanner can read (a local `macro_rules!`, `thread_local!`), so no
    /// item reaches the crate root, or misses its re-export, unseen.
    #[test]
    fn every_top_level_macro_is_one_the_scanner_reads() {
        let all: std::collections::HashSet<&str> =
            crate::generated::rust_runtime::RUST_RUNTIME_MODULES.iter().map(|(m, _)| *m).collect();
        let mut bodies = crate::rust_runtime_modules(&all);
        let open = bodies.find("use super::*;").expect("runtime module");
        bodies = bodies[open..].to_string();
        // A prelude macro the runtime invokes is readable when it declares no
        // item (`almide_into_str_leaf!` writes impls only).
        let prelude = crate::rust_runtime_prelude(false);
        let itemless: Vec<String> =
            top_level_macros(&prelude).into_iter().filter(|(_, n)| *n == 0).map(|(m, _)| m).collect();
        for (what, src) in [("runtime modules", bodies), ("prelude", prelude)] {
            let mut unresolved = unresolved_top_level_macros(&src);
            unresolved.retain(|m| !itemless.contains(m));
            assert!(unresolved.is_empty(), "{what}: top-level macro invocations the item scanner cannot read: {unresolved:?}");
        }
    }

    #[test]
    fn local_macro_items_are_seen() {
        let src = "macro_rules! gen { ($name:ident, $ty:ty) => { pub fn $name(x: $ty) -> $ty { x } }; }\ngen!(almide_rt_a, i64);\ngen!(almide_rt_b, u8);\n";
        assert_eq!(names(src), vec![("almide_rt_a".to_string(), Vis::Pub), ("almide_rt_b".to_string(), Vis::Pub)]);
        assert!(unresolved_top_level_macros(src).is_empty());
        assert_eq!(unresolved_top_level_macros("other!(x);\n"), vec!["other".to_string()]);
    }

    /// The helpers that collided (#4345) stay inside the runtime module, and
    /// the runtime's API is back at the root.
    #[test]
    fn helpers_stay_private_and_the_api_is_reexported() {
        let all: std::collections::HashSet<&str> =
            crate::generated::rust_runtime::RUST_RUNTIME_MODULES.iter().map(|(m, _)| *m).collect();
        let root: Vec<String> = top_level_items(&crate::rust_runtime_modules(&all)).into_iter().map(|it| it.name).collect();
        for helper in ["mk", "sha256", "request", "json_quote", "decode_str", "FsBeneathError"] {
            assert!(!root.iter().any(|n| n == helper), "`{helper}` reached the crate root");
        }
        for api in ["almide_rt_matrix_mul", "AlmideMatrix", RUNTIME_MODULE] {
            assert!(root.iter().any(|n| n == api), "`{api}` is missing from the crate root");
        }
    }
}
