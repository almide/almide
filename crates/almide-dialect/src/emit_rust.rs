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
}

impl NameMap {
    fn new() -> Self { NameMap { names: std::collections::HashMap::new() } }

    fn set(&mut self, id: ValueId, name: String) { self.names.insert(id, name); }

    fn get(&self, id: ValueId) -> String {
        self.names.get(&id).cloned().unwrap_or_else(|| format!("v{}", id.0))
    }
}

/// Emit a complete Rust source file from a dialect Module.
pub fn emit_module(module: &Module) -> String {
    let mut out = String::new();

    // Prelude
    out.push_str("#![allow(unused_variables, unused_mut, dead_code, unused_imports)]\n");
    out.push_str("use std::collections::HashMap;\n\n");

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
        emit_func(&mut out, f);
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

fn emit_func(out: &mut String, f: &FuncOp) {
    let mut names = NameMap::new();

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

        OpKind::BinOp { op, lhs, rhs } => {
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
        OpKind::UnOp { op, operand } => {
            let op_str = match op {
                almide_ir::UnOp::NegInt | almide_ir::UnOp::NegFloat => "-",
                almide_ir::UnOp::Not => "!",
            };
            out.push_str(&format!("{}let {} = {}{};\n", pad, result_name, op_str, names.get(*operand)));
        }

        OpKind::CallOp { callee, args } => {
            let args_str: Vec<_> = args.iter().map(|a| names.get(*a)).collect();
            let callee_str = callee.as_str();
            if callee_str == "println" {
                out.push_str(&format!("{}println!(\"{{}}\", {});\n", pad, args_str.join(", ")));
                out.push_str(&format!("{}let {} = ();\n", pad, result_name));
            } else {
                out.push_str(&format!("{}let {} = {}({});\n", pad, result_name, callee_str, args_str.join(", ")));
            }
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

        OpKind::IfOp { cond, then_region, else_region } => {
            out.push_str(&format!("{}let {} = if {} {{\n", pad, result_name, names.get(*cond)));
            for b in then_region { emit_block(out, b, indent + 1, names); }
            out.push_str(&format!("{}}} else {{\n", pad));
            for b in else_region { emit_block(out, b, indent + 1, names); }
            out.push_str(&format!("{}}};\n", pad));
        }

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

        _ => {
            out.push_str(&format!("{}let {} = (); // TODO: {:?}\n", pad, result_name, std::mem::discriminant(&op.kind)));
        }
    }
}
