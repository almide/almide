//! The export side of `--host js` (#3352, #3354): one wrapper per exported
//! function, planned from the boundary ABI the EMITTER recorded for it
//! (`almide_wasm::host_exports`) and checked against the source types.
//!
//! Scalars (`Int`, `Float`, `Bool`) ride the wasm value itself and a
//! `String` keeps the string helpers' path. Every other supported shape —
//! `Bytes`, `List[T]`, `Option[T]`, records, and an effect fn's `Result` —
//! is a block the glue's value helpers (`js_host_values.js`) build or read
//! by the recorded layout, so the host never re-derives one. A shape the
//! record marks unsupported, or a record that disagrees with the source
//! type, is a build-time refusal naming the function: never a wrapper that
//! reads garbage.

use super::jspi::Entry;
use super::{from_wasm, returns_result, to_wasm, visible_ret, HostFn, Marshal, Val, WasmSig};
use almide_lang::types::constructor::TypeConstructorId;
use almide_lang::types::Ty;
use almide_wasm::host_exports::{AbiShape, ExportRet};

/// How one argument crosses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ParamPlan {
    /// `Int` / `Float` / `Bool`: the wasm value.
    Scalar(Marshal),
    /// `String`: a block from `allocString`.
    Str,
    /// Any other block, built by `putBlock` from its shape.
    Block(AbiShape),
}

/// How the module's return becomes the JS value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RetPlan {
    Void,
    /// `Int` / `Float` / `Bool` / `String`: the existing conversions.
    Scalar(Marshal),
    /// A block read by `takeBlock` and released with its children.
    Block(AbiShape),
    /// A `Result` block: ok → the value, err → a thrown `AlmideError`.
    Unwrap(AbiShape),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ExportPlan {
    pub params: Vec<ParamPlan>,
    pub ret: RetPlan,
}

impl ExportPlan {
    /// Does the wrapper call the glue's value helpers?
    pub(super) fn needs_values(&self) -> bool {
        self.params.iter().any(|p| matches!(p, ParamPlan::Block(_))) || matches!(self.ret, RetPlan::Block(_) | RetPlan::Unwrap(_))
    }
}

fn is_ctor(ty: &Ty, ctor: TypeConstructorId) -> Option<&Ty> {
    match ty {
        Ty::Applied(c, args) if *c == ctor && args.len() == 1 => Some(&args[0]),
        _ => None,
    }
}

/// A source type the export boundary can carry, before the module's record
/// is read — so an obviously unsupported type is refused with its name even
/// when the structural leg did not export the fn. Whether a `Named` type is
/// a record (supported) or a variant (not) is the record's to say.
pub(super) fn expressible(ty: &Ty) -> bool {
    match ty {
        Ty::Int | Ty::Float | Ty::Bool | Ty::String | Ty::Unit | Ty::Bytes => true,
        Ty::Named(..) | Ty::Record { .. } => true,
        _ => is_ctor(ty, TypeConstructorId::List).or_else(|| is_ctor(ty, TypeConstructorId::Option)).is_some_and(expressible),
    }
}

/// Does the recorded shape describe the source type? A disagreement means
/// the host's picture of the module is wrong, and the build refuses.
fn agrees(ty: &Ty, shape: &AbiShape) -> bool {
    match (ty, shape) {
        (Ty::Int, AbiShape::Int) | (Ty::Float, AbiShape::Float) | (Ty::Bool, AbiShape::Bool) => true,
        (Ty::String, AbiShape::Str) | (Ty::Unit, AbiShape::Unit) | (Ty::Bytes, AbiShape::Bytes) => true,
        (_, AbiShape::List { el, .. }) => is_ctor(ty, TypeConstructorId::List).is_some_and(|t| agrees(t, el)),
        (_, AbiShape::Option(el)) => is_ctor(ty, TypeConstructorId::Option).is_some_and(|t| agrees(t, el)),
        // A generic instance's def name is the emitter's; the field check
        // below is what an anonymous record can be held to.
        (Ty::Named(n, args), AbiShape::Record { name, .. }) => !args.is_empty() || n.as_str() == name,
        (Ty::Record { fields }, AbiShape::Record { fields: recorded, .. }) => {
            fields.len() == recorded.len()
                && fields.iter().all(|(n, t)| recorded.iter().any(|(rn, _, rs)| rn == n.as_str() && agrees(t, rs)))
        }
        _ => false,
    }
}

