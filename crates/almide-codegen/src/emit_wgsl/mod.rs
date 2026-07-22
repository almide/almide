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

    // Emit uniform declarations and GPU functions.
    // Auto-assign @group/@binding: uniforms get group(0), numbered by order.
    let mut binding_counter: u32 = 0;
    for (func, stage) in &gpu_fns {
        let (uniform_decls, func_code) =
            emit_gpu_function_with_uniforms(func, *stage, &program.var_table, &mut binding_counter);
        out.push_str(&uniform_decls);
        out.push_str(&func_code);
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

/// A GPU resource parameter extracted from an @gpu function.
#[derive(Debug)]
struct GpuResource {
    name: String,
    ty: String,
    var_id: VarId,
    kind: GpuResourceKind,
}

#[derive(Debug)]
enum GpuResourceKind {
    Uniform,
    StorageRead,
    StorageReadWrite,
}

/// Parse GPU resource kind from parameter attributes.
fn parse_resource_kind(attrs: &[almide_lang::ast::Attribute]) -> Option<GpuResourceKind> {
    for attr in attrs {
        match attr.name.as_str() {
            "uniform" => return Some(GpuResourceKind::Uniform),
            "storage" => {
                // @storage or @storage(read) or @storage(read_write)
                if let Some(arg) = attr.args.first() {
                    if let almide_lang::ast::AttrValue::Ident { name } = &arg.value {
                        return match name.as_str() {
                            "read" => Some(GpuResourceKind::StorageRead),
                            "read_write" => Some(GpuResourceKind::StorageReadWrite),
                            _ => Some(GpuResourceKind::StorageRead),
                        };
                    }
                }
                return Some(GpuResourceKind::StorageRead);
            }
            _ => {}
        }
    }
    None
}

/// Emit a GPU function as WGSL, separating @uniform/@storage params into
/// module-level var declarations. Returns (resource_decls, function_code).
fn emit_gpu_function_with_uniforms(
    func: &IrFunction,
    stage: GpuStage,
    vt: &VarTable,
    binding_counter: &mut u32,
) -> (String, String) {
    // Separate GPU resource params from regular params
    let mut resources: Vec<GpuResource> = Vec::new();
    let mut regular_params: Vec<&IrParam> = Vec::new();

    for p in &func.params {
        if let Some(kind) = parse_resource_kind(&p.attrs) {
            resources.push(GpuResource {
                name: p.name.as_str().to_string(),
                ty: emit_type(&p.ty),
                var_id: p.var,
                kind,
            });
        } else {
            regular_params.push(p);
        }
    }

    // Generate resource declarations with auto-assigned @group/@binding
    // Uniform → group(0), Storage → group(0) (same group, different bindings)
    let mut decls = String::new();
    let mut rewrite_map: Vec<(VarId, String)> = Vec::new(); // (var_id, wgsl_var_name)

    for res in &resources {
        let var_name = format!("_{}", res.name);
        let var_qualifier = match res.kind {
            GpuResourceKind::Uniform => "uniform",
            GpuResourceKind::StorageRead => "storage, read",
            GpuResourceKind::StorageReadWrite => "storage, read_write",
        };
        decls.push_str(&format!(
            "@group(0) @binding({})\nvar<{}> {}: {};\n\n",
            binding_counter, var_qualifier, var_name, res.ty
        ));
        rewrite_map.push((res.var_id, var_name));
        *binding_counter += 1;
    }

    // Build function code
    let mut out = String::new();

    // Stage annotation
    let stage_attr = match stage {
        GpuStage::Vertex => "@vertex".to_string(),
        GpuStage::Fragment => "@fragment".to_string(),
        GpuStage::Compute => {
            let wg = parse_workgroup_size(func);
            format!("@compute @workgroup_size({})", wg)
        }
    };
    out.push_str(&stage_attr);
    out.push('\n');

    // Function signature (only regular params)
    out.push_str(&format!("fn {}(", func.name.as_str()));
    let params: Vec<String> = regular_params
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

    // Return type
    let ret_annotation = emit_return_attrs(func);
    if ret_annotation.is_empty() {
        out.push_str(&emit_type(&func.ret_ty));
    } else {
        out.push_str(&format!("{} {}", ret_annotation, emit_type(&func.ret_ty)));
    }

    out.push_str(" {\n");

    // Body — with resource variable rewriting
    let body_str = emit_expr(&func.body, vt, 1);
    let mut rewritten = body_str;
    for (var_id, var_name) in &rewrite_map {
        let param_name = vt.get(*var_id).name.as_str().to_string();
        rewritten = replace_identifier(&rewritten, &param_name, var_name);
    }
    out.push_str(&rewritten);

    out.push_str("}\n");
    (decls, out)
}

/// Replace a standalone identifier in code, respecting word boundaries.
fn replace_identifier(code: &str, from: &str, to: &str) -> String {
    let mut result = String::with_capacity(code.len());
    let bytes = code.as_bytes();
    let from_bytes = from.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + from_bytes.len() <= bytes.len() && &bytes[i..i + from_bytes.len()] == from_bytes {
            let before_ok = i == 0 || !is_ident_char(bytes[i - 1]);
            let after_ok = i + from_bytes.len() >= bytes.len()
                || !is_ident_char(bytes[i + from_bytes.len()]);
            if before_ok && after_ok {
                result.push_str(to);
                i += from_bytes.len();
                continue;
            }
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    result
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Parse workgroup size from @gpu(compute, workgroup = [x, y, z]).
/// Falls back to "256" if not specified.
fn parse_workgroup_size(func: &IrFunction) -> String {
    for attr in &func.attrs {
        if attr.name.as_str() == "gpu" {
            for arg in &attr.args {
                if arg.name.as_ref().map(|n| n.as_str()) == Some("workgroup") {
                    // workgroup value is stored as a string "[256, 1, 1]"
                    if let almide_lang::ast::AttrValue::String { value } = &arg.value {
                        let trimmed = value.trim_matches(|c| c == '[' || c == ']');
                        return trimmed.to_string();
                    }
                }
            }
        }
    }
    "256".to_string()
}

/// Map Almide types to WGSL types.
/// `Ty::Named` arm of [`emit_type`]: GPU vector/matrix type names map to
/// WGSL builtins, everything else passes through as-is.
fn emit_type_named(name: &str) -> String {
    match name {
        "Vec2" => "vec2<f32>".to_string(),
        "Vec3" => "vec3<f32>".to_string(),
        "Vec4" => "vec4<f32>".to_string(),
        "Mat3" => "mat3x3<f32>".to_string(),
        "Mat4" => "mat4x4<f32>".to_string(),
        "UInt32" => "u32".to_string(),
        other => other.to_string(),
    }
}

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
        Ty::Named(name, _args) => emit_type_named(name.as_str()),
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
/// WGSL infix operator spelling for `op`. `BinOp { op, left, right }` arm
/// of [`emit_expr_inline`].
fn binop_wgsl_str(op: BinOp) -> &'static str {
    match op {
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
    }
}

/// `CallTarget::Module` arm of [`emit_expr_inline_call`]: map lumen module
/// calls (`v2.new`/`v3.new`/`v4.new` and friends) to WGSL vector
/// constructors, extracted verbatim.
fn emit_expr_inline_call_module(m: &str, f: &str, arg_strs: &[String]) -> String {
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

/// `Call { target, args, .. }` arm of [`emit_expr_inline`].
fn emit_expr_inline_call(target: &CallTarget, args: &[IrExpr], vt: &VarTable) -> String {
    match target {
        CallTarget::Named { name } => {
            let arg_strs: Vec<String> = args.iter().map(|a| emit_expr_inline(a, vt)).collect();
            // Map GPU type constructors to WGSL builtins
            let wgsl_name = match name.as_str() {
                "Vec2" => "vec2<f32>",
                "Vec3" => "vec3<f32>",
                "Vec4" => "vec4<f32>",
                "Mat3" => "mat3x3<f32>",
                "Mat4" => "mat4x4<f32>",
                other => other,
            };
            format!("{}({})", wgsl_name, arg_strs.join(", "))
        }
        CallTarget::Module { module, func, .. } => {
            let arg_strs: Vec<String> = args.iter().map(|a| emit_expr_inline(a, vt)).collect();
            emit_expr_inline_call_module(module.as_str(), func.as_str(), &arg_strs)
        }
        _ => "/* unknown call */".to_string(),
    }
}

/// `emit_expr_inline` group: literals, variables, and operators. Not
/// exhaustive — the caller falls through to `emit_expr_inline_structural`
/// on `None`, so a `_ => None` here just means "not this group's variant"
/// (cog>25 decomposition).
fn emit_expr_inline_scalar(expr: &IrExpr, vt: &VarTable) -> Option<String> {
    match &expr.kind {
        IrExprKind::LitInt { value } => Some(format!("{}", value)),
        IrExprKind::LitFloat { value } => Some(format_float(*value)),
        IrExprKind::LitBool { value } => Some(format!("{}", value)),

        IrExprKind::Var { id } => Some(vt.get(*id).name.as_str().to_string()),

        IrExprKind::BinOp { op, left, right } => {
            let l = emit_expr_inline(left, vt);
            let r = emit_expr_inline(right, vt);
            Some(format!("({} {} {})", l, binop_wgsl_str(*op), r))
        }

        IrExprKind::UnOp { op, operand } => {
            let e = emit_expr_inline(operand, vt);
            Some(match op {
                UnOp::NegInt | UnOp::NegFloat => format!("(-{})", e),
                UnOp::Not => format!("(!{})", e),
            })
        }

        _ => None,
    }
}

/// `emit_expr_inline` group: structural nodes (member/index access, record
/// and list literals, calls, control flow). See `emit_expr_inline_scalar`.
fn emit_expr_inline_structural(expr: &IrExpr, vt: &VarTable) -> Option<String> {
    match &expr.kind {
        IrExprKind::Member { object, field } => {
            Some(format!("{}.{}", emit_expr_inline(object, vt), field.as_str()))
        }

        IrExprKind::IndexAccess { object, index } => {
            Some(format!("{}[{}]", emit_expr_inline(object, vt), emit_expr_inline(index, vt)))
        }

        IrExprKind::Record { name, fields } => {
            let name_str = name.map(|n| n.as_str().to_string()).unwrap_or_default();
            let field_strs: Vec<String> = fields
                .iter()
                .map(|(k, v)| format!("{}: {}", k.as_str(), emit_expr_inline(v, vt)))
                .collect();
            Some(format!("{} {{ {} }}", name_str, field_strs.join(", ")))
        }

        IrExprKind::List { elements } => {
            // Fixed-size array literal
            if elements.is_empty() {
                return Some("array()".to_string());
            }
            let elem_ty = emit_type(&elements[0].ty);
            let elems: Vec<String> = elements.iter().map(|e| emit_expr_inline(e, vt)).collect();
            Some(format!(
                "array<{}, {}>({})",
                elem_ty,
                elements.len(),
                elems.join(", ")
            ))
        }

        IrExprKind::Call { target, args, .. } => Some(emit_expr_inline_call(target, args, vt)),

        IrExprKind::If { cond, then, else_ } => {
            Some(format!(
                "select({}, {}, {})",
                emit_expr_inline(else_, vt),
                emit_expr_inline(then, vt),
                emit_expr_inline(cond, vt),
            ))
        }

        IrExprKind::Block { stmts: _, expr: Some(tail) } => {
            Some(emit_expr_inline(tail, vt))
        }

        _ => None,
    }
}

fn emit_expr_inline(expr: &IrExpr, vt: &VarTable) -> String {
    emit_expr_inline_scalar(expr, vt)
        .or_else(|| emit_expr_inline_structural(expr, vt))
        .unwrap_or_else(|| format!("/* unsupported expr */"))
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
        IrStmtKind::IndexAssign { target, index, value } => {
            let name = vt.get(*target).name.as_str();
            format!(
                "{}{}[{}] = {};\n",
                pad, name, emit_expr_inline(index, vt), emit_expr_inline(value, vt)
            )
        }
        IrStmtKind::FieldAssign { target, field, value } => {
            let name = vt.get(*target).name.as_str();
            format!(
                "{}{}.{} = {};\n",
                pad, name, field.as_str(), emit_expr_inline(value, vt)
            )
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
