//! Almide dialect type system.
//!
//! Maps almide-lang's `Ty` to MLIR-style dialect types.
//! These are the types that flow through SSA values in the dialect.

use almide_base::intern::Sym;

/// Dialect-level types. Correspond to MLIR's `!almide.*` types.
///
/// Unlike `almide_lang::types::Ty` (which carries inference artifacts
/// like TypeVar), these are fully resolved and target-independent.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DialectType {
    // ── Scalars ──
    I64,
    F64,
    Bool,
    Unit,
    String,
    Bytes,

    // ── Sized integers ──
    I8, I16, I32,
    U8, U16, U32, U64,
    F32,

    // ── Containers ──
    List(Box<DialectType>),
    Map(Box<DialectType>, Box<DialectType>),
    Option(Box<DialectType>),
    Result(Box<DialectType>, Box<DialectType>),
    Tuple(Vec<DialectType>),

    // ── User types ──
    Named(Sym),
    Record(Vec<(Sym, DialectType)>),

    // ── Functions ──
    Fn {
        params: Vec<DialectType>,
        ret: Box<DialectType>,
    },
    Closure {
        params: Vec<DialectType>,
        ret: Box<DialectType>,
    },

    // ── Special ──
    Matrix,
    RawPtr,
    /// Unresolved — should not survive verification.
    Unknown,
}

/// Convert from almide-lang Ty to dialect type.
pub fn from_ty(ty: &almide_lang::types::Ty) -> DialectType {
    use almide_lang::types::{Ty, TypeConstructorId as TCI};

    match ty {
        Ty::Int => DialectType::I64,
        Ty::Float => DialectType::F64,
        Ty::Bool => DialectType::Bool,
        Ty::Unit => DialectType::Unit,
        Ty::String => DialectType::String,
        Ty::Bytes => DialectType::Bytes,
        Ty::Matrix => DialectType::Matrix,
        Ty::RawPtr => DialectType::RawPtr,

        Ty::Int8 => DialectType::I8,
        Ty::Int16 => DialectType::I16,
        Ty::Int32 => DialectType::I32,
        Ty::UInt8 => DialectType::U8,
        Ty::UInt16 => DialectType::U16,
        Ty::UInt32 => DialectType::U32,
        Ty::UInt64 => DialectType::U64,
        Ty::Float32 => DialectType::F32,

        Ty::Applied(TCI::List, args) => {
            let inner = args.first().map(from_ty).unwrap_or(DialectType::Unknown);
            DialectType::List(Box::new(inner))
        }
        Ty::Applied(TCI::Map, args) => {
            let k = args.first().map(from_ty).unwrap_or(DialectType::Unknown);
            let v = args.get(1).map(from_ty).unwrap_or(DialectType::Unknown);
            DialectType::Map(Box::new(k), Box::new(v))
        }
        Ty::Applied(TCI::Option, args) => {
            let inner = args.first().map(from_ty).unwrap_or(DialectType::Unknown);
            DialectType::Option(Box::new(inner))
        }
        Ty::Applied(TCI::Result, args) => {
            let ok = args.first().map(from_ty).unwrap_or(DialectType::Unknown);
            let err = args.get(1).map(from_ty).unwrap_or(DialectType::Unknown);
            DialectType::Result(Box::new(ok), Box::new(err))
        }

        Ty::Tuple(elems) => DialectType::Tuple(elems.iter().map(from_ty).collect()),

        Ty::Named(name, _) => DialectType::Named(*name),

        Ty::Fn { params, ret, .. } => DialectType::Fn {
            params: params.iter().map(from_ty).collect(),
            ret: Box::new(from_ty(ret)),
        },

        Ty::Record { fields, .. } => {
            DialectType::Record(fields.iter().map(|(n, t)| (*n, from_ty(t))).collect())
        }

        _ => DialectType::Unknown,
    }
}
