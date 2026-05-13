//! WGSL emitter: `@gpu` annotated functions → WGSL shader source.
//!
//! Phase 0 scope:
//! - `@gpu(vertex)` / `@gpu(fragment)` / `@gpu(compute)` functions → WGSL
//! - Struct definitions referenced by GPU functions → WGSL structs
//! - Type mapping: Vec2→vec2<f32>, Vec4→vec4<f32>, Float→f32, Int→i32, UInt32→u32
//!
//! Unlike `emit_wasm` (binary output), this emits text (WGSL source).
//! Unlike the Walker (template-driven), this is direct emit because WGSL
//! syntax differs too much from Rust/TS to share templates.

use almide_ir::*;
use almide_lang::types::Ty;

/// GPU shader stage, parsed from `@gpu(vertex)` / `@gpu(fragment)` / `@gpu(compute)`.
#[derive(Debug, Clone, Copy)]
enum GpuStage {
    Vertex,
    Fragment,
    Compute,
}

/// Emit all `@gpu` functions from the program as a single WGSL source string.
pub fn emit(program: &IrProgram) -> String {
    let mut out = String::new();

    // Collect GPU functions
    let gpu_fns: Vec<(&IrFunction, GpuStage)> = program
        .functions
        .iter()
        .filter_map(|f| parse_gpu_stage(f).map(|stage| (f, stage)))
        .collect();

    if gpu_fns.is_empty() {
        return out;
    }

    // Emit struct definitions used by GPU functions
    for td in &program.type_decls {
        if let IrTypeDeclKind::Record { fields } = &td.kind {
            out.push_str(&emit_struct(td, fields));
            out.push('\n');
        }
    }

    // Emit GPU functions
    for (func, stage) in &gpu_fns {
        out.push_str(&emit_gpu_function(func, *stage, &program.var_table));
        out.push('\n');
    }

    out
}

/// Parse `@gpu(vertex|fragment|compute)` from function attributes.
fn parse_gpu_stage(func: &IrFunction) -> Option<GpuStage> {
    for attr in &func.attrs {
        if attr.name.as_str() == "gpu" {
            if let Some(arg) = attr.args.first() {
                if let almide_lang::ast::AttrValue::Ident { name } = &arg.value {
                    return match name.as_str() {
                        "vertex" => Some(GpuStage::Vertex),
                        "fragment" => Some(GpuStage::Fragment),
                        "compute" => Some(GpuStage::Compute),
                        _ => None,
                    };
                }
            }
        }
    }
    None
}

/// Emit a WGSL struct definition.
fn emit_struct(td: &IrTypeDecl, fields: &[IrFieldDecl]) -> String {
    let mut out = format!("struct {} {{\n", td.name.as_str());
    for f in fields {
        let prefix = emit_wgsl_attrs(&f.attrs);
        if prefix.is_empty() {
            out.push_str(&format!("  {}: {},\n", f.name.as_str(), emit_type(&f.ty)));
        } else {
            out.push_str(&format!("  {} {}: {},\n", prefix, f.name.as_str(), emit_type(&f.ty)));
        }
    }
    out.push_str("}\n");
    out
}

/// Emit a GPU function as WGSL.
fn emit_gpu_function(func: &IrFunction, stage: GpuStage, vt: &VarTable) -> String {
    let mut out = String::new();

    // Stage annotation
    let stage_attr = match stage {
        GpuStage::Vertex => "@vertex",
        GpuStage::Fragment => "@fragment",
        GpuStage::Compute => "@compute @workgroup_size(256)",
    };
    out.push_str(stage_attr);
    out.push('\n');

    // Function signature
    out.push_str(&format!("fn {}(", func.name.as_str()));

    // Parameters with WGSL annotations
    let params: Vec<String> = func
        .params
        .iter()
        .map(|p| {
            let wgsl_ty = emit_type(&p.ty);
            let prefix = emit_wgsl_attrs(&p.attrs);
            if prefix.is_empty() {
                format!("{}: {}", p.name.as_str(), wgsl_ty)
            } else {
                format!("{} {}: {}", prefix, p.name.as_str(), wgsl_ty)
            }
        })
        .collect();
    out.push_str(&params.join(", "));
    out.push_str(") -> ");

    // Return type may have annotations from @location on the function
    let ret_annotation = emit_return_attrs(func);
    if ret_annotation.is_empty() {
        out.push_str(&emit_type(&func.ret_ty));
    } else {
        out.push_str(&format!("{} {}", ret_annotation, emit_type(&func.ret_ty)));
    }

    out.push_str(" {\n");

    // Body
    out.push_str(&emit_expr(&func.body, vt, 1));

    out.push_str("}\n");
    out
}

