//! Built-in protocol registration.
//!
//! Registers the built-in conventions (`BUILTIN_PROTOCOLS`: Eq, Repr, Ord,
//! Hash, Codec, Encode, Decode, Numeric) as protocol definitions in
//! `TypeEnv.protocols`.

use almide_base::intern::sym;
use crate::types::{Ty, TypeEnv, ProtocolDef, ProtocolMethodSig};

/// One built-in protocol: its name and the methods it requires, given the
/// `Self` type variable and the `Value` type.
pub struct BuiltinProtocol {
    pub name: &'static str,
    methods: fn(self_ty: &Ty, value_ty: &Ty) -> Vec<ProtocolMethodSig>,
}

impl BuiltinProtocol {
    /// The protocol definition registered under `name`.
    pub fn def(&self) -> ProtocolDef {
        let self_ty = Ty::TypeVar(sym("Self"));
        let value_ty = Ty::Named(sym("Value"), vec![]);
        ProtocolDef {
            origin: None,
            name: self.name.into(),
            generics: vec![],
            methods: (self.methods)(&self_ty, &value_ty),
        }
    }
}

fn method(name: &str, params: &[(&str, &Ty)], ret: Ty) -> ProtocolMethodSig {
    ProtocolMethodSig {
        name: name.into(),
        params: params.iter().map(|(n, t)| ((*n).into(), (*t).clone())).collect(),
        ret,
        is_effect: false,
        mut_params: vec![],
    }
}

/// EVERY built-in protocol, and the only list of them (#2839):
/// `register_builtin_protocols` registers exactly these, and the scope check
/// asks `builtin_protocol` — the same table — whether a bare protocol name is
/// built in, so the two cannot disagree. A new built-in is added here or
/// nowhere.
pub const BUILTIN_PROTOCOLS: &[BuiltinProtocol] = &[
    // Eq: fn eq(a: Self, b: Self) -> Bool
    BuiltinProtocol { name: "Eq", methods: |s, _| vec![method("eq", &[("a", s), ("b", s)], Ty::Bool)] },
    // Repr: fn repr(v: Self) -> String
    BuiltinProtocol { name: "Repr", methods: |s, _| vec![method("repr", &[("v", s)], Ty::String)] },
    // Ord: fn cmp(a: Self, b: Self) -> Int
    BuiltinProtocol { name: "Ord", methods: |s, _| vec![method("cmp", &[("a", s), ("b", s)], Ty::Int)] },
    // Hash: fn hash(v: Self) -> Int
    BuiltinProtocol { name: "Hash", methods: |s, _| vec![method("hash", &[("v", s)], Ty::Int)] },
    // Codec: fn encode(v: Self) -> Value, fn decode(v: Value) -> Result[Self, String]
    BuiltinProtocol {
        name: "Codec",
        methods: |s, v| vec![
            method("encode", &[("v", s)], v.clone()),
            method("decode", &[("v", v)], Ty::result(s.clone(), Ty::String)),
        ],
    },
    // Encode: fn encode(v: Self) -> Value
    BuiltinProtocol { name: "Encode", methods: |s, v| vec![method("encode", &[("v", s)], v.clone())] },
    // Decode: fn decode(v: Value) -> Result[Self, String]
    BuiltinProtocol {
        name: "Decode",
        methods: |s, v| vec![method("decode", &[("v", v)], Ty::result(s.clone(), Ty::String))],
    },
    // Numeric: abstract interface for numeric primitive types. Methods
    // match the `BinOp` dispatch pairs so `fn f[T: Numeric](x: T, y: T)
    // = x + y` flows through without a separate hand-impl. Monomorph
    // repairs the `BinOp` kind once `T` resolves to a concrete width.
    BuiltinProtocol {
        name: "Numeric",
        methods: |s, _| ["add", "sub", "mul", "div"].iter()
            .map(|m| method(m, &[("a", s), ("b", s)], s.clone()))
            .collect(),
    },
];

/// The built-in protocol named `name`, if there is one.
pub fn builtin_protocol(name: &str) -> Option<&'static BuiltinProtocol> {
    BUILTIN_PROTOCOLS.iter().find(|p| p.name == name)
}

/// Register built-in conventions as protocols.
pub fn register_builtin_protocols(env: &mut TypeEnv) {
    for p in BUILTIN_PROTOCOLS {
        env.protocols.insert(sym(p.name), p.def());
    }

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

    // Register hashable primitive types as implementing `Hash`.
    // Float, Fn, and Map are excluded (NaN, identity, and nested mutability).
    let hashable_primitives: &[&str] = &[
        "Int", "String", "Bool", "Unit",
        "Int8", "Int16", "Int32",
        "UInt8", "UInt16", "UInt32", "UInt64",
    ];
    for prim in hashable_primitives {
        env.type_protocols
            .entry(sym(prim))
            .or_default()
            .insert(sym("Hash"));
    }
}
