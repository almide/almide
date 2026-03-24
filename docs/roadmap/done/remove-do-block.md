# Remove `do` Block [ACTIVE]

**優先度:** High — 1.0 前のラストチャンス。構文凍結前に決着させる
**原則:** Canonicity（1つの意味に1つの書き方）、Vocabulary Economy（語彙は最小限）

> 「`do` は手続き的な概念。LLM が最も正確に書ける言語に、手続き的ループの残骸は要らない。」

---

## Why

`do` ブロックは2つの役割を兼務している:

1. **guard 付きループ** — `do { guard cond else break; ... }`
2. **エラー自動伝播ブロック** — `do { let x = may_fail(); ... }`

問題:
- **`while` の追加でループ用途の 90% が不要になった。** 残るのは「値を返すループ」だけ
- **`effect fn` の自動伝播でエラー用途の大半が不要になった。** `effect fn` 内では `do` なしで `?` 相当が効く
- **Canonicity 違反**: 同じことが `while` と `do { guard }` の2通りで書ける
- **LLM の混乱源**: `do` の意味が文脈依存（guard があればループ、なければ伝播ブロック）。LLM は `while` を第一候補で書く

---

## 現状の使用箇所 (47箇所)

| パターン | 箇所数 | 代替手段 |
|---|---|---|
| `effect fn ... = do { }` | ~15 | `effect fn` の本体は暗黙に伝播スコープ → `do` 不要 |
| `do { guard ... else break }` (ループ) | ~15 | `while` で書き換え |
| `do { guard ... else ok(()) }` (early return) | ~5 | `if`/`when` + early return |
| 純粋関数内 `do { }` (エラー伝播) | ~12 | `try { }` ブロックに置き換え (新構文) |

---

## Design: `do` 廃止後の世界

### ループ → `while` に統一

```almd
// Before
do {
  guard current != "NONE" else break
  current = next
}

// After
while current != "NONE" {
  current = next
}
```

### effect fn の本体 → `do` 不要に

```almd
// Before
effect fn parse(path: String) -> Result[Config, String] = do {
  let text = fs.read_text(path)
  let raw = json.parse(text)
  decode(raw)
}

// After — effect fn の本体は暗黙に伝播スコープ
effect fn parse(path: String) -> Result[Config, String] = {
  let text = fs.read_text(path)
  let raw = json.parse(text)
  decode(raw)
}
```

`effect fn` は既に自動 `?` 伝播を行う。`do` は冗長。

### 純粋関数内のエラー伝播 → `try { }` に置き換え

```almd
// Before
fn process(input: String) -> Result[Data, String] = {
  let result = do {
    let parsed = json.parse(input)
    validate(parsed)
  }
  result
}

// After
fn process(input: String) -> Result[Data, String] = {
  let result = try {
    let parsed = json.parse(input)
    validate(parsed)
  }
  result
}
```

`try` は「このブロック内で失敗する呼び出しを自動伝播する」を明示する。
`do` より意図が明確。LLM も Rust の `try` / Swift の `do-catch` から類推できる。

---

## Phases

### Phase 1: `effect fn` の `do` 不要化

- [ ] `effect fn ... = do { }` を `effect fn ... = { }` に変更
- [ ] パーサー: `effect fn` の本体が `do` なしで伝播スコープになるよう修正
- [ ] 既存コード 15箇所を書き換え
- [ ] `do` を `effect fn` 内で使うと deprecation warning

### Phase 2: `do` ループ → `while` 移行

- [ ] spec/exercises の `do { guard ... }` を `while` に書き換え (~15箇所)
- [ ] CHEATSHEET から `do` ループパターンを削除
- [ ] `do { guard ... }` を使うと deprecation warning + migration hint

### Phase 3: `try { }` ブロック導入

- [ ] `try` キーワード追加 (純粋関数内のエラー伝播スコープ)
- [ ] パーサー + チェッカー + codegen 対応 (Rust: `(|| -> Result { })()`, TS: `try/catch`)
- [ ] 純粋関数内の `do { }` を `try { }` に書き換え (~12箇所)

### Phase 4: `do` キーワード廃止

- [ ] `do` をパーサーで reject (compile error + hint: "use `while` or `try`")
- [ ] `do` を予約語から削除
- [ ] DoBlock IR ノードを削除 (IrExprKind::DoBlock → 統合)
- [ ] BREAKING_CHANGE_POLICY に記録

---

## Migration Path

1.0 前なので deprecation cycle は短縮可能:
- Phase 1-2: 即実行 (既存テスト書き換え)
- Phase 3: `try` 導入
- Phase 4: `do` 削除

## Risk

- **破壊的変更だが 1.0 前**: 既存ユーザーは少ない。今がラストチャンス
- **`try` の導入**: 新キーワードだが意味が明確。Rust/Swift/Kotlin と同じ概念
- **IR 変更**: `IrExprKind::DoBlock` を消すか `TryBlock` にリネームするか。codegen への影響は限定的

## Non-Goals

- `do` を残して `try` と共存させること（Canonicity 違反を温存するだけ）
- `while` に値を返す機能を追加すること（`while` は Unit を返す設計で正しい）
