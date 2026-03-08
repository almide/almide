# External Interoperability

## FFI (Foreign Function Interface)
A mechanism for calling Rust libraries directly.

```almide
// proposed: bind Rust functions with extern declarations
extern "rust" fn crypto_hash(data: String) -> String

// integrates with Cargo.toml-based dependency management
```

## JavaScript Interop (TS target)
Allow using existing TS/JS libraries.

```almide
// proposed
extern "js" fn fetch(url: String) -> String
```

## WASM Extensions
- WASI preview 2 support
- Host bindings (Cloudflare Workers, Fastly Compute, etc.)
- Component model support

## C ABI
A mechanism for using low-level libraries (SQLite, OpenSSL, etc.).
Achievable via Rust's unsafe FFI.

## Priority
FFI (Rust) > JS interop > WASM extensions > C ABI
