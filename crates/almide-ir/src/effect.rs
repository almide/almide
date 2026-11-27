use std::collections::{HashMap, HashSet};

/// Effect categories: the leaves of ADR-0026 D2's closed two-level hierarchy.
/// Inference and `[permissions]` work in leaves; a manifest may grant a parent
/// (`FS`, `Net`, `IO`, `Env`), which means the leaves listed under it in
/// [`PARENTS`] — a leaf added later does not join a parent on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Effect {
    FsRead,
    FsWrite,
    NetFetch,
    NetListen,
    IoStdin,
    IoStdout,
    IoStderr,
    EnvRead,
    EnvWrite,
    Proc,
    Time,
    Rand,
    Fan,
    Abort,
}

impl std::fmt::Display for Effect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// The four parents of ADR-0026 D2 and their leaves. Every other leaf has no
/// parent. `Proc` is under none: starting a program can reach every other
/// capability through the program, so no grant implies it (#4155).
pub const PARENTS: &[(&str, &[Effect])] = &[
    ("FS", &[Effect::FsRead, Effect::FsWrite]),
    ("Net", &[Effect::NetFetch, Effect::NetListen]),
    ("IO", &[Effect::IoStdin, Effect::IoStdout, Effect::IoStderr]),
    ("Env", &[Effect::EnvRead, Effect::EnvWrite]),
];

impl Effect {
    /// Every leaf, in declaration order. It is also ⊤: what a call is assumed
    /// to do when nothing bounds it — today, a call to an `@extern` fn (#3245).
    /// A foreign body is opaque to inference and no bound can be declared on
    /// it yet (ADR-0027 §5), so `[permissions].allow` passes it only when it
    /// grants every leaf.
    pub const ALL: [Effect; 14] = [
        Effect::FsRead, Effect::FsWrite, Effect::NetFetch, Effect::NetListen,
        Effect::IoStdin, Effect::IoStdout, Effect::IoStderr, Effect::EnvRead,
        Effect::EnvWrite, Effect::Proc, Effect::Time, Effect::Rand, Effect::Fan,
        Effect::Abort,
    ];

    /// The leaf's name as `[permissions].allow`, `--effects` and diagnostics
    /// spell it: capitalised and dotted under a parent (`FS.write`), so it
    /// cannot be spelled like a function name (ADR-0026 D4).
    pub fn name(self) -> &'static str {
        match self {
            Effect::FsRead => "FS.read",
            Effect::FsWrite => "FS.write",
            Effect::NetFetch => "Net.fetch",
            Effect::NetListen => "Net.listen",
            Effect::IoStdin => "IO.stdin",
            Effect::IoStdout => "IO.stdout",
            Effect::IoStderr => "IO.stderr",
            Effect::EnvRead => "Env.read",
            Effect::EnvWrite => "Env.write",
            Effect::Proc => "Proc",
            Effect::Time => "Time",
            Effect::Rand => "Rand",
            Effect::Fan => "Fan",
            Effect::Abort => "Abort",
        }
    }

    /// The leaf a name spells, or `None`.
    pub fn from_name(name: &str) -> Option<Effect> {
        Self::ALL.into_iter().find(|e| e.name() == name)
    }

    /// The leaves a `[permissions].allow` name grants: a leaf grants itself, a
    /// parent its leaves (#4155). `None` for any other name.
    pub fn granted_by(name: &str) -> Option<Vec<Effect>> {
        if let Some(e) = Self::from_name(name) {
            return Some(vec![e]);
        }
        PARENTS.iter().find(|(p, _)| *p == name).map(|(_, leaves)| leaves.to_vec())
    }

    /// Every name a manifest may write: the parents, then the leaves.
    pub fn grantable_names() -> Vec<&'static str> {
        PARENTS.iter().map(|(p, _)| *p).chain(Self::ALL.iter().map(|e| e.name())).collect()
    }
}

/// What the output and abort builtins carry (ADR-0026 D2: `panic` carries
/// `Abort` and `IO.stderr` both). They are admissible in a plain `fn`
/// (ADR-0022), so they never make a function `effect`; a manifest that
/// grants neither still refuses them. A failing `assert*` writes its report
/// to stderr and ends the program.
pub const BUILTIN_EFFECTS: &[(&str, &[Effect])] = &[
    ("println", &[Effect::IoStdout]),
    ("eprintln", &[Effect::IoStderr]),
    ("panic", &[Effect::Abort, Effect::IoStderr]),
    ("assert", &[Effect::Abort, Effect::IoStderr]),
    ("assert_eq", &[Effect::Abort, Effect::IoStderr]),
    ("assert_ne", &[Effect::Abort, Effect::IoStderr]),
];

/// The categories of a call to the builtin `name`; empty for any other name.
pub fn builtin_call_effects(name: &str) -> Vec<Effect> {
    BUILTIN_EFFECTS.iter().find(|(n, _)| *n == name).map_or_else(Vec::new, |(_, e)| e.to_vec())
}

/// Is `name` an output builtin that does not end the program — one whose
/// leaves are streams only (`println`, `eprintln`), as opposed to an abort
/// builtin (`panic`, `assert*`), which carries `Abort`?
pub(crate) fn builtin_writes_without_aborting(name: &str) -> bool {
    let e = builtin_call_effects(name);
    !e.is_empty() && !e.contains(&Effect::Abort)
}

/// Do `cats` describe only ADR-0022's halt — `Abort`, with nothing beyond
/// what an abort builtin (`panic`, `assert`) carries, its report on stderr?
/// A testing assertion is such a fn (#4951): it reaches no host capability,
/// so a consumer asking "may this run without one" (the native renderer's
/// purity) admits it, while `@pure` and `[permissions]` still see the leaves.
pub fn only_halts(cats: &[Effect]) -> bool {
    let halt: Vec<Effect> = BUILTIN_EFFECTS.iter().filter(|(_, e)| e.contains(&Effect::Abort)).flat_map(|(_, e)| e.iter().copied()).collect();
    cats.contains(&Effect::Abort) && cats.iter().all(|c| halt.contains(c))
}

