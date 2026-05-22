<!-- description: Effect firewall — mock/allow/deny effects in tests and sandboxes -->
# Effect Firewall

> **Target: v0.23**
> **Status: Design**

## Problem

`effect fn` that performs IO (HTTP, TCP, filesystem) cannot be unit tested. The function directly calls `http.get`, `net.tcp_connect`, etc. — there's no way to intercept these calls in tests without running a real server.

```almide
// Untestable — always hits the network
effect fn authenticate() -> Profile = {
  let resp = http.get("https://login.live.com/...")!
  parse_profile(resp)
}
```

Every language solves this differently: Go uses interfaces + DI, Rust uses traits + mock crates, Haskell uses free monads. All require restructuring production code for testability. Almide should solve this at the language level.

## Solution: `effects` block

A firewall that controls which effects a block of code can perform.

```almide
test "auth parses profile" effects {
  default deny

  mock http.get(_) => ok("{\"name\":\"Steve\",\"id\":\"abc123\"}")
  allow time.now
  deny tcp.connect
} {
  let profile = authenticate()!
  assert_eq(profile.name, "Steve")
}
```

### Semantics

| Directive | Meaning |
|---|---|
| `default deny` | All effects blocked unless explicitly allowed/mocked. Calling an unhandled effect fails the test with a clear error listing the missing effect. |
| `default allow` | All effects permitted unless explicitly denied. For integration tests that need real IO. |
| `mock module.fn(pattern) => expr` | Intercept calls to `module.fn`, return `expr` instead. Pattern matching on arguments. |
| `allow module.fn` | Permit the real implementation to run. |
| `deny module.fn` | Block the effect. Calling it fails immediately with an error. |

### Default for tests

`default deny` is the default when `effects` block is present. Tests are deterministic by default.

### Pattern matching in mocks

```almide
test "router" effects {
  default deny
  mock http.get(url) => match url {
    "https://api.example.com/users" => ok("{\"users\":[]}")
    _ => err("unexpected URL: ${url}")
  }
} {
  let users = fetch_users()!
  assert_eq(list.len(users), 0)
}
```

### Multiple calls

```almide
test "retry logic" effects {
  default deny
  mock http.get(_) => [err("timeout"), err("timeout"), ok("success")]
} {
  let result = fetch_with_retry(3)!
  assert_eq(result, "success")
}
```

Sequence syntax: mock returns values in order. After exhaustion, repeats the last value.

## Beyond tests: sandboxing

The `effects` block works outside tests too:

```almide
// Sandbox untrusted code
let result = with effects { default deny; allow fs.read } {
  process_file("data.txt")!
}

// Capability-restricted plugin execution
effect fn run_plugin(plugin: Plugin) -> Value = with effects {
  default deny
  allow time.now
  allow math.*
  deny fs.*
  deny net.*
} {
  plugin.execute()!
}
```

### Compile-time verification

When the compiler can statically determine which effects a function calls (via effect inference), it can:
1. Warn if a `mock` is declared but never triggered
2. Error if an effect is called but neither mocked nor allowed
3. Generate documentation of effect dependencies per function

### Relationship to `[permissions]` in almide.toml

`almide.toml` already has `[permissions]` for package-level capability control. The `effects` block is the expression-level equivalent:

| Scope | Mechanism | Checked at |
|---|---|---|
| Package | `[permissions]` in almide.toml | Build time |
| Expression | `with effects { ... } { ... }` | Compile time + Runtime |
| Test | `test "..." effects { ... } { ... }` | Test time |

## Implementation

### Phase 1: Test-only `effects` block

1. **Parser**: New `test` variant with `effects` block before body
2. **Checker**: Validate mock patterns against known effect signatures
3. **Lowering**: Generate mock dispatch table as IR
4. **Codegen (Rust)**: Emit thread-local mock registry, intercept RuntimeCall dispatch

### Phase 2: `with effects` expression

1. **Parser**: `with effects { directives } { body }` as expression
2. **Checker**: Effect inference — track which effects `body` may call
3. **Codegen**: Runtime capability check before each effect call

### Phase 3: Compile-time effect verification

1. **Effect inference pass**: Annotate each function with its effect set
2. **Static checking**: Verify `effects` block covers all possible effects
3. **Documentation generation**: Auto-generate effect dependency docs

## Prior art

| Language | Mechanism | Limitation |
|---|---|---|
| Koka | Algebraic effect handlers | Requires effect types in every signature |
| OCaml 5 | Effect handlers | Runtime-only, no compile-time checking |
| Deno | `--allow-net` flags | CLI-only, not expression-level |
| WASM Component Model | Capability imports | Module-level, not expression-level |
| Almide | `effects` block | Expression-level, compile-time verifiable |

Almide's design is unique: effect control at the expression level with compile-time verification. No other language combines mock + allow + deny in a single construct.
