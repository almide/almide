//! Built-in protocol registration.
//!
//! Registers the built-in conventions (`BUILTIN_PROTOCOLS`: Eq, Repr, Ord,
//! Hash, Codec, Encode, Decode, Numeric, Integer, Text, Read, Write, Close,
//! HttpMessage) as protocol definitions in
//! `TypeEnv.protocols`.

use almide_base::intern::sym;
use crate::types::{Ty, TypeEnv, ProtocolDef, ProtocolMethodSig};

/// One built-in protocol: its name and the methods it requires, given the
/// `Self` type variable and the `Value` type.
pub struct BuiltinProtocol {
    pub name: &'static str,
    methods: fn(self_ty: &Ty, value_ty: &Ty) -> Vec<ProtocolMethodSig>,
    /// The built-in types that conform. `type_protocols` keys them by their
    /// display name; their requirements are their stdlib module's functions
    /// (`Ty::stdlib_module`). A stdlib-declared type (`BigInt`) conforms at its
    /// own declaration (`type BigInt: Numeric = ...`), not here.
    pub conformers: &'static [Ty],
    /// The runtime-backed stdlib types that conform, by name (`HttpRequest`):
    /// they have no `type` declaration to name the protocol at, and a
    /// `Ty::Named` cannot be a constant. Their requirements are functions of
    /// their owning module (`stdlib_owned_type_owner`), spelled by
    /// `requirement_fn_name`.
    pub named_conformers: &'static [&'static str],
    /// Whether a conforming type may leave the requirements to auto-derive
    /// (Eq, Repr, Ord, Hash, Codec). A type declaring a non-derivable
    /// protocol must define every requirement.
    pub derivable: bool,
    /// Whether only the stdlib may declare a type conforming to it (E118).
    /// `Body` is sealed (ADR-0032 Q15): a body type states the Content-Type a
    /// browser obeys, so a user type claiming `text/html` would serve any
    /// String as a page. User code reaches other media types through
    /// `http.typed`.
    pub sealed: bool,
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

    /// The names of the methods this protocol requires.
    pub fn method_names(&self) -> Vec<String> {
        self.def().methods.iter().map(|m| m.name.to_string()).collect()
    }
}

/// The operators a protocol requirement stands for: `a + b` on a conforming
/// type is its `add`, unary `-a` its `neg`. A primitive satisfies these with
/// the operator itself; a declared type (`BigInt`) with the function its
/// module defines under the requirement's name, which lowering desugars the
/// operator to. The one place an operator is paired with a method name.
pub const OPERATOR_REQUIREMENTS: &[(&str, &str)] = &[
    ("+", "add"), ("-", "sub"), ("*", "mul"), ("/", "div"), ("%", "rem"),
];
/// The method unary `-` desugars to on a conforming declared type.
pub const NEG_REQUIREMENT: &str = "neg";
/// The method `<` `<=` `>` `>=` desugar to (`compare(a, b) < 0`) on a declared
/// type whose protocols require it. A primitive orders with the operator.
pub const ORDER_REQUIREMENT: &str = "compare";

/// Is `method` of a built-in protocol satisfied by an operator on a primitive?
pub fn is_operator_requirement(method: &str) -> bool {
    method == NEG_REQUIREMENT || OPERATOR_REQUIREMENTS.iter().any(|(_, m)| *m == method)
}

/// Does built-in type `type_name` conform to `proto` by the table above?
pub fn is_builtin_conformer(type_name: &str, proto: &str) -> bool {
    let bare = type_name.rsplit_once('.').map_or(type_name, |(_, b)| b);
    builtin_protocol(proto).is_some_and(|p| {
        p.conformers.iter().any(|t| t.display() == type_name) || p.named_conformers.contains(&bare)
    })
}

/// The function a declared type's requirement `method` resolves to: its
/// convention method (`fn Dog.show`), or, for a type declared in a module, that
/// module's own function of the requirement's name (`bigint.add` for
/// `type BigInt: Numeric` in `bigint`) — the form a built-in type's
/// requirements take in its stdlib module. `None` when neither exists.
pub fn requirement_fn_key(env: &TypeEnv, type_name: &str, method: &str) -> Option<String> {
    if let Some(k) = super::registration::convention_fn_key(env, type_name, method) {
        return Some(k.to_string());
    }
    let module = declaring_module(env, type_name)?;
    let key = format!("{}.{}", module, almide_lang::stdlib_info::requirement_fn_name(type_name, method));
    env.functions.contains_key(&sym(&key)).then_some(key)
}