/// The leaves a plain `fn` may carry (ADR-0022): those of the output and
/// abort builtins, which a plain fn may call. Any other leaf belongs to an
/// `effect fn`.
#[cfg(test)]
pub(crate) fn plain_fn_leaves() -> Vec<Effect> {
    let mut v: Vec<Effect> = BUILTIN_EFFECTS.iter().flat_map(|(_, e)| e.iter().copied()).collect();
    v.sort_by_key(|e| e.name());
    v.dedup();
    v
}

/// How the calls into one stdlib module are classified. A function may name
/// its own categories with `@category("FS.write", …)` on its declaration,
/// which replaces the module's default for that function (ADR-0026 D2:
/// categories are per operation, not per module).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleEffects {
    /// No function of the module reaches the host.
    Pure,
    /// Every function of the module carries these categories unless it
    /// declares its own.
    All(&'static [Effect]),
    /// The module's `effect fn`s carry these categories unless they declare
    /// their own; its plain `fn`s are pure unless they declare some. Read from
    /// the module's own source, so the classification of a function is its
    /// `effect` marker and cannot drift from it.
    EffectFns(&'static [Effect]),
}

/// THE classification of every stdlib module, by what its functions do to the
/// host (#3246). One row per module in `STDLIB_MODULES` ∪ `BUNDLED_MODULES`,
/// plus the `fan` language module; the gate in this file's tests refuses a
/// module without a row, a row without a module, a `Pure` module that declares
/// an unexplained `effect fn`, and an `@intrinsic` symbol whose classification
/// disagrees with the function that declares it.
///
/// A row is the module's default; a function whose operation differs says so
/// with `@category(…)` in its own declaration (`fs.write_text` is `FS.write`,
/// `process.pid` `Env.read`), never in a list here (#4155).
pub const STDLIB_MODULE_EFFECTS: &[(&str, ModuleEffects)] = &[
    // Files. A function that changes the file system declares `FS.write`.
    ("fs", ModuleEffects::All(&[Effect::FsRead])),
    // The standard streams: each function declares its stream.
    ("io", ModuleEffects::All(&[Effect::IoStdin])),
    // The network. `http`'s plain fns are request/response builders and the
    // router — values, not traffic; `http.serve` declares `Net.listen`.
    ("http", ModuleEffects::EffectFns(&[Effect::NetFetch])),
    // Sockets: connecting is `Net.fetch`, listening and accepting
    // `Net.listen` (declared on those fns). An operation on an open stream or
    // a shared-memory region cannot tell which side opened it, so it carries
    // both.
    ("net", ModuleEffects::All(&[Effect::NetFetch, Effect::NetListen])),
    // Unix-domain sockets and shared memory (R-unix-01): connecting is
    // `Net.fetch`, listening and accepting `Net.listen`, as in `net`; an
    // operation on an open descriptor carries both.
    ("unix", ModuleEffects::All(&[Effect::NetFetch, Effect::NetListen])),
    // `uuid.v4` draws entropy; `v7` reads the clock too (declared there).
    // parse / to_string / the byte conversions / compare are pure.
    ("uuid", ModuleEffects::EffectFns(&[Effect::Rand])),
    // The process environment: variables and argv. `env.set` and
    // `env.remove` declare `Env.write`; the clock readers declare `Time`.
    ("env", ModuleEffects::All(&[Effect::EnvRead])),
    // Starting and signalling processes. The reader `process.pid`
    // declares `Env.read`, `process.exit` `Abort`.
    ("process", ModuleEffects::All(&[Effect::Proc])),
    ("args", ModuleEffects::All(&[Effect::EnvRead])),
    // The clock: `datetime.now` / `monotonic_ns`; the calendar math is pure.
    ("datetime", ModuleEffects::EffectFns(&[Effect::Time])),
    // Entropy.
    ("random", ModuleEffects::All(&[Effect::Rand])),
    // `fan` is a language form, not a stdlib module; `fan.map` and friends
    // lower to Module calls on it.
    ("fan", ModuleEffects::All(&[Effect::Fan])),
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
    // `ffi.read` / `ffi.write` touch memory outside Almide's values (R-ffi-02)
    // but reach no host capability: the address is the program's own memory,
    // or one an `@extern` fn handed back (and an `@extern` is already every
    // category). Memory safety is not a category: the fns say so with
    // `@memory_unsafe`, which `--profile critical` refuses (#4008).
    ("ffi", ModuleEffects::Pure),
    // Collections and scalars.
    ("list", ModuleEffects::Pure),
    ("map", ModuleEffects::Pure),
    ("set", ModuleEffects::Pure),
    ("deque", ModuleEffects::Pure),
    ("heap", ModuleEffects::Pure),
    ("secret", ModuleEffects::Pure),
    ("option", ModuleEffects::Pure),
    ("result", ModuleEffects::Pure),
    ("bytes", ModuleEffects::Pure),
    ("matrix", ModuleEffects::Pure),
    ("math", ModuleEffects::Pure),
    ("bool", ModuleEffects::Pure),
    ("int", ModuleEffects::Pure),
    ("float", ModuleEffects::Pure),
    ("bigint", ModuleEffects::Pure),
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
    // Assertion helpers: a failing one aborts through the builtins it calls,
    // which carry `Abort` themselves. The two abort-site leaves are effect
    // fns with their own `@category` (a child process, the selected site).
    ("testing", ModuleEffects::EffectFns(&[])),
    // The primitive floor is stdlib-internal (E085 outside it); its host ops
    // are classified at the public function that wraps them, so a stdlib
    // body over the floor does not charge its callers twice.
    ("prim", ModuleEffects::Pure),
];

/// `effect fn`s in a `Pure` module, each with no host access. The gate refuses
/// any other `effect fn` in a `Pure` module: a new one must be classified.
pub const HOST_FREE_EFFECT_FNS: &[&str] = &[
    "zlib.compress", "zlib.compress_level", "zlib.deflate", "zlib.deflate_level", "zlib.gzip",
];

/// Runtime-symbol prefixes that are not their module's name
/// (`almide_rt_<prefix>_…`).
const RUNTIME_PREFIX_ALIASES: &[(&str, &str)] = &[("test", "testing")];

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

/// The categories a declaration names with `@category("…", …)`, if it has
/// the attribute. An unknown name is a stdlib bug the gate below refuses;
/// here it is skipped.
fn declared_categories(decl: &almide_lang::ast::Decl) -> Option<Vec<Effect>> {
    let almide_lang::ast::Decl::Fn { attrs, .. } = decl else { return None };
    let a = attrs.iter().find(|a| a.name.as_str() == "category")?;
    Some(a.args.iter().filter_map(|x| match &x.value {
        almide_lang::ast::AttrValue::String { value } => Effect::from_name(value),
        _ => None,
    }).collect())
}

/// The categories of one declared function of a classified module.
fn decl_effects(row: ModuleEffects, decl: &almide_lang::ast::Decl) -> Vec<Effect> {
    if let Some(own) = declared_categories(decl) {
        return own;
    }
    match row {
        ModuleEffects::Pure => Vec::new(),
        ModuleEffects::All(e) => e.to_vec(),
        ModuleEffects::EffectFns(e) => if fn_is_effect(decl) { e.to_vec() } else { Vec::new() },
    }
}

/// The categories a declaration `decl` of the stdlib module `module` carries:
/// its own `@category`, else the module's row. Empty for a module with no
/// row. [`stdlib_call_effect`] is this over the bundled source; a gate that
/// parsed a declaration itself asks here (C-460).
pub fn declaration_effects(module: &str, decl: &almide_lang::ast::Decl) -> Vec<Effect> {
    module_effects(module).map_or_else(Vec::new, |row| decl_effects(row, decl))
}

/// Is `module.func` a function the stdlib declares — a fn of its module's
/// own source, or the `fan` language form — so [`stdlib_call_effect`] answers
/// for it from a declaration?
pub fn stdlib_declares(module: &str, func: &str) -> bool {
    match module_effects(module) {
        None => false,
        Some(_) if module == "fan" => true,
        Some(_) => module_fns(module).any(|d| fn_name(d) == func),
    }
}

/// What a call the classification cannot answer for carries — a stdlib
/// call of a fn no stdlib source declares, a runtime symbol whose head
/// names no classified module: every leaf, ⊤, as an `@extern` does
/// (decision rows purity.effect-classification, effect.flow-categories).
pub fn unknown_call_effects() -> Vec<Effect> {
    Effect::ALL.to_vec()
}

/// The categories a STDLIB call `module.func` carries, empty for a pure call.
///
/// Read off the declaration: its own `@category`, else its module's row. A
/// call this cannot answer for — a module the table does not classify, a fn
/// the module's source does not declare — is unknown and carries every
/// category ([`unknown_call_effects`]), never the empty set. A consumer asks
/// only about a call it resolved to the stdlib: a user module that shares a
/// stdlib module's name (`net`, `io` — #2223) is a user module, judged by its
/// own body through the call graph, and never reaches here.
pub fn stdlib_call_effect(module: &str, func: &str) -> Vec<Effect> {
    let Some(row) = module_effects(module) else { return unknown_call_effects() };
    if module == "fan" {
        // A language form with no stdlib source.
        return match row { ModuleEffects::All(e) | ModuleEffects::EffectFns(e) => e.to_vec(), ModuleEffects::Pure => Vec::new() };
    }
    module_fns(module).find(|d| fn_name(d) == func).map_or_else(unknown_call_effects, |d| decl_effects(row, d))
}

/// The classified module a runtime symbol's head names: the longest table
/// entry (or alias) after `almide_rt_` / `almide_`, `None` when it names none.
fn runtime_symbol_module(symbol: &str) -> Option<&'static str> {
    let rest = symbol.strip_prefix("almide_rt_").or_else(|| symbol.strip_prefix("almide_"))?;
    STDLIB_MODULE_EFFECTS
        .iter()
        .map(|(m, _)| (*m, *m))
        .chain(RUNTIME_PREFIX_ALIASES.iter().copied())
        .filter(|(prefix, _)| rest.strip_prefix(prefix).is_some_and(|r| r.starts_with('_')))
        .max_by_key(|(prefix, _)| prefix.len())
        .map(|(_, m)| m)
}

