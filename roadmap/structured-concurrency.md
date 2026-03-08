# Structured Concurrency

## Overview
Provides task lifecycle management as a higher-level layer on top of async/await.

## API Design

```almide
// wait for all tasks to complete
let results = await parallel([
  fetch_data(url1),
  fetch_data(url2),
])

// return the first one to complete
let fastest = await race([
  fetch_from_cache(key),
  fetch_from_db(key),
])

// with timeout
let data = await timeout(5000, fetch_data(url))
```

## Design Principles
- No fire-and-forget (all tasks complete within their scope)
- Cancellation propagation (if parent is cancelled, children stop too)
- No task leaks in AI-generated code

## Implementation Notes
- Rust: async task group + JoinHandle
- TS/Deno: Promise.all / Promise.race / AbortController
- WASM: single-threaded assumption, cooperative scheduling

## Status
Not started. async/await foundation is implemented.