/// The module a declared type belongs to: the qualifier of a qualified name
/// (`bigint.BigInt`), else the module of the one qualified registration of
/// the bare name.
pub fn declaring_module(env: &TypeEnv, type_name: &str) -> Option<String> {
    if let Some((m, _)) = type_name.rsplit_once('.') {
        return Some(m.to_string());
    }
    if let Some(m) = almide_lang::stdlib_info::stdlib_owned_type_owner(type_name) {
        return Some(m.to_string());
    }
    let needle = format!(".{}", type_name);
    env.types.keys().find_map(|k| k.as_str().strip_suffix(needle.as_str()).map(str::to_string))
}

/// Does declared type `type_name` conform to a protocol that requires
/// `method`? The conformance table keys a type by the spelling its
/// declaration registered, so the bare and qualified names are both tried.
pub fn type_requires(env: &TypeEnv, type_name: &str, method: &str) -> bool {
    let bare = type_name.rsplit_once('.').map_or(type_name, |(_, b)| b);
    [type_name, bare].iter().filter_map(|n| env.type_protocols.get(&sym(n))).flatten().any(|p| {
        env.protocols.get(p).is_some_and(|d| d.methods.iter().any(|m| m.name.as_str() == method))
    })
}

/// Every (built-in type, protocol, requirement) the built-in conformances
/// promise, operator requirements excluded (a primitive has the operator).
/// The matrix gate (`almide docs-gen`) asserts each one is a function of the
/// type's stdlib module.
pub fn builtin_conformance_requirements() -> Vec<ConformanceRequirement> {
    let mut out = Vec::new();
    for p in BUILTIN_PROTOCOLS {
        let typed = p.conformers.iter().filter_map(|ty| ty.stdlib_module().map(|m| (m, ty.display())));
        let named = p.named_conformers.iter().filter_map(|n| {
            almide_lang::stdlib_info::stdlib_owned_type_owner(n).map(|m| (m, n.to_string()))
        });
        for (module, type_name) in typed.chain(named) {
            for m in p.method_names() {
                if !p.derivable && !is_operator_requirement(&m) {
                    let function = almide_lang::stdlib_info::requirement_fn_name(&type_name, &m);
                    out.push(ConformanceRequirement { module, protocol: p.name, requirement: m, function });
                }
            }
        }
    }
    out
}

/// One cell of the built-in conformance matrix: `module`'s `function`
/// satisfies `protocol`'s `requirement` for one conforming type.
pub struct ConformanceRequirement {
    pub module: &'static str,
    pub protocol: &'static str,
    pub requirement: String,
    pub function: String,
}

/// The fixed-width integer primitives, every one of which is `Numeric` and
/// `Integer`.
const INTEGER_PRIMITIVES: &[Ty] = &[
    Ty::Int, Ty::Int8, Ty::Int16, Ty::Int32, Ty::Int64, Ty::UInt8, Ty::UInt16, Ty::UInt32, Ty::UInt64,
];
const NUMERIC_PRIMITIVES: &[Ty] = &[
    Ty::Int, Ty::Int8, Ty::Int16, Ty::Int32, Ty::Int64, Ty::UInt8, Ty::UInt16, Ty::UInt32, Ty::UInt64,
    Ty::Float, Ty::Float32, Ty::Float64,
];