/// Map Almide types to WGSL types.
fn emit_type(ty: &Ty) -> String {
    match ty {
        Ty::Float | Ty::Float64 => "f32".to_string(),
        Ty::Float32 => "f32".to_string(),
        Ty::Int | Ty::Int64 => "i32".to_string(),
        Ty::Int32 => "i32".to_string(),
        Ty::Int16 => "i16".to_string(),
        Ty::Int8 => "i32".to_string(),
        Ty::UInt32 => "u32".to_string(),
        Ty::UInt16 => "u32".to_string(),
        Ty::UInt8 => "u32".to_string(),
        Ty::UInt64 => "u32".to_string(),
        Ty::Bool => "bool".to_string(),
        Ty::Unit => "void".to_string(),
        Ty::Named(name, _args) => {
            match name.as_str() {
                "Vec2" => "vec2<f32>".to_string(),
                "Vec3" => "vec3<f32>".to_string(),
                "Vec4" => "vec4<f32>".to_string(),
                "Mat3" => "mat3x3<f32>".to_string(),
                "Mat4" => "mat4x4<f32>".to_string(),
                "UInt32" => "u32".to_string(),
                other => other.to_string(),
            }
        }
        Ty::Applied(_ctor, args) => {
            // List[T] → array<T>
            if let Some(elem) = args.first() {
                format!("array<{}>", emit_type(elem))
            } else {
                "array<f32>".to_string()
            }
        }
        _ => format!("/* unsupported type */"),
    }
}

/// Emit an expression as WGSL (block-level, with returns).
fn emit_expr(expr: &IrExpr, vt: &VarTable, indent: usize) -> String {
    let pad = "  ".repeat(indent);
    match &expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            let mut out = String::new();
            for stmt in stmts {
                out.push_str(&emit_stmt(stmt, vt, indent));
            }
            if let Some(tail) = tail {
                out.push_str(&format!("{}return {};\n", pad, emit_expr_inline(tail, vt)));
            }
            out
        }
        _ => format!("{}return {};\n", pad, emit_expr_inline(expr, vt)),
    }
}

/// Emit an expression as an inline value (no semicolons/returns).
fn emit_expr_inline(expr: &IrExpr, vt: &VarTable) -> String {
    match &expr.kind {
        IrExprKind::LitInt { value } => format!("{}", value),
        IrExprKind::LitFloat { value } => format_float(*value),
        IrExprKind::LitBool { value } => format!("{}", value),

        IrExprKind::Var { id } => {
            vt.get(*id).name.as_str().to_string()
        }

        IrExprKind::BinOp { op, left, right } => {
            let l = emit_expr_inline(left, vt);
            let r = emit_expr_inline(right, vt);
            let op_str = match op {
                BinOp::AddInt | BinOp::AddFloat => "+",
                BinOp::SubInt | BinOp::SubFloat => "-",
                BinOp::MulInt | BinOp::MulFloat => "*",
                BinOp::DivInt | BinOp::DivFloat => "/",
                BinOp::ModInt | BinOp::ModFloat => "%",
                BinOp::Eq => "==",
                BinOp::Neq => "!=",
                BinOp::Lt => "<",
                BinOp::Lte => "<=",
                BinOp::Gt => ">",
                BinOp::Gte => ">=",
                BinOp::And => "&&",
                BinOp::Or => "||",
                _ => "/* unsupported op */",
            };
            format!("({} {} {})", l, op_str, r)
        }

        IrExprKind::UnOp { op, operand } => {
            let e = emit_expr_inline(operand, vt);
            match op {
                UnOp::NegInt | UnOp::NegFloat => format!("(-{})", e),
                UnOp::Not => format!("(!{})", e),
            }
        }

        IrExprKind::Member { object, field } => {
            format!("{}.{}", emit_expr_inline(object, vt), field.as_str())
        }

        IrExprKind::IndexAccess { object, index } => {
            format!("{}[{}]", emit_expr_inline(object, vt), emit_expr_inline(index, vt))
        }

        IrExprKind::Record { name, fields } => {
            let name_str = name.map(|n| n.as_str().to_string()).unwrap_or_default();
            let field_strs: Vec<String> = fields
                .iter()
                .map(|(k, v)| format!("{}: {}", k.as_str(), emit_expr_inline(v, vt)))
                .collect();
            format!("{} {{ {} }}", name_str, field_strs.join(", "))
        }

        IrExprKind::List { elements } => {
            // Fixed-size array literal
            if elements.is_empty() {
                return "array()".to_string();
            }
            let elem_ty = emit_type(&elements[0].ty);
            let elems: Vec<String> = elements.iter().map(|e| emit_expr_inline(e, vt)).collect();
            format!(
                "array<{}, {}>({})",
                elem_ty,
                elements.len(),
                elems.join(", ")
            )
        }

        IrExprKind::Call { target, args, .. } => {
            match target {
                CallTarget::Named { name } => {
                    let arg_strs: Vec<String> = args.iter().map(|a| emit_expr_inline(a, vt)).collect();
                    format!("{}({})", name.as_str(), arg_strs.join(", "))
                }
                CallTarget::Module { module, func, .. } => {
                    let m = module.as_str();
                    let f = func.as_str();
                    let arg_strs: Vec<String> = args.iter().map(|a| emit_expr_inline(a, vt)).collect();
                    // Map lumen module calls to WGSL builtins
                    match (m, f) {
                        (_, "new") if m.contains("v2") || m.contains("vec2") => {
                            format!("vec2<f32>({})", arg_strs.join(", "))
                        }
                        (_, "new") if m.contains("v3") || m.contains("vec3") => {
                            format!("vec3<f32>({})", arg_strs.join(", "))
                        }
                        (_, "new") if m.contains("v4") || m.contains("vec4") => {
                            format!("vec4<f32>({})", arg_strs.join(", "))
                        }
                        _ => format!("{}({})", f, arg_strs.join(", ")),
                    }
                }
                _ => "/* unknown call */".to_string(),
            }
        }

        IrExprKind::If { cond, then, else_ } => {
            format!(
                "select({}, {}, {})",
                emit_expr_inline(else_, vt),
                emit_expr_inline(then, vt),
                emit_expr_inline(cond, vt),
            )
        }

        IrExprKind::Block { stmts: _, expr: Some(tail) } => {
            emit_expr_inline(tail, vt)
        }

        _ => format!("/* unsupported expr */"),
    }
}

