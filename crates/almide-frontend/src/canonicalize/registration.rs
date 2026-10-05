//! Declaration registration: collecting function signatures, type declarations,
//! protocol declarations, and protocol validation into the type environment.
//!
//! These are free functions operating on `&mut TypeEnv` + `&mut Vec<Diagnostic>`,
//! extracted from the former `Checker` methods in `check/registration.rs`.

use std::collections::HashMap;
use almide_lang::ast;
use almide_base::diagnostic::Diagnostic;
use almide_base::intern::{Sym, sym};
use almide_lang::types::TypeConstructorId;
use crate::types::{Ty, TypeEnv, FnSig, ProtocolDef, ProtocolMethodSig, VariantPayload};
use super::resolve::resolve_type_expr;
fn err(msg: impl Into<String>, hint: impl Into<String>, ctx: impl Into<String>) -> Diagnostic {
    Diagnostic::error(msg, hint, ctx)
}
/// Resolve an AST type expression using the current type environment.
fn resolve(env: &TypeEnv, te: &ast::TypeExpr) -> Ty {
    resolve_type_expr(te, Some(&env.types))
}
/// Like `resolve`, but pins a user module's own-type references to the qualified canonical name `mod.Type` (#433). `cur_mod` is the module being registered.
fn resolve_in(env: &TypeEnv, te: &ast::TypeExpr, cur_mod: Option<&str>) -> Ty {
    crate::canonicalize::resolve::resolve_type_expr_in(te, Some(&env.types), cur_mod)
}
/// The module a TYPE reference should be resolved against.
///
/// `infer_module` re-registers a module's declarations with `prefix: None` on
/// purpose — the KEYS must be bare so the module's own bodies resolve their own
/// names. But the TYPES still have to pin to `mod.Type`: with `cur_mod` unset,
/// a bare `Span` declared in two modules has no unique owner, so it stayed
/// bare and the #433 name-pin gate refused the build (#1087). That pass marks
/// itself with `alias_owner_module`, which names the module the bare keys
/// really belong to.
fn type_cur_mod<'a>(env: &TypeEnv, prefix: Option<&'a str>) -> Option<&'a str>
where
    'static: 'a,
{
    prefix.or_else(|| env.alias_owner_module.map(|m| m.as_str()))
}
/// Infer type from a literal expression (for top-level `let` without annotation). Used at registration time — before the full checker runs — so module top_lets have a concrete `env.top_lets` entry the moment the main program's inference looks them up. A shallow scalar-only version regresses records / lists / maps to `Ty::Unknown`, which later surfaces as `LazyLock<_>` in generated Rust and `ConcretizeTypes` post-condition failures on WASM. Recurse structurally through record / list / tuple / map literals so the cross-module user sees the right type. Seed type for an UNANNOTATED top-let. `infer_literal_type` covers literals and anonymous records only; a NAMED constructor (`Cfg { … }`) fell to `Ty::Unknown`, and because every driver checks MAIN before the modules, main's inference read that stale Unknown for a cross-module `m.CFG` — a spread of it then carried Unknown into the AllTypesConcrete refusal (#502). Resolve the ctor name through the SAME #433 predicate an explicit `: Cfg` annotation uses, so both spellings seed identically. Generic decls (unresolved type params) deliberately stay Unknown — the ctor args are not inferable here and the later module-check writeback only corrects exact-Unknown seeds.
pub fn infer_top_let_seed(env: &TypeEnv, prefix: Option<&str>, value: &ast::Expr) -> Ty {
    match &value.kind {
        ast::ExprKind::Paren { expr } => infer_top_let_seed(env, prefix, expr),
        ast::ExprKind::Record { name: Some(n), .. } => {
            let canonical = super::resolve::canonical_user_type_sym(n.as_str(), &env.types, prefix)
                .unwrap_or_else(|| sym(n.as_str()));
            let canonical = super::resolve::follow_record_alias(canonical, &env.types);
            match env.types.get(&canonical) {
                Some(decl) if !decl.has_unresolved_deep() => Ty::Named(canonical, vec![]),
                _ => Ty::Unknown,
            }
        }
        _ => infer_literal_type(value),
    }
}
pub fn infer_literal_type(expr: &ast::Expr) -> Ty {
    infer_literal_scalar(expr)
        .or_else(|| infer_literal_composite(expr))
        .unwrap_or(Ty::Unknown)
}

