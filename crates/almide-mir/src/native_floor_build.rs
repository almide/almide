use std::collections::BTreeMap;
include!("native_floor_cfg.rs");
// Shared by build.rs and the generated-index verification test.
/// What the first scan over the runtime modules' top-level items collects.
#[derive(Default)]
struct RuntimeScan {
    items: HashMap<&'static str, RtItem>,
    seen: BTreeMap<&'static str, usize>,
    /// Whole modules and whether the runtime glob-imports their contents.
    /// Private module members stay scoped; a glob also exposes their names.
    modules: Vec<(&'static str, &'static str, bool)>,
    /// Runtime types and their impl blocks: a type travels with its
    /// inherent impls and its impls of std traits.
    types: Vec<(&'static str, &'static str)>,
    impls: Vec<(&'static str, &'static str)>,
    fns: BTreeSet<&'static str>,
}

impl RuntimeScan {
    fn scan() -> Self {
        let mut scan = RuntimeScan::default();
        for (_, src) in almide_codegen::generated::rust_runtime::RUST_RUNTIME_MODULES {
            for item in scan_items(src, true) {
                scan.add(src, item);
            }
        }
        scan
    }

    fn add(&mut self, src: &'static str, (kind, name, text, attributed): TopItem<'static>) {
        if kind == ItemKind::Impl {
            if !attributed || native_only_cfg_impl(text) {
                self.impls.push((name, text));
            }
            return;
        }
        if kind == ItemKind::Fn {
            self.fns.insert(name);
        }
        if kind == ItemKind::Type && !attributed {
            self.types.push((name, text));
        }
        *self.seen.entry(name).or_default() += 1;
        // A `#[cfg]`-gated item may have a sibling for another target:
        // never take one along.
        if matches!(kind, ItemKind::Fn | ItemKind::Const) && !attributed {
            self.items.insert(name, RtItem { text });
        }
        let is_mod = kind == ItemKind::Other && lex(text).iter().take_while(|t| t.text != name).any(|t| t.text == "mod");
        if is_mod && !attributed {
            let glob = src.contains(&format!("\npub use {name}::*;"));
            let whole = if glob { &*Box::leak(format!("{text}\npub use {name}::*;").into_boxed_str()) } else { text };
            self.modules.push((name, whole, glob));
        }
    }
}

fn build_runtime_index() -> RuntimeIndex {
    let RuntimeScan { mut items, seen, modules, types, impls, fns } = RuntimeScan::scan();
    items.retain(|n, _| seen.get(n) == Some(&1));
    take_modules(&mut items, &seen, modules);
    take_complementary_cfg_functions(&mut items, &seen);
    // Every name the runtime or the v0 prelude defines, at any depth: what
    // a body may reach only as a takeable item. The prelude rides in the
    // runtime crate's source (`emit_runtime_crate`).
    let prelude: &'static str = Box::leak(almide_codegen::emit_runtime_crate().into_boxed_str());
    // Prelude trait impls may call ordinary helpers (for example quoted
    // string display). Index those definitions too, with the same uniqueness
    // rule as runtime functions; cfg families remain handled separately.
    let prelude_items = top_level_items(prelude);
    let mut prelude_counts = BTreeMap::new();
    for (kind, name, _, _) in &prelude_items {
        if *kind != ItemKind::Impl { *prelude_counts.entry(*name).or_insert(0usize) += 1; }
    }
    for (kind, name, text, gated) in prelude_items {
        if kind == ItemKind::Fn && !gated && !seen.contains_key(name) && prelude_counts.get(name) == Some(&1) {
            items.insert(name, RtItem { text });
        }
    }
    let mut defined: BTreeSet<&'static str> = defined_names(prelude);
    let mut trait_methods = trait_method_names(prelude);
    let mut runtime_traits: BTreeSet<&'static str> = BTreeSet::new();
    for (_, src) in almide_codegen::generated::rust_runtime::RUST_RUNTIME_MODULES {
        defined.extend(defined_names(src));
        defined.extend(thread_local_names(src));
        trait_methods.extend(trait_method_names(src));
        runtime_traits.extend(trait_names(src));
    }
    let mut idx = take_prelude_traits(prelude, &runtime_traits, RuntimeIndex { items, defined, trait_methods, fns, fs_built: BTreeSet::new() });
    take_types(&mut idx, &seen, types, &impls, &runtime_traits);
    take_runtime_traits(&mut idx, &seen, &impls);
    take_fs_host_module(&mut idx);
    // The same std-only kernel module the ordinary native runtime embeds.
    // Keep its target-feature implementations and cfg twins together.
    idx.items.insert("almide_kernel", RtItem {
        text: almide_codegen::generated::rust_runtime::ALMIDE_KERNEL_INLINE,
    });
    idx
}