/// The `@intrinsic` declaration of `symbol` in a classified module's source,
/// with its module: the declaration a runtime symbol is classified by,
/// wherever its head points (`value.__decode_list_string` is
/// `almide_rt___decode_list_string`). The first module in
/// [`STDLIB_MODULE_EFFECTS`] order that declares it answers.
///
/// Only a module whose source text names the symbol in an `@intrinsic(..)`
/// attribute is parsed (#4458: a program parses only the stdlib it reaches);
/// the parse then confirms the declaration, so the text alone decides
/// nothing (an occurrence in a comment finds no fn and is skipped).
fn intrinsic_declaration(symbol: &str) -> Option<(&'static str, &'static almide_lang::ast::Decl)> {
    static CANDIDATES: std::sync::OnceLock<HashMap<&'static str, Vec<&'static str>>> = std::sync::OnceLock::new();
    let candidates = CANDIDATES.get_or_init(|| {
        let mut map: HashMap<&'static str, Vec<&'static str>> = HashMap::new();
        for (m, _) in STDLIB_MODULE_EFFECTS {
            let Some(src) = almide_lang::stdlib_info::bundled_source(m) else { continue };
            for sym in intrinsic_literals(src) {
                let ms = map.entry(sym).or_default();
                if !ms.contains(m) {
                    ms.push(m);
                }
            }
        }
        map
    });
    candidates.get(symbol)?.iter().find_map(|m| module_fns(m).find(|d| intrinsic_symbol(d) == Some(symbol)).map(|d| (*m, d)))
}

