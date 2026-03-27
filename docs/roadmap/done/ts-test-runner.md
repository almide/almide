<!-- description: almide test --target ts command with Deno/Node support -->
# TypeScript Test Runner [DONE]

**完了日:** 2026-03-25

## 実装内容

`almide test --target ts` コマンドを追加。TypeScript ターゲットのテスト実行が可能に。

### 変更点
- `src/cli/commands.rs` に `cmd_test_ts()` を追加（229 行）
- `src/main.rs` に `--target ts`/`--target typescript` ディスパッチ追加
- Deno 優先、Node.js フォールバック
- `// ts:skip` マーカーでファイル単位スキップ対応
- codegen パニックは SKIP として報告
- 型エラーはソースコンテキスト付きで表示
