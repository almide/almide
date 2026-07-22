//! Dialect Module → Rust source code emitter.
//!
//! This is the Stage 2 re-wiring: instead of the existing walker reading
//! IrProgram directly, this emitter reads the dialect Module (SSA form)
//! and produces equivalent Rust code. The dialect is the single source
//! of truth for all backends.
//!
//! Strategy: each Operation maps to a Rust `let` binding. ValueIds become
//! local variable names (`v0`, `v1`, ...). Block structure maps to Rust
//! scoping with `{ }`.

use crate::{Module, Block, ValueId};
use crate::ops::*;
use crate::types::DialectType;

/// Maps ValueId to a Rust variable name. Params use their declared names,
/// SSA temps use `v{id}`.
struct NameMap {
    names: std::collections::HashMap<ValueId, String>,
    /// variant case name → parent enum name (for qualified references)
    variants: std::collections::HashMap<String, String>,
    /// How many times each ValueId is referenced. Values used >1 times get .clone().
    use_counts: std::collections::HashMap<ValueId, usize>,
    /// Track remaining uses to know when to clone vs move.
    remaining: std::collections::HashMap<ValueId, usize>,
}

impl NameMap {
    fn new(variants: std::collections::HashMap<String, String>, use_counts: std::collections::HashMap<ValueId, usize>) -> Self {
        let remaining = use_counts.clone();
        NameMap { names: std::collections::HashMap::new(), variants, use_counts, remaining }
    }

    fn set(&mut self, id: ValueId, name: String) { self.names.insert(id, name); }

    /// Get the variable name. If the value is used more than once and this isn't
    /// the last use, append `.clone()`. Scalars (i64, f64, bool) are Copy so
    /// they never need clone.
    fn get(&mut self, id: ValueId) -> String {
        let name = self.names.get(&id).cloned().unwrap_or_else(|| format!("v{}", id.0));
        let total = self.use_counts.get(&id).copied().unwrap_or(1);
        let rem = self.remaining.entry(id).or_insert(total);
        *rem = rem.saturating_sub(1);
        if total > 1 && *rem > 0 {
            format!("{}.clone()", name)
        } else {
            name
        }
    }

    /// Get without consuming a use (for pattern positions, etc.)
    fn get_ref(&self, id: ValueId) -> String {
        self.names.get(&id).cloned().unwrap_or_else(|| format!("v{}", id.0))
    }
}

/// Collect all variant case names → parent enum name for qualified references.
fn build_variant_map(module: &Module) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for td in &module.type_decls {
        if let TypeDeclKind::Variant { cases } = &td.kind {
            for case in cases {
                map.insert(case.name.as_str().to_string(), td.name.as_str().to_string());
            }
        }
    }
    map
}

/// Emit a complete Rust source file from a dialect Module.
pub fn emit_module(module: &Module) -> String {
    let mut out = String::new();
    let variant_map = build_variant_map(module);
    let use_counts = crate::compute_use_counts(module);

    // No prelude — runtime preamble is added by embed_rust_runtime() in codegen.
    // When used standalone (--emit-dialect --target rust), the output won't compile
    // without manually adding runtime. Use `--target dialect` for full output.

    // Type declarations
    for td in &module.type_decls {
        emit_type_decl(&mut out, td);
    }

    // Globals
    for g in &module.globals {
        emit_global(&mut out, g);
    }

    // Functions
    for f in &module.functions {
        if f.name.as_str().contains('.') { continue; } // skip convention methods for now
        emit_func(&mut out, f, &variant_map, &use_counts);
    }

    out
}

fn _val(v: ValueId) -> String { format!("v{}", v.0) }

