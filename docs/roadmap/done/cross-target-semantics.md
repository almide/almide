<!-- description: Fix cases where same .almd produces different results on Rust vs TS -->
# Cross-Target Semantics

同じ `.almd` が Rust と TS で異なる結果を出すケースの修正。Almide の「同じコードが両方で動く」前提を保証する。

## P0: Map の深層比較が TS で壊れている

**問題:** `__deep_eq` が `Object.keys()` を使うため、`Map` オブジェクトは空に見える。

```typescript
// emit_ts_runtime.rs — __deep_eq
const ka = Object.keys(a), kb = Object.keys(b);  // Map には効かない
```

`Map` に対して `Object.keys()` は `[]` を返すため、全ての Map 比較が `true`（両方空）になる。

**修正:**
- [x] `__deep_eq` に `Map` 判定を追加: `if (a instanceof Map && b instanceof Map)`
- [x] サイズ比較 → エントリ毎の再帰比較
- [ ] テスト: `assert_eq` で Map を含むケースを Rust/TS 両方で検証

## P0: Map の entries() 順序が Rust で非決定的

**問題:** Rust の `HashMap` はイテレーション順序が非決定的。`map.entries()` がソートなしで返される。

```rust
// collection_runtime.txt
fn almide_rt_map_entries<K, V>(map: &HashMap<K, V>) -> Vec<(K, V)> {
    map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()  // ソートなし
}
```

TS の `Map` は挿入順を保証する。同じプログラムが Rust で実行ごとに違う順序を返す。

**修正:**
- [x] `almide_rt_map_entries` でキーソートを追加（`map.keys()` と同じパターン）
- [ ] `for (k, v) in map` のイテレーション順もソート済みにする
- [ ] テスト: `map.entries()` の結果が Rust/TS で一致することを検証

## P1: 整数オーバーフローの挙動差

**問題:** Rust は i64 wrapping、TS は BigInt（無限精度）または Number（53bit float）。

```almide
let x = 9223372036854775807  // i64::MAX
let y = x + 1               // Rust: wraps to -9223372036854775808, TS: 9223372036854775808n
```

**選択肢:**
- (A) ドキュメントで明記（「オーバーフロー時の挙動はターゲット依存」）
- (B) TS でも i64 範囲の wrapping を模倣（`BigInt.asIntN(64, value)`）
- (C) コンパイル時にリテラルが i64 範囲を超える場合に warning

**修正:**
- [x] 方針決定: (B) TS でも i64 wrapping を模倣
- [x] TS codegen で BigInt 演算後に `BigInt.asIntN(64, result)` を挿入（`__bigop`, `__div`）
- [ ] テスト: オーバーフロー境界のクロスターゲットテスト

## P1: Float 文字列化精度の差

**問題:** Rust の `Display` trait と JS の `.toString()` で Float の文字列表現が異なる。

```almide
let f = 0.1 + 0.2
println("{f}")  // Rust: "0.30000000000000004", TS: "0.30000000000000004" (通常一致するが保証なし)
```

極端に大きい/小さい値で差が出る。

**修正:**
- [x] 方針: 明示的なフォーマット関数 `float.format(f, 6)` を推奨、暗黙の文字列化は「近似的」とドキュメント（下記注記参照）
- [ ] テスト: 基本的な float 値の文字列補間がクロスターゲットで一致することを検証

**注記:** Float の暗黙文字列化（`"{f}"` 等）は Rust の `Display` と JS の `.toString()` に依存するため、極端な値（very large/small, subnormals）で差異が出る可能性がある。精密なフォーマットが必要な場合は `float.format(f, precision)` を使用すること。通常の値（`0.1 + 0.2` 等）は IEEE 754 準拠により両ターゲットで一致する。

## P2: Map の assert_eq 表示が TS で空

**問題:** `assert_eq` が `JSON.stringify` を使うが、Map は `"{}"` になる。

**修正:**
- [ ] Map 用のカスタム stringify を追加（エントリを列挙）
- [ ] `__deep_eq` のエラーメッセージに実際の値を表示

## P2: Result エラー値の TS test ブロック内消失

**問題:** test ブロック内の try-catch で `__Err` をラップし直す際、元のエラー構造が失われる。

```typescript
catch (__e) { x = new __Err(__e.message); }  // 元の値が消える
```

**修正:**
- [ ] `__e.__almd_value` を保持して re-wrap
- [ ] テスト: ネストした Result の match が TS テストで正しく動くことを検証

## P2: Map の keys() ソートが TS で非文字列キーに壊れる

**問題:** `[...m.keys()].sort()` は文字列比較。オブジェクトキーの場合、ソートが不定。

**修正:**
- [ ] カスタム comparator を追加（型に応じた比較）
- [ ] または: Map キーは常にプリミティブ型であることをコンパイル時に保証（checker で検証済みだが codegen でも防御）

## テスト戦略

クロスターゲットの意味論保証には、**同じテストを Rust と TS の両方で実行し結果を比較する** CI パイプラインが必要。

- [ ] `almide test --target rust` と `almide test --target ts` の結果比較スクリプト
- [ ] 差異があればテスト失敗
- [ ] 最低限: spec/stdlib/ と spec/lang/ の全テストを両ターゲットで実行
