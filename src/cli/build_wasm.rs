//! `almide build --target wasm`: render the module, package it for the
//! requested runtime (a WASI core module, a 0.2/0.3 component, or the
//! `wasi:http` serve export), write it, and say which leg produced it.

use crate::err;
use super::wasm_compile::compile_to_wasm_bytes_surfaced;
use super::wasm_debug::with_debug_lines;

/// `--host js` (#2265): write `<mod>.js` + `<mod>.d.ts` next to the module
/// (a refusal removes the module too, so a failed build leaves nothing) and
/// return the note the `Built` line appends.
fn write_js_host(output: &str, file: &str, bytes: &[u8], surface: &crate::cli::js_host::HostSurface) -> String {
    let base = output.strip_suffix(".wasm").unwrap_or(output);
    let (js_path, dts_path) = (format!("{base}.js"), format!("{base}.d.ts"));
    let wasm_name = std::path::Path::new(output).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| output.to_string());
    let notes = crate::cli::js_host::ExportNotes {
        owned: almide_wasm::host_exports::export_param_owned(),
        params: almide_wasm::host_exports::export_params_noted(),
        rets: almide_wasm::host_exports::export_rets(),
        imports: almide_wasm::host_exports::import_rets(),
    };
    let (js, dts) = match crate::cli::js_host::generate(&wasm_name, file, bytes, surface, &notes) {
        Ok(g) => g,
        Err(message) => {
            let _ = std::fs::remove_file(output);
            err(&message);
            std::process::exit(1);
        }
    };
    for (path, text) in [(&js_path, &js), (&dts_path, &dts)] {
        if let Err(e) = std::fs::write(path, text) {
            err(&format!("Failed to write {}: {}", path, e));
            std::process::exit(1);
        }
    }
    format!(" + {js_path} + {dts_path}")
}

/// The `--host` switch, validated: `js` or nothing; never with `--component`.
fn js_host_requested(host: Option<&str>, component: bool) -> bool {
    let js_host = match host {
        None => false,
        Some("js") => true,
        Some(other) => {
            err(&format!("error: `--host` accepts only `js` (got `{other}`)"));
            std::process::exit(2);
        }
    };
    if js_host && component {
        err("error: --host js writes a core-module host; it does not apply to --component");
        std::process::exit(2);
    }
    js_host
}

