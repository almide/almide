<!-- description: with effect — intercept, mock, and sandbox effects -->
# Effect Firewall

> **Target: v0.23+**
> **Status: Requirements confirmed, syntax open**

## Problem

`effect fn` that performs IO cannot be unit tested. No way to intercept `http.get`, `net.tcp_connect`, etc. without running a real server.

## Solution: `with effect` + match arms

One construct. One new keyword pair. The body is match arms — no new syntax to learn.

### Test usage

```almide
test "auth parses profile" with effect {
  http.get(_) => ok("{\"name\":\"Steve\",\"id\":\"abc123\"}"),
  time.now    => allow,
  _           => deny,
} {
  let profile = authenticate()!
  assert_eq(profile.name, "Steve")
}
```

### How it reads

- `http.get(_) => ok("...")` — intercept, return mock value (it's a match arm)
- `time.now => allow` — let the real implementation run (`allow` is a built-in value)
- `_ => deny` — everything else is blocked (`deny` is a built-in value)

### New keywords: 0

| Token | Status | Role |
|---|---|---|
| `with` | **Existing** (spread record `{ ...x, field: val }`) | Context keyword before `effect` |
| `effect` | **Existing** (`effect fn`) | Reused in `with effect` |
| `allow` | **Built-in value** (like `none`, `ok`) | Permit real execution |
| `deny` | **Built-in value** (like `none`, `err`) | Block execution, fail with error |

The match arm syntax (`pattern => expr`) is already part of the language. No new constructs needed.

### Pattern matching on arguments

```almide
test "router" with effect {
  http.get(url) => match url {
    "https://api.example.com/users" => ok("[]"),
    _ => err("unexpected: ${url}"),
  },
  _ => deny,
} {
  let users = fetch_users()!
  assert_eq(list.len(users), 0)
}
```

### Sequential mock (ordered responses)

```almide
test "retry" with effect {
  http.get(_) => [err("timeout"), err("timeout"), ok("success")],
  _ => deny,
} {
  let result = fetch_with_retry(3)!
  assert_eq(result, "success")
}
```

List literal: returns values in order, repeats last after exhaustion.

### Beyond tests: expression-level sandboxing

```almide
// Restrict a plugin to read-only filesystem
let result = with effect {
  fs.read(_) => allow,
  fs.*        => deny,
  net.*       => deny,
  _           => allow,
} {
  plugin.execute()!
}
```

### Default behavior

- `with effect { ... }` without `_ =>` arm: unmatched effects pass through (implicit `_ => allow`)
- `_ => deny` makes it a sandbox — explicit opt-in only
- Tests should always include `_ => deny` for determinism

## Relationship to existing features

| Scope | Mechanism | When |
|---|---|---|
| Package | `[permissions]` in almide.toml | Build time |
| Expression | `with effect { ... } { ... }` | Runtime |
| Test | `test "..." with effect { ... } { ... }` | Test time |

All three use the same mental model: declare which effects are permitted.

## Implementation

### Phase 1: `test ... with effect`

1. Parser: `test` + `with effect` + match arms + body
2. Codegen: thread-local mock dispatch table, intercept before each effect call
3. Runtime: check table on every `effect fn` entry

### Phase 2: `with effect` expression

1. Parser: `with effect { arms } { body }` as expression
2. Effect inference: track which effects `body` may call
3. Compile-time warning: mock declared but never triggered

### Phase 3: Compile-time verification

1. Effect inference pass: annotate each function with its effect set
2. Static check: `_ => deny` + function calls effect not in arms → compile error
3. Auto-doc: generate effect dependency list per function

## Hard requirements

1. **Test 内で effect の返り値を指定できる** — ネットワーク不要で effect fn をテスト
2. **未指定の effect はテスト失敗** — 暗黙の外部通信を防ぐ（デフォルト deny）
3. **新キーワード最小** — 既存構文（match arm, effect）の再利用
4. **「mock」という語を使わない** — match arm で値を返すだけ。テスト用語の mock/stub/spy の区別は不要

## Syntax candidates (open)

構文は未確定。実戦（mc-bot, mc-auth のテスト）で「ここで欲しい」が溜まってから決める。

```almide
// Candidate A: effect block as statement
test "auth" {
  effect {
    http.get(_) => ok("{\"name\":\"Steve\"}"),
    _ => deny,
  }
  let profile = authenticate()!
  assert_eq(profile.name, "Steve")
}

// Candidate B: test + effect clause
test "auth" effect {
  http.get(_) => ok("{\"name\":\"Steve\"}"),
  _ => deny,
} {
  let profile = authenticate()!
  assert_eq(profile.name, "Steve")
}

// Candidate C: with effect (Koka-style)
test "auth" with effect {
  http.get(_) => ok("{\"name\":\"Steve\"}"),
  _ => deny,
} {
  ...
}
```

決定基準: LLM が最も正確に書ける構文を選ぶ（MSR で検証）。

## Prior art

| Language | Construct | Keywords added | Expression-level |
|---|---|---|---|
| Koka | `with handle { ... }` | 1 (`handle`) | Yes |
| OCaml 5 | `match ... with effect ...` | 2 | Yes |
| Deno | `--allow-net` | 0 (CLI flags) | No |
| Go | `interface` + DI | 0 | No |
| **Almide** | `with effect { ... }` | **0** | **Yes** |

Zero new keywords. Maximum reuse of existing syntax.