/// The string literal opening each `@intrinsic(` in `src`, unparsed.
fn intrinsic_literals(src: &'static str) -> impl Iterator<Item = &'static str> {
    src.split("@intrinsic(").skip(1).filter_map(|rest| {
        let lit = rest.trim_start().strip_prefix('"')?;
        lit.find('"').map(|end| &lit[..end])
    })
}

/// Is `symbol` a runtime symbol the classification answers for — the
/// `@intrinsic` of a declared stdlib fn, or one whose head names a classified
/// module ([`runtime_symbol_effect`])?
pub fn runtime_symbol_classified(symbol: &str) -> bool {
    intrinsic_declaration(symbol).is_some() || runtime_symbol_module(symbol).is_some()
}

/// The categories a runtime call (`almide_rt_<module>_<fn>`, the `@intrinsic`
/// mangling; `http` / `json` / `regex` also use `almide_<module>_<fn>`)
/// carries. The module is the longest table entry (or alias) the symbol's
/// head names; the declaring function is found by its `@intrinsic` symbol,
/// and an undeclared symbol fails closed to the module's whole default. A
/// symbol whose head names no classified module is unknown: every category
/// ([`unknown_call_effects`]).
pub fn runtime_symbol_effect(symbol: &str) -> Vec<Effect> {
    if let Some((m, d)) = intrinsic_declaration(symbol) {
        return module_effects(m).map_or_else(unknown_call_effects, |row| decl_effects(row, d));
    }
    let Some(module) = runtime_symbol_module(symbol) else { return unknown_call_effects() };
    let Some(row) = module_effects(module) else { return unknown_call_effects() };
    if row == ModuleEffects::Pure {
        return Vec::new();
    }
    match module_fns(module).find(|d| intrinsic_symbol(d) == Some(symbol)) {
        Some(d) => decl_effects(row, d),
        None => match row {
            ModuleEffects::All(e) | ModuleEffects::EffectFns(e) => e.to_vec(),
            ModuleEffects::Pure => Vec::new(),
        },
    }
}

/// The categories of a compiler-derived helper the IR calls by a bare name
/// (`__err_at`, `__decode_option_string`, `__encode_list_int`: the derived
/// Codec's), read off the bodies the self-host registry serves under that
/// name: a body's own `@category`, else nothing when no body serving it
/// reaches a floor host op or an output builtin, else every category.
/// `None` when no registry row serves the name — not a derived helper.
pub fn derived_helper_effect(call: &str) -> Option<Vec<Effect>> {
    let bodies = crate::host_reach::registry_bodies(call);
    if bodies.is_empty() {
        return None;
    }
    let mut out: Vec<Effect> = Vec::new();
    for (prog, name) in bodies {
        let Some(d) = prog.decls.iter().find(|d| matches!(d, almide_lang::ast::Decl::Fn { name: n, .. } if n.as_str() == name)) else { continue };
        let cats = match declared_categories(d) {
            Some(own) => own,
            None if crate::host_reach::body_reaches_category(prog, &name) => unknown_call_effects(),
            None => Vec::new(),
        };
        for c in cats {
            if !out.contains(&c) {
                out.push(c);
            }
        }
    }
    Some(out)
}

/// The declared stdlib fns the classification charges nothing — a fn of a
/// `Pure` module, a plain fn of an `EffectFns` module, an `@category()` —
/// that a body serving them shows reaching a floor host op or an output
/// builtin that does not abort, directly or through what it calls (decision
/// rows purity.effect-classification, pure.e092-reach, #4827, #4951). A
/// bodyless fn no self-host body serves is judged by its declaration (an
/// explicit `@category(…)`, else its module's row). An `effect fn` declaring
/// `@category()` whose only reach is a method it dispatches on a
/// protocol-bounded param (`io.read[H: Read](h)`) is charged at the conformer
/// instead: the handle is the capability (R-L0-44), the effect flow charges
/// the dispatch with every conformer the program may pick, and no plain fn —
/// so no `@pure` one — may call an `effect fn` (E006).
///
/// `extra` replaces one module's bundled source (a gate's control or novel
/// test). Returns the offenders and how many fns were judged.
pub fn uncharged_fns_reaching_a_category(extra: Option<(&str, &'static almide_lang::ast::Program)>) -> (Vec<String>, usize) {
    let mut bad = Vec::new();
    let mut judged = 0;
    for (m, row) in STDLIB_MODULE_EFFECTS {
        let Some(bundled) = almide_lang::stdlib_info::bundled_source(m).and_then(almide_lang::parse_cached) else { continue };
        let replaced = extra.filter(|(em, _)| em == m).map(|(_, p)| p);
        let prog = replaced.unwrap_or(bundled);
        for d in prog.decls.iter().filter(|d| matches!(d, almide_lang::ast::Decl::Fn { .. })) {
            if crate::host_reach::is_host_op(d) || !decl_effects(*row, d).is_empty() {
                continue;
            }
            judged += 1;
            let reaches = match replaced {
                Some(p) => crate::host_reach::fn_reaches_category_in(p, m, fn_name(d)),
                None => crate::host_reach::stdlib_fn_reaches_category(m, fn_name(d)),
            };
            let at_the_conformer = fn_is_effect(d)
                && declared_categories(d).is_some_and(|c| c.is_empty())
                && !crate::host_reach::stdlib_fn_reaches_category_but_dispatch(m, fn_name(d));
            if reaches && !at_the_conformer {
                bad.push(format!("{m}.{}", fn_name(d)));
            }
        }
    }
    bad.sort();
    bad.dedup();
    (bad, judged)
}

/// Is the stdlib function `module.func` declared `effect fn` (#4151)? Read
/// from the module's own source, like [`stdlib_call_effect`]. This is the
/// effect BIT, not a category: `zlib.compress` is an `effect fn` with no host
/// access, and `path.join` is a plain fn. A name the module does not declare
/// is not a stdlib function (a user module sharing the name), so `false`.
///
/// An `effect fn` whose result is `Never` and whose body is the runtime's
/// (`process.exit`) only ends the program: that is ADR-0022's abort, which a
/// plain `fn` may do (the lowered `assert` failure arm calls it), so it does
/// not carry the bit. Read off the declaration, never a name list.
pub fn stdlib_fn_is_effect(module: &str, func: &str) -> bool {
    module_fns(module).any(|d| fn_name(d) == func && fn_is_effect(d) && !fn_only_aborts(d))
}

/// A bodyless (`= _`) declaration whose result type is `Never`: the runtime
/// ends the program and nothing else (see [`stdlib_fn_is_effect`]).
fn fn_only_aborts(decl: &almide_lang::ast::Decl) -> bool {
    let almide_lang::ast::Decl::Fn { body, return_type: almide_lang::ast::TypeExpr::Simple { name }, .. } = decl else {
        return false;
    };
    let bodyless = body.as_ref().is_none_or(|b| matches!(b.kind, almide_lang::ast::ExprKind::Hole));
    bodyless && name.as_str() == "Never"
}

/// Is the runtime symbol `symbol` the `@intrinsic` of a stdlib `effect fn`
/// (#4151)? Found by the declaring function's `@intrinsic` symbol, over
/// every classified module.
pub fn runtime_symbol_is_effect(symbol: &str) -> bool {
    STDLIB_MODULE_EFFECTS
        .iter()
        // A module whose source never spells the symbol as a string literal
        // cannot declare it: only those that do are parsed (#4458). The quotes
        // keep a module that merely spells a longer symbol with this one as
        // its prefix from being parsed (#4695).
        .filter(|(m, _)| {
            !symbol.bytes().all(|b| b.is_ascii_graphic() && b != b'\\' && b != b'"')
                || almide_lang::stdlib_info::bundled_source(m).is_some_and(|s| s.contains(&format!("\"{symbol}\"")))
        })
        .any(|(m, _)| module_fns(m).any(|d| intrinsic_symbol(d) == Some(symbol) && fn_is_effect(d) && !fn_only_aborts(d)))
}

/// What an unscanned stdlib function may do with one argument that holds a
/// fn: run it, keep it (store it where something else reads it later), or
/// both. Read off the declaration (#4151) so `check`, which does not lower the
/// stdlib's bodies, follows a value as the build routes do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParamUse {
    pub runs: bool,
    pub keeps: bool,
}

/// [`ParamUse`] per parameter of the stdlib function `module.func`, or
/// `None` for a name the module does not declare.
///
/// - A bodyless (`= _`) declaration runs its fn-typed parameters and keeps
///   none of them (no `@intrinsic` stores a callback); any other parameter
///   may be kept.
/// - An Almide body is not read here: every parameter may be run and kept —
///   except a parameter whose type is an explicit `effect (A) -> B` slot (or
///   an alias of one) on a plain `fn`: the checker refuses that body a call
///   of it (E006), so it can only keep it (`http.route`'s handler).
pub fn stdlib_param_uses(module: &str, func: &str) -> Option<Vec<ParamUse>> {
    use almide_lang::ast::{Decl, ExprKind, TypeExpr};
    let decl = module_fns(module).find(|d| fn_name(d) == func)?;
    let Decl::Fn { params, body, effect, .. } = decl else { return None };
    let bodyless = body.as_ref().is_none_or(|b| matches!(b.kind, ExprKind::Hole));
    let is_effect_fn = effect.unwrap_or(false);
    let effect_slot = |t: &TypeExpr| match t {
        TypeExpr::Fn { is_effect, .. } => *is_effect,
        TypeExpr::Simple { name } => module_type_alias(module, name.as_str())
            .is_some_and(|t| matches!(t, TypeExpr::Fn { is_effect: true, .. })),
        _ => false,
    };
    Some(params.iter().map(|p| {
        let fn_typed = matches!(p.ty, TypeExpr::Fn { .. }) || effect_slot(&p.ty);
        match (bodyless, fn_typed) {
            (true, true) => ParamUse { runs: true, keeps: false },
            (true, false) => ParamUse { runs: false, keeps: true },
            (false, _) if effect_slot(&p.ty) && !is_effect_fn => ParamUse { runs: false, keeps: true },
            (false, _) => ParamUse { runs: true, keeps: true },
        }
    }).collect())
}

/// The target of `type name = …` in the stdlib module's own source.
fn module_type_alias(module: &str, name: &str) -> Option<&'static almide_lang::ast::TypeExpr> {
    almide_lang::stdlib_info::bundled_source(module)
        .and_then(almide_lang::parse_cached)?
        .decls
        .iter()
        .find_map(|d| match d {
            almide_lang::ast::Decl::Type { name: n, ty, .. } if n.as_str() == name => Some(ty),
            _ => None,
        })
}

