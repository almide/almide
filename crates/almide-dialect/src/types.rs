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
/// Map a checker `Ty` onto the dialect's type lattice.
///
/// Split into a flat SCALAR table and a structural recursion, so the 27-arm match
/// becomes two short ones. `None` from the scalar table means "not a scalar", and
/// the trailing `Unknown` fallback is unchanged.
pub fn from_ty(ty: &almide_lang::types::Ty) -> DialectType {
    from_ty_scalar(ty)
        .or_else(|| from_ty_structural(ty))
        .unwrap_or(DialectType::Unknown)
}

/// The nullary types: canonical scalars plus the sized numerics.
fn from_ty_scalar(ty: &almide_lang::types::Ty) -> Option<DialectType> {
    use almide_lang::types::Ty;
    Some(match ty {
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
        _ => return None,
    })
}

/// The forms that recurse into child types. A missing type argument maps to
/// `Unknown` rather than panicking, exactly as before.
fn from_ty_structural(ty: &almide_lang::types::Ty) -> Option<DialectType> {
    use almide_lang::types::{Ty, TypeConstructorId as TCI};
    let arg = |args: &[Ty], i: usize| {
        args.get(i).map(from_ty).unwrap_or(DialectType::Unknown)
    };
    Some(match ty {
        Ty::Applied(TCI::List, args) => DialectType::List(Box::new(arg(args, 0))),
        Ty::Applied(TCI::Option, args) => DialectType::Option(Box::new(arg(args, 0))),
        Ty::Applied(TCI::Map, args) => {
            DialectType::Map(Box::new(arg(args, 0)), Box::new(arg(args, 1)))
        }
        Ty::Applied(TCI::Result, args) => {
            DialectType::Result(Box::new(arg(args, 0)), Box::new(arg(args, 1)))
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
        _ => return None,
    })
}


/// The printed name of a NULLARY dialect type, for either surface.
///
/// `dump.rs` (and the since-retired `emit_rust.rs`, #930) formatted the same 17 scalar variants in
/// their own 28-arm match; the tables differ only in the spelling of a handful of
/// entries, so they share one function and keep only their structural recursion.
/// A non-scalar returns `None`.
pub fn scalar_name(ty: &DialectType, rust: bool) -> Option<&'static str> {
    if let Some(name) = uniform_scalar_name(ty) {
        return Some(name);
    }
    let (rust_name, almide_name) = dual_scalar_names(ty)?;
    Some(if rust { rust_name } else { almide_name })
}

/// The scalars whose Rust and Almide spellings are identical.
fn uniform_scalar_name(ty: &DialectType) -> Option<&'static str> {
    Some(match ty {
        DialectType::I64 => "i64",
        DialectType::F64 => "f64",
        DialectType::Bool => "bool",
        DialectType::I8 => "i8",
        DialectType::I16 => "i16",
        DialectType::I32 => "i32",
        DialectType::U8 => "u8",
        DialectType::U16 => "u16",
        DialectType::U32 => "u32",
        DialectType::U64 => "u64",
        DialectType::F32 => "f32",
        _ => return None,
    })
}

/// The scalars spelled differently on the two surfaces, as `(rust, almide)`.
fn dual_scalar_names(ty: &DialectType) -> Option<(&'static str, &'static str)> {
    Some(match ty {
        DialectType::Unit => ("()", "unit"),
        DialectType::String => ("String", "string"),
        DialectType::Bytes => ("Vec<u8>", "bytes"),
        DialectType::Matrix => ("Matrix", "matrix"),
        DialectType::RawPtr => ("*mut u8", "rawptr"),
        DialectType::Unknown => ("()", "unknown"),
        _ => return None,
    })
}