/// Scalars and the simple leaf literals.
///
/// One group of `infer_literal_type`'s arm table, arms verbatim and in source order.
/// `None` means "not my group"; the router tries the groups in that order,
/// so which type a literal seeds is unchanged.
pub fn infer_literal_scalar(expr: &ast::Expr) -> Option<Ty> {
    Some(match &expr.kind {
        ast::ExprKind::Int { .. } => Ty::Int,
        ast::ExprKind::Float { .. } => Ty::Float,
        ast::ExprKind::String { .. } => Ty::String,
        ast::ExprKind::Bool { .. } => Ty::Bool,
        ast::ExprKind::Unit => Ty::Unit,
        ast::ExprKind::Paren { expr } => infer_literal_type(expr),
        // A signed literal is a literal: `let MARGIN_AUTO = -2.0` must seed Float, not Unknown (#784 — the Unknown seed leaked into every cross-module reader of the constant). `-` keeps the operand's numeric type; `not` is Bool.
        ast::ExprKind::Unary { op, operand } => match op.as_str() {
            "-" => infer_literal_type(operand),
            "not" | "!" => Ty::Bool,
            _ => Ty::Unknown,
        },
        ast::ExprKind::Record { name: None, fields } => {
            let mut fs: Vec<(Sym, Ty)> = fields.iter()
                .map(|fi| (fi.name, infer_literal_type(&fi.value)))
                .collect();
            fs.sort_by_key(|(n, _)| *n);
            Ty::Record { fields: fs }
        }
        _ => return None,
    })
}

/// Collections, records and the constructor forms.
///
/// One group of `infer_literal_type`'s arm table, arms verbatim and in source order.
/// `None` means "not my group"; the router tries the groups in that order,
/// so which type a literal seeds is unchanged.
pub fn infer_literal_composite(expr: &ast::Expr) -> Option<Ty> {
    Some(match &expr.kind {
        ast::ExprKind::List { elements } => {
            let elem = elements.first()
                .map(|e| infer_literal_type(e))
                .unwrap_or(Ty::Unknown);
            Ty::Applied(TypeConstructorId::List, vec![elem])
        }
        ast::ExprKind::Tuple { elements } => {
            Ty::Tuple(elements.iter().map(infer_literal_type).collect())
        }
        ast::ExprKind::MapLiteral { entries } => {
            let (k, v) = entries.first()
                .map(|(k, v)| (infer_literal_type(k), infer_literal_type(v)))
                .unwrap_or((Ty::Unknown, Ty::Unknown));
            Ty::Applied(TypeConstructorId::Map, vec![k, v])
        }
        ast::ExprKind::Some { expr } => {
            Ty::Applied(TypeConstructorId::Option, vec![infer_literal_type(expr)])
        }
        ast::ExprKind::None => {
            Ty::Applied(TypeConstructorId::Option, vec![Ty::Unknown])
        }
        ast::ExprKind::Ok { expr } => {
            Ty::Applied(TypeConstructorId::Result, vec![infer_literal_type(expr), Ty::Unknown])
        }
        ast::ExprKind::Err { expr } => {
            Ty::Applied(TypeConstructorId::Result, vec![Ty::Unknown, infer_literal_type(expr)])
        }
        _ => return None,
    })
}
/// Build a prefixed key: "module.name" or just "name".
pub fn prefixed_key(prefix: Option<&str>, name: &str) -> String {
    prefix.map(|p| format!("{}.{}", p, name)).unwrap_or_else(|| name.to_string())
}

/// The identity prefix of a `type` declaration: its module when it has one;
/// otherwise, for a declaration of a STDLIB-OWNED name, the scope that keeps
/// it off the stdlib's bare key (#1828) — the module whose unprefixed pass
/// `infer_module` is running (`alias_owner_module`), else the entry
/// program's `ROOT_TYPE_SCOPE`. Every other unprefixed declaration keeps the
/// bare key it always had — including the owning stdlib module's own
/// declaration when THAT module is the entry program (`almide compile bytes`
/// checks the bundled source on its own, `entry_bundled_module`): that
/// registration is the one that WRITES the bare key, not a user shadow of it.
pub fn type_decl_prefix(env: &TypeEnv, prefix: Option<&str>, name: &str) -> Option<String> {
    if let Some(p) = prefix {
        return Some(p.to_string());
    }
    almide_lang::stdlib_info::stdlib_owned_type_owner(name)?;
    match env.alias_owner_module {
        Some(m) => Some(m.to_string()),
        None if env.entry_owns_stdlib_type(name) => None,
        None => Some(super::resolve::ROOT_TYPE_SCOPE.to_string()),
    }
}

