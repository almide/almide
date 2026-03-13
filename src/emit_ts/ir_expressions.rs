use crate::ir::*;
use super::TsEmitter;

impl TsEmitter {
    pub(crate) fn gen_ir_expr(&self, expr: &IrExpr) -> String {
        match &expr.kind {
            IrExprKind::LitInt { value } => {
                if *value > 9007199254740991 || *value < -9007199254740991 {
                    format!("{}n", value)
                } else {
                    format!("{}", value)
                }
            }
            IrExprKind::LitFloat { value } => format!("{}", value),
            IrExprKind::LitStr { value } => Self::json_string(value),
            IrExprKind::LitBool { value } => format!("{}", value),
            IrExprKind::Unit => "undefined".to_string(),

            IrExprKind::Var { id } => {
                let info = self.ir_var_table().get(*id);
                Self::sanitize(&info.name)
            }

            IrExprKind::BinOp { op, left, right } => self.gen_ir_binary(*op, left, right),
            IrExprKind::UnOp { op, operand } => {
                let o = self.gen_ir_expr(operand);
                match op {
                    UnOp::Not => format!("!({})", o),
                    UnOp::NegInt | UnOp::NegFloat => format!("-({})", o),
                }
            }

            IrExprKind::If { cond, then, else_ } => {
                let t = self.gen_ir_expr_value(then);
                let e = self.gen_ir_expr_value(else_);
                format!("({} ? {} : {})", self.gen_ir_expr(cond), t, e)
            }

            IrExprKind::Match { subject, arms } => self.gen_ir_match(subject, arms),
            IrExprKind::Block { stmts, expr } => self.gen_ir_block(stmts, expr.as_deref(), 0),
            IrExprKind::DoBlock { stmts, expr } => self.gen_ir_do_block(stmts, expr.as_deref(), 0),

            IrExprKind::ForIn { var, var_tuple, iterable, body } => {
                let stmts_str: Vec<String> = body.iter()
                    .map(|s| format!("  {}", self.gen_ir_stmt(s)))
                    .collect();
                let binding = if let Some(tuple_vars) = var_tuple {
                    let names: Vec<String> = tuple_vars.iter()
                        .map(|v| Self::sanitize(&self.ir_var_table().get(*v).name))
                        .collect();
                    format!("[{}]", names.join(", "))
                } else {
                    Self::sanitize(&self.ir_var_table().get(*var).name)
                };
                if let IrExprKind::Range { start, end, inclusive } = &iterable.kind {
                    let s = self.gen_ir_expr(start);
                    let e = self.gen_ir_expr(end);
                    let cmp = if *inclusive { "<=" } else { "<" };
                    format!("for (let {} = {}; {} {} {}; {}++) {{\n{}\n}}", binding, s, binding, cmp, e, binding, stmts_str.join("\n"))
                } else {
                    let iter_str = self.gen_ir_expr(iterable);
                    format!("for (const {} of {}) {{\n{}\n}}", binding, iter_str, stmts_str.join("\n"))
                }
            }

            IrExprKind::While { cond, body } => {
                let c = self.gen_ir_expr(cond);
                let stmts_str: Vec<String> = body.iter()
                    .map(|s| format!("  {}", self.gen_ir_stmt(s)))
                    .collect();
                format!("while ({}) {{\n{}\n}}", c, stmts_str.join("\n"))
            }
            IrExprKind::Break => "break".to_string(),
            IrExprKind::Continue => "continue".to_string(),

            IrExprKind::Call { target, args, .. } => self.gen_ir_call(target, args),

            IrExprKind::List { elements } => {
                let elems: Vec<String> = elements.iter().map(|e| self.gen_ir_expr(e)).collect();
                format!("[{}]", elems.join(", "))
            }

            IrExprKind::EmptyMap => "__almd_map.new()".to_string(),

            IrExprKind::MapLiteral { entries } => {
                let pairs: Vec<String> = entries.iter().map(|(k, v)| {
                    format!("[{}, {}]", self.gen_ir_expr(k), self.gen_ir_expr(v))
                }).collect();
                format!("new Map([{}])", pairs.join(", "))
            }

            IrExprKind::Record { name, fields } => {
                let fs: Vec<String> = fields.iter()
                    .map(|(fname, fval)| format!("{}: {}", fname, self.gen_ir_expr(fval)))
                    .collect();
                if let Some(cname) = name.as_ref() {
                    if self.variant_constructors.contains(cname.as_str()) {
                        let mut all = vec![format!("tag: {}", Self::json_string(cname))];
                        all.extend(fs);
                        return format!("{{ {} }}", all.join(", "));
                    }
                }
                format!("{{ {} }}", fs.join(", "))
            }

            IrExprKind::SpreadRecord { base, fields } => {
                let fs: Vec<String> = fields.iter()
                    .map(|(fname, fval)| format!("{}: {}", fname, self.gen_ir_expr(fval)))
                    .collect();
                format!("{{ ...{}, {} }}", self.gen_ir_expr(base), fs.join(", "))
            }

            IrExprKind::Tuple { elements } => {
                let parts: Vec<String> = elements.iter().map(|e| self.gen_ir_expr(e)).collect();
                format!("[{}]", parts.join(", "))
            }

            IrExprKind::Range { start, end, inclusive } => {
                let s = self.gen_ir_expr(start);
                let e = self.gen_ir_expr(end);
                if *inclusive {
                    format!("Array.from({{length: ({e}) - ({s}) + 1}}, (_, i) => ({s}) + i)")
                } else {
                    format!("Array.from({{length: ({e}) - ({s})}}, (_, i) => ({s}) + i)")
                }
            }

            IrExprKind::Member { object, field } => {
                if let IrExprKind::Var { id } = &object.kind {
                    let name = &self.ir_var_table().get(*id).name;
                    let mapped = self.map_module(name);
                    format!("{}.{}", mapped, Self::sanitize(field))
                } else {
                    format!("{}.{}", self.gen_ir_expr(object), Self::sanitize(field))
                }
            }

            IrExprKind::TupleIndex { object, index } => {
                format!("({})[{}]", self.gen_ir_expr(object), index)
            }

            IrExprKind::IndexAccess { object, index } => {
                if matches!(object.ty, crate::types::Ty::Map(_, _)) {
                    format!("__almd_map.get({}, {})", self.gen_ir_expr(object), self.gen_ir_expr(index))
                } else {
                    format!("{}[{}]", self.gen_ir_expr(object), self.gen_ir_expr(index))
                }
            }

            IrExprKind::Lambda { params, body } => {
                let ps: Vec<String> = params.iter().map(|(var, _)| {
                    self.ir_var_table().get(*var).name.clone()
                }).collect();
                format!("(({}) => {})", ps.join(", "), self.gen_ir_expr(body))
            }

            IrExprKind::StringInterp { parts } => {
                let mut result = String::from("`");
                for part in parts {
                    match part {
                        IrStringPart::Lit { value } => result.push_str(value),
                        IrStringPart::Expr { expr } => {
                            result.push_str(&format!("${{{}}}", self.gen_ir_expr(expr)));
                        }
                    }
                }
                result.push('`');
                result
            }

            IrExprKind::ResultOk { expr } => self.gen_ir_expr(expr),
            IrExprKind::ResultErr { expr } => self.gen_ir_err(expr),
            IrExprKind::OptionSome { expr } => self.gen_ir_expr(expr),
            IrExprKind::OptionNone => "null".to_string(),
            IrExprKind::Try { expr } => self.gen_ir_expr(expr),
            IrExprKind::Await { expr } => format!("await {}", self.gen_ir_expr(expr)),

            IrExprKind::Hole => {
                if self.js_mode { "null /* hole */".to_string() } else { "null as any /* hole */".to_string() }
            }
            IrExprKind::Todo { message } => format!("__throw({})", Self::json_string(message)),
        }
    }

