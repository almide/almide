//! Canonical type expression resolution.
//!
//! Single source of truth for converting `ast::TypeExpr` → `Ty`.
//! Used by both the checker (with type lookup) and lowering (without).

use std::collections::HashMap;
use almide_lang::ast;
use crate::types::{Ty, TypeConstructorId, VariantCase, VariantPayload};
use almide_base::intern::{Sym, sym};

/// Resolve an AST type expression to a Ty.
///
/// `known_types`: optional map of registered type names → Ty (from TypeEnv.types).
/// When provided (checker context), named types are looked up; when None (lowering),
/// unresolved names become `Ty::Named`.
pub fn resolve_type_expr(te: &ast::TypeExpr, known_types: Option<&HashMap<Sym, Ty>>) -> Ty {
    resolve_type_expr_in(te, known_types, None)
}

/// The key under which a file's bare spelling of an IMPORTED module's type is
/// recorded (#2715): `<in-scope:m>|Name` for module `m`, `<in-scope:>|Name`
/// for the entry program. No source can spell it, and it has no `.Name`
/// suffix, so no scan for a module's `m.Name` key ever matches it.
pub fn scoped_bare_type_key(scope: Option<&str>, name: &str) -> Sym {
    sym(&format!("<in-scope:{}>|{}", scope.unwrap_or(""), name))
}

/// The key under which a GENERIC type declaration's parameter list is
/// recorded (#3403): `<type-params:Pair>` / `<type-params:m.Pair>` holds
/// `Ty::Tuple` of the declared letters, in declaration order, beside the
/// declaration's own `types` entry. A transparent alias substitutes its
/// arguments for exactly these letters, and the arity check counts them. No
/// source can spell the key, and it has no `.Name` suffix, so no scan for a
/// module's `m.Name` key ever matches it.
pub fn type_params_key(type_key: &str) -> Sym {
    sym(&format!("<type-params:{}>", type_key))
}

/// The declared parameter letters of the generic type registered under
/// `type_key`, when it has any.
pub fn declared_type_params<'t>(type_key: &str, types: &'t HashMap<Sym, Ty>) -> Option<&'t [Ty]> {
    match types.get(&type_params_key(type_key))? {
        Ty::Tuple(ps) => Some(ps.as_slice()),
        _ => None,
    }
}

/// The key recording that the file `scope` (`None` = the entry program)
/// declares a type under the bare name of a builtin (#2858): `type Int = ..`
/// in a package's `src/int.almd`, `type Path = ..` in `main.almd`. A file's
/// own declaration answers its bare spelling (module-system §4.5, "自分で
/// 宣言した型"), so inside that file `Int` is the declared type in every
/// position — a signature, a field, a record literal alike. Every OTHER file
/// keeps the builtin for the bare spelling and reaches the type qualified
/// (`int.Int`). Only a bare spelling is claimed: `Map[K, V]` applied to the
/// builtin's arity stays the builtin (#2839). No source can spell the key.
pub fn builtin_named_type_key(scope: Option<&str>, name: &str) -> Sym {
    sym(&format!("<builtin-named-decl:{}>|{}", scope.unwrap_or(""), name))
}

/// Record [`builtin_named_type_key`] for each type `decls` declares under a
/// builtin's bare name, before any declaration body or signature resolves —
/// declaration order is not meaningful (#2645).
pub fn register_builtin_named_type_keys(env: &mut crate::types::TypeEnv, decls: &[ast::Decl], scope: Option<&str>) {
    for decl in decls {
        if let ast::Decl::Type { name, .. } = decl
            && builtin_type_head(name.as_str(), TypeSpelling::Bare).is_some()
        {
            env.types.insert(builtin_named_type_key(scope, name.as_str()), Ty::Named(*name, vec![]));
        }
    }
}

fn declares_builtin_named_type(known_types: Option<&HashMap<Sym, Ty>>, cur_mod: Option<&str>, name: &str) -> bool {
    known_types.is_some_and(|types| types.contains_key(&builtin_named_type_key(cur_mod, name)))
}

/// Record, for the file `scope` whose import table is `env.import_table`,
/// which module's type each bare type name means when exactly ONE module the
/// file imports declares it (#2715). The bare-name fallback consults this
/// before its program-wide "unique owner" scan, so the answer is the
/// imported module's type however many other modules — imported by someone
/// else, registered in any order — declare the same name. Keys are per file
/// and persist, so registration, the checker and lowering all read the same
/// answer for the same file.
pub fn register_scoped_bare_type_keys(env: &mut crate::types::TypeEnv, scope: Option<&str>) {
    let visible: std::collections::HashSet<&str> = env.import_table.accessible.iter()
        .chain(env.import_table.aliases.values())
        .map(|m| m.as_str())
        .filter(|m| !almide_lang::stdlib_info::is_bundled_module(m) && Some(*m) != scope)
        .collect();
    if visible.is_empty() {
        return;
    }
    // One scan of the type table, not one per imported module: a key
    // `m.Name` belongs to module `m` exactly when `m` is its prefix before
    // the LAST dot.
    let mut owners: HashMap<String, Vec<Sym>> = HashMap::new();
    for (k, v) in &env.types {
        if !matches!(v, Ty::Record { .. } | Ty::Variant { .. }) {
            continue;
        }
        let Some((m, rest)) = k.as_str().rsplit_once('.') else { continue };
        if visible.contains(m) {
            owners.entry(rest.to_string()).or_default().push(*k);
        }
    }
    for (name, ks) in owners {
        if let [only] = ks.as_slice() {
            env.types.insert(scoped_bare_type_key(scope, &name), Ty::Named(*only, vec![]));
        }
    }
    // The file's ALIAS spellings too (#2877): `import self.boxes as bx` makes
    // `bx.Box` mean `boxes.Box` in THIS file. `register_alias_type_keys`
    // mirrors aliases program-wide only while one file is being checked, so
    // a module's own alias spelling was unknown when its declarations were
    // REGISTERED (a `var n: bx.Box[Int]` recorded the bare `Box`) and when
    // it was LOWERED (unless the entry happened to bind the same alias).
    // Scoped like the bare keys, so every resolver reads one answer per file.
    let mut alias_adds: Vec<(Sym, Ty)> = Vec::new();
    for (alias, canonical) in &env.import_table.aliases {
        if alias == canonical || almide_lang::stdlib_info::is_bundled_module(canonical.as_str()) {
            continue;
        }
        let prefix = format!("{}.", canonical.as_str());
        for (k, v) in &env.types {
            if !matches!(v, Ty::Record { .. } | Ty::Variant { .. }) {
                continue;
            }
            let Some(rest) = k.as_str().strip_prefix(&prefix) else { continue };
            if rest.contains('.') {
                continue;
            }
            let spelled = format!("{}.{}", alias.as_str(), rest);
            alias_adds.push((scoped_bare_type_key(scope, &spelled), Ty::Named(*k, vec![])));
        }
    }
    for (k, v) in alias_adds {
        env.types.insert(k, v);
    }
}