fn mismatch(f: &HostFn, what: &str, declared: &Ty, recorded: &str) -> String {
    format!(
        "error: --host js cannot wrap {what} of `{name}`: the source declares `{declared:?}` but the module's export ABI is {recorded} — no wrapper would read it correctly (#3352, #3354)\n  \
         hint: the JS host carries Int, Float, Bool, String, Unit, Bytes, List, Option and records (an effect fn's err becomes a thrown AlmideError); keep `{name}` off the host surface otherwise",
        name = f.name,
    )
}

fn param_plan(f: &HostFn, p: &str, ty: &Ty, shape: &AbiShape) -> Result<ParamPlan, String> {
    let what = format!("parameter `{p}`");
    if let Some(other) = shape.unsupported() {
        return Err(mismatch(f, &what, ty, &format!("`{other}`, which has no JS marshalling")));
    }
    if !agrees(ty, shape) {
        return Err(mismatch(f, &what, ty, &format!("{shape:?}")));
    }
    Ok(match shape {
        AbiShape::Int => ParamPlan::Scalar(Marshal::Int),
        AbiShape::Float => ParamPlan::Scalar(Marshal::Float),
        AbiShape::Bool => ParamPlan::Scalar(Marshal::Bool),
        AbiShape::Str => ParamPlan::Str,
        AbiShape::Unit => return Err(mismatch(f, &what, ty, "a Unit argument, which has no JS value")),
        other => ParamPlan::Block(other.clone()),
    })
}

/// The wrapper's plan, from the boundary ABI the emitter recorded for the
/// export (#3352: an effect fn returning String was once read as a String;
/// #3354: every shape is read by the recorded layout).
pub(super) fn plan_export(f: &HostFn, params: Option<&[AbiShape]>, ret: Option<&ExportRet>) -> Result<ExportPlan, String> {
    let recorded = params.ok_or_else(|| mismatch(f, "the parameters", &f.ret, "not recorded"))?;
    if recorded.len() != f.params.len() {
        return Err(mismatch(f, "the parameters", &f.ret, &format!("{} parameter(s), the source has {}", recorded.len(), f.params.len())));
    }
    let params = f.params.iter().zip(recorded).map(|((p, ty), s)| param_plan(f, p, ty, s)).collect::<Result<Vec<_>, _>>()?;
    let vis = visible_ret(f);
    let ok = |s: &AbiShape| s.unsupported().is_none() && agrees(vis, s);
    let plan = match (ret, returns_result(f)) {
        (Some(ExportRet::Void), false) if matches!(vis, Ty::Unit) => Some(RetPlan::Void),
        (Some(ExportRet::Value(s)), false) if ok(s) => Some(match s {
            AbiShape::Int => RetPlan::Scalar(Marshal::Int),
            AbiShape::Float => RetPlan::Scalar(Marshal::Float),
            AbiShape::Bool => RetPlan::Scalar(Marshal::Bool),
            AbiShape::Str => RetPlan::Scalar(Marshal::Str),
            AbiShape::Unit => RetPlan::Void,
            other => RetPlan::Block(other.clone()),
        }),
        (Some(ExportRet::Result(s, AbiShape::Str)), true) if ok(s) => Some(RetPlan::Unwrap(s.clone())),
        _ => None,
    };
    let ret = plan.ok_or_else(|| {
        let recorded = match ret {
            Some(r) => format!("{r:?}"),
            None => "not recorded".to_string(),
        };
        let recorded = if f.is_effect { format!("{recorded} for an effect fn") } else { recorded };
        mismatch(f, "the return", &f.ret, &recorded)
    })?;
    Ok(ExportPlan { params, ret })
}

