//! Function-granularity reach (#4328): which functions of each bundled
//! stdlib module a program reaches, so the resolver hands the front end only
//! those. The module rules are the parent's ([`super::reached_auto_bundled`]);
//! this file decides, inside a reached module, which functions stay.

use std::collections::{HashMap, HashSet};

use almide_lang::ast::{Decl, Program, TypeExpr};

use super::{auto_modules, auto_program, fn_is_named, reaches, spell_unit, variant_case_name, Spelled, Surface};

/// The bundled stdlib a program reaches, by FUNCTION: which auto-imported
/// modules load, and, for every bundled module considered (those plus the
/// explicitly loaded ones), which of its functions are live.
pub struct LiveBundled {
    /// The auto-imported modules reached, in `AUTO_IMPORT_BUNDLED` order.
    pub auto_modules: Vec<&'static str>,
    /// Module name → the names of its live functions. A bundled module's
    /// other functions are never named by anything the program compiles, so
    /// the front end need not check or lower them.
    pub live_fns: HashMap<String, HashSet<String>>,
}

/// The bundled functions the compiler's own lowering calls without any
/// source spelling them (the derived codec's key listing, field read and
/// error text, `+` on `Bytes`, the matrix operators, the wasm emitter's
/// region-export wrapper, the optimizer's directed JSON reader and array
/// writer — stamped `@on_demand` by that pass, so a program no rewrite fired
/// in drops them after mono). A module that is loaded keeps these live.
/// `tests/compiler_inserted_calls_test.rs` scans the lowering and the wasm
/// emitter for every literal `module.__name` call they build and refuses one
/// that neither table lists.
pub const COMPILER_INSERTED: &[(&str, &str)] = &[
    ("value", "__keys"),
    ("value", "__field"),
    ("string", "__err_text"),
    ("bytes", "__concat"),
    ("matrix", "__neg"),
    ("matrix", "__div"),
    ("ffi", "__export_view"),
    ("ffi", "__lend"),
    ("json", "__ints_at"),
    ("json", "__array_text"),
];

/// The bundled functions a TEST build calls without any source spelling
/// them: a `testing.assert_aborts` site's verdict (the lowering) and the
/// test runner the driver synthesizes (`almide_driver::test_runner`: the
/// abort-site selector and the failure text). A program that declares a
/// `test` reaches them as if it named them, module included.
pub const TEST_SYNTHESIZED: &[(&str, &str)] = &[
    ("testing", "__assert_aborts_check"),
    ("testing", "__abort_site_run"),
    ("testing", "__abort_site_selected"),
    ("string", "__err_text"),
];

/// [`super::reached_auto_bundled`] at function granularity. A module still
/// loads by the module rules, but its functions start dormant: a function
/// becomes live — and its body is spelled, so what IT names is reached in
/// turn — when a loaded source names it (qualified, as a member, or bare),
/// or when the compiler calls it without any source spelling it:
///
/// - a built-in protocol requirement of a type its module owns, or a
///   convention method of such a type (`Secret.repr`): operators,
///   derivations, renderers and requirement dispatch call those;
/// - the fallible carrier `__fallible_<f>` of a named HOF `f` (ADR-0006);
/// - a row of [`COMPILER_INSERTED`], or of [`TEST_SYNTHESIZED`] when the
///   program declares a test;
/// - a call made by a self-hosted body the wasm leg links behind a reached
///   surface (`self_host_registry`) — a body `admit` accepts: the wasm
///   emitter calls an implementation through its surface only when its
///   whitelist admits it, and links no other (#4458).
///
/// What the wasm emitter still finds missing after this, the wasm link
/// relowers with the missing functions and what they reach in the module
/// (`src/wasm_leg.rs`, `link_self_host`).
///
/// `roots` are the sources compiled whole (the entry program and the user's
/// modules); `explicit` are the bundled modules already loaded by an
/// `import` (or by another bundled module's `import`): they count as reached
/// from the start, and their functions are pruned by the same rule.
pub fn live_bundled<'a>(
    roots: impl IntoIterator<Item = &'a Program>,
    explicit: &[(&'a str, &'a Program)],
    admit: &dyn Fn(&str) -> bool,
) -> LiveBundled {
    let mut spelled = Spelled::default();
    let mut has_tests = false;
    for p in roots {
        has_tests |= p.decls.iter().any(|d| matches!(d, Decl::Test { .. }));
        spell_unit(p, p.decls.iter().chain(p.imports.iter()), &mut spelled);
    }
    if has_tests {
        spelled.module_calls.extend(TEST_SYNTHESIZED.iter().map(|(m, f)| (m.to_string(), f.to_string())));
    }
    let explicit_surfaces: Vec<Surface> = explicit.iter().map(|(_, p)| surface_of(p)).collect();
    let mut cands: Vec<Cand<'_>> = explicit
        .iter()
        .zip(&explicit_surfaces)
        .map(|((n, p), s)| Cand { name: n, prog: Some(p), surface: s, auto: false })
        .collect();
    for (n, s) in auto_modules() {
        if !explicit.iter().any(|(e, _)| e == n) {
            cands.push(Cand { name: n, prog: None, surface: s, auto: true });
        }
    }
    let rows = almide_lang::self_host_registry::self_host_runtime();
    // The whitelist filter is a reach pruning too: with the pruning off every
    // self-host body behind a reached surface is entered.
    let admit_all = |_: &str| true;
    let admit: &dyn Fn(&str) -> bool = if almide_base::env::keep_bundled_fns() { &admit_all } else { admit };
    let mut reach = Reach {
        reached: cands.iter().map(|c| !c.auto).collect(),
        entered: vec![false; cands.len()],
        host_entered: vec![false; rows.len()],
        cands,
        rows,
        spelled,
        live: HashMap::new(),
        dormant: Vec::new(),
        host_dormant: Vec::new(),
        admit,
    };
    while reach.enter_modules() | reach.wake_dormant() | reach.enter_self_hosted() | reach.wake_self_hosted() {}
    let auto_modules = auto_modules()
        .iter()
        .map(|(n, ..)| *n)
        .filter(|n| reach.cands.iter().zip(&reach.reached).any(|(c, r)| *r && c.auto && c.name == *n))
        .collect();
    LiveBundled { auto_modules, live_fns: reach.live }
}