fn emit_rust_type(ty: &DialectType) -> String {
    match ty {
        DialectType::I64 => "i64".into(),
        DialectType::F64 => "f64".into(),
        DialectType::Bool => "bool".into(),
        DialectType::Unit => "()".into(),
        DialectType::String => "String".into(),
        DialectType::Bytes => "Vec<u8>".into(),
        DialectType::I8 => "i8".into(),
        DialectType::I16 => "i16".into(),
        DialectType::I32 => "i32".into(),
        DialectType::U8 => "u8".into(),
        DialectType::U16 => "u16".into(),
        DialectType::U32 => "u32".into(),
        DialectType::U64 => "u64".into(),
        DialectType::F32 => "f32".into(),
        DialectType::Matrix => "Matrix".into(),
        DialectType::RawPtr => "*mut u8".into(),
        DialectType::Unknown => "()".into(),
        DialectType::List(inner) => format!("Vec<{}>", emit_rust_type(inner)),
        DialectType::Map(k, v) => format!("HashMap<{}, {}>", emit_rust_type(k), emit_rust_type(v)),
        DialectType::Option(inner) => format!("Option<{}>", emit_rust_type(inner)),
        DialectType::Result(ok, err) => format!("Result<{}, {}>", emit_rust_type(ok), emit_rust_type(err)),
        DialectType::Tuple(elems) => {
            let parts: Vec<_> = elems.iter().map(|e| emit_rust_type(e)).collect();
            format!("({})", parts.join(", "))
        }
        DialectType::Named(sym) => sym.as_str().to_string(),
        DialectType::Record(fields) => {
            // Anonymous records not supported as Rust types
            "()".into()
        }
        DialectType::Fn { params, ret } => {
            let ps: Vec<_> = params.iter().map(|p| emit_rust_type(p)).collect();
            format!("impl Fn({}) -> {}", ps.join(", "), emit_rust_type(ret))
        }
        DialectType::Closure { params, ret } => {
            let ps: Vec<_> = params.iter().map(|p| emit_rust_type(p)).collect();
            format!("Box<dyn Fn({}) -> {}>", ps.join(", "), emit_rust_type(ret))
        }
    }
}

fn emit_type_decl(out: &mut String, td: &TypeDeclOp) {
    match &td.kind {
        TypeDeclKind::Record { fields } => {
            out.push_str(&format!("#[derive(Clone, Debug, PartialEq)]\npub struct {} {{\n", td.name));
            for (name, ty) in fields {
                out.push_str(&format!("    pub {}: {},\n", name, emit_rust_type(ty)));
            }
            out.push_str("}\n\n");
        }
        TypeDeclKind::Variant { cases } => {
            out.push_str(&format!("#[derive(Clone, Debug, PartialEq)]\npub enum {} {{\n", td.name));
            for case in cases {
                if case.payload.is_empty() {
                    out.push_str(&format!("    {},\n", case.name));
                } else {
                    let types: Vec<_> = case.payload.iter().map(|t| emit_rust_type(t)).collect();
                    out.push_str(&format!("    {}({}),\n", case.name, types.join(", ")));
                }
            }
            out.push_str("}\n\n");
        }
        TypeDeclKind::Alias(ty) => {
            out.push_str(&format!("pub type {} = {};\n\n", td.name, emit_rust_type(ty)));
        }
    }
}

fn emit_global(out: &mut String, g: &GlobalOp) {
    // Simplified: static lazy
    out.push_str(&format!("// global: {}\n", g.name));
}

fn emit_func(out: &mut String, f: &FuncOp, variant_map: &std::collections::HashMap<String, String>, use_counts: &std::collections::HashMap<ValueId, usize>) {
    let mut names = NameMap::new(variant_map.clone(), use_counts.clone());

    let params: Vec<_> = f.params.iter()
        .map(|(name, ty)| format!("{}: {}", name, emit_rust_type(ty)))
        .collect();
    let ret = if matches!(f.ret_ty, DialectType::Unit) {
        String::new()
    } else {
        format!(" -> {}", emit_rust_type(&f.ret_ty))
    };

    // Register parameter ValueIds with their declared names
    for block in &f.body {
        for (val_id, _) in &block.args {
            // Find matching param by position
            let idx = block.args.iter().position(|(v, _)| v == val_id).unwrap_or(0);
            if idx < f.params.len() {
                names.set(*val_id, f.params[idx].0.as_str().to_string());
            }
        }
    }

    out.push_str(&format!("fn {}({}){} {{\n", f.name, params.join(", "), ret));

    for block in &f.body {
        emit_block(out, block, 1, &mut names);
    }

    out.push_str("}\n\n");
}

fn emit_block(out: &mut String, block: &Block, indent: usize, names: &mut NameMap) {
    let pad = "    ".repeat(indent);

    for op in &block.ops {
        emit_op(out, op, indent, names);
    }

    match &block.terminator {
        Terminator::Return(v) => {
            if matches!(block.ops.last().map(|o| &o.result_ty), Some(DialectType::Unit)) {
            } else {
                out.push_str(&format!("{}{}\n", pad, names.get(*v)));
            }
        }
        Terminator::Yield(v) => {
            out.push_str(&format!("{}{}\n", pad, names.get(*v)));
        }
        _ => {}
    }
}

