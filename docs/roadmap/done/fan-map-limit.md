<!-- description: Add concurrency limit parameter to fan.map -->
<!-- done: 2026-03-19 -->
# fan.map Concurrency Limit

## Overview

`fan.map(xs, limit: n, f)` — fan.map with a maximum concurrency limit.

## Motivation

Processing 1000 URLs with `fan.map(urls, fetch)` spawns 1000 threads simultaneously → resource exhaustion.
With `limit: 16`, at most 16 run concurrently and the rest are queued.

## Syntax

```almide
let results = fan.map(urls, limit: 16, (url) => http.get(url))
```

## Implementation Approach

### Rust (thread backend)

Semaphore-style control. Chunk splitting or worker pool within `std::thread::scope`.

```rust
std::thread::scope(|s| {
    let (tx, rx) = std::sync::mpsc::channel();
    let semaphore = Arc::new(std::sync::Semaphore::new(limit));
    for item in xs {
        let permit = semaphore.acquire();
        s.spawn(move || { let r = f(item); tx.send(r); drop(permit); });
    }
    // collect in order
})
```

Note: `std::sync::Semaphore` is unstable. Alternatives: `Arc<Mutex<usize>>` + `Condvar`, or chunk-based approach.

### TS

```typescript
// p-limit pattern or hand-written concurrency limiter
async function fanMapLimit(xs, limit, f) {
    const results = new Array(xs.length);
    let idx = 0;
    const workers = Array.from({ length: limit }, async () => {
        while (idx < xs.length) { const i = idx++; results[i] = await f(xs[i]); }
    });
    await Promise.all(workers);
    return results;
}
```

## Prerequisites

- Named args support for `fan.map` (`limit:` is a default argument)
- Type checking of the `limit` parameter in the checker (Int)

## Priority

Low. The current all-at-once spawn is fine for small to medium scale. Implement when large-scale batch processing is needed.