/// Is this a USER declaration shadowing a stdlib-owned type name — one whose
/// canonical key is `prefix.name` while the bare `name` stays the stdlib's?
/// False for the stdlib's own bundled registration of the same name (that
/// registration is what WRITES the bare key).
fn shadows_stdlib_type(prefix: Option<&str>, name: &str) -> bool {
    prefix.is_some_and(|p| !almide_lang::stdlib_info::is_bundled_module(p))
        && almide_lang::stdlib_info::stdlib_owned_type_owner(name).is_some()
}

/// The `env.functions` key for a convention method (`encode`, `repr`, …) on
/// `type_name`, or `None` when the type has no such method.
///
/// The two producers disagree on shape and both are load-bearing: an EXPLICIT
/// `fn Color.repr` inside module `lib` is registered prefixed
/// (`lib.Color.repr`), while a DERIVED method is registered bare
/// (`P.encode`) because that bare name is what lowering emits and what the
/// backend resolves via `module_origin`. A checked expression carries the
/// canonical `lib.P` either way, so the qualified spelling is tried first and
/// the bare one is the fallback. Consumers must use the key this RETURNS —
/// guessing one shape is exactly how `p.encode()`, `json.encode(p)` and
/// `[T: Codec]` came to fail across an import (#1087, #1089).
/// #1726 (the #1591 ruling): a convention method defined on the SAME type in
/// MORE than one module is a check-time error. Open extension is ratified —
/// a caller module extending a foreign type with a FRESH method name is
/// deliberate surface — but a duplicate had no defined winner: native answered
/// the defining module's body and wasm the caller's (an I-divergence). No
/// precedence rule is worth that class of drift; the duplicate is refused
/// with both sites named.
///
/// Identity is the SELF parameter's canonical type (#433 gives user-module
/// types their qualified spelling), so two DIFFERENT types that share a bare
/// name (`modA.Box` vs `modB.Box`) each keep their own method set, and only a
/// true same-type duplicate collides. Derived methods are not in
/// `explicit_convention_fns`, so a custom override of a derived `repr`/`eq`
/// stays legal (#1087).
fn validate_convention_method_conflicts(env: &TypeEnv, diagnostics: &mut Vec<Diagnostic>) {
    // The TYPE a convention key targets, as a collision identity:
    //   - "P.Type.method" where P declares Type       -> "P.Type" (its own)
    //   - "Type.method" where the root declares Type  -> "Type"
    //   - a FOREIGN extension (the #1591 surface)     -> the sole declaring
    //     module's "M.Type" when exactly one module declares that bare name;
    //     with several same-named candidates the identity falls back to the
    //     key itself — self-unique, so ambiguity NEVER produces a false
    //     conflict (modA.Box and modB.Box each keep their own method set).
    let conv_target_id = |key: &str| -> Option<(String, String)> {
        let (head, method) = key.rsplit_once('.')?;
        let (prefix, type_seg) = match head.rsplit_once('.') {
            Some((p, t)) => (Some(p), t),
            _ => (Option::None, head),
        };
        // A convention key's middle segment is a TYPE name (uppercase); a
        // dotted PLAIN fn ("mod.helper") is not a convention target.
        if !type_seg.starts_with(|c: char| c.is_ascii_uppercase()) {
            return Option::None;
        }
        let id = match prefix {
            Some(p) => {
                let qualified = format!("{}.{}", p, type_seg);
                if env.types.contains_key(&sym(&qualified)) {
                    qualified // the module's own type
                } else {
                    foreign_owner_id(env, type_seg).unwrap_or_else(|| key.to_string())
                }
            }
            _ => {
                if env.types.contains_key(&sym(type_seg))
                    && !env.prefixed_bare_aliases.contains(&sym(type_seg))
                {
                    type_seg.to_string() // the root program's own type
                } else {
                    foreign_owner_id(env, type_seg).unwrap_or_else(|| key.to_string())
                }
            }
        };
        Some((id, method.to_string()))
    };
    let mut by_target: HashMap<(String, String), Vec<String>> = HashMap::new();
    for key in &env.explicit_convention_fns {
        let Some(target) = conv_target_id(key.as_str()) else { continue };
        by_target.entry(target).or_default().push(key.to_string());
    }
    let mut conflicts: Vec<((String, String), Vec<String>)> =
        by_target.into_iter().filter(|(_, keys)| keys.len() > 1).collect();
    conflicts.sort();
    for ((ty, method), mut keys) in conflicts {
        keys.sort();
        keys.dedup();
        if keys.len() < 2 { continue; }
        let msg = format!("convention method '{}.{}' is defined in more than one module", ty, method);
        // Registration validation can run more than once over one env — an
        // identical conflict must not stack.
        if diagnostics.iter().any(|d| d.message == msg) { continue; }
        diagnostics.push(err(
            msg,
            format!(
                "The definitions ({}) would shadow each other with no defined winner. \
                 Keep ONE definition — extension at a distance is fine for a NEW method \
                 name, but a duplicate on the same type is refused (#1591 ruling).",
                keys.join(", ")
            ),
            format!("fn {}.{}", ty, method),
        ).with_code("E012"));
    }
}

