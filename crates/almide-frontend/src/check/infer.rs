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
        match &mut expr.kind {
            ExprKind::Int { .. } => Ty::Int,
            ExprKind::Float { .. } => Ty::Float,
            ExprKind::String { .. } => Ty::String,
            ExprKind::InterpolatedString { parts, .. } => {
                for part in parts.iter_mut() {
                    if let ast::StringPart::Expr { expr } = part {
                        self.infer_expr(expr);
                    }
                }
                Ty::String
            }
            ExprKind::Bool { .. } => Ty::Bool,
            ExprKind::Unit => Ty::Unit,

            ExprKind::None => Ty::option(self.fresh_var()),

            ExprKind::Ident { name, .. } => {
                self.env.used_vars.insert(sym(name));
                if let Some(ty) = self.env.lookup_var(name).cloned() { self.instantiate_ty(&ty) }
                else if let Some(ty) = self.env.top_lets.get(&sym(name)).cloned() { self.instantiate_ty(&ty) }
                // Const param: `N: Int` in generic params resolves to its underlying type
                else if let Some(Ty::ConstParam { ty, .. }) = self.env.types.get(&sym(name)).cloned() {
                    *ty
                }
                else if let Some(sig) = self.env.functions.get(&sym(name)).cloned() {
                    Ty::Fn {
                        params: sig.params.iter().map(|(_, t)| t.clone()).collect(),
                        ret: Box::new(sig.ret.clone()),
                    }
                }
                else {
                    // Only suggest `import` for modules that require explicit import
                    // and whose names won't be confused with common variable names.
                    // e.g. `value`, `error`, `string`, `list` are too common as
                    // variable names — suggesting `import value` is misleading.
                    let (hint, fix): (String, Option<String>) = if crate::stdlib::is_import_suggestable(name) {
                        let desc = crate::stdlib::module_description(name);
                        (format!("Add `import {}` (stdlib: {})\nOr run `almide fmt` to auto-add missing imports", name, desc),
                         Some(format!("import {}", name)))
                    } else {
                        let candidates = self.env.all_visible_names();
                        if let Some(suggestion) = almide_base::diagnostic::suggest(name, candidates.iter().map(|s| s.as_str())) {
                            (format!("Did you mean `{}`?", suggestion), Some(suggestion.to_string()))
                        } else {
                            ("Check the variable name".to_string(), None)
                        }
                    };
                    let mut diag = super::err(format!("undefined variable '{}'", name), hint, format!("variable {}", name)).with_code("E003");
                    if let Some(fix) = fix {
                        if let Some(stripped) = fix.strip_prefix("import ") {
                            // Zero-width insert at the top of file — the
                            // new `import <module>\n` line is prepended.
                            // `apply_try_to` handles `end_col == col` as
                            // an insertion point.
                            diag = diag.with_try_replace(
                                1, 1, 1,
                                format!("import {}\n", stripped),
                            );
                        } else if let Some(span) = self.current_span {
                            // Typo fuzzy suggestion: replace the
                            // offending identifier with the suggested name.
                            diag = diag.with_try_replace(
                                span.line, span.col, span.end_col,
                                fix,
                            );
                        } else {
                            diag = diag.with_try(format!("// {}  →  {}\n{}", name, fix, fix));
                        }
                    }
                    self.emit(diag);
                    Ty::Unknown
                }
            }

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

            ExprKind::List { elements, .. } => {
                if elements.is_empty() {
                    let ty = Ty::list(self.fresh_var());
                    self.register_empty_collection(ty.clone(), super::EmptyCollectionKind::ListLiteral);
                    ty
                }
                else {
                    let first = self.infer_expr(&mut elements[0]);
                    for elem in elements.iter_mut().skip(1) { let et = self.infer_expr(elem); self.constrain(first.clone(), et, "list element"); }
                    Ty::list(first)
                }
            }

            ExprKind::Tuple { elements, .. } => Ty::Tuple(elements.iter_mut().map(|e| self.infer_expr(e)).collect()),

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
                        if let Some((type_name, case)) = self.env.lookup_ctor(&ctor_sym) {
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

            ExprKind::SpreadRecord { base, fields, .. } => {
                let base_ty = self.infer_expr(base);
                for f in fields.iter_mut() { self.infer_expr(&mut f.value); }
                base_ty
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

            ExprKind::IndexAccess { object, index, .. } => {
                let obj_ty = self.infer_expr(object);
                self.infer_expr(index);
                let is_range = matches!(&index.kind, ExprKind::Range { .. });
                let concrete = resolve_ty(&obj_ty, &self.uf);
                if is_range {
                    concrete
                } else {
                    match &concrete {
                        Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => args[0].clone(),
                        Ty::Applied(TypeConstructorId::Map, args) if args.len() == 2 => Ty::option(args[1].clone()),
                        Ty::Bytes => Ty::Int,
                        _ => Ty::Unknown,
                    }
                }
            }

            ExprKind::Binary { op, left, right, .. } => {
                let lt = self.infer_expr(left);
                let rt = self.infer_expr(right);
                match op.as_str() {
                    "+" => {
                        let lc = resolve_ty(&lt, &self.uf);
                        let rc = resolve_ty(&rt, &self.uf);
                        self.infer_plus_op(&lc, &rc, lt)
                    }
                    "-" | "*" | "/" | "%" | "^" => {
                        let lc = resolve_ty(&lt, &self.uf);
                        let rc = resolve_ty(&rt, &self.uf);
                        // Matrix operators: *, +, - on Matrix types
                        if lc == Ty::Matrix || rc == Ty::Matrix {
                            Ty::Matrix
                        } else {
                            // Sized Numeric Types (Stage 1c): same-width
                            // arithmetic accepts every sized numeric variant.
                            let is_numeric = |t: &Ty| matches!(
                                t,
                                Ty::Int | Ty::Float | Ty::Unknown | Ty::TypeVar(_)
                                    | Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64
                                    | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
                                    | Ty::Float32 | Ty::Float64
                                    | Ty::Matrix
                                    // GPU vector/matrix types (Vec2, Vec3, Vec4, Mat3, Mat4)
                                    // support arithmetic ops; emitted as WGSL builtins.
                                    | Ty::Named(..)
                            );
                            let is_sized_scalar = |t: &Ty| matches!(
                                t,
                                Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64
                                    | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
                                    | Ty::Float32 | Ty::Float64
                            );
                            if !is_numeric(&lc) || !is_numeric(&rc) {
                                self.emit(super::err(
                                    format!("operator '{}' requires numeric types but got {} and {}", op, lc.display(), rc.display()),
                                    "Use numeric types (Int or Float)", format!("operator {}", op)));
                            }
                            // Stage 1c: reject mixed-sized-width arithmetic.
                            // See `infer_plus_op` for rationale.
                            if is_sized_scalar(&lc) && is_sized_scalar(&rc) && lc != rc {
                                self.emit(super::err(
                                    format!(
                                        "operator '{}' mixes sized numeric types {} and {} — \
                                         explicit conversion required (e.g. `.to_{}()`)",
                                        op, lc.display(), rc.display(),
                                        lc.display().to_lowercase()),
                                    "Convert one side with `.to_intN()` / `.to_floatN()` before the op",
                                    format!("operator {}", op)));
                                lc
                            } else if lc.compatible(&rc) && is_sized_scalar(&lc) {
                                lc
                            } else if lc == Ty::Float || rc == Ty::Float { Ty::Float } else { lt }
                        }
                    }
                    "++" => {
                        self.emit(super::err(
                            format!("operator '++' has been removed. Use '+' for concatenation"),
                            "Replace ++ with +", "operator ++"));
                        lt
                    }
                    "==" | "!=" | "<" | ">" | "<=" | ">=" => {
                        // Check none comparison: only valid with Option types
                        let left_is_none = matches!(left.kind, ExprKind::None);
                        let right_is_none = matches!(right.kind, ExprKind::None);
                        if right_is_none && !left_is_none {
                            let lc = resolve_ty(&lt, &self.uf);
                            if !lc.is_option() && !matches!(lc, Ty::Unknown | Ty::TypeVar(_)) {
                                self.emit(super::err(
                                    format!("cannot compare {} with none — only Option types support none comparison", lc.display()),
                                    "Use Option type or check with is_ok()/is_err() for Result", "comparison with none"));
                            }
                        }
                        if left_is_none && !right_is_none {
                            let rc = resolve_ty(&rt, &self.uf);
                            if !rc.is_option() && !matches!(rc, Ty::Unknown | Ty::TypeVar(_)) {
                                self.emit(super::err(
                                    format!("cannot compare none with {} — only Option types support none comparison", rc.display()),
                                    "Use Option type or check with is_ok()/is_err() for Result", "comparison with none"));
                            }
                        }
                        // Unify left/right types so TypeVars in none/err/constructors get resolved
                        self.unify_infer(&lt, &rt);
                        Ty::Bool
                    }
                    "and" | "or" => {
                        let lc = resolve_ty(&lt, &self.uf);
                        let rc = resolve_ty(&rt, &self.uf);
                        let is_bool = |t: &Ty| matches!(t, Ty::Bool | Ty::Unknown | Ty::TypeVar(_));
                        if !is_bool(&lc) {
                            self.emit(super::err(
                                format!("operator '{}' requires Bool but got {}", op, lc.display()),
                                "Use Bool values with logical operators", format!("operator {}", op)));
                        }
                        if !is_bool(&rc) {
                            self.emit(super::err(
                                format!("operator '{}' requires Bool but got {}", op, rc.display()),
                                "Use Bool values with logical operators", format!("operator {}", op)));
                        }
                        Ty::Bool
                    }
                    _ => lt,
                }
            }

            ExprKind::Unary { op, operand, .. } => {
                let t = self.infer_expr(operand);
                match op.as_str() { "not" => Ty::Bool, _ => t }
            }

            ExprKind::If { cond, then, else_, .. } => {
                self.infer_expr(cond);
                let then_ty = self.infer_expr(then);
                let else_ty = self.infer_expr(else_);
                // In effect fn bodies, auto-unwrap Result[T, E] → T per
                // branch before unifying them, mirroring the match-arm rule
                // (see ExprKind::Match above). Without this, an `if` whose
                // one branch is a `match` on an effect-fn call (auto-unwrapped
                // to T) and whose other branch is an explicit `ok(...)`
                // (stays Result[T, E]) fails E001 — the asymmetry is a
                // checker artefact, not a real type error: codegen's
                // wrap_tail_in_ok normalizes both to Result form. Scoped to
                // `auto_unwrap`, so pure-fn / test if/else are untouched.
                // Auto-unwrap Result[T, E] → T on BOTH branches for the
                // cross-branch COMPARISON only, then return the THEN branch's
                // real (non-unwrapped) type as the if-expression's type.
                //
                // Two requirements pull in opposite directions and this split
                // satisfies both:
                //   • M1 (E001): an `if` whose one branch is a `match` on an
                //     effect-fn call (auto-unwrapped to `T` inside the match)
                //     and whose other branch is an explicit `ok(...)`
                //     (`Result[T, E]`) must not error. Comparing both at the
                //     unwrapped `T` level removes the spurious asymmetry.
                //   • No-regress (`validate_positive`: `if .. then ok(n) else
                //     err(..)`): the if's TYPE must stay `Result[T, E]` so the
                //     WASM emitter sees the real value shape (the branches are
                //     genuine Result constructors). Returning the un-unwrapped
                //     `then_ty` preserves this; codegen's wrap_tail_in_ok then
                //     normalizes every branch to Result form regardless.
                // Scoped to `auto_unwrap`, so pure-fn / test if/else are
                // untouched (they keep the strict same-type rule).
                let cmp_unwrap = |t: &Ty, uf: &_| -> Ty {
                    match resolve_ty(t, uf) {
                        Ty::Applied(TypeConstructorId::Result, ref args) if args.len() == 2 => args[0].clone(),
                        _ => t.clone(),
                    }
                };
                let (cmp_then, cmp_else) = if self.env.auto_unwrap {
                    (cmp_unwrap(&then_ty, &self.uf), cmp_unwrap(&else_ty, &self.uf))
                } else {
                    (then_ty.clone(), else_ty.clone())
                };
                // Specialize the Unit-leak `try:` snippet: if an arm is a
                // bare assignment `x = ...` (returns Unit), we want to cite
                // the actual variable name in the suggested rewrite.
                let hint = if_arm_fix_hint(then, else_);
                self.constrain_with_hint(cmp_then, cmp_else, "if branches", hint);
                then_ty
            }

            ExprKind::Match { subject, arms, .. } => {
                let subject_ty = self.infer_expr(subject);
                let sc = resolve_ty(&subject_ty, &self.uf);
                self.check_match_exhaustiveness(&sc, arms);
                let mut arm_types = Vec::new();
                // Real (un-substituted) arm types, used to pick the overall match
                // result type. An `err(..)` arm produces a genuine `Result[T, E]`
                // value — it is NOT divergent — so even when every arm is `err`,
                // the match still has a concrete Result type (not `Never`).
                let mut arm_real_types = Vec::new();
                for arm in arms.iter_mut() {
                    self.env.push_scope();
                    let sub_c = resolve_ty(&subject_ty, &self.uf);
                    self.bind_pattern(&arm.pattern, &sub_c);
                    if let Some(ref mut guard) = arm.guard { self.infer_expr(guard); }
                    let arm_ty = self.infer_expr(&mut arm.body);
                    arm_real_types.push(arm_ty.clone());
                    // err() in a match arm is an early return — unify as Never
                    // so it doesn't constrain sibling arm types.
                    let arm_ty = if matches!(&arm.body.kind, ExprKind::Err { .. }) {
                        Ty::Never
                    } else if self.env.auto_unwrap {
                        // In effect fn bodies, auto-unwrap Result[T, E] → T
                        // so match arms mixing effect fn calls (Result) with
                        // pure expressions (T) unify correctly.
                        let resolved = resolve_ty(&arm_ty, &self.uf);
                        match resolved {
                            Ty::Applied(TypeConstructorId::Result, ref args) if args.len() == 2 => args[0].clone(),
                            _ => arm_ty,
                        }
                    } else {
                        arm_ty
                    };
                    arm_types.push(arm_ty);
                    self.env.pop_scope();
                }
                // Unify all arm types with each other (not with a shared result var
                // that can be contaminated by external constraints)
                if let Some(first) = arm_types.first().cloned() {
                    for aty in &arm_types[1..] {
                        self.constrain(first.clone(), aty.clone(), "match arm");
                    }
                    // The overall match type is the first non-`Never` arm type.
                    // `Never` arms (every `err(..)` arm) carry no useful result
                    // type but they DO produce a Result value, so when they are
                    // the only arms we recover the concrete type from the real
                    // (un-substituted) arm types — preferring an `err` arm's
                    // `Result[T, E]` so the match types as Result, never `Never`.
                    if matches!(first, Ty::Never) {
                        arm_types.iter()
                            .find(|t| !matches!(t, Ty::Never))
                            .cloned()
                            .or_else(|| arm_real_types.iter()
                                .find(|t| !matches!(resolve_ty(t, &self.uf), Ty::Never))
                                .cloned())
                            .unwrap_or(first)
                    } else {
                        first
                    }
                } else {
                    Ty::Unit
                }
            }

            ExprKind::Block { stmts, expr, .. } => {
                self.env.push_scope();
                // Pre-scan for vars used as match subjects with Ok/Err
                // patterns — those bindings must keep their Result type.
                let saved_skip = std::mem::take(&mut self.env.skip_auto_unwrap_for);
                let result_match_vars = collect_block_result_match_vars(stmts, expr.as_deref());
                for n in &result_match_vars {
                    self.env.skip_auto_unwrap_for.insert(*n);
                }
                for stmt in stmts.iter_mut() { self.check_stmt(stmt); }
                let ty = if let Some(e) = expr { self.infer_expr(e) } else { Ty::Unit };
                self.env.pop_scope();
                self.env.skip_auto_unwrap_for = saved_skip;
                ty
            }

            ExprKind::Fan { exprs, .. } => {
                if !self.env.can_call_effect {
                    self.emit(super::err(
                        "fan block can only be used inside an effect fn".to_string(),
                        "Mark the enclosing function as `effect fn`",
                        "fan block".to_string()).with_code("E007"));
                }
                // Check for mutable variable capture
                let mutable_captures: Vec<String> = exprs.iter().flat_map(|e| {
                    let mut idents = Vec::new();
                    collect_idents(e, &mut idents);
                    idents.into_iter().filter(|name| self.env.mutable_vars.contains(&sym(name))).collect::<Vec<_>>()
                }).collect();
                for name in &mutable_captures {
                    self.emit(super::err(
                        format!("cannot capture mutable variable '{}' inside fan block", name),
                        "Use a `let` binding instead of `var` for values shared across fan expressions",
                        "fan block".to_string()).with_code("E008"));
                }
                let tys: Vec<Ty> = exprs.iter_mut().map(|e| {
                    let ty = self.infer_expr(e);
                    // Auto-unwrap Result: fan unwraps Result<T, E> to T
                    let concrete = resolve_ty(&ty, &self.uf);
                    match &concrete {
                        Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[0].clone(),
                        _ => ty,
                    }
                }).collect();
                match tys.len() {
                    1 => tys.into_iter().next().unwrap(),
                    _ => Ty::Tuple(tys.iter().map(|t| resolve_ty(t, &self.uf)).collect()),
                }
            }

            ExprKind::Call { callee, args, named_args, type_args, .. } => {
                // Publish the outer Call's span so UFCS / whole-expr
                // rewrites (E002 method-UFCS, E013 no-field) can emit
                // a `try_replace` range covering `callee(args)` in
                // full, not just the callee reference. Nested calls
                // save/restore the previous value.
                let prev_call = self.call_span_hint.take();
                self.call_span_hint = expr.span;
                let ty = self.infer_call(callee, args, named_args, type_args);
                self.call_span_hint = prev_call;
                // A generic collection constructor whose element type NO argument
                // constrains — `set.new()` / `list.with_capacity(n)` — must have
                // its element pinned by context (annotation / later use). Register
                // it for the post-solve undecidable-empty-collection check (E018).
                if let Some(kind) = empty_collection_ctor_kind(callee) {
                    self.register_empty_collection(ty.clone(), kind);
                }
                ty
            }

            ExprKind::Pipe { left, right, .. } => {
                self.infer_pipe(left, right)
            }

            ExprKind::Compose { left, right, .. } => {
                let left_ty = self.infer_expr(left);
                let right_ty = self.infer_expr(right);
                // If left is Fn[A] -> B and right is Fn[B] -> C, result is Fn[A] -> C
                let resolved_left = resolve_ty(&left_ty, &self.uf);
                let resolved_right = resolve_ty(&right_ty, &self.uf);
                match (&resolved_left, &resolved_right) {
                    (Ty::Fn { params: a_params, .. }, Ty::Fn { ret: c_ret, .. }) => {
                        Ty::Fn { params: a_params.clone(), ret: c_ret.clone() }
                    }
                    _ => Ty::Unknown,
                }
            }

            ExprKind::Lambda { params, body, .. } => {
                self.env.push_scope();
                // Lambda has its own return context — don't leak outer function's current_ret
                let saved_ret = self.env.current_ret.take();
                // A lambda is its own function: the enclosing effect fn's
                // auto-`?` cannot propagate out of a closure body (the closure
                // may escape), so an effect call inside a lambda yields the
                // EXPLICIT Result — auto_unwrap is off, matching the lowering,
                // which never inserts `?` inside Lambda bodies (#489).
                let saved_auto_unwrap = self.env.auto_unwrap;
                self.env.auto_unwrap = false;
                self.env.lambda_depth += 1;
                let param_tys: Vec<Ty> = params.iter().map(|p| {
                    let ty = p.ty.as_ref().map(|te| self.resolve_type_expr(te)).unwrap_or_else(|| self.fresh_var());
                    let concrete = resolve_ty(&ty, &self.uf);
                    self.env.define_var(&p.name, concrete);
                    ty
                }).collect();
                let ret_ty = self.infer_expr(body);
                self.env.lambda_depth -= 1;
                self.env.auto_unwrap = saved_auto_unwrap;
                self.env.current_ret = saved_ret;
                self.env.pop_scope();
                Ty::Fn { params: param_tys, ret: Box::new(ret_ty) }
            }

            ExprKind::ForIn { var, var_tuple, iterable, body, .. } => {
                self.infer_for_in(var, var_tuple, iterable, body)
            }

            ExprKind::While { cond, body, .. } => {
                self.infer_expr(cond);
                self.env.push_scope();
                for stmt in body.iter_mut() { self.check_stmt(stmt); }
                self.env.pop_scope();
                Ty::Unit
            }

            ExprKind::Range { start, end, .. } => { let st = self.infer_expr(start); self.infer_expr(end); Ty::list(st) }

            ExprKind::Some { expr, .. } => { let inner = self.infer_expr(expr); Ty::option(inner) }
            ExprKind::Ok { expr, .. } => {
                let ok_ty = self.infer_expr(expr);
                let err_ty = match &self.env.current_ret {
                    Some(Ty::Applied(TypeConstructorId::Result, args)) if args.len() == 2 => args[1].clone(),
                    _ => self.fresh_var(),
                };
                Ty::result(ok_ty, err_ty)
            }
            ExprKind::Err { expr, .. } => {
                let err_ty = self.infer_expr(expr);
                let ok_ty = match &self.env.current_ret {
                    Some(Ty::Applied(TypeConstructorId::Result, args)) if args.len() == 2 => args[0].clone(),
                    _ => self.fresh_var(),
                };
                Ty::result(ok_ty, err_ty)
            }
            ExprKind::Try { expr, .. } => {
                let ty = self.infer_expr(expr);
                match &ty {
                    Ty::Applied(TypeConstructorId::Result, args) if args.len() >= 1 => args[0].clone(),
                    _ => ty,
                }
            }

            ExprKind::Paren { expr, .. } => self.infer_expr(expr),
            ExprKind::Break | ExprKind::Continue => Ty::Unit,
            ExprKind::Hole | ExprKind::Todo { .. } => self.fresh_var(),
            ExprKind::Await { expr, .. } => self.infer_expr(expr),

            // expr! — unwrap with propagation (Option[T] → T, Result[T,E] → T)
            ExprKind::Unwrap { expr: inner, .. } => {
                let t = self.infer_expr(inner);
                let resolved = resolve_ty(&t, &self.uf);
                if let Some(inner_ty) = resolved.option_inner().or_else(|| resolved.result_ok_ty()) {
                    inner_ty
                } else if matches!(&resolved, Ty::Unknown | Ty::TypeVar(_)) {
                    self.fresh_var()
                } else {
                    self.emit(super::err(
                        format!("operator '!' requires Option or Result type but got {}", resolved.display()),
                        "Use '!' only on Option[T] or Result[T, E] values",
                        "operator !",
                    ));
                    Ty::Unknown
                }
            }
            // expr ?? fallback — unwrap with default (Option[T] → T, Result[T,E] → T)
            ExprKind::UnwrapOr { expr: inner, fallback, .. } => {
                let t = self.infer_expr(inner);
                let ft = self.infer_expr(fallback);
                let resolved = resolve_ty(&t, &self.uf);
                let inner_ty = if let Some(ty) = resolved.option_inner().or_else(|| resolved.result_ok_ty()) {
                    ty
                } else if matches!(&resolved, Ty::Unknown | Ty::TypeVar(_)) {
                    ft.clone()
                } else {
                    self.emit(super::err(
                        format!("operator '??' requires Option or Result type but got {}", resolved.display()),
                        "Use '??' only on Option[T] or Result[T, E] values",
                        "operator ??",
                    ));
                    ft.clone()
                };
                self.unify_infer(&inner_ty, &ft);
                inner_ty
            }
            // expr? — to Option (Result[T,E] → Option[T], Option[T] → Option[T])
            ExprKind::ToOption { expr: inner, .. } => {
                let t = self.infer_expr(inner);
                let resolved = resolve_ty(&t, &self.uf);
                if let Some(ok_ty) = resolved.result_ok_ty() {
                    Ty::option(ok_ty)
                } else if resolved.is_option() {
                    resolved.clone()
                } else if matches!(&resolved, Ty::Unknown | Ty::TypeVar(_)) {
                    Ty::option(self.fresh_var())
                } else {
                    self.emit(super::err(
                        format!("operator '?' requires Option or Result type but got {}", resolved.display()),
                        "Use '?' only on Option[T] or Result[T, E] values",
                        "operator ?",
                    ));
                    Ty::Unknown
                }
            }

            // expr?.field — optional chaining: Option[T] → access T.field → Option[FieldType]
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

            ExprKind::Error | ExprKind::Placeholder => Ty::Unknown,

            ExprKind::MapLiteral { entries, .. } => {
                if entries.is_empty() {
                    let ty = Ty::map_of(self.fresh_var(), self.fresh_var());
                    self.register_empty_collection(ty.clone(), super::EmptyCollectionKind::MapLiteral);
                    ty
                }
                else {
                    let kt = self.infer_expr(&mut entries[0].0);
                    let vt = self.infer_expr(&mut entries[0].1);
                    for entry in entries.iter_mut().skip(1) { self.infer_expr(&mut entry.0); self.infer_expr(&mut entry.1); }
                    self.deferred_map_key_checks.push((kt.clone(), self.current_span));
                    Ty::map_of(kt, vt)
                }
            }
            ExprKind::EmptyMap => {
                let ty = Ty::map_of(self.fresh_var(), self.fresh_var());
                self.register_empty_collection(ty.clone(), super::EmptyCollectionKind::MapLiteral);
                ty
            }

            ExprKind::TypeAscription { expr, ty } => {
                let inferred = self.infer_expr(expr);
                let ascribed = self.resolve_type_expr(ty);
                self.constrain(ascribed.clone(), inferred, "type ascription");
                ascribed
            }
        }
    }

    // ── Extracted inference helpers ──

    fn infer_call(
        &mut self,
        callee: &mut Box<ast::Expr>,
        args: &mut Vec<ast::Expr>,
        named_args: &mut Vec<(almide_base::intern::Sym, ast::Expr)>,
        type_args: &Option<Vec<ast::TypeExpr>>,
    ) -> Ty {
        // Save named arg names, then flatten into positional args temporarily.
        let named_names: Vec<almide_base::intern::Sym> = named_args.iter().map(|(n, _)| *n).collect();
        let named_start = args.len();
        args.extend(std::mem::take(named_args).into_iter().map(|(_, e)| e));
        let resolved_type_args: Option<Vec<crate::types::Ty>> = type_args.as_ref().map(|tas|
            tas.iter().map(|te| self.resolve_type_expr(te)).collect());
        let ret = self.check_call_with_type_args(callee, args, resolved_type_args.as_deref());
        // Restore named args
        let named_exprs: Vec<ast::Expr> = args.drain(named_start..).collect();
        *named_args = named_names.into_iter().zip(named_exprs).collect();
        ret
    }

    fn infer_pipe(&mut self, left: &mut Box<ast::Expr>, right: &mut Box<ast::Expr>) -> Ty {
        // Unwrap postfix operators (??, !, ?) on the RHS so the pipe targets the inner Call.
        // e.g. `xs |> list.find(pred) ?? fallback` → pipe into list.find, then apply ??
        match &mut right.kind {
            ExprKind::UnwrapOr { expr: inner, fallback, .. } => {
                let inner_ty = self.infer_pipe(left, inner);
                let fb_ty = self.infer_expr(fallback);
                self.unify_infer(&inner_ty, &fb_ty);
                // UnwrapOr unwraps Option[T]/Result[T,E] → T
                match &inner_ty {
                    Ty::Applied(TypeConstructorId::Option, args) if args.len() == 1 => args[0].clone(),
                    Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[0].clone(),
                    _ => inner_ty,
                }
            }
            ExprKind::Unwrap { expr: inner, .. } => {
                let inner_ty = self.infer_pipe(left, inner);
                // Annotate the inner expression with its resolved type so the lowering
                // can construct the correct IR type (e.g., Result[List[T], List[E]] for
                // result.collect rather than hardcoding Result[T, String]).
                self.type_map.insert(inner.id, inner_ty.clone());
                match &inner_ty {
                    Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[0].clone(),
                    Ty::Applied(TypeConstructorId::Option, args) if args.len() == 1 => args[0].clone(),
                    _ => inner_ty,
                }
            }
            ExprKind::Try { expr: inner, .. } => {
                let inner_ty = self.infer_pipe(left, inner);
                match &inner_ty {
                    Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 =>
                        Ty::Applied(TypeConstructorId::Option, vec![args[0].clone()]),
                    _ => Ty::Applied(TypeConstructorId::Option, vec![inner_ty]),
                }
            }
            _ => self.infer_pipe_direct(left, right),
        }
    }

    fn infer_pipe_direct(&mut self, left: &mut Box<ast::Expr>, right: &mut Box<ast::Expr>) -> Ty {
        let left_ty = self.infer_expr(left);
        // Resolve TypeVars eagerly via UnionFind — earlier pipes in the chain
        // have already been unified (constrain() calls unify_infer immediately),
        // so the concrete type is available now. Without this, chained UFCS like
        // `xs |> list.map(f) |> list.join(",")` sees a raw TypeVar for the
        // intermediate result, causing module resolution to fail.
        let left_ty = super::types::resolve_ty(&left_ty, &self.uf);
        match &mut right.kind {
            ExprKind::Call { callee, args, .. } => {
                // Pipe inserts left as the first argument
                let mut all_arg_tys: Vec<Ty> = vec![left_ty];
                all_arg_tys.extend(args.iter_mut().map(|a| self.infer_expr(a)));
                // Resolve module calls for pipe (e.g. xs |> list.filter(f))
                match &mut callee.kind {
                    ExprKind::Ident { name, .. } => self.check_named_call(name, &all_arg_tys),
                    ExprKind::Member { object, field, .. } => {
                        let module_key = self.resolve_module_call(object, field);
                        if let Some(key) = module_key {
                            return self.check_named_call(&key, &all_arg_tys);
                        }
                        let ct = self.infer_expr(callee);
                        let ret = self.fresh_var();
                        self.constrain(ct, Ty::Fn { params: all_arg_tys, ret: Box::new(ret.clone()) }, "pipe call");
                        ret
                    }
                    _ => {
                        let ct = self.infer_expr(callee);
                        let ret = self.fresh_var();
                        self.constrain(ct, Ty::Fn { params: all_arg_tys, ret: Box::new(ret.clone()) }, "pipe call");
                        ret
                    }
                }
            }
            // Pipe RHS is a bare function name (e.g. `5 |> double`)
            ExprKind::Ident { name, .. } => {
                let all_arg_tys = vec![left_ty];
                self.check_named_call(name, &all_arg_tys)
            }
            // Pipe RHS is a module-qualified function (e.g. `5 |> int.abs`)
            ExprKind::Member { object, field, .. } => {
                let all_arg_tys = vec![left_ty];
                if let Some(key) = self.resolve_module_call(object, field) {
                    return self.check_named_call(&key, &all_arg_tys);
                }
                let ct = self.infer_expr(right);
                let ret = self.fresh_var();
                self.constrain(ct, Ty::Fn { params: all_arg_tys, ret: Box::new(ret.clone()) }, "pipe call");
                ret
            }
            _ => {
                let rt = self.infer_expr(right);
                let ret = self.fresh_var();
                self.constrain(rt, Ty::Fn { params: vec![left_ty], ret: Box::new(ret.clone()) }, "pipe call");
                ret
            }
        }
    }

    fn infer_for_in(
        &mut self,
        var: &str,
        var_tuple: &Option<Vec<almide_base::intern::Sym>>,
        iterable: &mut Box<ast::Expr>,
        body: &mut Vec<ast::Stmt>,
    ) -> Ty {
        // An empty-list iterable (`for _ in []`) registers a generic ListLiteral
        // site via `infer_expr` below; retag it as `ForInEmpty` so the E018 hint
        // suggests the for-position fix `for _ in ([] : List[Int])` rather than a
        // `let`-binding example.
        let iterable_is_empty_list = matches!(&iterable.kind,
            ExprKind::List { elements, .. } if elements.is_empty());
        let iter_ty = self.infer_expr(iterable);
        if iterable_is_empty_list {
            if let Some(last) = self.deferred_empty_collection_checks.last_mut() {
                last.kind = super::EmptyCollectionKind::ForInEmpty;
            }
        }
        self.env.push_scope();
        let iter_resolved = resolve_ty(&iter_ty, &self.uf);
        let elem_ty = match &iter_resolved {
            Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => args[0].clone(),
            Ty::Applied(TypeConstructorId::Map, args) if args.len() == 2 => Ty::Tuple(vec![args[0].clone(), args[1].clone()]),
            _ => Ty::Unknown,
        };
        if let Some(names) = var_tuple {
            // Destructure tuple: for (a, b) in xs
            if let Ty::Tuple(tys) = &elem_ty {
                for (i, n) in names.iter().enumerate() {
                    self.env.define_var(n, tys.get(i).cloned().unwrap_or(Ty::Unknown));
                }
            } else {
                for n in names { self.env.define_var(n, Ty::Unknown); }
            }
        } else {
            self.env.define_var(var, elem_ty);
        }
        for stmt in body.iter_mut() { self.check_stmt(stmt); }
        self.env.pop_scope();
        Ty::Unit
    }

    // ── Statement checking ──

    /// Reject a binding whose type uses a function in a position that demands
    /// equality/hashing: a `Set` element or a `Map` key. Closures have neither,
    /// so such a type is meaningless — and the two targets disagree (native
    /// rustc rejects it, WASM silently drops the inserts). Closures are fine as
    /// `Map` *values*.
    pub(crate) fn check_collection_element_types(&mut self, ty: &Ty) {
        let resolved = resolve_ty(ty, &self.uf);
        if let Some((msg, hint)) = invalid_collection_type(&resolved) {
            self.emit(super::err(msg, hint, "collection element type").with_code("E016"));
        }
    }

    /// Record an empty-collection producer to re-check after constraint solving
    /// (the undecidable-empty-collection / E018 rule). The current span is
    /// captured now; the element type is verified post-solve in
    /// [`Checker::validate_empty_collection_elements`].
    pub(crate) fn register_empty_collection(&mut self, ty: Ty, kind: super::EmptyCollectionKind) {
        self.deferred_empty_collection_checks.push(super::EmptyCollectionSite {
            ty,
            kind,
            span: self.current_span,
        });
    }

    /// #488: classify a `TypeName(...)` call. All-named args on a record
    /// type or record-payload variant case rewrite the node in place to the
    /// brace `ExprKind::Record` form (one construction pipeline, both
    /// spellings); positional args on those, or named args on a tuple
    /// constructor, are E021. Returns true when the node was rewritten.
    fn normalize_ctor_paren_call(&mut self, expr: &mut ast::Expr) -> bool {
        let ExprKind::Call { callee, args, named_args, .. } = &expr.kind else { return false };
        // Both spellings of a constructor callee: bare/dotted `TypeName`, and
        // the cross-module `m.Cfg(...)` form, which parses as a MEMBER access
        // on the module ident — without this arm the paren-named normalization
        // only covered the same-file spelling (caught by the §2 matrix gate).
        let n = match &callee.kind {
            ExprKind::TypeName { name } => *name,
            ExprKind::Member { object, field }
                if field.as_str().chars().next().map_or(false, |c| c.is_uppercase()) =>
            {
                let ExprKind::Ident { name: obj, .. } = &object.kind else { return false };
                sym(&format!("{}.{}", obj, field))
            }
            _ => return false,
        };
        let bare = n.as_str().rsplit_once('.').map(|(_, b)| sym(b)).unwrap_or(n);
        // Record-payload variant case? (ctor table is keyed by bare name)
        let ctor_payload_record = self.env.lookup_ctor_in(&bare, self.current_module_prefix.as_deref())
            .map(|(_, case)| matches!(case.payload, crate::types::VariantPayload::Record(_)));
        // Record TYPE? (resolve through the same canonicalization annotations use)
        let is_record_type = ctor_payload_record.is_none() && {
            let key = match n.as_str().rsplit_once('.') {
                Some(_) => sym(n.as_str()),
                None => crate::canonicalize::resolve::canonical_user_type_sym(
                    n.as_str(), &self.env.types, self.current_module_prefix.as_deref(),
                ).unwrap_or(n),
            };
            matches!(self.env.resolve_named(&Ty::Named(key, vec![])), Ty::Record { .. } | Ty::OpenRecord { .. })
        };
        if ctor_payload_record == Some(true) || is_record_type {
            if !args.is_empty() {
                self.emit(super::err(
                    format!("'{}' takes named fields, not positional arguments", n),
                    format!("Name every field: `{}(field: value, ...)` or `{} {{ field: value, ... }}`", n, n),
                    format!("constructor {}(...)", n),
                ).with_code("E021"));
                return false;
            }
            // Rewrite to the brace form in place; re-inference routes it
            // through the Record arm (defaults, field validation, #433
            // qualification, both backends' Record emission — for free).
            let ExprKind::Call { named_args, .. } = std::mem::replace(&mut expr.kind, ExprKind::Unit) else { unreachable!() };
            let fields = named_args.into_iter()
                .map(|(fname, value)| ast::FieldInit { name: fname, value })
                .collect();
            expr.kind = ExprKind::Record { name: Some(n), fields };
            return true;
        }
        if ctor_payload_record == Some(false) && !named_args.is_empty() {
            self.emit(super::err(
                format!("constructor '{}' takes positional arguments, not named ones", n),
                format!("Drop the names: `{}(value, ...)`", n),
                format!("constructor {}(...)", n),
            ).with_code("E021"));
        }
        false
    }

    /// #488: validate a record construction's field set against the declared
    /// fields: duplicates always; unknown + missing-without-default when the
    /// declaration is CLOSED (a plain record or a record-payload case).
    fn validate_record_fields(
        &mut self,
        type_label: &str,
        given: &[ast::FieldInit],
        decl_fields: &[(Sym, Ty)],
        closed: bool,
        defaults: &std::collections::HashSet<Sym>,
    ) {
        let mut seen: std::collections::HashSet<Sym> = std::collections::HashSet::new();
        for f in given {
            if !seen.insert(f.name) {
                self.emit(super::err(
                    format!("field '{}' given more than once in '{}' construction", f.name, type_label),
                    "Remove the duplicate field",
                    format!("record literal {}", type_label),
                ).with_code("E021"));
            }
        }
        if !closed { return; }
        let available = || decl_fields.iter().map(|(d, _)| d.as_str()).collect::<Vec<_>>().join(", ");
        for f in given {
            if !decl_fields.iter().any(|(d, _)| *d == f.name) {
                self.emit(super::err(
                    format!("'{}' has no field '{}'", type_label, f.name),
                    format!("Available fields: {}", available()),
                    format!("record literal {}", type_label),
                ).with_code("E021"));
            }
        }
        for (d, _) in decl_fields {
            if !given.iter().any(|f| f.name == *d) && !defaults.contains(d) {
                self.emit(super::err(
                    format!("missing field '{}' in '{}' construction", d, type_label),
                    format!("Provide it: `{} {{ {}: ..., ... }}` (fields without defaults are required)", type_label, d),
                    format!("record literal {}", type_label),
                ).with_code("E021"));
            }
        }
    }

    /// The effect-fn auto-unwrap rule, shared by every binding-shaped
    /// position (let / var / assign): a Result[T, E]-typed RHS unwraps to T
    /// — the lowering inserts the matching `?` — unless the target itself
    /// keeps the Result (declared Result annotation, Result-typed var, or a
    /// usage-skip like `match x { ok/err }`). One function so the positions
    /// can never diverge again (#485).
    fn effect_unwrap_rhs(&self, t: Ty, target_keeps_result: bool) -> Ty {
        if self.env.auto_unwrap && !target_keeps_result {
            match t {
                Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args.into_iter().next().unwrap(),
                other => other,
            }
        } else { t }
    }

    pub(crate) fn check_stmt(&mut self, stmt: &mut ast::Stmt) {
        match stmt {
            ast::Stmt::Let { name, ty, value, span } => {
                let val_ty = self.infer_expr(value);
                let final_ty = if let Some(te) = ty {
                    let declared = self.resolve_type_expr(te);
                    self.constrain(declared.clone(), val_ty, format!("let {}", name));
                    declared
                } else {
                    let t = resolve_ty(&val_ty, &self.uf);
                    // Auto-unwrap Result in effect fns (but not in test blocks),
                    // unless this binding is later used as a `match x { ok(_) =>
                    // ..., err(_) => ... }` subject — in which case the user
                    // wants to inspect the Result directly.
                    self.effect_unwrap_rhs(t, self.env.skip_auto_unwrap_for.contains(&sym(name)))
                };
                if let Some(s) = span {
                    self.env.var_decl_locs.insert(sym(name), (s.line, s.col));
                }
                self.check_collection_element_types(&final_ty);
                self.env.define_var(name, final_ty);
            }
            ast::Stmt::Var { name, ty, value, span } => {
                let val_ty = self.infer_expr(value);
                let final_ty = if let Some(te) = ty {
                    let declared = self.resolve_type_expr(te);
                    self.constrain(declared.clone(), val_ty, format!("let {}", name));
                    declared
                } else {
                    let t = resolve_ty(&val_ty, &self.uf);
                    // Same rule as Let, including the usage-skip: a `var r =
                    // effectCall()` later matched on ok/err keeps the Result.
                    self.effect_unwrap_rhs(t, self.env.skip_auto_unwrap_for.contains(&sym(name)))
                };
                if let Some(s) = span {
                    self.env.var_decl_locs.insert(sym(name), (s.line, s.col));
                }
                self.check_collection_element_types(&final_ty);
                self.env.define_var(name, final_ty);
                self.env.mutable_vars.insert(sym(name));
                self.env.var_lambda_depth.insert(sym(name), self.env.lambda_depth);
            }
            ast::Stmt::LetDestructure { pattern, value, .. } => {
                let val_ty = self.infer_expr(value);
                let val_resolved = resolve_ty(&val_ty, &self.uf);
                self.bind_pattern(pattern, &val_resolved);
            }
            ast::Stmt::Assign { name, value, .. } => {
                let val_ty = self.infer_expr(value);
                // A mut-receiver stdlib mutator (`list.push`, `map.insert`,
                // `string.push`, …) returns Unit and mutates in place. Writing
                // `b = list.push(b, x)` therefore assigns Unit to a non-Unit
                // binding. Native catches this at rustc (E0308 "expected Vec,
                // found ()"); WASM erases Unit and silently RUNS the program —
                // a cross-target asymmetry (compiles on one target, not the
                // other). Reject it in the checker so BOTH targets agree, with
                // the fix spelled out: drop the assignment (the call already
                // mutates) or rebuild a fresh value.
                // A local binding (`lookup_var`) OR a module-level `var`
                // (`top_lets`) — both are valid assignment targets and both carry
                // a concrete declared type to flow into the value.
                let var_ty = self.env.lookup_var(name).cloned()
                    .or_else(|| self.env.top_lets.get(&sym(name)).cloned());
                if let Some(var_ty) = &var_ty {
                    let val_resolved = resolve_ty(&val_ty, &self.uf);
                    let var_resolved = self.env.resolve_named(var_ty);
                    if matches!(val_resolved, Ty::Unit) && !matches!(var_resolved, Ty::Unit | Ty::Unknown) {
                        // Rebuild form is type-directed: a List concatenates a
                        // singleton, a String appends a suffix string. Other
                        // collections (Map/Set/Bytes) have no `+` rebuild, so we
                        // steer toward the statement form only.
                        let rebuild = match &var_resolved {
                            Ty::Applied(TypeConstructorId::List, _) => Some(format!("{0} = {0} + [<item>]", name)),
                            Ty::String => Some(format!("{0} = {0} + \"<suffix>\"", name)),
                            _ => None,
                        };
                        let snippet = match &rebuild {
                            Some(rb) => format!(
                                "// the mutator already updates '{n}' in place — drop the `{n} =` and call it as a statement:\n<mutator>({n}, ...)\n// or rebuild a fresh value:\n{rb}",
                                n = name,
                            ),
                            None => format!(
                                "// the mutator already updates '{n}' in place — drop the `{n} =` and call it as a statement:\n<mutator>({n}, ...)",
                                n = name,
                            ),
                        };
                        let hint = match &rebuild {
                            Some(rb) => format!(
                                "the right-hand side returns Unit (an in-place mutator). Call it as a \
                                 statement instead of assigning its result, or rebuild '{}' with a \
                                 value-returning expression like `{}`",
                                name, rb
                            ),
                            None => format!(
                                "the right-hand side returns Unit (an in-place mutator). Call it as a \
                                 statement instead of assigning its result — '{}' is already mutated in place",
                                name
                            ),
                        };
                        self.emit(super::err(
                            format!("cannot assign a Unit value to '{}'", name),
                            hint,
                            format!("{} = ...", name),
                        ).with_code("E001").with_try(snippet));
                    } else {
                        // Unify the assigned value's type with the variable's
                        // declared type. The variable already carries a concrete
                        // type from its `var`/`let` declaration; flowing it into
                        // the value pins an otherwise-unconstrained element — e.g.
                        // `items = []` for `var items: List[Int]` resolves `[]`'s
                        // element to `Int` (it is the source of truth, exactly as
                        // a typed `let` binding is). Without this, an empty literal
                        // assigned to a typed var stays undecidable (E018).
                        //
                        // #485: apply the same effect-fn auto-unwrap rule as
                        // let/var first — `x = step(x)` with x: Int unwraps the
                        // lifted Result[Int, E]; a Result-typed target keeps it.
                        // Only substitute when the unwrap actually fires, so an
                        // unresolved TypeVar RHS keeps flowing through inference.
                        let unwrapped = self.effect_unwrap_rhs(val_resolved.clone(), var_resolved.is_result());
                        let constrain_val = if unwrapped != val_resolved { unwrapped } else { val_ty.clone() };
                        self.constrain(var_ty.clone(), constrain_val, format!("assign {}", name));
                    }
                }
                if self.env.lookup_var(name).is_some() && !self.env.mutable_vars.contains(&sym(name)) {
                    let is_param = self.env.param_vars.contains(&sym(name));
                    let hint = if is_param {
                        format!("'{}' is a function parameter (immutable). Use a local copy: var {0}_ = {0}", name)
                    } else {
                        format!("Use 'var {0} = ...' instead of 'let {0} = ...' to declare a mutable variable", name)
                    };
                    let snippet = if is_param {
                        format!("// '{n}' is a parameter — make a mutable copy:\nvar {n}_ = {n}\n// ...then reassign {n}_ instead of {n}", n = name)
                    } else {
                        format!("// let {n} = ...  →  var {n} = ...\nvar {n} = <initial value>", n = name)
                    };
                    let mut diag = super::err(
                        format!("cannot reassign immutable binding '{}'", name),
                        hint, format!("{} = ...", name)).with_code("E009").with_try(snippet);
                    if let Some(&(line, col)) = self.env.var_decl_locs.get(&sym(name)) {
                        diag = diag.with_secondary(line, Some(col), format!("'{}' declared here", name));
                    }
                    self.emit(diag);
                }
                // Escape analysis: block var mutation inside closures in pure fns
                if self.env.mutable_vars.contains(&sym(name)) && !self.env.can_call_effect {
                    if let Some(&decl_depth) = self.env.var_lambda_depth.get(&sym(name)) {
                        if self.env.lambda_depth > decl_depth {
                            self.emit(super::err(
                                format!("mutable variable '{}' is mutated inside a closure in a pure function — use effect fn instead", name),
                                "Move the mutation out of the closure, or mark the enclosing function as `effect fn`",
                                format!("{} = ...", name)).with_code("E011"));
                        }
                    }
                }
            }
            ast::Stmt::IndexAssign { target, index, value, .. } => {
                self.infer_expr(index);
                self.infer_expr(value);
                // A module-level `let g` is immutable just like a local `let` — its
                // contents may not be index-assigned. `lookup_var` only sees locals,
                // so without the `top_lets` arm a global `let g; g[2]=…` slipped past
                // this check and only failed later as opaque rustc `E0425` (the
                // ModuleRc lowering never kicks in for a non-mutable global). Catch it
                // here with the same E009 locals get.
                let is_known_binding = self.env.lookup_var(target.as_str()).is_some()
                    || self.env.top_lets.contains_key(&sym(target.as_str()));
                if is_known_binding && !self.env.mutable_vars.contains(target) {
                    let mut diag = super::err(
                        format!("cannot mutate immutable binding '{}'", target),
                        format!("Use 'var {} = ...' to declare a mutable variable", target),
                        format!("{}[...] = ...", target)).with_code("E009");
                    if let Some(&(line, col)) = self.env.var_decl_locs.get(target) {
                        diag = diag.with_secondary(line, Some(col), format!("'{}' declared here", target));
                    }
                    self.emit(diag);
                }
            }
            ast::Stmt::FieldAssign { value, .. } => { self.infer_expr(value); }
            ast::Stmt::Guard { cond, else_, .. } => { self.infer_expr(cond); self.infer_expr(else_); }
            ast::Stmt::Expr { expr, .. } => { self.infer_expr(expr); }
            ast::Stmt::Comment { .. } | ast::Stmt::Error { .. } => {}
        }
    }

    // ── Pattern binding ──

    pub(crate) fn bind_pattern(&mut self, pattern: &ast::Pattern, ty: &Ty) {
        match pattern {
            ast::Pattern::Wildcard => {}
            ast::Pattern::Ident { name } => { self.env.define_var(name, ty.clone()); }
            ast::Pattern::Constructor { name, args } => {
                let resolved = self.env.resolve_named(ty);
                // Normalize module-qualified names: "binary.Unreachable" → "Unreachable"
                let bare_name = name.as_str().rsplit_once('.').map(|(_, b)| sym(b)).unwrap_or(*name);
                let payload_tys: Vec<Ty> = match &resolved {
                    Ty::Variant { cases, .. } => cases.iter()
                        .find(|c| c.name == bare_name)
                        .map(|c| match &c.payload {
                            VariantPayload::Tuple(tys) => tys.clone(),
                            // #488: a paren pattern on a RECORD-payload case
                            // (`SetEmotion(_)`) bound nothing on native (rustc
                            // E0164) while wasm accepted it — reject with the
                            // brace spelling, both targets agree at check time.
                            VariantPayload::Record(_) => {
                                self.emit(super::err(
                                    format!("case '{}' has named fields — use a record pattern", name),
                                    format!("Match it as `{} {{ .. }}` (or bind fields: `{} {{ field }}`)", name, name),
                                    format!("pattern {}(...)", name),
                                ).with_code("E021"));
                                vec![]
                            }
                            _ => vec![],
                        })
                        .unwrap_or_default(),
                    // Opaque alias destructure: SafeHtml(s) → inner type
                    Ty::Named(tname, _) => {
                        if let Some(target) = self.env.opaque_alias_targets.get(tname).cloned() {
                            vec![target]
                        } else {
                            vec![]
                        }
                    }
                    _ => vec![],
                };
                for (i, arg) in args.iter().enumerate() {
                    self.bind_pattern(arg, payload_tys.get(i).unwrap_or(&Ty::Unknown));
                }
            }
            ast::Pattern::RecordPattern { name, fields, .. } => {
                let resolved = self.env.resolve_named(ty);
                let field_tys: Vec<(Sym, Ty)> = match &resolved {
                    Ty::Record { fields } | Ty::OpenRecord { fields } => fields.clone(),
                    Ty::Variant { cases, .. } => {
                        // Find the specific case matching the pattern name
                        cases.iter()
                            .find(|c| c.name == *name)
                            .and_then(|c| match &c.payload {
                                VariantPayload::Record(fs) => Some(fs.iter().map(|(n, t)| (*n, t.clone())).collect()),
                                _ => None,
                            })
                            .unwrap_or_default()
                    }
                    _ => vec![],
                };
                for f in fields {
                    let ft = field_tys.iter().find(|(n, _)| *n == f.name).map(|(_, t)| t.clone()).unwrap_or(Ty::Unknown);
                    if let Some(ref p) = f.pattern { self.bind_pattern(p, &ft); }
                    else { self.env.define_var(&f.name, ft); }
                }
            }
            ast::Pattern::Tuple { elements } => {
                let resolved = resolve_ty(ty, &self.uf);
                if let Ty::Tuple(tys) = &resolved {
                    for (i, e) in elements.iter().enumerate() { self.bind_pattern(e, tys.get(i).unwrap_or(&Ty::Unknown)); }
                } else if super::types::is_inference_var(&resolved).is_some() {
                    // Type is an unresolved inference var (e.g., lambda parameter).
                    // Create fresh vars for each element and constrain: ?N = (?a, ?b, ...).
                    // When the outer call context later resolves ?N, the element vars
                    // get their correct types through the constraint chain.
                    let elem_vars: Vec<Ty> = elements.iter().map(|_| self.fresh_var()).collect();
                    self.constrain(resolved, Ty::Tuple(elem_vars.clone()), "tuple destructure");
                    for (e, ev) in elements.iter().zip(elem_vars.iter()) { self.bind_pattern(e, ev); }
                } else {
                    for e in elements { self.bind_pattern(e, &Ty::Unknown); }
                }
            }
            ast::Pattern::List { elements } => {
                let resolved = resolve_ty(ty, &self.uf);
                let elem_ty = match &resolved {
                    Ty::Applied(TypeConstructorId::List, args) if !args.is_empty() => args[0].clone(),
                    // Subject is still an unresolved inference var (e.g. a
                    // lambda param not yet linked to its call site). Matching
                    // `[..]` asserts it is a List, so pin its structure to
                    // List[?elem] — otherwise the element pattern var
                    // dead-ends at Unknown and leaks to the IR.
                    Ty::TypeVar(_) => {
                        let elem_v = self.fresh_var();
                        self.unify_infer(&resolved, &Ty::list(elem_v.clone()));
                        elem_v
                    }
                    _ => Ty::Unknown,
                };
                for e in elements { self.bind_pattern(e, &elem_ty); }
            }
            ast::Pattern::Some { inner } => {
                let resolved = resolve_ty(ty, &self.uf);
                let it = match &resolved {
                    Ty::Applied(TypeConstructorId::Option, args) if args.len() == 1 => args[0].clone(),
                    // See List arm: pin an unresolved subject to Option[?inner].
                    Ty::TypeVar(_) => {
                        let inner_v = self.fresh_var();
                        self.unify_infer(&resolved, &Ty::option(inner_v.clone()));
                        inner_v
                    }
                    _ => Ty::Unknown,
                };
                self.bind_pattern(inner, &it);
            }
            ast::Pattern::Ok { inner } => {
                let resolved = resolve_ty(ty, &self.uf);
                let it = match &resolved {
                    Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[0].clone(),
                    Ty::TypeVar(_) => {
                        let ok_v = self.fresh_var();
                        let err_v = self.fresh_var();
                        self.unify_infer(&resolved, &Ty::result(ok_v.clone(), err_v));
                        ok_v
                    }
                    _ => Ty::Unknown,
                };
                self.bind_pattern(inner, &it);
            }
            ast::Pattern::Err { inner } => {
                let resolved = resolve_ty(ty, &self.uf);
                let it = match &resolved {
                    Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[1].clone(),
                    Ty::TypeVar(_) => {
                        let ok_v = self.fresh_var();
                        let err_v = self.fresh_var();
                        self.unify_infer(&resolved, &Ty::result(ok_v, err_v.clone()));
                        err_v
                    }
                    _ => Ty::Unknown,
                };
                self.bind_pattern(inner, &it);
            }
            ast::Pattern::None | ast::Pattern::Literal { .. } => {}
        }
    }

    /// Infer the result type of the + operator (numeric add or string/list concat).
    fn infer_plus_op(&mut self, lc: &Ty, rc: &Ty, lt: Ty) -> Ty {
        let is_concat_ty = |t: &Ty| matches!(t, Ty::String | Ty::Applied(TypeConstructorId::List, _));
        let is_unknown_ty = |t: &Ty| matches!(t, Ty::Unknown | Ty::TypeVar(_));
        // When one side is List and the other is TypeVar, unify the TypeVar with the List type
        if is_unknown_ty(lc) && is_concat_ty(rc) {
            self.unify_infer(&lt, rc);
            let resolved_lt = resolve_ty(&lt, &self.uf);
            // Now unify element types if both resolved to List
            if let (Ty::Applied(TypeConstructorId::List, la), Ty::Applied(TypeConstructorId::List, ra)) = (&resolved_lt, rc) {
                if let (Some(le), Some(re)) = (la.first(), ra.first()) {
                    self.unify_infer(le, re);
                }
            }
            return resolve_ty(&lt, &self.uf);
        }
        if (is_concat_ty(lc) && (is_concat_ty(rc) || is_unknown_ty(rc)))
            || (is_concat_ty(rc) && is_unknown_ty(lc)) {
            // Unify element types for list concatenation: List[?0] + List[Int] → ?0 = Int
            if let (Ty::Applied(TypeConstructorId::List, la), Ty::Applied(TypeConstructorId::List, ra)) = (lc, rc) {
                if let (Some(le), Some(re)) = (la.first(), ra.first()) {
                    self.unify_infer(le, re);
                }
            }
            return resolve_ty(&lt, &self.uf);
        }
        // Matrix addition
        if *lc == Ty::Matrix || *rc == Ty::Matrix {
            return Ty::Matrix;
        }
        // Sized Numeric Types (Stage 1c): arithmetic accepts canonical
        // `Int` / `Float` plus every sized variant. Same-type pairing is
        // enforced below; mixing widths is an explicit conversion.
        let is_numeric = |t: &Ty| matches!(
            t,
            Ty::Int | Ty::Float | Ty::Unknown | Ty::TypeVar(_)
                | Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64
                | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
                | Ty::Float32 | Ty::Float64
                | Ty::Matrix | Ty::Named(..)
        );
        if !is_numeric(lc) || !is_numeric(rc) {
            self.emit(super::err(
                format!("operator '+' requires numeric, String, or List types but got {} and {}", lc.display(), rc.display()),
                "Use + with numeric types, String, or List", format!("operator +")));
        }
        // Result type resolution:
        //   - Same sized type on both sides → that sized type.
        //   - Canonical Float promotes Int mixes to Float (legacy rule).
        //   - Mixed sized widths are rejected; the diagnostic is
        //     emitted by `compatible` / `unify_infer` callers, so here
        //     we just fall through with `lt` to avoid an extra error.
        let is_sized_scalar = |t: &Ty| matches!(
            t,
            Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64
                | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
                | Ty::Float32 | Ty::Float64
        );
        // Sized Numeric Types (Stage 1c): both sides sized AND widths
        // differ is a type error. The permissive `Ty::Int` / `Ty::Float`
        // canonical pair stays (it carries the literal-coercion slot for
        // `let x: Int32 = 42` style bindings). Mixing `Int32` and `Int16`
        // has no such cover — it's always wrong, always needs explicit
        // `.to_intN()`.
        if is_sized_scalar(lc) && is_sized_scalar(rc) && lc != rc {
            self.emit(super::err(
                format!(
                    "operator '+' mixes sized numeric types {} and {} — \
                     explicit conversion required (e.g. `.to_{}()`)",
                    lc.display(), rc.display(),
                    lc.display().to_lowercase()),
                "Convert one side with `.to_intN()` / `.to_floatN()` before the op",
                format!("operator +")));
            return lc.clone();
        }
        if lc.compatible(rc) && is_sized_scalar(lc) {
            return lc.clone();
        }
        if *lc == Ty::Float || *rc == Ty::Float { Ty::Float } else { lt }
    }

    /// Resolve a module.func Member expression to a qualified call key.
    fn resolve_module_call(&mut self, object: &ast::Expr, field: &str) -> Option<String> {
        if let ExprKind::Ident { name: module, .. } = &object.kind {
            if let Some(canonical) = self.env.import_table.resolve(module) {
                self.env.import_table.mark_used(module);
                let key = format!("{}.{}", canonical, field);
                self.check_fn_visibility(&canonical, field, &key);
                return Some(key);
            }
            // Check if Ident.field is a Type.method (protocol implementation)
            let key = format!("{}.{}", module, field);
            if self.env.functions.contains_key(&sym(&key)) {
                return Some(key);
            }
        }
        // Detect dot-chain submodule access (for pipe context)
        if let Some(dotted) = self.env.import_table.resolve_dotted_path(&object.kind) {
            let key = format!("{}.{}", dotted, field);
            if self.env.functions.contains_key(&sym(&key)) {
                let last_seg = dotted.rsplit('.').next().unwrap_or(&dotted);
                self.emit(super::err(
                    format!("dot-chain submodule access is no longer supported"),
                    format!("Add `import {}` and call `{}.{}()` instead", dotted, last_seg, field),
                    format!("call to {}.{}", dotted, field),
                ));
                return Some(key);
            }
        }
        // TypeName.method (e.g. Val.double in pipe)
        if let ExprKind::TypeName { name: type_name, .. } = &object.kind {
            let key = format!("{}.{}", type_name, field);
            if self.env.functions.contains_key(&sym(&key)) {
                return Some(key);
            }
        }
        None
    }

    /// Reject cross-module access to `mod fn` / `local fn` functions.
    ///
    /// A function has `Public` visibility by default — we only store entries
    /// for restricted (`Mod` / `Local`) declarations in `env.fn_visibility`.
    /// If the caller's own module (`self_module_name`) matches the callee's
    /// canonical module, the call is intra-module and all visibilities are
    /// allowed. Otherwise only `Public` is reachable.
    pub(super) fn check_fn_visibility(&mut self, callee_module: &str, field: &str, key: &str) {
        let vis = match self.env.fn_visibility.get(&sym(key)) {
            Some(v) => *v,
            None => return,
        };
        // Intra-module access (same package) is always allowed, regardless of
        // whether it's `mod fn` or `local fn`. This matches the spec for
        // `mod fn` and is a pragmatic relaxation for `local fn` (strict
        // same-file enforcement needs per-fn file tracking — TODO).
        if let Some(self_mod) = self.env.self_module_name {
            if self_mod.as_str() == callee_module {
                return;
            }
        }
        let (kind, scope_hint) = match vis {
            ast::Visibility::Mod => (
                "mod fn",
                "accessible only within the same project",
            ),
            ast::Visibility::Local => (
                "local fn",
                "accessible only within the same file",
            ),
            ast::Visibility::Public => return,
        };
        self.emit(super::err(
            format!("function '{}.{}' is not accessible", callee_module, field),
            format!("'{}' is declared as `{}` ({})", field, kind, scope_hint),
            format!("call to {}.{}", callee_module, field),
        ).with_code("E420"));
    }

}

