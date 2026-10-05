//! The `imports()` side of `--host js`: one closure per `@extern(wasm, ...)`
//! import the module names, plus the WASI shims (#2265, #3353, #3356).

use super::*;

/// A hook import's body around its call (#3356). A FALLIBLE extern (an
/// `effect fn`, or one declaring `Result[T, String]`) takes the hook's value
/// as ok and a throw or rejection as err — a Result block the calling Almide
/// code propagates through its own release path. An infallible one cannot
/// return an err, so a throw abandons the instance: the frames it unwinds
/// never release their blocks, and no later call may run on that heap.
/// A SYNC hook's call (#3371) goes through `sync()`, which refuses a
/// thenable: an unmarked hook that returns a Promise would otherwise hand the
/// module `"[object Promise]"` or `0`. That refusal is not the hook's err —
/// the fallible catch passes it through — and abandons the instance like a
/// throw from an infallible hook. A marked (`returns: promise`) hook awaits
/// instead and carries no check.
pub(super) fn hook_body(e: &HostExtern, call: &str, sync: bool, result: Option<&Val>, recorded: Option<&ExportRet>) -> Result<String, String> {
    let (module, name) = (&e.module, &e.import);
    if returns_result(&e.sig) {
        let m = marshal_of(visible_ret(&e.sig)).expect("checked by check_marshallable");
        let recorded_ok = match recorded {
            Some(ExportRet::Result(ok, AbiShape::Str)) => exports::shape_of_marshal(m) == *ok,
            _ => false,
        };
        if !recorded_ok || result != Some(&Val::I32) {
            return Err(format!(
                "error: --host js cannot wire the fallible import `{module}.{name}` of `{}`: the module's import ABI is {recorded:?} with result {result:?}, not a Result block (#3356)",
                e.sig.name
            ));
        }
        let pass = if sync { "if (e instanceof UnmarkedPromise) throw e; " } else { "" };
        return Ok(format!("try {{ return okResult({}, {call}); }} catch (e) {{ {pass}return errResult(e); }}", exports::shape_literal(m)));
    }
    let body = match (marshal_of(&e.sig.ret).expect("checked"), result) {
        (Marshal::Unit, _) | (_, None) => format!("{call};"),
        (m, Some(v)) => format!("return {};", to_wasm(m, *v, call, name)),
    };
    Ok(format!("try {{ {body} }} catch (e) {{ throw abandon(\"{module}\", \"{name}\", e); }}"))
}

pub(super) fn import_object_js(sigs: &WasmSigs, surface: &HostSurface, suspension: &jspi::Suspension, import_rets: &ImportRets) -> Result<String, String> {
    let mut js = String::from("\nfunction imports() {\n  const wasiImports = {};\n  const jsImports = {};\n");
    // #3383: the fan overlap protocol's imports (js_host_fan.rs).
    let mut fan_js = String::new();
    for (module, name, sig) in &sigs.imports {
        if module == "wasi_snapshot_preview1" {
            js.push_str(&format!("  wasiImports.{name} = wasi.{name};\n"));
            continue;
        }
        if module == almide_wasm::host_exports::FAN_MODULE {
            fan_js.push_str(&super::fan::import_js(name, sig, surface, import_rets)?);
            continue;
        }
        let e = surface.externs.iter().find(|e| &e.module == module && &e.import == name).expect("checked by check_imports_served");
        let args: Vec<String> = (0..sig.params.len()).map(|i| format!("a{i}")).collect();
        let mut conv = Vec::new();
        for (i, (_, ty)) in e.sig.params.iter().enumerate() {
            let m = marshal_of(ty).expect("checked by check_marshallable");
            let v = *sig.params.get(i).ok_or_else(|| format!("import `{name}` has fewer wasm params than `{}` declares", e.sig.name))?;
            conv.push(from_wasm(m, v, &args[i], name, false));
        }
        let suspends = suspension.imports.contains(name);
        // An async hook's arguments are decoded before it runs (the call
        // expression), its result encoded after the promise settles.
        let call = if suspends {
            format!("(await hook(\"{module}\", \"{name}\")({}))", conv.join(", "))
        } else {
            format!("sync(\"{module}\", \"{name}\", hook(\"{module}\", \"{name}\")({}))", conv.join(", "))
        };
        let body = hook_body(e, &call, !suspends, sig.results.first(), import_rets.get(&(module.clone(), name.clone())))?;
        if suspends {
            js.push_str(&format!("  jsImports.{name} = new WebAssembly.Suspending(async ({}) => {{ {body} }});\n", args.join(", ")));
        } else {
            js.push_str(&format!("  jsImports.{name} = ({}) => {{ {body} }};\n", args.join(", ")));
        }
    }
    js.push_str("  const obj = { wasi_snapshot_preview1: wasiImports };\n");
    let modules: std::collections::BTreeSet<&str> = surface.externs.iter().map(|e| e.module.as_str()).collect();
    for m in &modules {
        js.push_str(&format!("  obj[\"{m}\"] = jsImports;\n"));
    }
    js.push_str(&super::fan::object_js(&fan_js, surface)?);
    js.push_str("  return obj;\n}\n");
    if !fan_js.is_empty() {
        js.push_str(super::fan::JS_FAN_RUNTIME);
    }
    Ok(js)
}
