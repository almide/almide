<!-- description: Hono-like web framework as an external package over stdlib http, with template and Codec integration -->
<a id="web-framework"></a>

# Web Framework

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](web-framework.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](web-framework.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](web-framework.zh-CN.md)

> Non-normative pinned historical reader; original unchanged and authoritative, no automatic sync. No quoted command, CI change, benchmark or current issue recheck was performed. Issue inventory counts and dated states are retained, including source inconsistencies: four distinct blockers vs a script total of seven and the #2400 explanation, and differing decision-blocked versus one-PR-fix classifications. These are not a current task list. Web APIs and deployment adapters are proposals; shipped prerequisites do not establish a shipped framework. Comparison/260KB claims are attributed historical source assertions, not independently remeasured. Code and old API spellings are preserved, not certified runnable. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/web-framework.md)

<a id="vision"></a>

## Vision

Almide's first-party web framework. Achieves Hono-equivalent DX with Almide's philosophy.

A proposed thin layer on top of stdlib's `http` primitives, to be provided as
an external library. The `web.*` API and deployment examples below are design
sketches, not evidence that this package or its target adapters are shipped.

<a id="status-2026-10-24-decision"></a>

## Status (2026-10-24 decision)

- **Placement: an external package, not the stdlib.** Stdlib `http` keeps the surface the stdlib redesign fixed (`route` / `router` / `mount` / `wrap` / `param` / `serve`, unchanged in [L2-http](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-http.md)). This layer sits on top of it and adds nothing to `stdlib/http.almd`.
- **Hono's look, Almide's semantics.** Keep the vocabulary models already know (`app |> get(...) |> post(...) |> use(...)`, `json` / `text` / `html` responders). Replace what depends on shared mutable state:
  - `c.set` / `c.get` becomes values passed with the request;
  - the `await next()` onion becomes middleware as `Handler -> Handler` (what `http.wrap` already composes);
  - a mutating `app` becomes a pipe-built value.
- **Updated prerequisite:** default parameters are implemented (`spec/lang/default_args_test.almd`). The package still needs to choose and test its JSON helper signatures; the original absence-of-defaults premise no longer blocks it.
- **When:** after the stdlib redesign waves and the v0.68.0 release.

<a id="design-principles"></a>

## Design Principles

1. **Immutable** — No mutable context. Pure transformation of `Request -> Response`
2. **Composed via pipe chains** — Almide's natural composition mechanism
3. **Template integration** — Return `HtmlDoc` directly
4. **Codec integration** — Type-safe request/response JSON via the existing `: Codec` declaration
5. **Targets** — Design for supported native/Wasm hosts; TypeScript output is retired and deployment adapters still need validation
6. **Thin** — Routing + middleware + response builder. Nothing more

<a id="api-design"></a>

## API Design

<a id="basic"></a>

### Basic

```almide
import web

let app = web.new()
  |> web.get("/", fn(req) => web.text("Hello"))
  |> web.get("/about", fn(req) => web.text("About"))

effect fn main() -> Result[Unit, String] = {
  web.serve(app, 3000)
}
```

<a id="routing"></a>

### Routing

```almide
let app = web.new()
  |> web.get("/users/:id", fn(req) => {
    let id = req.param("id")              // proposed request API using existing UFCS
    web.json(json.object([("id", json.s(id))]))
  })
  |> web.post("/users", fn(req) => {
    web.json_status(201, req.body)
  })
  |> web.get("/files/*", fn(req) => {
    web.text("File: ${req.wildcard()}")    // UFCS
  })
```

<a id="template-integration"></a>

### Template Integration

```almide
import web

type User = { name: String, email: String }

template user_page(user: User) -> HtmlDoc = html {
  head { title { user.name } }
  body {
    h1 { user.name }
    p { user.email }
  }
}

let app = web.new()
  |> web.get("/users/:id", fn(req) => {
    let user = find_user(req.param("id"))
    web.html(user_page(user))
  })
```

`web.html(HtmlDoc) -> Response` internally calls `render(doc)` and sets `Content-Type: text/html`.

<a id="codec-integration-using-the-existing--codec-declaration"></a>

### Codec Integration (using the existing `: Codec` declaration)

