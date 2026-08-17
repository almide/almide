/// Expression type inference — Pass 1 of the constraint-based checker.
/// Walks the AST, populates TypeMap (ExprId→Ty), collects constraints.

use almide_lang::ast;
use almide_lang::ast::ExprKind;
use almide_base::diagnostic::Applicability;
use almide_base::intern::{Sym, sym};
use crate::types::{Ty, TypeConstructorId, VariantPayload};
use super::types::{resolve_ty, FixHint, IfArm};
use super::Checker;

/// One `object.field` → stdlib-call rewrite for E013 (`xs.head` →
/// `list.first(xs)`). `args_tpl` is a tiny `("{0}", 1)`-style mini-language:
/// `{0}` is substituted with the object's source slice, any trailing text
/// goes verbatim. `display_suffix` is comment-only info shown when no source
/// span is available and the snippet degrades to display-only.
struct MemberRewrite {
    field: &'static str,
    fn_name: &'static str,
    args_tpl: &'static str,
    display_suffix: &'static str,
    /// #1312. `MachineApplicable` **only** where the call yields the same
    /// type the field access denoted — `xs.length` and `list.len(xs)` are
    /// both `Int`, so the rewrite is a pure re-spelling. The
    /// Option-returning cells turn `T` into `Option[T]`, which the
    /// surrounding code did not ask for, so they stay suggestions: applying
    /// one unattended would move the error instead of closing it.
    applicability: Applicability,
}

const LIST_MEMBER_REWRITES: &[MemberRewrite] = &[
    MemberRewrite { field: "head",   fn_name: "list.first", args_tpl: "({0})",    display_suffix: "  // returns Option[T]", applicability: Applicability::MaybeIncorrect },
    MemberRewrite { field: "tail",   fn_name: "list.drop",  args_tpl: "({0}, 1)", display_suffix: "", applicability: Applicability::MachineApplicable },
    MemberRewrite { field: "length", fn_name: "list.len",   args_tpl: "({0})",    display_suffix: "", applicability: Applicability::MachineApplicable },
    MemberRewrite { field: "len",    fn_name: "list.len",   args_tpl: "({0})",    display_suffix: "", applicability: Applicability::MachineApplicable },
    MemberRewrite { field: "first",  fn_name: "list.first", args_tpl: "({0})",    display_suffix: "", applicability: Applicability::MaybeIncorrect },
    MemberRewrite { field: "last",   fn_name: "list.last",  args_tpl: "({0})",    display_suffix: "", applicability: Applicability::MaybeIncorrect },
    MemberRewrite { field: "size",   fn_name: "list.len",   args_tpl: "({0})",    display_suffix: "", applicability: Applicability::MachineApplicable },
];