fn emit_op(out: &mut String, op: &Operation, indent: usize, names: &mut NameMap) {
    let pad = "    ".repeat(indent);
    let result_name = op.result.map(|r| names.get(r)).unwrap_or_default();

    match &op.kind {
        OpKind::ConstInt(v) => {
            out.push_str(&format!("{}let {} = {}i64;\n", pad, result_name, v));
        }
        OpKind::ConstFloat(v) => {
            out.push_str(&format!("{}let {} = {:.6}f64;\n", pad, result_name, v));
        }
        OpKind::ConstBool(v) => {
            out.push_str(&format!("{}let {} = {};\n", pad, result_name, v));
        }
        OpKind::ConstString(v) => {
            out.push_str(&format!("{}let {} = \"{}\".to_string();\n", pad, result_name, v.escape_default()));
        }
        OpKind::ConstUnit => {
            out.push_str(&format!("{}let {} = ();\n", pad, result_name));
        }

        OpKind::BinOp { .. } => emit_op_binop(out, op, &pad, &result_name, names),
        OpKind::UnOp { op, operand } => {
            let op_str = match op {
                almide_ir::UnOp::NegInt | almide_ir::UnOp::NegFloat => "-",
                almide_ir::UnOp::Not => "!",
            };
            out.push_str(&format!("{}let {} = {}{};\n", pad, result_name, op_str, names.get(*operand)));
        }

        OpKind::CallOp { .. } => emit_op_call(out, op, &pad, &result_name, names),
        OpKind::ComputedCallOp { callee, args } => {
            let callee_name = names.get(*callee);
            let args_str: Vec<_> = args.iter().map(|a| names.get(*a)).collect();
            out.push_str(&format!("{}let {} = {}({});\n", pad, result_name, callee_name, args_str.join(", ")));
        }
        OpKind::IntrinsicCallOp { symbol, args } => {
            let args_str: Vec<_> = args.iter().map(|a| names.get(*a)).collect();
            out.push_str(&format!("{}let {} = {}({});\n", pad, result_name, symbol, args_str.join(", ")));
        }

        OpKind::RecordOp { name, fields } => {
            let n = name.map(|s| s.as_str().to_string()).unwrap_or_default();
            let fs: Vec<_> = fields.iter().map(|(k, v)| format!("{}: {}", k, names.get(*v))).collect();
            out.push_str(&format!("{}let {} = {} {{ {} }};\n", pad, result_name, n, fs.join(", ")));
        }
        OpKind::MemberOp { object, field } => {
            out.push_str(&format!("{}let {} = {}.{}.clone();\n", pad, result_name, names.get(*object), field));
        }

        OpKind::ListOp { elements } => {
            let vals: Vec<_> = elements.iter().map(|v| names.get(*v)).collect();
            out.push_str(&format!("{}let {} = vec![{}];\n", pad, result_name, vals.join(", ")));
        }
        OpKind::TupleOp { elements } => {
            let vals: Vec<_> = elements.iter().map(|v| names.get(*v)).collect();
            out.push_str(&format!("{}let {} = ({});\n", pad, result_name, vals.join(", ")));
        }

        OpKind::IfOp { .. } => emit_op_if(out, op, &pad, &result_name, indent, names),

        OpKind::ResultOkOp { value } => {
            out.push_str(&format!("{}let {} = Ok({});\n", pad, result_name, names.get(*value)));
        }
        OpKind::ResultErrOp { value } => {
            out.push_str(&format!("{}let {} = Err({});\n", pad, result_name, names.get(*value)));
        }
        OpKind::OptionSomeOp { value } => {
            out.push_str(&format!("{}let {} = Some({});\n", pad, result_name, names.get(*value)));
        }
        OpKind::OptionNoneOp => {
            out.push_str(&format!("{}let {} = None;\n", pad, result_name));
        }

        OpKind::AllocVar { init, .. } => {
            out.push_str(&format!("{}let mut {} = {};\n", pad, result_name, names.get(*init)));
        }
        OpKind::LoadVar { slot } => {
            out.push_str(&format!("{}let {} = {}.clone();\n", pad, result_name, names.get(*slot)));
        }
        OpKind::StoreVar { slot, value } => {
            out.push_str(&format!("{}{} = {};\n", pad, names.get(*slot), names.get(*value)));
        }

        OpKind::MatchOp { .. } => emit_op_match(out, op, &pad, &result_name, indent, names),

        OpKind::LambdaOp { .. } => emit_op_lambda(out, op, &pad, &result_name, indent, names),

        OpKind::ForOp { .. } => emit_op_for(out, op, &pad, &result_name, indent, names),

        OpKind::WhileOp { .. } => emit_op_while(out, op, &pad, &result_name, indent, names),

        OpKind::FanOp { regions } => emit_op_fan(out, regions, &pad, &result_name, indent, names),

        OpKind::UnwrapOp { value } => {
            out.push_str(&format!("{}let {} = {}.unwrap();\n", pad, result_name, names.get(*value)));
        }
        OpKind::UnwrapOrOp { value, fallback } => {
            out.push_str(&format!("{}let {} = {}.unwrap_or({});\n", pad, result_name, names.get(*value), names.get(*fallback)));
        }
        OpKind::TryOp { value } => {
            out.push_str(&format!("{}let {} = {}?;\n", pad, result_name, names.get(*value)));
        }

        OpKind::MapOp { entries } => {
            out.push_str(&format!("{}let {} = HashMap::from([", pad, result_name));
            for (i, (k, v)) in entries.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                out.push_str(&format!("({}, {})", names.get(*k), names.get(*v)));
            }
            out.push_str("]);\n");
        }
        OpKind::EmptyMapOp => {
            out.push_str(&format!("{}let {} = HashMap::new();\n", pad, result_name));
        }

        OpKind::IndexOp { object, index } => {
            out.push_str(&format!("{}let {} = {}[{} as usize].clone();\n", pad, result_name, names.get(*object), names.get(*index)));
        }
        OpKind::MapAccessOp { object, key } => {
            out.push_str(&format!("{}let {} = {}.get(&{}).cloned();\n", pad, result_name, names.get(*object), names.get(*key)));
        }
        OpKind::TupleIndexOp { object, index } => {
            out.push_str(&format!("{}let {} = {}.{}.clone();\n", pad, result_name, names.get(*object), index));
        }

        _ => {
            out.push_str(&format!("{}let {} = (); // TODO: {:?}\n", pad, result_name, std::mem::discriminant(&op.kind)));
        }
    }
}

