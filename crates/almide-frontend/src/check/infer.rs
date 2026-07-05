/// Expression type inference — Pass 1 of the constraint-based checker.
/// Walks the AST, populates TypeMap (ExprId→Ty), collects constraints.

use almide_lang::ast;
use almide_lang::ast::ExprKind;
use almide_base::intern::{Sym, sym};
use crate::types::{Ty, TypeConstructorId, VariantPayload};
use super::types::{resolve_ty, FixHint, IfArm};
use super::Checker;

impl Checker {
    pub(crate) fn infer_expr(&mut self, expr: &mut ast::Expr) -> Ty {
        if let Some(span) = expr.span {
            self.current_span = Some(span);
        }
        // #626: register a candidate for any int literal that overflows i64. The
        // actual range error is decided post-solve against context (a wider
        // annotation or unary negation may make it valid) — see
        // `validate_int_overflow_literals`. Registering here (not in the Int arm)
        // keeps `expr.id` / `expr.span` in scope.
        if let ExprKind::Int { raw, .. } = &expr.kind {
            if super::int_literal_overflows_i64(raw) {
                self.deferred_int_overflow_checks.push(super::IntOverflowSite {
                    expr_id: expr.id, raw: raw.clone(), negated: false, context_ty: None, span: expr.span,
                });
            }
        }
        let ity = self.infer_expr_inner(expr);
        self.type_map.insert(expr.id, ity.clone());
        ity
    }