/// Collect Ident names that appear as match subjects with Ok/Err
/// patterns inside a block. Used to suppress auto-unwrap of Result on
/// the corresponding `let` bindings — the user wants to inspect the
/// Result, so the Bind must keep its Result type.
pub(crate) fn collect_block_result_match_vars(stmts: &[ast::Stmt], tail: Option<&ast::Expr>) -> std::collections::HashSet<Sym> {
    let mut out = std::collections::HashSet::new();
    for s in stmts { collect_in_stmt(s, &mut out); }
    if let Some(e) = tail { collect_in_expr(e, &mut out); }
    out
}

fn collect_in_stmt(stmt: &ast::Stmt, out: &mut std::collections::HashSet<Sym>) {
    match stmt {
        ast::Stmt::Let { value, .. } | ast::Stmt::Var { value, .. } => collect_in_expr(value, out),
        ast::Stmt::Expr { expr, .. } => collect_in_expr(expr, out),
        ast::Stmt::Assign { value, .. } => collect_in_expr(value, out),
        ast::Stmt::Guard { cond, else_, .. } => { collect_in_expr(cond, out); collect_in_expr(else_, out); }
        _ => {}
    }
}

fn collect_in_expr(expr: &ast::Expr, out: &mut std::collections::HashSet<Sym>) {
    match &expr.kind {
        ExprKind::Match { subject, arms, .. } => {
            let arms_match_result = arms.iter().any(|a| matches!(
                &a.pattern,
                ast::Pattern::Ok { .. } | ast::Pattern::Err { .. }
            ));
            if arms_match_result {
                if let ExprKind::Ident { name, .. } = &subject.kind {
                    out.insert(*name);
                }
            }
            collect_in_expr(subject, out);
            for arm in arms { collect_in_expr(&arm.body, out); }
        }
        ExprKind::Block { stmts, expr: tail, .. } => {
            for s in stmts { collect_in_stmt(s, out); }
            if let Some(t) = tail { collect_in_expr(t, out); }
        }
        ExprKind::If { cond, then, else_, .. } => {
            collect_in_expr(cond, out);
            collect_in_expr(then, out);
            collect_in_expr(else_, out);
        }
        _ => {}
    }
}

