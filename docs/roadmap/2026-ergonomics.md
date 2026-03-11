# 2026 Ergonomics Roadmap

Self-tooling (Chrome extension, TextMate generator, Playground modules) で発見された
エルゴノミクス課題を、SPEC/DESIGN の設計原則と照合して判定したもの。

**設計原則** (SPEC §0):
1. Canonicity — 1つの意味に1つの書き方
2. Surface Semantics — 副作用/エラー/Optional が型に現れる
3. Vocabulary Economy — 語彙は最小限
4. No Magic — 暗黙の変換なし

---

## P0: `do` ブロックの純粋関数対応 + `guard else break/continue`

### 問題

SPEC は「`for...in` for collections, `do { guard }` for dynamic loops」と明言している。
しかし現在の `do` ブロックは Result/Option コンテキストでしか使えないため、
純粋関数で動的ループを書く手段がない。

結果、`for _ in 0..len { if done then () else { ... } }` という地獄パターンが頻出。

### 解決策

`while` キーワードは追加しない（3種のループで Canonicity 違反）。
代わりに `do` ブロックを純粋関数でも使えるよう拡張し、
`guard else break` / `guard else continue` を導入。

```almd
// 純粋 do ブロック — while 相当
var i = 0
do {
  guard i < len else break
  let ch = string.char_at(code, i).unwrap_or("")
  if ch == "\"" then break
  result = result ++ ch
  i = i + 1
}

// for 内でも guard else break/continue が使える
for ch in chars {
  guard ch != " " else continue
  result = result ++ ch
}
```

### 設計原則との整合

| 原則 | 判定 |
|------|------|
| Canonicity | ○ `while` を足さず `do { guard }` に統一。early exit は常に `guard else` 経由 |
| Vocabulary Economy | ○ 新キーワードは `break`/`continue` の2つ。ただし単独使用不可、常に `guard else` と組む |
| Surface Semantics | ○ ループの脱出条件が `guard` で明示される |
| LLM 親和性 | ○ LLM は `break`/`continue` を知っている。`guard else break` は自然に理解できる |

### 影響範囲

highlight.almd, runtime.almd のコード量 **30〜40% 削減**見込み。

---

## P1-a: `unwrap_or` バグ修正（型チェッカー）

### 問題

`unwrap_or(opt, default)` は codegen (calls.rs) にビルトインとして存在するが、
型チェッカー (check/) が認識しない。`undefined function 'unwrap_or'` エラーになる。

### 解決策

check/calls.rs にビルトイン関数の型シグネチャを追加。
`unwrap_or: (Option[T], T) -> T` を登録する。

### 設計原則との整合

バグ修正。SPEC §18 に `unwrap_or` は明記されている。

### 影響範囲

修正後、以下のパターンが全て動く:
```almd
string.index_of(data, needle).unwrap_or(0 - 1)
json.get_string(obj, key).unwrap_or("")
json.get_array(obj, key).unwrap_or([])
string.char_at(code, i).unwrap_or("")
```

これにより `??` 演算子は不要（Canonicity 維持: unwrap には1つの方法のみ）。

---

## P1-b: `json.parse` 自動 `?` 挿入バグ修正

### 問題

`json.parse(data)` は `Result[Json, String]` を返す純粋関数だが、
codegen が自動で `?` を挿入するため、呼び出し元が `Result` を返す関数でないとコンパイルエラー。

純粋関数内で `match json.parse(data) { ok(obj) => ..., err(_) => ... }` と
ローカルに処理することができない。

### 解決策

stdlib の `json.parse` を `result_fns`/`effect_fns` セットから外す。
自動 `?` は `effect fn` 内 or `do` ブロック内でのみ挿入。
通常の関数では `Result` をそのまま返し、ユーザーが明示的に match する。

```almd
// 純粋関数で動くべき（修正後）
fn safe_extract(data: String) -> String =
  match json.parse(data) {
    ok(obj) => json.get_string(obj, "text").unwrap_or("")
    err(_) => ""
  }

// effect fn + do 内では自動 ? でOK（既存動作）
effect fn load(data: String) -> Result[Json, String] = do {
  let obj = json.parse(data)  // auto-? here
  ok(obj)
}
```

### 設計原則との整合

| 原則 | 判定 |
|------|------|
| Surface Semantics | ○ `Result` が型に現れ、ユーザーが明示的に処理する |
| No Magic | ○ 暗黙の `?` 挿入を止めて、明示的な処理を促す |
| Canonicity | ○ `do` 内では auto-?、それ以外では match — 文脈で一意に決まる |

### 影響範囲

sse.almd で `import json` を使った純粋なSSEパーサが書ける。
手動JSON文字列パーサを書く必要がなくなる。

---

## Not Doing: `??` 演算子

**理由**: `unwrap_or` が修正されれば UFCS で `opt.unwrap_or(default)` と書ける。
`??` を追加すると同じ意味に3つの書き方ができ、Canonicity に違反する:
- `unwrap_or(opt, d)` — 関数呼び出し
- `opt.unwrap_or(d)` — UFCS
- `opt ?? d` — 演算子

Vocabulary Economy の観点からも、既存の語彙で表現可能なら新しい演算子は不要。

---

## Not Doing: `while` キーワード

**理由**: SPEC が `for...in` + `do { guard }` の2形態を規定。
`while` を追加すると3種のループ構文になり、Canonicity に違反する。
`do` ブロックの拡張で同じ表現力を得られる。

---

## Not Doing: `s[i]` インデックス構文

**理由**: SPEC §19 で演算子オーバーロードを明確に禁止。
`string.char_at(s, i).unwrap_or("")` で十分（`unwrap_or` 修正後）。

---

## Not Doing: match arm の `{}` 省略

**理由**: Rust と同じルール。LLM の既存知識と一致。
Canonicity にも合致（ブロックは常に `{}` で囲む）。

---

## Summary

| 項目 | 判定 | 理由 |
|------|------|------|
| `do` 純粋化 + `guard else break/continue` | **実装する** | SPEC の `do { guard }` 設計の完成 |
| `unwrap_or` 型チェック修正 | **実装する** | バグ修正 |
| `json.parse` 自動 `?` 修正 | **実装する** | バグ修正 |
| `??` 演算子 | **しない** | Canonicity 違反 |
| `while` キーワード | **しない** | Canonicity 違反 |
| `s[i]` インデックス | **しない** | 演算子オーバーロード禁止 |
| match arm `{}` 省略 | **しない** | 現状がRust同様で一貫 |

実装するものは3つ。全て設計原則に整合し、既存のSPEC設計の完成または修正。
