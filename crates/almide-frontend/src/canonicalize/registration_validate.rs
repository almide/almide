// ── Derive and protocol validation ──────────────────────────────
//
// Structural checks that a `deriving` clause or a `protocol` impl can actually
// be satisfied, run at the checker so neither backend has to reject the code
// later. Split out of `registration.rs`, which registers declarations; both
// halves share that file's imports via `include!`.

const FIELD_RECURSIVE_PROTOCOLS: &[&str] = &["Codec", "Ord", "Hash"];
/// The field-type slots a structural type exposes to its derive: record fields, and every variant case's payload (tuple positions / record fields).
fn type_field_slots(ty: &Ty) -> Vec<(String, Ty)> {
    match ty {
        Ty::Record { fields } | Ty::OpenRecord { fields } =>
            fields.iter().map(|(n, t)| (n.to_string(), t.clone())).collect(),
        Ty::Variant { cases, .. } => {
            let mut out = Vec::new();
            for c in cases {
                match &c.payload {
                    VariantPayload::Unit => {}
                    VariantPayload::Tuple(ts) => for (i, t) in ts.iter().enumerate() {
                        out.push((format!("{}.{}", c.name, i), t.clone()));
                    },
                    VariantPayload::Record(fs) => for (n, t) in fs {
                        out.push((n.to_string(), t.clone()));
                    },
                }
            }
            out
        }
        _ => Vec::new(),
    }
}
/// The nominal leaf types a derive must recurse into for one field type, descending through the standard containers (List/Option/Set/Map/Result via `Applied`, tuples, nested anon records). A `List[Pigment]` field under a `: Codec` type requires `Pigment` to be Codec, so the leaf is `Pigment`.
fn collect_leaf_nominals(ty: &Ty, out: &mut Vec<Sym>) {
    match ty {
        Ty::Named(n, args) => {
            out.push(*n);
            for a in args { collect_leaf_nominals(a, out); }
        }
        Ty::Variant { name, .. } => out.push(*name),
        Ty::Applied(_, args) => for a in args { collect_leaf_nominals(a, out); },
        Ty::Tuple(elems) => for e in elems { collect_leaf_nominals(e, out); },
        Ty::Record { fields } | Ty::OpenRecord { fields } =>
            for (_, t) in fields { collect_leaf_nominals(t, out); },
        _ => {}
    }
}
/// Does user type `leaf` satisfy protocol `proto`? Keyed leniently: `type_protocols` is interned bare, but a cross-module field type may carry a qualified `mod.Type` name — accept either spelling. For `Codec`, a hand-written `Type.encode`/`Type.decode` pair (without a `: Codec` declaration) also satisfies the requirement, since the derive only needs those functions to exist.
fn leaf_satisfies(env: &TypeEnv, leaf: Sym, proto: &str) -> bool {
    let bare = leaf.as_str().rsplit('.').next().unwrap_or(leaf.as_str());
    let declares = |name: &str| env.type_protocols.get(&sym(name))
        .map_or(false, |s| s.contains(&sym(proto)));
    if declares(leaf.as_str()) || declares(bare) {
        return true;
    }
    if proto == "Codec" {
        let has = |m: &str| env.functions.contains_key(&sym(&format!("{}.{}", leaf, m)))
            || env.functions.contains_key(&sym(&format!("{}.{}", bare, m)));
        return has("encode") && has("decode");
    }
    false
}
/// The Codec derive serializes a field by structural recursion over String/Int/Float/Bool/Option/List/Named — it has NO Map or Set arm, so a `Map[K,V]` / `Set[T]` field silently falls through to the `Value`-as-String fallback: invalid Rust natively (E0614/E0308) and wrong/silent on wasm (#655). Detect such a container anywhere in the field type (under List/Option/Result/Tuple/anon-record). A `Map`/`Set` reached only through a NAMED type is that type's own concern (its `: Codec` is checked by the leaf rule), so we stop at `Ty::Named`.
fn codec_unsupported_container(ty: &Ty) -> Option<&'static str> {
    use almide_lang::types::TypeConstructorId as TC;
    match ty {
        Ty::Applied(TC::Map, _) => Some("Map"),
        Ty::Applied(TC::Set, _) => Some("Set"),
        Ty::Applied(_, args) => args.iter().find_map(|a| codec_unsupported_container(a)),
        Ty::Tuple(elems) => elems.iter().find_map(|e| codec_unsupported_container(e)),
        Ty::Record { fields } | Ty::OpenRecord { fields } =>
            fields.iter().find_map(|(_, t)| codec_unsupported_container(t)),
        _ => None,
    }
}