/// `Numeric`'s requirements (R-L0-40, R-L0-42). The operators, then the
/// conversion pivots the `<T>.from` family is written over: every numeric
/// value is either an integer that fits `Int` or `UInt64`, or a float, so a
/// target module converts from any source through these without a cell per
/// (source, target) pair. `__sum`/`__product` take no receiver and are
/// reached through the type variable (`A.__sum(xs)`). The `__` names are
/// internal: they implement the public `from`/`sum` surface and are not
/// listed in it.
fn numeric_methods(s: &Ty) -> Vec<ProtocolMethodSig> {
    let mut out: Vec<ProtocolMethodSig> = OPERATOR_REQUIREMENTS.iter()
        .map(|(_, m)| method(m, &[("a", s), ("b", s)], s.clone()))
        .collect();
    out.push(method(NEG_REQUIREMENT, &[("a", s)], s.clone()));
    let unary = |name: &str, ret: Ty| method(name, &[("x", s)], ret);
    out.extend([
        // the exact integer value, when it is one and fits
        unary("__int_exact", Ty::option(Ty::Int)),
        unary("__uint64_exact", Ty::option(Ty::UInt64)),
        // toward zero (identity on an integer)
        unary("__trunc", s.clone()),
        // toward zero, clamped to the range; NaN is 0
        unary("__int_sat", Ty::Int),
        unary("__uint64_sat", Ty::UInt64),
        // nearest (ties to even); `_exact` is none unless the value is exact
        unary("__to_float", Ty::Float),
        unary("__to_float_exact", Ty::option(Ty::Float)),
        unary("__to_float32", Ty::Float32),
        unary("__to_float32_exact", Ty::option(Ty::Float32)),
        // the exact decimal digits of an integral value of any size
        unary("__decimal_exact", Ty::option(Ty::String)),
    ]);
    let list = Ty::list(s.clone());
    out.push(method("__sum", &[("xs", &list)], s.clone()));
    out.push(method("__product", &[("xs", &list)], s.clone()));
    out
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

/// A requirement that acts on the host (`Read`'s read): an `effect fn` on
/// both sides, which `validate_protocol_method_modes` holds a conformer to.
fn effect_method(name: &str, params: &[(&str, &Ty)], ret: Ty) -> ProtocolMethodSig {
    ProtocolMethodSig { is_effect: true, ..method(name, params, ret) }
}

/// EVERY built-in protocol, and the only list of them (#2839):
/// `register_builtin_protocols` registers exactly these, and the scope check
/// asks `builtin_protocol` — the same table — whether a bare protocol name is
/// built in, so the two cannot disagree. A new built-in is added here or
/// nowhere.
pub const BUILTIN_PROTOCOLS: &[BuiltinProtocol] = &[
    // Eq: fn eq(a: Self, b: Self) -> Bool
    BuiltinProtocol { name: "Eq", methods: |s, _| vec![method("eq", &[("a", s), ("b", s)], Ty::Bool)], conformers: &[], named_conformers: &[], derivable: true, sealed: false },
    // Repr: fn repr(v: Self) -> String
    BuiltinProtocol { name: "Repr", methods: |s, _| vec![method("repr", &[("v", s)], Ty::String)], conformers: &[], named_conformers: &[], derivable: true, sealed: false },
    // Ord: fn cmp(a: Self, b: Self) -> Int
    BuiltinProtocol { name: "Ord", methods: |s, _| vec![method("cmp", &[("a", s), ("b", s)], Ty::Int)], conformers: &[], named_conformers: &[], derivable: true, sealed: false },
    // Hash: fn hash(v: Self) -> Int
    // Float, Fn, and Map are not Hash (NaN, identity, and nested mutability).
    BuiltinProtocol {
        name: "Hash",
        methods: |s, _| vec![method("hash", &[("v", s)], Ty::Int)],
        conformers: &[
            Ty::Int, Ty::String, Ty::Bool, Ty::Unit,
            Ty::Int8, Ty::Int16, Ty::Int32, Ty::UInt8, Ty::UInt16, Ty::UInt32, Ty::UInt64,
        ],
        named_conformers: &[],
        derivable: true,
        sealed: false,
    },
    // Codec = Encode + Decode (Swift's `Codable`): its requirements are the
    // union of its parts', and a `: Codec` type conforms to both parts.
    BuiltinProtocol {
        name: "Codec",
        methods: |s, v| vec![
            method("encode", &[("v", s)], v.clone()),
            method("decode", &[("v", v)], Ty::result(s.clone(), Ty::String)),
            method(STRICT_DECODE_METHOD, &[("v", v)], Ty::result(s.clone(), Ty::String)),
        ],
        conformers: &[],
        named_conformers: &[],
        derivable: true,
        sealed: false,
    },
    // Encode: fn encode(v: Self) -> Value — the type is written out.
    BuiltinProtocol { name: "Encode", methods: |s, v| vec![method("encode", &[("v", s)], v.clone())], conformers: &[], named_conformers: &[], derivable: true, sealed: false },
    // Decode: fn decode(v: Value) -> Result[Self, String] — the type is read
    // from input — and its strict twin, which refuses a key outside the type
    // at every depth. A `: Decode` type that is not also `Encode` is an INPUT
    // type: every field of it is input (row 22, E110).
    BuiltinProtocol {
        name: "Decode",
        methods: |s, v| vec![
            method("decode", &[("v", v)], Ty::result(s.clone(), Ty::String)),
            method(STRICT_DECODE_METHOD, &[("v", v)], Ty::result(s.clone(), Ty::String)),
        ],
        conformers: &[],
        named_conformers: &[],
        derivable: true,
        sealed: false,
    },
    // Numeric: the numeric types. The operator requirements match the
    // `BinOp` dispatch pairs so `fn f[T: Numeric](x: T, y: T) = x + y`
    // flows through without a separate hand-impl; monomorph repairs the
    // `BinOp` kind once `T` resolves to a concrete width.
    BuiltinProtocol { name: "Numeric", methods: |s, _| numeric_methods(s), conformers: NUMERIC_PRIMITIVES, named_conformers: &[], derivable: false, sealed: false },
    // Integer: the integer types; `__bits64` is the low 64 bits in two's
    // complement, which `<T>.from_wrapping` narrows (R-L0-34).
    BuiltinProtocol {
        name: "Integer",
        methods: |s, _| vec![
            method(ORDER_REQUIREMENT, &[("a", s), ("b", s)], Ty::Int),
            method("__bits64", &[("x", s)], Ty::Int),
        ],
        conformers: INTEGER_PRIMITIVES,
        named_conformers: &[],
        derivable: false,
        sealed: false,
    },
    // Text: the text-like sequences (`list.join[T: Text]`, R-L0-42).
    BuiltinProtocol {
        name: "Text",
        methods: |s, _| vec![method("__join", &[("xs", &Ty::list(s.clone())), ("sep", s)], s.clone())],
        conformers: &[Ty::String, Ty::Bytes],
        named_conformers: &[],
        derivable: false,
        sealed: false,
    },
    // The I/O protocols (R-L0-44): a handle that can be read, written or
    // closed. `io.read` / `io.write` / `io.close` and the default
    // `read_exact` / `read_to_end` are written once over them. Several types
    // of one module conform (net's TcpStream, TcpListener and UdpSocket all
    // `Close`), so each requirement is per type (`__self_`, see
    // `almide_lang::stdlib_info::requirement_fn_name`). The conformers
    // declare it at their own `type` (`type TcpStream: Read, Write, Close`).
    BuiltinProtocol {
        name: "Read",
        methods: |s, _| vec![effect_method("__self_read", &[("h", s), ("max", &Ty::Int)], Ty::result(Ty::Bytes, Ty::String))],
        conformers: &[],
        named_conformers: &[],
        derivable: false,
        sealed: false,
    },
    BuiltinProtocol {
        name: "Write",
        methods: |s, _| vec![effect_method("__self_write", &[("h", s), ("data", &Ty::Bytes)], Ty::result(Ty::Unit, Ty::String))],
        conformers: &[],
        named_conformers: &[],
        derivable: false,
        sealed: false,
    },
    BuiltinProtocol {
        name: "Close",
        methods: |s, _| vec![effect_method("__self_close", &[("h", s)], Ty::result(Ty::Unit, Ty::String))],
        conformers: &[],
        named_conformers: &[],
        derivable: false,
        sealed: false,
    },
    // An HTTP message (R-http-12): the request and the response read and
    // replace their headers and body through one generic function each
    // (`http.header[M: HttpMessage]`). Both are runtime-backed http types
    // with no `type` declaration, so they are listed by name here.
    // A response body (ADR-0032): the Content-Type it is served with and its
    // bytes. `http.response` / `with_headers` / `with_body` take a `Body`, so
    // a response's Content-Type is its body's type, never a header string.
    // The stdlib-declared conformers name it at their declarations
    // (`html.SafeHtml`, `svg.SafeSvg`, `http.TypedBody`).
    BuiltinProtocol {
        name: "Body",
        methods: |s, _| vec![
            method("__self_media_type", &[("b", s)], Ty::String),
            method("__self_to_bytes", &[("b", s)], Ty::Bytes),
        ],
        conformers: &[Ty::String, Ty::Bytes],
        named_conformers: &["Value"],
        derivable: false,
        sealed: true,
    },
    BuiltinProtocol {
        name: "HttpMessage",
        methods: |s, _| http_message_methods(s),
        conformers: &[],
        named_conformers: &["HttpRequest", "HttpResponse"],
        derivable: false,
        sealed: false,
    },
];

/// `HttpMessage`'s requirements: what `http.header` / `header_values` /
/// `headers` / `body` / `body_bytes` / `set_header` / `set_body` /
/// `set_body_bytes` dispatch to for each message type.
fn http_message_methods(s: &Ty) -> Vec<ProtocolMethodSig> {
    let str_ = Ty::String;
    let pair = Ty::Tuple(vec![Ty::String, Ty::String]);
    vec![
        method("__self_header", &[("m", s), ("name", &str_)], Ty::option(Ty::String)),
        method("__self_header_values", &[("m", s), ("name", &str_)], Ty::list(Ty::String)),
        method("__self_headers", &[("m", s)], Ty::list(pair)),
        method("__self_body", &[("m", s)], Ty::String),
        method("__self_body_bytes", &[("m", s)], Ty::Bytes),
        method("__self_set_header", &[("m", s), ("name", &str_), ("value", &str_)], s.clone()),
        method("__self_set_body", &[("m", s), ("body", &str_)], s.clone()),
        method("__self_set_body_bytes", &[("m", s), ("body", &Ty::Bytes)], s.clone()),
    ]
}

/// The built-in protocol named `name`, if there is one.
pub fn builtin_protocol(name: &str) -> Option<&'static BuiltinProtocol> {
    BUILTIN_PROTOCOLS.iter().find(|p| p.name == name)
}

