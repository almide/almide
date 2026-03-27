<!-- description: Strategy for adding new codegen targets with minimal cost -->
<!-- done: 2026-03-18 -->
# Multi-Target Strategy

## Vision

Leverage the fact that Almide's multi-target design minimizes the cost of adding new target languages, and define a strategy for expanding target language support.

---

## Design Strength: Why New Targets Are Cheap to Add

Adding a new target requires only three things:

1. **Write `runtime/{lang}/core.{ext}`** — Result, Option representation, exception catching, type conversion rules (glue)
2. **Add `emit_{lang}/` to the compiler** — IR to target language source codegen (the largest task)
3. **Add `stdlib/*/extern.{ext}`** — platform-dependent modules only. Pure Almide modules need nothing

**Pure-first effect**: The more pure modules there are, the lower the cost of adding a new target. hash (189 lines), csv (70 lines), url (220 lines), path, encoding, args — these work with just the compiler's .almd to target language conversion, no extern files needed.

---

## Target List

### Tier 1: Current (Implemented)

| Target | Use Case | Status |
|---|---|---|
| **rust** | Native binaries, WASM, performance-critical | ✅ Implemented |
| **typescript** | Deno execution, typed source output | ✅ Implemented |
| **javascript** | Node/browser execution, untyped version of TS | ✅ Implemented |
| **wasm** | .wasm generation via Rust | ✅ Implemented (basic) |

### Tier 2: High Priority Candidates

| Target | Motivation | Compatibility with Almide | Extern Cost |
|---|---|---|---|
| **python** | Embedding into a massive ecosystem. Almide libraries installable via pip | High. Dynamic types, but Result representable with dataclass/NamedTuple. asyncio for async | Medium. fs/http/json/regex well covered by Python stdlib |
| **go** | Server-side ecosystem. Embedding into Go projects | High. Simple type system makes IR translation realistic. error is close to Result | Medium. os/net/http well covered by Go stdlib |

### Tier 3: Future Candidates

| Target | Motivation | Compatibility with Almide | Notes |
|---|---|---|---|
| **kotlin** | Android + JVM server | High. Result, sealed class provide good type expression compatibility | Choice between JVM bytecode vs Kotlin source |
| **swift** | iOS/macOS native | High. async/await design borrowed from Swift, making it a natural target | Demand limited to Apple platforms |
| **ruby** | Embedding into the Rails ecosystem | Medium. Dynamic types. Result expressible via Struct | Ecosystem async is immature |
| **c** | Embedded systems, legacy system integration | Low-Medium. Pure functions map almost directly, but List/String are hard without GC | Memory management strategy required |

---

## Glue + Extern Examples for Each Target

### Python

```python
# runtime/py/core.py
from dataclasses import dataclass
from typing import TypeVar, Union

T = TypeVar('T')
E = TypeVar('E')

@dataclass
class Ok:
    value: object
    ok: bool = True

@dataclass
class Err:
    error: object
    ok: bool = False

AlmResult = Union[Ok, Err]

def ok(value): return Ok(value=value)
def err(error): return Err(error=error)

def catch_to_result(f):
    try:
        return ok(f())
    except Exception as e:
        return err(str(e))
```

```python
# stdlib/fs/extern.py
from almide_runtime import ok, err

def almide_rt_fs_read_text(path: str):
    try:
        with open(path, 'r') as f:
            return ok(f.read())
    except Exception as e:
        return err(str(e))
```

### Go

```go
// runtime/go/core.go
package almide

type Result[T any] struct {
    Value T
    Error string
    Ok    bool
}

func Ok[T any](value T) Result[T] {
    return Result[T]{Value: value, Ok: true}
}

func Err[T any](error string) Result[T] {
    return Result[T]{Error: error, Ok: false}
}
```

```go
// stdlib/fs/extern.go
package almide_fs

import (
    "os"
    alm "almide/runtime/go"
)

func AlmideRtFsReadText(path string) alm.Result[string] {
    data, err := os.ReadFile(path)
    if err != nil {
        return alm.Err[string](err.Error())
    }
    return alm.Ok(string(data))
}
```