fn emit_op_binop(out: &mut String, op: &Operation, pad: &str, result_name: &str, names: &mut NameMap) {
    let OpKind::BinOp { op, lhs, rhs } = &op.kind else { unreachable!() };
    let op_str = match op {
        almide_ir::BinOp::AddInt | almide_ir::BinOp::AddFloat => "+",
        almide_ir::BinOp::SubInt | almide_ir::BinOp::SubFloat => "-",
        almide_ir::BinOp::MulInt | almide_ir::BinOp::MulFloat => "*",
        almide_ir::BinOp::DivInt | almide_ir::BinOp::DivFloat => "/",
        almide_ir::BinOp::ModInt | almide_ir::BinOp::ModFloat => "%",
        almide_ir::BinOp::Eq => "==",
        almide_ir::BinOp::Neq => "!=",
        almide_ir::BinOp::Lt => "<",
        almide_ir::BinOp::Gt => ">",
        almide_ir::BinOp::Lte => "<=",
        almide_ir::BinOp::Gte => ">=",
        almide_ir::BinOp::And => "&&",
        almide_ir::BinOp::Or => "||",
        almide_ir::BinOp::ConcatStr => {
            out.push_str(&format!("{}let {} = format!(\"{{}}{{}}\", {}, {});\n",
                pad, result_name, names.get(*lhs), names.get(*rhs)));
            return;
        }
        almide_ir::BinOp::ConcatList => {
            out.push_str(&format!("{}let {} = [{}, {}].concat();\n",
                pad, result_name, names.get(*lhs), names.get(*rhs)));
            return;
        }
        _ => "/* TODO */",
    };
    out.push_str(&format!("{}let {} = {} {} {};\n", pad, result_name, names.get(*lhs), op_str, names.get(*rhs)));
}