/// Result of effect inference for a single function.
#[derive(Debug, Clone, Default)]
pub struct FunctionEffects {
    pub direct: HashSet<Effect>,
    pub transitive: HashSet<Effect>,
    pub is_effect: bool,
    /// The fn-typed parameters whose closures this function may run (#3268),
    /// as `f (arg 1)`, in parameter order. What they do is charged at each
    /// call site, with the closure that call site passes (ADR-0026 D1): a
    /// bare fn-type parameter is transparent for the category set. A
    /// callee is never invented for them (ADR-0026 D4).
    pub indirect: Vec<String>,
    /// The categories of the closure this function returns, when it returns
    /// one: a returned fn value keeps the set of the value returned.
    pub returns: HashSet<Effect>,
    /// Parameters whose closures the returned closure may run.
    pub returns_indirect: Vec<String>,
    /// One representative path per category in `transitive`, from this
    /// function to the operation (ADR-0026 D4): `main → use_it → f (arg 1 of
    /// use_it) → closure (line 4:42 in make) → fs.read_text (line 4:51)`.
    pub paths: HashMap<Effect, String>,
}

impl FunctionEffects {
    /// No category of its own, but it runs closures it does not create.
    pub fn is_callback_dependent(&self) -> bool {
        self.transitive.is_empty() && !self.indirect.is_empty()
    }

