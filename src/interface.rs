//! Module Interface extraction: IR → JSON description of the public API.
//!
//! Used by external binding generators (almide-export) to produce
//! language-specific packages (pip, npm, gem, etc.) without needing
//! to parse Almide source or understand the IR format.

use serde::Serialize;
use crate::ir::*;
use crate::types::{Ty, VariantPayload};
use crate::types::constructor::TypeConstructorId;

// ── Interface types ──

#[derive(Debug, Serialize)]
pub struct ModuleInterface {
    pub module: String,
    pub types: Vec<TypeExport>,
    pub functions: Vec<FunctionExport>,
    pub constants: Vec<ConstantExport>,
}

#[derive(Debug, Serialize)]
pub struct TypeExport {
    pub name: String,
    pub kind: TypeKindExport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generics: Option<Vec<String>>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_default: Option<bool>,
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
}

/// Language-agnostic type reference for the interface.
#[derive(Debug, Serialize)]
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
    Named { name: std::string::String, #[serde(skip_serializing_if = "Vec::is_empty")] args: Vec<TypeRef> },
    Fn { params: Vec<TypeRef>, #[serde(rename = "return")] ret: Box<TypeRef> },
    TypeVar { name: std::string::String },
    Unknown,
}

// ── Extraction ──

/// Extract the public module interface from a type-checked IR program.
pub fn extract(program: &IrProgram, module_name: &str) -> ModuleInterface {
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
                    ty: ty_to_ref(&f.ty),
                    has_default: if f.default.is_some() { Some(true) } else { None },
                }).collect(),
            },
            IrTypeDeclKind::Variant { cases, .. } => TypeKindExport::Variant {
                cases: cases.iter().map(|c| CaseExport {
                    name: c.name.to_string(),
                    payload: match &c.kind {
                        IrVariantKind::Unit => None,
                        IrVariantKind::Tuple { fields } => Some(CasePayload::Tuple {
                            fields: fields.iter().map(ty_to_ref).collect(),
                        }),
                        IrVariantKind::Record { fields } => Some(CasePayload::Record {
                            fields: fields.iter().map(|f| FieldExport {
                                name: f.name.to_string(),
                                ty: ty_to_ref(&f.ty),
                                has_default: if f.default.is_some() { Some(true) } else { None },
                            }).collect(),
                        }),
                    },
                }).collect(),
            },
            IrTypeDeclKind::Alias { target } => TypeKindExport::Alias {
                target: ty_to_ref(target),
            },
        };
        types.push(TypeExport { name: td.name.to_string(), kind, generics });
    }

    // Functions
    for func in &program.functions {
        if func.is_test { continue; }
        if !matches!(func.visibility, IrVisibility::Public) { continue; }
        let generics = func.generics.as_ref()
            .filter(|g| !g.is_empty())
            .map(|g| g.iter().map(|p| p.name.to_string()).collect());

        // For effect fns: return type is the success type, error is String
        let (ret, error) = if func.is_effect {
            match &func.ret_ty {
                Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => {
                    (ty_to_ref(&args[0]), Some(ty_to_ref(&args[1])))
                }
                other => (ty_to_ref(other), Some(TypeRef::String)),
            }
        } else {
            (ty_to_ref(&func.ret_ty), None)
        };

        functions.push(FunctionExport {
            name: func.name.to_string(),
            params: func.params.iter().map(|p| ParamExport {
                name: p.name.to_string(),
                ty: ty_to_ref(&p.ty),
            }).collect(),
            ret,
            effect: func.is_effect,
            error,
            generics,
        });
    }

    // Top-level constants
    for tl in &program.top_lets {
        let name = program.var_table.get(tl.var).name.to_string();
        constants.push(ConstantExport {
            name,
            ty: ty_to_ref(&tl.ty),
        });
    }

    ModuleInterface { module: module_name.to_string(), types, functions, constants }
}

/// Convert a Ty to a serializable TypeRef.
fn ty_to_ref(ty: &Ty) -> TypeRef {
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
                TypeRef::List { inner: Box::new(ty_to_ref(&args[0])) },
            TypeConstructorId::Option if args.len() == 1 =>
                TypeRef::Option { inner: Box::new(ty_to_ref(&args[0])) },
            TypeConstructorId::Result if args.len() == 2 =>
                TypeRef::Result { ok: Box::new(ty_to_ref(&args[0])), err: Box::new(ty_to_ref(&args[1])) },
            TypeConstructorId::Map if args.len() == 2 =>
                TypeRef::Map { key: Box::new(ty_to_ref(&args[0])), value: Box::new(ty_to_ref(&args[1])) },
            TypeConstructorId::Set if args.len() == 1 =>
                TypeRef::Set { inner: Box::new(ty_to_ref(&args[0])) },
            TypeConstructorId::Tuple =>
                TypeRef::Tuple { elements: args.iter().map(ty_to_ref).collect() },
            TypeConstructorId::UserDefined(name) =>
                TypeRef::Named { name: name.clone(), args: args.iter().map(ty_to_ref).collect() },
            _ => TypeRef::Unknown,
        },
        Ty::Tuple(elements) => TypeRef::Tuple {
            elements: elements.iter().map(ty_to_ref).collect(),
        },
        Ty::Named(name, args) => TypeRef::Named {
            name: name.to_string(),
            args: args.iter().map(ty_to_ref).collect(),
        },
        Ty::Fn { params, ret } => TypeRef::Fn {
            params: params.iter().map(ty_to_ref).collect(),
            ret: Box::new(ty_to_ref(ret)),
        },
        Ty::TypeVar(name) => TypeRef::TypeVar { name: name.to_string() },
        Ty::Record { fields } => {
            // Anonymous record — serialize as a named type with fields
            TypeRef::Named {
                name: format!("{{{}}}", fields.iter().map(|(n, _)| n.to_string()).collect::<Vec<_>>().join(", ")),
                args: vec![],
            }
        }
        _ => TypeRef::Unknown,
    }
}
