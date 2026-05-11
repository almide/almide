//! Pretty-printer for dialect Module — human-readable MLIR-like textual form.

use crate::{Module, Block, ValueId};
use crate::ops::*;
use crate::types::DialectType;

pub fn dump_module(module: &Module) -> String {
    let mut out = String::new();
    out.push_str("// Almide dialect dump\n");
    if let Some(name) = module.name {
        out.push_str(&format!("module @{} {{\n", name));
    } else {
        out.push_str("module {\n");
    }

    for td in &module.type_decls {
        dump_type_decl(&mut out, td, 1);
    }

    for g in &module.globals {
        dump_global(&mut out, g, 1);
    }

    for f in &module.functions {
        dump_func(&mut out, f, 1);
    }

    out.push_str("}\n");
    out
}

fn indent(out: &mut String, level: usize) {
    for _ in 0..level { out.push_str("  "); }
}

fn fmt_val(v: ValueId) -> String { format!("%{}", v.0) }

fn fmt_type(ty: &DialectType) -> String {
    match ty {
        DialectType::I64 => "i64".into(),
        DialectType::F64 => "f64".into(),
        DialectType::Bool => "bool".into(),
        DialectType::Unit => "unit".into(),
        DialectType::String => "string".into(),
        DialectType::Bytes => "bytes".into(),
        DialectType::I8 => "i8".into(),
        DialectType::I16 => "i16".into(),
        DialectType::I32 => "i32".into(),
        DialectType::U8 => "u8".into(),
        DialectType::U16 => "u16".into(),
        DialectType::U32 => "u32".into(),
        DialectType::U64 => "u64".into(),
        DialectType::F32 => "f32".into(),
        DialectType::Matrix => "matrix".into(),
        DialectType::RawPtr => "rawptr".into(),
        DialectType::Unknown => "unknown".into(),
        DialectType::List(inner) => format!("list<{}>", fmt_type(inner)),
        DialectType::Map(k, v) => format!("map<{}, {}>", fmt_type(k), fmt_type(v)),
        DialectType::Option(inner) => format!("option<{}>", fmt_type(inner)),
        DialectType::Result(ok, err) => format!("result<{}, {}>", fmt_type(ok), fmt_type(err)),
        DialectType::Tuple(elems) => {
            let parts: Vec<_> = elems.iter().map(|e| fmt_type(e)).collect();
            format!("tuple<{}>", parts.join(", "))
        }
        DialectType::Named(sym) => format!("!{}", sym),
        DialectType::Record(fields) => {
            let parts: Vec<_> = fields.iter().map(|(n, t)| format!("{}: {}", n, fmt_type(t))).collect();
            format!("record<{}>", parts.join(", "))
        }
        DialectType::Fn { params, ret } => {
            let ps: Vec<_> = params.iter().map(|p| fmt_type(p)).collect();
            format!("fn({}) -> {}", ps.join(", "), fmt_type(ret))
        }
        DialectType::Closure { params, ret } => {
            let ps: Vec<_> = params.iter().map(|p| fmt_type(p)).collect();
            format!("closure({}) -> {}", ps.join(", "), fmt_type(ret))
        }
    }
}

fn dump_func(out: &mut String, f: &FuncOp, level: usize) {
    indent(out, level);
    let effect = if f.is_effect { "effect " } else { "" };
    let test = if f.is_test { " [test]" } else { "" };
    let params: Vec<_> = f.params.iter().map(|(n, t)| format!("{}: {}", n, fmt_type(t))).collect();
    out.push_str(&format!("{}func @{}({}) -> {}{} {{\n",
        effect, f.name, params.join(", "), fmt_type(&f.ret_ty), test));
    for block in &f.body {
        dump_block(out, block, level + 1);
    }
    indent(out, level);
    out.push_str("}\n\n");
}

fn dump_block(out: &mut String, block: &Block, level: usize) {
    indent(out, level);
    if block.args.is_empty() {
        out.push_str(&format!("bb{}:\n", block.id.0));
    } else {
        let args: Vec<_> = block.args.iter().map(|(v, t)| format!("{}: {}", fmt_val(*v), fmt_type(t))).collect();
        out.push_str(&format!("bb{}({}):\n", block.id.0, args.join(", ")));
    }
    for op in &block.ops {
        dump_op(out, op, level + 1);
    }
    indent(out, level + 1);
    dump_terminator(out, &block.terminator);
    out.push('\n');
}

