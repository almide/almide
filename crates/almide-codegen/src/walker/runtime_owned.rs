//! Runtime-owned nominal types (#1821).
//!
//! A handful of Almide-level type names are DEFINED by the native runtime
//! (runtime/rs/src), not by the emitted program, and every runtime module is
//! spliced flat into the user's Rust module. While the emitter spelled such a
//! type by its Almide name, a user's own `type Value = { … }` /
//! `type HttpRequest = …` / `type Endian = …` met the runtime's item of the
//! same spelling — E0428 / E0574 / E0560 at rustc after a green `almide
//! check` (the ALS-T6 check-passes/build-fails class). The runtime spells
//! them under reserved `Almide*` names; this table is the emitter's side of
//! that contract: every REFERENCE to a runtime-owned type renders the
//! reserved spelling, and the bare spelling belongs to the user.
//!
//! Three shapes, one table:
//! * builtin — `Value` has no declaration anywhere (the checker's
//!   `Ty::Named("Value")`).
//! * runtime-backed — `HttpRequest` / `HttpResponse` / `JsonPath`
//!   (`stdlib_info::RUNTIME_BACKED_TYPES`): named by bundled signatures, no
//!   declaration.
//! * bundled twin — `Endian` / `FileStat` / `ProcessStatus` ARE declared, in
//!   the bundled stdlib module (bytes / fs / process.almd), and that decl
//!   reaches the program's type decls whenever the import is explicit; the
//!   runtime defines the same shape under the reserved name. The twin decl is
//!   recognised by NAME + SHAPE: it is not emitted (the runtime's definition
//!   is the one, repr impl included), and its ctors and its record-literal key
//!   route to the reserved item. A user decl that merely shares the name has a
//!   different shape and is the user's type — it is emitted, it claims the
//!   bare spelling, and the runtime-owned mapping steps aside for it.

use std::collections::{HashMap, HashSet};

use almide_ir::{IrProgram, IrTypeDecl, IrTypeDeclKind, IrVariantKind};

/// The declared shape a bundled twin must carry to be the runtime's type.
enum TwinShape {
    /// Nullary variant cases, in declaration order.
    Variant(&'static [&'static str]),
    /// Record field names, in declaration order.
    Record(&'static [&'static str]),
}

struct RuntimeOwned {
    /// The Almide-level (bare) spelling.
    almd: &'static str,
    /// The runtime's reserved Rust spelling.
    rust: &'static str,
    /// The runtime module (runtime/rs/src/<module>.rs) that defines the item.
    module: &'static str,
    /// The bundled decl's shape for the twins; `None` for undeclared names.
    twin: Option<TwinShape>,
}

const TABLE: &[RuntimeOwned] = &[
    RuntimeOwned { almd: "Value", rust: "AlmideValue", module: "value", twin: None },
    RuntimeOwned { almd: "HttpRequest", rust: "AlmideHttpRequest", module: "http", twin: None },
    RuntimeOwned { almd: "HttpResponse", rust: "AlmideHttpResponse", module: "http", twin: None },
    RuntimeOwned { almd: "JsonPath", rust: "AlmideJsonPath", module: "json", twin: None },
    RuntimeOwned { almd: "Endian", rust: "AlmideEndian", module: "bytes", twin: Some(TwinShape::Variant(&["LittleEndian", "BigEndian"])) },
    RuntimeOwned { almd: "FileStat", rust: "AlmideFileStat", module: "fs", twin: Some(TwinShape::Record(&["size", "is_dir", "is_file", "modified"])) },
    RuntimeOwned { almd: "ProcessStatus", rust: "AlmideProcessStatus", module: "process", twin: Some(TwinShape::Record(&["code", "stdout", "stderr"])) },
];

/// The reserved spelling of a bundled-twin decl — `None` for every other
/// decl, including a user decl that only shares the name.
pub(crate) fn twin_spelling(td: &IrTypeDecl) -> Option<&'static str> {
    let entry = TABLE.iter().find(|e| e.almd == td.name.as_str())?;
    let is_twin = match (&entry.twin, &td.kind) {
        (Some(TwinShape::Variant(cases)), IrTypeDeclKind::Variant { cases: decl, .. }) => {
            decl.len() == cases.len()
                && decl.iter().zip(cases.iter())
                    .all(|(d, c)| d.name.as_str() == *c && matches!(d.kind, IrVariantKind::Unit))
        }
        (Some(TwinShape::Record(fields)), IrTypeDeclKind::Record { fields: decl }) => {
            decl.len() == fields.len()
                && decl.iter().zip(fields.iter()).all(|(d, f)| d.name.as_str() == *f)
        }
        _ => false,
    };
    is_twin.then_some(entry.rust)
}

