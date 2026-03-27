<!-- description: Allow main.almd to access pub functions from same-package mod.almd -->
# `import self` — Package Entry Point Access [DONE]

## Problem

`main.almd` から同パッケージの `mod.almd`（ライブラリエントリーポイント）の pub 関数にアクセスできない。

```
src/
  mod.almd    ← 外部: import almide_grammar で解決
  main.almd   ← CLI: mod.almd の pub fn を使いたい
```

- `import self.mod` → `mod` はキーワードなのでパースエラー
- データを別ファイル（`grammar.almd`）に分離すると、外部APIが `almide_grammar.grammar.keyword_groups()` と深くなる
- re-export が無いので `mod.almd` 経由のフラット化もできない

## Design

**`import self` で `mod.almd` を参照できるようにする。**

```almide
// main.almd
import self               // → src/mod.almd をロード
import self as grammar     // → エイリアスも可

grammar.keyword_groups()   // mod.almd の pub fn にアクセス
```

### Semantics

- `import self` = パッケージの `src/mod.almd` をインポート
- 既存の `import self.xxx` (サブモジュール) はそのまま
- `mod.almd` が存在しない場合はエラー: `"package has no mod.almd entry point"`
- エイリアス無しの場合、パッケージ名がプレフィックス（`almide_grammar.keyword_groups()`）

### Why not alternatives

| 案 | 問題 |
|----|------|
| `main.almd` が `mod.almd` を暗黙参照 | 暗黙は Almide の設計哲学に反する |
| re-export (`pub import`) | 新しい概念の導入コストが高い |
| `mod` をキーワードから外す | 既存コードへの影響大 |

## Implementation

`src/resolve.rs` の `resolve_imports_with_deps` 内:

```rust
if is_self_import {
    if path.len() < 2 {
        // NEW: import self → load mod.almd
        let mod_name = alias.as_deref()
            .unwrap_or_else(|| pkg_name.as_deref().unwrap_or("self"));
        // ... load src/mod.almd
    }
    // existing: import self.xxx
}
```

変更箇所は `resolve.rs` の1箇所のみ。パーサー変更不要（`import self` は既に有効なトークン列）。

## Motivation

`almide-grammar` パッケージで `mod.almd`（データ定義）と `main.almd`（CLIジェネレータ）を分離する際にブロッカーとなった。ライブラリ+CLIの構成は今後増えるため、早期に解決すべき。