/// The accepted Codec field-type grammar. Everything outside it is rejected
/// HERE, at declaration — passing `check` and dying in codegen is the one
/// forbidden outcome (#1061). Containers nest arbitrarily (#1065); the
/// rejected shapes are semantic, not depth-based:
///
///   field := scalar | Value | named | Option[field'] | List[field']
///     where Option[Option[..]] is void on the wire at ANY depth (encode
///     omits/nulls a none, decode folds it back — some(none) and none cannot
///     be told apart), and Option[Value] is only meaningful at FIELD position
///     (elements have no "absent" state, so its 3-state contract cannot hold).
///   scalar := String | Int | Float | Bool
///
/// `named` acceptance (user nominal must itself derive Codec; unknown module
/// nominals pass) is validated by the leaf machinery in the caller, not here.
/// Returns (offending description, hint).
fn codec_field_shape_error(ty: &Ty) -> Option<(String, String)> {
    use almide_lang::types::TypeConstructorId as TC;
    fn is_atom(ty: &Ty) -> bool {
        matches!(ty, Ty::String | Ty::Int | Ty::Float | Ty::Bool | Ty::Named(..))
    }
    fn sized_numeric(ty: &Ty) -> bool {
        matches!(ty,
            Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64
            | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
            | Ty::Float32 | Ty::Float64)
    }
    fn is_value(ty: &Ty) -> bool {
        matches!(ty, Ty::Named(n, _) if n.as_str() == "Value")
    }
    fn leaf_error(ty: &Ty) -> Option<(String, String)> {
        if is_atom(ty) { return None; }
        if sized_numeric(ty) {
            return Some((
                format!("sized numeric type '{}'", ty.display()),
                "Use Int or Float in wire types (JSON has a single number type) and convert at the boundary with the to_* conversions.".to_string(),
            ));
        }
        if matches!(ty, Ty::Bytes) {
            return Some((
                "type 'Bytes'".to_string(),
                "JSON has no bytes — carry base64 in a String field (base64.encode / base64.decode), or hand-write Type.encode/Type.decode.".to_string(),
            ));
        }
        Some((
            format!("type '{}'", ty.display()),
            "Codec fields are String/Int/Float/Bool/Value, Codec types, and Option/List combinations of those. Wrap other shapes in a named Codec type, or hand-write Type.encode/Type.decode.".to_string(),
        ))
    }
    fn walk(ty: &Ty, at_field_root: bool) -> Option<(String, String)> {
        match ty {
            Ty::Applied(TC::Option, args) if args.len() == 1 => match &args[0] {
                Ty::Applied(TC::Option, _) => Some((
                    "nested type 'Option[Option[...]]'".to_string(),
                    "Encode omits a none field and decode folds missing/null back to none, so some(none) and none cannot be told apart on the wire. Use a `Value` field to distinguish absent from explicit null.".to_string(),
                )),
                inner if is_value(inner) && !at_field_root => Some((
                    "an element-position 'Option[Value]'".to_string(),
                    "Option[Value]'s absent-vs-null contract needs a KEY that can be omitted, which an element position does not have. List[Value] already carries explicit nulls verbatim.".to_string(),
                )),
                inner => walk(inner, false),
            },
            Ty::Applied(TC::List, args) if args.len() == 1 => walk(&args[0], false),
            other => leaf_error(other),
        }
    }
    walk(ty, true).map(|(offender, hint)| {
        // When the offender sits INSIDE a container, name the whole field type
        // as well — "has type 'List[Int]'" for a List[List[Int]] field reads
        // as if the legal part were the problem.
        if matches!(ty, Ty::Applied(TC::Option, _) | Ty::Applied(TC::List, _))
            && !offender.contains(&ty.display())
        {
            (format!("{} inside its type '{}'", offender, ty.display()), hint)
        } else {
            (offender, hint)
        }
    })
}
/// A type that derives a field-recursive protocol (Codec/Ord/Hash) requires every field type to ALSO satisfy it — otherwise the derive emits a call to a non-existent `Field.encode` (Codec) or a Rust `#[derive(Ord/Hash)]` over a field whose Rust type lacks the impl, both of which the checker previously accepted and codegen then rejected as "invalid Rust" (#611). This validates the requirement structurally, at the checker, independent of target.
fn validate_derive_field_support(env: &TypeEnv, diagnostics: &mut Vec<Diagnostic>) {
    let pairs: Vec<(Sym, Vec<Sym>)> = env.type_protocols.iter()
        .map(|(ty, protos)| (*ty, protos.iter().copied().collect()))
        .collect();
    let mut reported: std::collections::HashSet<(Sym, Sym, Sym)> = std::collections::HashSet::new();
    for (type_name, protocols) in &pairs {
        let Some(ty) = env.types.get(type_name) else { continue };
        let slots = type_field_slots(ty);
        if slots.is_empty() { continue; }
        for proto in protocols {
            let p = proto.as_str();
            if !FIELD_RECURSIVE_PROTOCOLS.contains(&p) { continue; }
            // #1522: the Codec derive emits ONE encode/decode pair per type, so a
            // generic type's `T` field reaches codegen unresolved — check green,
            // a dozen rustc errors. A type parameter has no wire form; reject the
            // derive itself here until a monomorphizing derive exists.
            if p == "Codec" && ty.contains_typevar() {
                if reported.insert((*type_name, *proto, sym("<generic>"))) {
                    diagnostics.push(err(
                        format!("type '{}' derives 'Codec' but is generic — the Codec derive cannot encode a type parameter", type_name),
                        "The derive emits one encode/decode pair per type, and a type parameter has no wire form. Hand-write encode/decode for this type, or wrap each concrete instantiation in its own `: Codec` type.".to_string(),
                        format!("type {} : Codec", type_name),
                    ).with_code("E023"));
                }
                continue;
            }
            for (field_name, field_ty) in &slots {
                let site = DeriveSite { type_name: *type_name, proto: *proto };
                validate_derive_field(env, diagnostics, &mut reported, site, field_name, field_ty);
            }
        }
    }
}
/// Per-field derive-support check for a single (type, protocol, field) triple: the Codec-unsupported-container check, plus recursively requiring every leaf nominal in the field's type to derive the same protocol. `reported` dedupes diagnostics across (type, protocol, leaf) triples seen by earlier fields/protocols/types in the caller's loop nest. Verbatim text move out of [`validate_derive_field_support`].
/// The (type, protocol) pair a field-recursive derive check is running for.
///
/// Both are `Sym`, so as adjacent positional parameters they could be
/// transposed silently — the check would then report the type as the protocol.
#[derive(Copy, Clone)]
struct DeriveSite {
    type_name: Sym,
    proto: Sym,
}