/// Collect all Ident names referenced in an expression (shallow, for var capture check).
fn collect_idents(expr: &ast::Expr, out: &mut Vec<String>) {
    match &expr.kind {
        ExprKind::Ident { name, .. } => out.push(name.to_string()),
        ExprKind::Call { callee, args, .. } => {
            collect_idents(callee, out);
            for a in args { collect_idents(a, out); }
        }
        ExprKind::Member { object, .. } | ExprKind::TupleIndex { object, .. }
        | ExprKind::IndexAccess { object, .. } => collect_idents(object, out),
        ExprKind::Binary { left, right, .. } | ExprKind::Pipe { left, right, .. } | ExprKind::Compose { left, right, .. } => {
            collect_idents(left, out); collect_idents(right, out);
        }
        ExprKind::Unary { operand, .. } | ExprKind::Paren { expr: operand, .. }
        | ExprKind::Some { expr: operand, .. } | ExprKind::Ok { expr: operand, .. }
        | ExprKind::Err { expr: operand, .. } | ExprKind::Try { expr: operand, .. } => {
            collect_idents(operand, out);
        }
        ExprKind::If { cond, then, else_, .. } => {
            collect_idents(cond, out); collect_idents(then, out); collect_idents(else_, out);
        }
        ExprKind::List { elements, .. } | ExprKind::Tuple { elements, .. } => {
            for e in elements { collect_idents(e, out); }
        }
        ExprKind::Lambda { body, .. } => collect_idents(body, out),
        ExprKind::InterpolatedString { parts, .. } => {
            for p in parts { if let ast::StringPart::Expr { expr } = p { collect_idents(expr, out); } }
        }
        ExprKind::Record { fields, .. } => { for f in fields { collect_idents(&f.value, out); } }
        _ => {} // literals, none, unit, etc.
    }
}

