//! Async JS imports through JSPI (#3353): `--host js` with an
//! `@extern(wasm, "js", NAME, returns: promise)` (#3371).
//!
//! A marked `@extern(wasm, "js", NAME, ...)` import is wrapped in
//! `WebAssembly.Suspending`, so a hook that returns a Promise suspends the
//! wasm stack until it settles; Almide code sees an ordinary synchronous
//! call. JSPI throws `SuspendError` when a Suspending import is reached from
//! an export NOT entered through `WebAssembly.promising` — even when the hook
//! returns a plain value (measured on Node 24.21) — so exactly the exports
//! that can reach a suspending import are entered that way and become async
//! in the glue. Which ones can is read from the SHIPPED bytes' call graph,
//! with every `call_indirect` assumed to reach every function in the table:
//! an over-approximation only makes an export async needlessly, never leaves
//! one sync that can suspend. Design: docs/wasm/JS-HOST-ASYNC-IMPORTS.md.

use std::collections::{BTreeMap, BTreeSet};

use wasmparser::{ElementItems, ExternalKind, Operator, Parser, Payload, TypeRef};

/// What suspends: the async import names, and the exports (by export name;
/// `_start` for `run()`) whose call graph reaches one of them.
#[derive(Debug, Default)]
pub(super) struct Suspension {
    pub imports: BTreeSet<String>,
    pub exports: BTreeSet<String>,
}

impl Suspension {
    pub(super) fn active(&self) -> bool {
        !self.imports.is_empty()
    }
}

/// The module's function-level call graph: callees per defined function
/// (by function index), the async import indices, the table's functions,
/// and the function exports.
#[derive(Default)]
struct CallGraph {
    imported: u32,
    calls: Vec<BTreeSet<u32>>,
    indirect: Vec<bool>,
    table: BTreeSet<u32>,
    exports: BTreeMap<String, u32>,
    suspending: BTreeSet<u32>,
}

fn element_funcs(items: ElementItems<'_>, table: &mut BTreeSet<u32>) -> Result<(), String> {
    match items {
        ElementItems::Functions(r) => {
            for f in r {
                table.insert(f.map_err(|e| e.to_string())?);
            }
        }
        ElementItems::Expressions(_, r) => {
            for expr in r {
                for op in expr.map_err(|e| e.to_string())?.get_operators_reader() {
                    if let Operator::RefFunc { function_index } = op.map_err(|e| e.to_string())? {
                        table.insert(function_index);
                    }
                }
            }
        }
    }
    Ok(())
}

/// A function body's direct callees, and whether it calls indirectly.
fn body_calls(body: wasmparser::FunctionBody<'_>) -> Result<(BTreeSet<u32>, bool), String> {
    let (mut callees, mut indirect) = (BTreeSet::new(), false);
    for op in body.get_operators_reader().map_err(|e| e.to_string())? {
        match op.map_err(|e| e.to_string())? {
            Operator::Call { function_index } | Operator::ReturnCall { function_index } => {
                callees.insert(function_index);
            }
            Operator::CallIndirect { .. } | Operator::ReturnCallIndirect { .. } | Operator::CallRef { .. } | Operator::ReturnCallRef { .. } => indirect = true,
            _ => {}
        }
    }
    Ok((callees, indirect))
}

fn call_graph(bytes: &[u8], async_imports: &BTreeSet<String>) -> Result<CallGraph, String> {
    let mut g = CallGraph::default();
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.map_err(|e| e.to_string())? {
            Payload::ImportSection(reader) => {
                let funcs = reader.into_iter().flatten().flat_map(|grp| grp.into_iter().flatten()).filter(|(_, imp)| matches!(imp.ty, TypeRef::Func(_)));
                for (_, imp) in funcs {
                    // #3383: a fan's overlap protocol suspends in `wait` alone.
                    let suspends = if imp.module == almide_wasm::host_exports::FAN_MODULE { imp.name == "wait" } else { imp.module != "wasi_snapshot_preview1" && async_imports.contains(imp.name) };
                    if suspends {
                        g.suspending.insert(g.imported);
                    }
                    g.imported += 1;
                }
            }
            Payload::ElementSection(reader) => {
                for el in reader {
                    element_funcs(el.map_err(|e| e.to_string())?.items, &mut g.table)?;
                }
            }
            Payload::ExportSection(reader) => {
                for e in reader {
                    let e = e.map_err(|e| e.to_string())?;
                    if e.kind == ExternalKind::Func {
                        g.exports.insert(e.name.to_string(), e.index);
                    }
                }
            }
            Payload::CodeSectionEntry(body) => {
                let (callees, indirect) = body_calls(body)?;
                g.calls.push(callees);
                g.indirect.push(indirect);
            }
            _ => {}
        }
    }
    Ok(g)
}