fn dump_op(out: &mut String, op: &Operation, level: usize) {
    indent(out, level);
    if let Some(result) = op.result {
        out.push_str(&format!("{} = ", fmt_val(result)));
    }
    match &op.kind {
        OpKind::ConstInt(v) => out.push_str(&format!("almide.const {} : i64", v)),
        OpKind::ConstFloat(v) => out.push_str(&format!("almide.const {:.6} : f64", v)),
        OpKind::ConstBool(v) => out.push_str(&format!("almide.const {} : bool", v)),
        OpKind::ConstString(v) => out.push_str(&format!("almide.const \"{}\" : string", v.escape_default())),
        OpKind::ConstUnit => out.push_str("almide.const () : unit"),

        OpKind::BinOp { op: binop, lhs, rhs } => {
            out.push_str(&format!("almide.binop {:?} {}, {}", binop, fmt_val(*lhs), fmt_val(*rhs)));
        }
        OpKind::UnOp { op: unop, operand } => {
            out.push_str(&format!("almide.unop {:?} {}", unop, fmt_val(*operand)));
        }

        OpKind::CallOp { callee, args } => {
            let args_str: Vec<_> = args.iter().map(|a| fmt_val(*a)).collect();
            out.push_str(&format!("almide.call @{}({})", callee, args_str.join(", ")));
        }
        OpKind::AllocVar { init, ty } => {
            out.push_str(&format!("almide.alloc_var {} : {}", fmt_val(*init), fmt_type(ty)));
        }
        OpKind::LoadVar { slot } => {
            out.push_str(&format!("almide.load_var {}", fmt_val(*slot)));
        }
        OpKind::StoreVar { slot, value } => {
            out.push_str(&format!("almide.store_var {}, {}", fmt_val(*slot), fmt_val(*value)));
        }
        OpKind::ComputedCallOp { callee, args } => {
            let args_str: Vec<_> = args.iter().map(|a| fmt_val(*a)).collect();
            out.push_str(&format!("almide.computed_call {}({})", fmt_val(*callee), args_str.join(", ")));
        }
        OpKind::IntrinsicCallOp { symbol, args } => {
            let args_str: Vec<_> = args.iter().map(|a| fmt_val(*a)).collect();
            out.push_str(&format!("almide.intrinsic @{}({})", symbol, args_str.join(", ")));
        }

        OpKind::IfOp { cond, .. } => {
            out.push_str(&format!("almide.if {} ...", fmt_val(*cond)));
        }
        OpKind::MatchOp { subject, arms } => {
            out.push_str(&format!("almide.match {} [{} arms]", fmt_val(*subject), arms.len()));
        }

        OpKind::ListOp { elements } => {
            let vals: Vec<_> = elements.iter().map(|v| fmt_val(*v)).collect();
            out.push_str(&format!("almide.list [{}]", vals.join(", ")));
        }
        OpKind::MapOp { entries } => {
            out.push_str(&format!("almide.map [{} entries]", entries.len()));
        }
        OpKind::EmptyMapOp => out.push_str("almide.empty_map"),
        OpKind::RecordOp { name, fields } => {
            let n = name.map(|s| s.to_string()).unwrap_or_default();
            let fs: Vec<_> = fields.iter().map(|(k, v)| format!("{}: {}", k, fmt_val(*v))).collect();
            out.push_str(&format!("almide.record @{} {{{}}}", n, fs.join(", ")));
        }
        OpKind::TupleOp { elements } => {
            let vals: Vec<_> = elements.iter().map(|v| fmt_val(*v)).collect();
            out.push_str(&format!("almide.tuple ({})", vals.join(", ")));
        }

        OpKind::MemberOp { object, field } => {
            out.push_str(&format!("almide.member {}.{}", fmt_val(*object), field));
        }
        OpKind::TupleIndexOp { object, index } => {
            out.push_str(&format!("almide.tuple_index {}.{}", fmt_val(*object), index));
        }
        OpKind::IndexOp { object, index } => {
            out.push_str(&format!("almide.index {}[{}]", fmt_val(*object), fmt_val(*index)));
        }
        OpKind::MapAccessOp { object, key } => {
            out.push_str(&format!("almide.map_access {}[{}]", fmt_val(*object), fmt_val(*key)));
        }

        OpKind::ResultOkOp { value } => out.push_str(&format!("almide.ok {}", fmt_val(*value))),
        OpKind::ResultErrOp { value } => out.push_str(&format!("almide.err {}", fmt_val(*value))),
        OpKind::OptionSomeOp { value } => out.push_str(&format!("almide.some {}", fmt_val(*value))),
        OpKind::OptionNoneOp => out.push_str("almide.none"),
        OpKind::TryOp { value } => out.push_str(&format!("almide.try {}", fmt_val(*value))),
        OpKind::UnwrapOp { value } => out.push_str(&format!("almide.unwrap {}", fmt_val(*value))),
        OpKind::UnwrapOrOp { value, fallback } => {
            out.push_str(&format!("almide.unwrap_or {}, {}", fmt_val(*value), fmt_val(*fallback)));
        }

        OpKind::LambdaOp { params, .. } => {
            let ps: Vec<_> = params.iter().map(|(v, t)| format!("{}: {}", fmt_val(*v), fmt_type(t))).collect();
            out.push_str(&format!("almide.lambda ({}) ...", ps.join(", ")));
        }

        OpKind::FanOp { regions } => {
            out.push_str(&format!("almide.fan [{} branches]", regions.len()));
        }
        OpKind::ForOp { var, iterable, .. } => {
            out.push_str(&format!("almide.for {} in {} ...", fmt_val(*var), fmt_val(*iterable)));
        }
        OpKind::WhileOp { .. } => {
            out.push_str("almide.while ...");
        }
    }
    out.push_str(&format!(" : {}\n", fmt_type(&op.result_ty)));
}

