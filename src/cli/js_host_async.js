// JSPI (#3353): async imports suspend the module; the exports that can reach
// one are entered through WebAssembly.promising. Calls into the instance are
// serialised — one runs at a time, in call order — so a suspended call never
// interleaves with another: the observation is that of the calls made one
// after another (docs/wasm/JS-HOST-ASYNC-IMPORTS.md).
let promised = null;
let inFlight = false;
let tail = Promise.resolve();
function jspiOrRefuse(names) {
  if (typeof WebAssembly.Suspending === "function" && typeof WebAssembly.promising === "function") return;
  // Node 24.0–24.19 share a V8 with JSPI off by default; 24.20 turns it on (#3362).
  const node = typeof process !== "undefined" && process.versions && process.versions.node ? ` (this is Node ${process.versions.node})` : "";
  throw new Error(`almide: this module awaits async JS imports (${names.join(", ")}) through JSPI, and this runtime has no WebAssembly.Suspending / WebAssembly.promising${node} — run it on Node >= 24.20, Chrome >= 137 or workerd, or build without --async-import`);
}
function serial(run) {
  const result = tail.then(async () => {
    inFlight = true;
    try { return await run(); } finally { inFlight = false; }
  });
  tail = result.then(() => null, () => null);
  return result;
}
function idle(what) {
  if (inFlight) throw new Error(`almide: ${what} was called while an async call is suspended in the module — await that call first`);
}
