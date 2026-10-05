//! The JS-host export switch for the STRUCTURAL leg (#2265, the twin of
//! `almide_mir::host_exports`): `almide build --target wasm --host js`
//! needs two things a stock module does not export — the allocator, so the
//! host can build a `String` block the guest will own, and the flat release,
//! so the host can drop a `String` the guest handed back — and the callee's
//! ownership of each exported function's params, so the host knows whether
//! the credit it passed comes back. Off (the default) the module's bytes are
//! byte-identical to a build without the switch, so no size ledger moves.
//!
//! THREAD-LOCAL with a scoped guard, for the same cross-test hygiene the
//! heap-cap knob documents. The ownership notes are a side channel of the
//! same scope: the emitter records them while the guard is set and the CLI
//! takes them after the build.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

thread_local! {
    static JS_HOST: Cell<bool> = const { Cell::new(false) };
    static STRING_ABI: Cell<bool> = const { Cell::new(false) };
    static EXPORT_OWNED: RefCell<BTreeMap<String, Vec<bool>>> = const { RefCell::new(BTreeMap::new()) };
    static EXPORT_RET: RefCell<BTreeMap<String, ExportRet>> = const { RefCell::new(BTreeMap::new()) };
    static EXPORT_PARAMS: RefCell<BTreeMap<String, Vec<AbiShape>>> = const { RefCell::new(BTreeMap::new()) };
    static IMPORT_RET: RefCell<BTreeMap<(String, String), ExportRet>> = const { RefCell::new(BTreeMap::new()) };
    static ASYNC_IMPORTS: RefCell<std::collections::BTreeSet<(String, String)>> = const { RefCell::new(std::collections::BTreeSet::new()) };
}

/// The import module of the `--host js` fan overlap protocol (#3383,
/// fan_js_async.rs): the glue serves it and refuses an `@extern` naming it.
pub const FAN_MODULE: &str = "almide:fan";

/// What one slot on the export boundary holds, as the host converts it
/// (#3352, #3354) — the layout the emitter itself uses, so the host never
/// re-derives it. `Other` names a shape the host has no marshalling for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbiShape {
    Int,
    Float,
    Bool,
    Str,
    Unit,
    /// A byte-packed block; len = byte count.
    Bytes,
    /// A block of `len / stride` element slots of `stride` bytes.
    List { el: Box<AbiShape>, stride: u32 },
    /// `NULL_ADDR` = none; `some` is a block whose payload+0 slot holds the value.
    Option(Box<AbiShape>),
    /// A record block: `(field, payload-relative offset, shape)` in declared
    /// order, `size` payload bytes.
    Record { name: String, size: u32, fields: Vec<(String, u32, AbiShape)> },
    Other(String),
}

impl AbiShape {
    /// Does the shape (or anything inside it) lack a host marshalling?
    pub fn unsupported(&self) -> Option<&str> {
        match self {
            AbiShape::Other(what) => Some(what),
            AbiShape::List { el, .. } | AbiShape::Option(el) => el.unsupported(),
            AbiShape::Record { fields, .. } => fields.iter().find_map(|(_, _, f)| f.unsupported()),
            _ => None,
        }
    }
}

/// The return ABI the emitter gave an exported function (#3352) — read by
/// the JS host instead of guessing it from the source type. An effect fn's
/// wasm value is ALWAYS one `Result` block, whatever its declared return.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportRet {
    /// No result (a pure fn returning `Unit`).
    Void,
    /// The value itself.
    Value(AbiShape),
    /// An i32 `Result` block: tag at payload+`SUM_TAG` (0 = ok), the value
    /// at payload+`SUM_FIELD`.
    Result(AbiShape, AbiShape),
}

/// Records nest at most this deep on the boundary; a deeper (or recursive)
/// type is `Other`, a refusal rather than an unbounded walk.
const MAX_DEPTH: u32 = 16;

fn abi_shape(t: crate::SliceTy, types: &crate::types_table::TypeTable, depth: u32) -> AbiShape {
    use crate::{Scalar, SliceTy};
    if depth > MAX_DEPTH {
        return AbiShape::Other(format!("a type nested deeper than {MAX_DEPTH}"));
    }
    match t {
        SliceTy::Scalar(Scalar::Int) => AbiShape::Int,
        SliceTy::Scalar(Scalar::Float) => AbiShape::Float,
        SliceTy::Scalar(Scalar::Bool) => AbiShape::Bool,
        SliceTy::Scalar(Scalar::Str) => AbiShape::Str,
        SliceTy::Scalar(Scalar::Bytes) => AbiShape::Bytes,
        SliceTy::Unit => AbiShape::Unit,
        SliceTy::List(e) => {
            let el = types.el(e);
            AbiShape::List { stride: el.slot_size(), el: Box::new(abi_shape(el, types, depth + 1)) }
        }
        SliceTy::Option(e) => AbiShape::Option(Box::new(abi_shape(types.el(e), types, depth + 1))),
        SliceTy::Named(i) => match types.def(i) {
            crate::types_table::NamedDef::Record(r) => AbiShape::Record {
                name: types.name_of(i),
                size: r.size,
                fields: r.fields.iter().map(|f| (f.name.clone(), f.offset, abi_shape(f.ty, types, depth + 1))).collect(),
            },
            _ => AbiShape::Other(format!("variant `{}`", types.name_of(i))),
        },
        other => AbiShape::Other(format!("{other:?}")),
    }
}