/// A bundled module the program may reach: an auto-imported one, or one an
/// `import` already loaded (which wins over the auto entry of its name).
struct Cand<'p> {
    name: &'p str,
    /// `None` for an auto-imported module: parsed once it is entered (#4458).
    prog: Option<&'p Program>,
    surface: &'p Surface,
    auto: bool,
}

/// The fixpoint's state. Every step returns whether it grew anything; the
/// loop runs until none does.
struct Reach<'p> {
    cands: Vec<Cand<'p>>,
    reached: Vec<bool>,
    entered: Vec<bool>,
    spelled: Spelled,
    live: HashMap<String, HashSet<String>>,
    /// Functions of entered modules nothing has reached yet.
    dormant: Vec<(usize, &'p Decl)>,
    /// The self-hosted bodies (`self_host_registry`) the wasm leg links
    /// behind a stdlib surface call into bundled modules too (`json.parse`'s
    /// body calls `string.from_codepoint`). A source is parsed and entered
    /// once one of its surfaces is reached; its bodies then start dormant like
    /// a module's.
    rows: &'static [almide_lang::self_host_registry::SelfHostRow],
    host_entered: Vec<bool>,
    host_dormant: Vec<(usize, &'static Decl)>,
    /// Which implementations the wasm leg can link (`live_bundled`'s doc).
    admit: &'p dyn Fn(&str) -> bool,
}

impl<'p> Reach<'p> {
    /// Enter every module the spelled names now reach: spell its non-fn
    /// declarations, make the functions the compiler calls on its own live,
    /// and leave the rest dormant.
    fn enter_modules(&mut self) -> bool {
        let mut grew = false;
        for i in 0..self.cands.len() {
            let c = &self.cands[i];
            if !self.reached[i] && reaches(&self.spelled, c.name, c.surface) {
                self.reached[i] = true;
            }
            if !self.reached[i] || self.entered[i] {
                continue;
            }
            self.entered[i] = true;
            grew = true;
            let name = c.name;
            let prog: &'p Program = match c.prog {
                Some(p) => p,
                None => auto_program(name),
            };
            self.cands[i].prog = Some(prog);
            self.live.entry(name.to_string()).or_default();
            let (fns, rest): (Vec<&'p Decl>, Vec<&'p Decl>) =
                prog.decls.iter().chain(prog.imports.iter()).partition(|d| matches!(d, Decl::Fn { .. }));
            spell_unit(prog, rest.into_iter(), &mut self.spelled);
            let mut required = requirement_fns(name, prog);
            required.extend(COMPILER_INSERTED.iter().filter(|(m, _)| *m == name).map(|(_, f)| f.to_string()));
            for d in fns {
                if decl_name(d).is_some_and(|n| required.contains(n)) {
                    self.make_live(i, d);
                } else {
                    self.dormant.push((i, d));
                }
            }
        }
        grew
    }

    /// Make every dormant function that something now names live.
    fn wake_dormant(&mut self) -> bool {
        let dormant = std::mem::take(&mut self.dormant);
        let (named, rest): (Vec<_>, Vec<_>) = dormant.into_iter().partition(|(i, d)| self.is_reached(*i, d));
        self.dormant = rest;
        let grew = !named.is_empty();
        for (i, d) in named {
            self.make_live(i, d);
        }
        grew
    }

    /// Is dormant function `d` of candidate `i` reached: named by a loaded
    /// source, or the fallible carrier of a HOF that is? The HOF itself may
    /// have no declaration of its own (`list.each` lowers inline), so its
    /// carrier is reached whenever the HOF is named.
    fn is_reached(&self, i: usize, d: &Decl) -> bool {
        let m = self.cands[i].name;
        fn_is_named(&self.spelled, m, d)
            || decl_name(d).and_then(|n| n.strip_prefix("__fallible_")).is_some_and(|h| {
                almide_lang::fallible_hofs::is_fallible_hof(m, h)
                    && (self.live.get(m).is_some_and(|fs| fs.contains(h)) || hof_is_named(&self.spelled, m, h))
            })
    }

    fn make_live(&mut self, i: usize, d: &'p Decl) {
        let c = &self.cands[i];
        if let Some(n) = decl_name(d) {
            self.live.get_mut(c.name).expect("an entered module").insert(n.to_string());
        }
        spell_unit(c.prog.expect("an entered module is parsed"), std::iter::once(d), &mut self.spelled);
    }

    /// Enter every self-hosted source one of whose surfaces is now reached.
    fn enter_self_hosted(&mut self) -> bool {
        let mut grew = false;
        for (si, (src, impls)) in self.rows.iter().enumerate() {
            if self.host_entered[si]
                || !impls.iter().any(|(f, s)| (self.admit)(f) && surface_is_named(&self.spelled, &self.live, s))
            {
                continue;
            }
            self.host_entered[si] = true;
            grew = true;
            let Some(prog) = almide_lang::parse_cached(src) else { continue };
            let (fns, rest): (Vec<&'static Decl>, Vec<&'static Decl>) =
                prog.decls.iter().chain(prog.imports.iter()).partition(|d| matches!(d, Decl::Fn { .. }));
            spell_unit(prog, rest.into_iter(), &mut self.spelled);
            self.host_dormant.extend(fns.into_iter().map(|d| (si, d)));
        }
        grew
    }

    /// Spell every self-hosted body now reached: an impl whose surface is,
    /// or a helper a live body names.
    fn wake_self_hosted(&mut self) -> bool {
        let dormant = std::mem::take(&mut self.host_dormant);
        let (named, rest): (Vec<_>, Vec<_>) = dormant.into_iter().partition(|(si, d)| {
            let impl_live = self.rows[*si].1.iter().any(|(imp, s)| {
                decl_name(d) == Some(*imp) && (self.admit)(imp) && surface_is_named(&self.spelled, &self.live, s)
            });
            impl_live || fn_is_named(&self.spelled, "", d)
        });
        self.host_dormant = rest;
        let grew = !named.is_empty();
        for (si, d) in named {
            if let Some(prog) = almide_lang::parse_cached(self.rows[si].0) {
                spell_unit(prog, std::iter::once(d), &mut self.spelled);
            }
        }
        grew
    }
}