```almide
type CreateUser: Codec = { name: String, email: String }
type UserResponse: Codec = { id: String, name: String, email: String }

let app = web.new()
  |> web.post("/users", fn(req) => {
    let input = req.body_json[CreateUser]()
    let user = create_user(input)
    web.json(user)                            // auto encode via Codec
  })
```

<a id="middleware"></a>

### Middleware

Function composition. `Handler -> Handler`.

```almide
fn logger(next: web.Handler) -> web.Handler =
  fn(req) => {
    io.println("${req.method} ${req.path}")
    next(req)
  }

fn cors(origin: String) -> fn(web.Handler) -> web.Handler =
  fn(next) => fn(req) => {
    let res = next(req)
    match res {
      ok(r) => ok(r.add_header("Access-Control-Allow-Origin", origin)),
      err(e) => err(e),
    }
  }

let app = web.new()
  |> web.use(logger)
  |> web.use(cors("*"))
  |> web.get("/", fn(req) => web.text("Hello"))
```

<a id="route-grouping"></a>

### Route Grouping

```almide
let api = web.group("/api")
  |> web.get("/users", list_users)
  |> web.post("/users", create_user)

let pages = web.group("/")
  |> web.get("/", home_page)
  |> web.get("/about", about_page)

let app = web.new()
  |> web.use(logger)
  |> web.mount(api)
  |> web.mount(pages)
```

<a id="error-handling"></a>

### Error Handling

```almide
let app = web.new()
  |> web.get("/", fn(req) => web.text("Home"))
  |> web.on_error(fn(err, req) => {
    io.println("Error: ${err}")
    web.text_status(500, "Internal Server Error")
  })
  |> web.on_not_found(fn(req) =>
    web.html_status(404, error_page("Not Found", req.path))
  )
```

<a id="api-summary"></a>

## API Summary

```
// App lifecycle
web.new            : () -> App
web.serve          : (App, Int) -> Unit                     // effect

// Routing
web.get            : (App, String, Handler) -> App
web.post           : (App, String, Handler) -> App
web.put            : (App, String, Handler) -> App
web.delete         : (App, String, Handler) -> App

// Grouping
web.group          : (String) -> App
web.mount          : (App, App) -> App

// Middleware
web.use            : (App, fn(Handler) -> Handler) -> App

// Request (UFCS-able)
req.param          : (Request, String) -> String
req.query          : (Request, String) -> Option[String]
req.wildcard       : (Request) -> String
req.body_json[T]   : (Request) -> Result[T, String]         // Codec integration

// Response builders
web.text           : (String) -> Response
web.text_status    : (Int, String) -> Response
web.json           : (Json) -> Response                      // status 200 default
web.json_status    : (Int, Json) -> Response
web.html           : (HtmlDoc) -> Response                   // template integration
web.html_status    : (Int, HtmlDoc) -> Response
web.redirect       : (String) -> Response
web.redirect_status: (Int, String) -> Response

// Response mutation (UFCS-able)
res.add_header     : (Response, String, String) -> Response
res.status         : (Response, Int) -> Response

// Error handling
web.on_error       : (App, fn(String, Request) -> Response) -> App
web.on_not_found   : (App, fn(Request) -> Response) -> App
```

<a id="comparison-with-hono"></a>

## Comparison with Hono

| Aspect | Hono | Almide web |
|--------|------|-----------|
| Context | mutable `c` object | immutable `req` → `Response` |
| Middleware | `c.next()` onion model | `Handler -> Handler` function composition |
| Response | `c.text()`, `c.json()` | `web.text()`, `web.json()` (pure fn) |
| HTML | `c.html(string)` | `web.html(HtmlDoc)` — typed document |
| JSON | object literal | Type-safe via `: Codec` |
| Routing | `.get("/path", handler)` | `\|> web.get("/path", handler)` |
| Method access | `c.req.param()` | `req.param()` (UFCS) |

<a id="dx-gap-whats-needed-to-match-hono"></a>

## DX Gap: What's Needed to Match Hono