/// Mirror a dependency module's nominal type keys under every import-ALIAS
/// spelling the current file can write for it (#1955): `import dep.shape as
/// sh` (and the implicit last-segment alias `shape`) gets `sh.Box` /
/// `shape.Box` → `Ty::Named("dep.shape.Box")` indirections, so every
/// type-expression resolver — registration, checker, lowering — finds the
/// declaration through `canonical_user_type_sym_qualified` without carrying
/// the import table. Only the module's OWN types (no submodule segment);
/// a real key under the alias spelling is never overwritten.
pub fn register_alias_type_keys(env: &mut crate::types::TypeEnv) {
    let mut adds: Vec<(Sym, Ty)> = Vec::new();
    for (alias, canonical) in &env.import_table.aliases {
        if alias == canonical {
            continue;
        }
        let prefix = format!("{}.", canonical.as_str());
        for (k, v) in &env.types {
            if !matches!(v, Ty::Record { .. } | Ty::Variant { .. }) {
                continue;
            }
            let Some(rest) = k.as_str().strip_prefix(&prefix) else { continue };
            if rest.contains('.') {
                continue;
            }
            adds.push((sym(&format!("{}.{}", alias.as_str(), rest)), Ty::Named(*k, vec![])));
        }
    }
    for (k, v) in adds {
        env.types.entry(k).or_insert(v);
    }
}

/// The identity scope of an ENTRY-program declaration that shadows a
/// stdlib-owned type name (#1828): `type Value = { n: Int }` in the main
/// file is `self.Value`, the way a module `m`'s is `m.Value`. Defined
/// beside the owned-type registry so the display side (`repr` on both
/// backends) strips the same scope.
pub use almide_lang::stdlib_info::ROOT_TYPE_SCOPE;

/// The canonical `env.types` key a user declaration of a STDLIB-OWNED type
/// name takes in `cur_mod` (#1828): `self.Value` for the entry program,
/// `m.Value` inside user module `m`. The stdlib keeps the bare key — that
/// bare spelling is the identity every stdlib signature and both runtimes
/// carry — so the user's declaration never rebinds it. `None` for every
/// other name, for a qualified spelling, and inside a bundled module (a
/// stdlib body's bare `Value` IS the stdlib's).
pub fn stdlib_shadow_key(name: &str, cur_mod: Option<&str>) -> Option<String> {
    if name.contains('.') || almide_lang::stdlib_info::stdlib_owned_type_owner(name).is_none() {
        return None;
    }
    let scope = match cur_mod {
        Some(m) if almide_lang::stdlib_info::is_bundled_module(m) => return None,
        Some(m) => m,
        None => ROOT_TYPE_SCOPE,
    };
    Some(format!("{}.{}", scope, name))
}

/// The `stdlib_shadow_key` entry when the program declares one, whatever its
/// shape (record, variant, alias) — the user's declaration under that name.
fn stdlib_shadow_entry<'t>(name: &str, types: &'t HashMap<Sym, Ty>, cur_mod: Option<&str>) -> Option<(Sym, &'t Ty)> {
    let key = sym(&stdlib_shadow_key(name, cur_mod)?);
    types.get(&key).map(|t| (key, t))
}