/// The shape as the JS object literal the value helpers read.
fn shape_js(s: &AbiShape) -> String {
    match s {
        AbiShape::Int => r#"{k:"int"}"#.to_string(),
        AbiShape::Float => r#"{k:"float"}"#.to_string(),
        AbiShape::Bool => r#"{k:"bool"}"#.to_string(),
        AbiShape::Str => r#"{k:"str"}"#.to_string(),
        AbiShape::Unit => r#"{k:"unit"}"#.to_string(),
        AbiShape::Bytes => r#"{k:"bytes"}"#.to_string(),
        AbiShape::List { el, stride } => format!(r#"{{k:"list",stride:{stride},el:{}}}"#, shape_js(el)),
        AbiShape::Option(el) => format!(r#"{{k:"opt",el:{}}}"#, shape_js(el)),
        AbiShape::Record { size, fields, .. } => {
            let fs: Vec<String> = fields.iter().map(|(n, off, fs)| format!(r#"["{n}",{off},{}]"#, shape_js(fs))).collect();
            format!(r#"{{k:"rec",size:{size},fields:[{}]}}"#, fs.join(","))
        }
        AbiShape::Other(what) => format!("/* unsupported: {what} */ null"),
    }
}

/// The TypeScript type of a shape. `top` marks a return slot, where `Unit`
/// is `void`.
fn ts_shape(s: &AbiShape, top: bool) -> String {
    match s {
        AbiShape::Int | AbiShape::Float => "number".to_string(),
        AbiShape::Bool => "boolean".to_string(),
        AbiShape::Str => "string".to_string(),
        AbiShape::Unit if top => "void".to_string(),
        AbiShape::Unit => "undefined".to_string(),
        AbiShape::Bytes => "Uint8Array".to_string(),
        AbiShape::List { el, .. } => format!("Array<{}>", ts_shape(el, false)),
        AbiShape::Option(el) => format!("{} | undefined", ts_shape(el, false)),
        AbiShape::Record { fields, .. } => {
            let fs: Vec<String> = fields.iter().map(|(n, _, fs)| format!("{n}: {}", ts_shape(fs, false))).collect();
            format!("{{ {} }}", fs.join("; "))
        }
        AbiShape::Other(_) => "never".to_string(),
    }
}

fn ts_marshal(m: Marshal) -> &'static str {
    match m {
        Marshal::Int | Marshal::Float => "number",
        Marshal::Bool => "boolean",
        Marshal::Str => "string",
        Marshal::Unit => "void",
    }
}

/// The `.d.ts` line of an export.
pub(super) fn export_dts(f: &HostFn, plan: &ExportPlan, entry: Entry) -> String {
    let params: Vec<String> = f
        .params
        .iter()
        .zip(&plan.params)
        .map(|((p, _), pp)| {
            let ty = match pp {
                ParamPlan::Scalar(m) => ts_marshal(*m).to_string(),
                ParamPlan::Str => "string".to_string(),
                ParamPlan::Block(s) => ts_shape(s, false),
            };
            format!("{p}: {ty}")
        })
        .collect();
    let ret = match &plan.ret {
        RetPlan::Void => "void".to_string(),
        RetPlan::Scalar(m) => ts_marshal(*m).to_string(),
        RetPlan::Block(s) | RetPlan::Unwrap(s) => ts_shape(s, true),
    };
    let ret = if entry == Entry::Async { format!("Promise<{ret}>") } else { ret };
    format!("export function {}({}): {ret};\n", f.name, params.join(", "))
}

/// The wrapper for one export: the shape constants it reads, then the
/// function. A block argument the CALLEE owns (the recorded ownership) is
/// its to release; a borrowed one is released here after the call, with
/// the children it was the last owner of.
pub(super) fn wrapper_js(f: &HostFn, sig: &WasmSig, owned: &[bool], plan: &ExportPlan, entry: Entry) -> Result<String, String> {
    let mut consts = String::new();
    let mut shape_const = |key: String, s: &AbiShape| {
        let name = format!("S_{}_{key}", f.name);
        consts.push_str(&format!("const {name} = {};\n", shape_js(s)));
        name
    };
    let (mut pre, mut args, mut post) = (Vec::new(), Vec::new(), Vec::new());
    for (i, ((p, _), pp)) in f.params.iter().zip(&plan.params).enumerate() {
        let v = *sig.params.get(i).ok_or_else(|| format!("export `{}` has fewer wasm params than declared", f.name))?;
        let callee_owns = owned.get(i).copied().unwrap_or(false);
        match pp {
            ParamPlan::Scalar(m) => args.push(to_wasm(*m, v, p, &f.name)),
            ParamPlan::Str => {
                pre.push(format!("  const h{i} = allocString({p});\n"));
                args.push(format!("h{i}"));
                if !callee_owns {
                    post.push(format!("    instance.exports.__release(h{i});\n"));
                }
            }
            ParamPlan::Block(s) => {
                let c = shape_const(format!("p{i}"), s);
                pre.push(format!("  const h{i} = putBlock({c}, {p}, \"{}\");\n", f.name));
                args.push(format!("h{i}"));
                if !callee_owns {
                    post.push(format!("    drop(h{i}, {c});\n"));
                }
            }
        }
    }
    let call = if entry == Entry::Async {
        format!("(await promised.{}({}))", f.name, args.join(", "))
    } else {
        format!("instance.exports.{}({})", f.name, args.join(", "))
    };
    let block_result = |what: &str| -> Result<(), String> {
        match sig.results.first() {
            Some(Val::I32) => Ok(()),
            other => Err(format!("error: --host js cannot wrap `{}`: its {what} should be one i32 result, the module declares {other:?} (#3354)", f.name)),
        }
    };
    let body = match (&plan.ret, sig.results.first()) {
        (RetPlan::Void, _) | (RetPlan::Scalar(_), None) => format!("    {call};\n"),
        (RetPlan::Scalar(m), Some(v)) => format!("    return {};\n", from_wasm(*m, *v, &call, &f.name, true)),
        (RetPlan::Block(s), _) => {
            block_result("block")?;
            let c = shape_const("r".to_string(), s);
            format!("    return takeBlock({call}, {c}, \"{}\");\n", f.name)
        }
        (RetPlan::Unwrap(s), _) => {
            block_result("Result block")?;
            let res = format!(r#"{{k:"res",ok:{},err:{{k:"str"}}}}"#, shape_js(s));
            let c = format!("S_{}_r", f.name);
            consts.push_str(&format!("const {c} = {res};\n"));
            format!("    return takeResult({call}, {c}, \"{}\");\n", f.name)
        }
    };
    let names: Vec<&str> = f.params.iter().map(|(p, _)| p.as_str()).collect();
    let guard = if entry == Entry::Guarded { format!("  idle(\"{}\");\n", f.name) } else { String::new() };
    let mut inner = format!("  ready();\n{guard}{}", pre.concat());
    if post.is_empty() {
        inner.push_str(&body);
    } else {
        inner.push_str(&format!("  try {{\n{body}  }} finally {{\n{}  }}\n", post.concat()));
    }
    let names = names.join(", ");
    Ok(match entry {
        // One call in the instance at a time (#3353): the body runs when the
        // calls before it have settled.
        Entry::Async => format!("\n{consts}export function {}({names}) {{\n  return serial(async () => {{\n{inner}  }});\n}}\n", f.name),
        _ => format!("\n{consts}export function {}({names}) {{\n{inner}}}\n", f.name),
    })
}

/// Is `ty` (an export's param or visible return) carried by the wasm value
/// alone, so the host needs neither the allocator nor the release?
pub(super) fn scalar_only(ty: &Ty) -> bool {
    matches!(ty, Ty::Int | Ty::Float | Ty::Bool | Ty::Unit)
}