/// An unconditional module travels whole, keeping all nested cfg branches.
/// Only a glob-imported module exposes its members as unqualified items.
fn take_modules(
    items: &mut HashMap<&'static str, RtItem>,
    seen: &BTreeMap<&'static str, usize>,
    modules: Vec<(&'static str, &'static str, bool)>,
) {
    for (name, whole, glob) in modules {
        if seen.get(name) != Some(&1) {
            continue;
        }
        let body = &whole[whole.find('{').unwrap_or(0) + 1..whole.rfind("} //").or_else(|| whole.rfind('}')).unwrap_or(whole.len())];
        items.insert(name, RtItem { text: whole });
        if !glob { continue; }
        for inner in defined_names(body) {
            if !items.contains_key(inner) && !seen.contains_key(inner) {
                items.insert(inner, RtItem { text: whole });
            }
        }
    }
}

/// A trait the prelude alone defines travels whole: its declaration and
/// every prelude impl of it, taken by any of its methods or its name.
/// Runtime types carry their own impls of these traits when taken below.
fn take_prelude_traits(prelude: &'static str, runtime_traits: &BTreeSet<&'static str>, idx: RuntimeIndex) -> RuntimeIndex {
    let RuntimeIndex { mut items, defined, mut trait_methods, fns, fs_built } = idx;
    let traits: Vec<_> = prelude_traits(prelude)
        .into_iter()
        .filter(|(name, ..)| !runtime_traits.contains(name) && !items.contains_key(name))
        .collect();
    for (name, decl, _, methods) in &traits {
        items.insert(name, RtItem { text: decl });
        for m in methods {
            trait_methods.remove(m);
            items.entry(m).or_insert(RtItem { text: decl });
        }
    }
    // An impl for a type the header cannot carry (it reaches such a
    // name) stays behind: no carried body holds a value of that type.
    let provisional = RuntimeIndex { items, defined, trait_methods, fns, fs_built };
    let mut taken: Vec<(&'static str, &'static str, Vec<&'static str>)> = Vec::new();
    for (name, decl, impls, methods) in traits {
        let mut whole = String::from(decl);
        for imp in impls.into_iter().filter(|imp| references_in(&provisional, imp).is_ok()) {
            whole.push('\n');
            whole.push_str(imp);
        }
        taken.push((name, &*Box::leak(whole.into_boxed_str()), methods));
    }
    let RuntimeIndex { mut items, defined, trait_methods, fns, fs_built } = provisional;
    for (name, whole, methods) in taken {
        items.insert(name, RtItem { text: whole });
        for m in methods {
            items.insert(m, RtItem { text: whole });
        }
    }
    RuntimeIndex { items, defined, trait_methods, fns, fs_built }
}

/// A runtime type travels with its inherent impls and its impls of std or
/// prelude traits. Runtime traits travel as their own complete family below.
/// Added after the prelude so a type's impl can resolve its trait definition.
fn take_types(
    idx: &mut RuntimeIndex,
    seen: &BTreeMap<&'static str, usize>,
    types: Vec<(&'static str, &'static str)>,
    impls: &[(&'static str, &'static str)],
    runtime_traits: &BTreeSet<&str>,
) {
    for (name, decl) in types {
        if seen.get(name) != Some(&1) || idx.items.contains_key(name) {
            continue;
        }
        let mut whole = String::from(decl);
        for (_, imp) in impls.iter().filter(|(n, _)| *n == name) {
            let trait_of = impl_parts(&lex(imp)).and_then(|(t, _)| t);
            // Runtime traits travel with all their impls as one item. An
            // impl of a prelude trait instead belongs to its runtime type:
            // the prelude could not carry it before that type was available.
            if trait_of.is_some_and(|t| runtime_traits.contains(t)) {
                continue;
            }
            whole.push('\n');
            whole.push_str(imp);
        }
        idx.items.insert(name, RtItem { text: Box::leak(whole.into_boxed_str()) });
    }
}

/// A runtime trait defined once travels whole too: its declaration and
/// every runtime impl of it (`AlmideMapKey` and its blanket impls), taken
/// by its name or any of its methods.
fn take_runtime_traits(idx: &mut RuntimeIndex, seen: &BTreeMap<&'static str, usize>, impls: &[(&'static str, &'static str)]) {
    for (_, src) in almide_codegen::generated::rust_runtime::RUST_RUNTIME_MODULES {
        for (name, decl, _, methods) in prelude_traits(src) {
            if seen.get(name) != Some(&1) || idx.items.contains_key(name) {
                continue;
            }
            let mut whole = String::from(decl);
            for (_, imp) in impls {
                if impl_parts(&lex(imp)).and_then(|(t, _)| t) == Some(name) {
                    whole.push('\n');
                    whole.push_str(imp);
                }
            }
            let whole: &'static str = Box::leak(whole.into_boxed_str());
            idx.items.insert(name, RtItem { text: whole });
            for m in methods {
                idx.trait_methods.remove(m);
                idx.items.entry(m).or_insert(RtItem { text: whole });
            }
        }
    }
}

/// Make the fs host module's `#[cfg]` twins, its modules and its types
/// takeable: each name defined more than once or under a gate becomes one
/// item of all its definitions (attributes kept), a module one item of its
/// whole text, a type one item with every impl the module gives it of a trait
/// the runtime does not define (as `take_types` carries a type). Every other
/// top-level fn and const stays the item it was.
fn take_fs_host_module(idx: &mut RuntimeIndex) {
    let Some(src) = fs_host_source() else { return };
    let items = scan_items(src, true);
    let mut defs: BTreeMap<&'static str, Vec<(ItemKind, &'static str, bool)>> = BTreeMap::new();
    for &(kind, name, text, attributed) in &items {
        if kind != ItemKind::Impl {
            defs.entry(name).or_default().push((kind, text, attributed));
        }
    }
    let mut built = BTreeSet::new();
    for (name, ds) in defs {
        let is_mod = ds.iter().any(|(k, t, _)| *k == ItemKind::Other && lex(t).iter().take_while(|x| x.text != name).any(|x| x.text == "mod"));
        let is_type = ds.iter().any(|(k, ..)| *k == ItemKind::Type);
        let twinned = ds.len() > 1 || ds.iter().any(|(_, _, a)| *a);
        if !(is_mod || is_type || twinned) {
            continue;
        }
        let mut whole: Vec<&str> = ds.iter().map(|(_, t, _)| *t).collect();
        if is_type {
            for &(kind, self_ty, text, _) in &items {
                if kind != ItemKind::Impl || self_ty != name {
                    continue;
                }
                let toks = lex(text);
                let at = toks.iter().position(|t| t.text == "impl").unwrap_or(0);
                let trait_of = impl_parts(&toks[at..]).and_then(|(t, _)| t);
                if !trait_of.is_some_and(|t| idx.defined.contains(t)) {
                    whole.push(text);
                }
            }
        }
        let text: &'static str = Box::leak(whole.join("\n").into_boxed_str());
        built.insert(text.as_ptr() as usize);
        idx.items.insert(name, RtItem { text });
    }
    // A top-level `thread_local! { … }` travels whole, by any static it defines.
    let toks = lex(src);
    for (i, t) in toks.iter().enumerate() {
        if t.depth == 0 && t.text == "thread_local" && toks.get(i + 1).is_some_and(|n| n.text == "!") {
            let end = item_end(&toks, i);
            let text = &src[t.off..toks.get(end).map_or(src.len(), |e| e.off + e.text.len())];
            for name in thread_local_names(text) {
                idx.items.insert(name, RtItem { text });
            }
        }
    }
    idx.fs_built = built;
}