/// If an `if/else` arm is a statement-only block that assigns to a variable
/// (e.g. `{ high = mid - 1 }`), return its target name. This is the dojo
/// binary-search / matrix-ops pattern: an arm does a side-effect instead of
/// producing a value, so the whole if-expr types as Unit.
fn arm_assign_target(expr: &ast::Expr) -> Option<String> {
    let ExprKind::Block { stmts, expr: tail } = &expr.kind else { return None };
    if tail.is_some() { return None; }
    // Only single-statement blocks are unambiguous: `{ x = v }`. Multi-stmt
    // blocks might legitimately produce a value via a trailing stmt we
    // don't pattern-match, so skip to avoid false claims.
    if stmts.len() != 1 { return None; }
    match &stmts[0] {
        ast::Stmt::Assign { name, .. } => Some(name.to_string()),
        ast::Stmt::Let { name, .. } | ast::Stmt::Var { name, .. } => Some(name.to_string()),
        _ => None,
    }
}

fn if_arm_fix_hint(then: &ast::Expr, else_: &ast::Expr) -> Option<FixHint> {
    let then_tgt = arm_assign_target(then);
    let else_tgt = arm_assign_target(else_);
    match (then_tgt, else_tgt) {
        (Some(t), Some(e)) => Some(FixHint::IfArmsAssign {
            then_var: Some(t), else_var: Some(e),
        }),
        (Some(t), None) => Some(FixHint::IfArmAssign {
            arm: IfArm::Then, var_name: t,
        }),
        (None, Some(e)) => Some(FixHint::IfArmAssign {
            arm: IfArm::Else, var_name: e,
        }),
        (None, None) => None,
    }
}

