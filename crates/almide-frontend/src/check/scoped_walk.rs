// The statement / expression walk of the `scoped` checker (E086 / E087 per
// construct) — spliced into check/mod.rs between scoped.rs and
// scoped_shape.rs.

impl<'a, 'c> ScopedCx<'a, 'c> {
    fn walk_stmts(&mut self, stmts: &[ast::Stmt]) {
        for st in stmts {
            match st {
                ast::Stmt::Let { name, value, .. } | ast::Stmt::Var { name, value, .. } => {
                    self.walk_expr(value);
                    self.bind(name.as_str());
                    self.let_sites.insert(name.to_string(), value.span);
                }
                ast::Stmt::LetDestructure { pattern, value, .. } => {
                    self.walk_expr(value);
                    let mut names = Vec::new();
                    pattern_binders(pattern, &mut names);
                    for n in names {
                        self.bind(&n);
                    }
                }
                ast::Stmt::Assign { name, value, span } => {
                    self.walk_expr(value);
                    let n = name.as_str();
                    if self.is_global(n) && !self.is_local(n) {
                        self.refuse(&format!("writing the global `{n}`"), "a global lives outside the allocation region", "return the value from the scope and assign it outside", *span);
                    } else if self.block_floor != usize::MAX && !self.is_block_local(n) {
                        // E086: the write leaves the block.
                        let scalar = self.ty_of(value).is_some_and(is_scalar_ty);
                        let (msg, hint) = if scalar {
                            (format!("`{n}` is written from inside the scope"), format!("make the write the block's value: `{n} = scoped {{ ... }}`"))
                        } else {
                            (format!("`{n}` would outlive its scoped allocation"), "compute and return the scalar summary inside the scope".to_string())
                        };
                        let d = scoped_err(msg, &hint, "E086", &self.ctx(), self.file, *span)
                            .with_note(format!("assigned at {}", at(self.file, *span)))
                            .with_note(format!("scope ends at {}", at(self.file, self.block_end)));
                        self.diags.push(d);
                    }
                }
                ast::Stmt::IndexAssign { index, value, span, .. } => {
                    self.refuse("an indexed write", "it mutates a collection, which is outside the scoped fragment", "rebuild the value instead of writing into it", *span);
                    self.walk_expr(index);
                    self.walk_expr(value);
                }
                ast::Stmt::FieldAssign { value, span, .. } => {
                    self.refuse("a field write", "it mutates storage in place, which is outside the scoped fragment", "rebuild the record with the new field", *span);
                    self.walk_expr(value);
                }
                ast::Stmt::Guard { cond, else_, span } => {
                    self.refuse("`guard`", "an early exit leaves the allocation region before its end", "spell the exit as an `if` / `match` whose value is the result", *span);
                    self.walk_expr(cond);
                    self.walk_expr(else_);
                }
                ast::Stmt::GuardLet { name, scrutinee, else_, span } => {
                    self.refuse("`guard let`", "an early exit leaves the allocation region before its end", "spell the exit as a `match` whose value is the result", *span);
                    self.walk_expr(scrutinee);
                    self.walk_expr(else_);
                    self.bind(name.as_str());
                }
                ast::Stmt::Expr { expr, .. } => self.walk_expr(expr),
                ast::Stmt::Comment { .. } | ast::Stmt::Error { .. } => {}
            }
        }
    }