/// Resolve a nominal type NAME to its canonical (possibly module-qualified)
/// Sym, per the #433 rules. THE single place this qualification predicate
/// lives — annotations (via `resolve_type_expr_in`) and the checker's record
/// construction inference both call it, so the producers cannot diverge.
///
/// - A user module's bare reference to its own declared type → `mod.Type`.
/// - An already-qualified reference to a USER module's type → kept qualified.
/// - A bare reference to an IMPORTED user-module type (e.g. module `b` uses
///   module `d`'s `Logger` as a bare `Logger` brought in by `import d`) →
///   the unique owner's `X.Type` key, so it mangles to the same struct. Only
///   when EXACTLY ONE user module declares that bare name — otherwise it is
///   ambiguous and stays bare (a root-local type, which has no `X.Type` key,
///   also falls through to bare).
/// - Stdlib / local / unknown names → None (stay bare).
pub fn canonical_user_type_sym(name: &str, types: &HashMap<Sym, Ty>, cur_mod: Option<&str>) -> Option<Sym> {
    // A name the CURRENT resolution scope binds as a type variable (a
    // generic letter shadowed in by an overlay or registration scope) is a
    // bound variable, never a reference to a module's nominal type —
    // `fn go[Q](c: Q)` must not resolve `Q` to an imported `infra.Q`
    // (#1577). Without this the qualified canonicalization outranked the
    // TypeVar binding, mono saw a nominal param with no type var anywhere,
    // and the call site dangled unspecialized on both targets.
    if matches!(types.get(&sym(name)), Some(Ty::TypeVar(_) | Ty::ConstParam { .. })) {
        return None;
    }
    // A user declaration of a stdlib-owned name lives under its shadow key
    // (#1828); the bare key is the stdlib's and is never consulted for it.
    if let Some((key, Ty::Record { .. } | Ty::Variant { .. })) = stdlib_shadow_entry(name, types, cur_mod) {
        return Some(key);
    }
    if let Some(own) = canonical_user_type_sym_own_module(name, types, cur_mod) {
        return Some(own);
    }
    // The module declares the name itself as an ALIAS (`type Ctx =
    // List[String]`): its own declaration is the answer, and it is not
    // nominal, so no `X.Type` key may stand in for it. Without this the
    // unique-owner fallback below handed a module's bare `Ctx` to ANOTHER
    // module's same-named record (#3401).
    if own_module_alias(name, types, cur_mod).is_some() {
        return None;
    }
    canonical_user_type_sym_scoped_alias(name, types, cur_mod)
        .or_else(|| canonical_user_type_sym_qualified(name, types))
        .or_else(|| canonical_user_type_sym_sibling(name, types, cur_mod))
        .or_else(|| canonical_user_type_sym_bare(name, types, cur_mod))
}

/// The record a record LITERAL builds when its head names a transparent
/// alias of a nominal record (#3153): `type T = state.T` registers `term.T`
/// as `Ty::Named("state.T")`, so `term.T { n: 3 }` builds a `state.T`, the
/// type annotations and field access already read through the alias. Follows
/// non-generic alias links to the record they name; any other key is
/// returned unchanged.
pub fn follow_record_alias(key: Sym, types: &HashMap<Sym, Ty>) -> Sym {
    let mut cur = key;
    for _ in 0..8 {
        match types.get(&cur) {
            Some(Ty::Named(next, args)) if args.is_empty() && *next != cur => cur = *next,
            _ => break,
        }
    }
    match types.get(&cur) {
        Some(Ty::Record { .. } | Ty::OpenRecord { .. }) if cur != key => cur,
        _ => key,
    }
}

// A SIBLING submodule's type, referenced by the short module name the source
// actually writes: `domain.Span` inside `collidelib.wire` is
// `collidelib.domain.Span`.
//
// Inside a package reached by `import self.x` the two spellings coincide, so
// this only matters once that package is itself a DEPENDENCY and every module
// carries the package prefix. Without it the qualified reference fell through
// to the bare fallback below, which strips the module and finds the
// REFERENCING file's own same-named type — silently, since a structurally
// compatible type still type-checks (#1094).
fn canonical_user_type_sym_sibling(name: &str, types: &HashMap<Sym, Ty>, cur_mod: Option<&str>) -> Option<Sym> {
    let pkg = cur_mod?.split('.').next()?;
    if !name.contains('.') || almide_lang::stdlib_info::is_bundled_module(pkg) {
        return None;
    }
    let qual = sym(&format!("{}.{}", pkg, name));
    types
        .get(&qual)
        .is_some_and(|t| matches!(t, Ty::Record { .. } | Ty::Variant { .. }))
        .then_some(qual)
}

// A user module's own bare reference to its own declared type → `mod.Type`.
fn canonical_user_type_sym_own_module(name: &str, types: &HashMap<Sym, Ty>, cur_mod: Option<&str>) -> Option<Sym> {
    let m = cur_mod?;
    if !name.contains('.') && !almide_lang::stdlib_info::is_bundled_module(m) {
        let qual = format!("{}.{}", m, name);
        if let Some(t) = types.get(&sym(&qual)) {
            if matches!(t, Ty::Record { .. } | Ty::Variant { .. }) {
                return Some(sym(&qual));
            }
        }
    }
    None
}

// A user module's own bare reference to a type ALIAS it declares (#3401):
// the alias target registered under `mod.Name`. A record or variant is
// answered by `canonical_user_type_sym_own_module`; a generic letter in scope
// is a bound variable, not this declaration.
fn own_module_alias<'t>(name: &str, types: &'t HashMap<Sym, Ty>, cur_mod: Option<&str>) -> Option<&'t Ty> {
    let m = cur_mod?;
    if name.contains('.') || almide_lang::stdlib_info::is_bundled_module(m) {
        return None;
    }
    if matches!(types.get(&sym(name)), Some(Ty::TypeVar(_) | Ty::ConstParam { .. })) {
        return None;
    }
    match types.get(&sym(&format!("{}.{}", m, name)))? {
        Ty::Record { .. } | Ty::Variant { .. } | Ty::TypeVar(_) | Ty::ConstParam { .. } => None,
        t => Some(t),
    }
}

// A qualified spelling through THIS file's import alias (`bx.Box` under
// `import self.boxes as bx`) → the aliased module's type, as recorded by
// `register_scoped_bare_type_keys` (#2877).
fn canonical_user_type_sym_scoped_alias(name: &str, types: &HashMap<Sym, Ty>, cur_mod: Option<&str>) -> Option<Sym> {
    if !name.contains('.') {
        return None;
    }
    match types.get(&scoped_bare_type_key(cur_mod, name)) {
        Some(Ty::Named(k, args)) if args.is_empty() => Some(*k),
        _ => None,
    }
}

