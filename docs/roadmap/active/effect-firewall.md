<!-- description: test + effect — intercept effect calls in tests for deterministic testing -->
# Effect Firewall

> **Target: v0.23+**
> **Status: Requirements confirmed, syntax converging**

## Problem

`effect fn` that performs IO cannot be unit tested. No way to intercept `http.get`, `net.tcp_connect`, etc. without running a real server.

## Hard requirements

1. **Test 内で effect の返り値を指定できる** — ネットワーク不要で effect fn をテスト
2. **未指定の effect はテスト失敗** — 暗黙の外部通信を防ぐ（デフォルト deny）
3. **新キーワード最小** — 既存構文（match arm, effect）の再利用
4. **「mock」という語を使わない** — match arm で値を返すだけ

## Syntax

### Per-test effect block

```almide
test "auth parses profile" {
  effect {
    http.get(url) => match url {
      "https://login.live.com/token" => ok("{\"access_token\":\"abc\"}"),
      "https://api.minecraftservices.com/profile" => ok("{\"name\":\"Steve\"}"),
      _ => err("unexpected: ${url}"),
    },
    fs.read(path) => ok(bytes.from_string("cached")),
    _ => deny,
  }

  let profile = authenticate()!
  assert_eq(profile.name, "Steve")
}
```

- `effect { arms }` — テストブロック冒頭で宣言。以降のコードに適用
- パターン変数（`url`, `path`）は closure スコープで束縛
- `_ => deny` — 未宣言の effect はテスト失敗
- `allow` — 本物の実装を実行

### File-level effect (全テスト共通)

```almide
// ファイル冒頭: このファイルの全テストに適用
effect {
  http.get(url) => match url {
    "https://login.live.com/token" => ok("{\"access_token\":\"abc\"}"),
    _ => err("not mocked: ${url}"),
  },
  _ => deny,
}

test "auth flow" {
  let profile = authenticate()!
  assert_eq(profile.name, "Steve")
}

test "token refresh" {
  let token = refresh()!
  assert(string.len(token) > 0)
}
```

テストの外に `effect { ... }` を書くとファイル全体に適用。per-test の `effect` block があれば上書き（override）。

### Per-test override

```almide
// File-level default
effect {
  http.get(_) => ok("default response"),
  _ => deny,
}

test "normal flow" {
  // file-level effect が使われる
  let resp = fetch()!
  assert_eq(resp, "default response")
}

test "error case" {
  // この test だけ上書き
  effect {
    http.get(_) => err("connection refused"),
  }

  match fetch() {
    ok(_) => assert(false),
    err(e) => assert(string.contains(e, "refused")),
  }
}
```

### Sequential responses

```almide
test "retry logic" {
  effect {
    http.get(_) => [err("timeout"), err("timeout"), ok("success")],
  }

  let result = fetch_with_retry(3)!
  assert_eq(result, "success")
}
```

リストリテラル: 呼び出し順に返す。末尾要素は最後以降繰り返し。

## Scoping rules

| Scope | 宣言場所 | 適用範囲 | Override |
|---|---|---|---|
| **Per-test** | `test "..." { effect { ... } ... }` | そのテストのみ | — |
| **File** | ファイルトップレベル `effect { ... }` | 同ファイルの全テスト | per-test で上書き可 |

Global (プロジェクト全体) は `almide.toml` の `[permissions]` が既にカバー。テスト固有の global は `spec/setup.almd` のような convention file で対応可能（将来）。

## Beyond tests: expression-level sandboxing

```almide
fn run_plugin(plugin: Plugin) -> Value = {
  effect {
    fs.read(_) => allow,
    net.*      => deny,
    _          => allow,
  }
  plugin.execute()!
}
```

テストと同じ構文でプロダクションコードのサンドボックスにも使える。

## Design notes

- `effect` は既存キーワード（`effect fn`）の再利用。新キーワード追加ゼロ
- `allow` / `deny` はキーワードではなく組み込み値
- match arm 構文がそのまま。パターン変数で引数を受けて分岐可能
- `effect` block は文（statement）。以降のスコープに影響
- 構文の最終決定は MSR（LLM が最も正確に書ける形）で検証

## Prior art

| Language | Construct | New keywords | Expression-level |
|---|---|---|---|
| Koka | `with handle { ... }` | 1 | Yes |
| OCaml 5 | `match ... with effect ...` | 2 | Yes |
| Deno | `--allow-net` | 0 (CLI flags) | No |
| Jest | `jest.mock()` | 0 (function) | No |
| **Almide** | `effect { match arms }` | **0** | **Yes** |

## Implementation

### Phase 1: `effect` block in tests

1. Parser: `effect { arms }` as statement inside test blocks
2. Codegen: thread-local effect dispatch table
3. Runtime: intercept `effect fn` entry, check dispatch table

### Phase 2: File-level `effect`

1. Parser: top-level `effect { arms }` outside test blocks
2. Merge logic: file-level + per-test override

### Phase 3: Compile-time effect inference

1. Annotate each function with its effect set
2. Warn if effect block doesn't cover all effects in the body
3. Warn if effect arm is declared but never triggered
