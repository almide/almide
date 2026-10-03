use std::collections::{HashMap, HashSet};

/// Effect categories — mapped from stdlib module usage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Effect {
    IO,
    Net,
    Env,
    Time,
    Rand,
    Fan,
}

impl std::fmt::Display for Effect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Effect::IO => write!(f, "IO"),
            Effect::Net => write!(f, "Net"),
            Effect::Env => write!(f, "Env"),
            Effect::Time => write!(f, "Time"),
            Effect::Rand => write!(f, "Rand"),
            Effect::Fan => write!(f, "Fan"),
        }
    }
}

impl Effect {
    /// Every category, in declaration order. This is the whole vocabulary of
    /// `almide.toml [permissions].allow`: a name is valid there exactly when
    /// it is the `Display` of one of these (#3247).
    ///
    /// It is also ⊤: what a call is assumed to do when nothing bounds it —
    /// today, a call to an `@extern` fn (#3245). A foreign body is opaque to
    /// inference and no bound can be declared on it yet (ADR-0027 §5), so
    /// `[permissions].allow` passes it only when it allows every category.
    pub const ALL: [Effect; 6] = [Effect::IO, Effect::Net, Effect::Env, Effect::Time, Effect::Rand, Effect::Fan];

    /// The category a `[permissions].allow` name spells, or `None`.
    pub fn from_name(name: &str) -> Option<Effect> {
        Self::ALL.into_iter().find(|e| e.to_string() == name)
    }
}

/// How the calls into one stdlib module are classified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleEffects {
    /// No function of the module reaches the host.
    Pure,
    /// Every function of the module carries the category.
    All(Effect),
    /// Exactly the module's `effect fn`s carry the category; its plain `fn`s
    /// are pure. Read from the module's own source, so the classification of
    /// a function is its `effect` marker and cannot drift from it.
    EffectFns(Effect),
}

/// THE classification of every stdlib module, by what its functions do to the
/// host (#3246). One row per module in `STDLIB_MODULES` ∪ `BUNDLED_MODULES`,
/// plus the `fan` language module; the gate in this file's tests refuses a
/// module without a row, a row without a module, a `Pure` module that declares
/// an unexplained `effect fn`, and an `@intrinsic` symbol whose classification
/// disagrees with the function that declares it.
///
/// Categories are today's six (`IO` = files and the standard streams). The
/// per-operation registry ADR-0027 §6 proposes replaces this table; until
/// then a mixed module is split by its `effect` markers (`EffectFns`), not by
/// a hand list.
pub const STDLIB_MODULE_EFFECTS: &[(&str, ModuleEffects)] = &[
    // Files and the standard streams.
    ("fs", ModuleEffects::All(Effect::IO)),
    ("io", ModuleEffects::All(Effect::IO)),
    // The network. `http`'s plain fns are request/response builders and the
    // router — values, not traffic.
    ("http", ModuleEffects::EffectFns(Effect::Net)),
    ("net", ModuleEffects::All(Effect::Net)),
    // The process environment: variables, argv, the process table.
    ("env", ModuleEffects::All(Effect::Env)),
    ("process", ModuleEffects::All(Effect::Env)),
    ("args", ModuleEffects::All(Effect::Env)),
    // The clock: `datetime.now` / `monotonic_ns`; the calendar math is pure.
    ("datetime", ModuleEffects::EffectFns(Effect::Time)),
    // Entropy.
    ("random", ModuleEffects::All(Effect::Rand)),
    // `fan` is a language form, not a stdlib module; `fan.map` and friends
    // lower to Module calls on it.
    ("fan", ModuleEffects::All(Effect::Fan)),
    // String and path/URL manipulation: no host access. `path` never touches
    // the file system and `url` never touches the network.
    ("path", ModuleEffects::Pure),
    ("url", ModuleEffects::Pure),
    ("string", ModuleEffects::Pure),
    ("regex", ModuleEffects::Pure),
    ("html", ModuleEffects::Pure),
    // Codecs.
    ("json", ModuleEffects::Pure),
    ("value", ModuleEffects::Pure),
    ("base64", ModuleEffects::Pure),
    ("hex", ModuleEffects::Pure),
    ("hash", ModuleEffects::Pure),
    // `zlib`'s `effect fn`s are compress/decompress over in-memory bytes:
    // `effect` there buys the error channel (ADR-0002 §D6), not host access.
    ("zlib", ModuleEffects::Pure),
    // Collections and scalars.
    ("list", ModuleEffects::Pure),
    ("map", ModuleEffects::Pure),
    ("set", ModuleEffects::Pure),
    ("option", ModuleEffects::Pure),
    ("result", ModuleEffects::Pure),
    ("error", ModuleEffects::Pure),
    ("bytes", ModuleEffects::Pure),
    ("matrix", ModuleEffects::Pure),
    ("math", ModuleEffects::Pure),
    ("int", ModuleEffects::Pure),
    ("float", ModuleEffects::Pure),
    ("int8", ModuleEffects::Pure),
    ("int16", ModuleEffects::Pure),
    ("int32", ModuleEffects::Pure),
    ("int64", ModuleEffects::Pure),
    ("uint8", ModuleEffects::Pure),
    ("uint16", ModuleEffects::Pure),
    ("uint32", ModuleEffects::Pure),
    ("uint64", ModuleEffects::Pure),
    ("float32", ModuleEffects::Pure),
    ("float64", ModuleEffects::Pure),
    // Assertions: they abort, which is not a category (ADR-0022).
    ("testing", ModuleEffects::Pure),
    // The allocator's arena mark/rewind: program memory, not the host.
    ("mem", ModuleEffects::Pure),
    // The primitive floor is stdlib-internal (E085 outside it); its host ops
    // are classified at the public function that wraps them, so a stdlib
    // body over the floor does not charge its callers twice.
    ("prim", ModuleEffects::Pure),
];