// An already-qualified reference to a USER module's type → kept qualified.
fn canonical_user_type_sym_qualified(name: &str, types: &HashMap<Sym, Ty>) -> Option<Sym> {
    let (m, _bare) = name.rsplit_once('.')?;
    if almide_lang::stdlib_info::is_bundled_module(m) {
        return None;
    }
    match types.get(&sym(name)) {
        Some(Ty::Record { .. } | Ty::Variant { .. }) => return Some(sym(name)),
        // An import-alias spelling registered by `register_alias_type_keys`:
        // follow the indirection to the canonical key.
        Some(Ty::Named(k, args)) if args.is_empty()
            && matches!(types.get(k), Some(Ty::Record { .. } | Ty::Variant { .. })) => return Some(*k),
        _ => {}
    }
    // The source writes a DEPENDENCY module by its short spelling
    // (`shape.Box` for `import dep.shape`) while the table keys the type
    // under the package-prefixed canonical (`dep.shape.Box`). An explicitly
    // qualified reference must never fall through to the bare fallback: with
    // a same-named LOCAL type that fallback answered the local one, so
    // `fn take(b: shape.Box)` was checked as the entry program's `Box`
    // (#1955). Resolve by unique dotted suffix instead.
    let suffix = format!(".{}", name);
    let mut owners = types.iter().filter(|(k, v)| {
        k.as_str().ends_with(&suffix) && matches!(v, Ty::Record { .. } | Ty::Variant { .. })
    });
    let first = owners.next()?;
    if owners.next().is_none() {
        return Some(*first.0);
    }
    None
}

// A bare reference to an IMPORTED user-module type (the unique owner's
// `X.Type` key), or a genuine LOCAL type that shadows a same-name
// dependency type (#433).
fn canonical_user_type_sym_bare(name: &str, types: &HashMap<Sym, Ty>, cur_mod: Option<&str>) -> Option<Sym> {
    if name.contains('.') { return None; }
    // A LOCAL (main-program, unprefixed) type registered under the bare name
    // shadows a dependency's same-name type for unqualified use (#433). Prefer
    // the bare entry when it is structurally DISTINCT from every qualified
    // `<pkg>.name` owner — i.e. it is a genuine local type, not merely the
    // dependency's bare alias (which mirrors its qualified entry exactly).
    // Only for the main program (`cur_mod` is None); a user module's own types
    // are already qualified and handled by `canonical_user_type_sym_own_module`.
    // A `<mod>.name` key is a USER module's type when `<mod>` is neither a
    // bundled module nor the entry program's own shadow scope (#1828): the
    // entry program's declarations are not importable, so they are never a
    // bare reference's owner from anywhere else.
    let user_module_owner = |k: &Sym| {
        k.as_str().rsplit_once('.').map_or(false, |(p, base)| {
            base == name && p != ROOT_TYPE_SCOPE && !almide_lang::stdlib_info::is_bundled_module(p)
        })
    };
    if cur_mod.is_none() {
        if let Some(bare) = types.get(&sym(name)) {
            let is_alias_of_a_qualified = || types.iter().any(|(k, v)| user_module_owner(k) && v == bare);
            match bare {
                Ty::Record { .. } | Ty::Variant { .. } if !is_alias_of_a_qualified() => return Some(sym(name)),
                // The entry program's own ALIAS of the name (#3401): not
                // nominal, so it has no key to answer with, but it shadows an
                // imported module's same-named record just as a local record
                // does — the bare lookup after this returns it.
                Ty::Record { .. } | Ty::Variant { .. } | Ty::TypeVar(_) | Ty::ConstParam { .. } | Ty::Named(..) => {}
                _ if !is_alias_of_a_qualified() => return None,
                _ => {}
            }
        }
    }
    // A bundled module's body never references a user type: its bare names
    // are its own or the stdlib's, so the unique-owner fallback must not
    // hand it a user module's same-named type.
    if cur_mod.is_some_and(almide_lang::stdlib_info::is_bundled_module) {
        return None;
    }
    // The one module THIS file imports that declares the name (#2715), when
    // the file's scope has been recorded — never another module's type that
    // happens to be unique program-wide or to have registered last.
    if let Some(Ty::Named(k, _)) = types.get(&scoped_bare_type_key(cur_mod, name)) {
        return Some(*k);
    }
    let mut owners = types.iter().filter(|(k, v)| {
        user_module_owner(k) && matches!(v, Ty::Record { .. } | Ty::Variant { .. })
    });
    if let Some((k, _)) = owners.next() {
        if owners.next().is_none() {
            return Some(*k);
        }
    }
    None
}

