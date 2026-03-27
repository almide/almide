<!-- description: Extend UFCS resolution to external library functions -->
# UFCS for External Libraries [ACTIVE]

## Problem

UFCS は現在 stdlib にハードコードされている（`src/stdlib.rs` の `resolve_ufcs_candidates`）。外部ライブラリの関数は module prefix なしでは呼べない。

```almide
// 今: module prefix 必須
web.param(req, "id")
web.add_header(res, "X-Custom", "value")

// 欲しい: UFCS で method 風
req.param("id")
res.add_header("X-Custom", "value")
```

web framework のような API で DX を大きく損なうポイント。Hono の `c.req.param('id')` に対して `web.param(req, "id")` は冗長。

## Design

### 基本ルール

外部ライブラリの関数が以下の条件を満たすとき、UFCS 対象になる:

1. 第一引数が named type（record / variant）
2. 関数が第一引数の型と同じモジュール内で定義されている

```almide
// web/mod.almd
type Request = { method: String, path: String, headers: Map[String, String], body: String, params: Map[String, String] }

fn param(req: Request, name: String) -> String = ...
fn query(req: Request, name: String) -> Option[String] = ...

// 呼び出し側
import web

// 両方 OK
web.param(req, "id")     // 明示的
req.param("id")           // UFCS
```

### 解決方式

**Type-directed resolution**: receiver の型が分かっている場合、その型が定義されたモジュールの関数を探す。

```
req.param("id")
  → req の型は web.Request
  → web module に param(Request, String) がある
  → web.param(req, "id") に解決
```

これは今の stdlib UFCS（`resolve_ufcs_by_type`）の自然な拡張。stdlib のハードコードを、型定義元モジュールの自動探索に一般化するだけ。

### UFCS 対象にならないもの

- 第一引数が primitive 型（`String`, `Int` 等）の外部関数 → stdlib と衝突しうる
- 第一引数が open record / anonymous record → 型定義元モジュールが存在しない
- 異なるモジュールで同名関数が第一引数の型をまたぐ場合 → 曖昧性エラー

### stdlib との優先順位

1. stdlib の UFCS 候補を先に探す
2. 見つからなければ、receiver の型定義元モジュールを探す
3. 両方で見つかった場合 → コンパイルエラー（曖昧性）

## Implementation

### Phase 1: Type-directed module lookup

- checker が UFCS 呼び出しを解決するとき、receiver の型が named type なら型定義元モジュールを探す
- `resolve_ufcs_by_type` を拡張し、stdlib 以外のモジュールも探索対象にする
- 型定義元モジュールの関数シグネチャを import なしで参照可能にする（auto-import of associated functions）

### Phase 2: Conflict detection

- stdlib と外部モジュールで同名関数が UFCS 候補になった場合のエラーメッセージ
- 明示的な module prefix で曖昧性を解消できることを hint に表示

## Motivation

Web framework DX に直結:

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

- Module system がモジュール内の型定義を追跡できること（既に実装済み）
- Named type の定義元モジュール情報が checker から参照可能なこと
