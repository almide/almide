<!-- description: Auto-inferred effect capabilities with package-level permissions -->
# Effect System — Auto-Inferred Capabilities

**優先度:** 1.x (情報表示) → 2.x (制限適用)
**前提:** Effect Isolation Layer 1 完了済み
**原則:** ユーザーは `fn` / `effect fn` だけ書く。コンパイラが capability を自動推論。
**構文制約:** Effect の粒度はユーザー構文に一切にじませない。新キーワード追加なし。`effect fn` が唯一のマーカー。

> 「書くコードは変わらない。コンパイラが賢くなるだけ。」

---

## 完了 (Phase 1-2) → [done/effect-system-phase1-2.md](../done/effect-system-phase1-2.md)

- [x] **Phase 1: Effect 推論エンジン** — EffectInferencePass, 7 カテゴリ (IO/Net/Env/Time/Rand/Fan/Log), 推移的推論, `almide check --effects`
- [x] **Phase 2: Self-package 制限** — `almide.toml [permissions]`, 通常の `almide check` に統合, Security Layer 2
- [x] **Phase 2b: Permissions 貫通** — `almide run`/`almide build` でも permissions チェック実行 (`check_permissions()` 共通関数化)

---

## 残り

### Phase 3: Dependency 制限 (2.x)

```toml
[dependencies.api-client]
git = "https://github.com/example/api-client"
allow = ["Net"]  # IO は禁止
```

依存パッケージの capability を消費者が制限。Security Layer 3。

### Phase 4: 内部型レベル統合 (2.x, 構文変更なし)

コンパイラ内部で Effect set を型情報に持たせる (FnType に EffectSet を付与)。
ユーザー構文は一切変わらない — `effect fn` のまま。
vibe-lang の `with {Async}` のような明示的 effect 構文は **導入しない**。

---

## Effect Categories

| Effect | Modules | 実装 |
|--------|---------|------|
| `IO` | fs, path | ✅ |
| `Net` | http, url | ✅ |
| `Env` | env, process | ✅ |
| `Time` | time, datetime | ✅ |
| `Rand` | math.random | ✅ |
| `Fan` | fan | ✅ |
| `Log` | log | ✅ |

## vibe-lang との差別化

| | vibe-lang | Almide |
|---|-----------|--------|
| ユーザー構文 | `with {Async, Error}` 明示 | `effect fn` のみ — 推論 |
| LLM 負荷 | effect 選択が必要 | 変更なし |
| 制限単位 | 関数レベル | パッケージ境界 |
| 新キーワード | `with`, `handle` | なし |
| 破壊的変更 | — | なし (additive) |
