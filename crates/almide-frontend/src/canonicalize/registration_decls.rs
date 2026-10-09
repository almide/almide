// Type-declaration and top-level-declaration registration: the `type`
// registrars, nominal reservation, and `register_decls` with its per-kind
// arms. `include!`d by registration.rs (the 800-line file budget); it
// shares that module's scope and imports.

/// Protocols whose auto-derive RECURSES INTO EACH FIELD'S TYPE: deriving them on a struct/variant emits per-field work that requires the field type to ALSO satisfy the protocol. `Codec` calls `Field.encode` / `Field.decode`; `Ord`/`Hash` lower to a Rust `#[derive(Ord/Hash)]` that needs the field's Rust type to impl it. `Eq`/`Repr` are excluded — every generated struct gets `PartialEq` + a repr path unconditionally, so a field need not declare them (gating those would be a false positive).
/// A borrowed view of the `type` declaration being registered.
pub struct TypeDeclToRegister<'a> {
    pub name: &'a str,
    pub ty: &'a ast::TypeExpr,
    pub deriving: &'a Option<Vec<Sym>>,
    pub generics: &'a Option<Vec<ast::GenericParam>>,
    pub prefix: Option<&'a str>,
    pub visibility: ast::Visibility,
}

pub fn register_type_decl(env: &mut TypeEnv, diagnostics: &mut Vec<Diagnostic>, decl: &TypeDeclToRegister<'_>) {
    let TypeDeclToRegister { name, ty, deriving, generics, prefix, visibility } = *decl;
    if let Some(derives) = deriving {
        validate_protocols(env, diagnostics, derives, name);
    }
    let gnames: Vec<Sym> = generics.as_ref().map(|gs| gs.iter().map(|g| sym(&g.name)).collect()).unwrap_or_default();
    // Shadow-and-restore (#1574): this type's generic letters must not
    // destroy same-named type bindings that already exist.
    let shadowed: Vec<(Sym, Option<Ty>)> =
        gnames.iter().map(|gn| (*gn, env.types.insert(*gn, Ty::TypeVar(*gn)))).collect();
    // The declaration's BODY resolves in the declaring module's scope, like
    // every fn signature does: a module record's field `List[Entry]` must pin
    // to that module's own `Entry`. Resolved bare, it fell to the
    // unique-owner rule, which is ambiguous the moment a second module also
    // declares `Entry` — the field stayed a bare `Entry`, the entry program
    // saw it through `mod.Toc.symbols`, and the flat leg refused the build
    // (#433 gate) while the wasm leg read the other `Entry`'s layout
    // (#1957).
    let mut resolved = resolve_in(env, ty, type_cur_mod(env, prefix));
    for (gn, prev) in shadowed.into_iter().rev() {
        match prev {
            Some(t) => { env.types.insert(gn, t); }
            None => { env.types.remove(&gn); }
        }
    }

    // Every shape declared under a stdlib-owned name registers under its
    // shadow scope (#1828); everything below keys on `prefix`, so rebinding
    // it here gives the declaration its qualified identity end to end. The
    // OPAQUE alias (`mod type X = String`) included (#1835): its newtype
    // identity — the `Ty::Named` its constructor call and pattern carry,
    // `opaque_alias_identity` — takes the same scope, so `mod type Value =
    // String` beside `json.parse` is two types on every leg. The DEFINING
    // module of the newtype (the E033 boundary) is read off the original
    // prefix before the rebind: the entry program's shadow scope is `self`,
    // not a module its own constructor call would be foreign to.
    let defining_module = prefix.map(sym).or(env.alias_owner_module);
    let identity = opaque_alias_identity(env, prefix, name);
    let owner = type_decl_prefix(env, prefix, name);
    let prefix = owner.as_deref();
    let user_shadow = shadows_stdlib_type(prefix, name);

    resolved = register_type_decl_opaque_alias(env, identity, defining_module, resolved, &gnames, visibility);
    register_type_decl_variant_ctors(env, diagnostics, name, prefix, &mut resolved);
    register_type_decl_check_duplicate(env, diagnostics, name, prefix, &resolved);
    register_type_decl_finalize(env, name, ty, prefix, resolved, user_shadow);
    register_type_params(env, name, prefix, &gnames, user_shadow);

    if let Some(derives) = deriving {
        register_derive_sigs(env, derives, name, prefix);
    }
}
/// `mod`/local type alias → nominal newtype (opaque constructor), when the declared visibility isn't Public and the resolved shape isn't already a Record/Variant. Registers the opaque-alias bookkeeping under the newtype's `identity` and returns the (possibly rewritten) resolved type. Verbatim text move out of [`register_type_decl`].
fn register_type_decl_opaque_alias(env: &mut TypeEnv, identity: Sym, defining_module: Option<Sym>, resolved: Ty, gnames: &[Sym], visibility: ast::Visibility) -> Ty {
    if !opaque_alias_shape(&resolved, visibility) {
        return resolved;
    }
    // Store the inner target type for codegen
    env.opaque_alias_targets.insert(identity, resolved.clone());
    // Register as nominal type (not transparent alias)
    let generic_args: Vec<Ty> = gnames.iter().map(|g| Ty::TypeVar(*g)).collect();
    let resolved = Ty::Named(identity, generic_args);
    // Register constructor with visibility restriction. The OWNER is a
    // definition-time identity captured once: the prefixed registration
    // names it outright, and the per-module re-registration (which runs
    // with prefix = None under `alias_owner_module`) must not overwrite it
    // with "no module" — that read the defining module's own constructor
    // call as foreign, so a `mod type` alias could never be built anywhere
    // (the reference compilers key this privilege to the definition's
    // module identity and never re-derive it: Rust's DefId parent, Gleam's
    // opaque-type module, Roc's opaque wrap/unwrap scope).
    env.opaque_alias_visibility.insert(identity, visibility);
    env.opaque_alias_module.insert(identity, defining_module);
    resolved
}