/// The functions of one source — bundled module `module`, or a self-hosted
/// source (`module` empty) — that `roots` reach WITHIN it: a root, or a
/// function a reached body (or a non-function declaration) names, by the
/// naming rule of [`live_bundled`]. The wasm link loads only these (#4458);
/// a call the lowered bodies make outside this set is a demand of its next
/// round, which grows the set. With the reach pruning switched off
/// ([`almide_base::env::keep_bundled_fns`]) it is every function of `prog`.
pub fn reach_within(module: &str, prog: &Program, roots: &HashSet<String>) -> HashSet<String> {
    if almide_base::env::keep_bundled_fns() {
        return prog.decls.iter().filter(|d| matches!(d, Decl::Fn { .. })).filter_map(decl_name).map(str::to_string).collect();
    }
    let mut spelled = Spelled::default();
    let (fns, rest): (Vec<&Decl>, Vec<&Decl>) =
        prog.decls.iter().chain(prog.imports.iter()).partition(|d| matches!(d, Decl::Fn { .. }));
    spell_unit(prog, rest.into_iter(), &mut spelled);
    let mut live = HashSet::new();
    let (now, mut dormant): (Vec<&Decl>, Vec<&Decl>) =
        fns.into_iter().partition(|d| decl_name(d).is_some_and(|n| roots.contains(n)));
    let mut named = now;
    while !named.is_empty() {
        for d in &named {
            live.extend(decl_name(d).map(str::to_string));
        }
        spell_unit(prog, named.into_iter(), &mut spelled);
        (named, dormant) = dormant.into_iter().partition(|d| fn_is_named(&spelled, module, d));
    }
    live
}

