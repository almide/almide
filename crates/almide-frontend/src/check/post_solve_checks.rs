// The post-solve validation passes on `Checker`. `include!`d by
// post_solve_validation.rs (the 800-line file budget); it shares that
// module's scope and imports.

impl Checker {
    /// Validate that all Map literal key types are hashable (post-solve).
    /// ALS-C9 / E030: an order-sensitive combinator (`list.sort`/`min`/`max`,
    /// `sort_by`'s key) needs an ORDERABLE element — the native runtime's
    /// `T: Ord` bound. A bare Float element is fine (it routes to the `_float`
    /// twins); a Map/Set/Fn element — or Float NESTED inside a compound — has
    /// no order, and previously check accepted it while native rustc rejected
    /// the monomorph ("AlmideMap: Ord is not satisfied" — the check-vs-build
    /// gap, fuzz seed-20260718 index 629).
    fn validate_ord_elem_types(&mut self) {
        use std::collections::HashSet;
        let mut reported: HashSet<String> = HashSet::new();
        let checks = std::mem::take(&mut self.deferred_ord_elem_checks);
        for (subject_ty, span, fn_name) in checks {
            let resolved = resolve_ty(&subject_ty, &self.uf);
            // list.sort/min/max enqueue the LIST subject; sort_by enqueues the
            // key type directly. Extract the element when it is a List.
            let elem = match &resolved {
                Ty::Applied(almide_lang::types::TypeConstructorId::List, a) if a.len() == 1 => {
                    a[0].clone()
                }
                _ => resolved.clone(),
            };
            // An unresolved slot is E025's business; a BARE Float rides the
            // `_float` twins.
            if matches!(elem, Ty::Unknown | Ty::TypeVar(_) | Ty::Float) {
                continue;
            }
            if self.env.is_ord(&elem) {
                continue;
            }
            let ty_name = Self::type_display_name(&elem);
            if !reported.insert(format!("{fn_name}:{ty_name}")) {
                continue;
            }
            let mut diag = err(
                format!("type '{}' has no ordering — cannot be used with {}", ty_name, fn_name),
                // Adjacent literals, not one line with the source indentation baked
                // into it (#2167): the hint used to render with 18-space runs where
                // its line breaks had been. It must also name the DERIVE — a record
                // or variant orders, but only when it declares `: Ord` (#1521), and a
                // reader whose record was rejected for exactly that reason was being
                // told records are fine.
                concat!(
                    "Ordering needs Int, Bool, String or Float, or a tuple/list/Option of those. ",
                    "A record or variant orders too \u{2014} by field declaration order, by case ",
                    "order \u{2014} but only when it DECLARES the derive: `type T: Ord = { ... }`. ",
                    "Map, Set and function values have no order, and a Float INSIDE a compound has ",
                    "none either (native's derive cannot order f64) \u{2014} compare via an explicit key instead.",
                )
                .to_string(),
                format!("call to {}", fn_name),
            ).with_code("E030");
            if let Some(s) = span {
                diag.line = Some(s.line);
                diag.col = Some(s.col);
            }
            self.diagnostics.push(diag);
        }
    }

    /// E016 (#2606): `==` / `!=` / `assert_eq` / `assert_ne` need an operand
    /// type with EQUALITY, and "Function types are never Eq"
    /// (docs/specs/type-system.md) — through any Result / Option / List /
    /// tuple / record field / variant payload. The checker used to accept the
    /// comparison and leave it to rustc (E0369 on `Rc<dyn Fn>`, plus E0277 for
    /// the missing `Debug` in `assert_eq`); E016 is the class that already
    /// rejects a function in a Set element or Map key for the same reason.
    fn validate_eq_operand_types(&mut self) {
        use std::collections::HashSet;
        let mut reported: HashSet<(usize, usize)> = HashSet::new();
        let checks = std::mem::take(&mut self.deferred_eq_checks);
        for (ty, span, what) in checks {
            let resolved = resolve_ty(&ty, &self.uf);
            if self.env.is_eq(&resolved) {
                continue;
            }
            if span.is_some_and(|s| !reported.insert((s.line, s.col))) {
                continue;
            }
            let ty_name = resolved.display();
            let mut diag = err(
                format!("type '{}' has no equality — {} cannot compare it (it contains a function)", ty_name, what),
                concat!(
                    "Function values have no equality, so neither does a Result, Option, list, tuple, ",
                    "record or variant that holds one. Compare what you can observe instead: the tag ",
                    "(`result.is_err(r)`, `option.is_some(o)`), the error (`result.to_err_option(r) == some(e)`), ",
                    "or the function's output on a sample input.",
                )
                .to_string(),
                what.clone(),
            ).with_code("E016");
            if let Some(s) = span {
                diag.line = Some(s.line);
                diag.col = Some(s.col);
            }
            self.diagnostics.push(diag);
        }
    }