/// The traits `src` defines at its top level.
fn trait_names(src: &str) -> BTreeSet<&str> {
    top_level_items(src)
        .into_iter()
        .filter(|(kind, name, text, _)| *kind == ItemKind::Other && lex(text).iter().take_while(|t| t.text != *name).any(|t| t.text == "trait"))
        .map(|(_, name, _, _)| name)
        .collect()
}

/// Each trait `src` defines at its top level (with no `#[cfg]`), as (name,
/// its declaration, every top-level impl of it in `src`, its method names).
#[allow(clippy::type_complexity)]
fn prelude_traits(src: &'static str) -> Vec<(&'static str, &'static str, Vec<&'static str>, Vec<&'static str>)> {
    // Consume complete items, so `impl Trait` in a function signature cannot
    // be mistaken for an implementation block and swallow following items.
    let mut impls: BTreeMap<&'static str, Vec<&'static str>> = BTreeMap::new();
    for (kind, _, text, gated) in scan_items(src, true) {
        if kind == ItemKind::Impl && !gated {
            if let Some((Some(tr), _)) = impl_parts(&lex(text)) {
                impls.entry(tr).or_default().push(text);
            }
        }
    }
    let mut out = Vec::new();
    for (kind, name, text, attributed) in top_level_items(src) {
        let ltoks = lex(text);
        if kind != ItemKind::Other || attributed || !ltoks.iter().take_while(|t| t.text != name).any(|t| t.text == "trait") {
            continue;
        }
        let methods: Vec<&'static str> = ltoks.windows(2).filter(|w| w[0].text == "fn").map(|w| w[1].text).collect();
        out.push((name, text, impls.get(name).cloned().unwrap_or_default(), methods));
    }
    out
}

