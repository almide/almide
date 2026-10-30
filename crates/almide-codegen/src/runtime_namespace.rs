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
        let mut out = Vec::new();
        let mut mods: Vec<(String, Vec<TopItem>)> = Vec::new();
        let mut globs: Vec<(String, Vis, Vec<String>)> = Vec::new();
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
            let next = self.item(j, end, vis, &mut cfgs, &mut out, &mut mods, &mut globs);
            cfgs.clear();
            i = next.max(i + 1);
        }
        for (path, vis, cfgs) in globs {
            let Some((_, inner)) = mods.iter().find(|(m, _)| *m == path) else { continue };
            for it in inner.iter().filter(|it| it.vis != Vis::Private) {
                let vis = if vis == Vis::Pub && it.vis == Vis::Pub { Vis::Pub } else { vis.min_crate() };
                out.push(TopItem { name: it.name.clone(), vis, cfgs: [cfgs.clone(), it.cfgs.clone()].concat() });
            }
        }
        out
    }

    /// Record what the item at `j` binds; the index past it.
    #[allow(clippy::too_many_arguments)]
    fn item(
        &self,
        j: usize,
        end: usize,
        vis: Vis,
        cfgs: &mut Vec<String>,
        out: &mut Vec<TopItem>,
        mods: &mut Vec<(String, Vec<TopItem>)>,
        globs: &mut Vec<(String, Vis, Vec<String>)>,
    ) -> usize {
        let item_end = self.item_end(j, end);
        match self.tok(j) {
            "fn" | "struct" | "enum" | "union" | "trait" | "type" | "const" | "static" | "mod" => {
                let k = if self.tok(j + 1) == "mut" { j + 2 } else { j + 1 };
                let name = self.tok(k);
                if self.tok(j) == "mod" {
                    if let Some(open) = self.body_open(k, end) {
                        let close = self.group_end(open, end);
                        mods.push((name.to_string(), self.items(open + 1, close - 1)));
                    }
                }
                if self.is_ident(k) && name != "_" {
                    out.push(TopItem { name: name.to_string(), vis, cfgs: cfgs.clone() });
                }
                item_end
            }
            "use" => {
                self.use_names(j + 1, item_end, vis, cfgs, out, globs);
                item_end
            }
            "extern" if self.tok(j + 1) == "{" => {
                let close = self.group_end(j + 1, end);
                for mut it in self.items(j + 2, close - 1) {
                    it.cfgs = [cfgs.clone(), it.cfgs].concat();
                    out.push(it);
                }
                close
            }
            "thread_local" if self.tok(j + 1) == "!" => {
                let close = self.group_end(j + 2, end);
                let mut k = j + 3;
                while k < close {
                    let (svis, s) = self.visibility(k, close);
                    if self.tok(s) == "static" && self.is_ident(s + 1) {
                        out.push(TopItem { name: self.tok(s + 1).to_string(), vis: svis, cfgs: cfgs.clone() });
                    }
                    k = self.item_end(s, close).max(k + 1);
                }
                close
            }
            _ => item_end,
        }
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
        out: &mut Vec<TopItem>,
        globs: &mut Vec<(String, Vis, Vec<String>)>,
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
                        globs.push(((*single).to_string(), vis, cfgs.to_vec()));
                    }
                } else if name != "_" && name != "self" {
                    out.push(TopItem { name, vis, cfgs: cfgs.to_vec() });
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