/// The single module that declares bare type `type_seg`, as "M.Type" — or
/// None when zero or several modules do (ambiguity never flags; see caller).
fn foreign_owner_id(env: &TypeEnv, type_seg: &str) -> Option<String> {
    let suffix = format!(".{}", type_seg);
    let mut owner: Option<String> = Option::None;
    for k in env.types.keys() {
        if k.as_str().ends_with(&suffix) {
            if owner.is_some() {
                return Option::None;
            }
            owner = Some(k.to_string());
        }
    }
    owner
}

pub fn convention_fn_key(env: &TypeEnv, type_name: &str, method: &str) -> Option<Sym> {
    let qualified = sym(&format!("{}.{}", type_name, method));
    if env.functions.contains_key(&qualified) {
        return Some(qualified);
    }
    let k = sym(&format!("{}.{}", bare_type_name(type_name), method));
    env.functions.contains_key(&k).then_some(k)
}

fn bare_type_name(type_name: &str) -> &str {
    type_name.rsplit_once('.').map_or(type_name, |(_, t)| t)
}

/// The name a convention-method CALL must carry, when the method exists.
///
/// Deliberately different from [`convention_fn_key`]: the checker needs the
/// key that holds the signature (prefixed for an explicit method), while
/// lowering needs the name the DEFINITION lowers to — always the bare
/// `Type.method`, with the module re-attached later from `module_origin`.
/// Emitting the prefixed spelling produced calls to `lib_Color_repr` against a
/// definition called `almide_rt_lib_Color_repr` (#1087).
pub fn convention_emit_key(env: &TypeEnv, type_name: &str, method: &str) -> Option<Sym> {
    convention_fn_key(env, type_name, method).map(|_| {
        // A receiver whose canonical type is module-qualified (`moda.Box`)
        // emits the QUALIFIED method name ONLY when the bare spelling is
        // AMBIGUOUS — two modules each registered `<mod>.Box.tag`, so the
        // bare `Box.tag` is one key at symbol re-attachment and dispatched
        // to whichever module registered last (#1728). A single owner keeps
        // the bare form: that is the spelling BOTH backends' re-attachment
        // machinery links (#1087), and qualifying it walled the wasm leg
        // (the #1087 fixture fell off the fallback ratchet).
        if let Some((_, bare)) = type_name.rsplit_once('.') {
            let qualified = format!("{}.{}", type_name, method);
            let suffix = format!(".{}.{}", bare, method);
            if env.functions.contains_key(&sym(&qualified))
                && env
                    .functions
                    .keys()
                    .any(|k| k.as_str().ends_with(&suffix) && k.as_str() != qualified)
            {
                return sym(&qualified);
            }
        }
        sym(&format!("{}.{}", bare_type_name(type_name), method))
    })
}
/// Substitute `Self` → concrete type in a protocol method type.
fn substitute_self(ty: &Ty, replacement: &Ty) -> Ty {
    match ty {
        Ty::TypeVar(name) if name == "Self" => replacement.clone(),
        _ => ty.map_children(&|child| substitute_self(child, replacement)),
    }
}
/// Strip module qualification from every `Named` type anywhere in the tree (`m.Pigment` → `Pigment`), so a protocol-satisfaction signature check doesn't false-positive on a cross-module type that's spelled bare on one side and qualified on the other. Mirrors `leaf_satisfies`'s bare-name leniency, generalized to nested/compound types via `map_children`.
fn strip_module_qualifier(ty: &Ty) -> Ty {
    match ty {
        Ty::Named(name, args) => {
            let bare = name.as_str().rsplit('.').next().unwrap_or(name.as_str());
            Ty::Named(sym(bare), args.iter().map(strip_module_qualifier).collect())
        }
        _ => ty.map_children(&strip_module_qualifier),
    }
}
/// Collect structural bounds from generic params: Record → OpenRecord conversion.
pub fn collect_structural_bounds(env: &TypeEnv, generics: &Option<Vec<ast::GenericParam>>) -> HashMap<Sym, Ty> {
    let mut sb = HashMap::new();
    let gs = match generics { Some(gs) => gs, None => return sb };
    for g in gs {
        let bte = match &g.structural_bound { Some(bte) => bte, None => continue };
        let bt = resolve(env, bte);
        sb.insert(sym(&g.name), match bt { Ty::Record { fields } => Ty::OpenRecord { fields }, o => o });
    }
    sb
}
/// Scalar type names that indicate a compile-time value parameter (not a protocol bound).
pub const SCALAR_TYPE_NAMES: &[&str] = &[
    "Int", "Float", "Bool", "String",
    "Int8", "Int16", "Int32", "Int64",
    "UInt8", "UInt16", "UInt32", "UInt64",
    "Float32", "Float64",
];
/// The `Ty` a scalar type NAME denotes, for the const-generic bound syntax
/// `[N: Int]`.
///
/// `Some` for exactly the names in [`SCALAR_TYPE_NAMES`] —
/// `scalar_type_names_all_resolve` asserts the two agree. Before they shared
/// this function, a name could sit in the list with no arm here and the const
/// param would be silently skipped, so `[N: Int16]` would type-check as an
/// ordinary generic and then fail in codegen.
pub fn scalar_ty_by_name(name: &str) -> Option<Ty> {
    Some(match name {
        "Int" | "Int64" => Ty::Int,
        "Float" | "Float64" => Ty::Float,
        "Bool" => Ty::Bool,
        "String" => Ty::String,
        "Int8" => Ty::Int8,
        "Int16" => Ty::Int16,
        "Int32" => Ty::Int32,
        "UInt8" => Ty::UInt8,
        "UInt16" => Ty::UInt16,
        "UInt32" => Ty::UInt32,
        "UInt64" => Ty::UInt64,
        "Float32" => Ty::Float32,
        _ => return None,
    })
}