    /// E029: every `Ty::Named` mentioned in an ANNOTATION must be a declared
    /// type. An undeclared name flowed through unification unconstrained
    /// (`let xs: List[Inner] = []`) and compiled to a nonexistent Rust type
    /// (E0412/E0425) — check accepted, build failed (the acceptance-parity
    /// gap, differential-fuzz seed 20260718 index 940's mutated-away `type`
    /// declaration). Generic params are immune: resolve_type_expr turns an
    /// in-scope generic into `Ty::TypeVar` at annotation time, never `Named`.
    fn validate_unknown_named_types(&mut self) {
        use std::collections::HashSet;
        fn collect_named(ty: &Ty, out: &mut Vec<Sym>) {
            match ty {
                Ty::Named(s, args) => {
                    out.push(*s);
                    for a in args { collect_named(a, out); }
                }
                Ty::Applied(_, args) | Ty::Tuple(args) | Ty::Union(args) => {
                    for a in args { collect_named(a, out); }
                }
                Ty::Fn { is_effect: _, params, ret } => {
                    for p in params { collect_named(p, out); }
                    collect_named(ret, out);
                }
                Ty::Record { fields } | Ty::OpenRecord { fields } => {
                    for (_, f) in fields { collect_named(f, out); }
                }
                // A VARIANT's payload types are annotations too: an undeclared
                // name in `type Tree = | Leaf(Payload)` flowed to codegen as a
                // nonexistent Rust type (E0425) with check silent — the same
                // acceptance-parity gap E029 closed for let/param/return
                // positions (reference-port sweep finding, 2026-08-18). The
                // variant's own name is in env.types by registration, so only
                // dangling payload references survive the declared-name skip.
                Ty::Variant { name: _, cases } => {
                    for c in cases {
                        match &c.payload {
                            almide_lang::types::VariantPayload::Tuple(ts) => {
                                for t in ts { collect_named(t, out); }
                            }
                            almide_lang::types::VariantPayload::Record(fs) => {
                                for (_, t) in fs { collect_named(t, out); }
                            }
                            almide_lang::types::VariantPayload::Unit => {}
                        }
                    }
                }
                _ => {}
            }
        }
        let mut reported: HashSet<Sym> = HashSet::new();
        // Names an E029 was actually emitted for (#2771): their held
        // consequences are dropped, everything else held is released.
        let mut rooted: HashSet<Sym> = HashSet::new();
        let mut roots: Vec<Diagnostic> = Vec::new();
        let checks = std::mem::take(&mut self.deferred_unknown_type_checks);
        let mut arity_seen: HashSet<(Sym, usize)> = HashSet::new();
        for (ty, span, ctx) in checks {
            let resolved = resolve_ty(&ty, &self.uf);
            roots.extend(self.type_arity_diags(&resolved, span, &ctx, &mut arity_seen));
            let mut names = Vec::new();
            collect_named(&resolved, &mut names);
            for s in names {
                // `Value` is the BUILT-IN dynamic type (json/codec surface) —
                // nominal by name but never declared in env.types.
                if s.as_str() == "Value" || self.env.types.contains_key(&s) || !reported.insert(s) {
                    continue;
                }
                // Its qualified spelling already carries the root E029 (#3336).
                if self.qualified_type_misses.contains(&s) {
                    rooted.insert(s);
                    continue;
                }
                // A runtime-backed stdlib nominal (`HttpRequest`, bare or
                // `http.`-qualified) is a valid annotation whenever its owner
                // module is in scope — the stdlib's own signatures use it, so
                // the writer must be able to spell it too (#1053). Without the
                // import, the generic "declare it" hint would be a dead end
                // (the type can neither be declared nor selectively imported);
                // name the one action that works instead.
                if let Some(owner) = almide_lang::stdlib_info::runtime_backed_type_owner(s.as_str()) {
                    if self.env.import_table.is_module(owner) {
                        continue;
                    }
                    let mut diag = err(
                        format!("unknown type '{}'", s),
                        format!(
                            "'{}' is the `{owner}` stdlib module's runtime-backed type — \
                             add `import {owner}` and it resolves in annotations",
                            s,
                        ),
                        ctx.clone(),
                    ).with_code("E029").with_try(format!("import {owner}"));
                    if let Some(sp) = span {
                        diag.line = Some(sp.line);
                        diag.col = Some(sp.col);
                    }
                    roots.push(diag);
                    continue;
                }
                // #1590: the name IS declared — as a PROTOCOL. "unknown
                // type" reads as a typo to the writer who declared it two
                // lines up; name the actual boundary instead.
                rooted.insert(s);
                let diag = if self.env.protocols.contains_key(&s) {
                    let mut d = err(
                        format!("'{}' is a protocol, not a type", s),
                        format!(
                            "A protocol cannot be used as a value type yet (#1589 — no dyn dispatch): a parameter or field cannot hold `any {}`. Take the concrete adopting type instead.",
                            s
                        ),
                        ctx.clone(),
                    ).with_code("E029");
                    if let Some(sp) = span {
                        d.line = Some(sp.line);
                        d.col = Some(sp.col);
                    }
                    d
                } else {
                    self.unknown_type_diag(s.as_str(), span, ctx.clone())
                };
                roots.push(diag);
            }
        }
        // The root causes go ahead of this pass's body diagnostics — the
        // errors their unknown types caused were emitted during inference,
        // before this post-solve walk ran (#2771).
        let at = self.body_diag_start.min(self.diagnostics.len());
        self.diagnostics.splice(at..at, roots);
        self.release_type_cascade(&rooted);
    }