/// The nominal identity of an OPAQUE alias (`mod type X = T`, #1835): the
/// `Ty::Named` its constructor call and pattern carry, and the key of the
/// `opaque_alias_*` tables. A user module's is `m.X` whichever pass
/// registers it (the prefixed one names the module; `infer_module`'s
/// unprefixed one runs under `alias_owner_module`), and the entry program's
/// declaration of a stdlib-owned name is `self.X` (`type_decl_prefix`). A
/// BUNDLED module's own newtype (`html`'s `SafeHtml`) keeps the bare name —
/// the spelling every stdlib signature carries and `lower_type_decl`
/// declares it under — and so does the entry program's plain `mod type
/// UserId = Int`. The lowered ctor call, the ctor pattern and the type decl
/// all spell this one name, so the native flatten mangle and the wasm
/// newtype erasure see a single identity where the bare spelling used to
/// leave a module's `Token(s)` unresolved (rustc E0531) and unerased.
fn opaque_alias_identity(env: &TypeEnv, prefix: Option<&str>, name: &str) -> Sym {
    let scope = match prefix {
        Some(p) => Some(p.to_string()),
        None => env.alias_owner_module.map(|m| m.to_string()).or_else(|| type_decl_prefix(env, None, name)),
    };
    match scope {
        Some(p) if !almide_lang::stdlib_info::is_bundled_module(&p) => sym(&format!("{}.{}", p, name)),
        _ => sym(name),
    }
}