    /// The `almide check --effects` cell: `{IO}`, `{}`, or
    /// `{} + whatever f (arg 1) does`.
    pub fn report(&self) -> String {
        let set = EffectMap::format_effects(&self.transitive);
        match self.indirect.as_slice() {
            [] => set,
            [one] => format!("{set} + whatever {one} does"),
            many => format!("{set} + whatever {} do", many.join(", ")),
        }
    }

    /// `returns a closure doing {IO}`, when the function returns an
    /// effectful closure (or one that runs a closure it was handed).
    pub fn returns_report(&self) -> Option<String> {
        if self.returns.is_empty() && self.returns_indirect.is_empty() {
            return None;
        }
        let set = EffectMap::format_effects(&self.returns);
        Some(match self.returns_indirect.as_slice() {
            [] => format!("returns a closure doing {set}"),
            many => format!("returns a closure doing {set} + whatever {} does", many.join(", ")),
        })
    }
}

/// Effect analysis results for the entire program.
#[derive(Debug, Clone, Default)]
pub struct EffectMap {
    pub functions: HashMap<String, FunctionEffects>,
}

impl EffectMap {
    /// `(pure, callback-dependent, with effects)` over `functions`: a
    /// function that only calls closures it is handed is not counted pure.
    pub fn summary_counts<'a>(functions: impl Iterator<Item = &'a FunctionEffects>) -> (usize, usize, usize) {
        functions.fold((0, 0, 0), |(p, d, e), fe| match (fe.transitive.is_empty(), fe.indirect.is_empty()) {
            (true, true) => (p + 1, d, e),
            (true, false) => (p, d + 1, e),
            (false, _) => (p, d, e + 1),
        })
    }

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
                // `@memory_unsafe` (#4008) is the explanation: its effect is
                // memory outside Almide's values, not a host capability.
                if almide_lang::stdlib_info::is_memory_unsafe_stdlib_fn(m, fn_name(d)) { continue; }
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

    /// The fns of `prog`, a source of stdlib `module`, that reach a floor
    /// host op (#4827). A host op's own declaration is the floor, not a reach.
    fn host_reaching_fns(module: &str, prog: &almide_lang::ast::Program) -> Vec<String> {
        prog.decls.iter().filter(|d| !crate::host_reach::is_host_op(d)).map(fn_name).filter(|f| !f.is_empty())
            .filter(|f| crate::host_reach::fn_reaches_host(prog, module, f)).map(str::to_string).collect()
    }

    /// #4827: a `Pure` module charges nothing, so no fn of one — plain or
    /// `effect`, declared in its source or served under its name by a
    /// self-host body — may reach a host op the floor declares
    /// (`@prim_op(host)`), directly or through what it calls. The floor's
    /// own host-op declarations are the ops themselves, not a reach.
    #[test]
    fn a_pure_module_reaches_no_declared_host_op() {
        let mut bad = Vec::new();
        for (m, e) in STDLIB_MODULE_EFFECTS {
            if *e != ModuleEffects::Pure { continue; }
            if let Some(prog) = almide_lang::stdlib_info::bundled_source(m).and_then(almide_lang::parse_cached) {
                bad.extend(host_reaching_fns(m, prog).into_iter().map(|f| format!("{m}.{f}")));
            }
        }
        for (src, rows) in almide_lang::self_host_registry::self_host_runtime() {
            let Some(prog) = almide_lang::parse_cached(src) else { continue };
            for (imp, call) in rows.iter() {
                let Some((m, _)) = call.split_once('.') else { continue };
                if module_effects(m) == Some(ModuleEffects::Pure) && crate::host_reach::fn_reaches_host(prog, m, imp) {
                    bad.push(call.to_string());
                }
            }
        }
        bad.sort();
        bad.dedup();
        assert!(bad.is_empty(), "fns of a module classified Pure reach a host op: {bad:?} — classify the module or the fn");
    }

    /// The general form of the gate above (decision rows
    /// purity.effect-classification, pure.e092-reach, #4951): EVERY declared
    /// stdlib fn the classification charges nothing reaches no host op and
    /// no output builtin (an abort is ADR-0022's halt; a fn whose purpose is
    /// the abort, a testing assertion, declares it: #4951). So "charged nothing" is never a missed
    /// category, whatever the module's row says; a new fn of any module is
    /// held to it, and `@pure` (E092), `[permissions]` and `--profile
    /// critical`, which all read the declaration, see what the body does.
    #[test]
    fn a_fn_charged_no_category_reaches_no_category() {
        let (bad, judged) = uncharged_fns_reaching_a_category(None);
        assert!(bad.is_empty(), "stdlib fns charged no category reach a host op or an output builtin: {bad:?} — declare their @category");
        assert!(judged > 1000, "only {judged} uncharged fns judged — the source scan found nothing");
    }

    /// The control of the gate above: a fresh plain fn added to a module whose
    /// fns are charged nothing by default, writing stderr through a fresh
    /// helper, is refused; the same fn declaring the leaf is not.
    #[test]
    fn a_fresh_uncharged_fn_reaching_an_output_is_refused() {
        let real = almide_lang::stdlib_info::bundled_source("testing").expect("testing has a source");
        let fresh = format!("{real}\nfn __fresh_helper_4951(s: String) -> Unit = eprintln(s)\nfn fresh_check_4951(r: Bool) -> Unit = if r then () else __fresh_helper_4951(\"no\")\n");
        let prog = almide_lang::parse_cached(Box::leak(fresh.into_boxed_str())).expect("parses");
        let (bad, _) = uncharged_fns_reaching_a_category(Some(("testing", prog)));
        assert!(bad.contains(&"testing.fresh_check_4951".to_string()), "a fresh fn writing stderr is admitted: {bad:?}");
        let declared = format!("{real}\nfn __fresh_helper_4951(s: String) -> Unit = eprintln(s)\n@category(\"Abort\", \"IO.stderr\")\nfn fresh_check_4951(r: Bool) -> Unit = if r then () else __fresh_helper_4951(\"no\")\n");
        let prog = almide_lang::parse_cached(Box::leak(declared.into_boxed_str())).expect("parses");
        let (bad, _) = uncharged_fns_reaching_a_category(Some(("testing", prog)));
        assert!(!bad.contains(&"testing.fresh_check_4951".to_string()), "a declared fn is refused: {bad:?}");
    }

    /// The gate's fixture and control: a plain fn added to a `Pure` module
    /// whose body writes a file through the floor is refused; the module's
    /// own source is not.
    #[test]
    fn a_plain_fn_writing_a_file_in_a_pure_module_is_refused() {
        let real = almide_lang::stdlib_info::bundled_source("string").expect("string has a source");
        let control = almide_lang::parse_cached(real).expect("string parses");
        assert!(host_reaching_fns("string", control).is_empty(), "control: string reaches no host op");
        let injected = format!("{real}\nfn sneaky(p: String) -> Int = prim.write_text_file(p, p)\n");
        let prog = almide_lang::parse_cached(Box::leak(injected.into_boxed_str())).expect("injected source parses");
        assert_eq!(host_reaching_fns("string", prog), vec!["sneaky".to_string()]);
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

    /// The text scan that picks which modules to parse finds every
    /// `@intrinsic` a full parse of every classified module finds, and the
    /// same first declaring module.
    #[test]
    fn the_intrinsic_lookup_parses_only_where_the_text_names_the_symbol_and_misses_none() {
        let mut first: HashMap<&str, &str> = HashMap::new();
        for (m, _) in STDLIB_MODULE_EFFECTS {
            for d in module_fns(m) {
                if let Some(sym) = intrinsic_symbol(d) {
                    first.entry(sym).or_insert(m);
                }
            }
        }
        assert!(first.len() > 500, "only {} intrinsics found", first.len());
        for (sym, m) in &first {
            assert_eq!(intrinsic_declaration(sym).map(|(dm, _)| dm), Some(*m), "`{sym}`");
        }
        assert!(intrinsic_declaration("almide_rt_no_such_symbol_zz").is_none());
    }

    /// The rows #3246 fixed, in ADR-0026 D2's leaves.
    #[test]
    fn the_rows_3246_fixed() {
        use Effect::*;
        assert_eq!(stdlib_call_effect("random", "int"), vec![Rand]);
        assert_eq!(runtime_symbol_effect("almide_rt_random_int"), vec![Rand]);
        assert_eq!(stdlib_call_effect("io", "read_line"), vec![IoStdin]);
        // The generic writer carries none of its own: the handle is the
        // capability, and `io.stdout()` is `IO.stdout` (R-L0-44).
        assert!(stdlib_call_effect("io", "write").is_empty());
        assert_eq!(stdlib_call_effect("io", "stdout"), vec![IoStdout]);
        assert_eq!(stdlib_call_effect("io", "eprint"), vec![IoStderr]);
        assert!(stdlib_call_effect("path", "join").is_empty());
        assert!(stdlib_call_effect("url", "parse").is_empty());
        assert_eq!(stdlib_call_effect("datetime", "now"), vec![Time]);
        assert!(stdlib_call_effect("datetime", "format").is_empty());
        assert!(runtime_symbol_effect("almide_rt_datetime_format").is_empty());
        assert_eq!(stdlib_call_effect("http", "get"), vec![NetFetch]);
        assert!(stdlib_call_effect("http", "response").is_empty());
        assert!(stdlib_call_effect("list", "map").is_empty());
        // A call the classification cannot answer for is unknown, so every
        // category — never the empty set (decision row
        // purity.effect-classification). A user module named like a stdlib
        // module (#2223) is resolved to its own body by every consumer before
        // it would ask here.
        assert_eq!(stdlib_call_effect("not_a_module", "f"), Effect::ALL.to_vec());
        assert_eq!(stdlib_call_effect("net", "f"), Effect::ALL.to_vec());
        assert_eq!(stdlib_call_effect("random", "my_helper"), Effect::ALL.to_vec());
        assert_eq!(stdlib_call_effect("fan", "map"), vec![Fan]);
        assert_eq!(runtime_symbol_effect("almide_http_get"), vec![NetFetch]);
        assert!(runtime_symbol_effect("almide_regex_find").is_empty());
        assert_eq!(runtime_symbol_effect("println"), Effect::ALL.to_vec());
        assert_eq!(runtime_symbol_effect("almide_rt_no_such_module_f"), Effect::ALL.to_vec());
        assert!(runtime_symbol_classified("almide_rt_list_map") && !runtime_symbol_classified("almide_rt___err_at"));
    }

    /// #4155: starting a process is `Proc`, never `Env`; the environment
    /// readers stay `Env.read`; files are `FS`, split read / write.
    #[test]
    fn the_leaves_4155_assigns() {
        use Effect::*;
        for f in ["exec", "output", "run", "start", "poll", "wait", "stop", "read_new_stdout", "child_pid", "kill"] {
            assert_eq!(stdlib_call_effect("process", f), vec![Proc], "process.{f}");
        }
        // argv and the variables live in `env` (R-process-01); `process.pid`
        // is the one environment reader left in `process`.
        assert_eq!(stdlib_call_effect("process", "pid"), vec![EnvRead], "process.pid");
        assert_eq!(stdlib_call_effect("env", "args"), vec![EnvRead], "env.args");
        assert_eq!(stdlib_call_effect("uuid", "v4"), vec![Rand]);
        assert_eq!(stdlib_call_effect("uuid", "v7"), vec![Time, Rand]);
        assert!(stdlib_call_effect("uuid", "parse").is_empty());
        // C-460: a call that reaches a file entry carries its FS leaf too.
        assert_eq!(stdlib_call_effect("unix", "connect"), vec![NetFetch, FsRead]);
        assert_eq!(stdlib_call_effect("unix", "listen"), vec![NetListen, FsWrite]);
        assert_eq!(stdlib_call_effect("unix", "shm_create"), vec![FsWrite]);
        assert_eq!(stdlib_call_effect("unix", "shm_write"), vec![NetFetch, NetListen]);
        assert_eq!(stdlib_call_effect("fs", "temp_dir"), vec![EnvRead]);
        assert_eq!(stdlib_call_effect("net", "lookup_host"), vec![NetFetch]);
        assert!(stdlib_call_effect("net", "parse_ip").is_empty());
        assert_eq!(stdlib_call_effect("process", "exit"), vec![Abort]);
        assert_eq!(stdlib_call_effect("env", "get"), vec![EnvRead]);
        assert_eq!(stdlib_call_effect("env", "set"), vec![EnvWrite]);
        assert_eq!(stdlib_call_effect("env", "remove"), vec![EnvWrite]);
        assert_eq!(stdlib_call_effect("env", "vars"), vec![EnvRead]);
        assert_eq!(stdlib_call_effect("fs", "read_text"), vec![FsRead]);
        assert_eq!(stdlib_call_effect("fs", "remove_all"), vec![FsWrite]);
        assert_eq!(stdlib_call_effect("fs", "copy"), vec![FsRead, FsWrite]);
        assert_eq!(stdlib_call_effect("http", "serve"), vec![NetListen]);
        assert_eq!(stdlib_call_effect("net", "tcp_connect"), vec![NetFetch]);
        assert_eq!(stdlib_call_effect("net", "tcp_accept"), vec![NetListen]);
        assert_eq!(stdlib_call_effect("io", "print"), vec![IoStdout]);
        for parent in ["FS", "Net", "IO", "Env"] {
            assert!(!Effect::granted_by(parent).unwrap().contains(&Proc), "{parent} must not grant Proc");
        }
        assert_eq!(Effect::granted_by("Env"), Some(vec![EnvRead, EnvWrite]));
        assert_eq!(Effect::granted_by("Proc"), Some(vec![Proc]));
        assert_eq!(Effect::granted_by("Process"), None);
    }

    /// #4159, the reverse of the Pure-module gate: a public plain `fn` of a
    /// module classified `All(_)` would carry the module's host category but
    /// be callable from a pure fn (E006 reads the `effect` marker). Each one
    /// is an `effect fn`, or says with an empty `@category()` that it reaches
    /// no host (`random.next_int` over a seeded state, `env.os`). And no
    /// plain fn anywhere declares a host category.
    #[test]
    fn a_public_plain_fn_of_an_effect_module_reaches_no_host() {
        for (m, row) in STDLIB_MODULE_EFFECTS {
            if *m == "fan" { continue; }
            for d in module_fns(m) {
                let almide_lang::ast::Decl::Fn { visibility, .. } = d else { continue };
                if fn_is_effect(d) { continue; }
                let declared = declared_categories(d);
                // A plain fn may declare only what a plain fn may do: the
                // leaves of the output and abort builtins (ADR-0022) — a
                // testing assertion's `Abort` and `IO.stderr` (#4951).
                let plain = plain_fn_leaves();
                assert!(declared.as_ref().is_none_or(|c| c.iter().all(|e| plain.contains(e))),
                    "`{m}.{}` is a plain fn with a host category: make it `effect fn`", fn_name(d));
                if matches!(row, ModuleEffects::All(_)) && *visibility == almide_lang::ast::Visibility::Public {
                    assert!(declared.is_some(),
                        "`{m}.{}` is a public plain fn of an `All(_)` module: make it `effect fn`, or declare `@category()` if it reaches no host", fn_name(d));
                }
            }
        }
    }

    /// Every `@category` in the stdlib names a leaf, sits in a classified
    /// module that is not `Pure`, and every `effect fn` of such a module has a
    /// category (an abort-only one has `Abort`).
    #[test]
    fn every_declared_category_is_a_leaf_and_every_effect_fn_has_one() {
        for (m, row) in STDLIB_MODULE_EFFECTS {
            if *m == "fan" { continue; }
            for d in module_fns(m) {
                let almide_lang::ast::Decl::Fn { attrs, .. } = d else { continue };
                if let Some(a) = attrs.iter().find(|a| a.name.as_str() == "category") {
                    assert!(*row != ModuleEffects::Pure, "`{m}.{}` declares @category in a Pure module", fn_name(d));
                    for x in &a.args {
                        let almide_lang::ast::AttrValue::String { value } = &x.value else {
                            panic!("`{m}.{}`: @category takes leaf names as strings", fn_name(d))
                        };
                        assert!(Effect::from_name(value).is_some(), "`{m}.{}`: @category(\"{value}\") is not a leaf", fn_name(d));
                    }
                }
                // An explicit `@category()` says "none of its own" (a router
                // dispatch only runs the handlers it is given).
                if *row != ModuleEffects::Pure && fn_is_effect(d) && declared_categories(d).is_none() {
                    assert!(!decl_effects(*row, d).is_empty(), "`{m}.{}` is an effect fn with no category", fn_name(d));
                }
            }
        }
    }
}
