//! The fan overlap protocol on `--host js` (#3383): the glue side of
//! `crates/almide-wasm/src/fan_js_async.rs`.
//!
//! A `fan` whose every element is one call of an async hook (`@extern(wasm,
//! "js", NAME, returns: promise)`, #3371) imports three functions of
//! `almide:fan` (`host_exports::FAN_MODULE`) instead of suspending once per
//! element:
//!
//! - `start:NAME(args…) -> slot` — a plain import: decodes the arguments,
//!   calls the hook and keeps its Promise in a slot;
//! - `wait()` — the ONLY Suspending import: awaits every started slot, and
//!   settles each as its value or its rejection;
//! - `take:NAME(slot) -> ret` — a plain import: the settled value, encoded
//!   as the hook's own answer is (`imports::hook_body`) — a rejection is the
//!   err of a fallible extern and abandons the instance for an infallible
//!   one, as the hook's direct call does (#3356).
//!
//! The export-side inference is unchanged: an export whose call graph reaches
//! `wait` is entered through `WebAssembly.promising` (js_host_async.rs).
//! Design: docs/wasm/JS-HOST-ASYNC-IMPORTS.md, "Fan overlap".

use super::*;
use almide_wasm::host_exports::FAN_MODULE;

/// The protocol runtime: the slot table and the three entry points the
/// generated imports call. Shipped only when the module names the protocol.
pub(super) const JS_FAN_RUNTIME: &str = include_str!("js_host_fan.js");

/// A protocol import's role, by name.
enum Proto<'a> {
    Start(&'a str),
    Wait,
    Take(&'a str),
}

fn proto(name: &str) -> Option<Proto<'_>> {
    if name == "wait" {
        return Some(Proto::Wait);
    }
    if let Some(h) = name.strip_prefix("start:") {
        return Some(Proto::Start(h));
    }
    name.strip_prefix("take:").map(Proto::Take)
}

/// The async extern a protocol import names.
fn async_extern<'s>(surface: &'s HostSurface, hook: &str) -> Option<&'s HostExtern> {
    surface.externs.iter().find(|e| e.module == "js" && e.import == hook && surface.async_imports.iter().any(|n| n == hook))
}

/// Does the glue serve `module.name` as part of the protocol?
pub(super) fn serves(module: &str, name: &str, surface: &HostSurface) -> bool {
    module == FAN_MODULE
        && match proto(name) {
            Some(Proto::Wait) => true,
            Some(Proto::Start(h) | Proto::Take(h)) => async_extern(surface, h).is_some(),
            None => false,
        }
}

/// One protocol import's line of `imports()`.
pub(super) fn import_js(name: &str, sig: &WasmSig, surface: &HostSurface, import_rets: &ImportRets) -> Result<String, String> {
    let unknown = || format!("error: --host js has no fan protocol import `{}.{name}` (#3383)", FAN_MODULE);
    let (hook, start) = match proto(name).ok_or_else(unknown)? {
        Proto::Wait => return Ok("  fanImports[\"wait\"] = new WebAssembly.Suspending(fanWait);\n".to_string()),
        Proto::Start(h) => (h, true),
        Proto::Take(h) => (h, false),
    };
    let e = async_extern(surface, hook).ok_or_else(unknown)?;
    if start {
        let args: Vec<String> = (0..sig.params.len()).map(|i| format!("a{i}")).collect();
        let mut conv = Vec::new();
        for (i, (_, ty)) in e.sig.params.iter().enumerate() {
            let m = marshal_of(ty).expect("checked by check_marshallable");
            let v = *sig.params.get(i).ok_or_else(unknown)?;
            conv.push(from_wasm(m, v, &args[i], hook, false));
        }
        return Ok(format!("  fanImports[\"{name}\"] = ({}) => fanStart(() => hook(\"js\", \"{hook}\")({}));\n", args.join(", "), conv.join(", ")));
    }
    let recorded = import_rets.get(&("js".to_string(), hook.to_string()));
    let body = super::imports::hook_body(e, "fanTake(s)", false, sig.results.first(), recorded)?;
    Ok(format!("  fanImports[\"{name}\"] = (s) => {{ {body} }};\n"))
}

/// The protocol's part of the import object (empty without one). An
/// `@extern` naming the protocol's module is refused: the glue owns it.
pub(super) fn object_js(fan_js: &str, surface: &HostSurface) -> Result<String, String> {
    if let Some(e) = surface.externs.iter().find(|e| e.module == FAN_MODULE) {
        return Err(format!(
            "error: --host js reserves the import module \"{}\" for the fan overlap protocol (#3383), but `{}` declares @extern(wasm, \"{}\", \"{}\")\n  hint: bind the hook under another module name, such as \"js\"",
            FAN_MODULE,
            e.sig.name,
            e.module,
            e.import
        ));
    }
    if fan_js.is_empty() {
        return Ok(String::new());
    }
    Ok(format!("  const fanImports = {{}};\n{fan_js}  obj[\"{}\"] = fanImports;\n", FAN_MODULE))
}