    fn infer_expr_inner(&mut self, expr: &mut ast::Expr) -> Ty {
        // #488: a paren call on a record type or record-payload constructor is
        // either NORMALIZED into the brace Record pipeline (all-named args) or
        // rejected with E021 — it must never fall through the generic Call
        // path, which has no field identity and silently dropped named args.
        if matches!(&expr.kind, ExprKind::Call { .. }) && self.normalize_ctor_paren_call(expr) {
            return self.infer_expr_inner(expr);
        }
        // Behavior-preserving split: the giant match is partitioned into three
        // DISJOINT groups over `&mut expr.kind`. Groups 2 and 3 live in
        // `infer_p2.rs` / `infer_p3.rs` (sub-methods returning `Option<Ty>`,
        // `Some(_)` exactly for their own variants). The chain is order-
        // independent — every `ExprKind` variant matches exactly one group — so
        // dispatching to them first, then the group-1 arms below, is identical
        // to the original single match. Group 1 keeps the arms whose bodies
        // early-`return` (TypeName / Record / Member / TupleIndex /
        // OptionalChain), which a wrapper cannot factor out.
        if let Some(t) = self.infer_expr_inner_g2(expr) { return t; }
        if let Some(t) = self.infer_expr_inner_g3(expr) { return t; }
        match &mut expr.kind {
            ExprKind::TypeName { name, .. } => {
                // Const param reference: `N` where `N: Int` is a compile-time value param
                if let Some(Ty::ConstParam { ty, .. }) = self.env.types.get(&sym(name)).cloned() {
                    return *ty;
                }
                if let Some((type_name, case)) = self.env.lookup_ctor_in(&sym(name), self.current_module_prefix.as_deref()) {
                    self.report_ambiguous_ctor(name);
                    match &case.payload {
                        VariantPayload::Tuple(tys) if !tys.is_empty() => {
                            // Constructor with payload used as value → function type
                            let generic_args = self.instantiate_type_generics(type_name.as_str());
                            let ret = Ty::Named(type_name, generic_args.clone());
                            let params = if generic_args.is_empty() {
                                tys.clone()
                            } else {
                                // Substitute TypeVars with fresh inference vars
                                if let Some(ty_def) = self.env.types.get(&type_name).cloned() {
                                    let mut type_var_names = Vec::new();
                                    crate::types::TypeEnv::collect_typevars(&ty_def, &mut type_var_names);
                                    let subst: std::collections::HashMap<Sym, Ty> = type_var_names.iter()
                                        .zip(generic_args.iter())
                                        .map(|(tv, fresh)| (*tv, fresh.clone()))
                                        .collect();
                                    tys.iter().map(|t| super::calls::subst_ty(t, &subst)).collect()
                                } else {
                                    tys.clone()
                                }
                            };
                            Ty::Fn { params, ret: Box::new(ret) }
                        }
                        _ => Ty::Named(type_name, vec![])
                    }
                }
                else if let Some(ty) = self.env.top_lets.get(&sym(name)).cloned() { ty }
                else { Ty::Named(sym(name), vec![]) }
            }
            ExprKind::Record { name, fields, .. } => {
                for f in fields.iter_mut() { self.infer_expr(&mut f.value); }
                if let Some(n) = name {
                    // A qualified record-variant name (`mod.Ctor { … }`) keys the
                    // constructor table by its BARE name, so strip any module prefix
                    // before a ctor lookup — otherwise a cross-module record-variant
                    // is mis-typed as a standalone `mod.Ctor` type (#412).
                    let ctor_sym = n.rsplit_once('.').map(|(_, b)| sym(b)).unwrap_or_else(|| sym(n));
                    // Constructing `EnumType { field: ... }` via the ENUM type
                    // name (not a case name) is a category error: an enum has no
                    // fields of its own. Native rustc would leak E0574 and WASM
                    // silently mis-constructs, so reject it here with a proper
                    // diagnostic that lists the available record-variant cases.
                    if !self.env.constructors.contains_key(&ctor_sym) {
                        if let Some(Ty::Variant { cases, .. }) = self.env.types.get(&sym(n)) {
                            let record_cases: Vec<&str> = cases.iter()
                                .filter(|c| matches!(c.payload, VariantPayload::Record(_)))
                                .map(|c| c.name.as_str())
                                .collect();
                            let hint = if record_cases.is_empty() {
                                format!("`{}` is an enum type; none of its cases take named fields. Construct a case directly, e.g. `{}::SomeCase(...)`.", n, n)
                            } else {
                                format!("`{}` is an enum type, not a record. Construct a case instead: {}.",
                                    n,
                                    record_cases.iter().map(|c| format!("`{} {{ ... }}`", c)).collect::<Vec<_>>().join(" or "))
                            };
                            self.emit(super::err(
                                format!("cannot construct enum type '{}' with record syntax", n),
                                hint,
                                format!("record literal {}", n),
                            ).with_code("E017"));
                            return Ty::Unknown;
                        }
                    }
                    // Constrain each provided field value to its DECLARED field
                    // type (with the parent type's generics instantiated to fresh
                    // vars). This is the record-literal analogue of the tuple
                    // payload unification in `check_call_with_type_args`: it pins
                    // an otherwise-unconstrained field value — e.g. an empty `[]`
                    // assigned to a `List[Shape]` field resolves its element to
                    // `Shape`, so the value is concrete (no spurious E018) and the
                    // IR carries the real type. Field-COUNT / name validation stays
                    // wherever it already lives; this only adds the type flow.
                    //
                    // Two declaration sources: a record-VARIANT case
                    // (`| Group { items: List[Shape] }`, found in `constructors`)
                    // and a bare NAMED RECORD type (`type WithList = { items:
                    // List[Int] }`, resolved from `env.types`). Both reduce to a
                    // `(field, declared_ty)` list with generics already substituted.
                    let (result_ty, decl_fields, closed, defaults): (Ty, Vec<(Sym, Ty)>, bool, std::collections::HashSet<Sym>) =
                        // #631: module-aware lookup so a BARE record-variant ctor
                        // used INSIDE its owning submodule (`Circle { radius: r }`)
                        // pins `type_name` to the owner-qualified `mod.Shape`,
                        // matching the tuple-ctor path. Without this the bare result
                        // type tripped the #433 name-pinning guard at codegen.
                        if let Some((type_name, case)) = self.env.lookup_ctor_in(&ctor_sym, self.current_module_prefix.as_deref()) {
                            // Brace construction of a NON-record case is a
                            // category error (`Wrap { x: 1 }` on a tuple case):
                            // reject here, or rustc/wasm explode downstream.
                            if !matches!(case.payload, crate::types::VariantPayload::Record(_)) {
                                self.emit(super::err(
                                    format!("case '{}' does not take named fields", n),
                                    format!("'{}' is a tuple or unit case — construct it positionally: `{}(...)`", n, n),
                                    format!("record literal {}", n),
                                ).with_code("E021"));
                                return Ty::Unknown;
                            }
                            let generic_args = self.instantiate_type_generics(type_name.as_str());
                            let subst: std::collections::HashMap<Sym, Ty> = if !generic_args.is_empty() {
                                self.env.types.get(&type_name).cloned().map(|ty_def| {
                                    let mut tv_names = Vec::new();
                                    crate::types::TypeEnv::collect_typevars(&ty_def, &mut tv_names);
                                    tv_names.iter().zip(generic_args.iter())
                                        .map(|(tv, fresh)| (*tv, fresh.clone())).collect()
                                }).unwrap_or_default()
                            } else { std::collections::HashMap::new() };
                            let decl = match &case.payload {
                                crate::types::VariantPayload::Record(fs) =>
                                    fs.iter().map(|(fname, fty)| (*fname, super::calls::subst_ty(fty, &subst))).collect(),
                                _ => Vec::new(),
                            };
                            // #433: a qualified record-variant `mod.Ctor { … }` takes
                            // the namespaced `mod.Type` so it mangles to the right enum.
                            let result_named = match n.as_str().rsplit_once('.') {
                                Some((m, _)) => {
                                    let rm = self.env.import_table.resolve(m).map(|s| s.to_string()).unwrap_or_else(|| m.to_string());
                                    let q = format!("{}.{}", rm, type_name.as_str());
                                    if self.env.types.contains_key(&sym(&q)) { sym(&q) } else { type_name }
                                }
                                None => type_name,
                            };
                            let case_defaults = self.env.ctor_field_defaults.get(&ctor_sym).cloned().unwrap_or_default();
                            (Ty::Named(result_named, generic_args), decl, true, case_defaults)
                        } else {
                            // Named record type: instantiate its generics with
                            // fresh vars so the declared field types carry the
                            // SAME vars as the result type (so e.g. `List[T]`
                            // unifies across the field and the binding's ascription).
                            //
                            // #433: the constructed type's NAME must be the
                            // canonical qualified `mod.Type`, like the variant
                            // branch above and annotation resolution. This was
                            // the one producer still leaking bare cross-module
                            // names — a module's record top-let carried bare
                            // `Cfg` into IrTopLet.ty, rendering an unmangled
                            // static type on native (E0425) and missing the
                            // qualified record_fields key on wasm (trap).
                            let canon = match n.rsplit_once('.') {
                                // `alias.Cfg { … }`: resolve the import alias to
                                // the real module, keep qualified if registered.
                                Some((m, base)) => {
                                    let rm = self.env.import_table.resolve(m).map(|s| s.to_string()).unwrap_or_else(|| m.to_string());
                                    let q = format!("{}.{}", rm, base);
                                    if self.env.types.contains_key(&sym(&q)) { sym(&q) } else { sym(n) }
                                }
                                None => crate::canonicalize::resolve::canonical_user_type_sym(
                                    n, &self.env.types, self.current_module_prefix.as_deref(),
                                ).unwrap_or_else(|| sym(n)),
                            };
                            let generic_args = self.instantiate_type_generics(n);
                            let named = Ty::Named(canon, generic_args);
                            let (decl, closed) = match self.env.resolve_named(&named) {
                                Ty::Record { fields } => (fields, true),
                                Ty::OpenRecord { fields } => (fields, false),
                                _ => (Vec::new(), false),
                            };
                            let defaults = self.env.record_field_defaults.get(&canon)
                                .or_else(|| self.env.record_field_defaults.get(&sym(n)))
                                .cloned().unwrap_or_default();
                            (named, decl, closed, defaults)
                        };
                    // #488: field-set validation — duplicates always; unknown
                    // and missing-without-default for closed declarations.
                    if closed || !decl_fields.is_empty() {
                        let given = fields.clone();
                        self.validate_record_fields(n.as_str(), &given, &decl_fields, closed, &defaults);
                    }
                    for f in fields.iter() {
                        if let Some((_, ety)) = decl_fields.iter().find(|(fname, _)| fname.as_str() == f.name.as_str()) {
                            if let Some(vty) = self.type_map.get(&f.value.id).cloned() {
                                self.constrain(ety.clone(), vty, format!("field {}", f.name));
                            }
                        }
                    }
                    result_ty
                }
                else {
                    let field_tys: Vec<(Sym, Ty)> = fields.iter().map(|f| {
                        let ty = self.type_map.get(&f.value.id).map(|it| resolve_ty(it, &self.uf)).unwrap_or(Ty::Unknown);
                        (sym(&f.name), ty)
                    }).collect();
                    Ty::Record { fields: field_tys }
                }
            }
            ExprKind::Member { object, field, .. } => {
                // `infer_expr(object)` below overwrites `current_span`
                // with the object's range, so capture the Member expr's
                // own span now. E013 uses it to position the
                // `try_replace` rewrite that covers `object.field`.
                let member_span = self.current_span;
                // Module function used as a first-class value: `string.len`,
                // `list.map`, etc. Detect this BEFORE inferring the object
                // (which would fail because `string` is not a variable) and
                // return the function's type signature.
                if let ExprKind::Ident { name: mod_name, .. } = &object.kind {
                    if let Some(sig) = crate::stdlib::lookup_sig(mod_name, field) {
                        self.type_map.insert(object.id, Ty::Unit); // placeholder; object isn't evaluated
                        return Ty::Fn {
                            params: sig.params.iter().map(|(_, t)| t.clone()).collect(),
                            ret: Box::new(sig.ret.clone()),
                        };
                    }
                    let resolved_mod_name = self.env.import_table.resolve(mod_name)
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| mod_name.to_string());
                    let key = format!("{}.{}", resolved_mod_name, field);
                    if let Some(sig) = self.env.functions.get(&sym(&key)).cloned() {
                        self.type_map.insert(object.id, Ty::Unit);
                        self.env.import_table.mark_used(mod_name);
                        return Ty::Fn {
                            params: sig.params.iter().map(|(_, t)| t.clone()).collect(),
                            ret: Box::new(sig.ret.clone()),
                        };
                    }
                    // Cross-module top-level `let` access: `utils.CATEGORY_ORDER`.
                    // Spec Visibility section applies to fn, type, AND let.
                    if let Some(let_ty) = self.env.top_lets.get(&sym(&key)).cloned() {
                        self.type_map.insert(object.id, Ty::Unit);
                        self.env.import_table.mark_used(mod_name);
                        return let_ty;
                    }
                    // Cross-module variant constructor as value: dispatch.Never, binary.ImportFunc
                    if let Some((type_name, case)) = self.env.lookup_ctor(&sym(field)) {
                        let resolved_mod = self.env.import_table.resolve(mod_name)
                            .unwrap_or(sym(mod_name));
                        let qualified = format!("{}.{}", resolved_mod.as_str(), type_name.as_str());
                        if self.env.types.contains_key(&sym(&qualified)) {
                            self.type_map.insert(object.id, Ty::Unit);
                            let generic_args = self.instantiate_type_generics(type_name.as_str());
                            // #433: return the qualified `mod.Type` (it exists and was
                            // just confirmed) so the binding mangles to the namespaced
                            // struct, not the ambiguous bare name.
                            let qual_ty = sym(&qualified);
                            return match &case.payload {
                                VariantPayload::Unit => Ty::Named(qual_ty, generic_args),
                                VariantPayload::Tuple(param_tys) => Ty::Fn {
                                    params: param_tys.clone(),
                                    ret: Box::new(Ty::Named(qual_ty, generic_args)),
                                },
                                VariantPayload::Record(_) => Ty::Named(qual_ty, generic_args),
                            };
                        }
                    }
                }
                let obj_ty = self.infer_expr(object);
                let concrete = resolve_ty(&obj_ty, &self.uf);
                let field_ty = self.resolve_field_type(&concrete, field);
                // Almide-friendly diagnostic for list / string field access:
                // LLMs trained on Haskell / Python / Ruby write `xs.head`,
                // `xs.tail`, `xs.length`, `s.length`. In Almide these are
                // stdlib calls — intercept here so rustc never leaks
                // `error[E0609]: no field 'head' on type 'Vec<i64>'`.
                if matches!(field_ty, Ty::Unknown) {
                    // (field → (module_fn, args_template, display_suffix))
                    // `args_template` is a tiny `("{0}", 1)`-style mini-
                    // language: `{0}` is substituted with the object's
                    // source slice; any trailing text goes verbatim.
                    // `display_suffix` is comment-only info shown after
                    // the mechanical replacement (e.g. the Option[T]
                    // reminder for `head`).
                    let module_and_subs: Option<(&str, Vec<(&str, &str, &str, &str)>)> = match &concrete {
                        Ty::Applied(TypeConstructorId::List, _) => Some(("list", vec![
                            ("head",   "list.first", "({0})", "  // returns Option[T]"),
                            ("tail",   "list.drop",  "({0}, 1)", ""),
                            ("length", "list.len",   "({0})", ""),
                            ("len",    "list.len",   "({0})", ""),
                            ("first",  "list.first", "({0})", ""),
                            ("last",   "list.last",  "({0})", ""),
                            ("size",   "list.len",   "({0})", ""),
                        ])),
                        Ty::String => Some(("string", vec![
                            ("length", "string.len",      "({0})", ""),
                            ("len",    "string.len",      "({0})", ""),
                            ("size",   "string.len",      "({0})", ""),
                            ("chars",  "string.to_chars", "({0})", ""),
                        ])),
                        _ => None,
                    };
                    if let Some((module, subs)) = module_and_subs {
                        let matched = subs.iter().find(|(n, _, _, _)| n == field).cloned();
                        let hint = if matched.is_some() {
                            format!(
                                "Almide values have no fields — use the `{m}` stdlib module. No method-call or field-access syntax is supported.",
                                m = module
                            )
                        } else {
                            format!(
                                "Almide values have no fields. Use `{m}.<fn>(x)` (or `x |> {m}.<fn>`) — see docs/stdlib/{m}.md for available functions.",
                                m = module
                            )
                        };
                        let mut diag = super::err(
                            format!("no field '{}' on {}", field, module),
                            hint,
                            format!("field access .{}", field),
                        ).with_code("E013");
                        if let Some((_, fn_name, args_tpl, _display_suffix)) = matched {
                            // Mechanical rewrite: substitute the object's
                            // source text into `args_tpl`. `member_span`
                            // now covers the full `object.field` (parser
                            // upgrade from the E002 arc), so replacing
                            // that range leaves the surrounding source
                            // intact. Falls back to a display-only
                            // snippet when source text isn't available.
                            let rewrite = object.span
                                .and_then(|s| self.source_slice(s))
                                .and_then(|obj_src| {
                                    let span = member_span?;
                                    let args = args_tpl.replace("{0}", &obj_src);
                                    Some((span, format!("{}{}", fn_name, args)))
                                });
                            if let Some((span, snippet)) = rewrite {
                                diag = diag.with_try_replace(
                                    span.line, span.col, span.end_col,
                                    snippet,
                                );
                            } else {
                                let display = format!(
                                    "{}{}{}",
                                    fn_name,
                                    args_tpl.replace("{0}", "xs"),
                                    _display_suffix,
                                );
                                diag = diag.with_try(display);
                            }
                        }
                        self.emit(diag);
                    }
                }
                field_ty
            }
            ExprKind::TupleIndex { object, index, .. } => {
                let obj_ty = self.infer_expr(object);
                if let Ty::Tuple(elems) = &obj_ty {
                    if *index < elems.len() { return elems[*index].clone(); }
                }
                let concrete = resolve_ty(&obj_ty, &self.uf);
                match &concrete {
                    Ty::Tuple(elems) if *index < elems.len() => elems[*index].clone(),
                    // Object's type is still an open inference var (e.g. a
                    // fresh lambda param yet to be bound by its call site).
                    // Park a fresh result var and resolve it once the
                    // union-find binds the object to a concrete `Tuple`
                    // (see `Checker::resolve_deferred_tuple_indices`).
                    // Without this deferral the body type freezes to
                    // `Unknown` here and propagates outward — breaking
                    // chains like `xs |> list.map((p) => p.1) |>
                    // list.fold(0.0, (a, b) => a + b)` where the fold's
                    // element-typed lambda param gets no constraint.
                    Ty::TypeVar(name) if name.starts_with('?') => {
                        let result = self.fresh_var();
                        self.deferred_tuple_indices.push((obj_ty, *index, result.clone()));
                        result
                    }
                    _ => Ty::Unknown,
                }
            }
            ExprKind::OptionalChain { expr: inner, field, .. } => {
                let t = self.infer_expr(inner);
                let resolved = resolve_ty(&t, &self.uf);
                let inner_ty = if let Some(ty) = resolved.option_inner() {
                    ty
                } else if matches!(&resolved, Ty::Unknown | Ty::TypeVar(_)) {
                    return self.fresh_var();
                } else {
                    self.emit(super::err(
                        format!("operator '?.' requires Option type but got {}", resolved.display()),
                        "Use '?.' only on Option[T] values",
                        "operator ?.",
                    ));
                    return Ty::Unknown;
                };
                // Resolve field type from inner_ty
                match &inner_ty {
                    Ty::Record { fields } | Ty::OpenRecord { fields } => {
                        if let Some((_, field_ty)) = fields.iter().find(|(n, _)| n == field) {
                            Ty::option(field_ty.clone())
                        } else {
                            self.emit(super::err(
                                format!("field '{}' not found on type {}", field, inner_ty.display()),
                                "Check the field name",
                                format!("field {}", field),
                            ));
                            Ty::Unknown
                        }
                    }
                    _ => {
                        let field_ty = self.resolve_field_type(&inner_ty, field);
                        if !matches!(field_ty, Ty::Unknown) {
                            Ty::option(field_ty)
                        } else {
                            self.emit(super::err(
                                format!("cannot access field '{}' on type {}", field, inner_ty.display()),
                                "Optional chaining requires a record type inside Option",
                                format!("field {}", field),
                            ));
                            Ty::Unknown
                        }
                    }
                }
            }
            // Every other `ExprKind` variant is handled by `infer_expr_inner_g2`
            // / `infer_expr_inner_g3` above (the partition is total), so this is
            // genuinely unreachable for any well-formed AST.
            _ => unreachable!("infer_expr_inner: ExprKind variant not in any group"),
        }
    }
}

include!("infer_p2.rs");
include!("infer_p3.rs");
include!("infer_p4.rs");
include!("infer_p5.rs");
