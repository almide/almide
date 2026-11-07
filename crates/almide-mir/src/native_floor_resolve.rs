// Runtime consumers of the build-time index.
fn runtime_index() -> &'static RuntimeIndex {
    static IDX: OnceLock<RuntimeIndex> = OnceLock::new();
    IDX.get_or_init(load_runtime_index)
}

/// The runtime body of `symbol` and every item it reaches, or `None` when it
/// reaches something the native header cannot carry.
fn closure(symbol: &str, rt: &'static RuntimeIndex) -> Result<Vec<&'static str>, String> {
    let mut taken: BTreeSet<&'static str> = BTreeSet::new();
    let mut texts: BTreeSet<&'static str> = BTreeSet::new();
    let mut todo: Vec<String> = vec![symbol.to_string()];
    let mut out = Vec::new();
    while let Some(name) = todo.pop() {
        let (&key, item) = rt
            .items
            .get_key_value(name.as_str())
            .ok_or_else(|| format!("`{name}` is not a takeable runtime item"))?;
        if header_items().contains(key) || !taken.insert(key) || !texts.insert(item.text) {
            continue;
        }
        // A runtime module that needs a crate (`almide_codegen::runtime_crate_deps`)
        // cannot ride the native program, which links no dependency.
        if let Some((dep, _)) = almide_codegen::runtime_crate_deps(item.text).first() {
            return Err(format!("`{key}` needs the `{dep}` crate, which a native program does not link"));
        }
        out.push(item.text);
        for r in references(item.text).map_err(|n| format!("`{key}` reaches `{n}`, which the native header cannot carry"))? {
            if !taken.contains(r.as_str()) {
                todo.push(r);
            }
        }
    }
    fs_host_gate(symbol, &out)?;
    Ok(out)
}

/// The runtime items `text` names, each closed over what it reaches: what a
/// render takes along for the runtime types its generated table aliases
/// (#4514), the way a floor body takes its callees. A declared type nobody
/// operates on is reached by no body, so without this its alias would name a
/// definition that never arrives. `Err` names one the header cannot carry.
pub(crate) fn runtime_items_named_by(text: &str) -> Result<Vec<&'static str>, String> {
    let mut out = Vec::new();
    for name in references(text).map_err(|n| format!("the generated types reach `{n}`, which the native header cannot carry"))? {
        out.extend(closure(&name, runtime_index())?);
    }
    Ok(out)
}

/// The runtime item names `text` reaches that need a definition beside it,
/// or `Err(name)` when it reaches a name the runtime or its prelude defines that
/// is not an item we can take along (a type, a macro, a module, a prelude
/// fn, a `#[cfg]` twin). A name neither of them defines is Rust's own.
fn references(text: &str) -> Result<Vec<String>, String> {
    if is_fs_host_text(text) {
        // The fs host module's own names, at any depth, are its own.
        return references_seeded(runtime_index(), text, fs_host_own_names());
    }
    references_in(runtime_index(), text)
}

/// Is `text` a runtime item (the floor header or an item of the embedded
/// runtime the floor takes along)? The render places these in the program's
/// runtime part (`render_native::RT_MOD`), which the build compiles as its
/// own crate with the runtime's profile (owner ruling 2026-11-02).
pub(crate) fn is_runtime_text(text: &str) -> bool {
    static TEXTS: OnceLock<BTreeSet<&'static str>> = OnceLock::new();
    text == FLOOR_HEADER || TEXTS.get_or_init(|| runtime_index().items.values().map(|i| i.text).collect()).contains(text)
}

/// `Err` when the closure of `symbol` carries the fs host module and
/// `symbol` is not one of the named native fs host calls.
fn fs_host_gate(symbol: &str, items: &[&'static str]) -> Result<(), String> {
    if items.iter().any(|t| is_fs_host_text(t)) && !crate::host_ops::is_native_fs_host_call(symbol) {
        return Err(format!(
            "`{symbol}` reaches the native fs host module, which only the named native fs host calls may (proofs/TRUSTED_BASE.md)"
        ));
    }
    Ok(())
}

/// The text a runtime fn's signature is read from: its own item, or, for a
/// fn of the fs host module (whose name resolves to the whole module), its
/// own definition inside the module.
fn signature_text(rt: &'static RuntimeIndex, symbol: &str) -> Option<&'static str> {
    let item = rt.items.get(symbol)?.text;
    if !is_fs_host_text(item) {
        return Some(item);
    }
    static FNS: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    FNS.get_or_init(|| {
        let mut out = HashMap::new();
        let mut twice = BTreeSet::new();
        for (kind, name, text, attributed) in fs_host_source().map(top_level_items).unwrap_or_default() {
            if kind == ItemKind::Fn && !attributed && out.insert(name, text).is_some() {
                twice.insert(name);
            }
        }
        out.retain(|n, _| !twice.contains(n));
        out
    })
    .get(symbol)
    .copied()
}

#[cfg(test)]
mod fs_host_tests {
    use super::*;

    /// A floor body outside the named set whose closure reaches fs host code
    /// is refused; a named one is admitted; one carrying no fs code is free.
    #[test]
    fn only_a_named_call_may_carry_fs_host_code() {
        let fs_item = runtime_index().items.get("almide_rt_fs_write").expect("fs.write_text's body is an item").text;
        assert!(is_fs_host_text(fs_item));
        assert!(fs_host_gate("almide_rt_fs_write", &[fs_item]).is_ok());
        let refused = fs_host_gate("almide_rt_string_len", &[fs_item]).unwrap_err();
        assert!(refused.contains("native fs host module"), "{refused}");
        assert!(fs_host_gate("almide_rt_string_len", &["fn f() {}"]).is_ok());
    }

    /// The module's `#[cfg]` twins travel together: the name resolves to an
    /// item holding every gated definition.
    #[test]
    fn a_cfg_twinned_fs_name_carries_every_twin() {
        let src = fs_host_source().expect("the runtime has an fs module");
        let twinned = scan_items(src, true)
            .into_iter()
            .filter(|(k, _, _, a)| *k != ItemKind::Impl && *a)
            .map(|(_, n, _, _)| n)
            .next()
            .expect("the fs module has a #[cfg] item");
        let item = runtime_index().items.get(twinned).expect("the twinned name is takeable").text;
        assert!(is_fs_host_text(item));
        assert!(item.contains("#[cfg"), "{twinned}: the gate travels with the item");
    }
}

fn is_fs_host_text(text: &str) -> bool {
    is_fs_host_text_in(runtime_index(), text)
}
