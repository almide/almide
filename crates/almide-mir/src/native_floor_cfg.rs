// A function with exactly two complementary cfg definitions can travel as
// one item. Keep BOTH gates and bodies; never pick the compiler host's arm.
// This admits the fan cost model's CPU-clock helper without dropping its
// target fallback or admitting a lone/overlapping cfg definition.
fn take_complementary_cfg_functions(
    items: &mut HashMap<&'static str, RtItem>,
    seen: &BTreeMap<&'static str, usize>,
) {
    let mut families: BTreeMap<&'static str, Vec<&'static str>> = BTreeMap::new();
    for (_, src) in almide_codegen::generated::rust_runtime::RUST_RUNTIME_MODULES {
        for (kind, name, text, gated) in scan_items(src, true) {
            if kind == ItemKind::Fn && gated {
                families.entry(name).or_default().push(text);
            }
        }
    }
    for (name, bodies) in families {
        if items.contains_key(name) { continue; }
        // This floor emits only native Rust. A unique function gated solely
        // off wasm32 is available on every supported native target. Preserve
        // the attribute; never select a host OS/CPU-specific implementation.
        if seen.get(name) == Some(&1) && bodies.len() == 1 && native_only_cfg_function(bodies[0]) {
            items.insert(name, RtItem { text: bodies[0] });
            continue;
        }
        if seen.get(name) != Some(&2) { continue; }
        if let Some(text) = complementary_cfg_pair(&bodies) {
            items.insert(name, RtItem { text: Box::leak(text.into_boxed_str()) });
        }
    }
}

fn native_only_cfg_function(text: &str) -> bool {
    cfg_function_head(text).is_some_and(|(predicate, _)|
        predicate.chars().filter(|c| !c.is_whitespace()).collect::<String>() == "not(target_arch=\"wasm32\")")
}

fn native_only_cfg_impl(text: &str) -> bool {
    cfg_item_head(text, "impl").is_some_and(|(predicate, _)|
        predicate.chars().filter(|c| !c.is_whitespace()).collect::<String>() == "not(target_arch=\"wasm32\")")
}

fn complementary_cfg_pair(bodies: &[&str]) -> Option<String> {
    let [a, b] = bodies else { return None };
    let (pa, ha) = cfg_function_head(a)?;
    let (pb, hb) = cfg_function_head(b)?;
    // Compare the complete function head, including generic bounds. The
    // return-type/argument scanner alone would forget those bounds.
    if ha != hb { return None; }
    let negates = |negative: &str, positive: &str| {
        negative.strip_prefix("not(").and_then(|s| s.strip_suffix(')'))
            .is_some_and(|s| s.trim() == positive)
    };
    (negates(pa, pb) || negates(pb, pa)).then(|| format!("{a}\n{b}"))
}

/// Exactly one cfg attribute followed by an ordinary function. The lexer
/// skips strings but its offsets let us retain their exact bytes: predicates
/// containing different string values must never compare equal.
fn cfg_function_head(text: &str) -> Option<(&str, &str)> {
    cfg_item_head(text, "fn")
}

fn cfg_item_head<'a>(text: &'a str, kind: &str) -> Option<(&'a str, &'a str)> {
    let toks = lex(text);
    if toks.iter().take(4).map(|t| t.text).collect::<Vec<_>>() != ["#", "[", "cfg", "("] {
        return None;
    }
    let mut depth = 1;
    let mut end = None;
    for (i, t) in toks.iter().enumerate().skip(4) {
        match t.text {
            "(" => depth += 1,
            ")" => { depth -= 1; if depth == 0 { end = Some(i); break; } }
            _ => {}
        }
    }
    let end = end?;
    if toks.get(end + 1)?.text != "]" { return None; }
    let start = end + 2;
    let fn_at = if toks.get(start)?.text == "pub" { start + 1 } else { start };
    if toks.get(fn_at)?.text != kind { return None; }
    let open = toks.iter().skip(fn_at + 1).find(|t| t.text == "{")?;
    Some((text[toks[3].off + 1..toks[end].off].trim(), text[toks[start].off..open.off].trim()))
}

#[cfg(test)]
mod complementary_cfg_tests {
    #[test]
    fn native_only_cfg_keeps_its_gate_and_refuses_other_conditions() {
        use super::native_only_cfg_function as admit;
        assert!(admit("#[cfg(not(target_arch = \"wasm32\"))] pub fn tls() {}"));
        for bad in ["#[cfg(unix)] fn tls() {}", "#[cfg(not(target_os = \"linux\"))] fn tls() {}",
            "#[cfg(target_arch = \"wasm32\")] fn tls() {}", "#[cfg(not(target_arch = \"wasm32\"))] #[inline] fn tls() {}"] {
            assert!(!admit(bad));
        }
        assert!(super::native_only_cfg_impl("#[cfg(not(target_arch = \"wasm32\"))] impl Sock for T {}"));
        assert!(!super::native_only_cfg_impl("#[cfg(unix)] impl Sock for T {}"));
    }
    use super::*;

    #[test]
    fn only_complete_disjoint_same_abi_pairs_are_carried() {
        let a = "#[cfg(target_os = \"linux\")] fn clock() -> u64 { 1 }";
        let b = "#[cfg(not(target_os = \"linux\"))] fn clock() -> u64 { 2 }";
        assert_eq!(complementary_cfg_pair(&[a, b]).unwrap(), format!("{a}\n{b}"));
        assert!(complementary_cfg_pair(&[a]).is_none());
        assert!(complementary_cfg_pair(&[a, a]).is_none());
        assert!(complementary_cfg_pair(&[a, b, b]).is_none());
        assert!(complementary_cfg_pair(&[a, &b.replace("linux", "macos")]).is_none());
        assert!(complementary_cfg_pair(&[a, &b.replace("u64", "i64")]).is_none());
        assert!(complementary_cfg_pair(&[a, &format!("#[cfg(unix)] {b}")]).is_none());
        assert!(complementary_cfg_pair(&[a, &b.replace("cfg(", "cfg_attr(")]).is_none());
        let generic = |bound| format!("#[cfg(unix)] fn f<T: {bound}>(x: T) -> T {{ x }}");
        assert!(complementary_cfg_pair(&[&generic("Send"), &generic("Sync").replace("cfg(unix)", "cfg(not(unix))")]).is_none());
    }

    #[test]
    fn fan_clock_keeps_target_cpu_clock_and_fallback_together() {
        let items = closure("almide_fan_cpu_ns", runtime_index()).expect("complete clock runtime dependency closure");
        let clock = items[0];
        assert_eq!(clock.matches("fn almide_fan_cpu_ns()").count(), 2);
        assert!(clock.contains("#[cfg(not(all("));
        assert!(clock.contains("thread_cpu_ns()"));
        assert!(clock.contains("std::time::Instant"));
        assert!(items.iter().any(|s| s.contains("mod almide_rt_sys_ffi")));
    }
}