/// The `mod` / `local` alias-to-nominal-newtype rule: a non-public
/// declaration whose resolved shape is not already a record or variant.
fn opaque_alias_shape(resolved: &Ty, visibility: ast::Visibility) -> bool {
    !matches!(visibility, ast::Visibility::Public)
        && !matches!(resolved, Ty::Variant { .. } | Ty::Record { .. })
}
/// Fix up a `Variant`'s registered name to the DECLARED name, and register each of its constructors. Verbatim text move out of [`register_type_decl`].
fn register_type_decl_variant_ctors(env: &mut TypeEnv, diagnostics: &mut Vec<Diagnostic>, name: &str, prefix: Option<&str>, resolved: &mut Ty) {
    if let Ty::Variant { name: vn, cases } = resolved {
        *vn = sym(name);
        // Push (not overwrite) so a constructor name declared in multiple variant types keeps ALL candidates — needed to detect ambiguity (#413) instead of silently letting the last-registered type win. #413: record each candidate's OWNING MODULE so a shared ctor name can be disambiguated by the current module (`lookup_ctor_in`). type_name stays BARE here — other consumers expect that; `lookup_ctor_in` qualifies on demand.
        // A `None` prefix during `infer_module`'s temporary unprefixed pass still
        // belongs to the module being inferred — attribute it there so the dedupe
        // guard below collapses it onto the canonical prefixed candidate.
        let owner_mod = prefix.map(sym).or(env.alias_owner_module);
        for case in cases.iter() {
            let entry = env.constructors.entry(case.name).or_default();
            // E019 (#1426): a SECOND type in the SAME module declaring this
            // case name would make bare resolution registration-order-dependent
            // — `lookup_ctor_in`'s owned-first find() would silently keep the
            // older type winning and leave the new case unreachable. Reported
            // once, on the canonical pass (`infer_module`'s unprefixed alias
            // pass re-registers the same declarations; skip it like
            // `register_type_decl_check_duplicate` does).
            if env.alias_owner_module.is_none() {
                if let Some((prior, _, _)) = entry.iter().find(|(t, m, _)| *t != sym(name) && *m == owner_mod) {
                    diagnostics.push(err(
                        format!("ambiguous constructor '{}': declared in both '{}' and '{}' of the same module", case.name, prior, name),
                        format!("Rename the case in one of them so '{}' has exactly one meaning here.", case.name),
                        format!("constructor {}", case.name),
                    ).with_code("E019"));
                }
            }
            if !entry.iter().any(|(t, m, _)| *t == sym(name) && *m == owner_mod) {
                entry.push((sym(name), owner_mod, case.clone()));
            }
        }
    }
}
/// #433: a DIFFERENT structural type already holds this BARE name — two distinct types (a local type and a dependency's, or two sub-modules') sharing a name. Type identity is by bare name through link + codegen, so the second silently shadows the first and the function that used the shadowed type fails with a cryptic generated-Rust E0560/E0609. Until types are namespaced per package, surface the collision at the source so the user renames one. Structurally-identical re-registration (the diamond case: same package via two import paths) compares equal and is NOT flagged. #433: types are now namespaced per (user) package — `dep_a.Config` and `dep_b.Config` coexist as distinct qualified names. So a collision is only a real error when the SAME canonical key is re-declared with a different structure (a duplicate within one module/file), which we detect on the prefixed key. Structurally-identical re-registration (the diamond case) is equal and not flagged. Verbatim text move out of [`register_type_decl`].
fn register_type_decl_check_duplicate(env: &TypeEnv, diagnostics: &mut Vec<Diagnostic>, name: &str, prefix: Option<&str>, resolved: &Ty) {
    // `infer_module`'s temporary UNPREFIXED pass re-registers declarations the
    // canonical prefixed registration already validated, purely so intra-module
    // references resolve bare. Re-running the duplicate check there compares a
    // module's own type against a SIBLING package's bare alias of the same name
    // and reports a phantom E020 (`collqa.Config` vs `collqb.Config`, which #433
    // made legal). The real check already ran; skip the alias pass.
    if env.alias_owner_module.is_some() {
        return;
    }
    if matches!(resolved, Ty::Record { .. } | Ty::OpenRecord { .. } | Ty::Variant { .. }) {
        let canonical_key = prefixed_key(prefix, name);
        // A LOCAL type (main program, no prefix) is allowed to SHADOW a dependency's bare-name dual-registration rather than collide with it (#433): the existing bare `Persona` mirrors some `dep.Persona`, and a local `type Persona` should win for unqualified use (the dep stays reachable via `dep.Persona`). Only flag E020 for a genuine duplicate — another type registered under the SAME canonical key that is NOT just a dependency's bare alias being shadowed by a local.
        let shadows_dep_alias = prefix.is_none() && env.prefixed_bare_aliases.contains(&sym(&canonical_key));
        // The stdlib's pre-registration of this key is not a declaration of
        // the module being registered: a package module keyed like a stdlib
        // module (`url`) replaces it (#2843).
        let replaces_stdlib_preregistration = env.stdlib_preregistered_types.contains(&sym(&canonical_key));
        if !shadows_dep_alias && !replaces_stdlib_preregistration {
            if let Some(existing) = env.types.get(&sym(&canonical_key)) {
                if existing != resolved
                    && matches!(existing, Ty::Record { .. } | Ty::OpenRecord { .. } | Ty::Variant { .. })
                {
                    diagnostics.push(err(
                        format!("type '{}' is declared more than once with different structures", name),
                        format!("Two distinct types share the name '{}' within the same module. Rename one so the name is unique.", name),
                        format!("type {}", name),
                    ).with_code("E020"));
                }
            }
        }
    }
}
/// Register field defaults (both plain and record-payload variant cases), insert the resolved type under its canonical key, and — for a prefixed (imported/sub-module) type — dual-register the bare name for unqualified access. Verbatim text move out of [`register_type_decl`].
fn register_type_decl_finalize(env: &mut TypeEnv, name: &str, ty: &ast::TypeExpr, prefix: Option<&str>, resolved: Ty, user_shadow: bool) {
    let key = prefixed_key(prefix, name);
    // A user declaration shadowing a stdlib-owned name never writes the
    // BARE key (#1828): that key is the stdlib type's identity — the twin's
    // own bundled registration writes it, an undeclared builtin (`Value`)
    // has none — and every stdlib signature resolves through it. The user's
    // type is reachable through its qualified key alone.
    let dual_register_bare = prefix.is_some() && !user_shadow;
    // Field defaults, keyed like `types` (both keys when prefixed), so record-construction validation knows which fields may be omitted (#488).
    if let ast::TypeExpr::Record { fields } | ast::TypeExpr::OpenRecord { fields } = ty {
        let defaults: std::collections::HashSet<Sym> =
            fields.iter().filter(|f| f.default.is_some()).map(|f| f.name).collect();
        env.record_field_defaults.insert(sym(&key), defaults.clone());
        if dual_register_bare {
            env.record_field_defaults.insert(sym(name), defaults);
        }
    }
    // Record-payload variant cases carry field defaults too (`| Rect { color: String = "" }`) — harvest them from the AST, since the resolved `VariantPayload::Record` keeps only (name, ty).
    if let ast::TypeExpr::Variant { cases, .. } = ty {
        for c in cases {
            if let ast::VariantCase::Record { name: cname, fields } = c {
                let defs: Vec<Sym> = fields.iter().filter(|f| f.default.is_some()).map(|f| f.name).collect();
                if !defs.is_empty() {
                    env.ctor_field_defaults.entry(*cname).or_default().extend(defs);
                }
            }
        }
    }
    register_field_default_exprs(env, &key, ty, prefix);
    env.types.insert(sym(&key), resolved.clone());
    env.stdlib_preregistered_types.remove(&sym(&key));
    if dual_register_bare {
        // Bare-name dual-registration of a prefixed type, for unqualified access. Record it so a local same-name type may shadow it (#433).
        env.types.insert(sym(name), resolved);
        env.prefixed_bare_aliases.insert(sym(name));
    } else if prefix.is_none() {
        // A local type owns the bare name now — it is no longer a dependency alias, so a later genuine local duplicate is still caught by E020.
        env.prefixed_bare_aliases.remove(&sym(name));
    }
}
/// Record the declaration's parameter letters, in declared order, under the
/// same keys as its `types` entry (#3403): a generic transparent alias
/// substitutes its arguments for exactly these, and the checker counts them
/// against every application. A non-generic declaration records none, so
/// `Score[Int]` under `type Score = Int` is a count of 1 against 0.
fn register_type_params(env: &mut TypeEnv, name: &str, prefix: Option<&str>, gnames: &[Sym], user_shadow: bool) {
    let params = Ty::Tuple(gnames.iter().map(|g| Ty::TypeVar(*g)).collect());
    let key = prefixed_key(prefix, name);
    env.types.insert(crate::canonicalize::resolve::type_params_key(&key), params.clone());
    if prefix.is_some() && !user_shadow {
        env.types.insert(crate::canonicalize::resolve::type_params_key(name), params);
    }
}
/// A module type's field default expressions, keyed `mod.Type` /
/// `mod.Type.Case` (#3165). Only the canonical, module-prefixed registration
/// records them: a literal in the declaring module splices its own default
/// as before, and a literal in another module fills them in qualified
/// (`check/call_defaults.rs`).
fn register_field_default_exprs(env: &mut TypeEnv, key: &str, ty: &ast::TypeExpr, prefix: Option<&str>) {
    let Some(module) = prefix else { return };
    if env.alias_owner_module.is_some() || !env.user_modules.contains(&sym(module)) {
        return;
    }
    let harvest = |fields: &[ast::FieldType]| -> Vec<(Sym, ast::Expr)> {
        fields.iter().filter_map(|f| f.default.as_ref().map(|d| (f.name, d.clone()))).collect()
    };
    let mut put = |k: String, defs: Vec<(Sym, ast::Expr)>| {
        if !defs.is_empty() {
            env.field_default_exprs.insert(sym(&k), (sym(module), defs));
        }
    };
    match ty {
        ast::TypeExpr::Record { fields } | ast::TypeExpr::OpenRecord { fields } => put(key.to_string(), harvest(fields)),
        ast::TypeExpr::Variant { cases, .. } => {
            for c in cases {
                if let ast::VariantCase::Record { name: cname, fields } = c {
                    put(format!("{}.{}", key, cname), harvest(fields));
                }
            }
        }
        _ => {}
    }
}
/// Walk all declarations and register them into the type environment.
/// The placeholder a reserved key holds until its declaration registers: an
/// empty record or variant carrying a name no source can spell.
const RESERVATION: &str = "<reserved>";