/// `effect fn`s in a `Pure` module, each with no host access. The gate refuses
/// any other `effect fn` in a `Pure` module: a new one must be classified.
pub const HOST_FREE_EFFECT_FNS: &[&str] = &[
    "zlib.compress", "zlib.compress_level", "zlib.decompress", "zlib.deflate",
    "zlib.deflate_level", "zlib.inflate", "zlib.gzip", "zlib.gunzip",
];

/// Runtime-symbol prefixes that are not their module's name
/// (`almide_rt_<prefix>_…`).
const RUNTIME_PREFIX_ALIASES: &[(&str, &str)] = &[("sse", "http"), ("test", "testing")];

pub fn module_effects(module: &str) -> Option<ModuleEffects> {
    STDLIB_MODULE_EFFECTS.iter().find(|(m, _)| *m == module).map(|(_, e)| *e)
}

/// The parsed declarations of a stdlib module's own source.
fn module_fns(module: &str) -> impl Iterator<Item = &'static almide_lang::ast::Decl> {
    almide_lang::stdlib_info::bundled_source(module)
        .and_then(almide_lang::parse_cached)
        .into_iter()
        .flat_map(|p| p.decls.iter())
        .filter(|d| matches!(d, almide_lang::ast::Decl::Fn { .. }))
}

fn fn_is_effect(decl: &almide_lang::ast::Decl) -> bool {
    matches!(decl, almide_lang::ast::Decl::Fn { effect: Some(true), .. })
}

fn fn_name(decl: &almide_lang::ast::Decl) -> &str {
    match decl {
        almide_lang::ast::Decl::Fn { name, .. } => name.as_str(),
        _ => "",
    }
}

fn intrinsic_symbol(decl: &almide_lang::ast::Decl) -> Option<&str> {
    let almide_lang::ast::Decl::Fn { attrs, .. } = decl else { return None };
    attrs.iter().find(|a| a.name.as_str() == "intrinsic").and_then(|a| match a.args.first().map(|x| &x.value) {
        Some(almide_lang::ast::AttrValue::String { value }) => Some(value.as_str()),
        _ => None,
    })
}