| Priority | Feature | Roadmap | Effect |
|----------|---------|---------|--------|
| **1** | `: Codec` (implemented) | [CHEATSHEET](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/CHEATSHEET.md) | Use generated encode/decode in the proposed response API |
| **2** | UFCS (implemented) | [Type system](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/type-system.md) | Design the package API to use current resolution rules |
| **3** | `(req) => ...` lambdas (implemented) | [CHEATSHEET](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/CHEATSHEET.md) | Use current lambda syntax |
| **4** | Default params (implemented) | `spec/lang/default_args_test.almd` | Choose and test the proposed helper signature |

With all of the above:

```almide
let app = web.new()
  |> get("/", (req) => text("Hello"))
  |> get("/users/:id", (req) => {
    let id = req.param("id")
    json(UserResponse { id: id, name: "Alice" })
  })
  |> post("/users", (req) => {
    let input = req.body_json[CreateUser]()
    json_status(201, create_user(input))
  })
```

<a id="patterns-to-learn-from-merjs-zig"></a>

## Patterns to Learn from merjs (Zig)

[merjs](https://github.com/justrach/merjs) achieves a Next.js equivalent in a single Zig binary (260KB). The following patterns are worth adopting for Almide:

<a id="file-based-routing"></a>

### File-Based Routing

Auto-generate route tables from directory structure.

```
app/
  index.almd          → /
  about.almd          → /about
  users/
    index.almd        → /users
    [id].almd         → /users/:id
api/
  users.almd          → /api/users (JSON API)
```

Achievable as a compiler extension in Almide — `almide build --web app/` scans the directory and generates route tables at the IR level. No separate codegen binary needed.

<a id="html-dsl"></a>

### HTML DSL

```almide
fn page(user: User) -> Html =
  html.div({class: "card"}, [
    html.h1({}, [html.text(user.name)]),
    html.p({}, [html.text(user.email)]),
  ])
```

Props are verified at type check time. `Html` type can be returned directly as response.

<a id="streaming-ssr"></a>

### Streaming SSR

```almide
effect fn render_stream(req: Request, w: Writer) -> Result[Unit, String] = {
  w.write(shell_head(meta))
  w.flush()
  let data = fetch_data(req.param("id"))  // Send shell ahead while fetching data asynchronously
  w.write(render_body(data))
  w.write(shell_tail())
  ok(())
}
```

Send shell ahead via chunked transfer encoding, stream the body after data arrives.

<a id="multi-target-deployment"></a>

### Multi-Target Deployment

Proposed deployment mapping; availability of a compiler target alone does not
establish host imports or platform deployment compatibility:

| Deploy target | Target | Notes |
|---------------|--------|-------|
| VPS / container | `--target rust` | Standalone binary |
| Wasm host / edge platform | `--target wasm` | Verify the host/import adapter and deployment constraints; not an automatic Workers compatibility claim |
| Deno Deploy / Bun | No TS output target | A separate supported-host integration would be needed |

The original three-target/merjs comparison is not a current supported-deployment
claim. Native and Wasm integrations need their own executable acceptance tests.

<a id="depends-on"></a>

## Depends On

- `http` stdlib (done)
- `template` (active — for `web.html`)
- `: Codec` (implemented; proposed `web.json` still needs package implementation)
- UFCS (implemented; design/test the proposed request helpers against current resolution)
- Package system (implemented Git/local resolution; external distribution need not wait for a registry)
- Directory traversal (shared with self-hosting roadmap)
- Streaming SSR integration remains proposed; existing `io.Write` is a foundation, not an implemented SSR adapter
- Session layer remains proposed; existing `hash.hmac_sha256` provides HMAC, not a complete session policy

<a id="implementation-order"></a>

## Implementation Order

1. **Phase 1**: Routing + Response builders + `web.serve` (wrapper for http.serve)
2. **Phase 2**: Middleware (`web.use`) + route groups (`web.mount`)
3. **Phase 3**: Template integration (`web.html`) + HTML DSL
4. **Phase 4**: Codec integration (`req.body_json[T]`, `web.json(record)`)
5. **Phase 5**: File-based routing (compiler extension)
6. **Phase 6**: Streaming SSR + multi-target deployment

<a id="position"></a>

## Position

- **Not stdlib** — Framework opinions don't belong in stdlib
- **First-party external lib** — Officially published by Almide
- Same relationship as Echo/Gin (external) to Go's `net/http` (stdlib)
- **Shares prerequisite features with self-hosting roadmap** — Web framework development serves as dogfooding that strengthens language features