    /// Post-solve #1051: warn when an interpolation segment holds a Result the
    /// lowering will not auto-? — it prints the debug form (`ok(…)`/`err(…)`),
    /// which is legal for debug output but a silent surprise when the writer
    /// meant the payload (the classic shape: a can-err call bound inside a
    /// lambda, then `"${resp}"`). Warning, not error: interpolating the
    /// Result itself is how you debug one.
    /// #1123 / ADR-0008 N+1: the switch. Every site the pre-switch
    /// implementation auto-?'d is now a hard error — the value stays a Result:
    ///   E041 — implicit propagation (un-annotated let/var, condition,
    ///          value-pattern match subject, interpolation, assignment)
    ///   E042 — must-use: a statement-position Result silently discarded
    /// The checker still strips the Result in the TypeMap as RECOVERY (so one
    /// error does not cascade into downstream mismatches), but a flagged
    /// program never compiles. The try-replace appends `!` at the
    /// expression's end — a zero-width insertion, so the migration stays a
    /// mechanical apply (`almide check --json` + span apply).
    fn validate_implicit_propagation(&mut self) {
        let checks = std::mem::take(&mut self.deferred_implicit_prop_checks);
        // One report per span. A leaf several sites reach (#2182: a `match`
        // arm's value queued by the arm join AND by the statement or `-> Unit`
        // tail that discards it) keeps its first entry, upgraded to must-use
        // when any later entry says the value is discarded — E042 outranks
        // E041, and the discarding position names itself.
        let mut order: Vec<(usize, usize)> = Vec::new();
        let mut by_span: std::collections::HashMap<(usize, usize), (Ty, ast::Span, &'static str, bool, bool)> =
            std::collections::HashMap::new();
        for (ty, span, what, mechanical, must_use) in checks {
            let resolved = resolve_ty(&ty, &self.uf);
            if !resolved.is_result() {
                continue;
            }
            let Some(s) = span else { continue };
            match by_span.entry((s.line, s.col)) {
                std::collections::hash_map::Entry::Vacant(e) => {
                    order.push((s.line, s.col));
                    e.insert((ty, s, what, mechanical, must_use));
                }
                std::collections::hash_map::Entry::Occupied(mut e) => {
                    if must_use && !e.get().4 {
                        e.get_mut().2 = what;
                        e.get_mut().4 = true;
                    }
                }
            }
        }
        for key in order {
            let Some((_, s, what, mechanical, must_use)) = by_span.remove(&key) else { continue };
            let mut d = if must_use {
                // A TAIL position (the fn's tail, a guard's else) does not drop
                // the value: the Result IS the fn's value, so its error leaves
                // through the fn's own error channel — the propagation is real
                // and merely unspelled (#2277: the old "silently dropped" text
                // was false for a `-> Unit` effect callee, and read as "no
                // Result is involved"). Only a statement position drops it.
                let (discarded, hint) = match what {
                    "of this fn's tail value" => (
                        "the tail of this `-> Unit` effect fn is a Result (an effect call's error channel), and its error propagates implicitly — ADR-0008 spells every propagation",
                        "Write `expr!` to propagate it explicitly (the fn still fails on the error; nothing changes at run time), or `let _ = expr` to discard the error on purpose. Matching on ok/err also consumes it.",
                    ),
                    "of this guard's else value" => (
                        "this guard's else value is a Result (an effect call's error channel) leaving a `-> Unit` fn, and its error propagates implicitly — ADR-0008 spells every propagation",
                        "Write `expr!` to propagate it explicitly (the fn still fails on the error; nothing changes at run time), or `let _ = expr` to discard the error on purpose. Matching on ok/err also consumes it.",
                    ),
                    _ => (
                        "this statement discards a Result — the error would be silently dropped",
                        "Propagate it with `expr!`, or discard it on purpose with `let _ = expr` (the explicit-discard spelling, ADR-0008 D2). Matching on ok/err also consumes it.",
                    ),
                };
                Diagnostic::error(discarded.to_string(), hint, "unused Result").with_code("E042")
            } else {
                Diagnostic::error(
                    format!("implicit propagation {} was removed — this value is a Result (ADR-0008)", what),
                    "The auto-? of the 0.54 deprecation window (E041) is gone: a fallible \
                     call yields a Result VALUE in every position. Write `expr!` to \
                     propagate, or consume the Result as a value (`??`, `?`, match ok/err).",
                    "implicit propagation",
                ).with_code("E041")
            };
            d.file = self.source_file.clone();
            d.line = Some(s.line);
            d.col = Some(s.col);
            d.end_col = Some(s.end_col);
            // The insertion is attached only for a PLAIN CALL site — a
            // compound RHS (pipe chains etc.) has a span that ends inside
            // the expression, and a blind append corrupts the source
            // (`x |>! f` — caught by the harness's try-apply gate).
            // Tagged as a SUGGESTION, not machine-applicable (#1312):
            // `!` is one of several legal consumptions of a Result
            // (`??`, `?`, match on ok/err, `let _ =`), and picking
            // propagation is a semantic decision about how this program
            // handles failure. The span is exact, so the fix-it is
            // offered and IDE/model-appliable — `almide fix` just won't
            // choose for the author.
            // And only when the span's text really ENDS the expression
            // (#2250): a `Span` carries no end line, so a call whose `)` sits
            // on a later line carries the span of its `(` alone, and a `!`
            // inserted at that end column lands mid-call. A harness applying
            // `suggestions[]` would corrupt the source, so a span whose text
            // is not a closed call (or a bare name) — or that the source text
            // cannot locate at all — gets the fix as display only, never as a
            // position.
            if mechanical
                && s.end_col > s.col
                && self.source_slice(s).is_some_and(|text| Self::fix_anchor_ends_expression(&text))
            {
                d = d.with_suggested_fix(s.line, s.end_col, s.end_col, "!");
            }
            self.diagnostics.push(d);
        }
    }

    /// #2250: whether inserting `!` right after `slice` (a span's text on its
    /// own line) appends to a whole expression. True for a call closed on
    /// this line — ends in `)` with its parentheses balanced outside string
    /// literals — and for a bare (possibly dotted) name. False for the lone
    /// `(` a multi-line call carries as its span, and for any span that stops
    /// inside the expression it names.
    pub(crate) fn fix_anchor_ends_expression(slice: &str) -> bool {
        let s = slice.trim_end();
        if s.is_empty() {
            return false;
        }
        if s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.') {
            return !s.ends_with('.');
        }
        if !s.ends_with(')') {
            return false;
        }
        let (mut depth, mut in_str, mut prev) = (0i64, false, '\0');
        for c in s.chars() {
            if in_str {
                if c == '"' && prev != '\\' {
                    in_str = false;
                }
            } else {
                match c {
                    '"' => in_str = true,
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth < 0 {
                            return false;
                        }
                    }
                    _ => {}
                }
            }
            prev = c;
        }
        depth == 0 && !in_str
    }

    fn validate_map_key_types(&mut self) {
        use std::collections::HashSet;
        let mut reported: HashSet<String> = HashSet::new();
        let checks = std::mem::take(&mut self.deferred_map_key_checks);
        for (key_ty, span) in checks {
            let resolved = resolve_ty(&key_ty, &self.uf);
            if !self.env.is_hash(&resolved) {
                let ty_name = Self::type_display_name(&resolved);
                reported.insert(ty_name.clone());
                let mut diag = Self::unhashable_key_diag(&self.env, &resolved, &ty_name, "map literal");
                if let Some(s) = span {
                    diag.line = Some(s.line);
                    diag.col = Some(s.col);
                }
                self.diagnostics.push(diag);
            }
        }
        // #598: the deferred queue only catches map LITERALS. Sweep the SOLVED
        // type map for EVERY Map[K, V] — so the stdlib builder API
        // (map.new/map.set/from_list/insert) gets the same authoritative
        // unhashable-key rejection the literal already gets, in the frontend
        // (identical on both targets). Previously an unhashable Float key built
        // via map.from_list passed `check`, then on wasm SILENTLY collapsed
        // distinct keys (len 2 → 1) or produced a [COMPILER BUG] invalid module.
        let map_keys: Vec<crate::types::Ty> = self.type_map.values()
            .filter_map(|t| match t {
                crate::types::Ty::Applied(almide_lang::types::TypeConstructorId::Map, args) if args.len() == 2 =>
                    Some(args[0].clone()),
                _ => None,
            })
            .collect();
        for key_ty in map_keys {
            let resolved = resolve_ty(&key_ty, &self.uf);
            // Skip inference vars that never pinned down — that is the empty
            // collection class, reported elsewhere; only fire on a CONCRETE
            // unhashable key.
            if matches!(resolved, crate::types::Ty::Unknown | crate::types::Ty::TypeVar(_)) {
                continue;
            }
            if !self.env.is_hash(&resolved) {
                let ty_name = Self::type_display_name(&resolved);
                if reported.insert(ty_name.clone()) {
                    self.diagnostics.push(Self::unhashable_key_diag(&self.env, &resolved, &ty_name, "map key"));
                }
            }
        }
    }

    /// The unhashable-Map-key diagnostic, coded by CAUSE (#1518: this error
    /// used to carry no code at all, so `almide explain` and the coverage
    /// gate never saw it). A key blocked by a FUNCTION anywhere in its type —
    /// including a fn-typed field reached through a Named record, which the
    /// bind-site E016 recursion does not expand — is E016's own class and
    /// keeps its closure wording; every other blocker (Float, Map) is E058.
    fn unhashable_key_diag(env: &crate::type_env::TypeEnv, resolved: &crate::types::Ty, ty_name: &str, context: &str) -> Diagnostic {
        match env.hash_blocker(resolved) {
            Some("a function") => err(
                format!("type '{}' is not hashable — cannot be used as a Map key", ty_name),
                "A function-typed field anywhere in the key type makes it unusable: closures have no equality or hashing. Closures are fine as `Map` values; only the key must be comparable.".to_string(),
                context.to_string(),
            ).with_code("E016"),
            blocker => err(
                format!("type '{}' is not hashable — cannot be used as a Map key{}", ty_name,
                    blocker.map(|b| format!(" (it contains {})", b)).unwrap_or_default()),
                "Map keys must be hashable. Use String, Int, Bool, or a record/variant with only hashable fields.".to_string(),
                context.to_string(),
            ).with_code("E058"),
        }
    }

    /// Reject empty-collection producers whose element type the program never
    /// pins down (post-solve). The Rust/Swift rule: an empty `[]`/`[:]`/set whose
    /// element type cannot be inferred from any surrounding context is a COMPILE
    /// ERROR — not a slot codegen may silently default. Firing here, in the
    /// frontend, makes the error identical on BOTH targets (Rust rustc would
    /// reject `Vec::<_>::new()` with E0282; wasm carries no element type and used
    /// to run — that cross-target asymmetry is what this closes). Observability is
    /// irrelevant, exactly as in Rust/Swift: `for _ in []` is an error even though
    /// the element is never read. Each `?A` that survived the whole-program solve
    /// (against `self.uf`) had no inference source; a populated/annotated form
    /// would have unified it the normal way and resolved clean here.
    /// Post-solve range check for int literals that overflow `i64` (#626). The
    /// effective type is the binding's declared type if one was recorded, else
    /// the literal's resolved type, else a default `Int` (i64). A literal that
    /// does not fit that type's range — and is not the negated `i64::MIN`
    /// magnitude — would silently fold to 0 in codegen, so it is rejected (E024).
    fn validate_int_overflow_literals(&mut self) {
        let checks = std::mem::take(&mut self.deferred_int_overflow_checks);
        for site in checks {
            // Effective type: explicit binding annotation, else the literal's
            // own resolved type, else default Int.
            let eff = site.context_ty.clone()
                .map(|t| resolve_ty(&t, &self.uf))
                .or_else(|| self.type_map.get(&site.expr_id).map(|t| resolve_ty(t, &self.uf)))
                .unwrap_or(Ty::Int);
            // Only a concrete integer context decides this; an unresolved/var or
            // non-integer effective type is left to the normal checker.
            let eff = match eff {
                Ty::Int | Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64
                | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64 => eff,
                _ => Ty::Int, // fall back to the default Int context
            };
            let hint = match classify_int_literal(&site.raw, &eff, site.negated) {
                LiteralFit::Fits => continue,
                // Not a magnitude overflow: no negative value is representable
                // at all, so "use a smaller literal" is the wrong advice.
                LiteralFit::Sign => format!(
                    "{} is unsigned — it has no negative values at all; drop the '-', \
                     or use the signed {} if the value can go below zero",
                    eff.display(),
                    signed_counterpart(&eff).unwrap_or("Int"),
                ),
                // State the range rather than referring to it. Rust puts it in
                // a note on the same error, and it is the difference between a
                // hint that can be acted on and one that sends the reader to go
                // look up a constant.
                LiteralFit::Magnitude => format!(
                    "{} would silently fold to 0 here; its range is {}, so use a literal \
                     within it, or model larger magnitudes as Float (lossy) or a parsed string",
                    eff.display(),
                    int_type_range(&eff).unwrap_or_else(|| "the type's".to_string()),
                ),
            };
            // Show the literal as WRITTEN — a bare `-` is part of what is out of
            // range, and "literal '5' is out of range for UInt64" reads as a
            // compiler bug when the source says `-5`.
            let shown = if site.negated { format!("-{}", site.raw) } else { site.raw.clone() };
            let mut diag = err(
                format!("integer literal '{}' is out of range for {}", shown, eff.display()),
                hint,
                format!("integer literal {}", shown),
            ).with_code("E024");
            if let Some(s) = site.span {
                diag.file = self.source_file.clone();
                diag.line = Some(s.line);
                diag.col = Some(s.col);
                if s.end_col > s.col { diag.end_col = Some(s.end_col); }
            }
            self.diagnostics.push(diag);
        }
    }

    /// Post-solve range check for float literals at a Float32 effective type
    /// whose magnitude exceeds f32's finite range (Wave 4 L7): check accepted
    /// `let p: Float32 = 1e300…` while native rustc rejected the emitted
    /// `1e300f32` ("literal out of range for f32") — a check-green build-red
    /// split, the float sibling of the E024 integer domain. Only pre-filtered
    /// literals (finite as f64, infinite as f32) reach here; a Float/Float64
    /// effective type passes trivially.
    fn validate_float_overflow_literals(&mut self) {
        let checks = std::mem::take(&mut self.deferred_float_overflow_checks);
        for site in checks {
            let eff = site
                .context_ty
                .clone()
                .map(|t| resolve_ty(&t, &self.uf))
                .or_else(|| self.type_map.get(&site.expr_id).map(|t| resolve_ty(t, &self.uf)));
            if !matches!(eff, Some(Ty::Float32)) {
                continue;
            }
            let mut diag = err(
                format!("float literal '{}' is out of range for Float32", site.value),
                "Float32's finite range is ±3.4028235e38; use a literal within it, \
                 or use Float (f64) for larger magnitudes"
                    .to_string(),
                format!("float literal {}", site.value),
            )
            .with_code("E024");
            if let Some(s) = site.span {
                diag.file = self.source_file.clone();
                diag.line = Some(s.line);
                diag.col = Some(s.col);
                if s.end_col > s.col { diag.end_col = Some(s.end_col); }
            }
            self.diagnostics.push(diag);
        }
    }

    /// Post-solve directional check for annotated bindings (#867): a narrow
    /// sized numeric VALUE does not flow into an `Int`/`Float` annotation —
    /// the emitted Rust would be an E0308 (i32 where i64 expected), so the
    /// checker rejects it and names the explicit `.to_int64()` idiom. The
    /// solver itself joins numeric widths symmetrically (peer sites must not
    /// depend on visit order), which is why annotation sites re-check here
    /// with the direction the source actually states.
    fn validate_numeric_narrowing(&mut self) {
        let checks = std::mem::take(&mut self.deferred_numeric_narrowing_checks);
        for site in checks {
            let expected = resolve_ty(&site.expected, &self.uf);
            let actual = resolve_ty(&site.actual, &self.uf);
            if !crate::check::solving::is_numeric_scalar(&expected)
                || !crate::check::solving::is_numeric_scalar(&actual)
                || !crate::check::builtin_calls::types_mismatch(&expected, &actual)
            {
                continue;
            }
            let mut diag = err(
                format!(
                    "type mismatch in {}: expected {} but got {}",
                    site.context, expected.display(), actual.display()
                ),
                Self::hint_with_conversion("Fix the expression type or change the annotation", &expected, &actual),
                site.context.clone(),
            ).with_code("E001");
            if let Some(s) = site.span {
                diag.file = self.source_file.clone();
                diag.line = Some(s.line);
                diag.col = Some(s.col);
            }
            self.diagnostics.push(diag);
        }
    }

    fn validate_empty_collection_elements(&mut self) {
        let checks = std::mem::take(&mut self.deferred_empty_collection_checks);
        for site in checks {
            let resolved = resolve_ty(&site.ty, &self.uf);
            if !Self::has_unconstrained_element(&resolved) { continue; }
            let EmptyCollectionAdvice { what, fix, try_fix } = empty_collection_advice(site.kind);
            let hint = format!(
                "{}'s element type cannot be inferred here. An empty collection \
                 carries no element to infer from — {}. (Almide follows Rust/Swift: \
                 an undecidable empty collection is an error even if its elements are \
                 never read; it is never silently defaulted.)",
                what, fix,
            );
            let mut diag = err(
                format!("cannot infer the element type of {}", what),
                hint,
                format!("{} with no element-type context", what),
            ).with_code("E018").with_try(try_fix);
            if let Some(s) = site.span {
                diag.file = self.source_file.clone();
                diag.line = Some(s.line);
                diag.col = Some(s.col);
                if s.end_col > s.col { diag.end_col = Some(s.end_col); }
            }
            self.diagnostics.push(diag);
        }
    }

    /// Post-solve: reject an un-annotated binding / discarded expression whose
    /// type still carries an unbound `?`-prefixed inference var anywhere in its
    /// tree. Such a var had NO inference source for the whole program (a phantom
    /// slot only reachable through an un-exercised branch — e.g. the error type
    /// of `result.or_else(r0, (e) => ok(0))`). Firing here, in the frontend,
    /// makes the error identical on BOTH targets and gives a `type annotation
    /// needed` diagnostic (cf. Rust E0282 / the sibling empty-collection E018)
    /// instead of the cryptic ConcretizeTypes COMPILER-BUG gate ICE (#662). A
    /// fully-decidable program leaves every binding type concrete, so this only
    /// fires on the genuinely-undecidable case; sites are deduped so a binding
    /// reused N times yields one error.
    fn validate_unresolved_binding_types(&mut self) {
        let checks = std::mem::take(&mut self.deferred_unresolved_binding_checks);
        let mut reported: std::collections::HashSet<(Option<u32>, Option<u32>)> = std::collections::HashSet::new();
        for site in checks {
            let resolved = resolve_ty(&site.ty, &self.uf);
            // A WHOLLY-Unknown binding type is error-recovery (a prior error was
            // already reported) or a structure the checker leaves Unknown but
            // codegen resolves (e.g. a cross-module variant `match` whose arms are
            // concrete). Neither is a genuinely-undecidable SLOT, so skip it — only
            // fire when a CONCRETE outer type carries an undecidable inner slot
            // (`Result[Int, ?]`, never pinned). Without this guard E025 false-fired
            // on valid cross-module variant matches.
            if matches!(resolved, Ty::Unknown) { continue; }
            // An unbound `?`-prefixed inference var (`fresh_var` the solver never
            // bound) anywhere in the tree is undecidable. A BARE `TypeVar` (`T`,
            // no `?`) is a rigid generic param — concrete in its scope — so it
            // must NOT trigger this (mirrors `has_unconstrained_element`).
            let undecidable = resolved.any_child_recursive(&|t: &Ty| match t {
                Ty::Unknown => true,
                Ty::TypeVar(n) => n.as_str().starts_with('?'),
                _ => false,
            });
            if !undecidable { continue; }
            let key = (site.span.map(|s| s.line as u32), site.span.map(|s| s.col as u32));
            if !reported.insert(key) { continue; }
            self.emit_unresolved_binding_diagnostic(&site, &resolved);
        }
    }

    /// Emit the E025 for a binding whose type keeps an undecidable slot.
    ///
    /// The wording splits on whether the site has a NAME: a named binding can be
    /// annotated in place and gets a `try` line naming it, while a bare
    /// expression has to be bound first before there is anywhere to put the
    /// annotation. The example annotation is derived from the REPORTED shape
    /// (`List[Unknown]` → `List[Int]`), never a fixed exemplar — a hint whose
    /// example cannot be correct for the binding teaches a false move (#1054).
    fn emit_unresolved_binding_diagnostic(&mut self, site: &UnresolvedBindingSite, resolved: &Ty) {
        let what = match &site.name {
            Some(n) => format!("binding '{}'", n),
            None => "this expression".to_string(),
        };
            let example = fill_example_ty(resolved).display();
            let fix = match &site.name {
                Some(n) => format!(
                    "Annotate the binding with the full type, e.g. `let {}: {} = ...`. \
                     An unconstrained slot (such as the error type of a value that is always `ok(...)`, \
                     reachable only through an un-exercised branch) cannot be inferred and is never \
                     silently defaulted (Almide follows Rust/Swift; cf. Rust E0282).",
                    n, example,
                ),
                None => format!(
                    "Bind the expression to an explicitly-typed `let`, e.g. \
                     `let r: {} = ...`, so the unconstrained slot is pinned. \
                     An unconstrained type slot cannot be inferred and is never silently defaulted \
                     (Almide follows Rust/Swift; cf. Rust E0282).", example),
            };
            let mut diag = err(
                format!("cannot infer a concrete type for {} (type {})", what, resolved.display()),
                fix,
                format!("{} with an unconstrained type", what),
            ).with_code("E025");
            if let Some(n) = &site.name {
                diag = diag.with_try(format!("let {}: {} = ...", n, example));
            }
            if let Some(s) = site.span {
                diag.file = self.source_file.clone();
                diag.line = Some(s.line);
                diag.col = Some(s.col);
                if s.end_col > s.col { diag.end_col = Some(s.end_col); }
            }
            self.diagnostics.push(diag);
    }

    /// True when `ty` (already resolved against the union-find) is a collection
    /// whose element/key/value slot is still an unresolved INFERENCE var — i.e.
    /// the element type was never pinned by context. `List[?A]`, `Set[?A]`,
    /// `Map[?K, _]`, `Map[_, ?V]` all qualify; a fully-concrete collection does
    /// not. We look one constructor deep (the producer's own container); a
    /// concrete element that itself nests an unresolved deeper payload is some
    /// OTHER expression's empty collection and is reported at its own site.
    ///
    /// A `?`-prefixed `TypeVar` is a fresh inference var (`fresh_var`) that the
    /// solver left unbound — undecidable. A BARE `TypeVar` (`T`, no `?`) is a
    /// rigid GENERIC PARAMETER, a perfectly good concrete element type in its
    /// scope: `fn make[T]() -> List[T] = []` is fine (Rust accepts
    /// `Vec::<T>::new()`), so it must NOT trigger the error.
    fn has_unconstrained_element(ty: &Ty) -> bool {
        use crate::types::TypeConstructorId as TCI;
        // Only an `Unknown` or a fresh inference var (`?`-prefixed) is
        // undecidable; a rigid generic param (`T`) is concrete.
        let is_unresolved = |t: &Ty| match t {
            Ty::Unknown => true,
            Ty::TypeVar(n) => n.as_str().starts_with('?'),
            _ => false,
        };
        match ty {
            Ty::Applied(TCI::List, args) | Ty::Applied(TCI::Set, args) if args.len() == 1 =>
                is_unresolved(&args[0]),
            Ty::Applied(TCI::Map, args) if args.len() == 2 =>
                is_unresolved(&args[0]) || is_unresolved(&args[1]),
            _ => false,
        }
    }

    /// Human-readable type name for diagnostics.
    fn type_display_name(ty: &Ty) -> String {
        match ty {
            Ty::Int => "Int".into(),
            Ty::Float => "Float".into(),
            Ty::String => "String".into(),
            Ty::Bool => "Bool".into(),
            Ty::Unit => "Unit".into(),
            Ty::Bytes => "Bytes".into(),
            Ty::Named(name, _) => name.as_str().to_string(),
            Ty::Fn { .. } => "Fn".into(),
            Ty::Applied(crate::types::TypeConstructorId::Map, _) => "Map".into(),
            Ty::Applied(crate::types::TypeConstructorId::List, args) => {
                if let Some(inner) = args.first() {
                    format!("List[{}]", Self::type_display_name(inner))
                } else {
                    "List".into()
                }
            }
            _ => format!("{:?}", ty),
        }
    }
}
