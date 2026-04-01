<!-- description: HTTP client support for the WASM target via WASI or host imports -->
# WASM HTTP Client

**Priority:** Medium — Directly impacts practicality in V8 Isolate environments, but hard to solve short-term due to WASI constraints
**Prerequisites:** WASM fs I/O implemented (read_text, write, exists)

---

## Current State

- HTTP response construction functions (response, json, set_header, etc.) already work in WASM
- HTTP client functions (get, post, put, etc.) are stubs — return default values
- WASI preview1 has no networking API

## Why This Is Difficult

1. **WASI preview1 has no sockets/HTTP** — outside the spec
2. **wasi-http (preview2)** requires Component Model — Almide is Core WASM (preview1) based
3. **Host-specific imports** hurt portability — making it Cloudflare Workers-specific would eliminate the value of generic WASI binaries

## Options

### A. WASI preview2 + Component Model Support (Large Scope)

- Requires Component Model support in wasm-encoder
- Implement wasi-http interfaces
- Support both wasmtime and Cloudflare Workers
- **Effort: Large (several weeks)**

### B. Host-Provided Import Approach (Medium Scope)

- Custom imports like `__almide_http_get(url_ptr, url_len) -> (status, body_ptr, body_len)`
- Host runtime (wasmtime wrapper / CF Worker) provides the implementation
- Low portability but works immediately
- **Effort: Medium (a few days)**

### C. Status Quo + Error Result Return (Minimal Scope)

- Calling http client from WASM returns `err("http client not supported on WASM target")`
- Consider compile-time warning as well
- **Effort: Small (a few hours)**

## Recommendation

Short-term: **C** (error Result). Medium-term: evaluate **B** (host imports). Wait for WASI preview2 stabilization before pursuing A.