impl CallGraph {
    /// Every function that can reach a suspending import (a fixpoint over
    /// the callee sets, indirect calls reaching the whole table).
    fn reaching(&self) -> BTreeSet<u32> {
        let mut reach = self.suspending.clone();
        loop {
            let before = reach.len();
            for (i, callees) in self.calls.iter().enumerate() {
                let f = self.imported + i as u32;
                let via_table = self.indirect[i] && self.table.iter().any(|t| reach.contains(t));
                if !reach.contains(&f) && (via_table || callees.iter().any(|c| reach.contains(c))) {
                    reach.insert(f);
                }
            }
            if reach.len() == before {
                return reach;
            }
        }
    }
}

/// Which of the `wrapped` exports (the glue's wrappers, and `_start`)
/// suspend, read from the shipped `bytes`.
pub(super) fn analyse(bytes: &[u8], async_imports: &[String], wrapped: &BTreeSet<String>) -> Result<Suspension, String> {
    if async_imports.is_empty() {
        return Ok(Suspension::default());
    }
    let imports: BTreeSet<String> = async_imports.iter().cloned().collect();
    let graph = call_graph(bytes, &imports)?;
    let reach = graph.reaching();
    let exports = graph.exports.iter().filter(|(n, f)| wrapped.contains(*n) && reach.contains(f)).map(|(n, _)| n.clone()).collect();
    // An async import the optimiser removed (nothing calls it) suspends nothing.
    Ok(Suspension { imports, exports })
}

/// The `init()` preamble: refuse clearly where JSPI is missing.
pub(super) fn init_check(s: &Suspension) -> String {
    if !s.active() {
        return String::new();
    }
    let names: Vec<String> = s.imports.iter().map(|n| format!("\"{n}\"")).collect();
    format!("  jspiOrRefuse([{}]);\n", names.join(", "))
}

/// The `init()` epilogue: the `WebAssembly.promising` entry of every
/// suspending export.
pub(super) fn init_promised(s: &Suspension) -> String {
    if !s.active() {
        return String::new();
    }
    let rows: Vec<String> = s.exports.iter().map(|n| format!("{n}: WebAssembly.promising(instance.exports.{n})")).collect();
    format!("  promised = {{ {} }};\n", rows.join(", "))
}

/// The JSPI runtime: the feature check, the serialising queue and the
/// busy guard. Shipped only when some import is async.
pub(super) const JS_ASYNC_RUNTIME: &str = include_str!("js_host_async.js");

/// `run()` when `main` can suspend.
pub(super) const RUN_ASYNC_JS: &str = "\n/** Run `main` (the module's `_start`), which awaits async imports; a non-zero exit rejects with AlmideExit. */\nexport function run() {\n  return serial(async () => {\n    ready();\n    try {\n      await promised._start();\n    } catch (e) {\n      if (e instanceof AlmideExit && e.code === 0) return;\n      throw e;\n    } finally {\n      flush();\n    }\n  });\n}\n";

/// How an export's wrapper enters the module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Entry {
    /// No async import anywhere: today's wrapper, unchanged.
    Plain,
    /// The module has async imports but this export cannot reach one: a sync
    /// wrapper that refuses to enter while an async call is suspended.
    Guarded,
    /// Can reach an async import: entered through `WebAssembly.promising`,
    /// one call at a time.
    Async,
}

impl Suspension {
    pub(super) fn entry(&self, export: &str) -> Entry {
        if !self.active() {
            Entry::Plain
        } else if self.exports.contains(export) {
            Entry::Async
        } else {
            Entry::Guarded
        }
    }
}