/// The category a call `module.func` carries, `None` for a pure call.
///
/// The function must be one the stdlib module declares: a call to a name the
/// module's source does not declare is a USER module that shares the stdlib
/// module's name (`net`, `io` — #2223), and its effects travel through the
/// call graph like any user fn's. (A user module that also reuses a stdlib
/// function's name is classified as the stdlib function: an
/// over-approximation, never a missed category.)
pub fn stdlib_call_effect(module: &str, func: &str) -> Option<Effect> {
    let row = module_effects(module)?;
    if row == ModuleEffects::Pure {
        return None;
    }
    if module == "fan" {
        // A language form with no stdlib source.
        return match row { ModuleEffects::All(e) | ModuleEffects::EffectFns(e) => Some(e), ModuleEffects::Pure => None };
    }
    let decl = module_fns(module).find(|d| fn_name(d) == func)?;
    match row {
        ModuleEffects::Pure => None,
        ModuleEffects::All(e) => Some(e),
        ModuleEffects::EffectFns(e) => fn_is_effect(decl).then_some(e),
    }
}

/// The category a runtime call (`almide_rt_<module>_<fn>`, the `@intrinsic`
/// mangling; `http` / `json` / `regex` also use `almide_<module>_<fn>`) carries. The module is the longest table entry (or alias) the
/// symbol's head names; for an `EffectFns` module the declaring function is
/// found by its `@intrinsic` symbol, and an undeclared symbol fails closed.
pub fn runtime_symbol_effect(symbol: &str) -> Option<Effect> {
    let rest = symbol.strip_prefix("almide_rt_").or_else(|| symbol.strip_prefix("almide_"))?;
    let module = STDLIB_MODULE_EFFECTS
        .iter()
        .map(|(m, _)| (*m, *m))
        .chain(RUNTIME_PREFIX_ALIASES.iter().copied())
        .filter(|(prefix, _)| rest.strip_prefix(prefix).is_some_and(|r| r.starts_with('_')))
        .max_by_key(|(prefix, _)| prefix.len())
        .map(|(_, m)| m)?;
    match module_effects(module)? {
        ModuleEffects::Pure => None,
        ModuleEffects::All(e) => Some(e),
        ModuleEffects::EffectFns(e) => {
            let plain = module_fns(module).any(|d| intrinsic_symbol(d) == Some(symbol) && !fn_is_effect(d));
            (!plain).then_some(e)
        }
    }
}

/// Result of effect inference for a single function.
#[derive(Debug, Clone, Default)]
pub struct FunctionEffects {
    pub direct: HashSet<Effect>,
    pub transitive: HashSet<Effect>,
    pub is_effect: bool,
}

/// Effect analysis results for the entire program.
#[derive(Debug, Clone, Default)]
pub struct EffectMap {
    pub functions: HashMap<String, FunctionEffects>,
}

impl EffectMap {
    pub fn format_effects(effects: &HashSet<Effect>) -> String {
        if effects.is_empty() {
            return "{}".to_string();
        }
        let mut sorted: Vec<_> = effects.iter().collect();
        sorted.sort();
        format!("{{{}}}", sorted.iter().map(|e| e.to_string()).collect::<Vec<_>>().join(", "))
    }
}

#[cfg(test)]
mod classification_gate {
    use super::*;
    use almide_lang::stdlib_info::{BUNDLED_MODULES, STDLIB_MODULES};