    fn walk_expr(&mut self, e: &ast::Expr) {
        use ast::ExprKind as K;
        match &e.kind {
            K::Ident { name } => self.check_ident(name.as_str(), e),
            K::TypeName { name } => {
                if self.is_global(name.as_str()) {
                    self.refuse(&format!("`{name}`"), "a global lives outside the allocation region", "pass the value in as a parameter", e.span);
                }
            }
            K::Call { callee, args, .. } => {
                if self.check_call(e, callee, args) {
                    self.check_type(e);
                }
                for a in args {
                    self.walk_expr(a);
                }
            }
            K::Pipe { left, right } => self.walk_pipe(e, left, right),
            K::Compose { .. } | K::Lambda { .. } => {
                self.refuse("a function value", "the callee is unknown, so what it retains is unknown", "call a `scoped fn` by name", e.span);
            }
            K::Scoped { .. } => self.walk_scoped_block(e),
            K::Block { stmts, expr } => {
                self.check_type(e);
                self.push_scope();
                self.walk_stmts(stmts);
                if let Some(t) = expr {
                    self.walk_expr(t);
                }
                self.pop_scope();
            }
            K::Match { subject, arms } => {
                self.check_type(e);
                self.walk_expr(subject);
                for arm in arms {
                    self.push_scope();
                    let mut names = Vec::new();
                    pattern_binders(&arm.pattern, &mut names);
                    for n in names {
                        self.bind(&n);
                    }
                    if let Some(g) = &arm.guard {
                        self.walk_expr(g);
                    }
                    self.walk_expr(&arm.body);
                    self.pop_scope();
                }
            }
            K::IfLet { name, scrutinee, then, else_ } => {
                self.check_type(e);
                self.walk_expr(scrutinee);
                self.push_scope();
                self.bind(name.as_str());
                self.walk_expr(then);
                self.pop_scope();
                self.walk_expr(else_);
            }
            K::ForIn { var, var_tuple, iterable, body } => {
                if !matches!(iterable.kind, K::Range { .. }) {
                    self.refuse("`for` over a collection", "only a counted range `a..<b` is admitted", "iterate a range and index by position, or recurse over the variant", e.span);
                } else if let K::Range { start, end, .. } = &iterable.kind {
                    self.walk_expr(start);
                    self.walk_expr(end);
                }
                self.push_scope();
                self.bind(var.as_str());
                for n in var_tuple.iter().flatten() {
                    self.bind(n.as_str());
                }
                self.loop_depth += 1;
                self.walk_stmts(body);
                self.loop_depth -= 1;
                self.pop_scope();
            }
            K::While { cond, body } => {
                self.walk_expr(cond);
                self.push_scope();
                self.loop_depth += 1;
                self.walk_stmts(body);
                self.loop_depth -= 1;
                self.pop_scope();
            }
            K::Unwrap { expr } => {
                self.refuse("`!`", "a propagated failure leaves the allocation region early", "handle the case inside the scope with `match` or `??`", e.span);
                self.walk_expr(expr);
            }
            K::Try { expr } | K::ToOption { expr } => {
                self.refuse("`?`", "a Result is outside the scoped fragment", "handle the case inside the scope with `match`", e.span);
                self.walk_expr(expr);
            }
            K::Ok { expr } | K::Err { expr } => {
                self.refuse("a Result value", "a Result is outside the scoped fragment", "return a scalar or an admitted variant, and handle failure outside the scope", e.span);
                self.walk_expr(expr);
            }
            K::String { .. } | K::InterpolatedString { .. } => {
                self.refuse("a `String`", "a String is a heap view outside the scoped fragment", "return a scalar summary and format it outside the scope", e.span);
            }
            K::List { .. } | K::MapLiteral { .. } | K::EmptyMap => {
                self.refuse("a collection literal", "collections are outside the scoped fragment", "model the data as a variant of scalars", e.span);
            }
            K::SpreadRecord { .. } => {
                self.refuse("a record spread", "it copies from storage the region does not own", "build the record from its fields", e.span);
            }
            K::IndexAccess { .. } => {
                self.refuse("an index access", "collections are outside the scoped fragment", "model the data as a variant of scalars", e.span);
            }
            K::Fan { .. } | K::FanBounded { .. } | K::FanRace { .. } | K::FanRaceMap { .. } | K::FanTimeout { .. } | K::FanSettle { .. } => {
                self.refuse("`fan`", "scheduling runs outside the allocation region", "run the `fan` outside and pass scalars into the scope", e.span);
            }
            K::Todo { .. } => {
                self.refuse("`todo`", "an abort with a message allocates outside the fragment", "return a scalar sentinel", e.span);
            }
            K::Range { .. } => {
                self.refuse("a range value", "a range is admitted only as a `for` bound", "write the range in the `for` head", e.span);
            }
            K::Record { fields, .. } => {
                self.check_type(e);
                for f in fields {
                    self.walk_expr(&f.value);
                }
            }
            K::Tuple { elements } => {
                self.check_type(e);
                for x in elements {
                    self.walk_expr(x);
                }
            }
            K::Some { expr } => {
                self.check_type(e);
                self.walk_expr(expr);
            }
            K::Member { object, .. } | K::TupleIndex { object, .. } | K::OptionalChain { expr: object, .. } => {
                self.check_type(e);
                self.walk_expr(object);
            }
            K::Binary { left, right, .. } => {
                self.check_type(e);
                self.walk_expr(left);
                self.walk_expr(right);
            }
            K::Unary { operand, .. } => {
                self.check_type(e);
                self.walk_expr(operand);
            }
            K::If { cond, then, else_ } => {
                self.check_type(e);
                self.walk_expr(cond);
                self.walk_expr(then);
                self.walk_expr(else_);
            }
            K::UnwrapOr { expr, fallback } => {
                self.check_type(e);
                self.walk_expr(expr);
                self.walk_expr(fallback);
            }
            K::Paren { expr } | K::TypeAscription { expr, .. } => self.walk_expr(expr),
            K::Int { .. } | K::Float { .. } | K::Bool { .. } | K::Unit | K::None | K::Break | K::Continue
            | K::Hole | K::Placeholder | K::Error => {}
        }
    }
}
