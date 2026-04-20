//! Built-in protocol registration.
//!
//! Registers the seven built-in conventions (Eq, Repr, Ord, Hash, Codec,
//! Encode, Decode) as protocol definitions in `TypeEnv.protocols`.

use almide_base::intern::sym;
use crate::types::{Ty, TypeEnv, ProtocolDef, ProtocolMethodSig};

/// Register built-in conventions as protocols.
pub fn register_builtin_protocols(env: &mut TypeEnv) {
    let self_ty = Ty::TypeVar(sym("Self"));
    let value_ty = Ty::Named(sym("Value"), vec![]);

    // Eq: fn eq(a: Self, b: Self) -> Bool
    env.protocols.insert("Eq".into(), ProtocolDef {
        name: "Eq".into(),
        generics: vec![],
        methods: vec![ProtocolMethodSig {
            name: "eq".into(),
            params: vec![("a".into(), self_ty.clone()), ("b".into(), self_ty.clone())],
            ret: Ty::Bool,
            is_effect: false,
        }],
    });

    // Repr: fn repr(v: Self) -> String
    env.protocols.insert("Repr".into(), ProtocolDef {
        name: "Repr".into(),
        generics: vec![],
        methods: vec![ProtocolMethodSig {
            name: "repr".into(),
            params: vec![("v".into(), self_ty.clone())],
            ret: Ty::String,
            is_effect: false,
        }],
    });

    // Ord: fn cmp(a: Self, b: Self) -> Int
    env.protocols.insert("Ord".into(), ProtocolDef {
        name: "Ord".into(),
        generics: vec![],
        methods: vec![ProtocolMethodSig {
            name: "cmp".into(),
            params: vec![("a".into(), self_ty.clone()), ("b".into(), self_ty.clone())],
            ret: Ty::Int,
            is_effect: false,
        }],
    });

    // Hash: fn hash(v: Self) -> Int
    env.protocols.insert("Hash".into(), ProtocolDef {
        name: "Hash".into(),
        generics: vec![],
        methods: vec![ProtocolMethodSig {
            name: "hash".into(),
            params: vec![("v".into(), self_ty.clone())],
            ret: Ty::Int,
            is_effect: false,
        }],
    });

    // Codec: fn encode(v: Self) -> Value, fn decode(v: Value) -> Result[Self, String]
    env.protocols.insert("Codec".into(), ProtocolDef {
        name: "Codec".into(),
        generics: vec![],
        methods: vec![
            ProtocolMethodSig {
                name: "encode".into(),
                params: vec![("v".into(), self_ty.clone())],
                ret: value_ty.clone(),
                is_effect: false,
            },
            ProtocolMethodSig {
                name: "decode".into(),
                params: vec![("v".into(), value_ty.clone())],
                ret: Ty::result(self_ty.clone(), Ty::String),
                is_effect: false,
            },
        ],
    });

    // Encode: fn encode(v: Self) -> Value
    env.protocols.insert("Encode".into(), ProtocolDef {
        name: "Encode".into(),
        generics: vec![],
        methods: vec![ProtocolMethodSig {
            name: "encode".into(),
            params: vec![("v".into(), self_ty.clone())],
            ret: value_ty.clone(),
            is_effect: false,
        }],
    });

    // Decode: fn decode(v: Value) -> Result[Self, String]
    env.protocols.insert("Decode".into(), ProtocolDef {
        name: "Decode".into(),
        generics: vec![],
        methods: vec![ProtocolMethodSig {
            name: "decode".into(),
            params: vec![("v".into(), value_ty.clone())],
            ret: Ty::result(self_ty.clone(), Ty::String),
            is_effect: false,
        }],
    });

    // Numeric: abstract interface for numeric primitive types. Methods
    // match the `BinOp` dispatch pairs so `fn f[T: Numeric](x: T, y: T)
    // = x + y` flows through without a separate hand-impl. Monomorph
    // repairs the `BinOp` kind once `T` resolves to a concrete width.
    env.protocols.insert("Numeric".into(), ProtocolDef {
        name: "Numeric".into(),
        generics: vec![],
        methods: vec![
            ProtocolMethodSig {
                name: "add".into(),
                params: vec![("a".into(), self_ty.clone()), ("b".into(), self_ty.clone())],
                ret: self_ty.clone(),
                is_effect: false,
            },
            ProtocolMethodSig {
                name: "sub".into(),
                params: vec![("a".into(), self_ty.clone()), ("b".into(), self_ty.clone())],
                ret: self_ty.clone(),
                is_effect: false,
            },
            ProtocolMethodSig {
                name: "mul".into(),
                params: vec![("a".into(), self_ty.clone()), ("b".into(), self_ty.clone())],
                ret: self_ty.clone(),
                is_effect: false,
            },
            ProtocolMethodSig {
                name: "div".into(),
                params: vec![("a".into(), self_ty.clone()), ("b".into(), self_ty.clone())],
                ret: self_ty.clone(),
                is_effect: false,
            },
        ],
    });

    // Register every numeric primitive type as implementing `Numeric`.
    // Without this, `T: Numeric` bounds fail the
    // `type '{}' does not implement protocol '{}'` check whenever a
    // concrete primitive (Int / Int32 / Float / ...) substitutes `T`.
    let numeric_primitives: &[&str] = &[
        "Int", "Float",
        "Int8", "Int16", "Int32",
        "UInt8", "UInt16", "UInt32", "UInt64",
        "Float32",
    ];
    for prim in numeric_primitives {
        env.type_protocols
            .entry(sym(prim))
            .or_default()
            .insert(sym("Numeric"));
    }
}
