<!-- description: Optional tokio-based async backend for high-concurrency workloads -->
# Async Backend — tokio opt-in

## Overview

Add an async backend alongside the current sync/thread backend. tokio is not mixed into the language spec — it's provided as one backend implementation.

## Motivation

- The thread model hits its limits for high-concurrency HTTP servers (10K+ connections)
- async I/O significantly improves CPU efficiency
- Required for streaming workloads like WebSocket / SSE

## Design Principles

- **No language spec changes** — `effect fn`, `fan` semantics remain unchanged
- **Abstract spawn/join/sleep via runtime trait**
- **Only the entrypoint depends on the backend** — `#[tokio::main]` appears only in generated code
- **Switch via feature flag**: `almide build --runtime tokio`

## Changes

### Rust codegen

- `effect fn` → `async fn`
- Auto-insert `.await` on effect fn calls
- `fan { }` → `tokio::try_join!`
- `fan.map` → `futures::future::try_join_all`
- `fan.race` → `tokio::select!`
- `main` → `#[tokio::main] async fn main()`

### Generated Cargo.toml

```toml
[dependencies]
tokio = { version = "1", features = ["rt", "time", "macros"] }
futures = "0.3"
```

### WASM Target

Does not use tokio. JSPI or sequential fallback.

## Prerequisites

- fan language feature is stable (Phase 0-5 completed)
- After practical HTTP server use cases emerge

## Priority

Low. The sync/thread backend covers foreseeable use cases.