/// Emit a statement as WGSL.
fn emit_stmt(stmt: &IrStmt, vt: &VarTable, indent: usize) -> String {
    let pad = "  ".repeat(indent);
    match &stmt.kind {
        IrStmtKind::Bind { var, value, mutability, .. } => {
            let name = vt.get(*var).name.as_str();
            let keyword = if *mutability == Mutability::Var { "var" } else { "let" };
            format!("{}{} {} = {};\n", pad, keyword, name, emit_expr_inline(value, vt))
        }
        IrStmtKind::Assign { var, value } => {
            let name = vt.get(*var).name.as_str();
            format!("{}{} = {};\n", pad, name, emit_expr_inline(value, vt))
        }
        IrStmtKind::Expr { expr } => {
            format!("{}{};\n", pad, emit_expr_inline(expr, vt))
        }
        _ => format!("{}/* unsupported stmt */\n", pad),
    }
}

/// Emit WGSL annotations from Almide attributes.
/// Maps `@builtin(position)` → `@builtin(position)`,
///      `@location(0)` → `@location(0)`.
fn emit_wgsl_attrs(attrs: &[almide_lang::ast::Attribute]) -> String {
    let parts: Vec<String> = attrs.iter().filter_map(|attr| {
        let name = attr.name.as_str();
        match name {
            "builtin" | "location" | "group" | "binding" => {
                if attr.args.is_empty() {
                    Some(format!("@{}", name))
                } else {
                    let args: Vec<String> = attr.args.iter().map(|a| {
                        match &a.value {
                            almide_lang::ast::AttrValue::Ident { name } => name.as_str().to_string(),
                            almide_lang::ast::AttrValue::Int { value } => format!("{}", value),
                            almide_lang::ast::AttrValue::String { value } => value.clone(),
                            almide_lang::ast::AttrValue::Bool { value } => format!("{}", value),
                        }
                    }).collect();
                    Some(format!("@{}({})", name, args.join(", ")))
                }
            }
            _ => None,
        }
    }).collect();
    parts.join(" ")
}

/// Extract @location annotations from function-level attrs for the return type.
fn emit_return_attrs(func: &IrFunction) -> String {
    let parts: Vec<String> = func.attrs.iter().filter_map(|attr| {
        if attr.name.as_str() == "location" {
            let args: Vec<String> = attr.args.iter().map(|a| {
                match &a.value {
                    almide_lang::ast::AttrValue::Int { value } => format!("{}", value),
                    almide_lang::ast::AttrValue::Ident { name } => name.as_str().to_string(),
                    _ => String::new(),
                }
            }).collect();
            Some(format!("@location({})", args.join(", ")))
        } else {
            None
        }
    }).collect();
    parts.join(" ")
}

/// Format a float for WGSL (ensure decimal point is present).
fn format_float(v: f64) -> String {
    let s = format!("{}", v);
    if s.contains('.') || s.contains('e') || s.contains('E') {
        s
    } else {
        format!("{}.0", s)
    }
}
