// Runtime consumers of the build-time index.
fn runtime_index() -> &'static RuntimeIndex {
    static IDX: OnceLock<RuntimeIndex> = OnceLock::new();
    IDX.get_or_init(load_runtime_index)
}

/// The runtime body of `symbol` and every item it reaches, or `None` when it
/// reaches something the native header cannot carry.
fn closure(symbol: &str, rt: &'static RuntimeIndex) -> Result<Vec<&'static str>, String> {
    closure_using(symbol, rt, references)
}

fn closure_using(
    symbol: &str,
    rt: &'static RuntimeIndex,
    refs: impl Fn(&str) -> Result<Vec<String>, String>,
) -> Result<Vec<&'static str>, String> {
    let out = closure_contents_using(symbol, rt, refs)?;
    host_gate(symbol, &out)?;
    Ok(out)
}

/// Runtime dependency closure, also used for the support of an already
/// admitted scoped fs host call. Public floor admission still uses the gate.
fn closure_contents_using(
    symbol: &str,
    rt: &'static RuntimeIndex,
    refs: impl Fn(&str) -> Result<Vec<String>, String>,
) -> Result<Vec<&'static str>, String> {
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
        // Dependency-bearing runtime bodies take the product's Cargo route.
        // Its manifest derives from these exact bodies, including a split
        // runtime crate; the floor compile gate checks the complete closure.
        out.push(item.text);
        for r in refs(item.text).map_err(|n| format!("`{key}` reaches `{n}`, which the native header cannot carry"))? {
            if !taken.contains(r.as_str()) {
                todo.push(r);
            }
        }
    }
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
    if text == almide_codegen::generated::rust_runtime::ALMIDE_KERNEL_INLINE {
        // Internal helpers and cfg twins are defined inside this whole module.
        static OWN: OnceLock<BTreeSet<&'static str>> = OnceLock::new();
        let own = OWN.get_or_init(|| defined_names(almide_codegen::generated::rust_runtime::ALMIDE_KERNEL_INLINE));
        return references_seeded(runtime_index(), text, own);
    }
    if let Some(module) = host_module_of_text(text) {
        // A host module's own names, at any depth, are its own.
        return references_seeded(runtime_index(), text, host_own_names(module));
    }
    references_in(runtime_index(), text)
}

/// Is `text` a runtime item (the floor header or an item of the embedded
/// runtime the floor takes along)? The render places these in the program's
/// runtime part (`render_native::RT_MOD`), which the build compiles as its
/// own crate with the runtime's profile (owner ruling 2026-11-02).
pub(crate) fn is_runtime_text(text: &str) -> bool {
    static TEXTS: OnceLock<BTreeSet<&'static str>> = OnceLock::new();
    text == floor_header() || TEXTS.get_or_init(|| runtime_index().items.values().map(|i| i.text).collect()).contains(text)
}

/// A runtime implementation of a named host operation. The generated
/// Send-free twin has the same operation identity and capability witness;
/// it changes callback scheduling, not the admitted host interface.
fn is_native_host_body(module: &str, symbol: &str) -> bool {
    crate::host_ops::native_host_class(module).is_some_and(|calls| {
        calls.contains(&symbol)
            || calls.iter().any(|host| almide_codegen::generated::runtime_fn_modes::send_free_twin(host) == Some(symbol))
    })
}