/// Direct WASM emit: parse → check → lower → optimize → monomorphize → emit WASM binary.
#[allow(clippy::too_many_arguments)]
pub(super) fn cmd_build_wasm_direct(file: &str, output: Option<&str>, _no_check: bool, allow_unverified: bool, verified: bool, wasm_opt: bool, component: bool, host: Option<&str>) {
    let default_output = format!("{}.wasm", file.strip_suffix(".almd").unwrap_or("a.out"));
    let output = output.unwrap_or(&default_output);
    // `--host js` (#2265): the compiler writes the JS host next to the
    // module. The module exports its allocator + release under the guard,
    // and the emitter notes which exported params the callee owns.
    let js_host = js_host_requested(host, component);
    let _js_host = js_host.then(almide_wasm::host_exports::JsHostGuard::set);

    // The whole parse→check→lower→emit pipeline lives in `compile_to_wasm_bytes`
    // so `almide run --target wasm` produces the byte-identical module this
    // command writes — the cross-target equivalence guarantee depends on both
    // entry points sharing one code path. Any compile diagnostic was already
    // printed there; we just propagate the exit.
    let (bytes, host_ops, surface) = match compile_to_wasm_bytes_surfaced(file, allow_unverified, verified, true, false) {
        Ok(b) => b,
        Err(()) => std::process::exit(1),
    };
    // The module imports `almide.*` (the embedded host's surface). A BUILD artifact must run on stock runtimes, so it ships in
    // the WASI form — same index space, shimmed imports, proc_exit on trap
    // (the #1588 transform; the 578-fixture stock-wasmtime gate is its
    // reproduction witness).
    // `--component` (#1628 stage 1): the DIRECT p2 path — canonical-ABI
    // imports straight off the almide.* module, no preview1 adapter (~25 KB
    // lighter, and the only shape the stage-2 fan/async lowering can build
    // on). `ALMIDE_COMPONENT_ADAPTER=1` is the switch back to the stage-0
    // adapter wrap over the `to_wasi` module (#2752 keeps it: the adapter
    // route is also what an fs program takes below).
    // #2589 (ADR-0025): the subprocess family rides the private
    // `almide:process/spawn` import, which the p1 core module carries and no
    // component world declares — refuse the component build by name rather
    // than let a transform fail on an import it cannot place.
    let proc_op = host_ops.iter().copied().find(|op| (80..=90).contains(op));
    if component && let Some(op) = proc_op {
        err(&format!(
            "error[E081]: process.* (host op {op}) needs the private almide:process/spawn capability, which no component world declares (ADR-0025)\n  \
             hint: build the core module (drop --component) for a host that implements almide:process/spawn, or run it with `almide run {file} --target wasm`"
        ));
        std::process::exit(1);
    }
    // #2659 (C-375): a program that reaches `http.serve` (its main
    // serve-shaped — checked in `compile_to_wasm_bytes_surfaced`) builds as
    // the stock serve export, a `wasi:http/handler@0.3.0` component, with or
    // without `--component`.
    let serve_export = host_ops.iter().any(|op| (70..=72).contains(op));
    if serve_export && js_host {
        err("error[E081]: `http.serve` builds as a wasi:http/handler@0.3.0 component, which --host js does not write");
        std::process::exit(1);
    }
    let component = component || serve_export;
    let direct_p2 = component && !serve_export && !almide_base::env::flag("ALMIDE_COMPONENT_ADAPTER");
    // `ALMIDE_COMPONENT_P3=1` (#1628 stage 2, experimental): the WASI 0.3
    // component — stdio over component-model streams on the async
    // canonical ABI. Needs a p3-capable runtime (wasmtime 46+); stays an
    // env opt-in until the fan lowering lands on the same plumbing and
    // the corpus gates cover it.
    let direct_p3 = direct_p2 && almide_base::env::flag("ALMIDE_COMPONENT_P3");
    // #2742: a WASI 0.2 component that reaches the p1 fs service keeps the
    // stage-0 adapter route whenever the p1 shim serves its whole op set.
    // Those programs used to reach that route through the incumbent (the
    // p1 op audit rerouted every fs op there); the fs service plus the
    // preview1 adapter now serve them from the structural module, so the
    // direct shim's E081 must not claim a program that built before. An op
    // set without an fs op keeps the direct shim's verdict (#2113).
    let fs_via_adapter = direct_p2
        && !direct_p3
        && almide_wasm_run::component_availability::check(&host_ops, false).is_err()
        && host_ops.iter().any(|op| almide_wasm_run::wasi::FS_SERVICE_OPS.iter().any(|(o, _, _)| o == op))
        && host_ops.iter().all(|op| almide_wasm_run::wasi::P1_SERVED_OPS.contains(op));
    let direct_p2 = direct_p2 && !fs_via_adapter;
    if direct_p2
        && let Err(message) = almide_wasm_run::component_availability::check(&host_ops, direct_p3)
    {
        err(&message);
        std::process::exit(1);
    }
    let bytes = if serve_export {
        match almide_wasm_run::wasi_p3::to_p3_service(&bytes, &host_ops) {
            Ok(c) => c,
            Err(e) => {
                err(&format!("error: p3 serve export transform failed — this is an Almide bug: {e}"));
                std::process::exit(1);
            }
        }
    } else if direct_p3 {
        match almide_wasm_run::wasi_p3::to_p3(&bytes, &host_ops) {
            Ok(c) => c,
            Err(e) => {
                err(&format!("error: p3 component transform failed — this is an Almide bug: {e}"));
                std::process::exit(1);
            }
        }
    } else if direct_p2 {
        match almide_wasm_run::wasi_p2::to_p2(&bytes) {
            Ok(c) => c,
            Err(e) => {
                err(&format!("error: p2 component transform failed — this is an Almide bug: {e}"));
                std::process::exit(1);
            }
        }
    } else {
        let wasi = match almide_wasm_run::wasi::to_wasi_mapped(&bytes, &host_ops) {
            Ok(w) => w,
            Err(e) => {
                err(&format!("error: WASI transform failed — this is an Almide bug: {e}"));
                std::process::exit(1);
            }
        };
        let bytes = with_debug_lines(file, &bytes, wasi);
        // Stage-0 adapter wrap: the WASI core module + the Cargo-pinned
        // preview1 adapter. Packaging, not a rewrite.
        if component {
            match wrap_component(&bytes) {
                Ok(c) => c,
                Err(e) => {
                    err(&format!("error: component encoding failed — this is an Almide bug: {e}"));
                    std::process::exit(1);
                }
            }
        } else {
            bytes
        }
    };

    let pre_size = bytes.len();
    if let Err(e) = std::fs::write(output, &bytes) {
        err(&format!("Failed to write {}: {}", output, e));
        std::process::exit(1);
    }
    // The JS host is derived from the bytes that SHIP (#2276): after the
    // optional `wasm-opt` rewrite below, never from the pre-opt module.
    let host_from_shipped = |shipped: &[u8]| if js_host { write_js_host(output, file, shipped, &surface) } else { String::new() };

    // The trust-spine ships the bytes ITS OWN rendering process produced —
    // reachability DCE and the name-section trim already ran inside that
    // pipeline (docs/wasm/WASM-OUTPUT.md). `wasm-opt` is a different kind of
    // thing: an EXTERNAL, unverified transform applied to the renderer's
    // finished output, so running it replaces bytes the trust-spine produced
    // with bytes a separate, un-certified tool rewrote. That is why it stays
    // an explicit, default-off opt-in (`--wasm-opt`) rather than automatic —
    // see the wasm-opt parity leg (`tests/wasm_runtime_opt_parity.rs::wasm_opt_parity_spec`) for the
    // differential-testing evidence backing this tier's own guarantee.
    // Name the LEG in the one line every build prints: "which renderer
    // produced these bytes" was invisible by default (the line said
    // v1-verified even for structural output), and that opacity cost real
    // diagnosis time — a "wasm doesn't work" report cannot be split
    // between legs without it.
    let leg = match component {
        true if serve_export => "structural leg, WASI 0.3 wasi:http/handler export — serve it with `wasmtime serve`",
        false => "structural leg",
        true if direct_p3 => "structural leg, WASI 0.3 component (direct, async ABI)",
        true if direct_p2 => "structural leg, WASI 0.2 component (direct)",
        true => "structural leg, WASI 0.2 component (adapter)",
    };
    // The trust word belongs to what earned it: the structural leg is
    // trusted end to end with its certificate PENDING (#1696,
    // docs/contracts/proven-vs-trusted.md). `verified` on output no
    // certificate covers is how #2154's run-time trap shipped (#2184).
    let trust = "trusted, certificate pending";
    // The artifact needs a host that grants the subprocess capability: say
    // so at build time, since a stock runtime will refuse it at load.
    if proc_op.is_some() {
        err(&format!(
            "note: {output} imports almide:process/spawn (process.*, ADR-0025): a stock WASI runtime refuses it at load; \
             it runs on a host that implements that import — `almide run {file} --target wasm` is one"
        ));
    }
    if !wasm_opt {
        let host_note = host_from_shipped(&bytes);
        err(&format!(
            "Built {}{} ({} bytes, {}, {} — wasm-opt skipped; pass --wasm-opt for a smaller build rewritten outside the renderer)",
            output, host_note, pre_size, leg, trust
        ));
        return;
    }

    match run_wasm_opt(output) {
        Ok(post_size) => {
            let shipped = std::fs::read(output).unwrap_or_else(|e| { err(&format!("Failed to read back {}: {}", output, e)); std::process::exit(1); });
            let host_note = host_from_shipped(&shipped);
            let pct = if pre_size > 0 { 100.0 * (pre_size - post_size) as f64 / pre_size as f64 } else { 0.0 };
            err(&format!(
                "Built {}{} ({} bytes → {} bytes, -{:.1}%, {}) — wasm-opt applied: these are NOT the renderer's own bytes",
                output, host_note, pre_size, post_size, pct, leg
            ));
        }
        Err(why) => {
            let host_note = host_from_shipped(&bytes);
            err(&format!(
                "Built {}{} ({} bytes, {}, {}) — --wasm-opt requested but not applied: {}; shipped the renderer's own module unoptimized",
                output, host_note, pre_size, leg, trust, why
            ));
        }
    }
}