const STRING_MEMBER_REWRITES: &[MemberRewrite] = &[
    MemberRewrite { field: "length", fn_name: "string.len",      args_tpl: "({0})", display_suffix: "", applicability: Applicability::MachineApplicable },
    MemberRewrite { field: "len",    fn_name: "string.len",      args_tpl: "({0})", display_suffix: "", applicability: Applicability::MachineApplicable },
    MemberRewrite { field: "size",   fn_name: "string.len",      args_tpl: "({0})", display_suffix: "", applicability: Applicability::MachineApplicable },
    MemberRewrite { field: "chars",  fn_name: "string.to_chars", args_tpl: "({0})", display_suffix: "", applicability: Applicability::MachineApplicable },
];

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
            // EVERY int literal gets a post-solve range site, not just the
            // i64-overflowing ones: a SIZED context can overflow an i64-fitting
            // literal in ANY position — `(x - x) - 256` with x: Int8 passed
            // check while native rustc rejected `256i8` (fuzz seed-20260718
            // index 114, the binop-operand edition of index 92). The validator
            // resolves each site's effective type (context_ty, else the
            // literal's own solved type) and plain-Int literals pass trivially,
            // so the liberal enqueue costs one Vec push per literal. The Unary
            // parent still flips `negated` (#626) and the binding/arg hooks
            // still pin `context_ty`.
            self.deferred_int_overflow_checks.push(super::IntOverflowSite {
                expr_id: expr.id, raw: raw.clone(), negated: false, context_ty: None, span: expr.span,
            });
        }
        // Wave 4 L7: a float literal beyond f32's finite range is queued for the
        // post-solve Float32 range check — an error only if its type RESOLVES to
        // Float32 (its own solved type carries the context per C-182), where the
        // emitted `<lit>f32` would be rustc's "literal out of range for f32".
        // Only such literals are queued (cheap pre-filter); magnitude is
        // sign-symmetric so a negated parent needs no flag.
        if let ExprKind::Float { value, .. } = &expr.kind {
            if value.is_finite() && (*value as f32).is_infinite() {
                self.deferred_float_overflow_checks.push(super::FloatOverflowSite {
                    expr_id: expr.id, value: *value, context_ty: None, span: expr.span,
                });
            }
        }
        let ity = self.infer_expr_inner(expr);
        self.type_map.insert(expr.id, ity.clone());
        // #662 extension (fuzz seed-20260718 index 145): a CALL's instantiated
        // result can carry an unconstrained phantom slot even when no binding
        // sees it — `let r: Bool = result.is_ok(result.or_else(n, f))` leaves
        // or_else's F forever unpinned, and the un-annotated-binding hook never
        // fires (r IS annotated, the slot lives in the INTERMEDIATE expr).
        // Native tolerated the Unknown while the wasm leg refused the build —
        // an acceptance-parity split. Enqueue every call-result type; the
        // post-solve validator (E025) fires only on the genuinely undecidable
        // ones, so the liberal enqueue costs one Vec push per call.
        if matches!(expr.kind, ExprKind::Call { .. }) {
            self.deferred_unresolved_binding_checks.push(super::UnresolvedBindingSite {
                ty: ity.clone(),
                name: None,
                span: expr.span,
            });
        }
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
        // `infer_control_ops.rs` / `infer_calls_closures.rs` (sub-methods returning `Option<Ty>`,
        // `Some(_)` exactly for their own variants). The chain is order-
        // independent — every `ExprKind` variant matches exactly one group — so
        // dispatching to them first, then the group-1 arms below, is identical
        // to the original single match. Group 1's remaining arms (TypeName /
        // Record / Member / TupleIndex / OptionalChain, 2026-07-20 #781) are
        // each a NAMED method that re-destructures `&mut expr.kind` internally
        // (`let PATTERN = &mut expr.kind else { unreachable!() }`) — a pure
        // text move, its `return` statements untouched since they now return
        // from a real function, not a match arm.
        if let Some(t) = self.infer_expr_inner_g2(expr) { return t; }
        if let Some(t) = self.infer_expr_inner_g3(expr) { return t; }
        match &expr.kind {
            ExprKind::TypeName { .. } => self.infer_expr_type_name(expr),
            ExprKind::Record { .. } => self.infer_expr_record(expr),
            ExprKind::Member { .. } => self.infer_expr_member(expr),
            ExprKind::TupleIndex { .. } => self.infer_expr_tuple_index(expr),
            ExprKind::OptionalChain { .. } => self.infer_expr_optional_chain(expr),
            _ => unreachable!("infer_expr_inner: ExprKind variant not in {{group1,group2,group3}}"),
        }
    }


    fn infer_expr_type_name(&mut self, expr: &mut ast::Expr) -> Ty {
        let ExprKind::TypeName { name, .. } = &mut expr.kind else { unreachable!("infer_expr_type_name called on the wrong ExprKind") };
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
                            Ty::Fn { params, ret: Box::new(ret), is_effect: false }
                        }
                        _ => Ty::Named(type_name, vec![])
                    }
                }
                else if let Some(ty) = self.env.top_lets.get(&sym(name)).cloned() { ty }
                else { Ty::Named(sym(name), vec![]) }
    }

    fn infer_expr_record(&mut self, expr: &mut ast::Expr) -> Ty {
        let ExprKind::Record { name, fields, .. } = &mut expr.kind else { unreachable!("infer_expr_record called on the wrong ExprKind") };
                for f in fields.iter_mut() { self.infer_expr(&mut f.value); }
                if let Some(n) = name {
                    self.infer_expr_record_named(n, fields)
                } else {
                    let field_tys: Vec<(Sym, Ty)> = fields.iter().map(|f| {
                        let ty = self.type_map.get(&f.value.id).map(|it| resolve_ty(it, &self.uf)).unwrap_or(Ty::Unknown);
                        (sym(&f.name), ty)
                    }).collect();
                    Ty::Record { fields: field_tys }
                }
    }

    // The `name: Some(n)` branch of `infer_expr_record` — resolving a named
    // record literal (record-variant ctor or bare named-record type) to its
    // result type, with field validation and per-field type constraints.
    fn infer_expr_record_named(&mut self, n: &Sym, fields: &Vec<ast::FieldInit>) -> Ty {
                    // A qualified record-variant name (`mod.Ctor { … }`) keys the
                    // constructor table by its BARE name, so strip any module prefix
                    // before a ctor lookup — otherwise a cross-module record-variant
                    // is mis-typed as a standalone `mod.Ctor` type (#412).
                    let ctor_sym = n.rsplit_once('.').map(|(_, b)| sym(b)).unwrap_or_else(|| sym(n));
                    if let Some(early) = self.check_record_enum_misuse(n, ctor_sym) {
                        return early;
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
                    // #631: module-aware lookup so a BARE record-variant ctor
                    // used INSIDE its owning submodule (`Circle { radius: r }`)
                    // pins `type_name` to the owner-qualified `mod.Shape`,
                    // matching the tuple-ctor path. Without this the bare result
                    // type tripped the #433 name-pinning guard at codegen.
                    let ctor_lookup = self.env.lookup_ctor_in(&ctor_sym, self.current_module_prefix.as_deref());
                    let (result_ty, decl_fields, closed, defaults): (Ty, Vec<(Sym, Ty)>, bool, std::collections::HashSet<Sym>) =
                        match ctor_lookup {
                            Some((type_name, case)) => match self.infer_expr_record_variant_ctor(n, ctor_sym, type_name, &case) {
                                Ok(v) => v,
                                Err(early) => return early,
                            },
                            None => match self.infer_expr_record_bare_type(n) {
                                Ok(v) => v,
                                Err(early) => return early,
                            },
                        };
                    self.constrain_record_fields(n, fields, &decl_fields, closed, &defaults);
                    result_ty
    }

    // Constructing `EnumType { field: ... }` via the ENUM type name (not a
    // case name) is a category error: an enum has no fields of its own.
    // Native rustc would leak E0574 and WASM silently mis-constructs, so
    // reject it here with a proper diagnostic that lists the available
    // record-variant cases. Returns `Some(Ty::Unknown)` to signal an
    // early-return to the caller, `None` to continue.
    fn check_record_enum_misuse(&mut self, n: &Sym, ctor_sym: Sym) -> Option<Ty> {
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
                return Some(Ty::Unknown);
            }
        }
        None
    }

    // #488: field-set validation (duplicates always; unknown and
    // missing-without-default for closed declarations), then E024 int-literal
    // context pinning and per-field type constraints.
    fn constrain_record_fields(&mut self, n: &Sym, fields: &Vec<ast::FieldInit>, decl_fields: &[(Sym, Ty)], closed: bool, defaults: &std::collections::HashSet<Sym>) {
        if closed || !decl_fields.is_empty() {
            let given = fields.clone();
            self.validate_record_fields(n.as_str(), &given, decl_fields, closed, defaults);
        }
        for f in fields.iter() {
            if let Some((_, ety)) = decl_fields.iter().find(|(fname, _)| fname.as_str() == f.name.as_str()) {
                // E024, record-field edition: pin a bare/negated int
                // literal to the DECLARED sized field type so the
                // post-solve range validator sees it — `Inner { x:
                // -4294967295 }` over `x: Int8` was check-accepted
                // while native rustc rejected the narrowed literal
                // (fuzz seed-20260718 index 940, the C-038 mutation).
                self.record_int_literal_context(&f.value, ety);
                if let Some(vty) = self.type_map.get(&f.value.id).cloned() {
                    self.constrain(ety.clone(), vty, format!("field {}", f.name));
                }
            }
        }
    }

    // Record-variant ctor path of `infer_expr_record_named`: `n` names a case
    // with a Record payload (`| Group { items: List[Shape] }`). Returns
    // `Err(Ty)` to signal an early-return diagnostic to the caller.
    fn infer_expr_record_variant_ctor(&mut self, n: &Sym, ctor_sym: Sym, type_name: Sym, case: &crate::types::VariantCase) -> Result<(Ty, Vec<(Sym, Ty)>, bool, std::collections::HashSet<Sym>), Ty> {
        // Brace construction of a NON-record case is a
        // category error (`Wrap { x: 1 }` on a tuple case):
        // reject here, or rustc/wasm explode downstream.
        if !matches!(case.payload, crate::types::VariantPayload::Record(_)) {
            self.emit(super::err(
                format!("case '{}' does not take named fields", n),
                format!("'{}' is a tuple or unit case — construct it positionally: `{}(...)`", n, n),
                format!("record literal {}", n),
            ).with_code("E021"));
            return Err(Ty::Unknown);
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
        Ok((Ty::Named(result_named, generic_args), decl, true, case_defaults))
    }

    // Bare named-record-type path of `infer_expr_record_named`: `n` names a
    // plain `type X = { ... }` declaration (not a record-variant ctor).
    // Returns `Err(Ty)` to signal an early-return diagnostic to the caller.
    fn infer_expr_record_bare_type(&mut self, n: &Sym) -> Result<(Ty, Vec<(Sym, Ty)>, bool, std::collections::HashSet<Sym>), Ty> {
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
        // E029: a record literal naming an UNDECLARED type
        // previously fell through with empty decl fields —
        // validation skipped, `Ty::Named(Inner)` flowed into
        // the IR, and codegen emitted a nonexistent Rust
        // struct (E0422 — check accepted, build failed; fuzz
        // seed-20260718 index 940's mutated-away decl).
        if !self.env.types.contains_key(&canon) && !self.env.types.contains_key(&sym(n)) {
            self.emit(super::err(
                format!("unknown type '{}'", n),
                format!("no `type {}` is declared (or imported) in this program — declare it, or check the spelling", n),
                format!("record literal {}", n),
            ).with_code("E029"));
            return Err(Ty::Unknown);
        }
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
        Ok((named, decl, closed, defaults))
    }

    fn infer_expr_member(&mut self, expr: &mut ast::Expr) -> Ty {
        let ExprKind::Member { object, field, .. } = &mut expr.kind else { unreachable!("infer_expr_member called on the wrong ExprKind") };
        // `infer_expr(object)` below overwrites `current_span` with the object's
        // range, so capture the Member expr's own span now. E013 uses it to
        // position the `try_replace` rewrite that covers `object.field`.
        let member_span = self.current_span;
        // A module-qualified reference (`string.len`, `utils.CATEGORY_ORDER`) is
        // resolved BEFORE the object is inferred, because inferring it would fail:
        // `string` is a module name, not a variable.
        if let Some(ty) = self.infer_module_qualified_member(object, field) {
            return ty;
        }
        let obj_ty = self.infer_expr(object);
        let concrete = resolve_ty(&obj_ty, &self.uf);
        let field_ty = self.resolve_field_type(&concrete, field);
        if matches!(field_ty, Ty::Unknown) {
            self.report_unknown_member(object, field, &concrete, member_span);
        }
        field_ty
    }

    /// Resolve `mod.name` where `mod` names a module rather than a value.
    ///
    /// Covers a stdlib signature, a user module's fn, and a cross-module
    /// top-level `let` — the Visibility section of the spec applies to `fn`,
    /// `type` AND `let`. `None` means the object is not a module reference and
    /// the caller should infer it as an ordinary expression.
    fn infer_module_qualified_member(
        &mut self,
        object: &mut ast::Expr,
        field: &almide_base::intern::Sym,
    ) -> Option<Ty> {
        if let ExprKind::Ident { name: mod_name, .. } = &object.kind {
            self.reject_dead_try_spelling(mod_name, field, object.id, object.span);
            if let Some(sig) = crate::stdlib::lookup_sig(mod_name, field) {
                self.type_map.insert(object.id, Ty::Unit); // placeholder; object isn't evaluated
                return Some(self.fn_value_ty(&sig));
            }
            let resolved_mod_name = self.env.import_table.resolve(mod_name)
                .map(|s| s.to_string())
                .unwrap_or_else(|| mod_name.to_string());
            let key = format!("{}.{}", resolved_mod_name, field);
            if let Some(sig) = self.env.functions.get(&sym(&key)).cloned() {
                self.type_map.insert(object.id, Ty::Unit);
                self.env.import_table.mark_used(mod_name);
                return Some(self.fn_value_ty(&sig));
            }
            // Cross-module top-level `let` access: `utils.CATEGORY_ORDER`.
            // Spec Visibility section applies to fn, type, AND let.
            if let Some(let_ty) = self.env.top_lets.get(&sym(&key)).cloned() {
                super::debug_trace("TOPLET", || format!("reader: key={} -> {:?}", key, let_ty));
                self.type_map.insert(object.id, Ty::Unit);
                self.env.import_table.mark_used(mod_name);
                return Some(let_ty);
            }
            // Cross-module variant constructor as value: dispatch.Never, binary.ImportFunc.
            // Owner-filtered (#1426): resolve inside the named module alone.
            let resolved_mod = self.env.import_table.resolve(mod_name)
                .unwrap_or(sym(mod_name));
            if let Some((type_name, case)) = self.env.lookup_ctor_owned(&sym(field), resolved_mod.as_str()) {
                let qualified = format!("{}.{}", resolved_mod.as_str(), type_name.as_str());
                if self.env.types.contains_key(&sym(&qualified)) {
                    self.type_map.insert(object.id, Ty::Unit);
                    let generic_args = self.instantiate_type_generics(type_name.as_str());
                    // #433: return the qualified `mod.Type` (it exists and was
                    // just confirmed) so the binding mangles to the namespaced
                    // struct, not the ambiguous bare name.
                    let qual_ty = sym(&qualified);
                    return Some(match &case.payload {
                        VariantPayload::Unit => Ty::Named(qual_ty, generic_args),
                        VariantPayload::Tuple(param_tys) => Ty::Fn {
                            params: param_tys.clone(),
                            ret: Box::new(Ty::Named(qual_ty, generic_args)),
                            is_effect: false,
                        },
                        VariantPayload::Record(_) => Ty::Named(qual_ty, generic_args),
                    });
                }
            }
        }
        None
    }

    /// Emit the E013 for a field access that resolved to no field.
    ///
    /// LLMs trained on Haskell / Python / Ruby write `xs.head`, `xs.tail`,
    /// `xs.length`, `s.length`. In Almide those are stdlib calls, so the
    /// diagnostic is intercepted here and carries the mechanical rewrite —
    /// otherwise rustc leaks `error[E0609]: no field 'head' on type 'Vec<i64>'`
    /// from generated code the user never wrote.
    fn report_unknown_member(
        &mut self,
        object: &ast::Expr,
        field: &almide_base::intern::Sym,
        concrete: &Ty,
        member_span: Option<crate::ast::Span>,
    ) {
        self.report_missing_record_field(object, field, concrete, member_span);
        self.suggest_stdlib_for_member(object, field, concrete, member_span);
    }

    /// #847: a MISSING field on a CLOSED record used to sail through as
    /// `Unknown` with no diagnostic at all — the failure surfaced as a codegen
    /// postcondition ICE, or leaked rustc's E0609 from code the user never wrote.
    /// Reported here with the record's field roster.
    fn report_missing_record_field(
        &mut self,
        object: &ast::Expr,
        field: &almide_base::intern::Sym,
        concrete: &Ty,
        member_span: Option<crate::ast::Span>,
    ) {
    // #1120: `.field` on an Option (forgetting the `?`) used to sail through
    // as Unknown and die at the ConcretizeTypes wall. Suggest the operator
    // that exists for exactly this: `?.` (ADR-0005 D2).
    if let Ty::Applied(crate::types::TypeConstructorId::Option, args) = &concrete {
        let inner_display = args.first().map(|t| t.display()).unwrap_or_else(|| "T".to_string());
        let mut diag = super::err(
            format!("field access '.{}' on {} — the value is optional", field, concrete.display()),
            format!("Use optional chaining: `?.{f}` yields Option[field type] ({inner} may be absent). \
                     To unwrap first: `?? fallback`, or `match` on some/none.", f = field, inner = inner_display),
            format!("field access .{}", field),
        ).with_code("E013");
        // SUGGESTION, not machine-applicable (#1312): `?.` yields
        // `Option[field]` where the surrounding code asked for the field
        // itself, so applying it moves the problem one expression out.
        // Unwrapping (`??`, `match`) is the other legal reading.
        if let (Some(span), Some(obj_src)) = (member_span, object.span.and_then(|s| self.source_slice(s))) {
            diag = diag.with_suggested_fix(
                span.line, span.col, span.end_col,
                format!("{}?.{}", obj_src, field),
            );
        }
        self.emit(diag);
        return;
    }
    // #847: a MISSING field on a closed record used to sail
    // through as Unknown (no diagnostic at all — the failure
    // surfaced as a codegen postcondition ICE, or leaked
    // rustc's E0609). Report it here with the field roster.
    let record_shape = self.env.resolve_named(&concrete);
    if let Ty::Record { fields } = &record_shape {
        let available = fields.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ");
        let suggestion = almide_base::diagnostic::suggest(
            field, fields.iter().map(|(n, _)| n.as_str()));
        let hint = match &suggestion {
            Some(close) => format!("Did you mean `{}`? Available fields: {}", close, available),
            None => format!("Available fields: {}", available),
        };
        let mut diag = super::err(
            format!("no field '{}' on {}", field, concrete.display()),
            hint,
            format!("field access .{}", field),
        ).with_code("E013");
        // SUGGESTION: the field name came from an edit distance over the
        // record's roster — a plausible neighbour, not a known intent.
        if let (Some(close), Some(span)) = (&suggestion, member_span) {
            if let Some(obj_src) = object.span.and_then(|s| self.source_slice(s)) {
                diag = diag.with_suggested_fix(
                    span.line, span.col, span.end_col,
                    format!("{}.{}", obj_src, close),
                );
            }
        }
        self.emit(diag);
    }
    }

    /// Rewrite a Haskell/Python/Ruby-style field access into the Almide stdlib
    /// call it means (`xs.head` → `list.first(xs)`).
    fn suggest_stdlib_for_member(
        &mut self,
        object: &ast::Expr,
        field: &almide_base::intern::Sym,
        concrete: &Ty,
        member_span: Option<crate::ast::Span>,
    ) {
    let module_and_subs: Option<(&str, &[MemberRewrite])> = match &concrete {
        Ty::Applied(TypeConstructorId::List, _) => Some(("list", LIST_MEMBER_REWRITES)),
        Ty::String => Some(("string", STRING_MEMBER_REWRITES)),
        _ => None,
    };
    if let Some((module, subs)) = module_and_subs {
        let matched = subs.iter().find(|r| r.field == field.as_str());
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
        if let Some(rule) = matched {
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
                    let args = rule.args_tpl.replace("{0}", &obj_src);
                    Some((span, format!("{}{}", rule.fn_name, args)))
                });
            if let Some((span, snippet)) = rewrite {
                // #1312: the per-cell applicability decides whether
                // `almide fix` may apply this unattended.
                diag = if rule.applicability.is_machine_applicable() {
                    diag.with_machine_fix(span.line, span.col, span.end_col, snippet)
                } else {
                    diag.with_suggested_fix(span.line, span.col, span.end_col, snippet)
                };
            } else {
                let display = format!(
                    "{}{}{}",
                    rule.fn_name,
                    rule.args_tpl.replace("{0}", "xs"),
                    rule.display_suffix,
                );
                diag = diag.with_try(display);
            }
        }
        self.emit(diag);
    }
    }

    fn infer_expr_tuple_index(&mut self, expr: &mut ast::Expr) -> Ty {
        let ExprKind::TupleIndex { object, index, .. } = &mut expr.kind else { unreachable!("infer_expr_tuple_index called on the wrong ExprKind") };
                let obj_ty = self.infer_expr(object);
                if let Ty::Tuple(elems) = &obj_ty {
                    if *index < elems.len() { return elems[*index].clone(); }
                }
                let concrete = resolve_ty(&obj_ty, &self.uf);
                match &concrete {
                    Ty::Tuple(elems) if *index < elems.len() => elems[*index].clone(),
                    // Out of range on a KNOWN tuple: a check-time error, never a
                    // silent `Unknown` (#1266 — the Unknown sailed through check
                    // and died at build as a [COMPILER BUG] banner that told the
                    // user their own type error was ours).
                    Ty::Tuple(elems) => {
                        self.emit(
                            super::err(
                                format!(
                                    "tuple index .{index} is out of range for {} (valid: .0 through .{})",
                                    concrete.display(),
                                    elems.len() - 1
                                ),
                                format!("the tuple has {} element(s)", elems.len()),
                                "tuple index",
                            )
                            .with_code("E045"),
                        );
                        Ty::Unknown
                    }
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
                    // An object that already failed to type keeps its silence —
                    // the upstream error owns the report, a second one is noise.
                    Ty::Unknown => Ty::Unknown,
                    // `.k` on a concrete NON-tuple (`n.0` over Int): the other
                    // half of #1266, also a check-time error now.
                    other => {
                        self.emit(
                            super::err(
                                format!("tuple index .{index} on non-tuple type {}", other.display()),
                                "only tuple values support positional .k access",
                                "tuple index",
                            )
                            .with_code("E045"),
                        );
                        Ty::Unknown
                    }
                }
    }

    fn infer_expr_optional_chain(&mut self, expr: &mut ast::Expr) -> Ty {
        let ExprKind::OptionalChain { expr: inner, field, .. } = &mut expr.kind else { unreachable!("infer_expr_optional_chain called on the wrong ExprKind") };
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

}