/// Like `resolve_type_expr`, but aware of the module currently being resolved
/// (`cur_mod`), so a USER module's reference to one of its own types is pinned to
/// the qualified canonical name `mod.Type` instead of the bare name. This is what
/// keeps two packages' same-name types distinct end-to-end (#433). Stdlib modules
/// are exempt — their types stay bare to match the bare-named Rust runtime.
pub fn resolve_type_expr_in(te: &ast::TypeExpr, known_types: Option<&HashMap<Sym, Ty>>, cur_mod: Option<&str>) -> Ty {
    match te {
        ast::TypeExpr::Simple { name } => match builtin_type_head(name.as_str(), TypeSpelling::Bare) {
            // The file declares a type under this builtin's bare name: in
            // that file the bare spelling is its own type (#2858).
            Some(_) if declares_builtin_named_type(known_types, cur_mod, name.as_str()) => {
                resolve_simple_type_other(name.as_str(), known_types, cur_mod)
            }
            Some(head) => (head.build)(&[]),
            None => resolve_simple_type_other(name.as_str(), known_types, cur_mod),
        },
        ast::TypeExpr::Generic { name, args } => {
            let ra: Vec<Ty> = args.iter().map(|a| resolve_type_expr_in(a, known_types, cur_mod)).collect();
            match builtin_type_head(name.as_str(), TypeSpelling::Applied(args.len())) {
                Some(head) => (head.build)(&ra),
                None => resolve_nominal_generic_type_expr(name, ra, known_types, cur_mod),
            }
        },
        ast::TypeExpr::Record { fields } => Ty::Record {
            fields: fields.iter().map(|f| (sym(&f.name), resolve_type_expr_in(&f.ty, known_types, cur_mod))).collect(),
        },
        ast::TypeExpr::OpenRecord { fields } => Ty::OpenRecord {
            fields: fields.iter().map(|f| (sym(&f.name), resolve_type_expr_in(&f.ty, known_types, cur_mod))).collect(),
        },
        ast::TypeExpr::Fn { is_effect, params, ret } => Ty::Fn {
            is_effect: *is_effect,
            params: params.iter().map(|p| resolve_type_expr_in(p, known_types, cur_mod)).collect(),
            ret: Box::new(resolve_type_expr_in(ret, known_types, cur_mod)),
        },
        ast::TypeExpr::Tuple { elements } => Ty::Tuple(
            elements.iter().map(|e| resolve_type_expr_in(e, known_types, cur_mod)).collect(),
        ),
        ast::TypeExpr::Union { members } => Ty::union(
            members.iter().map(|m| resolve_type_expr_in(m, known_types, cur_mod)).collect(),
        ),
        ast::TypeExpr::ConstLit { value } => Ty::ConstValue { ty: Box::new(Ty::Int), value: *value },
        ast::TypeExpr::Variant { cases, .. } => resolve_variant_type_expr(cases, known_types, cur_mod),
    }
}

/// How a type name is spelled at one position, which decides whether the
/// builtin table can answer it: `Int` is written bare, `Map[K, V]` applied to
/// two arguments, and `Point { .. }` / `Point { x, .. } =>` is a record head,
/// which only ever names a nominal record — no builtin is constructed that way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TypeSpelling {
    /// `TypeExpr::Simple`: `Int`, `Point`.
    Bare,
    /// `TypeExpr::Generic` with this many arguments: `List[T]`, `T!E`.
    Applied(usize),
    /// The head of a record literal or record pattern.
    RecordHead,
}

/// Which spellings of a builtin head the resolver answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinArity {
    /// Only the bare spelling (`Int`, and `Matrix` without arguments).
    Bare,
    /// Applied to any number of arguments (`List[T]`; a missing element type
    /// resolves to `Unknown`).
    Any,
    /// Applied to at least this many (`Map[K, V]`; fewer is a nominal name).
    AtLeast(usize),
    /// Applied to between `lo` and `hi` arguments inclusive (`T!`, `T!E`).
    Between(usize, usize),
}

impl BuiltinArity {
    fn accepts(self, spelling: TypeSpelling) -> bool {
        match (self, spelling) {
            (BuiltinArity::Bare, TypeSpelling::Bare) => true,
            (BuiltinArity::Any, TypeSpelling::Applied(_)) => true,
            (BuiltinArity::AtLeast(lo), TypeSpelling::Applied(n)) => n >= lo,
            (BuiltinArity::Between(lo, hi), TypeSpelling::Applied(n)) => lo <= n && n <= hi,
            _ => false,
        }
    }

    /// One spelling this arity accepts — what a test writes to exercise the
    /// head (`Map[Int, Int]` for `AtLeast(2)`).
    pub fn sample(self) -> TypeSpelling {
        match self {
            BuiltinArity::Bare => TypeSpelling::Bare,
            BuiltinArity::Any => TypeSpelling::Applied(1),
            BuiltinArity::AtLeast(n) | BuiltinArity::Between(n, _) => TypeSpelling::Applied(n),
        }
    }
}

/// One builtin type head: the name, the spellings it answers, and the type
/// it builds from the already-resolved arguments.
pub struct BuiltinTypeHead {
    pub name: &'static str,
    pub arity: BuiltinArity,
    pub build: fn(&[Ty]) -> Ty,
}

fn first_or_unknown(ra: &[Ty]) -> Ty {
    ra.first().cloned().unwrap_or(Ty::Unknown)
}

