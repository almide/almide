<!-- description: Rewrite stdlib in Almide with a 3-layer architecture -->
# Stdlib in Almide: Unified Library Architecture

**目標**: stdlib を Almide で書き直し、userlib と同じ仕組みにする。全ライブラリが 3 層構造で動く。
**現状**: 381 関数 × 2 ターゲット（Rust/TS）を手書きで維持。新ターゲット追加コストが極大。
**効果**: 新ターゲット追加時に書くのはプリミティブ 20-30 個だけ。stdlib/userlib の区別が消える。

---

## Why

### 現状の問題

```
stdlib/defs/list.toml     → 関数シグネチャ定義
runtime/rust/list.rs      → Rust 実装（手書き）
runtime/ts/list.ts        → TS 実装（手書き）
```

- 381 関数 × N ターゲットの手書き維持
- Rust と TS で実装が微妙に乖離するリスク
- 新ターゲット（Go, Python 等）追加時に 381 関数を全部書き直す必要がある
- stdlib と userlib が完全に別の仕組みで動いている

### あるべき姿

```
stdlib/list.almd          → Almide で実装（1 ソース）
  ↓ コンパイル
IR に含まれる（ユーザーコードと同列）
  ↓ codegen
各ターゲットのコードが自動生成される
```

## 3 層アーキテクチャ

### Layer 1: プリミティブ（ターゲット別、必須）

Almide では書けない最低限の操作。各ターゲット 20-30 個。

```
println, eprintln          — 出力
math.sin, math.cos, ...   — 数学関数（ハードウェア命令）
list.alloc, list.get_raw   — メモリ操作
string.len_bytes           — バイト長
fs.read_raw, fs.write_raw  — ファイル I/O
random.int_raw             — 乱数生成
```

codegen がターゲットごとに提供する：
- Rust: `println!()`, `f64::sin()`, `Vec::new()`, ...
- TS: `console.log()`, `Math.sin()`, `[]`, ...
- WASM: `fd_write`, WASI imports, linear memory ops, ...

### Layer 2: Almide 実装（共通、デフォルト）

stdlib の大部分。Almide で書かれ、全ターゲットで自動的に動く。

```almide
fn map(xs: List[A], f: fn(A) -> B) -> List[B] = {
  var result: List[B] = []
  for x in xs {
    result = result + [f(x)]
  }
  result
}

fn filter(xs: List[A], f: fn(A) -> Bool) -> List[A] = {
  var result: List[A] = []
  for x in xs {
    if f(x) then { result = result + [x] } else ()
  }
  result
}

fn join(xs: List[String], sep: String) -> String = {
  var result = ""
  for (i, x) in list.enumerate(xs) {
    if i > 0 then { result = result + sep } else ()
    result = result + x
  }
  result
}
```

### Layer 3: ターゲット最適化（オプション、オーバーライド）

性能が重要な関数だけ、ターゲットネイティブ実装で上書き。

```almide
// デフォルト実装（Layer 2）
fn sort(xs: List[Int]) -> List[Int] = {
  // マージソート等
}

// ターゲット最適化（Layer 3）
@native(rust, "vec_sort")     // Rust: Vec::sort()（pdqsort）
@native(ts, "array_sort")     // TS: Array.sort()（TimSort）
// WASM: オーバーライドなし → Layer 2 の Almide 実装
```

## stdlib = userlib

この仕組みは stdlib 専用ではない。userlib も全く同じ。

```almide
// ユーザーが書いたライブラリ
fn hash(data: String) -> String = {
  // Almide でのフォールバック実装
}

@native(rust, "ring_digest")   // Rust: ring crate
@native(ts, "node_crypto")     // TS: Node crypto
// WASM: Almide 実装
```

**stdlib は「最初から入っている userlib」に過ぎない。**

## IR への影響

### 現状

```json
{
  "kind": "call",
  "target": { "kind": "module", "module": "list", "func": "sort" }
}
```

stdlib 呼び出しは未解決の参照。codegen がランタイムを注入する。

### 移行後

```json
{
  "functions": [
    { "name": "list.sort", "body": { "..." }, "native_override": { "rust": "vec_sort", "ts": "array_sort" } },
    { "name": "list.map", "body": { "..." } },
    { "name": "main", "body": { "..." } }
  ],
  "primitives": ["println", "math.sin", "list.alloc"]
}
```

- stdlib の関数は `functions` にユーザーコードと同列で入る
- `native_override` があればターゲット別に差し替え
- `primitives` だけが codegen の責務

## codegen 分離との関係

この設計により、外部 codegen ツールが持つべきものが最小化される：

```
IR (JSON)                    → 全関数の実装（stdlib 含む）
+ primitives（20-30 個/ターゲット）  → ターゲット固有の最低限の実装
= 完全な出力
```

新ターゲット追加時：
- **今**: 381 関数を全部書く
- **移行後**: プリミティブ 20-30 個 + 性能重要な関数の @native 数十個

## Phases

### Phase 1: @native メカニズム

- [ ] `@native(target, impl)` 属性のパーサー・チェッカー対応
- [ ] IR に `native_override` フィールド追加
- [ ] codegen で override がある場合にネイティブ実装を選択

### Phase 2: プリミティブ定義

- [ ] Layer 1 プリミティブの洗い出し（目標: 各ターゲット 30 個以下）
- [ ] プリミティブを IR の `primitives` として明示的に扱う
- [ ] codegen のプリミティブ実装（Rust / TS / WASM）

### Phase 3: stdlib 移行（段階的）

優先度順に Almide で書き直す:

| 優先度 | モジュール | 関数数 | 理由 |
|--------|-----------|--------|------|
| 1 | option | 12 | 純粋ロジック、プリミティブ不要 |
| 2 | result | 15 | 同上 |
| 3 | list | 45 | map/filter/fold 等は Almide で書ける。sort は @native |
| 4 | string | 35 | 多くは list 操作に帰着。len/chars はプリミティブ |
| 5 | map | 20 | 内部データ構造の設計が必要 |
| 6 | set | 20 | map に依存 |
| 7 | math | 25 | ほぼ全部プリミティブ（sin, cos, ...） |
| 8 | int, float | 20 | parse はプリミティブ、算術は言語組込み |
| 9 | json | 23 | パーサーを Almide で書く |
| 10 | io, fs | 15 | ほぼ全部プリミティブ |
| 11 | http | 20 | ターゲット差異が大きい |
| 12 | その他 | ~150 | datetime, regex, crypto, ... |

### Phase 4: userlib 統合

- [ ] ユーザー定義モジュールでも `@native` が使えることを確認
- [ ] `almide pack --target ts` で npm パッケージ構造を生成
- [ ] `almide pack --target rust` で crate 構造を生成

## Success Criteria

- stdlib の 80% 以上が Almide で記述されている
- 新ターゲット追加時にプリミティブ + @native のみで全 stdlib が動作する
- IR が自己完結（primitives 以外の外部依存なし）
- userlib が stdlib と同じ仕組みで動作する
- `almide pack` でターゲットエコシステムのパッケージを出力できる

## 他言語からの学び

| 言語 | stdlib 戦略 | Almide が取り入れるもの |
|------|------------|----------------------|
| Gleam | 自言語 + @external FFI | Layer 1-2 の分離、@external の宣言的 FFI |
| Kotlin | expect/actual | Layer 3 の @native オーバーライド |
| ReScript | 最小 stdlib + external | ランタイム最小化の思想 |
| Haxe | 自言語で記述 | 1 ソースから全ターゲット生成 |