include!("infer_control_ops.rs");
include!("infer_calls_closures.rs");
include!("infer_statements.rs");
include!("infer_ident_collection.rs");
include!("infer_loops_records.rs");

impl Checker {
    /// ADR-0006 D3 (#1108): reject a DEAD HOF spelling at the call site.
    ///
    /// Two families land here, and the distinction is the whole point:
    ///
    ///   - `list.try_map` … — the seven PUBLIC names deleted in 0.56.0. The
    ///     core HOF is fallibility-polymorphic, so the callback's `!` selects
    ///     the fallible form and one name per combinator is enough.
    ///   - `list.__fallible_map` … — the seven INTERNAL carriers those names
    ///     left behind. Deleting the public spelling but leaving the carrier
    ///     reachable did not remove the second spelling, it RENAMED it: a
    ///     user (or a model that read stdlib/list.almd) could write the
    ///     carrier and it compiled, ran, and warned about nothing — while the
    ///     IDE outline offered it by name. For a language whose metric is
    ///     modification survival rate, two working spellings where one is
    ///     undocumented is worse than the deprecated name was: the writer
    ///     cannot tell which is blessed. The carrier stays as the desugar
    ///     TARGET; it is simply no longer something source may name.
    ///
    ///     The carriers were themselves called `__try_*` until they were
    ///     renamed to `__fallible_*`: `try` is the loan-word this ADR
    ///     rejected (its lenders disagree — Rust's `try_fold` short-circuits,
    ///     Swift/Zig's `try` propagates, which is almide's `!`), so keeping
    ///     it as the internal name contradicted the reason for deleting it.
    ///     `fallible` is ADR-0006's own word for the form.
    ///
    /// Fires only on a USER-SPELLED name: the normalization's own rewrites
    /// are registered in `hof_rewritten_calls` and skipped.
    pub(crate) fn reject_dead_try_spelling(
        &mut self,
        mod_name: &almide_base::intern::Sym,
        field: &almide_base::intern::Sym,
        object_id: almide_lang::ast::ExprId,
        object_span: Option<almide_lang::ast::Span>,
    ) {
        let module = mod_name.as_str();
        if !matches!(module, "list" | "fs") || self.hof_rewritten_calls.contains(&object_id) {
            return;
        }
        let name = field.as_str();
        let (core, internal) = match name.strip_prefix("__fallible_") {
            Some(core) => (core, true),
            None => match name.strip_prefix("try_") {
                Some(core) => (core, false),
                None => return,
            },
        };
        // #1144: the fs streaming walkers carry the same carriers, so they need
        // the same "not a spelling" guard — `fs.__fallible_fold_lines` must be
        // as unwritable as `list.__fallible_map`.
        let known = match module {
            "list" => matches!(
                core,
                "map" | "filter" | "flat_map" | "filter_map" | "fold" | "find" | "each"
            ),
            _ => matches!(core, "fold_lines" | "for_each_line"),
        };
        if !known {
            return;
        }
        let rewrite = match (module, core) {
            ("list", "fold") => "list.fold(xs, z, (a, x) => f(a, x)!)!".to_string(),
            ("list", _) => format!("list.{}(xs, (x) => f(x)!)!", core),
            (_, "fold_lines") => "fs.fold_lines(path, z, (a, l) => f(a, l)!)!".to_string(),
            _ => "fs.for_each_line(path, (l) => f(l)!)!".to_string(),
        };
        let (msg, hint) = if internal {
            (
                format!(
                    "{module}.{name} is an internal carrier, not a spelling — source may not name it (ADR-0006)"
                ),
                // The `list` carriers are what `try_*` left behind, so their
                // hint names that history; the fs carriers (#1144) never had a
                // public `try_` name and only ever existed as a desugar target.
                if module == "list" {
                    format!(
                        "{rewrite}\n        \
                         `__fallible_{core}` is what the checker instantiates FOR you when the callback \
                         propagates; writing it by hand is the second spelling `try_{core}`'s \
                         removal was meant to end."
                    )
                } else {
                    format!(
                        "{rewrite}\n        \
                         `__fallible_{core}` is the desugar TARGET the checker instantiates FOR you \
                         when the callback propagates — one blessed spelling per combinator, never two."
                    )
                },
            )
        } else {
            (
                format!(
                    "{module}.{name} was removed — the core HOF is fallibility-polymorphic (ADR-0006)"
                ),
                format!(
                    "{rewrite}\n        \
                     The callback's `!` instantiates the fallible form (first-err short-circuit); \
                     the try_ family's one name per combinator is the core name."
                ),
            )
        };
        let mut d = crate::diagnostic::Diagnostic::error(msg, hint, format!("call to {module}.{name}"))
            .with_code("E043");
        d.file = self.source_file.clone();
        if let Some(sp) = object_span {
            d.line = Some(sp.line);
            d.col = Some(sp.col);
        }
        self.diagnostics.push(d);
    }
}