    fn stdlib_modules() -> Vec<&'static str> {
        let mut all: Vec<&str> = STDLIB_MODULES.iter().chain(BUNDLED_MODULES).copied().collect();
        all.sort_unstable();
        all.dedup();
        all
    }

    /// Every stdlib module has exactly one row, and every row names a stdlib
    /// module (or `fan`). A module added without a row is unclassified, which
    /// used to mean silently pure.
    #[test]
    fn every_stdlib_module_is_classified_once() {
        let mods = stdlib_modules();
        let missing: Vec<_> = mods.iter().filter(|m| module_effects(m).is_none()).collect();
        assert!(missing.is_empty(), "stdlib modules with no STDLIB_MODULE_EFFECTS row: {missing:?}");
        let mut seen = HashSet::new();
        for (m, _) in STDLIB_MODULE_EFFECTS {
            assert!(seen.insert(*m), "`{m}` has two rows");
            assert!(*m == "fan" || mods.contains(m), "row `{m}` names no stdlib module");
        }
    }

    /// The module the stdlib source of a classified module actually has.
    #[test]
    fn every_classified_stdlib_module_has_a_source() {
        for (m, _) in STDLIB_MODULE_EFFECTS {
            if *m == "fan" { continue; }
            assert!(module_fns(m).next().is_some(), "`{m}` has no parsable source with fns");
        }
    }

    /// A `Pure` module that declares an `effect fn` has to say why that fn
    /// touches nothing; anything else means the module gained host access and
    /// needs a category.
    #[test]
    fn a_pure_module_declares_no_unexplained_effect_fn() {
        for (m, e) in STDLIB_MODULE_EFFECTS {
            if *e != ModuleEffects::Pure { continue; }
            for d in module_fns(m).filter(|d| fn_is_effect(d)) {
                let q = format!("{m}.{}", fn_name(d));
                assert!(HOST_FREE_EFFECT_FNS.contains(&q.as_str()),
                    "`{q}` is an effect fn in a module classified Pure — classify `{m}` or list it in HOST_FREE_EFFECT_FNS");
            }
        }
        for q in HOST_FREE_EFFECT_FNS {
            let (m, f) = q.split_once('.').expect("HOST_FREE_EFFECT_FNS rows are module.fn");
            assert_eq!(module_effects(m), Some(ModuleEffects::Pure), "`{q}` is listed but `{m}` is not Pure");
            assert!(module_fns(m).any(|d| fn_name(d) == f && fn_is_effect(d)), "`{q}` is not an effect fn of `{m}`");
        }
    }

    /// The two routes into a stdlib function — the source call `m.f` and the
    /// runtime call its `@intrinsic` symbol lowers to — classify alike, for
    /// every intrinsic in the stdlib.
    #[test]
    fn every_intrinsic_symbol_classifies_like_its_function() {
        let mut checked = 0;
        for (m, _) in STDLIB_MODULE_EFFECTS {
            for d in module_fns(m) {
                let Some(sym) = intrinsic_symbol(d) else { continue };
                assert_eq!(runtime_symbol_effect(sym), stdlib_call_effect(m, fn_name(d)),
                    "`{sym}` (declared by `{m}.{}`) classifies unlike its function", fn_name(d));
                checked += 1;
            }
        }
        assert!(checked > 500, "only {checked} intrinsics checked — the source scan found nothing");
    }

    /// The rows #3246 fixed.
    #[test]
    fn the_rows_3246_fixed() {
        assert_eq!(stdlib_call_effect("random", "int"), Some(Effect::Rand));
        assert_eq!(runtime_symbol_effect("almide_rt_random_int"), Some(Effect::Rand));
        assert_eq!(stdlib_call_effect("io", "read_line"), Some(Effect::IO));
        assert_eq!(stdlib_call_effect("io", "write"), Some(Effect::IO));
        assert_eq!(stdlib_call_effect("path", "join"), None);
        assert_eq!(stdlib_call_effect("url", "parse"), None);
        assert_eq!(stdlib_call_effect("datetime", "now"), Some(Effect::Time));
        assert_eq!(stdlib_call_effect("datetime", "format"), None);
        assert_eq!(runtime_symbol_effect("almide_rt_datetime_format"), None);
        assert_eq!(stdlib_call_effect("http", "get"), Some(Effect::Net));
        assert_eq!(stdlib_call_effect("http", "response"), None);
        assert_eq!(runtime_symbol_effect("almide_rt_sse_openai_chat"), Some(Effect::Net));
        assert_eq!(stdlib_call_effect("list", "map"), None);
        assert_eq!(stdlib_call_effect("not_a_module", "f"), None);
        // #2223: a user module named like a stdlib module, calling a fn the
        // stdlib module does not declare, is not the stdlib module.
        assert_eq!(stdlib_call_effect("net", "f"), None);
        assert_eq!(stdlib_call_effect("random", "my_helper"), None);
        assert_eq!(stdlib_call_effect("fan", "map"), Some(Effect::Fan));
        assert_eq!(runtime_symbol_effect("almide_http_get"), Some(Effect::Net));
        assert_eq!(runtime_symbol_effect("almide_regex_find"), None);
        assert_eq!(runtime_symbol_effect("println"), None);
    }
}
