//! Module Interface extraction: IR → JSON description of the public API.
//!
//! Used by external binding generators (almide-export) to produce
//! language-specific packages (pip, npm, gem, etc.) without needing
//! to parse Almide source or understand the IR format.

use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use almide_ir::*;
use almide_lang::types::Ty;
use almide_lang::types::constructor::TypeConstructorId;

// ── Interface types ──

#[derive(Debug, Serialize, Deserialize)]
pub struct ModuleInterface {
    pub module: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub types: Vec<TypeExport>,
    pub functions: Vec<FunctionExport>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub constants: Vec<ConstantExport>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<DependencyExport>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TypeExport {
    pub name: String,
    pub kind: TypeKindExport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generics: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub abi: Option<AbiLayout>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<String>,
}

/// C ABI layout information for a type (size, alignment, field offsets).
#[derive(Debug, Serialize, Deserialize)]
pub struct AbiLayout {
    pub size: usize,
    pub align: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<AbiField>,
}

/// C ABI field layout within a struct.
#[derive(Debug, Serialize, Deserialize)]
pub struct AbiField {
    pub name: String,
    pub offset: usize,
    pub size: usize,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TypeKindExport {
    Record { fields: Vec<FieldExport> },
    Variant { cases: Vec<CaseExport> },
    Alias { target: TypeRef },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FieldExport {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: TypeRef,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub has_default: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CaseExport {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<CasePayload>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CasePayload {
    Tuple { fields: Vec<TypeRef> },
    Record { fields: Vec<FieldExport> },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionExport {
    pub name: String,
    pub params: Vec<ParamExport>,
    #[serde(rename = "return")]
    pub ret: TypeRef,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub effect: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<TypeRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generics: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ParamExport {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: TypeRef,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConstantExport {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: TypeRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<ConstValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

/// Serializable constant value.
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConstValue {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
}

/// Dependency on another module (stdlib or user module).
#[derive(Debug, Serialize, Deserialize)]
pub struct DependencyExport {
    pub module: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub stdlib: bool,
}

/// Language-agnostic type reference for the interface.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TypeRef {
    Int,
    Float,
    String,
    Bool,
    Unit,
    Bytes,
    Matrix,
    List { inner: Box<TypeRef> },
    Option { inner: Box<TypeRef> },
    Result { ok: Box<TypeRef>, err: Box<TypeRef> },
    Map { key: Box<TypeRef>, value: Box<TypeRef> },
    Set { inner: Box<TypeRef> },
    Tuple { elements: Vec<TypeRef> },
    Named {
        name: std::string::String,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        args: Vec<TypeRef>,
    },
    Fn {
        params: Vec<TypeRef>,
        #[serde(rename = "return")]
        ret: Box<TypeRef>,
    },
    TypeVar { name: std::string::String },
    Unknown,
}

// ── Extraction ──

/// Extract the public module interface from a type-checked IR program.
/// `source` is the original source text (for doc/example/deprecation extraction).
pub fn extract(program: &IrProgram, module_name: &str, source: Option<&str>) -> ModuleInterface {
    let record_names = build_record_lookup(program);
    let variant_names = build_variant_lookup(program);
    let doc_info = source.map(|s| extract_docs(s)).unwrap_or_default();

    let mut types = Vec::new();
    let mut functions = Vec::new();
    let mut constants = Vec::new();
    let mut dependencies = Vec::new();

    // Types
    for td in &program.type_decls {
        if !matches!(td.visibility, IrVisibility::Public) { continue; }
        let generics = td.generics.as_ref()
            .filter(|g| !g.is_empty())
            .map(|g| g.iter().map(|p| p.name.to_string()).collect());
        let kind = match &td.kind {
            IrTypeDeclKind::Record { fields } => TypeKindExport::Record {
                fields: fields.iter().map(|f| FieldExport {
                    name: f.name.to_string(),
                    ty: resolve_ty(&f.ty, &record_names, &variant_names),
                    has_default: f.default.is_some(),
                }).collect(),
            },
            IrTypeDeclKind::Variant { cases, .. } => TypeKindExport::Variant {
                cases: cases.iter().map(|c| CaseExport {
                    name: c.name.to_string(),
                    payload: match &c.kind {
                        IrVariantKind::Unit => None,
                        IrVariantKind::Tuple { fields } => Some(CasePayload::Tuple {
                            fields: fields.iter().map(|t| resolve_ty(t, &record_names, &variant_names)).collect(),
                        }),
                        IrVariantKind::Record { fields } => Some(CasePayload::Record {
                            fields: fields.iter().map(|f| FieldExport {
                                name: f.name.to_string(),
                                ty: resolve_ty(&f.ty, &record_names, &variant_names),
                                has_default: f.default.is_some(),
                            }).collect(),
                        }),
                    },
                }).collect(),
            },
            IrTypeDeclKind::Alias { target } => TypeKindExport::Alias {
                target: resolve_ty(target, &record_names, &variant_names),
            },
        };
        let abi = compute_abi(td);
        let info = doc_info.get(&td.name.to_string());
        types.push(TypeExport {
            name: td.name.to_string(),
            kind,
            generics,
            abi,
            doc: info.and_then(|i| i.doc.clone()),
            deprecated: info.and_then(|i| i.deprecated.clone()),
        });
    }

    // Functions
    for func in &program.functions {
        if func.is_test { continue; }
        if !matches!(func.visibility, IrVisibility::Public) { continue; }
        let generics = func.generics.as_ref()
            .filter(|g| !g.is_empty())
            .map(|g| g.iter().map(|p| p.name.to_string()).collect());

        let (ret, error) = if func.is_effect {
            match &func.ret_ty {
                Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => {
                    (resolve_ty(&args[0], &record_names, &variant_names),
                     Some(resolve_ty(&args[1], &record_names, &variant_names)))
                }
                other => (resolve_ty(other, &record_names, &variant_names), Some(TypeRef::String)),
            }
        } else {
            (resolve_ty(&func.ret_ty, &record_names, &variant_names), None)
        };

        let info = doc_info.get(&func.name.to_string());
        functions.push(FunctionExport {
            name: func.name.to_string(),
            params: func.params.iter().map(|p| ParamExport {
                name: p.name.to_string(),
                ty: resolve_ty(&p.ty, &record_names, &variant_names),
            }).collect(),
            ret,
            effect: func.is_effect,
            error,
            generics,
            doc: info.and_then(|i| i.doc.clone()),
            examples: info.map(|i| i.examples.clone()).unwrap_or_default(),
            deprecated: info.and_then(|i| i.deprecated.clone()),
        });
    }

    // Top-level constants (with values for literals)
    for tl in &program.top_lets {
        let name = program.var_table.get(tl.var).name.to_string();
        let value = extract_const_value(&tl.value);
        constants.push(ConstantExport {
            name: name.clone(),
            ty: resolve_ty(&tl.ty, &record_names, &variant_names),
            value,
            doc: doc_info.get(&name).and_then(|i| i.doc.clone()),
        });
    }

    // Dependencies (imported modules)
    for m in &program.modules {
        let name = m.name.to_string();
        let is_stdlib = almide_lang::stdlib_info::is_stdlib_module(&name);
        dependencies.push(DependencyExport {
            module: name,
            stdlib: is_stdlib,
        });
    }

    ModuleInterface {
        module: module_name.to_string(),
        version: None,
        types,
        functions,
        constants,
        dependencies,
    }
}

// ── Constant value extraction ──

fn extract_const_value(expr: &IrExpr) -> Option<ConstValue> {
    match &expr.kind {
        IrExprKind::LitInt { value } => Some(ConstValue::Int(*value)),
        IrExprKind::LitFloat { value } => Some(ConstValue::Float(*value)),
        IrExprKind::LitStr { value } => Some(ConstValue::String(value.clone())),
        IrExprKind::LitBool { value } => Some(ConstValue::Bool(*value)),
        _ => None,
    }
}

// ── Type resolution ──

type RecordLookup = HashMap<Vec<std::string::String>, std::string::String>;
type VariantLookup = HashMap<Vec<std::string::String>, std::string::String>;

fn build_record_lookup(program: &IrProgram) -> RecordLookup {
    let mut map = HashMap::new();
    for td in &program.type_decls {
        if let IrTypeDeclKind::Record { fields } = &td.kind {
            let mut names: Vec<std::string::String> = fields.iter().map(|f| f.name.to_string()).collect();
            names.sort();
            map.insert(names, td.name.to_string());
        }
    }
    map
}

fn build_variant_lookup(program: &IrProgram) -> VariantLookup {
    let mut map = HashMap::new();
    for td in &program.type_decls {
        if let IrTypeDeclKind::Variant { cases, .. } = &td.kind {
            let mut names: Vec<std::string::String> = cases.iter().map(|c| c.name.to_string()).collect();
            names.sort();
            map.insert(names, td.name.to_string());
        }
    }
    map
}

fn resolve_ty(ty: &Ty, records: &RecordLookup, variants: &VariantLookup) -> TypeRef {
    match ty {
        Ty::Int => TypeRef::Int,
        Ty::Float => TypeRef::Float,
        Ty::String => TypeRef::String,
        Ty::Bool => TypeRef::Bool,
        Ty::Unit => TypeRef::Unit,
        Ty::Bytes => TypeRef::Bytes,
        Ty::Matrix => TypeRef::Matrix,
        Ty::Applied(id, args) => match id {
            TypeConstructorId::List if args.len() == 1 =>
                TypeRef::List { inner: Box::new(resolve_ty(&args[0], records, variants)) },
            TypeConstructorId::Option if args.len() == 1 =>
                TypeRef::Option { inner: Box::new(resolve_ty(&args[0], records, variants)) },
            TypeConstructorId::Result if args.len() == 2 =>
                TypeRef::Result {
                    ok: Box::new(resolve_ty(&args[0], records, variants)),
                    err: Box::new(resolve_ty(&args[1], records, variants)),
                },
            TypeConstructorId::Map if args.len() == 2 =>
                TypeRef::Map {
                    key: Box::new(resolve_ty(&args[0], records, variants)),
                    value: Box::new(resolve_ty(&args[1], records, variants)),
                },
            TypeConstructorId::Set if args.len() == 1 =>
                TypeRef::Set { inner: Box::new(resolve_ty(&args[0], records, variants)) },
            TypeConstructorId::Tuple =>
                TypeRef::Tuple { elements: args.iter().map(|t| resolve_ty(t, records, variants)).collect() },
            TypeConstructorId::UserDefined(name) =>
                TypeRef::Named {
                    name: name.clone(),
                    args: args.iter().map(|t| resolve_ty(t, records, variants)).collect(),
                },
            _ => TypeRef::Unknown,
        },
        Ty::Tuple(elements) => TypeRef::Tuple {
            elements: elements.iter().map(|t| resolve_ty(t, records, variants)).collect(),
        },
        Ty::Named(name, args) => TypeRef::Named {
            name: name.to_string(),
            args: args.iter().map(|t| resolve_ty(t, records, variants)).collect(),
        },
        Ty::Fn { params, ret } => TypeRef::Fn {
            params: params.iter().map(|t| resolve_ty(t, records, variants)).collect(),
            ret: Box::new(resolve_ty(ret, records, variants)),
        },
        Ty::TypeVar(name) => TypeRef::TypeVar { name: name.to_string() },
        Ty::Record { fields } => {
            let mut field_names: Vec<std::string::String> = fields.iter().map(|(n, _)| n.to_string()).collect();
            field_names.sort();
            if let Some(name) = records.get(&field_names) {
                TypeRef::Named { name: name.clone(), args: vec![] }
            } else {
                TypeRef::Named {
                    name: format!("{{{}}}", fields.iter().map(|(n, _)| n.to_string()).collect::<Vec<_>>().join(", ")),
                    args: vec![],
                }
            }
        }
        Ty::Variant { cases, .. } => {
            let mut case_names: Vec<std::string::String> = cases.iter().map(|c| c.name.to_string()).collect();
            case_names.sort();
            if let Some(name) = variants.get(&case_names) {
                TypeRef::Named { name: name.clone(), args: vec![] }
            } else {
                TypeRef::Unknown
            }
        }
        // OpenRecord: same resolution as Record (match fields against known type decls)
        Ty::OpenRecord { fields } => {
            let mut field_names: Vec<std::string::String> = fields.iter().map(|(n, _)| n.to_string()).collect();
            field_names.sort();
            if let Some(name) = records.get(&field_names) {
                TypeRef::Named { name: name.clone(), args: vec![] }
            } else {
                TypeRef::Named {
                    name: format!("{{{}}}", fields.iter().map(|(n, _)| n.to_string()).collect::<Vec<_>>().join(", ")),
                    args: vec![],
                }
            }
        }
        // Union: serialize each member
        Ty::Union(members) => {
            if members.len() == 1 {
                resolve_ty(&members[0], records, variants)
            } else {
                // Represent as a tuple of alternatives (binding generators decide how to render)
                TypeRef::Tuple {
                    elements: members.iter().map(|t| resolve_ty(t, records, variants)).collect(),
                }
            }
        }
        _ => TypeRef::Unknown,
    }
}

// ── ABI layout computation ──

/// Compute C ABI layout for a type, if it has a fixed layout (no generics, no opaque types).
fn compute_abi(td: &IrTypeDecl) -> Option<AbiLayout> {
    // Skip generic types — layout depends on type parameters
    if td.generics.as_ref().map_or(false, |g| !g.is_empty()) {
        return None;
    }
    match &td.kind {
        IrTypeDeclKind::Record { fields } => {
            let mut abi_fields = Vec::new();
            let mut offset = 0usize;
            let mut max_align = 1usize;
            for f in fields {
                let (size, align) = c_abi_size_align(&f.ty)?;
                // Pad to alignment
                let padding = (align - (offset % align)) % align;
                offset += padding;
                abi_fields.push(AbiField {
                    name: f.name.to_string(),
                    offset,
                    size,
                });
                offset += size;
                max_align = max_align.max(align);
            }
            // Pad struct size to alignment
            let padding = (max_align - (offset % max_align)) % max_align;
            offset += padding;
            Some(AbiLayout { size: offset, align: max_align, fields: abi_fields })
        }
        IrTypeDeclKind::Variant { cases, .. } => {
            // C repr enum: tag (i32 = 4 bytes) + max payload
            let tag_size = 4usize;
            let tag_align = 4usize;
            let mut max_payload_size = 0usize;
            let mut max_payload_align = 1usize;
            for case in cases {
                let (payload_size, payload_align) = match &case.kind {
                    IrVariantKind::Unit => (0, 1),
                    IrVariantKind::Tuple { fields } => {
                        let mut size = 0usize;
                        let mut align = 1usize;
                        for f in fields {
                            let (fs, fa) = c_abi_size_align(f)?;
                            let padding = (fa - (size % fa)) % fa;
                            size += padding + fs;
                            align = align.max(fa);
                        }
                        (size, align)
                    }
                    IrVariantKind::Record { fields } => {
                        let mut size = 0usize;
                        let mut align = 1usize;
                        for f in fields {
                            let (fs, fa) = c_abi_size_align(&f.ty)?;
                            let padding = (fa - (size % fa)) % fa;
                            size += padding + fs;
                            align = align.max(fa);
                        }
                        (size, align)
                    }
                };
                max_payload_size = max_payload_size.max(payload_size);
                max_payload_align = max_payload_align.max(payload_align);
            }
            let total_align = tag_align.max(max_payload_align);
            // tag + padding + max_payload
            let payload_offset = tag_size + (total_align - (tag_size % total_align)) % total_align;
            let raw_size = if max_payload_size == 0 { tag_size } else { payload_offset + max_payload_size };
            let total_size = raw_size + (total_align - (raw_size % total_align)) % total_align;
            Some(AbiLayout { size: total_size, align: total_align, fields: vec![] })
        }
        IrTypeDeclKind::Alias { .. } => None,
    }
}

/// Return (size, align) for a type under C ABI rules (64-bit platform).
fn c_abi_size_align(ty: &Ty) -> Option<(usize, usize)> {
    match ty {
        Ty::Bool => Some((1, 1)),
        Ty::Int => Some((8, 8)),    // i64
        Ty::Float => Some((8, 8)),  // f64
        // String, List, Map, Set, Option, Result are pointer-based (opaque)
        Ty::String => Some((16, 8)),  // (ptr, len)
        Ty::Bytes => Some((16, 8)),
        Ty::Unit => Some((0, 1)),
        // Named user types: would need full type table lookup — skip for now
        Ty::Named(_, _) => None,
        Ty::Applied(_, _) => None,
        Ty::Tuple(elems) => {
            let mut size = 0usize;
            let mut align = 1usize;
            for e in elems {
                let (es, ea) = c_abi_size_align(e)?;
                let padding = (ea - (size % ea)) % ea;
                size += padding + es;
                align = align.max(ea);
            }
            let padding = (align - (size % align)) % align;
            size += padding;
            Some((size, align))
        }
        _ => None,
    }
}

// ── Doc comment extraction ──

/// Extracted documentation info for a declaration.
#[derive(Debug, Default)]
struct DocInfo {
    doc: Option<String>,
    examples: Vec<String>,
    deprecated: Option<String>,
}

/// Extract doc comments, examples, and deprecation markers from source text.
fn extract_docs(source: &str) -> HashMap<std::string::String, DocInfo> {
    let mut result = HashMap::new();
    let lines: Vec<&str> = source.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        let name = if let Some(rest) = trimmed.strip_prefix("fn ") {
            extract_decl_name(rest)
        } else if let Some(rest) = trimmed.strip_prefix("effect fn ") {
            extract_decl_name(rest)
        } else if let Some(rest) = trimmed.strip_prefix("type ") {
            extract_decl_name(rest)
        } else if let Some(rest) = trimmed.strip_prefix("let ") {
            extract_decl_name(rest)
        } else {
            None
        };

        if let Some(name) = name {
            let mut doc_lines = Vec::new();
            let mut examples = Vec::new();
            let mut deprecated = None;
            let mut j = i;
            while j > 0 {
                j -= 1;
                let prev = lines[j].trim();
                let comment = if let Some(c) = prev.strip_prefix("// ") {
                    Some(c)
                } else if let Some(c) = prev.strip_prefix("//") {
                    Some(c)
                } else {
                    None
                };
                match comment {
                    Some(c) => {
                        if let Some(ex) = c.strip_prefix("example: ") {
                            examples.push(ex.trim().to_string());
                        } else if let Some(dep) = c.strip_prefix("deprecated: ") {
                            deprecated = Some(dep.trim().to_string());
                        } else if c.starts_with("deprecated") {
                            deprecated = Some(String::new());
                        } else {
                            doc_lines.push(c.to_string());
                        }
                    }
                    None => break,
                }
            }
            doc_lines.reverse();
            examples.reverse();
            let doc = if doc_lines.is_empty() { None } else { Some(doc_lines.join("\n")) };
            result.insert(name, DocInfo { doc, examples, deprecated });
        }
    }
    result
}

fn extract_decl_name(rest: &str) -> Option<std::string::String> {
    let rest = rest.trim();
    let name: std::string::String = rest.chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() { None } else { Some(name) }
}