/// The fn names the top-level `trait` items of `src` declare.
fn trait_method_names(src: &str) -> BTreeSet<&str> {
    let mut out = BTreeSet::new();
    for (kind, name, text, _) in top_level_items(src) {
        let toks = lex(text);
        if kind == ItemKind::Other && toks.iter().take_while(|t| t.text != name).any(|t| t.text == "trait") {
            out.extend(toks.windows(2).filter(|w| w[0].text == "fn").map(|w| w[1].text));
        }
    }
    out
}

#[cfg(test)]
mod scoped_module_tests {
    use super::*;

    #[test]
    fn runtime_display_types_carry_the_prelude_leaf_implementations() {
        let idx = runtime_index();
        let text = closure("AlmideHttpResponse", idx).unwrap().join("\n");
        assert!(text.contains("impl AlmideRepr for i64"), "response display needs its status's repr");
        assert!(text.contains("impl AlmideRepr for std::string::String"), "response display needs its body's repr");
    }

    #[test]
    fn private_module_members_do_not_become_global_runtime_items() {
        const SRC: &str = "mod scoped { #[cfg(unix)] pub fn inside() {} #[cfg(not(unix))] pub fn inside() {} }";
        let mut scan = RuntimeScan::default();
        for item in top_level_items(SRC) { scan.add(SRC, item); }
        take_modules(&mut scan.items, &scan.seen, scan.modules);
        assert_eq!(scan.items.len(), 1);
        let text = scan.items["scoped"].text;
        assert!(text.contains("#[cfg(unix)]") && text.contains("#[cfg(not(unix))]"));
        let rt = RuntimeIndex { items: scan.items, defined: defined_names(SRC),
            trait_methods: BTreeSet::new(), fns: BTreeSet::new(), fs_built: BTreeSet::new() };
        assert_eq!(references_in(&rt, text).unwrap(), ["scoped"]);
    }

    #[test]
    fn a_scoped_module_does_not_hide_an_unresolved_parent_call() {
        let rt = RuntimeIndex { items: HashMap::new(), defined: ["outside"].into_iter().collect(),
            trait_methods: BTreeSet::new(), fns: ["outside"].into_iter().collect(), fs_built: BTreeSet::new() };
        assert_eq!(references_in(&rt, "mod scoped { pub fn inside() { super::outside(); } }"), Err("outside".into()));
        let mut scan = RuntimeScan::default();
        const SRC: &str = "#[cfg(unix)] mod only_unix { pub fn inside() {} }";
        for item in top_level_items(SRC) { scan.add(SRC, item); }
        assert!(scan.modules.is_empty(), "a conditional outer module must remain refused");
    }
}
