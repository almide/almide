# 標準ライブラリ拡充ロードマップ

## 目的
AI生成コードのボイラープレートを削減し、LOC・トークン数・生成時間を改善する。

## ベンチマーク分析結果
minigit (3試行) と miniconf (1試行) のAI生成コードを分析した結果:
- **文字分類**: AI が `is_digit`, `is_alpha` を毎回 60文字リストで自作（40-60 LOC）
- **文字列の文字分解**: `list.filter(string.split(s, ""), ...)` パターンが10箇所以上
- **マジックナンバーslice**: `string.slice(line, 8)` で "parent: " をスキップ等
- **match get 冗長**: `match list.get(xs, i) { some(v) => v, none => "" }` が64箇所
- **手動indexedループ**: `var i = 0; do { guard i < len; ... }` が5-7箇所

## Phase 1: 文字列・文字操作（LOC -40〜60）

### 1.1 string.chars
```almide
string.chars("abc") → ["a", "b", "c"]   (* UTF-8文字単位の分解 *)
```
- 現状 AI は `list.filter(string.split(s, ""), fn(c) => string.len(c) > 0)` で代替
- UFCS: `s.chars()`

### 1.2 string.index_of
```almide
string.index_of("hello world", "world") → some(6)
string.index_of("hello", "xyz") → none
```
- パーサー的なコードで頻出。`string.slice` と組み合わせて使う

### 1.3 string.repeat
```almide
string.repeat("ab", 3) → "ababab"
```

### 1.4 string.from_bytes
```almide
string.from_bytes([104, 105]) → "hi"
```
- `string.to_bytes` の逆

### 1.5 char 判定関数
```almide
string.is_digit?("3") → true       (* 0-9 *)
string.is_alpha?("a") → true       (* a-zA-Z *)
string.is_alphanumeric?("a") → true
string.is_whitespace?(" ") → true  (* space, tab, newline *)
```
- `string` モジュールに配置（charモジュールは作らない、単一文字Stringで判定）
- AI が毎回 `["0","1",...,"9"]` を書くのを防ぐ
- UFCS: `c.is_digit?()`

## Phase 2: リスト操作拡充（LOC -20〜30）

### 2.1 list.enumerate
```almide
list.enumerate(["a", "b", "c"]) → [(0, "a"), (1, "b"), (2, "c")]
```
- 手動 `var i = 0` ループの削減
- タプルとして `(Int, T)` を返す

### 2.2 list.zip
```almide
list.zip([1, 2], ["a", "b"]) → [(1, "a"), (2, "b")]
```

### 2.3 list.flatten
```almide
list.flatten([[1, 2], [3], [4, 5]]) → [1, 2, 3, 4, 5]
```

### 2.4 list.take / list.drop
```almide
list.take([1, 2, 3, 4], 2) → [1, 2]
list.drop([1, 2, 3, 4], 2) → [3, 4]
```

### 2.5 list.sort_by
```almide
list.sort_by(users, fn(u) => u.name)
```
- 現状 `list.sort` は自然順序のみ

### 2.6 list.unique
```almide
list.unique([1, 2, 2, 3, 1]) → [1, 2, 3]
```

## Phase 3: ユーティリティモジュール（新規）

### 3.1 math モジュール
```almide
import math

math.min(3, 5) → 3
math.max(3, 5) → 5
math.abs(-3) → 3
math.pow(2, 10) → 1024
math.pi → 3.14159...
math.e → 2.71828...
math.sin(x) math.cos(x) math.log(x) math.exp(x) math.sqrt(x)
```
- `math.min`/`math.max` は Int 版。Float 版は `float.min`/`float.max` にするか要検討
- auto-import 対象外（`import math` 必要）

### 3.2 random モジュール
```almide
import random

random.int(1, 100) → 42          (* min..max inclusive *)
random.float() → 0.7234          (* 0.0..1.0 *)
random.choice(["a", "b", "c"]) → "b"
random.shuffle([1, 2, 3]) → [3, 1, 2]
```
- effect fn（非決定的）
- Rust: `rand` crate 不使用、`getrandom` or 自前 xorshift
- auto-import 対象外

### 3.3 time モジュール
```almide
import time

time.now() → 1709913600          (* unix timestamp, env.unix_timestamp の移動 *)
time.sleep(1000)                 (* ms *)
```
- `env.unix_timestamp` との互換は alias で対応

## Phase 4: 正規表現（最大インパクト）

### 4.1 regex モジュール
```almide
import regex

regex.match?("[0-9]+", "abc123") → true
regex.find("[0-9]+", "abc123def") → some("123")
regex.find_all("[0-9]+", "a1b22c333") → ["1", "22", "333"]
regex.replace("[0-9]+", "a1b2", "X") → "aXbX"
regex.split("[,;]", "a,b;c") → ["a", "b", "c"]
```
- Rust: 自前の基本正規表現エンジン（依存ゼロ方針）
- 対応: `.` `*` `+` `?` `[]` `[^]` `\d` `\w` `\s` `^` `$` `|` `()`
- 最も LOC 削減効果が高い（miniconf のパーサーコードが半減する可能性）
- ただし実装コストも最大

## 実装順序

```
Phase 1 (string/char)  →  ベンチ計測  →  Phase 2 (list)  →  ベンチ計測  →  Phase 3/4
```

### Phase 1 の優先順
1. `string.chars` — 即効性が高い（10箇所の `split+filter` 置換）
2. `string.is_digit?` / `string.is_alpha?` — 文字分類ボイラープレート排除
3. `string.index_of` — パーサーコード簡素化
4. `string.repeat` — 低コスト、あると便利
5. `string.from_bytes` — `to_bytes` の対称性

### 各 Phase の作業
1. `stdlib.rs` に型シグネチャ追加
2. `emit_rust/calls.rs` に Rust codegen 追加
3. `emit_ts/expressions.rs` に TS codegen 追加（該当する場合）
4. `stdlib.rs` の `resolve_ufcs_module` に UFCS 追加
5. `exercises/stdlib-test/` にテスト追加
6. 全 exercise 通過確認

### 見積もり LOC 削減効果
| Phase | AI生成コード削減 | ベンチ影響 |
|-------|-----------------|-----------|
| Phase 1 | -40〜60 LOC/task | トークン -15〜20% |
| Phase 2 | -20〜30 LOC/task | トークン -5〜10% |
| Phase 3 | 新機能（削減ではなく拡張） | — |
| Phase 4 | -50〜80 LOC/task | トークン -20〜30% |

## 原則
- auto-import: Phase 1, 2 は既存モジュール（string, list）への追加なので auto-import 済み
- Phase 3, 4 は新モジュールなので `import` 必要
- 依存ゼロ方針を維持（regex も自前実装）
- 推測で足さない、ベンチで効果を計測してから次へ進む