fn validate_derive_field(
    env: &TypeEnv, diagnostics: &mut Vec<Diagnostic>,
    reported: &mut std::collections::HashSet<(Sym, Sym, Sym)>,
    site: DeriveSite, field_name: &str, field_ty: &Ty,
) {
    let DeriveSite { type_name, proto } = site;
    // The protocol arrived twice — as a `Sym` and as its own `&str` — so the
    // two could be passed from different protocols at the call site. One name,
    // derived here.
    let p = proto.as_str();
    // #655: the Codec derive has no Map/Set arm — reject such a field here rather than emitting invalid Rust / silent-wrong wasm. Same E023 family (a field that cannot satisfy Codec).
    if p == "Codec" {
        if let Some(container) = codec_unsupported_container(field_ty) {
            if reported.insert((type_name, proto, sym(container))) {
                diagnostics.push(err(
                    format!("type '{}' derives 'Codec' but field '{}' has a '{}' type, which the Codec derive cannot encode",
                        type_name, field_name, container),
                    format!("The Codec derive serializes a {} as a String, which is wrong. Use a List[(K, V)] field (or List[T] for a Set), or implement encode/decode manually.",
                        container),
                    format!("type {} : Codec", type_name),
                ).with_code("E023"));
            }
        } else if !field_ty.contains_typevar() {
            if let Some((offender, hint)) = codec_field_shape_error(field_ty) {
                if reported.insert((type_name, proto, sym(&offender))) {
                    diagnostics.push(err(
                        format!("type '{}' derives 'Codec' but field '{}' has {}, which the Codec derive cannot encode",
                            type_name, field_name, offender),
                        hint,
                        format!("type {} : Codec", type_name),
                    ).with_code("E023"));
                }
            }
        }
    }
    let mut leaves = Vec::new();
    collect_leaf_nominals(field_ty, &mut leaves);
    for leaf in leaves {
        if leaf == type_name { continue; }          // self-reference is fine
        if !env.types.contains_key(&leaf) { continue; } // not a user nominal → native support
        if leaf_satisfies(env, leaf, p) { continue; }
        if !reported.insert((type_name, proto, leaf)) { continue; }
        diagnostics.push(err(
            format!("type '{}' derives '{}' but field '{}' has type '{}', which does not derive '{}'",
                type_name, p, field_name, leaf, p),
            format!("Add `: {}` to the declaration of type '{}' (every field of a `: {}` type must itself be `{}`)",
                p, leaf, p, p),
            format!("type {} : {}", type_name, p),
        ).with_code("E023"));
    }
}
/// Validate that types declaring `: ProtocolName` have all required convention methods, AND that each present method's signature actually matches the protocol's declared signature (arity, parameter types, return type) — `Self` substituted for the declaring type. Called after all declarations are registered so all `Type.method` functions are available. Signature checking is skipped for generic types (`contains_typevar`): `Self` would need to carry the type's own type arguments (e.g. `Self` for `Container[X]` must resolve to `Container[X]`, not bare `Container`), and nothing currently threads a user type's declared generic parameters back in here. Presence is still required either way.
pub fn validate_protocol_impls(env: &TypeEnv, diagnostics: &mut Vec<Diagnostic>) {
    let type_protocols: Vec<(Sym, Vec<Sym>)> = env.type_protocols.iter()
        .map(|(ty, protos)| (*ty, protos.iter().copied().collect()))
        .collect();

    for (type_name, protocol_names) in &type_protocols {
        // A BARE name that mirrors a prefixed type is a dual-registration
        // alias, and the last module to register wins that slot. Validating
        // through it compared one module's method against another module's
        // fields, and filed the mismatch against whichever file happened to be
        // under inference. The qualified entry is validated instead (#1087).
        if env.prefixed_bare_aliases.contains(type_name) {
            continue;
        }
        let is_generic = env.types.get(type_name).is_some_and(|t| t.contains_typevar());
        let type_ty = Ty::Named(*type_name, vec![]);

        for proto_name in protocol_names {
            let proto_def = match env.protocols.get(proto_name) {
                Some(p) => p.clone(),
                None => continue,
            };

            for method_sig in &proto_def.methods {
                let target = ImplTarget { name: *type_name, is_generic, ty: &type_ty };
                validate_protocol_method_impl(env, diagnostics, &target, *proto_name, method_sig);
            }
        }
    }
}
/// Validate a single protocol method's implementation on `type_name`: presence (missing-method E023 unless it's a builtin-derived protocol), then — for non-generic types — arity, parameter types, and return type against the protocol's declared signature (`Self` substituted for `type_ty`). Verbatim text move out of [`validate_protocol_impls`].
/// The type a protocol implementation is being validated on.
///
/// `is_generic` gates the signature comparison: a generic type's method
/// signature carries unsubstituted type vars, so comparing it against the
/// protocol's declaration would report spurious mismatches.
struct ImplTarget<'a> {
    name: Sym,
    is_generic: bool,
    ty: &'a Ty,
}

