<!-- description: Extend UFCS resolution to external library functions -->
<!-- done: 2026-03-18 -->
# UFCS for External Libraries

## Problem

UFCS is currently hardcoded to stdlib (`resolve_ufcs_candidates` in `src/stdlib.rs`). External library functions cannot be called without a module prefix.

```almide
// Current: module prefix required
web.param(req, "id")
web.add_header(res, "X-Custom", "value")

// Desired: method-style via UFCS
req.param("id")
res.add_header("X-Custom", "value")
```

This significantly hurts DX for APIs like web frameworks. Compared to Hono's `c.req.param('id')`, `web.param(req, "id")` is verbose.

## Design

### Basic Rules

External library functions become UFCS candidates when they meet these conditions:

1. The first argument is a named type (record / variant)
2. The function is defined in the same module as the first argument's type

```almide
// web/mod.almd
type Request = { method: String, path: String, headers: Map[String, String], body: String, params: Map[String, String] }

fn param(req: Request, name: String) -> String = ...
fn query(req: Request, name: String) -> Option[String] = ...

// Call site
import web

// Both OK
web.param(req, "id")     // Explicit
req.param("id")           // UFCS
```

### Resolution Strategy

**Type-directed resolution**: when the receiver's type is known, search for functions in the module where that type is defined.

```
req.param("id")
  → req's type is web.Request
  → web module has param(Request, String)
  → resolves to web.param(req, "id")
```

This is a natural extension of the current stdlib UFCS (`resolve_ufcs_by_type`). It simply generalizes the stdlib hardcoding to automatic lookup of the type's defining module.

### Not UFCS Candidates

- External functions whose first argument is a primitive type (`String`, `Int`, etc.) → could conflict with stdlib
- First argument is an open record / anonymous record → no defining module exists
- Same-named functions across different modules whose first argument types overlap → ambiguity error

### Priority Relative to stdlib

1. Search stdlib UFCS candidates first
2. If not found, search the receiver type's defining module
3. If found in both → compile error (ambiguity)

## Implementation

### Phase 1: Type-directed module lookup

- When the checker resolves a UFCS call, if the receiver type is a named type, search its defining module
- Extend `resolve_ufcs_by_type` to search modules beyond stdlib
- Make function signatures from the type's defining module accessible without explicit import (auto-import of associated functions)

### Phase 2: Conflict detection

- Error message when same-named functions in stdlib and external modules both become UFCS candidates
- Show hint that explicit module prefix can resolve the ambiguity

## Motivation

Directly impacts web framework DX:

```almide
// before
let id = web.param(req, "id")
let page = web.query(req, "page")
let res = web.add_header(web.json(data), "X-Request-Id", req_id)

// after
let id = req.param("id")
let page = req.query("page")
let res = web.json(data).add_header("X-Request-Id", req_id)
```

## Depends On

- Module system can track type definitions within modules (already implemented)
- Named type's defining module information is accessible from the checker