/// The built-in protocols that are compositions of others, with their parts:
/// `Codec` is `Encode + Decode`. A type declaring a composition conforms to
/// each part, and derives exactly the union of what the parts derive.
pub const PROTOCOL_PARTS: &[(&str, &[&str])] = &[("Codec", &["Encode", "Decode"])];

/// The direction protocols a derive writes code for: `Encode` derives
/// `encode`; `Decode` derives `decode` and `decode_strict` (the decode that
/// refuses a key outside the type, at every depth — C-429, C-436).
pub const ENCODE_PROTOCOL: &str = "Encode";
pub const DECODE_PROTOCOL: &str = "Decode";
/// `Decode`'s strict requirement, derived for records and variants alike.
pub const STRICT_DECODE_METHOD: &str = "decode_strict";

/// `name` and every protocol it is composed of (`Codec` → Codec, Encode,
/// Decode). A protocol with no parts is just itself.
pub fn protocol_closure(name: &str) -> Vec<&str> {
    let mut out = vec![name];
    if let Some((_, parts)) = PROTOCOL_PARTS.iter().find(|(p, _)| *p == name) {
        out.extend(parts.iter().copied());
    }
    out
}

/// Is `part` a part of another protocol in `declared` (so the declaration
/// already covers it)? The derive-field and implementation validators judge
/// a composition once, through the protocol the type declared.
pub fn implied_by_another(part: &str, declared: &[almide_base::intern::Sym]) -> bool {
    declared.iter().any(|d| d.as_str() != part
        && PROTOCOL_PARTS.iter().any(|(p, parts)| *p == d.as_str() && parts.contains(&part)))
}