    fn gen_ir_err(&self, expr: &IrExpr) -> String {
        // Check if this is a structured error (variant constructor)
        let is_variant = match &expr.kind {
            IrExprKind::Call { target: CallTarget::Named { name }, .. } =>
                name.chars().next().map_or(false, |c| c.is_uppercase()),
            IrExprKind::Var { id } => {
                let name = &self.ir_var_table().get(*id).name;
                name.chars().next().map_or(false, |c| c.is_uppercase())
            }
            _ => false,
        };

        if is_variant {
            // Structured error: preserve variant object
            let val = self.gen_ir_expr(expr);
            let msg = self.gen_ir_err_msg_string(expr);
            if self.in_effect.get() {
                // Throw an Error with message, but attach value for catch handlers
                format!("(() => {{ const __e = new Error({}); __e.__almd_value = {}; throw __e; }})()", msg, val)
            } else {
                format!("new __Err({}, {})", msg, val)
            }
        } else {
            // Simple string error
            let msg = self.gen_ir_err_msg_string(expr);
            if self.in_effect.get() {
                format!("__throw({})", msg)
            } else {
                format!("new __Err({})", msg)
            }
        }
    }

    pub(crate) fn gen_ir_err_msg(&self, expr: &IrExpr) -> String {
        self.gen_ir_err_msg_string(expr)
    }