### Kotlin

```kotlin
// runtime/kt/Core.kt
sealed class AlmResult<out T, out E> {
    data class Ok<T>(val value: T) : AlmResult<T, Nothing>()
    data class Err<E>(val error: E) : AlmResult<Nothing, E>()
}

fun <T> ok(value: T): AlmResult<T, Nothing> = AlmResult.Ok(value)
fun <E> err(error: E): AlmResult<Nothing, E> = AlmResult.Err(error)

inline fun <T> catchToResult(f: () -> T): AlmResult<T, String> =
    try { ok(f()) } catch (e: Exception) { err(e.message ?: "unknown error") }
```

### Swift

```swift
// runtime/swift/Core.swift
enum AlmResult<T, E> {
    case ok(T)
    case err(E)
}

func catchToResult<T>(_ f: () throws -> T) -> AlmResult<T, String> {
    do { return .ok(try f()) }
    catch { return .err(error.localizedDescription) }
}
```

---

## Effort Estimate for Adding a New Target

| Task | Estimated Lines | Notes |
|---|---|---|
| `runtime/{lang}/core.{ext}` | ~50 lines | Glue is thin (Rule 7) |
| `emit_{lang}/` codegen | ~2000-4000 lines | IR to source conversion. The largest task |
| `stdlib/*/extern.{ext}` x ~10 modules | ~500 lines | Platform-dependent only |
| **Pure modules** | **0 lines** | **Compiler converts .almd to target** |
| Tests | ~500 lines | conformance test + extern test |
| **Total** | **~3000-5000 lines** | |

For comparison: current Rust codegen is ~3000 lines in `emit_rust/`, TS codegen is ~2000 lines in `emit_ts/`.

---

## Criteria for Prioritization

Motivations for adding new targets fall into three categories:

| Motivation | Example | Decision Criteria |
|---|---|---|
| **Execution environment** | Native, browser, edge | Can it only run in that environment? |
| **Ecosystem integration** | Embedding into Python/Go/Kotlin projects | Can it be distributed via pip/go get/gradle? |
| **Source output (inspect)** | Want to read generated code | `almide emit --lang X` is sufficient |

**Rule 6 check**: The compiler does not know the deployment target. Targets are "language/runtime", not "platform".

---

## Roadmap

### Phase 0: Complete Current Targets (CLI-First)

Reach a state where CLI tools can be fully written in Rust + TS/JS. @extern + glue + Result unification. This is the foundation for all targets.

### Phase 1: Python Target

- `runtime/py/core.py` — glue
- `emit_py/` — IR to Python codegen
- `stdlib/*/extern.py` — fs, io, env, process, http, json, regex
- `almide build app.almd --target py` — output `.py` files
- `almide emit app.almd --lang py` — Python source inspect
- Goal: distribute as a `pip install`-able package

### Phase 2: Go Target

- `runtime/go/core.go` — glue
- `emit_go/` — IR to Go codegen
- `stdlib/*/extern.go` — os, net/http, encoding/json, regexp
- `almide build app.almd --target go` — output `.go` files
- Leverage Go generics (1.18+)

### Phase 3: Kotlin / Swift (On Demand)

- When mobile/desktop expansion becomes viable
- Result naturally expressed with sealed class (Kotlin) / enum (Swift)
- async/await is native to both languages

### Phase 4: C (On Demand)

- Embedded systems / legacy integration
- Pure functions can be translated, but List/String memory management strategy is needed
- Requires deciding between arena allocator or reference counting

---

## Dependencies

- [Runtime Architecture Reform](stdlib-self-hosted-redesign.md) — Foundation for @extern + glue
- [CLI-First](cli-first.md) — Phase 0 (completing current targets)

## Related

- [New Codegen Targets](new-codegen-targets.md) — Existing codegen target roadmap (if any)