fn validate_protocol_method_impl(
    env: &TypeEnv, diagnostics: &mut Vec<Diagnostic>,
    target: &ImplTarget<'_>, proto_name: Sym,
    method_sig: &crate::types::ProtocolMethodSig,
) {
    let ImplTarget { name: type_name, is_generic, ty: type_ty } = *target;
    // A derived method is keyed bare while an explicit one is keyed prefixed,
    // so the key has to be resolved rather than assumed (#1087).
    let fn_key = super::registration::convention_fn_key(env, &type_name.to_string(), &method_sig.name.to_string())
        .map_or_else(|| format!("{}.{}", type_name, method_sig.name), |k| k.to_string());
    let Some(sig) = env.functions.get(&sym(&fn_key)) else {
        let is_builtin = matches!(proto_name.as_str(),
            "Eq" | "Repr" | "Ord" | "Hash" | "Codec" | "Encode" | "Decode"
            | "Numeric");
        if !is_builtin {
            diagnostics.push(err(
                format!("type '{}' declares protocol '{}' but missing method '{}'",
                    type_name, proto_name, method_sig.name),
                format!("Add: fn {}.{}({}) -> {}",
                    type_name, method_sig.name,
                    method_sig.params.iter()
                        .map(|(n, t)| {
                            let display_ty = if *t == Ty::TypeVar(sym("Self")) {
                                type_name.to_string()
                            } else {
                                t.display()
                            };
                            format!("{}: {}", n, display_ty)
                        })
                        .collect::<Vec<_>>()
                        .join(", "),
                    {
                        let ret = &method_sig.ret;
                        if *ret == Ty::TypeVar(sym("Self")) {
                            type_name.to_string()
                        } else {
                            ret.display()
                        }
                    }),
                format!("type {} : {}", type_name, proto_name),
            ));
        }
        return;
    };
    if is_generic {
        return;
    }

    let expected_params: Vec<Ty> = method_sig.params.iter()
        .map(|(_, ty)| env.resolve_named(&substitute_self(ty, type_ty)))
        .collect();
    let expected_ret = env.resolve_named(&substitute_self(&method_sig.ret, type_ty));
    let actual_params: Vec<Ty> = sig.params.iter().map(|(_, t)| env.resolve_named(t)).collect();
    let actual_ret = env.resolve_named(&sig.ret);

    if actual_params.len() != expected_params.len() {
        diagnostics.push(err(
            format!("method '{}' on type '{}' has {} parameter(s), expected {} to satisfy protocol '{}'",
                method_sig.name, type_name, actual_params.len(), expected_params.len(), proto_name),
            format!("Protocol '{}' defines: fn {}({})", proto_name, method_sig.name,
                method_sig.params.iter().map(|(n, t)| {
                    format!("{}: {}", n, substitute_self(t, type_ty).display())
                }).collect::<Vec<_>>().join(", ")),
            format!("fn {}.{}", type_name, method_sig.name),
        ));
        return;
    }
    for (i, (actual_ty, expected_ty)) in actual_params.iter().zip(expected_params.iter()).enumerate() {
        let matches = strip_module_qualifier(actual_ty) == strip_module_qualifier(expected_ty);
        if !matches && *expected_ty != Ty::Unknown && *actual_ty != Ty::Unknown {
            let param_name = &sig.params[i].0;
            diagnostics.push(err(
                format!("method '{}.{}' parameter '{}' has type '{}', expected '{}' to satisfy protocol '{}'",
                    type_name, method_sig.name, param_name, actual_ty.display(), expected_ty.display(), proto_name),
                format!("Change type to '{}'", expected_ty.display()),
                format!("fn {}.{}", type_name, method_sig.name),
            ));
        }
    }
    let ret_matches = strip_module_qualifier(&actual_ret) == strip_module_qualifier(&expected_ret);
    if !ret_matches && expected_ret != Ty::Unknown && actual_ret != Ty::Unknown {
        diagnostics.push(err(
            format!("method '{}.{}' returns '{}', expected '{}' to satisfy protocol '{}'",
                type_name, method_sig.name, actual_ret.display(), expected_ret.display(), proto_name),
            format!("Change return type to '{}'", expected_ret.display()),
            format!("fn {}.{}", type_name, method_sig.name),
        ));
    }
}