/// Identify const (value) parameters in generic params. A param `N: Int` where `Int` is a scalar type (not a protocol) becomes a const param. Returns: param name → scalar Ty.
pub fn collect_const_params(generics: &Option<Vec<ast::GenericParam>>) -> HashMap<Sym, Ty> {
    let mut cp = HashMap::new();
    let gs = match generics { Some(gs) => gs, None => return cp };
    for g in gs {
        if let Some(bounds) = &g.bounds {
            // A single scalar-type-name bound makes this a const param.
            if let [only] = bounds.as_slice() {
                if let Some(ty) = scalar_ty_by_name(only) {
                    cp.insert(sym(&g.name), ty);
                }
            }
        }
    }
    cp
}
/// Collect protocol bounds from generic params: TypeVar name → list of protocol names. Excludes const params (scalar type bounds like `N: Int`).
pub fn collect_protocol_bounds(generics: &Option<Vec<ast::GenericParam>>) -> HashMap<Sym, Vec<Sym>> {
    let mut pb = HashMap::new();
    let gs = match generics { Some(gs) => gs, None => return pb };
    for g in gs {
        if let Some(bounds) = &g.bounds {
            if !bounds.is_empty() {
                // Skip if this is a const param (single scalar type bound)
                if bounds.len() == 1 && SCALAR_TYPE_NAMES.contains(&bounds[0].as_str()) {
                    continue;
                }
                pb.insert(sym(&g.name), bounds.iter().map(|b| sym(b)).collect());
            }
        }
    }
    pb
}
/// Type arguments of every APPLIED protocol bound (#1589), resolved in the
/// declaring module's scope — call inside the window where the generic
/// letters are shadowed to `Ty::TypeVar`, so `[K, R: Repository[K, User]]`
/// resolves `K` to the fn's own letter.
pub fn collect_protocol_bound_args(env: &TypeEnv, generics: &Option<Vec<ast::GenericParam>>, cur_mod: Option<&str>) -> HashMap<(Sym, Sym), Vec<Ty>> {
    let mut out = HashMap::new();
    for g in generics.iter().flatten() {
        for (i, b) in g.bounds.iter().flatten().enumerate() {
            let Some(r) = ast::protocol_ref_at(&g.bound_refs, i) else { continue };
            if r.args.is_empty() { continue; }
            out.insert((g.name, *b), r.args.iter().map(|a| resolve_in(env, a, cur_mod)).collect());
        }
    }
    out
}
/// A borrowed view of the `fn` signature being registered.
///
/// `effect` is an `&Option<bool>` and was adjacent
/// positional parameters, so transposing them type-checked. Named fields make
/// that a compile error instead of a signature registered with the wrong
/// effect-ness.
pub struct FnSigToRegister<'a> {
    pub name: &'a str,
    pub params: &'a [ast::Param],
    pub return_type: &'a ast::TypeExpr,
    pub effect: &'a Option<bool>,
    pub generics: &'a Option<Vec<ast::GenericParam>>,
    pub prefix: Option<&'a str>,
    pub span: Option<&'a ast::Span>,
    pub visibility: ast::Visibility,
    /// The declaration's attributes, so registration can read `@deprecated`.
    pub attrs: &'a [ast::Attribute],
}