/// Wrap a WASI-preview1 core module into a WASI 0.2 component: the
/// wit-component encoder + the adapter the provider crate pins. The
/// spike evidence (issue #1628): hello-world and the stdin family run
/// byte-identically core vs component on wasmtime 47.
fn wrap_component(core: &[u8]) -> Result<Vec<u8>, String> {
    wit_component::ComponentEncoder::default()
        .module(core)
        .map_err(|e| format!("module: {e}"))?
        .adapter(
            "wasi_snapshot_preview1",
            wasi_preview1_component_adapter_provider::WASI_SNAPSHOT_PREVIEW1_COMMAND_ADAPTER,
        )
        .map_err(|e| format!("adapter: {e}"))?
        .encode()
        .map_err(|e| format!("encode: {e}"))
}

/// Run `wasm-opt -Oz` on the output file, in-place.
/// Returns the new file size on success.
fn run_wasm_opt(path: &str) -> Result<usize, String> {
    // `-Oz`, matching the flag's documented contract: `--wasm-opt` exists to
    // shrink the module (the published size tables are -Oz numbers), and the
    // implementation silently ran `-O3 --enable-simd` instead — a speed
    // profile with a feature the v1 renderer does not emit (no v128 in the
    // default output, #864/#916), so the documented numbers were not
    // reproducible through the flag.
    // --enable-nontrapping-float-to-int: float→int renders as
    // `i64.trunc_sat_f64_s` (the saturating truncate, lib_b.rs).
    // --enable-tail-call: mutual/self tail recursion renders `return_call`.
    // --enable-bulk-memory: the STRUCTURAL leg renders `memory.copy`
    // (#1616 — without the flag wasm-opt refuses the module, and the old
    // message blamed a missing install).
    // --enable-mutable-globals: the structural leg EXPORTS mutable
    // globals; newer binaryen accepts that by default but older releases
    // (CI's) validate the MVP restriction unless told otherwise — the
    // first CI run of the honest-refusal path caught exactly this.
    let out = std::process::Command::new("wasm-opt")
        .args([
            "-Oz",
            "--enable-nontrapping-float-to-int",
            "--enable-tail-call",
            "--enable-bulk-memory",
            "--enable-mutable-globals",
            path,
            "-o",
            path,
        ])
        .output()
        .map_err(|e| format!("wasm-opt is not installed ({})", e))?;
    if !out.status.success() {
        // The tool RAN and refused — a different failure class than a
        // missing install, and its stderr names the real cause (#1616:
        // "not installed" sent users to reinstall a tool they had).
        return Err(format!(
            "wasm-opt ran and refused (exit {:?}): {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let meta = std::fs::metadata(path).map_err(|e| format!("stat {}: {}", path, e))?;
    Ok(meta.len() as usize)
}
