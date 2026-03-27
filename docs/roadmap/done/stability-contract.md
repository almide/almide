<!-- description: Backward compatibility policy, edition system, and API freeze -->
# Stability Contract [DONE — 1.0 Phase II]

> Go 1 compatibility promise: "every Go program that compiles today compiles forever."
> Rust editions: syntax evolution without breaking existing code.
> Python 2→3: silent semantic changes nearly killed the language.

## 実装済み

- [x] `almide.toml` に `edition = "2026"` フィールド追加
- [x] `almide init` が edition を生成
- [x] 破壊的変更ポリシー文書: `docs/BREAKING_CHANGE_POLICY.md`
- [x] コア型 API 凍結監査: `docs/FROZEN_API.md` (string 41, int 19, float 16, list 54, map 16, result 9)
- [x] Rejected Patterns リスト: `docs/REJECTED_PATTERNS.md` (20+ 項目)
- [x] Hidden operations 文書化: `docs/HIDDEN_OPERATIONS.md` (clone, auto-?, Result erasure, runtime, fan)