pub fn register_fn_sig(env: &mut TypeEnv, decl: &FnSigToRegister<'_>) {
    let FnSigToRegister {
        name, params, return_type, effect, generics, prefix, span, visibility, attrs,
    } = *decl;
    let gnames: Vec<Sym> = generics.as_ref().map(|gs| gs.iter().map(|g| sym(&g.name)).collect()).unwrap_or_default();
    let sb = collect_structural_bounds(env, generics);
    let pb = collect_protocol_bounds(generics);
    let const_params = collect_const_params(generics);
    // A generic letter SHADOWS an existing type binding for the duration of
    // this one signature — it must never destroy it. The remove-without-
    // restore form deleted a user type's bare binding whenever any later-
    // registered fn (a stdlib module's, another user module's) used that
    // letter as a generic, making the stdlib's generic letters an invisible
    // reserved set (#1574).
    let shadowed: Vec<(Sym, Option<Ty>)> = gnames.iter().map(|gn| {
        let prev = if let Some(scalar_ty) = const_params.get(gn) {
            env.types.insert(*gn, Ty::ConstParam { name: *gn, ty: Box::new(scalar_ty.clone()) })
        } else {
            env.types.insert(*gn, Ty::TypeVar(*gn))
        };
        (*gn, prev)
    }).collect();
    // A bare `self` first parameter is sugar for `self: Self` (the parser always types it `TypeExpr::Simple { name: "Self" }`). Inside a `protocol { ... }` declaration `Self` is a legitimate unresolved placeholder, but on a real convention method (`fn Type.method(self, ...)`) it must resolve to the enclosing type, the same way `Self` in a protocol's own signature gets substituted when checked against one.
    // The synthesized receiver resolves through the SAME canonicalizer as a
    // written type reference: with TWO modules owning the bare name, a bare
    // `Ty::Named` here typed the method's self as the ambiguous bare name
    // while the checked receiver carried the canonical `mod.Type` (#1728).
    let tcm = type_cur_mod(env, prefix);
    let receiver_ty = name
        .split_once('.')
        .map(|(ty_name, _)| resolve_in(env, &ast::TypeExpr::Simple { name: sym(ty_name) }, tcm));
    let ptys: Vec<(Sym, Ty)> = params.iter().enumerate().map(|(i, p)| {
        if i == 0 && p.name.as_str() == "self" {
            if let (ast::TypeExpr::Simple { name: tn }, Some(rt)) = (&p.ty, &receiver_ty) {
                if tn.as_str() == "Self" {
                    return (sym(&p.name), rt.clone());
                }
            }
        }
        (sym(&p.name), resolve_in(env, &p.ty, tcm))
    }).collect();
    let mut_params: Vec<usize> = params.iter().enumerate()
        .filter(|(_, p)| p.is_mut)
        .map(|(i, _)| i)
        .collect();
    let ret = resolve_in(env, return_type, tcm);
    let pba = collect_protocol_bound_args(env, generics, tcm);
    for (gn, prev) in shadowed.into_iter().rev() {
        match prev {
            Some(t) => { env.types.insert(gn, t); }
            None => { env.types.remove(&gn); }
        }
    }
    let is_effect = effect.unwrap_or(false);
    let key = prefixed_key(prefix, name);
    let min_p = params.iter().take_while(|p| p.default.is_none()).count();
    env.functions.insert(sym(&key), FnSig { params: ptys, ret, is_effect, generics: gnames, structural_bounds: sb, protocol_bounds: pb, protocol_bound_args: pba, mut_params });
    match crate::deprecation::parse(attrs) {
        Ok(Some(dep)) => { env.deprecations.insert(sym(&key), dep); }
        Ok(None) => {}
        Err(e) => {
            // #1518: a malformed `@deprecated` was silently dropped here (the
            // Err discarded), so the deprecation never registered while the
            // author believed the marker was live — exactly what the error
            // type's own doc forbids. Same E053 attribute family, but an
            // ERROR: an unreadable deprecation is worse than none.
            let mut diag = almide_base::diagnostic::Diagnostic::error(
                e.message(), e.hint(), format!("fn {}", key)).with_code("E053");
            if let Some(s) = attrs.iter().find(|a| a.name.as_str() == "deprecated")
                .and_then(|a| a.span.as_ref()) {
                diag.line = Some(s.line);
                diag.col = Some(s.col);
            }
            env.attr_diagnostics.push(diag);
        }
    }
    // An attribute nobody reads is dropped, so a typo silently does nothing.
    // Collected here rather than at the parser because the vocabulary is a
    // front-end fact, not a grammar one.
    crate::attr_vocab::check_attrs(attrs, &mut env.attr_diagnostics);
    // Record visibility so `resolve_module_call` can reject cross-module access to `mod fn` / `local fn`. Only non-Public entries need to be stored — the lookup in the checker treats "missing" as Public (stdlib, impl methods, derived stubs).
    if !matches!(visibility, ast::Visibility::Public) {
        env.fn_visibility.insert(sym(&key), visibility);
    }
    if let Some(s) = span {
        env.fn_decl_spans.insert(sym(&key), (s.line, s.col));
    }
    if min_p < params.len() {
        env.fn_min_params.insert(sym(&key), min_p);
        // Keyed by the prefixed name so a caller in another module can fill
        // these in — lowering's per-file map never sees an imported program.
        env.fn_defaults.insert(
            sym(&key),
            params.iter().map(|p| p.default.as_ref().map(|d| (**d).clone())).collect(),
        );
    }
}
pub fn validate_protocols(env: &TypeEnv, diagnostics: &mut Vec<Diagnostic>, derives: &[Sym], type_name: &str) {
    for d in derives {
        if !env.protocols.contains_key(&sym(d)) {
            let valid: Vec<&str> = env.protocols.keys().map(|s| s.as_str()).collect();
            diagnostics.push(err(
                format!("unknown protocol '{}' on type '{}'", d, type_name),
                format!("Known protocols: {}", {
                    let mut sorted = valid; sorted.sort(); sorted.join(", ")
                }),
                format!("type {}", type_name),
            ));
        }
    }
}
pub fn register_derive_sigs(env: &mut TypeEnv, derives: &[Sym], type_name: &str, prefix: Option<&str>) {
    // #433: the VALUE type in derived signatures must carry the canonical qualified name for a user module's type — `Pigment.decode`'s `Result[Pigment, String]` with a bare name leaked into callers' var tables (found by the NameResolutionTotal gate). The fn KEYS stay as they were (separate resolution system).
    let canonical = match prefix {
        Some(m) if !almide_lang::stdlib_info::is_bundled_module(m) => format!("{}.{}", m, type_name),
        _ => type_name.to_string(),
    };
    let type_ty = Ty::Named(sym(&canonical), vec![]);
    let value_ty = Ty::Named(sym("Value"), vec![]);
    let empty_sb: HashMap<Sym, Ty> = HashMap::new();
    let empty_pb: HashMap<Sym, Vec<Sym>> = HashMap::new();
    // The VALUE type is canonical (#433); the KEYS stay BARE. Consumers that
    // hold a qualified `Ty::Named("lib.P")` reach these through
    // `convention_fn_key`, which falls back to the bare spelling — registering
    // the qualified key here instead would change the name LOWERING emits, and
    // the backend resolves a derived method by its bare name plus
    // `module_origin` (#1087).
    let register = |env: &mut TypeEnv, method: &str, sig: FnSig| {
        let key = format!("{}.{}", type_name, method);
        if !env.functions.contains_key(&sym(&key)) {
            env.functions.insert(sym(&key), sig);
        }
    };
    for d in derives {
        match d.as_str() {
            "Eq" => register(env, "eq", FnSig { params: vec![("a".into(), type_ty.clone()), ("b".into(), type_ty.clone())], ret: Ty::Bool, is_effect: false, generics: vec![], structural_bounds: empty_sb.clone(), protocol_bounds: empty_pb.clone(), protocol_bound_args: HashMap::new(), mut_params: vec![] }),
            "Repr" => register(env, "repr", FnSig { params: vec![("v".into(), type_ty.clone())], ret: Ty::String, is_effect: false, generics: vec![], structural_bounds: empty_sb.clone(), protocol_bounds: empty_pb.clone(), protocol_bound_args: HashMap::new(), mut_params: vec![] }),
            "Codec" => {
                register(env, "encode", FnSig { params: vec![("v".into(), type_ty.clone())], ret: value_ty.clone(), is_effect: false, generics: vec![], structural_bounds: empty_sb.clone(), protocol_bounds: empty_pb.clone(), protocol_bound_args: HashMap::new(), mut_params: vec![] });
                register(env, "decode", FnSig { params: vec![("v".into(), value_ty.clone())], ret: Ty::result(type_ty.clone(), Ty::String), is_effect: false, generics: vec![], structural_bounds: empty_sb.clone(), protocol_bounds: empty_pb.clone(), protocol_bound_args: HashMap::new(), mut_params: vec![] });
            }
            _ => {}
        }
    }
}
/// Register a user-defined protocol declaration into env.protocols.
pub fn register_protocol_decl(env: &mut TypeEnv, name: &str, generics: &Option<Vec<ast::GenericParam>>, methods: &[ast::ProtocolMethod], prefix: Option<&str>) {
    let gnames: Vec<Sym> = generics.as_ref()
        .map(|gs| gs.iter().map(|g| sym(&g.name)).collect())
        .unwrap_or_default();

    // Temporarily register `Self` as a TypeVar so resolve_type_expr handles
    // it. Shadow-and-restore, not insert-and-remove: a protocol's generic
    // letter must not destroy a coexisting type binding of the same name
    // (#1574 — same rule as register_fn_sig).
    let mut shadowed: Vec<(Sym, Option<Ty>)> = Vec::new();
    shadowed.push((sym("Self"), env.types.insert(sym("Self"), Ty::TypeVar(sym("Self")))));
    for gn in &gnames {
        shadowed.push((*gn, env.types.insert(*gn, Ty::TypeVar(*gn))));
    }

    let method_sigs: Vec<ProtocolMethodSig> = methods.iter().map(|m| {
        let params: Vec<(Sym, Ty)> = m.params.iter()
            .map(|p| (sym(&p.name), resolve(env, &p.ty)))
            .collect();
        let ret = resolve(env, &m.return_type);
        ProtocolMethodSig {
            name: sym(&m.name),
            params,
            ret,
            is_effect: m.effect,
            mut_params: m.params.iter().enumerate().filter(|(_, p)| p.is_mut).map(|(i, _)| i).collect(),
        }
    }).collect();

    for (gn, prev) in shadowed.into_iter().rev() {
        match prev {
            Some(t) => { env.types.insert(gn, t); }
            None => { env.types.remove(&gn); }
        }
    }

    // The origin is a definition-time identity: the prefixed registration
    // sets it, and `infer_module`'s unprefixed re-registration of the same
    // declarations must not erase it (the same rule as opaque-alias owners).
    let origin = prefix.map(sym)
        .or_else(|| env.protocols.get(&sym(name)).and_then(|p| p.origin));
    env.protocols.insert(sym(name), ProtocolDef {
        name: sym(name),
        generics: gnames,
        methods: method_sigs,
        origin,
    });
}
include!("registration_decls.rs");
include!("registration_order.rs");

include!("registration_validate.rs");

#[cfg(test)]
mod scalar_name_tests {
    use super::*;

    /// Every name the const-generic bound syntax accepts must resolve to a type.
    /// A name in the list with no `scalar_ty_by_name` arm silently downgrades
    /// `[N: T]` to an ordinary generic, which then fails in codegen rather than
    /// at the annotation.
    #[test]
    fn scalar_type_names_all_resolve() {
        let unresolved: Vec<&str> = SCALAR_TYPE_NAMES.iter().copied()
            .filter(|n| scalar_ty_by_name(n).is_none())
            .collect();
        assert!(unresolved.is_empty(), "scalar type names with no Ty: {unresolved:?}");
    }
}