/// EVERY type name the resolver answers without consulting a declaration,
/// and the ONLY place those answers live (#2839). `resolve_type_expr_in`
/// dispatches through it before any module's type is looked up, so a
/// module's own `type Map` never takes `Map[K, V]` away; the scope check
/// (`FileTypeScope::locate`) asks the same table, so the two cannot disagree
/// about which spellings are builtin. A new builtin is added here or nowhere.
pub const BUILTIN_TYPE_HEADS: &[BuiltinTypeHead] = &[
    // Sized numeric types (Stage 1a of the sized-numeric-types arc). `Int64`
    // / `Float64` alias to `Ty::Int` / `Ty::Float` — writing either form is
    // indistinguishable at the type checker layer, so existing code that
    // uses `Int` keeps compiling while new code can use the precise width.
    BuiltinTypeHead { name: "Int", arity: BuiltinArity::Bare, build: |_| Ty::Int },
    BuiltinTypeHead { name: "Float", arity: BuiltinArity::Bare, build: |_| Ty::Float },
    BuiltinTypeHead { name: "Int64", arity: BuiltinArity::Bare, build: |_| Ty::Int64 },
    BuiltinTypeHead { name: "Float64", arity: BuiltinArity::Bare, build: |_| Ty::Float64 },
    BuiltinTypeHead { name: "Int8", arity: BuiltinArity::Bare, build: |_| Ty::Int8 },
    BuiltinTypeHead { name: "Int16", arity: BuiltinArity::Bare, build: |_| Ty::Int16 },
    BuiltinTypeHead { name: "Int32", arity: BuiltinArity::Bare, build: |_| Ty::Int32 },
    BuiltinTypeHead { name: "UInt8", arity: BuiltinArity::Bare, build: |_| Ty::UInt8 },
    BuiltinTypeHead { name: "UInt16", arity: BuiltinArity::Bare, build: |_| Ty::UInt16 },
    BuiltinTypeHead { name: "UInt32", arity: BuiltinArity::Bare, build: |_| Ty::UInt32 },
    BuiltinTypeHead { name: "UInt64", arity: BuiltinArity::Bare, build: |_| Ty::UInt64 },
    BuiltinTypeHead { name: "Float32", arity: BuiltinArity::Bare, build: |_| Ty::Float32 },
    BuiltinTypeHead { name: "String", arity: BuiltinArity::Bare, build: |_| Ty::String },
    BuiltinTypeHead { name: "Bool", arity: BuiltinArity::Bare, build: |_| Ty::Bool },
    BuiltinTypeHead { name: "Unit", arity: BuiltinArity::Bare, build: |_| Ty::Unit },
    BuiltinTypeHead { name: "Bytes", arity: BuiltinArity::Bare, build: |_| Ty::Bytes },
    // Bare `Matrix` (no args) stays `Ty::Matrix` — the compat rule in
    // `types/mod.rs` bridges bare `Matrix` ↔ `Matrix[Float]`.
    BuiltinTypeHead { name: "Matrix", arity: BuiltinArity::Bare, build: |_| Ty::Matrix },
    BuiltinTypeHead { name: "RawPtr", arity: BuiltinArity::Bare, build: |_| Ty::RawPtr },
    BuiltinTypeHead { name: "Path", arity: BuiltinArity::Bare, build: |_| Ty::String },
    // `Never` is the bottom type — used by `process.exit` and similar
    // diverging fns. It has to surface as `Ty::Never` (not
    // `Ty::Named("Never", [])`); otherwise bundled sigs that spell
    // `-> Never` would be unifiable only with another nominal `Never` type,
    // which doesn't exist.
    BuiltinTypeHead { name: "Never", arity: BuiltinArity::Bare, build: |_| Ty::Never },
    // ADR-0002 Phase 1 (#1103): the pseudo-generic `!` is the pure-fallible
    // return marker — `-> T!` ≡ `-> Result[T, String]`. ADR-0012 D2 (#1193):
    // the 2-arg marker carries a TYPED error — `T!E` ≡ `Result[T, E]`.
    BuiltinTypeHead {
        name: "!", arity: BuiltinArity::Between(1, 2),
        build: |ra| Ty::result(ra[0].clone(), ra.get(1).cloned().unwrap_or(Ty::String)),
    },
    // ADR-0010: the pseudo-generic `?` is the Option marker — `T?` ≡
    // `Option[T]` in EVERY type position (unlike `!`, which is a
    // return-position marker: `?` is a property of the value, `!` of the
    // arrow).
    BuiltinTypeHead { name: "?", arity: BuiltinArity::Between(1, 1), build: |ra| Ty::option(ra[0].clone()) },
    BuiltinTypeHead { name: "List", arity: BuiltinArity::Any, build: |ra| Ty::list(first_or_unknown(ra)) },
    BuiltinTypeHead { name: "Option", arity: BuiltinArity::Any, build: |ra| Ty::option(first_or_unknown(ra)) },
    BuiltinTypeHead {
        name: "Result", arity: BuiltinArity::AtLeast(2),
        build: |ra| Ty::result(ra[0].clone(), ra[1].clone()),
    },
    BuiltinTypeHead {
        name: "Map", arity: BuiltinArity::AtLeast(2),
        build: |ra| Ty::map_of(ra[0].clone(), ra[1].clone()),
    },
    BuiltinTypeHead { name: "Set", arity: BuiltinArity::Any, build: |ra| Ty::set_of(first_or_unknown(ra)) },
    // Sized Numeric Types P4 kickoff: `Matrix[T]` resolves to
    // `Applied(Matrix, [T])` so the checker can discriminate
    // `Matrix[Float32]` / `Matrix[Float64]`.
    BuiltinTypeHead {
        name: "Matrix", arity: BuiltinArity::Any,
        build: |ra| Ty::Applied(TypeConstructorId::Matrix, ra.to_vec()),
    },
];

/// The builtin head a spelling resolves to, if any — the first thing the
/// resolver asks of every type name, before any declaration is consulted.
/// The generated-source spelling ([`generated_builtin_spelling`]) answers the
/// same head as the bare name.
pub fn builtin_type_head(name: &str, spelling: TypeSpelling) -> Option<&'static BuiltinTypeHead> {
    let name = name.strip_prefix(GENERATED_BUILTIN_MARK).unwrap_or(name);
    BUILTIN_TYPE_HEADS.iter().find(|h| h.name == name && h.arity.accepts(spelling))
}

/// The mark that makes a builtin's name the generated-source spelling. The
/// lexer never puts it in a name, backticks included, so no declaration can
/// take the spelling.
const GENERATED_BUILTIN_MARK: char = '%';

/// How compiler-GENERATED source names a builtin type (#2870): the same head
/// as the bare `name`, under a spelling no file can declare. A file's own
/// declaration answers its bare spelling of a builtin's name (#2858), and
/// generated helpers appended to the entry program are read in that file's
/// scope, so `fn __repr_quote(s: String)` read the user's `type String`. A
/// generated `String` is always the builtin: the source generator's
/// token stream carries this spelling, never the bare one, and the key
/// [`builtin_named_type_key`] records is never consulted for it.
pub fn generated_builtin_spelling(name: &str) -> Option<String> {
    builtin_type_head(name, TypeSpelling::Bare).map(|h| format!("{GENERATED_BUILTIN_MARK}{}", h.name))
}