/// The Rust name a type decl is keyed under: the reserved spelling for a
/// bundled twin, the decl's own name otherwise.
pub(crate) fn decl_rust_name(td: &IrTypeDecl) -> String {
    twin_spelling(td).unwrap_or(td.name.as_str()).to_string()
}

/// Almide name → reserved spelling for every runtime-owned type the program's
/// references must map: the table minus the names a USER decl claims (a decl
/// of that name that is not the bundled twin). The user's declaration wins
/// the bare spelling outright — record or variant, every reference to it is
/// the nominal `Ty::Named(name)`, exactly the form a runtime-backed reference
/// takes, so the two cannot be told apart per reference and the program-level
/// claim is the only sound rule. (A program that declares `type Value = …`
/// AND uses the runtime's `Value` under that name has already been conflated
/// by the checker — `json.parse(..)!.n` type-checks — a checker-side gap, not
/// a spelling collision.) Read by the type, record-literal and pattern
/// spelling sites through `CodegenAnnotations::runtime_owned_types`.
pub(crate) fn spellings_for(program: &IrProgram) -> HashMap<String, String> {
    let user_claimed: HashSet<&str> = program.type_decls.iter()
        .chain(program.modules.iter().flat_map(|m| m.type_decls.iter()))
        .filter(|td| twin_spelling(td).is_none())
        .map(|td| td.name.as_str())
        .collect();
    TABLE.iter()
        .filter(|e| !user_claimed.contains(e.almd))
        .map(|e| (e.almd.to_string(), e.rust.to_string()))
        .collect()
}

/// The runtime-owned variant ctors as `(ctor, reserved enum)`. Registered
/// FIRST in `ctor_to_enum`, so construction and patterns qualify against the
/// runtime enum whether or not the bundled decl reached the program — with
/// `bytes` auto-imported it never does, and a bare `LittleEndian` PATTERN was
/// a catch-all binding (E0170) while construction leaned on a runtime shim fn
/// — and so that a user decl registered after it wins its own ctor names.
pub(crate) fn variant_ctors() -> impl Iterator<Item = (&'static str, &'static str)> {
    TABLE.iter()
        .filter_map(|e| match e.twin {
            Some(TwinShape::Variant(cases)) => Some(cases.iter().map(move |c| (*c, e.rust))),
            _ => None,
        })
        .flatten()
}

/// The runtime modules whose owned type the rendered user code spells — the
/// module a TYPE reference pulls in (#1829). `IrProgram::used_stdlib_modules`
/// is call-driven, so a program that only NAMES a runtime-owned type (an
/// `Endian` annotation, a `BigEndian` ctor or pattern, a `FileStat` param)
/// without calling into its module never spliced the module that defines it:
/// E0425 / E0433 at rustc after a green `almide check`. The reserved spelling
/// is emitter-only — the bare name is the user's and a user type never
/// renders as `Almide*` — so its presence in the rendered user code IS the
/// reference, the same text-level union `emit_source` already performs for
/// the `almide_rt_<module>_` symbols an operator lowers to.
pub(crate) fn modules_spelled_in(user_code: &str) -> Vec<&'static str> {
    TABLE.iter()
        .filter(|e| user_code.contains(e.rust))
        .map(|e| e.module)
        .collect()
}

/// Whether the runtime's definition of a runtime-owned type carries an
/// `AlmideRepr` impl — the bundled twins do (`impl AlmideRepr for
/// AlmideEndian` in bytes.rs, `AlmideFileStat` in fs.rs,
/// `AlmideProcessStatus` in process.rs: the impls the walker used to emit
/// beside the twin decl, moved into the runtime by #1821). `${e}` on such a
/// value routes through `almide_repr` whether or not the bundled decl
/// reached the program: under the auto-import it never does, so the
/// decl-driven `repr_named_types` set left the walker on the `Display` path
/// and the runtime enum has none (E0277, #1829). The undeclared names
/// (`Value`, the http/json handles) keep their `Display` route untouched.
pub(crate) fn has_runtime_repr(almd: &str) -> bool {
    TABLE.iter().any(|e| e.almd == almd && e.twin.is_some())
}