/// `Err` when the closure carries a host module's code but implements
/// neither a call named for that module's class nor its generated Send-free
/// twin. Witnesses name the operation.
fn host_gate(symbol: &str, items: &[&'static str]) -> Result<(), String> {
    for module in items.iter().filter_map(|t| host_module_of_text(t)) {
        if !is_native_host_body(module, symbol) {
            return Err(format!(
                "`{symbol}` reaches the native {module} host module, which only the named native {module} host calls may (proofs/TRUSTED_BASE.md)"
            ));
        }
    }
    Ok(())
}

/// The text a runtime fn's signature is read from: its own item, or, for a
/// fn of a host module (whose name resolves to the whole module), its own
/// definition inside the module.
fn signature_text(rt: &'static RuntimeIndex, symbol: &str) -> Option<&'static str> {
    let item = rt.items.get(symbol)?.text;
    if host_module_of_text(item).is_none() {
        return Some(item);
    }
    static FNS: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    FNS.get_or_init(|| {
        let mut out = HashMap::new();
        let mut twice = BTreeSet::new();
        for src in HOST_MODULES.iter().filter_map(|m| host_source(m)) {
            for (kind, name, text, attributed) in top_level_items(src) {
                if kind == ItemKind::Fn && !attributed && out.insert(name, text).is_some() {
                    twice.insert(name);
                }
            }
        }
        out.retain(|n, _| !twice.contains(n));
        out
    })
    .get(symbol)
    .copied()
}

#[cfg(test)]
mod host_tests {
    use super::*;

    /// A floor body outside the named set whose closure reaches fs host code
    /// is refused; a named one is admitted; one carrying no fs code is free.
    #[test]
    fn only_a_named_call_may_carry_fs_host_code() {
        let fs_item = runtime_index().items.get("almide_rt_fs_write").expect("fs.write_text's body is an item").text;
        assert!(is_fs_host_text(fs_item));
        assert!(host_gate("almide_rt_fs_write", &[fs_item]).is_ok());
        for host in crate::host_ops::NATIVE_FS_HOST_CALLS {
            if let Some(twin) = almide_codegen::generated::runtime_fn_modes::send_free_twin(host) {
                assert!(host_gate(twin, &[fs_item]).is_ok());
            }
        }
        assert!(host_gate("almide_rt_fs_write_seq", &[fs_item]).is_err());
        let refused = host_gate("almide_rt_string_len", &[fs_item]).unwrap_err();
        assert!(refused.contains("native fs host module"), "{refused}");
        assert!(host_gate("almide_rt_string_len", &["fn f() {}"]).is_ok());
    }

    /// The unix class gates its own module: a named unix call may carry it,
    /// an fs host call or any other body may not, and a unix call may not
    /// carry fs code.
    #[test]
    fn only_a_named_unix_call_may_carry_unix_host_code() {
        let rt = runtime_index();
        let unix_item = rt.items.get("almide_rt_unix___os_close").expect("unix.__os_close's body is an item").text;
        assert_eq!(host_module_of_text(unix_item), Some("unix"));
        for host in crate::host_ops::NATIVE_UNIX_HOST_CALLS {
            assert!(host_gate(host, &[unix_item]).is_ok(), "{host}");
        }
        let refused = host_gate("almide_rt_fs_write", &[unix_item]).unwrap_err();
        assert!(refused.contains("native unix host module"), "{refused}");
        assert!(host_gate("almide_rt_net___os_tcp_close", &[unix_item]).is_err());
        let fs_item = rt.items.get("almide_rt_fs_write").expect("fs.write_text's body is an item").text;
        assert!(host_gate("almide_rt_unix___os_close", &[fs_item]).is_err());
    }

    /// Every named unix call carries its module's closure: the cfg-gated
    /// table impls and the syscall module travel with it.
    #[test]
    fn every_named_unix_call_resolves_with_its_module() {
        for host in crate::host_ops::NATIVE_UNIX_HOST_CALLS {
            let items = closure(host, runtime_index()).unwrap_or_else(|e| panic!("{host}: {e}"));
            assert!(items.iter().any(|t| t.contains("impl AlmideUnixTables")), "{host}: the table's impls travel");
        }
    }

    /// The build-script copy of the module list is the classes' list.
    #[test]
    fn host_modules_are_the_host_classes() {
        let classes: Vec<&str> = crate::host_ops::NATIVE_HOST_CLASSES.iter().map(|(m, _)| *m).collect();
        assert_eq!(HOST_MODULES, classes.as_slice());
    }

    /// The module's `#[cfg]` twins travel together: the name resolves to an
    /// item holding every gated definition.
    #[test]
    fn a_cfg_twinned_host_name_carries_every_twin() {
        for module in HOST_MODULES {
            let src = host_source(module).expect("the runtime has the host module");
            let twinned = scan_items(src, true)
                .into_iter()
                .filter(|(k, _, _, a)| *k != ItemKind::Impl && *a)
                .map(|(_, n, _, _)| n)
                .next()
                .expect("the host module has a #[cfg] item");
            let item = runtime_index().items.get(twinned).expect("the twinned name is takeable").text;
            assert_eq!(host_module_of_text(item), Some(*module));
            assert!(item.contains("#[cfg"), "{twinned}: the gate travels with the item");
        }
    }
}

fn host_module_of_text(text: &str) -> Option<&'static str> {
    host_module_of_text_in(runtime_index(), text)
}

fn is_fs_host_text(text: &str) -> bool {
    host_module_of_text(text) == Some("fs")
}

/// Additional trusted runtime support for a scoped host implementation. This
/// is not an admission route: the render must already carry a named fs host.
pub(crate) fn scoped_fs_runtime_items(text: &str, used: &[&str], with_fan: bool) -> Result<Vec<&'static str>, String> {
    if !used.iter().any(|s| is_fs_host_text(s)) {
        return Err("a scoped filesystem runtime requires an admitted fs host call".into());
    }
    let glue = if with_fan { crate::render_native::fan_glue() } else { String::new() };
    let mut supplied = defined_names(&glue);
    supplied.extend(thread_local_names(&glue));
    let refs = |s: &str| if with_fan {
        references_with_supplied(runtime_index(), s, &supplied)
    } else { references(s) };
    let mut out = Vec::new();
    for name in refs(text)? {
        out.extend(closure_contents_using(&name, runtime_index(), refs)?);
    }
    Ok(out)
}

pub(crate) fn carries_fs_host(used: &[&str]) -> bool {
    used.iter().any(|s| is_fs_host_text(s))
}