/// True if `ty` mentions a function type anywhere (directly or nested in a
/// container/tuple/record). Such a type can't be hashed or compared.
fn ty_mentions_fn(ty: &Ty) -> bool {
    match ty {
        Ty::Fn { .. } => true,
        Ty::Tuple(ts) => ts.iter().any(ty_mentions_fn),
        Ty::Record { fields } | Ty::OpenRecord { fields } => fields.iter().any(|(_, t)| ty_mentions_fn(t)),
        Ty::Applied(_, args) | Ty::Named(_, args) => args.iter().any(ty_mentions_fn),
        _ => false,
    }
}

/// `Some((message, hint))` if `ty` (or a nested type) uses a function where a
/// comparable/hashable type is required: a `Set` element or a `Map` key.
/// Closures are allowed as `Map` values, so only the key (arg 0) is checked.
fn invalid_collection_type(ty: &Ty) -> Option<(&'static str, &'static str)> {
    match ty {
        Ty::Applied(TypeConstructorId::Set, args) if args.len() == 1 && ty_mentions_fn(&args[0]) => {
            return Some((
                "a `Set` element type cannot contain a function — closures have no equality or hashing",
                "Closures can't be deduplicated. Keep them in a `List`, or build the set from a comparable key.",
            ));
        }
        Ty::Applied(TypeConstructorId::Map, args) if args.len() == 2 && ty_mentions_fn(&args[0]) => {
            return Some((
                "a `Map` key type cannot contain a function — closures have no equality or hashing",
                "Closures are fine as `Map` values; only the key must be comparable.",
            ));
        }
        _ => {}
    }
    let children: Vec<&Ty> = match ty {
        Ty::Tuple(ts) => ts.iter().collect(),
        Ty::Record { fields } | Ty::OpenRecord { fields } => fields.iter().map(|(_, t)| t).collect(),
        Ty::Applied(_, args) | Ty::Named(_, args) => args.iter().collect(),
        Ty::Fn { params, ret } => params.iter().chain(std::iter::once(ret.as_ref())).collect(),
        _ => Vec::new(),
    };
    for c in children {
        if let Some(e) = invalid_collection_type(c) { return Some(e); }
    }
    None
}

/// If `callee` names a generic collection constructor whose element type NO
/// argument can constrain — `set.new()` (`Set[A]`) or `list.with_capacity(n)`
/// (`List[A]`, the only arg being the capacity) — return its
/// [`EmptyCollectionKind`] so the call is registered for the undecidable-empty
/// check (E018). Any other callee returns `None`. Matched structurally on the
/// `module.func` member form the parser produces for stdlib calls.
fn empty_collection_ctor_kind(callee: &ast::Expr) -> Option<super::EmptyCollectionKind> {
    let ExprKind::Member { object, field } = &callee.kind else { return None; };
    let ExprKind::Ident { name: module } = &object.kind else { return None; };
    match (module.as_str(), field.as_str()) {
        ("set", "new") => Some(super::EmptyCollectionKind::SetNew),
        ("list", "with_capacity") => Some(super::EmptyCollectionKind::ListWithCapacity),
        // `map.new()` is the constructor analogue of the empty `[:]` literal —
        // a generic `Map[K, V]` with no key/value-bearing argument. Same fix
        // family as the map literal (annotate the key/value types).
        ("map", "new") => Some(super::EmptyCollectionKind::MapLiteral),
        _ => None,
    }
}