/// Where one bare type spelling of a file goes (#2715, #2839), in the order
/// the resolver consults them. Only `OutOfScope` is an error (E029).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeNameOrigin {
    /// A `BUILTIN_TYPE_HEADS` entry answers the spelling before any
    /// declaration is consulted.
    Builtin,
    /// The file declares the name itself.
    OwnDecl,
    /// The stdlib owns the name (the auto-import).
    Stdlib,
    /// A user module the file can see declares it: the file's own module or
    /// one it imports.
    InScope,
    /// Only user modules this file never imports declare it — sorted and
    /// deduplicated canonical module names.
    OutOfScope(Vec<Sym>),
    /// No user module declares it; left to the ordinary unknown-type path.
    Undeclared,
}

/// One file's view of the program's type declarations: which user modules
/// declare which bare names, and which of those modules the file can see.
/// Built with one scan of the type table for the names the file spells.
pub struct FileTypeScope {
    own: std::collections::HashSet<Sym>,
    visible: std::collections::HashSet<Sym>,
    owners: HashMap<Sym, Vec<Sym>>,
}

impl FileTypeScope {
    /// `scope` is the file's canonical module (`None` for the entry file),
    /// `own` the type names it declares, `names` the bare names it spells.
    pub fn new(
        env: &crate::types::TypeEnv,
        scope: Option<&str>,
        own: std::collections::HashSet<Sym>,
        names: &std::collections::HashSet<Sym>,
    ) -> Self {
        // The file's spelled names are checked first, so a key no file
        // spells costs no module lookup; the module set is the env's own,
        // probed by symbol rather than copied into a string set per file.
        let mut owners: HashMap<Sym, Vec<Sym>> = HashMap::new();
        for k in env.types.keys() {
            if let Some((m, base)) = k.as_str().rsplit_once('.') {
                let base = sym(base);
                if !names.contains(&base) || almide_lang::stdlib_info::is_bundled_module(m) {
                    continue;
                }
                let m = sym(m);
                if env.user_modules.contains(&m) {
                    owners.entry(base).or_default().push(m);
                }
            }
        }
        for mods in owners.values_mut() {
            mods.sort_by(|a, b| a.as_str().cmp(b.as_str()));
            mods.dedup();
        }
        // `locate` asks `visible` only about a module in `owners`, so only
        // those are decided — not a copy of every module the file can see
        // (#3340: that copy, per file, was most of this check's cost).
        let here = scope.map(sym);
        let visible: std::collections::HashSet<Sym> = owners.values().flatten().copied()
            .filter(|m| {
                env.import_table.accessible.contains(m)
                    || env.import_table.aliases.values().any(|a| a == m)
                    || Some(*m) == here
            })
            .collect();
        FileTypeScope { own, visible, owners }
    }

    /// Where `name`, spelled as `spelling`, goes in this file.
    pub fn locate(&self, name: Sym, spelling: TypeSpelling) -> TypeNameOrigin {
        // The file's own declaration answers a bare spelling before the
        // builtin of the same name, as `resolve_type_expr_in` does (#2858).
        if spelling == TypeSpelling::Bare && self.own.contains(&name) {
            return TypeNameOrigin::OwnDecl;
        }
        if builtin_type_head(name.as_str(), spelling).is_some() {
            return TypeNameOrigin::Builtin;
        }
        if self.own.contains(&name) {
            return TypeNameOrigin::OwnDecl;
        }
        let Some(mods) = self.owners.get(&name) else { return TypeNameOrigin::Undeclared };
        if mods.iter().any(|m| self.visible.contains(m)) {
            return TypeNameOrigin::InScope;
        }
        // A stdlib type of the same name is what the bare spelling means
        // here (the auto-import); only a user-module-only name is out of scope.
        if almide_lang::stdlib_info::stdlib_owned_type_owner(name.as_str()).is_some()
            || crate::bundled_sigs::bundled_type_owner(name.as_str()).is_some()
        {
            return TypeNameOrigin::Stdlib;
        }
        TypeNameOrigin::OutOfScope(mods.clone())
    }
}

// The nominal (non-builtin) arm of `TypeExpr::Simple` resolution.
fn resolve_simple_type_other(other: &str, known_types: Option<&HashMap<Sym, Ty>>, cur_mod: Option<&str>) -> Ty {
    // #433: a user module's (qualified) reference to a namespaced
    // type resolves to its canonical `mod.Type` name; falls through
    // to the existing bare resolution for stdlib / local types.
    if let Some(qualified) = known_types.and_then(|types| canonical_user_type_sym(other, types, cur_mod)).map(|s| Ty::Named(s, vec![])) {
        return qualified;
    }
    if let Some((_, alias)) = known_types.and_then(|types| scoped_alias_entry(other, types, cur_mod)) {
        return alias.clone();
    }
    // - Generic type parameters (T, U, Self, ...) resolve via
    //   known_types as `Ty::TypeVar`.
    // - Record/Variant declarations must keep their nominal
    //   identity — expanding them to the structural form here
    //   would collapse two distinct types with identical shapes
    //   (e.g. Dog and Cat both `{name: String}`). They come back
    //   as `Ty::Named` and are expanded on demand via
    //   `canonical_user_type_sym` above.
    // - OpenRecord aliases (`type Named = { name: String, .. }`)
    //   are *shape aliases* meant to act as structural bounds,
    //   not nominal types. Keep them transparent so they can
    //   still accept any record with at least those fields.
    // - Transparent aliases (e.g. `type Score = Int`) follow
    //   through to the target type so `a + b` works.
    if let Some(types) = known_types {
        if let Some((_, found)) = table_type_entry(other, types) {
            match found {
                Ty::TypeVar(tv) => return Ty::TypeVar(*tv),
                Ty::Record { .. } | Ty::Variant { .. } => {
                    // nominal — keep as Named, but use the canonical name
                    if let Some((_, bare)) = other.rsplit_once('.') {
                        return Ty::Named(sym(bare), vec![]);
                    }
                }
                other_ty => return other_ty.clone(),
            }
        }
    }
    // For module-qualified names, use the bare name for Ty::Named
    if let Some((_, bare)) = other.rsplit_once('.') {
        Ty::Named(sym(bare), vec![])
    } else {
        Ty::Named(sym(other), vec![])
    }
}