/// The functions of bundled module `module` the compiler calls with no
/// source naming them: [`requirement_fns`] and its [`COMPILER_INSERTED`]
/// rows. A module the wasm link loads keeps them, as a resolved one does.
pub fn compiler_called_fns(module: &str, prog: &Program) -> HashSet<String> {
    let mut out = requirement_fns(module, prog);
    out.extend(COMPILER_INSERTED.iter().filter(|(m, _)| *m == module).map(|(_, f)| f.to_string()));
    out
}

fn decl_name(d: &Decl) -> Option<&str> {
    match d {
        Decl::Fn { name, .. } => Some(name.as_str()),
        _ => None,
    }
}

/// Is stdlib surface `m.f` reached? For a bundled module, exactly when its
/// declaration `f` is live (the naming rules already ran on it); for a
/// runtime-backed module (`fs`, `env`), when a loaded source names it
/// qualified, or names `f` as a member while `m` itself is spelled.
fn surface_is_named(spelled: &Spelled, live: &HashMap<String, HashSet<String>>, surface: &str) -> bool {
    let Some((m, f)) = surface.rsplit_once('.') else { return false };
    if almide_lang::stdlib_info::bundled_source(m).is_some() {
        return live.get(m).is_some_and(|fs| fs.contains(f));
    }
    spelled.module_calls.contains(&(m.to_string(), f.to_string()))
        || (spelled.members.contains(f) && spelled.module_calls.iter().any(|(a, _)| a == m))
}

/// Is HOF `h` of module `m` named by a loaded source — qualified, as a
/// member, or bare?
fn hof_is_named(spelled: &Spelled, m: &str, h: &str) -> bool {
    spelled.module_calls.contains(&(m.to_string(), h.to_string()))
        || spelled.members.contains(h)
        || spelled.idents.contains(h)
}

/// The surface (function, type and constructor names) `prog` declares.
pub(super) fn surface_of(prog: &Program) -> Surface {
    let mut fns = HashSet::new();
    let mut types = HashSet::new();
    for d in &prog.decls {
        match d {
            Decl::Fn { name, .. } => {
                fns.insert(name.to_string());
            }
            Decl::Type { name, ty, .. } => {
                types.insert(name.to_string());
                if let TypeExpr::Variant { cases, .. } = ty {
                    types.extend(cases.iter().map(|c| variant_case_name(c).to_string()));
                }
            }
            _ => {}
        }
    }
    Surface { fns, types }
}

/// The functions of bundled module `module` the compiler calls for a type
/// the module owns: a built-in protocol requirement of a built-in conformer
/// whose stdlib module it is (`int` for `Int`), of a runtime-backed type it
/// owns (`HttpRequest`) or of a type it declares conforming (`type BigInt:
/// Numeric`), read off the protocol table; and a convention method of a
/// type (`mod fn Secret.repr`), which every renderer and requirement
/// dispatch reaches through the TYPE, never by a spelled call.
fn requirement_fns(module: &str, prog: &Program) -> HashSet<String> {
    use crate::canonicalize::protocols::{builtin_protocol, BUILTIN_PROTOCOLS};
    use almide_lang::stdlib_info::{requirement_fn_name, stdlib_owned_type_owner};
    let mut out = HashSet::new();
    for p in BUILTIN_PROTOCOLS {
        let owned = p.conformers.iter().filter(|t| t.stdlib_module() == Some(module)).map(|t| t.display()).chain(
            p.named_conformers.iter().filter(|n| stdlib_owned_type_owner(n) == Some(module)).map(|n| n.to_string()),
        );
        for t in owned {
            out.extend(p.method_names().iter().map(|m| requirement_fn_name(&t, m)));
        }
    }
    for d in &prog.decls {
        match d {
            Decl::Fn { name, .. } if name.as_str().contains('.') => {
                out.insert(name.to_string());
            }
            Decl::Type { name, deriving, .. } => {
                for p in deriving.iter().flatten().filter_map(|proto| builtin_protocol(proto.as_str())) {
                    out.extend(p.method_names().iter().map(|m| requirement_fn_name(name.as_str(), m)));
                }
            }
            _ => {}
        }
    }
    out
}
