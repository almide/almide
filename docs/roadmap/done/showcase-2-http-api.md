<!-- description: Showcase: REST API server with http.serve, json, and codec -->
<!-- done: 2026-03-18 -->
# Showcase 2: Todo API (HTTP API)

**Domain:** HTTP API server
**Purpose:** REST API. Practical example of http.serve + json + codec.

## Specification

```
almide run showcase/todo-api.almd
# GET  /todos       → list all
# POST /todos       → create
# GET  /todos/:id   → get one
# DELETE /todos/:id → delete
```

- In-memory store (Map)
- JSON request/response
- I/O separation via effect fn

## Features Used

- `http.serve`, `http.response`, `http.json`
- `json.parse`, `json.stringify`
- `map.get`, `map.set`, `map.remove`
- `effect fn`
- `match` (request routing)
- `string.starts_with`, `string.split`

## Success Criteria

- [ ] Works on Tier 1 (Rust)
- [ ] CRUD operations possible with curl
- [ ] Under 80 lines
- [ ] Usage documented in README