/// Which codec directions a list of declared protocols derives:
/// `(encode, decode)`.
pub fn codec_directions(declared: &[almide_base::intern::Sym]) -> (bool, bool) {
    let mut enc = false;
    let mut dec = false;
    for d in declared {
        for p in protocol_closure(d.as_str()) {
            enc |= p == ENCODE_PROTOCOL;
            dec |= p == DECODE_PROTOCOL;
        }
    }
    (enc, dec)
}

/// Is `name` a codec-family protocol (one whose closure holds a direction)?
/// Its derive serializes field by field, so the field grammar applies.
pub fn is_codec_family(name: &str) -> bool {
    protocol_closure(name).iter().any(|p| *p == ENCODE_PROTOCOL || *p == DECODE_PROTOCOL)
}

/// Register built-in conventions as protocols.
pub fn register_builtin_protocols(env: &mut TypeEnv) {
    for p in BUILTIN_PROTOCOLS {
        env.protocols.insert(sym(p.name), p.def());
    }

    // Each built-in protocol's built-in conformers. Without this, a
    // `T: Numeric` bound fails the `type '{}' does not implement protocol
    // '{}'` check whenever a concrete primitive substitutes `T`.
    for p in BUILTIN_PROTOCOLS {
        for prim in p.conformers {
            env.type_protocols.entry(sym(&prim.display())).or_default().insert(sym(p.name));
        }
        for name in p.named_conformers {
            env.type_protocols.entry(sym(name)).or_default().insert(sym(p.name));
        }
    }
}
