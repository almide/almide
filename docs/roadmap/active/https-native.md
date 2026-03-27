<!-- description: Native HTTPS support via rustls across all targets -->
# HTTPS Native Support

**Goal**: `http.get("https://...")` works across all targets

## Current State

| Target | HTTP | HTTPS | Implementation | Status |
|---|---|---|---|---|
| Rust (almide run) | OK | **OK** | rustls (pure Rust TLS) | ✅ Verified working via CLI |
| Rust (almide build) | OK | **Unverified** | Need to confirm rustls is included in generated binary | Needs verification |
| TS (Deno) | OK | OK | `fetch` native | ✅ |
| WASM | NG | NG | No socket API in WASI | Future support (wasi:http) |

## Done

- [x] **Phase 1: rustls Integration** — Integrated rustls + webpki-roots into `runtime/rs/src/http.rs`. `parse_url` recognizes the scheme and uses `ClientConnection` + `StreamOwned` for TLS connections on https.
- [x] **CLI Verification** — Requests to `https://` URLs succeed via `almide run`

- [x] **HTTPS Support in `almide build`** — After fixing effect fn Result wrapping, confirmed `effect fn main() -> Unit` + `http.get("https://...")` → `almide build` → execution succeeds. rustls is auto-linked via `runtime/rs/Cargo.toml`.

## Remaining

### WASM Target (Future)

WASM has no sockets at all, so resolution requires host imports:
```
// Once WASI HTTP stabilizes
import wasi:http/outgoing-handler@0.2.0
```

Wait for the WASI HTTP spec to stabilize.