fn emit_op_call(out: &mut String, op: &Operation, pad: &str, result_name: &str, names: &mut NameMap) {
    let OpKind::CallOp { callee, args } = &op.kind else { unreachable!() };
    let callee_str = callee.as_str();
    // Check if callee is a variant constructor
    let variant_parent = names.variants.get(callee_str).cloned();
    if let Some(parent) = variant_parent {
        let args_str: Vec<_> = args.iter().map(|a| names.get(*a)).collect();
        if args_str.is_empty() {
            out.push_str(&format!("{}let {} = {}::{};\n", pad, result_name, parent, callee_str));
        } else {
            out.push_str(&format!("{}let {} = {}::{}({});\n", pad, result_name, parent, callee_str, args_str.join(", ")));
        }
        return;
    }
    if callee_str == "println" {
        let args_str: Vec<_> = args.iter().map(|a| names.get(*a)).collect();
        out.push_str(&format!("{}println!(\"{{}}\", {});\n", pad, args_str.join(", ")));
        out.push_str(&format!("{}let {} = ();\n", pad, result_name));
    } else {
        let resolved = resolve_call(callee_str);
        let is_user_fn = !callee_str.contains('.');
        let args_str: Vec<_> = args.iter().enumerate().map(|(i, a)| {
            let name = names.get(*a);
            if resolved.borrow_args.contains(&i) {
                format!("&{}", name)
            } else {
                name
            }
        }).collect();
        out.push_str(&format!("{}let {} = {}({});\n", pad, result_name, resolved.rust_name, args_str.join(", ")));
    }
}

fn emit_op_if(out: &mut String, op: &Operation, pad: &str, result_name: &str, indent: usize, names: &mut NameMap) {
    let OpKind::IfOp { cond, then_region, else_region } = &op.kind else { unreachable!() };
    out.push_str(&format!("{}let {} = if {} {{\n", pad, result_name, names.get(*cond)));
    for b in then_region { emit_block(out, b, indent + 1, names); }
    out.push_str(&format!("{}}} else {{\n", pad));
    for b in else_region { emit_block(out, b, indent + 1, names); }
    out.push_str(&format!("{}}};\n", pad));
}

fn emit_op_match(out: &mut String, op: &Operation, pad: &str, result_name: &str, indent: usize, names: &mut NameMap) {
    let OpKind::MatchOp { subject, arms } = &op.kind else { unreachable!() };
    out.push_str(&format!("{}let {} = match {} {{\n", pad, result_name, names.get(*subject)));
    for arm in arms {
        let pat = emit_pattern(&arm.pattern, names);
        indent_str(out, indent + 1);
        out.push_str(&format!("{} => {{\n", pat));
        for b in &arm.body { emit_block(out, b, indent + 2, names); }
        indent_str(out, indent + 1);
        out.push_str("},\n");
    }
    out.push_str(&format!("{}}};\n", pad));
}

fn emit_op_lambda(out: &mut String, op: &Operation, pad: &str, result_name: &str, indent: usize, names: &mut NameMap) {
    let OpKind::LambdaOp { params, body } = &op.kind else { unreachable!() };
    let ps: Vec<_> = params.iter().map(|(v, ty)| {
        let name = format!("v{}", v.0);
        names.set(*v, name.clone());
        format!("{}: {}", name, emit_rust_type(ty))
    }).collect();
    out.push_str(&format!("{}let {} = move |{}| {{\n", pad, result_name, ps.join(", ")));
    for b in body { emit_block(out, b, indent + 1, names); }
    out.push_str(&format!("{}}};\n", pad));
}

fn emit_op_for(out: &mut String, op: &Operation, pad: &str, result_name: &str, indent: usize, names: &mut NameMap) {
    let OpKind::ForOp { var, iterable, body } = &op.kind else { unreachable!() };
    let var_name = format!("v{}", var.0);
    names.set(*var, var_name.clone());
    out.push_str(&format!("{}for {} in {} {{\n", pad, var_name, names.get(*iterable)));
    for b in body { emit_block(out, b, indent + 1, names); }
    out.push_str(&format!("{}}}\n", pad));
    out.push_str(&format!("{}let {} = ();\n", pad, result_name));
}

fn emit_op_while(out: &mut String, op: &Operation, pad: &str, result_name: &str, indent: usize, names: &mut NameMap) {
    let OpKind::WhileOp { cond_region, body } = &op.kind else { unreachable!() };
    out.push_str(&format!("{}while {{\n", pad));
    // Emit cond as block that yields bool
    for b in cond_region { emit_block(out, b, indent + 1, names); }
    out.push_str(&format!("{}}} {{\n", pad));
    for b in body { emit_block(out, b, indent + 1, names); }
    out.push_str(&format!("{}}}\n", pad));
    out.push_str(&format!("{}let {} = ();\n", pad, result_name));
}