fn dump_terminator(out: &mut String, term: &Terminator) {
    match term {
        Terminator::Yield(v) => out.push_str(&format!("yield {}\n", fmt_val(*v))),
        Terminator::Return(v) => out.push_str(&format!("return {}\n", fmt_val(*v))),
        Terminator::Branch(dest, args) => {
            let args_str: Vec<_> = args.iter().map(|a| fmt_val(*a)).collect();
            out.push_str(&format!("br bb{}({})\n", dest.0, args_str.join(", ")));
        }
        Terminator::CondBranch { cond, true_dest, false_dest } => {
            out.push_str(&format!("cond_br {}, bb{}, bb{}\n", fmt_val(*cond), true_dest.0, false_dest.0));
        }
        Terminator::Fallthrough => out.push_str("fallthrough\n"),
        Terminator::Break => out.push_str("break\n"),
        Terminator::Continue => out.push_str("continue\n"),
    }
}

fn dump_type_decl(out: &mut String, td: &TypeDeclOp, level: usize) {
    indent(out, level);
    match &td.kind {
        TypeDeclKind::Record { fields } => {
            let fs: Vec<_> = fields.iter().map(|(n, t)| format!("{}: {}", n, fmt_type(t))).collect();
            out.push_str(&format!("almide.type @{} = record<{}>\n", td.name, fs.join(", ")));
        }
        TypeDeclKind::Variant { cases } => {
            let cs: Vec<_> = cases.iter().map(|c| {
                if c.payload.is_empty() { c.name.to_string() }
                else {
                    let ps: Vec<_> = c.payload.iter().map(|t| fmt_type(t)).collect();
                    format!("{}({})", c.name, ps.join(", "))
                }
            }).collect();
            out.push_str(&format!("almide.type @{} = variant<{}>\n", td.name, cs.join(" | ")));
        }
        TypeDeclKind::Alias(ty) => {
            out.push_str(&format!("almide.type @{} = {}\n", td.name, fmt_type(ty)));
        }
    }
}

fn dump_global(out: &mut String, g: &GlobalOp, level: usize) {
    indent(out, level);
    out.push_str(&format!("almide.global @{} : {} = {{\n", g.name, fmt_type(&g.ty)));
    for block in &g.init {
        dump_block(out, block, level + 1);
    }
    indent(out, level);
    out.push_str("}\n\n");
}
