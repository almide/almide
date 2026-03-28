//! Module Interface extraction: IR → JSON description of the public API.
//!
//! Used by external binding generators (almide-export) to produce
//! language-specific packages (pip, npm, gem, etc.) without needing
//! to parse Almide source or understand the IR format.

use std::collections::HashMap;
use serde::Serialize;
use crate::ir::*;
use crate::types::{Ty, VariantPayload};
use crate::types::constructor::TypeConstructorId;

// ── Interface types ──

#[derive(Debug, Serialize)]
pub struct ModuleInterface {
    pub module: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub types: Vec<TypeExport>,
    pub functions: Vec<FunctionExport>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub constants: Vec<ConstantExport>,
}

#[derive(Debug, Serialize)]
pub struct TypeExport {
    pub name: String,
    pub kind: TypeKindExport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generics: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TypeKindExport {
    Record { fields: Vec<FieldExport> },
    Variant { cases: Vec<CaseExport> },
    Alias { target: TypeRef },
}

#[derive(Debug, Serialize)]
pub struct FieldExport {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: TypeRef,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub has_default: bool,
}

#[derive(Debug, Serialize)]
pub struct CaseExport {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<CasePayload>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CasePayload {
    Tuple { fields: Vec<TypeRef> },
    Record { fields: Vec<FieldExport> },
}

#[derive(Debug, Serialize)]
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
}

#[derive(Debug, Serialize)]
pub struct ParamExport {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: TypeRef,
}

#[derive(Debug, Serialize)]
pub struct ConstantExport {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: TypeRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

/// Language-agnostic type reference for the interface.
#[derive(Debug, Clone, Serialize)]
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
/// `source` is the original source text (for doc comment extraction).
pub fn extract(program: &IrProgram, module_name: &str, source: Option<&str>) -> ModuleInterface {
    // Build record field→name lookup for anonymous record resolution.
    // Key: sorted field names, Value: type decl name
    let record_names = build_record_lookup(program);
    // Build variant case→name lookup for Ty::Variant resolution
    let variant_names = build_variant_lookup(program);

    // Extract doc comments from source if available
    let docs = source.map(|s| extract_docs(s)).unwrap_or_default();

    let mut types = Vec::new();
    let mut functions = Vec::new();
    let mut constants = Vec::new();

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
        let doc = docs.get(&td.name.to_string()).cloned();
        types.push(TypeExport { name: td.name.to_string(), kind, generics, doc });
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

        let doc = docs.get(&func.name.to_string()).cloned();
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
            doc,
        });
    }

    // Top-level constants
    for tl in &program.top_lets {
        let name = program.var_table.get(tl.var).name.to_string();
        constants.push(ConstantExport {
            name,
            ty: resolve_ty(&tl.ty, &record_names, &variant_names),
            doc: None,
        });
    }

    ModuleInterface {
        module: module_name.to_string(),
        version: None,
        types,
        functions,
        constants,
    }
}

// ── Type resolution ──

type RecordLookup = HashMap<Vec<std::string::String>, std::string::String>;
type VariantLookup = HashMap<Vec<std::string::String>, std::string::String>;

/// Build a lookup table: sorted field names → record type name
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

/// Build a lookup table: sorted case names → variant type name
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

/// Convert Ty to TypeRef, resolving anonymous records/variants to named types.
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
        // Anonymous record → resolve to named type if field signature matches
        Ty::Record { fields } => {
            let mut field_names: Vec<std::string::String> = fields.iter().map(|(n, _)| n.to_string()).collect();
            field_names.sort();
            if let Some(name) = records.get(&field_names) {
                TypeRef::Named { name: name.clone(), args: vec![] }
            } else {
                // No matching named type — emit as inline record
                TypeRef::Named {
                    name: format!("{{{}}}", fields.iter().map(|(n, _)| n.to_string()).collect::<Vec<_>>().join(", ")),
                    args: vec![],
                }
            }
        }
        // Anonymous variant → resolve to named type if case signature matches
        Ty::Variant { cases, .. } => {
            let mut case_names: Vec<std::string::String> = cases.iter().map(|c| c.name.to_string()).collect();
            case_names.sort();
            if let Some(name) = variants.get(&case_names) {
                TypeRef::Named { name: name.clone(), args: vec![] }
            } else {
                TypeRef::Unknown
            }
        }
        _ => TypeRef::Unknown,
    }
}

// ── Doc comment extraction ──

/// Extract doc comments from source text.
/// Returns a map of name → doc string.
/// Scans for `// comment` lines immediately preceding `type`, `fn`, `effect fn` declarations.
fn extract_docs(source: &str) -> HashMap<std::string::String, std::string::String> {
    let mut docs = HashMap::new();
    let lines: Vec<&str> = source.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        // Find declarations
        let name = if let Some(rest) = trimmed.strip_prefix("fn ") {
            extract_decl_name(rest)
        } else if let Some(rest) = trimmed.strip_prefix("effect fn ") {
            extract_decl_name(rest)
        } else if let Some(rest) = trimmed.strip_prefix("type ") {
            extract_decl_name(rest)
        } else {
            None
        };

        if let Some(name) = name {
            // Collect preceding comment lines
            let mut doc_lines = Vec::new();
            let mut j = i;
            while j > 0 {
                j -= 1;
                let prev = lines[j].trim();
                if let Some(comment) = prev.strip_prefix("// ") {
                    doc_lines.push(comment.to_string());
                } else if let Some(comment) = prev.strip_prefix("//") {
                    doc_lines.push(comment.to_string());
                } else {
                    break;
                }
            }
            if !doc_lines.is_empty() {
                doc_lines.reverse();
                docs.insert(name, doc_lines.join("\n"));
            }
        }
    }
    docs
}

/// Extract the name from a declaration line (after `fn `, `type `, etc.)
fn extract_decl_name(rest: &str) -> Option<std::string::String> {
    let rest = rest.trim();
    // Take chars until non-identifier
    let name: std::string::String = rest.chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() { None } else { Some(name) }
}