// The alias entry a name means in `cur_mod` ahead of the plain table lookup:
// the module's own alias of the name (#3401) — read under its qualified key,
// since the bare key belongs to whichever module registered last — or a user
// alias of a stdlib-owned name (`type Value = Int`), which lives under the
// shadow key (#1828). The nominal shapes are `canonical_user_type_sym`'s.
fn scoped_alias_entry<'t>(name: &str, types: &'t HashMap<Sym, Ty>, cur_mod: Option<&str>) -> Option<(Sym, &'t Ty)> {
    if let Some(alias) = own_module_alias(name, types, cur_mod) {
        return Some((sym(&format!("{}.{}", cur_mod?, name)), alias));
    }
    stdlib_shadow_entry(name, types, cur_mod)
}

// The table entry a type name falls back to: the exact key (`Instr` or
// `binary.Instr`), else a qualified name's bare key.
fn table_type_entry<'t>(name: &str, types: &'t HashMap<Sym, Ty>) -> Option<(Sym, &'t Ty)> {
    let exact = sym(name);
    if let Some(t) = types.get(&exact) {
        return Some((exact, t));
    }
    let bare = sym(name.rsplit_once('.')?.1);
    types.get(&bare).map(|t| (bare, t))
}

// The TRANSPARENT alias a non-nominal name resolves to, with the key it is
// registered under — the same lookup `resolve_simple_type_other` makes for
// the bare spelling, so `Pair` and `Pair[Int]` name one declaration. A
// record, variant or type variable is not an alias.
fn transparent_alias_entry<'t>(name: &str, types: &'t HashMap<Sym, Ty>, cur_mod: Option<&str>) -> Option<(Sym, &'t Ty)> {
    let (key, body) = scoped_alias_entry(name, types, cur_mod).or_else(|| table_type_entry(name, types))?;
    match body {
        Ty::Record { .. } | Ty::Variant { .. } | Ty::TypeVar(_) | Ty::ConstParam { .. } => None,
        _ => Some((key, body)),
    }
}

// The nominal (non-builtin) arm of `TypeExpr::Generic` resolution, given the
// already-resolved argument types `ra`.
fn resolve_nominal_generic_type_expr(name: &Sym, ra: Vec<Ty>, known_types: Option<&HashMap<Sym, Ty>>, cur_mod: Option<&str>) -> Ty {
    // #433: qualify a user module's generic type to its canonical `mod.Type`
    // name; stdlib / local generics stay bare.
    let qualified = known_types.and_then(|types| canonical_user_type_sym(name.as_str(), types, cur_mod));
    if let Some(qn) = qualified {
        return Ty::Named(qn, ra);
    }
    // A generic TRANSPARENT alias applied to its arguments (#3403) is its
    // body with each declared letter replaced by its argument, as a
    // non-generic alias is its body: `Pair[Int]` under `type Pair[T] =
    // (T, T)` is `(Int, Int)`. Records and variants stay nominal (above).
    // A wrong argument count keeps the alias's `Named` key, which the
    // checker's annotation sweep reports (E093).
    if let Some((key, body)) = known_types.and_then(|types| transparent_alias_entry(name.as_str(), types, cur_mod)) {
        let params = known_types.and_then(|types| declared_type_params(key.as_str(), types)).unwrap_or(&[]);
        if params.len() != ra.len() {
            return Ty::Named(key, ra);
        }
        let bindings: HashMap<Sym, Ty> = params.iter().zip(ra)
            .filter_map(|(p, a)| match p { Ty::TypeVar(v) => Some((*v, a)), _ => None })
            .collect();
        return almide_lang::types::substitute(body, &bindings);
    }
    let resolved_name = name.as_str().rsplit_once('.').map(|(_, bare)| sym(bare)).unwrap_or(*name);
    Ty::Named(resolved_name, ra)
}

// `TypeExpr::Variant { cases, .. }` resolution: lower each AST variant case form
// (Unit/Tuple/Record) to its `VariantCase` counterpart.
fn resolve_variant_type_expr(cases: &[ast::VariantCase], known_types: Option<&HashMap<Sym, Ty>>, cur_mod: Option<&str>) -> Ty {
    let cs = cases.iter().map(|c| match c {
        ast::VariantCase::Unit { name } => VariantCase {
            name: sym(name), payload: VariantPayload::Unit,
        },
        ast::VariantCase::Tuple { name, fields } => VariantCase {
            name: sym(name),
            payload: VariantPayload::Tuple(
                fields.iter().map(|f| resolve_type_expr_in(f, known_types, cur_mod)).collect(),
            ),
        },
        ast::VariantCase::Record { name, fields } => VariantCase {
            name: sym(name),
            payload: VariantPayload::Record(
                fields.iter().map(|f| (sym(&f.name), resolve_type_expr_in(&f.ty, known_types, cur_mod))).collect(),
            ),
        },
    }).collect();
    Ty::Variant { name: sym(""), cases: cs }
}