fn is_reservation(t: &Ty) -> bool {
    match t {
        Ty::Record { fields } => fields.len() == 1 && fields[0].0.as_str() == RESERVATION,
        Ty::Variant { name, cases } => cases.is_empty() && name.as_str() == RESERVATION,
        _ => false,
    }
}

/// Reserve `mod.Name` for every record and variant type a user module
/// declares (see `register_decls`). Aliases and open-record shapes are
/// transparent — resolution expands them — so they are not reserved. Returns
/// the keys reserved.
fn reserve_own_nominal_types(env: &mut TypeEnv, decls: &[ast::Decl], prefix: Option<&str>) -> Vec<Sym> {
    let Some(m) = type_cur_mod(env, prefix) else { return Vec::new() };
    if almide_lang::stdlib_info::is_bundled_module(m) {
        return Vec::new();
    }
    let mut keys = Vec::new();
    for decl in decls {
        let ast::Decl::Type { name, ty, .. } = decl else { continue };
        let placeholder = match ty {
            ast::TypeExpr::Record { .. } => Ty::Record { fields: vec![(sym(RESERVATION), Ty::Unknown)] },
            ast::TypeExpr::Variant { .. } => Ty::Variant { name: sym(RESERVATION), cases: vec![] },
            _ => continue,
        };
        let key = sym(&format!("{}.{}", m, name));
        if let std::collections::hash_map::Entry::Vacant(slot) = env.types.entry(key) {
            slot.insert(placeholder);
            keys.push(key);
        }
    }
    keys
}