/// The host-facing form of an export's emitted parameter types.
pub(crate) fn export_params(params: &[crate::SliceTy], types: &crate::types_table::TypeTable) -> Vec<AbiShape> {
    params.iter().map(|t| abi_shape(*t, types, 0)).collect()
}

/// The host-facing form of an export's emitted return type.
pub(crate) fn export_ret(ret: Option<crate::SliceTy>, types: &crate::types_table::TypeTable) -> ExportRet {
    match ret {
        None => ExportRet::Void,
        Some(crate::SliceTy::Result(ok, err)) => ExportRet::Result(abi_shape(types.el(ok), types, 0), abi_shape(types.el(err), types, 0)),
        Some(t) => ExportRet::Value(abi_shape(t, types, 0)),
    }
}

/// Is a JS host being built for the module under emission?
pub fn js_host() -> bool {
    JS_HOST.with(|c| c.get())
}

/// Does the host surface marshal a `String` (#2276)? Only then does the host
/// need the allocator and release exports; a scalar-only surface keeps the
/// module byte-identical to a build without the switch.
pub fn string_abi() -> bool {
    js_host() && STRING_ABI.with(|c| c.get())
}

/// Set by the CLI once the program's host surface is known (before emission).
pub fn set_string_abi(on: bool) {
    STRING_ABI.with(|c| c.set(on));
}

/// The export name of the allocator (`(len: i32) -> block: i32`, header set).
pub const ALLOC_EXPORT: &str = "__alloc";
/// The export name of the flat release (`(block: i32)`).
pub const RELEASE_EXPORT: &str = "__release";

/// Record which params of an exported function the CALLEE owns (releases
/// at its exit plan) — `false` means borrowed, the caller keeps its credit.
pub(crate) fn note_export(name: &str, param_owned: Vec<bool>, params: Vec<AbiShape>, ret: ExportRet) {
    if js_host() {
        EXPORT_PARAMS.with(|m| { m.borrow_mut().insert(name.to_string(), params); });
        EXPORT_OWNED.with(|m| { m.borrow_mut().insert(name.to_string(), param_owned); });
        EXPORT_RET.with(|m| { m.borrow_mut().insert(name.to_string(), ret); });
    }
}

/// The ownership notes recorded since the guard was set, keyed by export name.
pub fn export_param_owned() -> BTreeMap<String, Vec<bool>> {
    EXPORT_OWNED.with(|m| m.borrow().clone())
}

/// The return ABI of every export recorded since the guard was set (#3352).
pub fn export_rets() -> BTreeMap<String, ExportRet> {
    EXPORT_RET.with(|m| m.borrow().clone())
}

/// Record the return ABI of a declared `@extern(wasm, module, name)` import
/// (#3356): a `Result` there means the host answers with a Result block.
/// `promise`: the extern is marked `returns: promise` (#3371), so a fan over
/// it may overlap its waits (#3383).
pub(crate) fn note_import(module: &str, name: &str, ret: ExportRet, promise: bool) {
    if js_host() {
        let key = (module.to_string(), name.to_string());
        if promise {
            ASYNC_IMPORTS.with(|m| { m.borrow_mut().insert(key.clone()); });
        }
        IMPORT_RET.with(|m| { m.borrow_mut().insert(key, ret); });
    }
}

/// Is `module.name` an async hook noted under the guard (#3383)?
pub(crate) fn is_async_import(module: &str, name: &str) -> bool {
    ASYNC_IMPORTS.with(|m| m.borrow().contains(&(module.to_string(), name.to_string())))
}

/// The return ABI of every import recorded since the guard was set (#3356),
/// keyed by (module, name).
pub fn import_rets() -> BTreeMap<(String, String), ExportRet> {
    IMPORT_RET.with(|m| m.borrow().clone())
}

/// The parameter shapes of every export recorded since the guard was set (#3354).
pub fn export_params_noted() -> BTreeMap<String, Vec<AbiShape>> {
    EXPORT_PARAMS.with(|m| m.borrow().clone())
}

/// Turn the switch on for a scope and restore the previous state on drop.
#[must_use = "the guard restores the previous state when dropped; binding it to `_` restores immediately"]
pub struct JsHostGuard(bool);

impl JsHostGuard {
    pub fn set() -> Self {
        let prev = js_host();
        JS_HOST.with(|c| c.set(true));
        STRING_ABI.with(|c| c.set(false));
        EXPORT_OWNED.with(|m| m.borrow_mut().clear());
        EXPORT_RET.with(|m| m.borrow_mut().clear());
        EXPORT_PARAMS.with(|m| m.borrow_mut().clear());
        IMPORT_RET.with(|m| m.borrow_mut().clear());
        ASYNC_IMPORTS.with(|m| m.borrow_mut().clear());
        Self(prev)
    }
}

impl Drop for JsHostGuard {
    fn drop(&mut self) {
        JS_HOST.with(|c| c.set(self.0));
        STRING_ABI.with(|c| c.set(false));
    }
}