fn emit_op_fan(out: &mut String, regions: &[Vec<Block>], pad: &str, result_name: &str, indent: usize, names: &mut NameMap) {
    // Fan: emit each region sequentially (TODO: actual concurrency)
    for region in regions {
        for b in region { emit_block(out, b, indent, names); }
    }
    out.push_str(&format!("{}let {} = ();\n", pad, result_name));
}

/// Resolved stdlib call info.
struct ResolvedCall {
    rust_name: String,
    /// Which argument indices need `&` borrow.
    borrow_args: Vec<usize>,
}

/// Convert almide-style callee names to valid Rust identifiers and determine borrow needs.
fn resolve_call(callee: &str) -> ResolvedCall {
    if let Some(dot_pos) = callee.find('.') {
        let module = &callee[..dot_pos];
        let func_part = &callee[dot_pos + 1..];
        let func = if let Some(mono_pos) = func_part.find("__") {
            &func_part[..mono_pos]
        } else {
            func_part
        };
        let rust_name = format!("almide_rt_{}_{}", module, func);

        // Determine which args need borrow based on module + function.
        // List: read-only fns take &[T] (first arg), consuming fns take Vec<T>.
        // String: most fns take &str (first arg).
        // Map: read-only fns take &HashMap (first arg).
        // Int/Float: all by value.
        let borrow_args = match module {
            "list" => {
                // Consuming: map, filter, fold, find, flat_map, filter_map, take, drop,
                // enumerate, zip, zip_with, flatten, take_while, drop_while, partition,
                // group_by, find_index, update, scan, unique_by, slice, insert, remove_at,
                // intersperse, take_end, drop_end, shuffle, window, reduce
                let consuming = [
                    "map", "filter", "fold", "find", "flat_map", "filter_map",
                    "take", "drop", "enumerate", "zip", "zip_with", "flatten",
                    "take_while", "drop_while", "partition", "group_by", "find_index",
                    "update", "scan", "unique_by", "slice", "insert", "remove_at",
                    "intersperse", "take_end", "drop_end", "shuffle", "window", "reduce",
                    "sort_by", "any", "all", "count", "push", "pop", "clear",
                ];
                if consuming.contains(&func) { vec![] } else { vec![0] }
            }
            "string" => vec![0], // All string fns take &str
            "map" => {
                // get, len, keys, values, contains_key, is_empty take &HashMap
                let read_only = ["get", "len", "keys", "values", "contains_key", "is_empty"];
                if read_only.contains(&func) { vec![0] } else { vec![] }
            }
            _ => vec![], // int, float, math, etc. — by value
        };

        ResolvedCall { rust_name, borrow_args }
    } else {
        ResolvedCall { rust_name: callee.to_string(), borrow_args: vec![] }
    }
}

fn indent_str(out: &mut String, level: usize) {
    for _ in 0..level { out.push_str("    "); }
}

fn emit_pattern(pattern: &MatchPattern, names: &mut NameMap) -> String {
    match pattern {
        MatchPattern::Wildcard => "_".into(),
        MatchPattern::LitInt(v) => format!("{}", v),
        MatchPattern::LitStr(v) => format!("\"{}\"", v.escape_default()),
        MatchPattern::LitBool(v) => format!("{}", v),
        MatchPattern::Binding(v) => {
            let name = format!("v{}", v.0);
            names.set(*v, name.clone());
            name
        }
        MatchPattern::Variant { tag, bindings } => {
            let tag_str = tag.as_str();
            let qualified = names.variants.get(tag_str)
                .map(|parent| format!("{}::{}", parent, tag_str))
                .unwrap_or_else(|| tag_str.to_string());
            if bindings.is_empty() {
                qualified
            } else {
                let bs: Vec<_> = bindings.iter().map(|v| {
                    let name = format!("v{}", v.0);
                    names.set(*v, name.clone());
                    name
                }).collect();
                format!("{}({})", qualified, bs.join(", "))
            }
        }
        MatchPattern::Record { fields } => {
            let fs: Vec<_> = fields.iter().map(|(n, v)| {
                let name = format!("v{}", v.0);
                names.set(*v, name.clone());
                format!("{}: {}", n, name)
            }).collect();
            format!("{{ {} }}", fs.join(", "))
        }
        MatchPattern::Tuple(elems) => {
            let ps: Vec<_> = elems.iter().map(|p| emit_pattern(p, names)).collect();
            format!("({})", ps.join(", "))
        }
    }
}