pub fn register_decls(env: &mut TypeEnv, diagnostics: &mut Vec<Diagnostic>, decls: &[ast::Decl], prefix: Option<&str>) {
    // Catch duplicate `fn <name>` / `test "<name>"` at the Almide stage so that rustc's E0428 "defined multiple times" never leaks to the user with a src/main.rs span. Tracked per (kind, name), remembering the first span.
    let mut seen_fn: HashMap<String, Option<ast::Span>> = HashMap::new();
    let mut seen_test: HashMap<String, Option<ast::Span>> = HashMap::new();

    // Types and protocols first, then everything that REFERS to a type (#2645).
    // A signature or top-level `let` annotation pins a bare `Step` to its own
    // module's `mod.Step` only when that key already exists; registered in
    // source order, `fn one() -> Step` above `type Step` found no `a.Step`
    // yet, fell through to whichever module's bare alias was registered last,
    // and typed the fn against ANOTHER module's same-named type — E013 on a
    // correct program, or a bare name at the #433 codegen gate. Declaration
    // order is not meaningful in Almide, so the result must not depend on it.
    // Every nominal type the module declares is reserved under its qualified
    // key BEFORE any declaration body is resolved, so a type that names a
    // sibling declared further down (`type R = { u: U }` above `type U`)
    // pins to its own `mod.U`. Without the reservation the own-module lookup
    // missed, the reference stayed a bare `U`, and the bare key belongs to
    // whichever module registered its `U` last — an order (import order,
    // directory order) that differs between machines, so the same program
    // checked on one OS and failed on another with E013 against a foreign
    // `U`. Each reservation is dropped right before its declaration
    // registers, so the E020 duplicate check never sees it.
    let reserved = reserve_own_nominal_types(env, decls, prefix);
    super::resolve::register_builtin_named_type_keys(env, decls, type_cur_mod(env, prefix));
    // An alias registers after every alias of this module it spells, and an
    // alias in a cycle registers as `Unknown` (#3407, registration_order.rs).
    for step in type_registration_steps(decls) {
        match step {
            DeclStep::Type { decl, cyclic } => {
                let ast::Decl::Type { name, .. } = decl else { continue };
                if let Some(key) = reserved.iter().find(|k| k.as_str().rsplit_once('.').is_some_and(|(_, b)| b == name.as_str()))
                    && env.types.get(key).is_some_and(is_reservation)
                {
                    env.types.remove(key);
                }
                register_decl_type(env, diagnostics, decl, prefix);
                if cyclic {
                    register_cyclic_alias_unknown(env, name.as_str(), prefix);
                }
            }
            DeclStep::Protocol(ast::Decl::Protocol { name, generics, methods, .. }) => {
                register_protocol_decl(env, name, generics, methods, prefix);
            }
            DeclStep::Protocol(_) => {}
            // Reported by the checker, which knows the file (`alias_cycle_diags`).
            DeclStep::Cycle(_) => {}
        }
    }
    for decl in decls {
        match decl {
            ast::Decl::Fn { .. } => register_decl_fn(env, diagnostics, &mut seen_fn, decl, prefix),
            ast::Decl::Test { .. } => register_decl_test(diagnostics, &mut seen_test, decl),
            ast::Decl::TopLet { .. } => register_decl_top_let(env, decl, prefix),
            _ => {}
        }
    }
    // `infer_module` re-registers a module's decls UNPREFIXED so its own
    // bodies resolve bare names, and marks that pass with `alias_owner_module`.
    // Validating from inside it compared one module's bare `Span` against
    // another's — over the WHOLE env, attributed to whichever file was under
    // inference — which is how a Codec on one module's type produced a field
    // mismatch pointing at an unrelated file (#1087). The canonical prefixed
    // registration validates the same declarations properly.
    if env.alias_owner_module.is_none() {
        validate_protocol_impls(env, diagnostics);
        validate_derive_field_support(env, diagnostics);
        validate_convention_method_conflicts(env, diagnostics);
    }
}
/// `ast::Decl::Fn` arm of [`register_decls`] — E012 duplicate-function diagnostic (skipped for `@extern` re-exports), signature registration, and DefTable registration. Verbatim text move; `continue` in the original loop becomes an early `return` here (both simply skip the rest of this decl's registration and move on to the next `decl`).
fn register_decl_fn(env: &mut TypeEnv, diagnostics: &mut Vec<Diagnostic>, seen_fn: &mut HashMap<String, Option<ast::Span>>, decl: &ast::Decl, prefix: Option<&str>) {
    let ast::Decl::Fn { name, params, return_type, effect, generics, span, visibility, extern_attrs, body, attrs, .. } = decl else { unreachable!() };
    // Skip duplicates that come from @extern re-export (name may appear twice by design).
    if extern_attrs.is_empty() {
        let key = prefixed_key(prefix, name);
        if let Some(first_span) = seen_fn.get(&key) {
            let mut diag = err(
                format!("duplicate function '{}'", name),
                format!("Rename one of the definitions, or remove the earlier one. Almide requires each function name to be unique within a module."),
                format!("fn {}", name),
            ).with_code("E012");
            if let Some(s) = span {
                diag.line = Some(s.line);
                diag.col = Some(s.col);
            }
            if let Some(first) = first_span {
                diag.secondary.push(almide_base::diagnostic::SecondarySpan {
                    line: first.line,
                    col: Some(first.col),
                    label: format!("first definition of '{}' here", name),
                });
            }
            diagnostics.push(diag);
            return;
        }
        seen_fn.insert(key, span.clone());
    }
    register_fn_sig(env, &FnSigToRegister {
        name, params, return_type, effect, generics,
        prefix, span: span.as_ref(), visibility: *visibility, attrs,
    });
    // Register in DefTable
    let fn_key = prefixed_key(prefix, name);
    let pkg = prefix.and_then(|p| p.split('.').next()).unwrap_or("");
    let mod_path = prefix.unwrap_or("");
    let ret = env.functions.get(&sym(&fn_key)).map(|s| s.ret.clone()).unwrap_or(Ty::Unknown);
    let did = env.def_table.alloc(sym(pkg), sym(mod_path), sym(name), almide_ir::DefKind::Function, ret);
    env.def_map.insert(sym(&fn_key), did);
    if !extern_attrs.is_empty() {
        env.extern_fns.insert(sym(&fn_key));
    }
    // #3504: a user module's fn spelled into the compiler's fn-name space is
    // escaped at lowering; recorded here so an importer lowered before or
    // after the module names it the same way.
    if prefix.is_some_and(crate::lower::is_user_module)
        && crate::lower::user_fn_needs_escape(name, extern_attrs, attrs)
    {
        env.escaped_module_fns.insert(sym(&fn_key));
    }
    // An EXPLICIT `fn Type.method` with a body, recorded on the shared env so
    // another module can find it. Lowering's own set only ever holds the
    // program being lowered, so a custom `repr` was silently ignored across an
    // import and `"${lib.Red}"` fell back to the variant name (#1087).
    if name.contains('.') && body.is_some() {
        env.explicit_convention_fns.insert(sym(&fn_key));
    }
}
/// `ast::Decl::Test` arm of [`register_decls`] — E012 duplicate-test diagnostic. Verbatim text move; `continue` becomes an early `return` (see [`register_decl_fn`]).
fn register_decl_test(diagnostics: &mut Vec<Diagnostic>, seen_test: &mut HashMap<String, Option<ast::Span>>, decl: &ast::Decl) {
    let ast::Decl::Test { name, span, .. } = decl else { unreachable!() };
    let test_key = name.to_string();
    if let Some(first_span) = seen_test.get(&test_key) {
        let mut diag = err(
            format!("duplicate test '{}'", name),
            format!("Rename one of the tests, or merge them. Each test name must be unique within a file."),
            format!("test \"{}\"", name),
        ).with_code("E012");
        if let Some(s) = span {
            diag.line = Some(s.line);
            diag.col = Some(s.col);
        }
        if let Some(first) = first_span {
            diag.secondary.push(almide_base::diagnostic::SecondarySpan {
                line: first.line,
                col: Some(first.col),
                label: format!("first test '{}' here", name),
            });
        }
        diagnostics.push(diag);
        return;
    }
    seen_test.insert(test_key, span.clone());
}
/// `ast::Decl::Type` arm of [`register_decls`] — type registration plus DefTable and `type_protocols` bookkeeping. Verbatim text move out of [`register_decls`].
fn register_decl_type(env: &mut TypeEnv, diagnostics: &mut Vec<Diagnostic>, decl: &ast::Decl, prefix: Option<&str>) {
    let ast::Decl::Type { name, ty, deriving, generics, visibility, .. } = decl else { unreachable!() };
    register_type_decl(env, diagnostics, &TypeDeclToRegister {
        name, ty, deriving, generics, prefix, visibility: *visibility,
    });
    // Register in DefTable, under the same identity key the type env holds:
    // the shadow scope when `register_type_decl` gave the declaration one
    // (a stdlib-owned name, #1828 — the opaque alias included, #1835), else
    // the prefixed key.
    let owner = type_decl_prefix(env, prefix, name);
    let scoped_key = prefixed_key(owner.as_deref(), name);
    let type_key = if env.types.contains_key(&sym(&scoped_key)) { scoped_key } else { prefixed_key(prefix, name) };
    let pkg = prefix.and_then(|p| p.split('.').next()).unwrap_or("");
    let mod_path = prefix.unwrap_or("");
    let resolved_ty = env.types.get(&sym(&type_key)).cloned().unwrap_or(Ty::Unknown);
    let did = env.def_table.alloc(sym(pkg), sym(mod_path), sym(name), almide_ir::DefKind::Type, resolved_ty);
    env.def_map.insert(sym(&type_key), did);
    if let Some(derives) = deriving {
        // Recorded under the bare name AND the canonical prefixed one: a
        // `[T: Codec]` bound resolves its argument to `Ty::Named("lib.P")` and
        // looked that up here, where only bare `P` had ever been written — so
        // a conforming type from another module was reported as not
        // implementing the protocol (#1087). A stdlib-owned name's bare slot
        // is the stdlib's (#1828): the user's derives never land on it.
        let protocol_keys: Vec<Sym> = if type_key != prefixed_key(prefix, name) {
            vec![sym(&type_key)]
        } else {
            vec![sym(name), sym(&type_key)]
        };
        for d in derives {
            for key in protocol_keys.iter().copied() {
                env.type_protocols
                    .entry(key)
                    .or_insert_with(std::collections::HashSet::new)
                    .insert(sym(d));
            }
        }
        register_conformance_args(env, diagnostics, decl, prefix, &protocol_keys);
    }
}
/// The type arguments of each explicit conformance (#1589: `type UserRepo:
/// Repository[UserId, User]`), resolved in the declaring module's scope with
/// the type's own letters shadowed, recorded under the same keys as
/// `type_protocols`. A type conforms to a protocol at most ONCE: its
/// `Type.method` fns are the implementation, and two conformances to one
/// protocol would need two — the second is reported as ambiguous rather than
/// silently picking one.
fn register_conformance_args(env: &mut TypeEnv, diagnostics: &mut Vec<Diagnostic>, decl: &ast::Decl, prefix: Option<&str>, keys: &[Sym]) {
    let ast::Decl::Type { name, deriving: Some(derives), deriving_refs, generics, .. } = decl else { return };
    let mut seen: HashMap<Sym, usize> = HashMap::new();
    for (i, d) in derives.iter().enumerate() {
        if let Some(&first) = seen.get(d) {
            let show = |j: usize| display_protocol_ref(env, derives[j], ast::protocol_ref_at(deriving_refs, j), prefix);
            diagnostics.push(err(
                format!("type '{}' conforms to protocol '{}' twice ('{}' and '{}') — the conformance is ambiguous", name, d, show(first), show(i)),
                format!("A type implements a protocol at most once: its `fn {}.<method>` definitions are that one implementation. Keep one conformance and wrap the type (`type {}2: {} = {{ inner: {} }}`) for the other", name, name, show(i), name),
                format!("type {} : {}", name, d),
            ));
            continue;
        }
        seen.insert(*d, i);
    }
    let Some(refs) = deriving_refs else { return };
    let gnames: Vec<Sym> = generics.iter().flatten().map(|g| g.name).collect();
    let shadowed: Vec<(Sym, Option<Ty>)> =
        gnames.iter().map(|gn| (*gn, env.types.insert(*gn, Ty::TypeVar(*gn)))).collect();
    let tcm = type_cur_mod(env, prefix);
    let resolved: Vec<(Sym, Vec<Ty>)> = derives.iter().zip(refs.iter()).enumerate()
        .filter(|(i, (d, r))| !r.args.is_empty() && seen.get(*d) == Some(i))
        .map(|(_, (d, r))| (*d, r.args.iter().map(|a| resolve_in(env, a, tcm)).collect()))
        .collect();
    for (gn, prev) in shadowed.into_iter().rev() {
        match prev {
            Some(t) => { env.types.insert(gn, t); }
            None => { env.types.remove(&gn); }
        }
    }
    for (d, args) in resolved {
        for key in keys.iter().copied() {
            env.type_protocol_args.entry(key).or_default().entry(d).or_insert_with(|| args.clone());
        }
    }
}
/// `Repository[UserId, User]` as written in a conformance list, for messages.
fn display_protocol_ref(env: &TypeEnv, name: Sym, r: Option<&ast::ProtocolRef>, prefix: Option<&str>) -> String {
    let Some(r) = r.filter(|r| !r.args.is_empty()) else { return name.to_string() };
    let tcm = type_cur_mod(env, prefix);
    format!("{}[{}]", name, r.args.iter().map(|a| resolve_in(env, a, tcm).display()).collect::<Vec<_>>().join(", "))
}
/// `ast::Decl::TopLet` arm of [`register_decls`] — top-level `let` type seeding (or reuse of a fully-inferred prior entry) and DefTable registration. Verbatim text move out of [`register_decls`].
fn register_decl_top_let(env: &mut TypeEnv, decl: &ast::Decl, prefix: Option<&str>) {
    let ast::Decl::TopLet { name, ty, value, mutable, .. } = decl else { unreachable!() };
    // #2645: the annotation pins to `mod.Type` exactly as a fn signature does
    // (`register_fn_sig`). With the plain `resolve` a module's
    // `let STEPS: List[Step]` stayed bare `Step`; that seed is concrete, so
    // the checker never upgraded it, and every reader — the module's own
    // lowering and `b.STEPS` from another file — carried a bare name that two
    // modules own into codegen (#433 gate). The seed of an annotation-less
    // record initializer takes the same owner for the same reason.
    let tcm = type_cur_mod(env, prefix);
    let rt = ty.as_ref().map(|te| resolve_in(env, te, tcm))
        .unwrap_or_else(|| infer_top_let_seed(env, tcm, value));
    let key = prefixed_key(prefix, name);
    // A PREFIXED key names exactly one decl program-wide, and registration re-runs per driver leg over a persistent env — re-seeding must not downgrade a fully inferred entry (the post-solve flush's `Option[Cfg]`) back to the seed's partial `Option[Unknown]`. Unprefixed keys stay unconditional: they are scoped aliases (main program / intra-module temp) where an entry may legitimately describe a DIFFERENT decl.
    let keep_existing = prefix.is_some()
        && (rt.contains_unknown() || rt.contains_typevar())
        && env.top_lets.get(&sym(&key)).is_some_and(|t| {
            !t.contains_unknown() && !t.contains_typevar()
        });
    if !keep_existing {
        env.top_lets.insert(sym(&key), rt.clone());
    }
    // Register in DefTable
    let pkg = prefix.and_then(|p| p.split('.').next()).unwrap_or("");
    let mod_path = prefix.unwrap_or("");
    let did = env.def_table.alloc(sym(pkg), sym(mod_path), sym(name), almide_ir::DefKind::TopLet, rt);
    env.def_map.insert(sym(&key), did);
    if *mutable { env.mutable_top_lets.insert(sym(&key)); }
}
