<!-- description: Strategy for adding new codegen targets with minimal cost -->
# Multi-Target Strategy

## Vision

Almide のマルチターゲット設計が新しいターゲット言語の追加コストを最小化する構造になっていることを活かし、ターゲット言語の拡充戦略を定める。

---

## 設計の強み: なぜ新ターゲットが安く追加できるか

新ターゲットを足すときにやることは3つだけ:

1. **`runtime/{lang}/core.{ext}` を書く** — Result, Option の表現、例外の捕捉、型の変換規則（glue）
2. **`emit_{lang}/` をコンパイラに追加する** — IR → ターゲット言語ソースの codegen（一番大きい仕事）
3. **`stdlib/*/extern.{ext}` を追加する** — プラットフォーム依存モジュールだけ。純粋 Almide モジュールは何もしなくていい

**pure-first の効果**: 純粋モジュールが多いほど、新ターゲット追加コストが下がる。hash (189行), csv (70行), url (220行), path, encoding, args — これらは extern ファイルを書かずにコンパイラの .almd → ターゲット言語変換だけで動く。

---

## ターゲット一覧

### Tier 1: 現行（実装済み）

| ターゲット | 用途 | 状態 |
|---|---|---|
| **rust** | ネイティブバイナリ、WASM、性能重視 | ✅ 実装済み |
| **typescript** | Deno 実行、型付きソース出力 | ✅ 実装済み |
| **javascript** | Node/ブラウザ実行、TS の型なし版 | ✅ 実装済み |
| **wasm** | Rust 経由で .wasm 生成 | ✅ 実装済み（基本） |

### Tier 2: 高優先度候補

| ターゲット | 動機 | Almide との相性 | extern コスト |
|---|---|---|---|
| **python** | 巨大エコシステムへの埋め込み。pip install できる Almide ライブラリ | 高。動的型だが Result は dataclass/NamedTuple で表現可能。asyncio で async 対応 | 中。fs/http/json/regex は Python 標準ライブラリが充実 |
| **go** | サーバーサイドエコシステム。Go プロジェクトへの埋め込み | 高。型システムがシンプルで IR からの変換が現実的。error は Result に近い | 中。os/net/http は Go 標準が充実 |

### Tier 3: 将来候補

| ターゲット | 動機 | Almide との相性 | 備考 |
|---|---|---|---|
| **kotlin** | Android + JVM サーバー | 高。Result, sealed class があり型表現と相性良い | JVM バイトコード vs Kotlin ソースの選択がある |
| **swift** | iOS/macOS ネイティブ | 高。async/await の設計を Swift から借りているので変換先として自然 | Apple プラットフォーム限定の需要 |
| **ruby** | Rails エコシステムへの埋め込み | 中。動的型。Struct で Result 表現可能 | エコシステムの async が未成熟 |
| **c** | 組み込み、レガシーシステム連携 | 低〜中。pure 関数はほぼそのまま落とせるが、GC なし環境で List/String が難しい | メモリ管理戦略が必要 |

---

## 各ターゲットの glue + extern 例

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

## 新ターゲット追加の作業量見積もり

| 作業 | 行数目安 | 備考 |
|---|---|---|
| `runtime/{lang}/core.{ext}` | ~50行 | glue は薄い（Rule 7） |
| `emit_{lang}/` codegen | ~2000-4000行 | IR → ソースの変換。一番大きい |
| `stdlib/*/extern.{ext}` × ~10モジュール | ~500行 | プラットフォーム依存のみ |
| **純粋モジュール** | **0行** | **コンパイラが .almd → ターゲットに変換** |
| テスト | ~500行 | conformance test + extern test |
| **合計** | **~3000-5000行** | |

比較: 現行の Rust codegen は `emit_rust/` に ~3000行、TS codegen は `emit_ts/` に ~2000行。

---

## 優先度の判断基準

新ターゲットを追加する動機は3つに分類できる:

| 動機 | 例 | 判断基準 |
|---|---|---|
| **実行環境** | ネイティブ、ブラウザ、エッジ | その環境でしか動かせないか |
| **エコシステム統合** | Python/Go/Kotlin のプロジェクトに埋め込み | pip/go get/gradle で配布できるか |
| **ソース出力 (inspect)** | 生成コードを読みたい | `almide emit --lang X` で十分 |

**Rule 6 の確認**: コンパイラはデプロイ先を知らない。ターゲットは「言語/ランタイム」であって「プラットフォーム」ではない。

---

## ロードマップ

### Phase 0: 現行ターゲットの完成（CLI-First）

Rust + TS/JS で CLI ツールが完全に書ける状態にする。@extern + glue + Result 統一。ここが全ターゲットの基盤。

### Phase 1: Python ターゲット

- `runtime/py/core.py` — glue
- `emit_py/` — IR → Python codegen
- `stdlib/*/extern.py` — fs, io, env, process, http, json, regex
- `almide build app.almd --target py` → `.py` ファイル出力
- `almide emit app.almd --lang py` → Python ソース inspect
- 目標: `pip install` 可能なパッケージとして配布

### Phase 2: Go ターゲット

- `runtime/go/core.go` — glue
- `emit_go/` — IR → Go codegen
- `stdlib/*/extern.go` — os, net/http, encoding/json, regexp
- `almide build app.almd --target go` → `.go` ファイル出力
- Go のジェネリクス (1.18+) を活用

### Phase 3: Kotlin / Swift（需要に応じて）

- モバイル/デスクトップへの展開が見えたとき
- sealed class (Kotlin) / enum (Swift) で Result が自然に表現できる
- async/await も両言語にネイティブにある

### Phase 4: C（需要に応じて）

- 組み込み/レガシー連携
- pure 関数は落とせるが、List/String のメモリ管理戦略が必要
- arena allocator or reference counting の判断が要る

---

## Dependencies

- [Runtime Architecture Reform](stdlib-self-hosted-redesign.md) — @extern + glue の基盤
- [CLI-First](cli-first.md) — Phase 0（現行ターゲットの完成）

## Related

- [New Codegen Targets](new-codegen-targets.md) — 既存の codegen ターゲット roadmap（あれば）