    fn gen_ir_err_msg_string(&self, expr: &IrExpr) -> String {
        match &expr.kind {
            IrExprKind::LitStr { value } => Self::json_string(value),
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                let callee_str = if name.chars().next().map_or(false, |c| c.is_uppercase()) {
                    Self::pascal_to_message(name)
                } else {
                    name.clone()
                };
                let arg = if !args.is_empty() { self.gen_ir_expr(&args[0]) } else { "\"\"".to_string() };
                format!("{} + \": \" + {}", Self::json_string(&callee_str), arg)
            }
            _ => format!("String({})", self.gen_ir_expr(expr)),
        }
    }

    /// Generate IR expression as a value, unwrapping trivial blocks
    pub(crate) fn gen_ir_expr_value(&self, expr: &IrExpr) -> String {
        match &expr.kind {
            IrExprKind::Block { stmts, expr: Some(final_expr) } if stmts.is_empty() => {
                self.gen_ir_expr_value(final_expr)
            }
            IrExprKind::Block { .. } | IrExprKind::DoBlock { .. } => {
                format!("(() => {})()", self.gen_ir_expr(expr))
            }
            _ => self.gen_ir_expr(expr),
        }
    }

    fn gen_ir_binary(&self, op: BinOp, left: &IrExpr, right: &IrExpr) -> String {
        let l = self.gen_ir_expr(left);
        let r = self.gen_ir_expr(right);
        match op {
            BinOp::And => format!("({} && {})", l, r),
            BinOp::Or => format!("({} || {})", l, r),
            BinOp::Eq => format!("__deep_eq({}, {})", l, r),
            BinOp::Neq => format!("!__deep_eq({}, {})", l, r),
            BinOp::ConcatStr | BinOp::ConcatList => format!("__concat({}, {})", l, r),
            BinOp::PowFloat => format!("Math.pow({}, {})", l, r),
            BinOp::XorInt => format!("__bigop(\"^\", {}, {})", l, r),
            BinOp::MulInt => format!("__bigop(\"*\", {}, {})", l, r),
            BinOp::ModInt => format!("__bigop(\"%\", {}, {})", l, r),
            BinOp::DivInt => format!("__div({}, {})", l, r),
            BinOp::AddInt | BinOp::AddFloat => format!("({} + {})", l, r),
            BinOp::SubInt | BinOp::SubFloat => format!("({} - {})", l, r),
            BinOp::MulFloat => format!("({} * {})", l, r),
            BinOp::DivFloat => format!("({} / {})", l, r),
            BinOp::ModFloat => format!("({} % {})", l, r),
            BinOp::Lt => format!("({} < {})", l, r),
            BinOp::Gt => format!("({} > {})", l, r),
            BinOp::Lte => format!("({} <= {})", l, r),
            BinOp::Gte => format!("({} >= {})", l, r),
        }
    }

    pub(crate) fn gen_ir_call(&self, target: &CallTarget, args: &[IrExpr]) -> String {
        match target {
            CallTarget::Named { name } => {
                // For generic unit constructors used as callees, use raw name
                let callee_str = if self.generic_variant_unit_ctors.contains(name) {
                    name.clone()
                } else {
                    Self::sanitize(name)
                };
                // assert_eq with err(): wrap in try-catch
                if callee_str == "assert_eq" && args.len() == 2 {
                    if matches!(&args[1].kind, IrExprKind::ResultErr { .. }) {
                        let other = self.gen_ir_expr(&args[0]);
                        let err_val = self.gen_ir_expr(&args[1]);
                        return format!("(() => {{ let __v; try {{ __v = {}; }} catch (__e) {{ __v = new __Err(__e instanceof Error ? __e.message : String(__e), __e.__almd_value); }} assert_eq(__v, {}); }})()", other, err_val);
                    }
                    if matches!(&args[0].kind, IrExprKind::ResultErr { .. }) {
                        let other = self.gen_ir_expr(&args[1]);
                        let err_val = self.gen_ir_expr(&args[0]);
                        return format!("(() => {{ let __v; try {{ __v = {}; }} catch (__e) {{ __v = new __Err(__e instanceof Error ? __e.message : String(__e), __e.__almd_value); }} assert_eq({}, __v); }})()", other, err_val);
                    }
                }
                // Unit variants (no payload) are const values, not functions — emit bare identifier
                if args.is_empty() && self.unit_variant_names.contains(name) {
                    return callee_str;
                }
                let args_str: Vec<String> = args.iter().map(|a| self.gen_ir_expr(a)).collect();
                format!("{}({})", callee_str, args_str.join(", "))
            }

            CallTarget::Module { module, func } => {
                let module_str = self.map_module(module);
                let args_str: Vec<String> = args.iter().map(|a| self.gen_ir_expr(a)).collect();
                format!("{}.{}({})", module_str, Self::sanitize(func), args_str.join(", "))
            }

            CallTarget::Method { object, method } => {
                // Fallback method call — UFCS should already be resolved to Module
                let obj_str = self.gen_ir_expr(object);
                // Built-in method calls
                if method == "unwrap_or" && args.len() == 1 {
                    let default = self.gen_ir_expr(&args[0]);
                    return format!("unwrap_or({}, {})", obj_str, default);
                }
                let args_str: Vec<String> = args.iter().map(|a| self.gen_ir_expr(a)).collect();
                let mut all_args = vec![obj_str];
                all_args.extend(args_str);
                format!("{}({})", Self::sanitize(method), all_args.join(", "))
            }

            CallTarget::Computed { callee } => {
                let callee_str = self.gen_ir_expr(callee);
                let args_str: Vec<String> = args.iter().map(|a| self.gen_ir_expr(a)).collect();
                format!("({})({})", callee_str, args_str.join(", "))
            }
        }
    }

    /// Access the IR var table
    pub(crate) fn ir_var_table(&self) -> &crate::ir::VarTable {
        &self.ir_program.as_ref().expect("IR must be available for IR codegen").var_table
    }
}
